//! SaaS-mode auth helpers built on the OIDC BFF session layer in `crate::oidc`.
//!
//! The legacy HS256 cookie validator and `SaasUserClaims` (`access_token`
//! cookie) were removed in favor of the OIDC Authorization Code + PKCE flow.

use actix_web::{
    body::MessageBody,
    dev::{ServiceRequest, ServiceResponse},
    middleware::Next,
    web, HttpRequest, HttpResponse, Result,
};
use std::sync::atomic::Ordering;
use tracing::{debug, warn};

use crate::db::AppState;
use crate::oidc::session::{lookup_session, AuthenticatedUser};

/// Extract the authenticated user from the request via the BFF session cookie.
fn current_user(req: &HttpRequest, state: &AppState) -> Option<AuthenticatedUser> {
    let cookie = req.cookie(crate::oidc::RUS_SESSION_COOKIE)?;
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    match lookup_session(&db, cookie.value()) {
        Ok(Some(u)) => Some(u),
        Ok(None) => {
            debug!("session cookie present but invalid or expired");
            None
        }
        Err(e) => {
            warn!(error = %e, "session lookup failed");
            None
        }
    }
}

/// Returns the current SaaS user's profile (name + admin flag).
pub async fn saas_me(user: AuthenticatedUser) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "username": user.username,
        "is_admin": user.is_admin,
    })))
}

/// Paths that bypass maintenance mode entirely.
const MAINTENANCE_ALLOWLIST: &[&str] = &[
    "/health",
    "/api/config",
    "/api/version",
    "/styles.css",
    "/k9f3x2m7.js",
    "/theme.js",
];

