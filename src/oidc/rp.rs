//! OIDC Relying Party (BFF) - Authorization Code + PKCE flow for browser clients.
//!
//! Routes:
//! - `GET  /oauth2/login`               - start the OIDC auth flow
//! - `GET  /oauth2/callback`            - exchange code for tokens, create session
//! - `GET  /oauth2/logout`              - RP-initiated logout
//! - `POST /oauth2/backchannel-logout`  - receive OIDC Back-Channel Logout tokens
//! - `POST /oauth2/lifecycle-event`     - receive SaaS user lifecycle events
//! - `GET  /dev/seed-session`           - (debug builds only) inject a dev session
//! - `GET  /dev/logout`                 - (debug builds only) clear session cookie

use actix_web::{
    cookie::{time::Duration as CookieDuration, Cookie, SameSite},
    http::header,
    web, HttpRequest, HttpResponse,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::Utc;
use rand::RngCore;
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use crate::config::OidcConfig;
use crate::db::AppState;

use super::jit::{self, JitError};
use super::session::{hash_session_token, RUS_SESSION_COOKIE};
use super::verifier::OidcVerifier;

#[derive(Clone)]
pub struct OidcRpState {
    pub config: OidcConfig,
    pub verifier: Arc<OidcVerifier>,
    pub jti_cache: Arc<moka::future::Cache<String, ()>>,
}

impl OidcRpState {
    pub fn new(config: OidcConfig, verifier: Arc<OidcVerifier>) -> Self {
        let jti_cache = Arc::new(
            moka::future::Cache::builder()
                .time_to_live(Duration::from_secs(config.lifecycle_jti_cache_ttl))
                .build(),
        );
        Self {
            config,
            verifier,
            jti_cache,
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn random_b64url(n: usize) -> String {
    let mut buf = vec![0u8; n];
    rand::thread_rng().fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(&buf)
}

fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn build_session_cookie(token: &str, ttl_seconds: u64, secure: bool) -> Cookie<'static> {
    Cookie::build(RUS_SESSION_COOKIE, token.to_string())
        .http_only(true)
        .secure(secure)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(CookieDuration::seconds(ttl_seconds as i64))
        .finish()
}

fn clear_session_cookie(secure: bool) -> Cookie<'static> {
    Cookie::build(RUS_SESSION_COOKIE, "")
        .http_only(true)
        .secure(secure)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(CookieDuration::ZERO)
        .finish()
}

fn redirect(location: &str) -> HttpResponse {
    HttpResponse::SeeOther()
        .append_header((header::LOCATION, location))
        .finish()
}

fn enabled_or_404(state: &OidcRpState) -> Option<HttpResponse> {
    if state.config.enabled() {
        None
    } else {
        Some(HttpResponse::NotFound().finish())
    }
}

fn rfc3339(t: chrono::DateTime<Utc>) -> String {
    t.to_rfc3339()
}

// ── Query / form parameter types ─────────────────────────────────────────────

#[derive(Deserialize)]
pub struct LoginQuery {
    pub return_to: Option<String>,
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

#[derive(Deserialize)]
pub struct BackchannelLogoutForm {
    pub logout_token: String,
}

#[derive(Deserialize)]
pub struct LifecycleEventForm {
    pub lifecycle_event: String,
}

#[derive(serde::Deserialize)]
struct TokenResponse {
    #[allow(dead_code)]
    access_token: String,
    id_token: String,
    #[allow(dead_code)]
    refresh_token: Option<String>,
    #[allow(dead_code)]
    expires_in: Option<u64>,
}

#[derive(serde::Deserialize)]
struct TokenErrorResponse {
    error: String,
    error_description: Option<String>,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

pub async fn login(
    state: web::Data<OidcRpState>,
    app_state: web::Data<AppState>,
    params: web::Query<LoginQuery>,
) -> HttpResponse {
    if let Some(r) = enabled_or_404(&state) {
        return r;
    }

    let pkce_state = random_b64url(32);
    let nonce = random_b64url(32);
    let code_verifier = random_b64url(43);
    let code_challenge = pkce_challenge(&code_verifier);

    let session_id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let expires_at = now + chrono::Duration::minutes(10);

    {
        let db = app_state.db.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = db.execute(
            "INSERT INTO rp_sessions (id, state, nonce, code_verifier, return_to, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                session_id,
                pkce_state,
                nonce,
                code_verifier,
                params.return_to.as_deref(),
                rfc3339(now),
                rfc3339(expires_at),
            ],
        ) {
            tracing::error!(error = %e, "failed to persist rp_session");
            return HttpResponse::InternalServerError().finish();
        }
    }

    let issuer = state.config.issuer.trim_end_matches('/');
    let scopes = "openid email offline_access";
    let auth_url = format!(
        "{issuer}/oauth2/authorize?response_type=code&client_id={cid}&redirect_uri={ruri}\
         &scope={scope}&state={st}&nonce={nc}&code_challenge={ch}&code_challenge_method=S256",
        cid = urlencoding::encode(&state.config.client_id),
        ruri = urlencoding::encode(&state.config.redirect_uri),
        scope = urlencoding::encode(scopes),
        st = urlencoding::encode(&pkce_state),
        nc = urlencoding::encode(&nonce),
        ch = urlencoding::encode(&code_challenge),
    );

    redirect(&auth_url)
}

pub async fn callback(
    state: web::Data<OidcRpState>,
    app_state: web::Data<AppState>,
    params: web::Query<CallbackQuery>,
) -> HttpResponse {
    if let Some(r) = enabled_or_404(&state) {
        return r;
    }

    if let Some(err) = &params.error {
        let desc = params.error_description.as_deref().unwrap_or(err.as_str());
        tracing::warn!(error = %err, description = %desc, "IdP returned error at callback");
        let location = format!(
            "/?error={}&error_description={}",
            urlencoding::encode(err),
            urlencoding::encode(desc),
        );
        return redirect(&location);
    }

    let Some(code) = params.code.as_deref() else {
        return HttpResponse::BadRequest().body("Missing 'code' parameter");
    };
    let Some(state_param) = params.state.as_deref() else {
        return HttpResponse::BadRequest().body("Missing 'state' parameter");
    };

    // Look up and consume the PKCE session.
    let rp_session = {
        let db = app_state.db.lock().unwrap_or_else(|e| e.into_inner());
        match db
            .query_row(
                "SELECT id, nonce, code_verifier, return_to, expires_at
                 FROM rp_sessions WHERE state = ?1",
                params![state_param],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()
        {
            Ok(Some(row)) => row,
            Ok(None) => return HttpResponse::BadRequest().body("Unknown or expired state"),
            Err(e) => {
                tracing::error!(error = %e, "rp_sessions lookup failed");
                return HttpResponse::InternalServerError().finish();
            }
        }
    };
    let (rp_id, nonce, code_verifier, return_to, expires_at) = rp_session;

    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(&expires_at) {
        if parsed.with_timezone(&Utc) < Utc::now() {
            let db = app_state.db.lock().unwrap_or_else(|e| e.into_inner());
            let _ = db.execute("DELETE FROM rp_sessions WHERE id = ?1", params![rp_id]);
            return HttpResponse::BadRequest().body("Login session expired; please try again");
        }
    }

    // Token exchange.
    let token_url = format!("{}/oauth2/token", state.config.issuer.trim_end_matches('/'));
    let resp = state
        .verifier
        .http
        .post(&token_url)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", state.config.redirect_uri.as_str()),
            ("client_id", state.config.client_id.as_str()),
            ("client_secret", state.config.client_secret.as_str()),
            ("code_verifier", code_verifier.as_str()),
        ])
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "token endpoint request failed");
            return HttpResponse::BadGateway().body("Token endpoint request failed");
        }
    };

    if !resp.status().is_success() {
        let err: TokenErrorResponse = resp.json().await.unwrap_or(TokenErrorResponse {
            error: "server_error".into(),
            error_description: None,
        });
        tracing::warn!(error = %err.error, "Token endpoint returned error");
        return HttpResponse::BadGateway().body(format!(
            "Token exchange failed: {}",
            err.error_description.unwrap_or(err.error)
        ));
    }

    let tokens: TokenResponse = match resp.json().await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(error = %e, "failed to parse token response");
            return HttpResponse::BadGateway().finish();
        }
    };

    // Validate ID token.
    let id_claims = match state
        .verifier
        .verify_id_token(&tokens.id_token, &nonce)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "ID token validation failed");
            return HttpResponse::Unauthorized().body("ID token validation failed");
        }
    };

    // JIT provision (or load) the local user, then issue session.
    let provisioned = {
        let db = app_state.db.lock().unwrap_or_else(|e| e.into_inner());
        // Consume the PKCE row whether or not provisioning succeeds.
        let _ = db.execute("DELETE FROM rp_sessions WHERE id = ?1", params![rp_id]);
        match jit::load_or_provision(&db, &id_claims) {
            Ok(p) => p,
            Err(JitError::Forbidden(msg)) => {
                let location = format!(
                    "/?error=access_denied&error_description={}",
                    urlencoding::encode(&msg)
                );
                return redirect(&location);
            }
            Err(JitError::Internal(msg)) => {
                tracing::error!(error = %msg, "JIT provisioning failed");
                return HttpResponse::InternalServerError().body("Provisioning failed");
            }
        }
    };

    // Issue session.
    let session_token = random_b64url(32);
    let token_hash = hash_session_token(&session_token);
    let now = Utc::now();
    let expires_at = now + chrono::Duration::seconds(state.config.session_ttl_seconds as i64);

    {
        let db = app_state.db.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = db.execute(
            "INSERT INTO user_sessions (id, session_token_hash, user_id, session_version, auth_via_oidc, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6)",
            params![
                Uuid::new_v4().to_string(),
                token_hash,
                provisioned.user_id,
                provisioned.session_version,
                rfc3339(now),
                rfc3339(expires_at),
            ],
        ) {
            tracing::error!(error = %e, "failed to insert user_session");
            return HttpResponse::InternalServerError().finish();
        }
    }

    let secure = state.config.redirect_uri.starts_with("https://");
    let cookie = build_session_cookie(&session_token, state.config.session_ttl_seconds, secure);

    // Same-origin only. `s.starts_with('/')` alone would accept protocol-relative
    // paths like `//evil.com/x`, which browsers resolve as `https://evil.com/x`.
    let destination = return_to
        .as_deref()
        .filter(|s| s.starts_with('/') && !s.starts_with("//"))
        .unwrap_or("/dashboard.html");

    HttpResponse::SeeOther()
        .cookie(cookie)
        .append_header((header::LOCATION, destination))
        .finish()
}

