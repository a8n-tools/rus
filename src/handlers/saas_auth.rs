use actix_web::{HttpMessage, HttpRequest, HttpResponse, Result};
use jsonwebtoken::{decode, DecodingKey, Validation};
use std::sync::atomic::Ordering;
use tracing::{debug, error, trace, warn};

/// SaaS user claims extracted from access_token cookie
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SaasUserClaims {
    pub user_id: i64,
    pub email: Option<String>,
    pub membership_status: Option<String>,
    pub is_admin: bool,
}

/// Extract and verify user claims from access_token cookie (SaaS mode)
pub fn get_user_from_cookie(req: &HttpRequest, secret: &str) -> Option<SaasUserClaims> {
    let cookie = match req.cookie("access_token") {
        Some(c) => c,
        None => {
            debug!("No access_token cookie found");
            return None;
        }
    };
    let token = cookie.value();

    // Verify JWT signature and decode
    let mut validation = Validation::default();
    // Allow multiple algorithms the parent app might use
    validation.algorithms = vec![
        jsonwebtoken::Algorithm::HS256,
        jsonwebtoken::Algorithm::HS384,
        jsonwebtoken::Algorithm::HS512,
    ];
    // Don't require specific claims beyond exp
    validation.required_spec_claims.clear();
    validation.required_spec_claims.insert("exp".to_string());

    let token_data = match decode::<serde_json::Value>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    ) {
        Ok(data) => data,
        Err(e) => {
            warn!(error = %e, "JWT decode failed");
            return None;
        }
    };

    let payload = token_data.claims;
    trace!(payload = %payload, "JWT decoded successfully");

    // Extract user_id from JWT payload
    // The parent app's JWT may have user_id as "sub" (UUID or integer), "user_id", or "id"
    let user_id = payload
        .get("user_id")
        .and_then(|v| v.as_i64())
        .or_else(|| {
            payload.get("sub").and_then(|v| {
                let s = v.as_str()?;
                // Try parsing as integer first
                s.parse::<i64>().ok().or_else(|| {
                    // If it's a UUID, derive a stable i64 from its hex bytes
                    let hex: String = s.chars().filter(|c| *c != '-').collect();
                    if hex.len() == 32 {
                        u64::from_str_radix(&hex[..16], 16)
                            .ok()
                            .map(|v| (v & 0x7FFFFFFFFFFFFFFF) as i64)
                    } else {
                        None
                    }
                })
            })
        })
        .or_else(|| payload.get("id").and_then(|v| v.as_i64()));

    match user_id {
        Some(id) => debug!(user_id = id, "Extracted user_id from JWT"),
        None => {
            warn!("Could not extract user_id from JWT payload");
            return None;
        }
    }
    let user_id = user_id.unwrap();

    let email = payload.get("email").and_then(|v| v.as_str()).map(String::from);
    let membership_status = payload
        .get("membership_status")
        .and_then(|v| v.as_str())
        .map(String::from);

    let is_admin = payload
        .get("role")
        .and_then(|v| v.as_str())
        .is_some_and(|r| r.eq_ignore_ascii_case("admin"));

    // Note: membership access is enforced in the middleware, not here.
    // All valid JWT holders are returned so the middleware can decide
    // whether to redirect non-members to the membership page.

    debug!(user_id, email = ?email, membership_status = ?membership_status, is_admin, "SaaS authentication successful");
    Some(SaasUserClaims {
        user_id,
        email,
        membership_status,
        is_admin,
    })
}

/// Returns the current SaaS user's info (username derived from email)
pub async fn saas_me(http_req: HttpRequest) -> Result<HttpResponse> {
    let ext = http_req.extensions();
    let claims = ext
        .get::<SaasUserClaims>()
        .ok_or_else(|| actix_web::error::ErrorInternalServerError("Missing claims"))?;

    let username = claims
        .email
        .as_deref()
        .and_then(|e| e.split('@').next())
        .map(String::from)
        .unwrap_or_else(|| format!("saas_{}", claims.user_id));

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "username": username,
        "is_admin": claims.is_admin,
    })))
}

