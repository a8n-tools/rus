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
use super::session::{self, build_session_cookie, hash_session_token, RUS_SESSION_COOKIE};
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
    http_req: HttpRequest,
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

    // RUS-19: the OP authenticated the user, but this is where THIS app first
    // mints its own credential, and an OP session is portable: a stolen one
    // replays into rus from anywhere with no second prompt. So the same
    // new-country signal that alerts below holds the sign-in here.
    let gate = {
        let db = app_state.db.lock().unwrap_or_else(|e| e.into_inner());
        crate::login_approval::gate_login(
            &db,
            &app_state.config.mail,
            provisioned.user_id,
            &http_req,
        )
    };
    if let Some(context) = &gate {
        match &context.decision {
            crate::login_approval::GateDecision::Hold(recipient) => {
                let recipient = recipient.clone();
                return match crate::login_approval::request_login_approval(
                    &app_state,
                    provisioned.user_id,
                    context,
                    &recipient,
                    &http_req,
                )
                .await
                {
                    Ok(()) => redirect(&format!(
                        "{}?pending=1",
                        crate::login_approval::APPROVAL_PAGE_PATH
                    )),
                    Err(e) => {
                        tracing::error!(error = %e, "RUS-19: approval mail failed, sign-in refused");
                        redirect(&format!(
                            "{}?error=mail",
                            crate::login_approval::APPROVAL_PAGE_PATH
                        ))
                    }
                };
            }
            // Nothing could deliver the link, and a hold with no way to release
            // it is a lockout, so the sign-in completes and RUS-7 alerts.
            crate::login_approval::GateDecision::AllowUndeliverable => {
                tracing::warn!(
                    user_id = provisioned.user_id,
                    "RUS-19: new-country sign-in allowed because no approval link could be delivered"
                );
            }
            crate::login_approval::GateDecision::Allow => {}
        }
    }

    // Issue session.
    let session_token = {
        let db = app_state.db.lock().unwrap_or_else(|e| e.into_inner());
        match session::establish_session(
            &db,
            provisioned.user_id,
            state.config.session_ttl_seconds,
            true,
        ) {
            Ok(token) => token,
            Err(e) => {
                tracing::error!(error = %e, "failed to insert user_session");
                return HttpResponse::InternalServerError().finish();
            }
        }
    };

    // RUS-7: the OP alerts on sign-ins to itself, not to this app, so a reused
    // OP session from a new country would otherwise be silent here.
    crate::location_alert::spawn_new_location_check(&app_state, provisioned.user_id, &http_req);

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

    let seeded: rusqlite::Result<String> = (|| {
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
        // Same helper as the callback, so the dev fixture cannot drift from the
        // real mint site. Not gated: it is a local fixture, not a sign-in.
        session::establish_session(&db, user_id, state.config.session_ttl_seconds, false)
    })();

    let session_token = match seeded {
        Ok(token) => token,
        Err(e) => {
            tracing::error!(error = %e, "dev_seed_session failed");
            return HttpResponse::InternalServerError().body(format!("dev seed failed: {e}"));
        }
    };

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
    use crate::config::{MailConfig, OidcConfig};
    use crate::testing::{insert_saas_user, make_saas_session, make_test_state, StubOp, StubSmtp};
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

    // ── RUS-22: the callback end to end against a stubbed OP ─────────────────
    //
    // The saas gate lives in `callback`, and until there was an OP to talk to,
    // its call site was covered by reading the code. These drive the real route
    // against a loopback stub that signs ID tokens with a key the app fetches
    // from the stub's JWKS and verifies, so nothing here is bypassed.

    const STUB_SUB: &str = "22222222-2222-2222-2222-222222222222";
    const STUB_STATE: &str = "stub-pkce-state";
    const STUB_NONCE: &str = "stub-pkce-nonce";
    const STUB_PEER: &str = "127.0.0.1:34567";

    /// Trust the loopback so `X-IPCountry` resolves at all (RUS-12). The set is
    /// a `OnceLock`; every test that installs one installs these same CIDRs.
    fn trust_loopback() {
        crate::location_alert::init_trusted_proxies(vec![
            "127.0.0.0/8".parse().unwrap(),
            "::1/128".parse().unwrap(),
        ]);
    }

    /// App and RP state wired to `op`, sharing one `OidcConfig` so the token
    /// exchange, the verifier and the cookie all read the same values.
    fn stub_state(op: &StubOp, mail: MailConfig) -> (web::Data<AppState>, web::Data<OidcRpState>) {
        let mut config = crate::testing::test_config();
        config.oidc = op.oidc_config();
        config.mail = mail;
        let oidc = config.oidc.clone();
        let app_state = web::Data::new(AppState::new(config).expect("stub AppState"));
        let verifier = Arc::new(OidcVerifier::new(oidc.clone()));
        (app_state, web::Data::new(OidcRpState::new(oidc, verifier)))
    }

    /// Seed the `rp_sessions` row `login` would have written. A negative
    /// `minutes` makes it already expired.
    fn seed_rp_session(state: &web::Data<AppState>, pkce_state: &str, nonce: &str, minutes: i64) {
        let now = Utc::now();
        let db = state.db.lock().unwrap();
        db.execute(
            "INSERT INTO rp_sessions (id, state, nonce, code_verifier, return_to, created_at, expires_at)
             VALUES (?1, ?2, ?3, 'stub-code-verifier', NULL, ?4, ?5)",
            params![
                Uuid::new_v4().to_string(),
                pkce_state,
                nonce,
                rfc3339(now),
                rfc3339(now + chrono::Duration::minutes(minutes)),
            ],
        )
        .expect("seed rp_session");
    }

    /// An account that has signed in before, from `country`.
    fn returning_user(state: &web::Data<AppState>, country: &str) -> i64 {
        let user_id = insert_saas_user(state, "alice", STUB_SUB, false);
        let db = state.db.lock().unwrap();
        db.execute(
            "UPDATE users SET last_login_country = ?1 WHERE userID = ?2",
            params![country, user_id],
        )
        .expect("seed last_login_country");
        user_id
    }

    /// The callback the OP redirects the browser to. `country` is what the
    /// geoblock edge resolved, and is absent off that edge.
    fn callback_request(pkce_state: &str, country: Option<&str>) -> test::TestRequest {
        let mut req = test::TestRequest::get()
            .uri(&format!(
                "/oauth2/callback?code=stub-code&state={pkce_state}"
            ))
            .peer_addr(STUB_PEER.parse().unwrap());
        if let Some(value) = country {
            req = req.insert_header(("X-IPCountry", value));
        }
        req
    }

    /// The app as `main.rs` mounts it: the callback plus the approval routes,
    /// so a held sign-in can be released through the same app that held it.
    macro_rules! callback_app {
        ($app_state:expr, $rp_state:expr) => {
            test::init_service(
                App::new()
                    .app_data($app_state)
                    .app_data($rp_state)
                    .route("/oauth2/callback", web::get().to(callback))
                    .configure(crate::login_approval::configure_routes),
            )
            .await
        };
    }

    /// The `rus_session` value a response sets, if it sets one at all.
    fn session_cookie(headers: &actix_web::http::header::HeaderMap) -> Option<String> {
        headers
            .get_all(header::SET_COOKIE)
            .filter_map(|value| value.to_str().ok())
            .filter_map(|raw| Cookie::parse_encoded(raw.to_string()).ok())
            .find(|cookie| cookie.name() == RUS_SESSION_COOKIE)
            .map(|cookie| cookie.value().to_string())
    }

    fn location(headers: &actix_web::http::header::HeaderMap) -> String {
        headers
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string()
    }

    fn count(state: &web::Data<AppState>, table: &str) -> i64 {
        let db = state.db.lock().unwrap();
        db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .expect("count query")
    }

    fn stored_email(state: &web::Data<AppState>, user_id: i64) -> Option<String> {
        let db = state.db.lock().unwrap();
        db.query_row(
            "SELECT email FROM users WHERE userID = ?1",
            params![user_id],
            |row| row.get(0),
        )
        .expect("read users.email")
    }

    /// The `token` query value from the approval link in a delivered message.
    fn approval_token(message: &str) -> Option<String> {
        let needle = format!("{}?token=", crate::login_approval::APPROVAL_PAGE_PATH);
        let start = message.find(&needle)? + needle.len();
        Some(
            message[start..]
                .chars()
                .take_while(|c| !c.is_whitespace())
                .collect(),
        )
    }

    #[actix_web::test]
    async fn a_callback_mints_a_session_and_provisions_the_user() {
        let op = StubOp::start().await;
        op.issue_id_token(&op.claims(STUB_SUB, STUB_NONCE, Some("Alice@Example.com")));
        let (app_state, rp) = stub_state(&op, MailConfig::default());
        seed_rp_session(&app_state, STUB_STATE, STUB_NONCE, 10);
        let app = callback_app!(app_state.clone(), rp);

        let resp = test::call_service(&app, callback_request(STUB_STATE, None).to_request()).await;

        assert_eq!(resp.status(), 303);
        assert_eq!(location(resp.headers()), "/dashboard.html");
        let token = session_cookie(resp.headers()).expect("the callback sets rus_session");

        let db = app_state.db.lock().unwrap();
        let user = session::lookup_session(&db, &token)
            .unwrap()
            .expect("the cookie resolves to a session");
        let (username, saas_user_id, email): (String, String, Option<String>) = db
            .query_row(
                "SELECT username, saas_user_id, email FROM users WHERE userID = ?1",
                params![user.user_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("the user was provisioned");
        assert!(user.auth_via_oidc);
        assert_eq!(saas_user_id, STUB_SUB);
        assert_eq!(username, "alice", "the username comes off the email claim");
        assert_eq!(
            email.as_deref(),
            Some("alice@example.com"),
            "the OP identity lands in users.email, normalized"
        );

        // The PKCE row is consumed, and the exchange really carried its verifier.
        let leftover: i64 = db
            .query_row("SELECT COUNT(*) FROM rp_sessions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(leftover, 0);
        let exchanged = op.token_requests();
        let form = exchanged.first().expect("the token endpoint was called");
        assert!(form.contains("grant_type=authorization_code"));
        assert!(form.contains("code=stub-code"));
        assert!(form.contains("code_verifier=stub-code-verifier"));
    }

    #[actix_web::test]
    async fn an_id_token_signed_by_another_key_is_rejected() {
        let op = StubOp::start().await;
        // Same claims and the same published kid as the accepted token above,
        // signed with a key the JWKS does not carry. The only difference
        // between this and a completed sign-in is the signature itself.
        op.issue_forged_id_token(&op.claims(STUB_SUB, STUB_NONCE, Some("alice@example.com")));
        let (app_state, rp) = stub_state(&op, MailConfig::default());
        seed_rp_session(&app_state, STUB_STATE, STUB_NONCE, 10);
        let app = callback_app!(app_state.clone(), rp);

        let resp = test::call_service(&app, callback_request(STUB_STATE, None).to_request()).await;

        assert_eq!(resp.status(), 401);
        assert!(session_cookie(resp.headers()).is_none());
        assert_eq!(count(&app_state, "users"), 0, "nothing is provisioned");
        assert_eq!(count(&app_state, "user_sessions"), 0);
    }

    #[actix_web::test]
    async fn an_id_token_bound_to_another_nonce_is_rejected() {
        let op = StubOp::start().await;
        op.issue_id_token(&op.claims(STUB_SUB, "a-nonce-from-another-flow", Some("a@b.com")));
        let (app_state, rp) = stub_state(&op, MailConfig::default());
        seed_rp_session(&app_state, STUB_STATE, STUB_NONCE, 10);
        let app = callback_app!(app_state.clone(), rp);

        let resp = test::call_service(&app, callback_request(STUB_STATE, None).to_request()).await;

        assert_eq!(resp.status(), 401);
        assert!(session_cookie(resp.headers()).is_none());
        assert_eq!(count(&app_state, "user_sessions"), 0);
    }

    // ── The two RUS-19 never-gated properties, at the route level ────────────

    #[actix_web::test]
    async fn route_level_a_first_ever_sign_in_through_the_callback_is_never_held() {
        trust_loopback();
        let sink = StubSmtp::start().await;
        let op = StubOp::start().await;
        op.issue_id_token(&op.claims(STUB_SUB, STUB_NONCE, Some("alice@example.com")));
        // The gate is on and the country resolves; the account is provisioned
        // by this very request, so it has no country to be new against.
        let (app_state, rp) = stub_state(&op, sink.mail_config());
        seed_rp_session(&app_state, STUB_STATE, STUB_NONCE, 10);
        let app = callback_app!(app_state.clone(), rp);

        let resp =
            test::call_service(&app, callback_request(STUB_STATE, Some("DE")).to_request()).await;

        assert_eq!(resp.status(), 303);
        assert_eq!(location(resp.headers()), "/dashboard.html");
        assert!(
            session_cookie(resp.headers()).is_some(),
            "the first sign-in an account ever makes must complete, or it could never sign in"
        );
        assert_eq!(count(&app_state, "pending_login_approvals"), 0);
        assert!(
            sink.messages().is_empty(),
            "nothing to approve, nothing sent"
        );
    }

    #[actix_web::test]
    async fn route_level_an_unresolved_country_through_the_callback_is_never_held() {
        trust_loopback();
        let sink = StubSmtp::start().await;
        let op = StubOp::start().await;
        op.issue_id_token(&op.claims(STUB_SUB, STUB_NONCE, Some("alice@example.com")));
        let (app_state, rp) = stub_state(&op, sink.mail_config());
        // Signed in from the US before, and this request carries no country:
        // off the geoblock edge every sign-in looks like this one.
        returning_user(&app_state, "US");
        seed_rp_session(&app_state, STUB_STATE, STUB_NONCE, 10);
        let app = callback_app!(app_state.clone(), rp);

        let resp = test::call_service(&app, callback_request(STUB_STATE, None).to_request()).await;

        assert_eq!(resp.status(), 303);
        assert_eq!(location(resp.headers()), "/dashboard.html");
        assert!(
            session_cookie(resp.headers()).is_some(),
            "an unresolved country must never hold, or a deployment without the edge bricks itself"
        );
        assert_eq!(count(&app_state, "pending_login_approvals"), 0);
        assert!(sink.messages().is_empty());
    }

    #[actix_web::test]
    async fn route_level_a_new_country_is_held_and_the_emailed_link_releases_it() {
        trust_loopback();
        let sink = StubSmtp::start().await;
        let op = StubOp::start().await;
        op.issue_id_token(&op.claims(STUB_SUB, STUB_NONCE, Some("alice@example.com")));
        let (app_state, rp) = stub_state(&op, sink.mail_config());
        let user_id = returning_user(&app_state, "US");
        seed_rp_session(&app_state, STUB_STATE, STUB_NONCE, 10);
        let app = callback_app!(app_state.clone(), rp);

        let held =
            test::call_service(&app, callback_request(STUB_STATE, Some("DE")).to_request()).await;

        assert_eq!(held.status(), 303);
        assert_eq!(
            location(held.headers()),
            format!("{}?pending=1", crate::login_approval::APPROVAL_PAGE_PATH)
        );
        assert!(
            session_cookie(held.headers()).is_none(),
            "a held sign-in mints no credential"
        );
        assert_eq!(count(&app_state, "user_sessions"), 0);
        assert_eq!(count(&app_state, "pending_login_approvals"), 1);

        // The link the app actually mailed, not one the test made up.
        let messages = sink.messages();
        let message = messages.first().expect("the approval mail was sent");
        let token = approval_token(message).expect("the mail carries an approval link");

        let released = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(crate::login_approval::APPROVAL_API_PATH)
                .peer_addr(STUB_PEER.parse().unwrap())
                .set_json(serde_json::json!({ "token": token }))
                .to_request(),
        )
        .await;

        assert_eq!(released.status(), 200);
        let session = session_cookie(released.headers()).expect("the release mints the session");
        let db = app_state.db.lock().unwrap();
        let user = session::lookup_session(&db, &session)
            .unwrap()
            .expect("the released cookie resolves");
        assert_eq!(user.user_id, user_id);
        let country: Option<String> = db
            .query_row(
                "SELECT last_login_country FROM users WHERE userID = ?1",
                params![user_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            country.as_deref(),
            Some("DE"),
            "only a completed sign-in makes the country familiar"
        );
    }

    // ── RUS-11: the email claim on a repeat sign-in ──────────────────────────

    #[actix_web::test]
    async fn a_repeat_sign_in_stores_the_email_claim_the_op_sent() {
        let op = StubOp::start().await;
        op.issue_id_token(&op.claims(STUB_SUB, STUB_NONCE, Some("alice.new@example.com")));
        let (app_state, rp) = stub_state(&op, MailConfig::default());
        let user_id = insert_saas_user(&app_state, "alice", STUB_SUB, false);
        seed_rp_session(&app_state, STUB_STATE, STUB_NONCE, 10);
        let app = callback_app!(app_state.clone(), rp);

        let resp = test::call_service(&app, callback_request(STUB_STATE, None).to_request()).await;

        assert_eq!(resp.status(), 303);
        assert_eq!(
            stored_email(&app_state, user_id).as_deref(),
            Some("alice.new@example.com")
        );
    }

    #[actix_web::test]
    async fn a_repeat_sign_in_with_no_email_claim_stores_null_not_a_blank() {
        let op = StubOp::start().await;
        op.issue_id_token(&op.claims(STUB_SUB, STUB_NONCE, None));
        let (app_state, rp) = stub_state(&op, MailConfig::default());
        let user_id = insert_saas_user(&app_state, "alice", STUB_SUB, false);
        seed_rp_session(&app_state, STUB_STATE, STUB_NONCE, 10);
        let app = callback_app!(app_state.clone(), rp);

        let resp = test::call_service(&app, callback_request(STUB_STATE, None).to_request()).await;

        assert_eq!(resp.status(), 303);
        assert_eq!(
            stored_email(&app_state, user_id),
            None,
            "an omitted claim stores NULL, never the empty string RUS-11 found"
        );
    }

    // ── PKCE state and session expiry ────────────────────────────────────────

    #[actix_web::test]
    async fn a_callback_with_an_unknown_state_is_rejected() {
        let op = StubOp::start().await;
        op.issue_id_token(&op.claims(STUB_SUB, STUB_NONCE, Some("alice@example.com")));
        let (app_state, rp) = stub_state(&op, MailConfig::default());
        seed_rp_session(&app_state, STUB_STATE, STUB_NONCE, 10);
        let app = callback_app!(app_state.clone(), rp);

        let resp = test::call_service(
            &app,
            callback_request("not-the-state-we-issued", None).to_request(),
        )
        .await;

        assert_eq!(resp.status(), 400);
        assert!(session_cookie(resp.headers()).is_none());
        assert!(
            op.token_requests().is_empty(),
            "a state we never issued must not reach the token endpoint"
        );
        assert_eq!(count(&app_state, "rp_sessions"), 1, "the real row survives");
    }

    #[actix_web::test]
    async fn a_replayed_state_is_rejected_after_the_first_use() {
        let op = StubOp::start().await;
        op.issue_id_token(&op.claims(STUB_SUB, STUB_NONCE, Some("alice@example.com")));
        let (app_state, rp) = stub_state(&op, MailConfig::default());
        seed_rp_session(&app_state, STUB_STATE, STUB_NONCE, 10);
        let app = callback_app!(app_state.clone(), rp);

        let first = test::call_service(&app, callback_request(STUB_STATE, None).to_request()).await;
        assert_eq!(first.status(), 303);
        assert!(session_cookie(first.headers()).is_some());

        let replay =
            test::call_service(&app, callback_request(STUB_STATE, None).to_request()).await;

        assert_eq!(
            replay.status(),
            400,
            "the state was consumed by the first use"
        );
        assert!(session_cookie(replay.headers()).is_none());
        assert_eq!(count(&app_state, "user_sessions"), 1, "no second session");
    }

    #[actix_web::test]
    async fn an_expired_rp_session_is_rejected() {
        let op = StubOp::start().await;
        op.issue_id_token(&op.claims(STUB_SUB, STUB_NONCE, Some("alice@example.com")));
        let (app_state, rp) = stub_state(&op, MailConfig::default());
        seed_rp_session(&app_state, STUB_STATE, STUB_NONCE, -1);
        let app = callback_app!(app_state.clone(), rp);

        let resp = test::call_service(&app, callback_request(STUB_STATE, None).to_request()).await;

        assert_eq!(resp.status(), 400);
        assert!(session_cookie(resp.headers()).is_none());
        assert!(
            op.token_requests().is_empty(),
            "an expired login session must not reach the token endpoint"
        );
        assert_eq!(
            count(&app_state, "rp_sessions"),
            0,
            "the stale row is dropped"
        );
    }
}
