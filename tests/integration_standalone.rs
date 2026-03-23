//! Integration tests for the standalone build mode.
//!
//! These tests spin up a near-complete Actix application (without rate limiting)
//! and exercise end-to-end flows: registration → login → shorten → redirect → stats.

#![cfg(feature = "standalone")]

use actix_web::{test, web, App};
use actix_web_httpauth::middleware::HttpAuthentication;
use serde_json::Value;

// We import from the `rus` library crate.
use rus::auth::middleware::{admin_validator, jwt_validator};
use rus::config::Config;
use rus::db::AppState;
use rus::handlers::*;

const TEST_PASSWORD: &str = "TestPass1!";

fn test_config() -> Config {
    Config {
        max_url_length: 2048,
        click_retention_days: 30,
        host_url: "http://localhost:4001".to_string(),
        db_path: ":memory:".to_string(),
        host: "127.0.0.1".to_string(),
        port: 4001,
        jwt_secret: "test-secret-at-least-32-chars-ok!".to_string(),
        jwt_expiry_hours: 1,
        refresh_token_expiry_days: 7,
        account_lockout_attempts: 5,
        account_lockout_duration_minutes: 30,
        allow_registration: true,
    }
}

fn make_state() -> web::Data<AppState> {
    web::Data::new(AppState::new(test_config()).unwrap())
}

/// Build an app that mirrors the real standalone route layout (minus rate limiting).
async fn build_app() -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    let state = make_state();
    build_app_with_state(state).await
}

async fn build_app_with_state(
    state: web::Data<AppState>,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    let auth = HttpAuthentication::bearer(jwt_validator);
    let admin_auth = HttpAuthentication::bearer(admin_validator);

    test::init_service(
        App::new()
            .app_data(state)
            .route("/api/register", web::post().to(register))
            .route("/api/login", web::post().to(login))
            .route("/api/refresh", web::post().to(refresh_token))
            .route("/api/config", web::get().to(get_config))
            .route("/api/version", web::get().to(get_version))
            .route("/api/setup/required", web::get().to(check_setup_required))
            .route("/api/report-abuse", web::post().to(submit_abuse_report))
            .service(
                web::scope("/api/admin")
                    .wrap(admin_auth)
                    .route("/users", web::get().to(admin_list_users))
                    .route("/users/{user_id}", web::delete().to(admin_delete_user))
                    .route("/users/{user_id}/promote", web::post().to(admin_promote_user))
                    .route("/stats", web::get().to(admin_get_stats))
                    .route("/reports", web::get().to(admin_list_reports))
                    .route("/reports/{report_id}", web::post().to(admin_resolve_report)),
            )
            .service(
                web::scope("/api")
                    .wrap(auth)
                    .route("/me", web::get().to(get_current_user))
                    .route("/shorten", web::post().to(shorten_url))
                    .route("/stats/{code}", web::get().to(get_stats))
                    .route("/urls", web::get().to(get_user_urls))
                    .route("/urls/{code}", web::delete().to(delete_url))
                    .route("/urls/{code}/name", web::patch().to(update_url_name))
                    .route("/urls/{code}/clicks", web::get().to(get_click_history))
                    .route("/urls/{code}/qr/{format}", web::get().to(get_qr_code)),
            )
            .route("/", web::get().to(index))
            .route("/login.html", web::get().to(login_page))
            .route("/signup.html", web::get().to(signup_page))
            .route("/dashboard.html", web::get().to(dashboard_page))
            .route("/setup.html", web::get().to(setup_page))
            .route("/admin.html", web::get().to(admin_page))
            .route("/report.html", web::get().to(report_page))
            .route("/styles.css", web::get().to(serve_css))
            .route("/k9f3x2m7.js", web::get().to(serve_auth_js))
            .route("/health", web::get().to(health_check))
            .route("/{code}", web::get().to(redirect_url)),
    )
    .await
}

/// Helper: register a user, return the full JSON body.
async fn do_register(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    username: &str,
) -> Value {
    let req = test::TestRequest::post()
        .uri("/api/register")
        .set_json(serde_json::json!({"username": username, "password": TEST_PASSWORD}))
        .to_request();
    test::call_and_read_body_json(app, req).await
}

/// Helper: login a user, return the full JSON body.
async fn do_login(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    username: &str,
) -> Value {
    let req = test::TestRequest::post()
        .uri("/api/login")
        .set_json(serde_json::json!({"username": username, "password": TEST_PASSWORD}))
        .to_request();
    test::call_and_read_body_json(app, req).await
}