pub async fn logout(
    state: web::Data<OidcRpState>,
    app_state: web::Data<AppState>,
    req: HttpRequest,
) -> HttpResponse {
    if let Some(r) = enabled_or_404(&state) {
        return r;
    }

    if let Some(cookie) = req.cookie(RUS_SESSION_COOKIE) {
        let token_hash = hash_session_token(cookie.value());
        let db = app_state.db.lock().unwrap_or_else(|e| e.into_inner());
        let _ = db.execute(
            "DELETE FROM user_sessions WHERE session_token_hash = ?1",
            params![token_hash],
        );
    }

    let secure = state.config.redirect_uri.starts_with("https://");
    let cleared = clear_session_cookie(secure);

    let logout_url = if state.config.post_logout_redirect_uri.is_empty() {
        format!(
            "{}/oauth2/logout",
            state.config.issuer.trim_end_matches('/')
        )
    } else {
        format!(
            "{}/oauth2/logout?post_logout_redirect_uri={}",
            state.config.issuer.trim_end_matches('/'),
            urlencoding::encode(&state.config.post_logout_redirect_uri),
        )
    };

    HttpResponse::SeeOther()
        .cookie(cleared)
        .append_header((header::LOCATION, logout_url))
        .finish()
}

pub async fn backchannel_logout(
    state: web::Data<OidcRpState>,
    app_state: web::Data<AppState>,
    form: web::Form<BackchannelLogoutForm>,
) -> HttpResponse {
    if let Some(r) = enabled_or_404(&state) {
        return r;
    }

    let claims = match state.verifier.verify_logout_token(&form.logout_token).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "back-channel logout token rejected");
            return HttpResponse::BadRequest().finish();
        }
    };

    if let Some(sub) = &claims.sub {
        if Uuid::parse_str(sub).is_ok() {
            let db = app_state.db.lock().unwrap_or_else(|e| e.into_inner());
            match db.execute(
                "UPDATE users SET session_version = session_version + 1 WHERE saas_user_id = ?1",
                params![sub],
            ) {
                Ok(n) if n > 0 => {
                    tracing::info!(saas_user_id = %sub, "back-channel logout: session_version incremented");
                }
                Err(e) => tracing::warn!(error = %e, "back-channel logout DB update failed"),
                _ => {}
            }
        }
    }

    HttpResponse::Ok().finish()
}