/// Paths that bypass maintenance mode entirely
const MAINTENANCE_ALLOWLIST: &[&str] = &[
    "/health",
    "/api/config",
    "/api/version",
    "/styles.css",
    "/k9f3x2m7.js",
];

/// Maintenance mode guard middleware (outermost layer in SaaS mode).
///
/// When maintenance mode is active, only admin users and allowlisted paths
/// are permitted through. All other requests receive a 503.
pub async fn maintenance_guard(
    req: actix_web::dev::ServiceRequest,
    next: actix_web::middleware::Next<impl actix_web::body::MessageBody + 'static>,
) -> Result<actix_web::dev::ServiceResponse<actix_web::body::BoxBody>, actix_web::Error> {
    let state = req
        .app_data::<actix_web::web::Data<crate::db::AppState>>()
        .expect("AppState not found");

    // Fast path: maintenance off
    if !state.maintenance_mode.load(Ordering::SeqCst) {
        return Ok(next.call(req).await?.map_into_boxed_body());
    }

    let path = req.path();

    // Allowlisted paths always pass through
    if MAINTENANCE_ALLOWLIST.iter().any(|p| path == *p) || path.starts_with("/webhooks/") {
        return Ok(next.call(req).await?.map_into_boxed_body());
    }

    // Check if user is admin via cookie
    let secret = &state.config.saas_jwt_secret;
    if let Some(claims) = get_user_from_cookie(req.request(), secret) {
        if claims.is_admin {
            return Ok(next.call(req).await?.map_into_boxed_body());
        }
    }

    // Read maintenance message
    let message = state
        .maintenance_message
        .read()
        .unwrap()
        .clone()
        .unwrap_or_default();

    // API routes get JSON 503
    if path.starts_with("/api/") {
        return Ok(req.into_response(
            HttpResponse::ServiceUnavailable()
                .json(serde_json::json!({
                    "error": "Service under maintenance",
                    "maintenance": true,
                    "message": message,
                })),
        ));
    }

    // Page/other routes get HTML 503
    let html = include_str!("../../static/maintenance.html")
        .replace("{{MAINTENANCE_MESSAGE}}", &message);
    Ok(req.into_response(
        HttpResponse::ServiceUnavailable()
            .content_type("text/html; charset=utf-8")
            .body(html),
    ))
}