/// Helper: shorten a URL, return the full JSON body.
async fn do_shorten(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: &str,
    url: &str,
) -> Value {
    let req = test::TestRequest::post()
        .uri("/api/shorten")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(serde_json::json!({"url": url}))
        .to_request();
    test::call_and_read_body_json(app, req).await
}

// =============================================================================
// End-to-end flows
// =============================================================================

#[actix_web::test]
async fn e2e_register_login_shorten_redirect() {
    let app = build_app().await;

    // Register
    let reg = do_register(&app, "alice").await;
    assert!(reg["token"].is_string());

    // Login
    let login = do_login(&app, "alice").await;
    let token = login["token"].as_str().unwrap();

    // Shorten
    let shortened = do_shorten(&app, token, "https://example.com").await;
    let code = shortened["short_code"].as_str().unwrap();
    assert_eq!(code.len(), 6);

    // Redirect
    let req = test::TestRequest::get()
        .uri(&format!("/{code}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers().get("Location").unwrap(),
        "https://example.com"
    );
}

#[actix_web::test]
async fn e2e_shorten_then_stats_then_click_history() {
    let app = build_app().await;
    let reg = do_register(&app, "alice").await;
    let token = reg["token"].as_str().unwrap();

    let shortened = do_shorten(&app, token, "https://example.com/stats-test").await;
    let code = shortened["short_code"].as_str().unwrap();

    // Click 3 times
    for _ in 0..3 {
        test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/{code}"))
                .to_request(),
        )
        .await;
    }

    // Stats
    let req = test::TestRequest::get()
        .uri(&format!("/api/stats/{code}"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let stats: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(stats["clicks"], 3);

    // Click history
    let req = test::TestRequest::get()
        .uri(&format!("/api/urls/{code}/clicks"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let history: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(history["total_clicks"], 3);
    assert_eq!(history["history"].as_array().unwrap().len(), 3);
}

#[actix_web::test]
async fn e2e_shorten_rename_list_delete() {
    let app = build_app().await;
    let reg = do_register(&app, "alice").await;
    let token = reg["token"].as_str().unwrap();

    // Shorten
    let shortened = do_shorten(&app, token, "https://example.com/lifecycle").await;
    let code = shortened["short_code"].as_str().unwrap();

    // Rename
    let req = test::TestRequest::patch()
        .uri(&format!("/api/urls/{code}/name"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(serde_json::json!({"name": "My Link"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // List and verify name
    let req = test::TestRequest::get()
        .uri("/api/urls")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let urls: Value = test::call_and_read_body_json(&app, req).await;
    let arr = urls.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "My Link");

    // Delete
    let req = test::TestRequest::delete()
        .uri(&format!("/api/urls/{code}"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // Verify gone
    let req = test::TestRequest::get()
        .uri("/api/urls")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let urls: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(urls.as_array().unwrap().len(), 0);

    // Redirect now 404
    let req = test::TestRequest::get()
        .uri(&format!("/{code}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn e2e_qr_code_generation() {
    let app = build_app().await;
    let reg = do_register(&app, "alice").await;
    let token = reg["token"].as_str().unwrap();

    let shortened = do_shorten(&app, token, "https://example.com/qr").await;
    let code = shortened["short_code"].as_str().unwrap();

    // PNG
    let req = test::TestRequest::get()
        .uri(&format!("/api/urls/{code}/qr/png"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("content-type").unwrap(), "image/png");

    // SVG
    let req = test::TestRequest::get()
        .uri(&format!("/api/urls/{code}/qr/svg"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("content-type").unwrap(), "image/svg+xml");
}

// =============================================================================
// Multi-user isolation
// =============================================================================

#[actix_web::test]
async fn e2e_users_cannot_see_each_others_urls() {
    let app = build_app().await;

    let alice = do_register(&app, "alice").await;
    let alice_token = alice["token"].as_str().unwrap();

    let bob = do_register(&app, "bob").await;
    let bob_token = bob["token"].as_str().unwrap();

    // Alice shortens
    let shortened = do_shorten(&app, alice_token, "https://alice-secret.com").await;
    let code = shortened["short_code"].as_str().unwrap();

    // Bob cannot see Alice's stats
    let req = test::TestRequest::get()
        .uri(&format!("/api/stats/{code}"))
        .insert_header(("Authorization", format!("Bearer {bob_token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);

    // Bob cannot delete Alice's URL
    let req = test::TestRequest::delete()
        .uri(&format!("/api/urls/{code}"))
        .insert_header(("Authorization", format!("Bearer {bob_token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);

    // Bob's URL list is empty
    let req = test::TestRequest::get()
        .uri("/api/urls")
        .insert_header(("Authorization", format!("Bearer {bob_token}")))
        .to_request();
    let urls: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(urls.as_array().unwrap().len(), 0);
}

// =============================================================================
// Admin flows
// =============================================================================

#[actix_web::test]
async fn e2e_admin_list_users_and_stats() {
    let app = build_app().await;

    // First user is admin
    let admin = do_register(&app, "admin").await;
    let admin_token = admin["token"].as_str().unwrap();

    // Register a second user
    do_register(&app, "bob").await;

    // Admin stats
    let req = test::TestRequest::get()
        .uri("/api/admin/stats")
        .insert_header(("Authorization", format!("Bearer {admin_token}")))
        .to_request();
    let stats: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(stats["total_users"], 2);

    // Admin list users
    let req = test::TestRequest::get()
        .uri("/api/admin/users")
        .insert_header(("Authorization", format!("Bearer {admin_token}")))
        .to_request();
    let users: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(users.as_array().unwrap().len(), 2);
}

#[actix_web::test]
async fn e2e_admin_promote_user() {
    let app = build_app().await;

    let admin = do_register(&app, "admin").await;
    let admin_token = admin["token"].as_str().unwrap();

    // Get bob's user_id via /api/me
    let bob_reg = do_register(&app, "bob").await;
    let bob_token = bob_reg["token"].as_str().unwrap();
    let req = test::TestRequest::get()
        .uri("/api/me")
        .insert_header(("Authorization", format!("Bearer {bob_token}")))
        .to_request();
    let me: Value = test::call_and_read_body_json(&app, req).await;
    let bob_id = me["user_id"].as_i64().unwrap();

    // Promote bob
    let req = test::TestRequest::post()
        .uri(&format!("/api/admin/users/{bob_id}/promote"))
        .insert_header(("Authorization", format!("Bearer {admin_token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn e2e_admin_delete_user_cascades_urls() {
    let app = build_app().await;

    let admin = do_register(&app, "admin").await;
    let admin_token = admin["token"].as_str().unwrap();

    let bob = do_register(&app, "bob").await;
    let bob_token = bob["token"].as_str().unwrap();

    // Bob shortens a URL
    let shortened = do_shorten(&app, bob_token, "https://bob.com").await;
    let code = shortened["short_code"].as_str().unwrap().to_string();

    // Get Bob's user_id
    let req = test::TestRequest::get()
        .uri("/api/me")
        .insert_header(("Authorization", format!("Bearer {bob_token}")))
        .to_request();
    let me: Value = test::call_and_read_body_json(&app, req).await;
    let bob_id = me["user_id"].as_i64().unwrap();

    // Admin deletes Bob
    let req = test::TestRequest::delete()
        .uri(&format!("/api/admin/users/{bob_id}"))
        .insert_header(("Authorization", format!("Bearer {admin_token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // Bob's URL is now gone (redirect returns 404)
    let req = test::TestRequest::get()
        .uri(&format!("/{code}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn e2e_non_admin_cannot_access_admin_routes() {
    let app = build_app().await;

    // First user is admin, second is not
    do_register(&app, "admin").await;
    let bob = do_register(&app, "bob").await;
    let bob_token = bob["token"].as_str().unwrap();

    let req = test::TestRequest::get()
        .uri("/api/admin/users")
        .insert_header(("Authorization", format!("Bearer {bob_token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::get()
        .uri("/api/admin/stats")
        .insert_header(("Authorization", format!("Bearer {bob_token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

// =============================================================================
// Token refresh flow
// =============================================================================

#[actix_web::test]
async fn e2e_refresh_token_provides_working_access_token() {
    let app = build_app().await;
    let reg = do_register(&app, "alice").await;
    let refresh = reg["refresh_token"].as_str().unwrap();

    // Refresh
    let req = test::TestRequest::post()
        .uri("/api/refresh")
        .set_json(serde_json::json!({"refresh_token": refresh}))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    let new_token = body["token"].as_str().unwrap();

    // Use new token to access protected route
    let req = test::TestRequest::get()
        .uri("/api/me")
        .insert_header(("Authorization", format!("Bearer {new_token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

// =============================================================================
// Setup check
// =============================================================================

#[actix_web::test]
async fn e2e_setup_required_then_not_after_register() {
    let app = build_app().await;

    // Before any registration
    let req = test::TestRequest::get()
        .uri("/api/setup/required")
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(body["setup_required"], true);

    // Register first user
    do_register(&app, "admin").await;

    // After registration
    let req = test::TestRequest::get()
        .uri("/api/setup/required")
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(body["setup_required"], false);
}

// =============================================================================
// Abuse reporting
// =============================================================================

#[actix_web::test]
async fn e2e_abuse_report_and_admin_resolution() {
    let app = build_app().await;

    let admin = do_register(&app, "admin").await;
    let admin_token = admin["token"].as_str().unwrap();

    // Shorten a URL
    let shortened = do_shorten(&app, admin_token, "https://bad-site.com").await;
    let code = shortened["short_code"].as_str().unwrap();

    // Submit abuse report (public)
    let req = test::TestRequest::post()
        .uri("/api/report-abuse")
        .set_json(serde_json::json!({
            "short_code": code,
            "reason": "spam",
            "reporter_email": "reporter@example.com",
            "description": "This is spam"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    // Admin lists reports
    let req = test::TestRequest::get()
        .uri("/api/admin/reports")
        .insert_header(("Authorization", format!("Bearer {admin_token}")))
        .to_request();
    let reports: Value = test::call_and_read_body_json(&app, req).await;
    let arr = reports.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    let report_id = arr[0]["id"].as_i64().unwrap();

    // Resolve (dismiss)
    let req = test::TestRequest::post()
        .uri(&format!("/api/admin/reports/{report_id}"))
        .insert_header(("Authorization", format!("Bearer {admin_token}")))
        .set_json(serde_json::json!({"action": "dismiss"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

// =============================================================================
// Public endpoints (no auth required)
// =============================================================================

#[actix_web::test]
async fn e2e_public_endpoints_accessible() {
    let app = build_app().await;

    // Health
    let req = test::TestRequest::get().uri("/health").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // Config
    let req = test::TestRequest::get().uri("/api/config").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // Version
    let req = test::TestRequest::get().uri("/api/version").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // Index page
    let req = test::TestRequest::get().uri("/").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // Static assets
    let req = test::TestRequest::get().uri("/styles.css").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get().uri("/k9f3x2m7.js").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

// =============================================================================
// Registration disabled
// =============================================================================

#[actix_web::test]
async fn e2e_registration_disabled_after_first_user() {
    let mut config = test_config();
    config.allow_registration = false;
    let state = web::Data::new(AppState::new(config).unwrap());
    let app = build_app_with_state(state).await;

    // First user always allowed (bootstrap)
    let req = test::TestRequest::post()
        .uri("/api/register")
        .set_json(serde_json::json!({"username": "admin", "password": TEST_PASSWORD}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    // Second user blocked
    let req = test::TestRequest::post()
        .uri("/api/register")
        .set_json(serde_json::json!({"username": "bob", "password": TEST_PASSWORD}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

// =============================================================================
// Duplicate URL returns same short code
// =============================================================================

#[actix_web::test]
async fn e2e_duplicate_url_returns_same_code() {
    let app = build_app().await;
    let reg = do_register(&app, "alice").await;
    let token = reg["token"].as_str().unwrap();

    let first = do_shorten(&app, token, "https://example.com/dup").await;
    let second = do_shorten(&app, token, "https://example.com/dup").await;
    assert_eq!(first["short_code"], second["short_code"]);
}

// =============================================================================
// Abuse report → ban user
// =============================================================================

#[actix_web::test]
async fn e2e_abuse_report_ban_user() {
    let app = build_app().await;

    // Admin registers (first user)
    let admin = do_register(&app, "admin").await;
    let admin_token = admin["token"].as_str().unwrap();

    // Bad user registers
    let bad_user = do_register(&app, "badguy").await;
    let bad_token = bad_user["token"].as_str().unwrap();

    // Bad user shortens a URL
    let shortened = do_shorten(&app, bad_token, "https://evil.example.com").await;
    let code = shortened["short_code"].as_str().unwrap();

    // Abuse report is filed
    let req = test::TestRequest::post()
        .uri("/api/report-abuse")
        .set_json(serde_json::json!({
            "short_code": code,
            "reason": "malware distribution"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    // Admin lists reports and gets the ID
    let req = test::TestRequest::get()
        .uri("/api/admin/reports")
        .insert_header(("Authorization", format!("Bearer {admin_token}")))
        .to_request();
    let reports: Value = test::call_and_read_body_json(&app, req).await;
    let report_id = reports[0]["id"].as_i64().unwrap();

    // Admin bans the user via abuse report
    let req = test::TestRequest::post()
        .uri(&format!("/api/admin/reports/{report_id}"))
        .insert_header(("Authorization", format!("Bearer {admin_token}")))
        .set_json(serde_json::json!({"action": "ban_user"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // The URL should now 404
    let req = test::TestRequest::get()
        .uri(&format!("/{code}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);

    // The banned user cannot login
    let req = test::TestRequest::post()
        .uri("/api/login")
        .set_json(serde_json::json!({"username": "badguy", "password": TEST_PASSWORD}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}