pub async fn lifecycle_event(
    state: web::Data<OidcRpState>,
    app_state: web::Data<AppState>,
    form: web::Form<LifecycleEventForm>,
) -> HttpResponse {
    if let Some(r) = enabled_or_404(&state) {
        return r;
    }

    let claims = match state
        .verifier
        .verify_lifecycle_token(&form.lifecycle_event)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "lifecycle event token rejected");
            return HttpResponse::BadRequest().finish();
        }
    };

    if state.jti_cache.get(&claims.jti).await.is_some() {
        tracing::debug!(jti = %claims.jti, "lifecycle event already processed");
        return HttpResponse::Ok().finish();
    }

    let event = match claims.lifecycle_event() {
        Some(e) => e.clone(),
        None => {
            tracing::debug!(jti = %claims.jti, "lifecycle event with unknown schema; ignoring");
            return HttpResponse::Ok().finish();
        }
    };

    let subject_id = match Uuid::parse_str(&event.subject.id) {
        Ok(u) => u.to_string(),
        Err(_) => {
            tracing::warn!(subject = %event.subject.id, "lifecycle event subject is not a UUID");
            return HttpResponse::Ok().finish();
        }
    };

    let now = rfc3339(Utc::now());

    {
        let db = app_state.db.lock().unwrap_or_else(|e| e.into_inner());
        let result = match event.event_type.as_str() {
            "user.suspended" => db.execute(
                "UPDATE users SET suspended_at = ?1, session_version = session_version + 1
                 WHERE saas_user_id = ?2",
                params![now, subject_id],
            ),
            "user.unsuspended" => db.execute(
                "UPDATE users SET suspended_at = NULL WHERE saas_user_id = ?1",
                params![subject_id],
            ),
            "user.deleted" => db.execute(
                "DELETE FROM users WHERE saas_user_id = ?1",
                params![subject_id],
            ),
            "entitlement.revoked" => db.execute(
                "UPDATE users SET session_version = session_version + 1 WHERE saas_user_id = ?1",
                params![subject_id],
            ),
            "entitlement.granted" => Ok(0),
            unknown => {
                tracing::debug!(event_type = %unknown, jti = %claims.jti, "unknown lifecycle event type");
                Ok(0)
            }
        };
        if let Err(e) = result {
            tracing::error!(error = %e, "lifecycle event DB update failed");
            return HttpResponse::InternalServerError().finish();
        }
    }

    state.jti_cache.insert(claims.jti.clone(), ()).await;

    tracing::info!(
        jti = %claims.jti,
        event_type = %event.event_type,
        subject = %subject_id,
        "lifecycle event processed"
    );

    HttpResponse::Ok().finish()
}