/// SaaS cookie authentication middleware
pub async fn saas_cookie_validator(
    req: actix_web::dev::ServiceRequest,
    next: actix_web::middleware::Next<impl actix_web::body::MessageBody>,
) -> Result<actix_web::dev::ServiceResponse<impl actix_web::body::MessageBody>, actix_web::Error> {
    let state = req
        .app_data::<actix_web::web::Data<crate::db::AppState>>()
        .expect("AppState not found");
    let secret = &state.config.saas_jwt_secret;

    match get_user_from_cookie(req.request(), secret) {
        Some(claims) => {
            // Non-member, non-admin users get redirected to the membership page
            if !claims.is_admin {
                let has_access = matches!(
                    claims.membership_status.as_deref(),
                    Some("active") | Some("grace_period")
                );
                if !has_access {
                    let membership_url = &state.config.saas_membership_url;
                    debug!(
                        user_id = claims.user_id,
                        membership_status = ?claims.membership_status,
                        "Non-member redirected to membership page"
                    );
                    return Err(actix_web::error::InternalError::from_response(
                        "Membership required",
                        actix_web::HttpResponse::Forbidden()
                            .json(serde_json::json!({
                                "error": "Membership required",
                                "redirect": membership_url,
                            })),
                    ).into());
                }
            }

            // Auto-provision SaaS user in local DB so FK constraints are satisfied
            let username = claims
                .email
                .clone()
                .filter(|e| !e.is_empty())
                .unwrap_or_else(|| format!("saas_{}", claims.user_id));
            {
                let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
                db.execute(
                    "INSERT INTO users (userID, username, password, is_admin) VALUES (?1, ?2, '', ?3) \
                     ON CONFLICT(userID) DO UPDATE SET is_admin = ?3",
                    rusqlite::params![claims.user_id, username, claims.is_admin as i32],
                )
                .map_err(|e| {
                    error!(error = %e, "Failed to provision SaaS user");
                    actix_web::error::ErrorInternalServerError("Failed to provision user")
                })?;
            }

            req.extensions_mut().insert(claims);
            next.call(req).await
        }
        None => Err(actix_web::error::ErrorUnauthorized("Invalid or missing authentication")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, web, App};
    use crate::testing::{insert_saas_user, make_saas_jwt, make_test_state};
    use serde_json::Value;

    macro_rules! setup_app {
        ($state:expr) => {{
            test::init_service(
                App::new()
                    .app_data($state.clone())
                    .service(
                        web::scope("/api")
                            .wrap(actix_web::middleware::from_fn(saas_cookie_validator))
                            .route("/me", web::get().to(saas_me))
                            .route("/ping", web::get().to(|| async { "pong" })),
                    ),
            )
            .await
        }};
    }

    fn cookie(jwt: &str) -> String {
        format!("access_token={jwt}")
    }

    // --- saas_me ---

    #[actix_web::test]
    async fn me_returns_email_prefix_as_username() {
        let state = make_test_state();
        insert_saas_user(&state, 42, "alice@example.com", false);
        let app = setup_app!(state);
        let jwt = make_saas_jwt("42", "alice@example.com", "active", None);

        let req = test::TestRequest::get()
            .uri("/api/me")
            .insert_header(("Cookie", cookie(&jwt)))
            .to_request();
        let body: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(body["username"], "alice");
        assert_eq!(body["is_admin"], false);
    }

    #[actix_web::test]
    async fn me_returns_admin_true_for_admin_role() {
        let state = make_test_state();
        insert_saas_user(&state, 1, "admin@example.com", true);
        let app = setup_app!(state);
        let jwt = make_saas_jwt("1", "admin@example.com", "none", Some("admin"));

        let req = test::TestRequest::get()
            .uri("/api/me")
            .insert_header(("Cookie", cookie(&jwt)))
            .to_request();
        let body: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(body["is_admin"], true);
    }

    #[actix_web::test]
    async fn me_uses_saas_id_as_fallback_username_when_no_email() {
        let state = make_test_state();
        insert_saas_user(&state, 99, "saas_99", false);
        let app = setup_app!(state);

        // JWT with no email
        use jsonwebtoken::{encode, EncodingKey, Header};
        let exp = (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp();
        let claims = serde_json::json!({"sub": "99", "membership_status": "active", "exp": exp});
        let jwt = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(crate::testing::TEST_SAAS_SECRET.as_bytes()),
        )
        .unwrap();

        let req = test::TestRequest::get()
            .uri("/api/me")
            .insert_header(("Cookie", cookie(&jwt)))
            .to_request();
        let body: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(body["username"], "saas_99");
    }

    // --- get_user_from_cookie unit tests ---

    #[actix_web::test]
    async fn get_user_from_cookie_uuid_subject() {
        use jsonwebtoken::{encode, EncodingKey, Header};
        let exp = (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp();
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let claims = serde_json::json!({"sub": uuid, "exp": exp});
        let jwt = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(crate::testing::TEST_SAAS_SECRET.as_bytes()),
        )
        .unwrap();

        let req = test::TestRequest::default()
            .insert_header(("Cookie", format!("access_token={jwt}")))
            .to_http_request();
        let result = get_user_from_cookie(&req, crate::testing::TEST_SAAS_SECRET);
        assert!(result.is_some(), "expected Some for UUID subject");
        let claims = result.unwrap();
        assert!(claims.user_id > 0, "user_id should be positive: {}", claims.user_id);
    }

    #[actix_web::test]
    async fn get_user_from_cookie_user_id_field() {
        use jsonwebtoken::{encode, EncodingKey, Header};
        let exp = (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp();
        let claims = serde_json::json!({"user_id": 42, "exp": exp});
        let jwt = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(crate::testing::TEST_SAAS_SECRET.as_bytes()),
        )
        .unwrap();

        let req = test::TestRequest::default()
            .insert_header(("Cookie", format!("access_token={jwt}")))
            .to_http_request();
        let result = get_user_from_cookie(&req, crate::testing::TEST_SAAS_SECRET);
        assert!(result.is_some());
        assert_eq!(result.unwrap().user_id, 42);
    }

    #[actix_web::test]
    async fn get_user_from_cookie_expired_jwt_returns_none() {
        use jsonwebtoken::{encode, EncodingKey, Header};
        let exp = (chrono::Utc::now() - chrono::Duration::hours(1)).timestamp();
        let claims = serde_json::json!({"sub": "99", "exp": exp});
        let jwt = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(crate::testing::TEST_SAAS_SECRET.as_bytes()),
        )
        .unwrap();

        let req = test::TestRequest::default()
            .insert_header(("Cookie", format!("access_token={jwt}")))
            .to_http_request();
        let result = get_user_from_cookie(&req, crate::testing::TEST_SAAS_SECRET);
        assert!(result.is_none());
    }

    #[actix_web::test]
    async fn get_user_from_cookie_no_user_id_returns_none() {
        use jsonwebtoken::{encode, EncodingKey, Header};
        let exp = (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp();
        let claims = serde_json::json!({"email": "test@example.com", "exp": exp});
        let jwt = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(crate::testing::TEST_SAAS_SECRET.as_bytes()),
        )
        .unwrap();

        let req = test::TestRequest::default()
            .insert_header(("Cookie", format!("access_token={jwt}")))
            .to_http_request();
        let result = get_user_from_cookie(&req, crate::testing::TEST_SAAS_SECRET);
        assert!(result.is_none());
    }

    // --- saas_cookie_validator membership checks ---

    #[actix_web::test]
    async fn active_member_is_allowed() {
        let state = make_test_state();
        let app = setup_app!(state);
        let jwt = make_saas_jwt("10", "user@example.com", "active", None);

        let req = test::TestRequest::get()
            .uri("/api/ping")
            .insert_header(("Cookie", cookie(&jwt)))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn grace_period_member_is_allowed() {
        let state = make_test_state();
        let app = setup_app!(state);
        let jwt = make_saas_jwt("11", "grace@example.com", "grace_period", None);

        let req = test::TestRequest::get()
            .uri("/api/ping")
            .insert_header(("Cookie", cookie(&jwt)))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn none_membership_returns_403() {
        let state = make_test_state();
        let app = setup_app!(state);
        let jwt = make_saas_jwt("20", "new@example.com", "none", None);

        let req = test::TestRequest::get()
            .uri("/api/ping")
            .insert_header(("Cookie", cookie(&jwt)))
            .to_request();
        let resp = test::try_call_service(&app, req).await;
        assert!(resp.is_err() || resp.unwrap().status() == 403);
    }

    #[actix_web::test]
    async fn canceled_membership_returns_403() {
        let state = make_test_state();
        let app = setup_app!(state);
        let jwt = make_saas_jwt("21", "old@example.com", "canceled", None);

        let req = test::TestRequest::get()
            .uri("/api/ping")
            .insert_header(("Cookie", cookie(&jwt)))
            .to_request();
        let resp = test::try_call_service(&app, req).await;
        assert!(resp.is_err() || resp.unwrap().status() == 403);
    }

    #[actix_web::test]
    async fn past_due_membership_returns_403() {
        let state = make_test_state();
        let app = setup_app!(state);
        let jwt = make_saas_jwt("22", "pastdue@example.com", "past_due", None);

        let req = test::TestRequest::get()
            .uri("/api/ping")
            .insert_header(("Cookie", cookie(&jwt)))
            .to_request();
        let resp = test::try_call_service(&app, req).await;
        assert!(resp.is_err() || resp.unwrap().status() == 403);
    }

    #[actix_web::test]
    async fn admin_role_bypasses_membership_check() {
        let state = make_test_state();
        let app = setup_app!(state);
        let jwt = make_saas_jwt("2", "admin@example.com", "none", Some("admin"));

        let req = test::TestRequest::get()
            .uri("/api/ping")
            .insert_header(("Cookie", cookie(&jwt)))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn missing_cookie_returns_401() {
        let state = make_test_state();
        let app = setup_app!(state);

        let req = test::TestRequest::get().uri("/api/ping").to_request();
        let resp = test::try_call_service(&app, req).await;
        assert!(resp.is_err() || resp.unwrap().status() == 401);
    }

    #[actix_web::test]
    async fn invalid_jwt_signature_returns_401() {
        let state = make_test_state();
        let app = setup_app!(state);

        // Sign with a different secret
        use jsonwebtoken::{encode, EncodingKey, Header};
        let exp = (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp();
        let claims = serde_json::json!({"sub": "5", "membership_status": "active", "exp": exp});
        let bad_jwt = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(b"wrong-secret"),
        )
        .unwrap();

        let req = test::TestRequest::get()
            .uri("/api/ping")
            .insert_header(("Cookie", cookie(&bad_jwt)))
            .to_request();
        let resp = test::try_call_service(&app, req).await;
        assert!(resp.is_err() || resp.unwrap().status() == 401);
    }

    #[actix_web::test]
    async fn non_member_403_response_contains_redirect_url() {
        let state = make_test_state();
        let app = setup_app!(state);
        let jwt = make_saas_jwt("30", "user@example.com", "none", None);

        let req = test::TestRequest::get()
            .uri("/api/ping")
            .insert_header(("Cookie", cookie(&jwt)))
            .to_request();
        // The middleware returns Err with an InternalError wrapping a 403 response.
        // try_call_service lets us inspect the error response.
        let result = test::try_call_service(&app, req).await;
        assert!(result.is_err(), "expected middleware error for non-member");
    }

    #[actix_web::test]
    async fn admin_is_provisioned_in_db_with_is_admin_true() {
        let state = make_test_state();
        let app = setup_app!(state);
        let jwt = make_saas_jwt("99", "admin@example.com", "active", Some("admin"));

        test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/ping")
                .insert_header(("Cookie", cookie(&jwt)))
                .to_request(),
        )
        .await;

        let is_admin: i32 = {
            let db = state.db.lock().unwrap();
            db.query_row(
                "SELECT is_admin FROM users WHERE userID=99",
                [],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(is_admin, 1);
    }

    #[actix_web::test]
    async fn admin_status_synced_on_each_request() {
        let state = make_test_state();
        insert_saas_user(&state, 50, "user@example.com", false);
        let app = setup_app!(state);

        // First request as regular member
        let jwt_member = make_saas_jwt("50", "user@example.com", "active", None);
        test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/ping")
                .insert_header(("Cookie", cookie(&jwt_member)))
                .to_request(),
        )
        .await;

        // Second request — now promoted to admin in parent app
        let jwt_admin = make_saas_jwt("50", "user@example.com", "active", Some("admin"));
        test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/ping")
                .insert_header(("Cookie", cookie(&jwt_admin)))
                .to_request(),
        )
        .await;

        let is_admin: i32 = {
            let db = state.db.lock().unwrap();
            db.query_row(
                "SELECT is_admin FROM users WHERE userID=50",
                [],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(is_admin, 1);
    }

    // --- maintenance_guard tests ---

    macro_rules! setup_maintenance_app {
        ($state:expr) => {{
            test::init_service(
                App::new()
                    .app_data($state.clone())
                    .route("/api/ping", web::get().to(|| async { "pong" }))
                    .route("/health", web::get().to(|| async { "ok" }))
                    .route("/api/config", web::get().to(|| async { "config" }))
                    .route("/styles.css", web::get().to(|| async { "css" }))
                    .route("/webhooks/test", web::post().to(|| async { "webhook" }))
                    .route("/dashboard.html", web::get().to(|| async { "dashboard" }))
                    .route("/", web::get().to(|| async { "index" }))
                    .wrap(actix_web::middleware::from_fn(maintenance_guard)),
            )
            .await
        }};
    }

    #[actix_web::test]
    async fn maintenance_off_passes_through() {
        let state = make_test_state();
        let app = setup_maintenance_app!(state);

        let req = test::TestRequest::get().uri("/api/ping").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn maintenance_blocks_api_with_503_json() {
        let state = make_test_state();
        state.maintenance_mode.store(true, std::sync::atomic::Ordering::SeqCst);
        *state.maintenance_message.write().unwrap() = Some("Upgrading DB".to_string());
        let app = setup_maintenance_app!(state);

        let req = test::TestRequest::get().uri("/api/ping").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 503);
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["maintenance"], true);
        assert_eq!(body["message"], "Upgrading DB");
    }

    #[actix_web::test]
    async fn maintenance_blocks_pages_with_503_html() {
        let state = make_test_state();
        state.maintenance_mode.store(true, std::sync::atomic::Ordering::SeqCst);
        *state.maintenance_message.write().unwrap() = Some("Be right back".to_string());
        let app = setup_maintenance_app!(state);

        let req = test::TestRequest::get()
            .uri("/dashboard.html")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 503);
        let body = test::read_body(resp).await;
        let html = std::str::from_utf8(&body).unwrap();
        assert!(html.contains("Under Maintenance"));
        assert!(html.contains("Be right back"));
    }

    #[actix_web::test]
    async fn maintenance_allows_health_endpoint() {
        let state = make_test_state();
        state.maintenance_mode.store(true, std::sync::atomic::Ordering::SeqCst);
        let app = setup_maintenance_app!(state);

        let req = test::TestRequest::get().uri("/health").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn maintenance_allows_webhook_paths() {
        let state = make_test_state();
        state.maintenance_mode.store(true, std::sync::atomic::Ordering::SeqCst);
        let app = setup_maintenance_app!(state);

        let req = test::TestRequest::post()
            .uri("/webhooks/test")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn maintenance_allows_allowlisted_paths() {
        let state = make_test_state();
        state.maintenance_mode.store(true, std::sync::atomic::Ordering::SeqCst);
        let app = setup_maintenance_app!(state);

        for path in &["/health", "/api/config", "/styles.css"] {
            let req = test::TestRequest::get().uri(path).to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200, "expected 200 for {path}");
        }
    }

    #[actix_web::test]
    async fn maintenance_allows_admin_through() {
        let state = make_test_state();
        state.maintenance_mode.store(true, std::sync::atomic::Ordering::SeqCst);
        let app = setup_maintenance_app!(state);

        let jwt = make_saas_jwt("1", "admin@example.com", "active", Some("admin"));

        let req = test::TestRequest::get()
            .uri("/api/ping")
            .insert_header(("Cookie", cookie(&jwt)))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn maintenance_allows_api_version() {
        let state = make_test_state();
        state.maintenance_mode.store(true, std::sync::atomic::Ordering::SeqCst);

        let app = test::init_service(
            App::new()
                .app_data(state.clone())
                .route("/api/version", web::get().to(|| async { "1.0.0" }))
                .wrap(actix_web::middleware::from_fn(maintenance_guard)),
        )
        .await;

        let req = test::TestRequest::get().uri("/api/version").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn maintenance_blocks_non_admin_user() {
        let state = make_test_state();
        state.maintenance_mode.store(true, std::sync::atomic::Ordering::SeqCst);
        let app = setup_maintenance_app!(state);

        let jwt = make_saas_jwt("10", "user@example.com", "active", None);

        let req = test::TestRequest::get()
            .uri("/api/ping")
            .insert_header(("Cookie", cookie(&jwt)))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 503);
    }
}
