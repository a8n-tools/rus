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
use crate::models::UpdateAccountRequest;
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

/// Returns the current SaaS user's profile (name, admin flag, alert opt-out).
pub async fn saas_me(state: web::Data<AppState>, user: AuthenticatedUser) -> Result<HttpResponse> {
    // RUS-15: the account needs to see whether its new-location alerts are on.
    let notify_new_location = {
        let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
        crate::location_alert::get_notify_new_location(&db, user.user_id)
            .ok()
            .flatten()
            .unwrap_or(true)
    };

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "username": user.username,
        "is_admin": user.is_admin,
        "notify_new_location": notify_new_location,
    })))
}

/// Account settings endpoint: turn this account's new-location sign-in alerts
/// on or off (RUS-15). The account is the session's, never an id from the body.
pub async fn saas_update_me(
    state: web::Data<AppState>,
    req: web::Json<UpdateAccountRequest>,
    user: AuthenticatedUser,
) -> Result<HttpResponse> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());

    // An absent key is "not submitted", so the stored value stands; an explicit
    // false is a real opt-out and persists.
    if let Some(enabled) = req.notify_new_location {
        if let Err(e) = crate::location_alert::set_notify_new_location(&db, user.user_id, enabled) {
            warn!(user_id = user.user_id, error = %e, "failed to update account settings");
            return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to update account"
            })));
        }
        debug!(
            user_id = user.user_id,
            notify_new_location = enabled,
            "account settings updated"
        );
    }

    match crate::location_alert::get_notify_new_location(&db, user.user_id) {
        Ok(Some(notify_new_location)) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "message": "Account updated successfully",
            "notify_new_location": notify_new_location,
        }))),
        Ok(None) => Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "User not found"
        }))),
        Err(e) => {
            warn!(user_id = user.user_id, error = %e, "failed to read account settings");
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to update account"
            })))
        }
    }
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

    // OIDC routes always pass through so users can finish auth flow. RUS-19's
    // approval page and API join them: an admin whose sign-in is held cannot
    // become an admin session without them, so blocking these would lock the
    // only account that can lift maintenance out of the app.
    if path.starts_with("/oauth2/")
        || path.starts_with("/webhooks/")
        || path.starts_with("/dev/")
        || path == crate::login_approval::APPROVAL_PAGE_PATH
        || path == crate::login_approval::APPROVAL_API_PATH
    {
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
    async fn approval_routes_survive_maintenance_mode() {
        let state = make_test_state();
        state.maintenance_mode.store(true, Ordering::SeqCst);
        let app = test::init_service(build(state).route(
            crate::login_approval::APPROVAL_PAGE_PATH,
            web::get().to(|| async { "approve" }),
        ))
        .await;

        let req = test::TestRequest::get()
            .uri(crate::login_approval::APPROVAL_PAGE_PATH)
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 200);
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

    // --- new-location alert opt-out (RUS-15) ---

    const SUB_OTHER: &str = "33333333-3333-3333-3333-333333333333";

    /// App exposing only the account-owned /api/me pair. The routes are wired
    /// bare: `AuthenticatedUser` resolves the session cookie itself.
    fn build_me(
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
            .route("/api/me", web::get().to(saas_me))
            .route("/api/me", web::patch().to(saas_update_me))
    }

    /// The account's stored preference, straight from the column the alert reads.
    fn stored_notify(state: &actix_web::web::Data<crate::db::AppState>, user_id: i64) -> bool {
        let db = state.db.lock().unwrap();
        crate::location_alert::get_notify_new_location(&db, user_id)
            .unwrap()
            .unwrap()
    }

    /// PATCH /api/me as the holder of `token`.
    async fn patch_me(
        app: &impl actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse,
            Error = actix_web::Error,
        >,
        token: &str,
        body: serde_json::Value,
    ) -> actix_web::dev::ServiceResponse {
        let req = test::TestRequest::patch()
            .uri("/api/me")
            .insert_header(("Cookie", format!("{RUS_SESSION_COOKIE}={token}")))
            .set_json(body)
            .to_request();
        test::call_service(app, req).await
    }

    /// GET /api/me as the holder of `token`.
    async fn get_me(
        app: &impl actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse,
            Error = actix_web::Error,
        >,
        token: &str,
    ) -> serde_json::Value {
        let req = test::TestRequest::get()
            .uri("/api/me")
            .insert_header(("Cookie", format!("{RUS_SESSION_COOKIE}={token}")))
            .to_request();
        test::call_and_read_body_json(app, req).await
    }

    #[actix_web::test]
    async fn saas_me_reports_the_stored_preference() {
        let state = make_test_state();
        let uid = insert_saas_user(&state, "alice", SUB_USER, false);
        let token = make_saas_session(&state, uid);
        let app = test::init_service(build_me(state.clone())).await;

        let me = get_me(&app, &token).await;
        assert_eq!(me["notify_new_location"], true, "alerts default to on");

        patch_me(
            &app,
            &token,
            serde_json::json!({"notify_new_location": false}),
        )
        .await;
        let me = get_me(&app, &token).await;
        assert_eq!(me["notify_new_location"], false);
    }

    // An explicit false is a real opt-out and has to reach the column; sending
    // true again turns the alerts back on.
    #[actix_web::test]
    async fn saas_patch_me_persists_both_directions() {
        let state = make_test_state();
        let uid = insert_saas_user(&state, "alice", SUB_USER, false);
        let token = make_saas_session(&state, uid);
        let app = test::init_service(build_me(state.clone())).await;

        let resp = patch_me(
            &app,
            &token,
            serde_json::json!({"notify_new_location": false}),
        )
        .await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["notify_new_location"], false);
        assert!(!stored_notify(&state, uid));

        let resp = patch_me(
            &app,
            &token,
            serde_json::json!({"notify_new_location": true}),
        )
        .await;
        assert_eq!(resp.status(), 200);
        assert!(stored_notify(&state, uid));
    }

    // A missing key means "not submitted", so the stored value stands rather
    // than silently reverting to the default.
    #[actix_web::test]
    async fn saas_patch_me_without_the_key_leaves_the_value_unchanged() {
        let state = make_test_state();
        let uid = insert_saas_user(&state, "alice", SUB_USER, false);
        let token = make_saas_session(&state, uid);
        let app = test::init_service(build_me(state.clone())).await;
        patch_me(
            &app,
            &token,
            serde_json::json!({"notify_new_location": false}),
        )
        .await;

        for body in [serde_json::json!({}), serde_json::json!({"unrelated": 1})] {
            let resp = patch_me(&app, &token, body).await;
            assert_eq!(resp.status(), 200);
            assert!(
                !stored_notify(&state, uid),
                "an absent key must not re-enable alerts"
            );
        }
    }

    // The type is validated, never coerced: a string is a bad request and
    // changes nothing.
    #[actix_web::test]
    async fn saas_patch_me_rejects_a_non_boolean() {
        let state = make_test_state();
        let uid = insert_saas_user(&state, "alice", SUB_USER, false);
        let token = make_saas_session(&state, uid);
        let app = test::init_service(build_me(state.clone())).await;

        let resp = patch_me(
            &app,
            &token,
            serde_json::json!({"notify_new_location": "false"}),
        )
        .await;
        assert_eq!(resp.status(), 400);
        assert!(stored_notify(&state, uid));
    }

    // The write targets the session's account. An id in the body is ignored, so
    // one account cannot flip another's setting.
    #[actix_web::test]
    async fn saas_patch_me_ignores_a_user_id_in_the_body() {
        let state = make_test_state();
        let alice = insert_saas_user(&state, "alice", SUB_USER, false);
        let bob = insert_saas_user(&state, "bob", SUB_OTHER, false);
        let token = make_saas_session(&state, alice);
        let app = test::init_service(build_me(state.clone())).await;

        let resp = patch_me(
            &app,
            &token,
            serde_json::json!({
                "notify_new_location": false,
                "user_id": bob,
                "userID": bob,
                "username": "bob",
            }),
        )
        .await;
        assert_eq!(resp.status(), 200);
        assert!(!stored_notify(&state, alice), "alice opted out");
        assert!(stored_notify(&state, bob), "bob is untouched");
    }

    #[actix_web::test]
    async fn saas_patch_me_without_a_session_returns_401() {
        let state = make_test_state();
        insert_saas_user(&state, "alice", SUB_USER, false);
        let app = test::init_service(build_me(state)).await;

        let req = test::TestRequest::patch()
            .uri("/api/me")
            .set_json(serde_json::json!({"notify_new_location": false}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }
}