// ── Dev-only seed-session (debug builds only) ─────────────────────────────────

#[cfg(debug_assertions)]
pub async fn dev_logout(
    state: web::Data<OidcRpState>,
    app_state: web::Data<AppState>,
    req: HttpRequest,
) -> HttpResponse {
    if let Some(cookie) = req.cookie(RUS_SESSION_COOKIE) {
        let token_hash = hash_session_token(cookie.value());
        let db = app_state.db.lock().unwrap_or_else(|e| e.into_inner());
        let _ = db.execute(
            "DELETE FROM user_sessions WHERE session_token_hash = ?1",
            params![token_hash],
        );
    }
    let secure = state.config.redirect_uri.starts_with("https://");
    HttpResponse::SeeOther()
        .cookie(clear_session_cookie(secure))
        .append_header((header::LOCATION, "/"))
        .finish()
}

#[cfg(debug_assertions)]
pub async fn dev_seed_session(
    state: web::Data<OidcRpState>,
    app_state: web::Data<AppState>,
) -> HttpResponse {
    const DEV_USERNAME: &str = "dev";
    const DEV_EMAIL: &str = "dev@dev.local";
    const DEV_SAAS_UUID: &str = "00000000-0000-0000-0000-000000000001";

    let now = Utc::now();
    let expires_at = now + chrono::Duration::days(30);
    let session_token = random_b64url(32);
    let token_hash = hash_session_token(&session_token);

    let user_id_result: rusqlite::Result<i64> = (|| {
        let db = app_state.db.lock().unwrap_or_else(|e| e.into_inner());
        // Upsert dev user.
        db.execute(
            "INSERT INTO users (username, password, is_admin, saas_user_id, email)
             VALUES (?1, '!sso:no-password', 1, ?2, ?3)
             ON CONFLICT(username) DO UPDATE SET saas_user_id = excluded.saas_user_id, email = excluded.email, is_admin = 1",
            params![DEV_USERNAME, DEV_SAAS_UUID, DEV_EMAIL],
        )?;
        let user_id: i64 = db.query_row(
            "SELECT userID FROM users WHERE username = ?1",
            params![DEV_USERNAME],
            |r| r.get(0),
        )?;
        let session_version: i32 = db.query_row(
            "SELECT session_version FROM users WHERE userID = ?1",
            params![user_id],
            |r| r.get(0),
        )?;
        db.execute(
            "INSERT INTO user_sessions (id, session_token_hash, user_id, session_version, auth_via_oidc, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6)",
            params![
                Uuid::new_v4().to_string(),
                token_hash,
                user_id,
                session_version,
                rfc3339(now),
                rfc3339(expires_at),
            ],
        )?;
        Ok(user_id)
    })();

    if let Err(e) = user_id_result {
        tracing::error!(error = %e, "dev_seed_session failed");
        return HttpResponse::InternalServerError().body(format!("dev seed failed: {e}"));
    }

    let secure = state.config.redirect_uri.starts_with("https://");
    HttpResponse::SeeOther()
        .cookie(build_session_cookie(
            &session_token,
            state.config.session_ttl_seconds,
            secure,
        ))
        .append_header((header::LOCATION, "/dashboard.html"))
        .finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OidcConfig;
    use crate::testing::{insert_saas_user, make_saas_session, make_test_state};
    use actix_web::{test, App};

    const SUB_A: &str = "11111111-1111-1111-1111-111111111111";

    fn rp_state(enabled: bool) -> web::Data<OidcRpState> {
        let mut cfg = OidcConfig {
            issuer: if enabled {
                "https://idp.example.com".into()
            } else {
                String::new()
            },
            audience: "https://rus.example.com/api".into(),
            jwks_url: "https://idp.example.com/.well-known/jwks.json".into(),
            jwks_cache_ttl: 300,
            client_id: "test-client".into(),
            client_secret: "secret".into(),
            redirect_uri: "https://rus.example.com/oauth2/callback".into(),
            post_logout_redirect_uri: "https://rus.example.com/".into(),
            leeway_seconds: 30,
            lifecycle_jti_cache_ttl: 300,
            session_ttl_seconds: 1_209_600,
        };
        if !enabled {
            cfg.issuer = String::new();
        }
        let verifier = std::sync::Arc::new(OidcVerifier::new(cfg.clone()));
        web::Data::new(OidcRpState::new(cfg, verifier))
    }

    #[actix_web::test]
    async fn login_returns_404_when_oidc_disabled() {
        let app = test::init_service(
            App::new()
                .app_data(make_test_state())
                .app_data(rp_state(false))
                .route("/oauth2/login", web::get().to(login)),
        )
        .await;
        let req = test::TestRequest::get().uri("/oauth2/login").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);
    }

    #[actix_web::test]
    async fn login_redirects_to_authorize_with_pkce() {
        let app_state = make_test_state();
        let app = test::init_service(
            App::new()
                .app_data(app_state.clone())
                .app_data(rp_state(true))
                .route("/oauth2/login", web::get().to(login)),
        )
        .await;
        let req = test::TestRequest::get().uri("/oauth2/login").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 303);
        let loc = resp.headers().get("Location").unwrap().to_str().unwrap();
        assert!(loc.starts_with("https://idp.example.com/oauth2/authorize?"));
        assert!(loc.contains("client_id=test-client"));
        assert!(loc.contains("code_challenge_method=S256"));
        assert!(loc.contains("response_type=code"));

        // rp_session row should have been written
        let count: i64 = app_state
            .db
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM rp_sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[actix_web::test]
    async fn callback_propagates_idp_error_via_redirect() {
        let app = test::init_service(
            App::new()
                .app_data(make_test_state())
                .app_data(rp_state(true))
                .route("/oauth2/callback", web::get().to(callback)),
        )
        .await;
        let req = test::TestRequest::get()
            .uri("/oauth2/callback?error=access_denied&error_description=nope")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 303);
        let loc = resp.headers().get("Location").unwrap().to_str().unwrap();
        assert!(loc.starts_with("/?error=access_denied"));
    }

    #[actix_web::test]
    async fn callback_400_when_missing_code() {
        let app = test::init_service(
            App::new()
                .app_data(make_test_state())
                .app_data(rp_state(true))
                .route("/oauth2/callback", web::get().to(callback)),
        )
        .await;
        let req = test::TestRequest::get()
            .uri("/oauth2/callback?state=abc")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn callback_400_when_missing_state() {
        let app = test::init_service(
            App::new()
                .app_data(make_test_state())
                .app_data(rp_state(true))
                .route("/oauth2/callback", web::get().to(callback)),
        )
        .await;
        let req = test::TestRequest::get()
            .uri("/oauth2/callback?code=xyz")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn logout_clears_cookie_and_redirects() {
        let app_state = make_test_state();
        let uid = insert_saas_user(&app_state, "alice", SUB_A, false);
        let token = make_saas_session(&app_state, uid);
        let app = test::init_service(
            App::new()
                .app_data(app_state.clone())
                .app_data(rp_state(true))
                .route("/oauth2/logout", web::get().to(logout)),
        )
        .await;
        let req = test::TestRequest::get()
            .uri("/oauth2/logout")
            .insert_header(("Cookie", format!("{RUS_SESSION_COOKIE}={token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 303);
        let set_cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
        assert!(set_cookie.contains(&format!("{RUS_SESSION_COOKIE}=")));
        assert!(set_cookie.contains("Max-Age=0"));
        let loc = resp.headers().get("Location").unwrap().to_str().unwrap();
        assert!(loc.starts_with("https://idp.example.com/oauth2/logout"));

        // user_sessions row deleted
        let count: i64 = app_state
            .db
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM user_sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