/// Maintenance mode guard middleware (outermost layer in SaaS mode).
///
/// When maintenance mode is active, only admin users and allowlisted paths
/// are permitted through. All other requests receive a 503.
pub async fn maintenance_guard(
    req: ServiceRequest,
    next: Next<impl MessageBody + 'static>,
) -> Result<ServiceResponse<actix_web::body::BoxBody>, actix_web::Error> {
    let state = req
        .app_data::<web::Data<AppState>>()
        .expect("AppState not found")
        .clone();

    if !state.maintenance_mode.load(Ordering::SeqCst) {
        return Ok(next.call(req).await?.map_into_boxed_body());
    }

    let path = req.path().to_string();

    // OIDC routes always pass through so users can finish auth flow.
    if path.starts_with("/oauth2/") || path.starts_with("/webhooks/") || path.starts_with("/dev/") {
        return Ok(next.call(req).await?.map_into_boxed_body());
    }
    if MAINTENANCE_ALLOWLIST.iter().any(|p| path == *p) {
        return Ok(next.call(req).await?.map_into_boxed_body());
    }

    // Admin via session cookie bypasses maintenance.
    if let Some(user) = current_user(req.request(), &state) {
        if user.is_admin {
            return Ok(next.call(req).await?.map_into_boxed_body());
        }
    }

    let message = state
        .maintenance_message
        .read()
        .unwrap()
        .clone()
        .unwrap_or_default();

    if path.starts_with("/api/") {
        return Ok(
            req.into_response(HttpResponse::ServiceUnavailable().json(serde_json::json!({
                "error": "Service under maintenance",
                "maintenance": true,
                "message": message,
            }))),
        );
    }

    let html =
        include_str!("../../static/maintenance.html").replace("{{MAINTENANCE_MESSAGE}}", &message);
    Ok(req.into_response(
        HttpResponse::ServiceUnavailable()
            .content_type("text/html; charset=utf-8")
            .body(html),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oidc::session::RUS_SESSION_COOKIE;
    use crate::testing::{insert_saas_user, make_saas_session, make_test_state};
    use actix_web::{test, App};
    use std::sync::atomic::Ordering;

    const SUB_ADMIN: &str = "11111111-1111-1111-1111-111111111111";
    const SUB_USER: &str = "22222222-2222-2222-2222-222222222222";

    fn build(
        state: actix_web::web::Data<crate::db::AppState>,
    ) -> App<
        impl actix_web::dev::ServiceFactory<
            actix_web::dev::ServiceRequest,
            Config = (),
            Response = actix_web::dev::ServiceResponse,
            Error = actix_web::Error,
            InitError = (),
        >,
    > {
        App::new()
            .app_data(state)
            .route("/api/ping", web::get().to(|| async { "pong" }))
            .route("/dashboard.html", web::get().to(|| async { "dash" }))
            .route("/health", web::get().to(|| async { "ok" }))
            .route("/api/config", web::get().to(|| async { "config" }))
            .route("/oauth2/login", web::get().to(|| async { "login" }))
            .route("/webhooks/maintenance", web::post().to(|| async { "wh" }))
            .wrap(actix_web::middleware::from_fn(maintenance_guard))
    }

    #[actix_web::test]
    async fn passes_through_when_maintenance_off() {
        let state = make_test_state();
        let app = test::init_service(build(state)).await;
        let resp =
            test::call_service(&app, test::TestRequest::get().uri("/api/ping").to_request()).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn maintenance_on_blocks_api_with_503_json() {
        let state = make_test_state();
        state.maintenance_mode.store(true, Ordering::SeqCst);
        *state.maintenance_message.write().unwrap() = Some("upgrading".into());
        let app = test::init_service(build(state)).await;
        let resp =
            test::call_service(&app, test::TestRequest::get().uri("/api/ping").to_request()).await;
        assert_eq!(resp.status(), 503);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["maintenance"], true);
        assert_eq!(body["message"], "upgrading");
    }

    #[actix_web::test]
    async fn maintenance_on_blocks_pages_with_503_html() {
        let state = make_test_state();
        state.maintenance_mode.store(true, Ordering::SeqCst);
        let app = test::init_service(build(state)).await;
        let resp = test::call_service(
            &app,
            test::TestRequest::get().uri("/dashboard.html").to_request(),
        )
        .await;
        assert_eq!(resp.status(), 503);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "text/html; charset=utf-8"
        );
    }

    #[actix_web::test]
    async fn allowlist_paths_bypass_maintenance() {
        let state = make_test_state();
        state.maintenance_mode.store(true, Ordering::SeqCst);
        let app = test::init_service(build(state)).await;
        for path in &["/health", "/api/config"] {
            let resp =
                test::call_service(&app, test::TestRequest::get().uri(path).to_request()).await;
            assert_eq!(resp.status(), 200, "expected 200 for {path}");
        }
    }

    #[actix_web::test]
    async fn oauth2_routes_bypass_maintenance() {
        let state = make_test_state();
        state.maintenance_mode.store(true, Ordering::SeqCst);
        let app = test::init_service(build(state)).await;
        let resp = test::call_service(
            &app,
            test::TestRequest::get().uri("/oauth2/login").to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn webhook_routes_bypass_maintenance() {
        let state = make_test_state();
        state.maintenance_mode.store(true, Ordering::SeqCst);
        let app = test::init_service(build(state)).await;
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/webhooks/maintenance")
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn admin_with_valid_session_bypasses_maintenance() {
        let state = make_test_state();
        state.maintenance_mode.store(true, Ordering::SeqCst);
        let uid = insert_saas_user(&state, "admin", SUB_ADMIN, true);
        let token = make_saas_session(&state, uid);
        let app = test::init_service(build(state)).await;
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/ping")
                .insert_header(("Cookie", format!("{RUS_SESSION_COOKIE}={token}")))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn non_admin_session_blocked_by_maintenance() {
        let state = make_test_state();
        state.maintenance_mode.store(true, Ordering::SeqCst);
        let uid = insert_saas_user(&state, "alice", SUB_USER, false);
        let token = make_saas_session(&state, uid);
        let app = test::init_service(build(state)).await;
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/ping")
                .insert_header(("Cookie", format!("{RUS_SESSION_COOKIE}={token}")))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 503);
    }
}
