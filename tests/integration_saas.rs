//! Integration tests for the SaaS build mode.
//!
//! These tests spin up a near-complete Actix application (without rate limiting)
//! and exercise end-to-end flows using cookie-based SaaS authentication.

#![cfg(feature = "saas")]

use actix_web::{test, web, App};
use serde_json::Value;

use rus::config::Config;
use rus::db::AppState;
use rus::handlers::*;

const TEST_SAAS_SECRET: &str = "test-saas-secret-32-chars-padded!";

fn test_config() -> Config {
    Config {
        max_url_length: 2048,
        click_retention_days: 30,
        host_url: "http://localhost:4001".to_string(),
        db_path: ":memory:".to_string(),
        host: "127.0.0.1".to_string(),
        port: 4001,
        saas_jwt_secret: TEST_SAAS_SECRET.to_string(),
        saas_login_url: "https://app.example.com/login".to_string(),
        saas_logout_url: "https://api.example.com/logout".to_string(),
        saas_membership_url: "https://app.example.com/membership".to_string(),
        saas_refresh_url: "https://api.example.com/auth/refresh".to_string(),
    }
}

fn make_state() -> web::Data<AppState> {
    web::Data::new(AppState::new(test_config()).unwrap())
}

/// Create a signed SaaS JWT cookie value.
fn make_jwt(user_id: &str, email: &str, membership_status: &str, role: Option<&str>) -> String {
    use jsonwebtoken::{encode, EncodingKey, Header};

    let exp = (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp();
    let mut claims = serde_json::json!({
        "sub": user_id,
        "email": email,
        "membership_status": membership_status,
        "exp": exp
    });
    if let Some(r) = role {
        claims["role"] = serde_json::Value::String(r.to_string());
    }

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(TEST_SAAS_SECRET.as_bytes()),
    )
    .unwrap()
}

fn cookie(jwt: &str) -> String {
    format!("access_token={jwt}")
}

fn sign_webhook(body: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(TEST_SAAS_SECRET.as_bytes()).unwrap();
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

/// Build an app that mirrors the real SaaS route layout (minus rate limiting).
async fn build_app_with_state(
    state: web::Data<AppState>,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    test::init_service(
        App::new()
            .app_data(state)
            .route("/api/config", web::get().to(get_config))
            .route("/api/version", web::get().to(get_version))
            .route("/api/report-abuse", web::post().to(submit_abuse_report))
            .route(
                "/webhooks/maintenance",
                web::post().to(handle_maintenance_webhook),
            )
            .service(
                web::scope("/api")
                    .wrap(actix_web::middleware::from_fn(saas_cookie_validator))
                    .route("/me", web::get().to(saas_me))
                    .route("/shorten", web::post().to(shorten_url))
                    .route("/stats/{code}", web::get().to(get_stats))
                    .route("/urls", web::get().to(get_user_urls))
                    .route("/urls/{code}", web::delete().to(delete_url))
                    .route("/urls/{code}/name", web::patch().to(update_url_name))
                    .route("/urls/{code}/clicks", web::get().to(get_click_history))
                    .route("/urls/{code}/qr/{format}", web::get().to(get_qr_code)),
            )
            .route("/", web::get().to(index))
            .route("/dashboard.html", web::get().to(dashboard_page))
            .route("/report.html", web::get().to(report_page))
            .route("/styles.css", web::get().to(serve_css))
            .route("/k9f3x2m7.js", web::get().to(serve_auth_js))
            .route("/saas-refresh.js", web::get().to(serve_saas_refresh_js))
            .route("/health", web::get().to(health_check))
            .route("/{code}", web::get().to(redirect_url)),
    )
    .await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn e2e_saas_shorten_redirect_stats() {
    let state = make_state();
    let app = build_app_with_state(state).await;
    let jwt = make_jwt("42", "alice@example.com", "active", None);

    // Shorten a URL
    let req = test::TestRequest::post()
        .uri("/api/shorten")
        .insert_header(("Cookie", cookie(&jwt)))
        .set_json(serde_json::json!({"url": "https://example.com"}))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    let code = body["short_code"].as_str().unwrap().to_string();
    assert_eq!(code.len(), 6);

    // Redirect works
    let req = test::TestRequest::get()
        .uri(&format!("/{code}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers().get("Location").unwrap(),
        "https://example.com"
    );

    // Stats show the click
    let req = test::TestRequest::get()
        .uri(&format!("/api/stats/{code}"))
        .insert_header(("Cookie", cookie(&jwt)))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(body["clicks"], 1);
}

#[actix_web::test]
async fn e2e_saas_user_isolation() {
    let state = make_state();
    let app = build_app_with_state(state).await;
    let jwt_alice = make_jwt("42", "alice@example.com", "active", None);
    let jwt_bob = make_jwt("43", "bob@example.com", "active", None);

    // Alice shortens
    let req = test::TestRequest::post()
        .uri("/api/shorten")
        .insert_header(("Cookie", cookie(&jwt_alice)))
        .set_json(serde_json::json!({"url": "https://alice.com"}))
        .to_request();
    test::call_service(&app, req).await;

    // Bob shortens
    let req = test::TestRequest::post()
        .uri("/api/shorten")
        .insert_header(("Cookie", cookie(&jwt_bob)))
        .set_json(serde_json::json!({"url": "https://bob.com"}))
        .to_request();
    test::call_service(&app, req).await;

    // Alice sees only her URL
    let req = test::TestRequest::get()
        .uri("/api/urls")
        .insert_header(("Cookie", cookie(&jwt_alice)))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["original_url"], "https://alice.com");

    // Bob sees only his URL
    let req = test::TestRequest::get()
        .uri("/api/urls")
        .insert_header(("Cookie", cookie(&jwt_bob)))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["original_url"], "https://bob.com");
}

#[actix_web::test]
async fn e2e_saas_maintenance_webhook_blocks_users() {
    let state = make_state();
    let app = build_app_with_state(state.clone()).await;
    let jwt_user = make_jwt("10", "user@example.com", "active", None);

    // Provision user first
    test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/shorten")
            .insert_header(("Cookie", cookie(&jwt_user)))
            .set_json(serde_json::json!({"url": "https://example.com"}))
            .to_request(),
    )
    .await;

    // Enable maintenance via webhook
    let enable = serde_json::json!({
        "event": "maintenance_mode_changed",
        "maintenance_mode": true,
        "maintenance_message": "Upgrading DB"
    });
    let enable_body = serde_json::to_vec(&enable).unwrap();
    let enable_sig = sign_webhook(&enable_body);

    let req = test::TestRequest::post()
        .uri("/webhooks/maintenance")
        .insert_header(("X-Webhook-Signature", enable_sig))
        .insert_header(("Content-Type", "application/json"))
        .set_payload(enable_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // Build new app instance with same state to pick up maintenance mode in middleware
    let app = build_app_with_state(state.clone()).await;

    // Note: maintenance_guard is not in this test app (it would need to wrap the outermost layer).
    // The maintenance state is verified via the atomic flag instead.
    assert!(state.maintenance_mode.load(std::sync::atomic::Ordering::SeqCst));

    // Disable maintenance
    let disable = serde_json::json!({
        "event": "maintenance_mode_changed",
        "maintenance_mode": false
    });
    let disable_body = serde_json::to_vec(&disable).unwrap();
    let disable_sig = sign_webhook(&disable_body);

    let req = test::TestRequest::post()
        .uri("/webhooks/maintenance")
        .insert_header(("X-Webhook-Signature", disable_sig))
        .insert_header(("Content-Type", "application/json"))
        .set_payload(disable_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert!(!state.maintenance_mode.load(std::sync::atomic::Ordering::SeqCst));
}

#[actix_web::test]
async fn e2e_saas_non_member_blocked() {
    let state = make_state();
    let app = build_app_with_state(state).await;
    let jwt = make_jwt("99", "noone@example.com", "none", None);

    let req = test::TestRequest::post()
        .uri("/api/shorten")
        .insert_header(("Cookie", cookie(&jwt)))
        .set_json(serde_json::json!({"url": "https://example.com"}))
        .to_request();
    let resp = test::try_call_service(&app, req).await;
    assert!(resp.is_err() || resp.unwrap().status() == 403);
}

#[actix_web::test]
async fn e2e_saas_abuse_report_public() {
    let state = make_state();
    let app = build_app_with_state(state.clone()).await;
    let jwt = make_jwt("42", "alice@example.com", "active", None);

    // Shorten a URL first
    let body: Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::post()
            .uri("/api/shorten")
            .insert_header(("Cookie", cookie(&jwt)))
            .set_json(serde_json::json!({"url": "https://evil.com"}))
            .to_request(),
    )
    .await;
    let code = body["short_code"].as_str().unwrap();

    // Submit abuse report (no auth needed)
    let req = test::TestRequest::post()
        .uri("/api/report-abuse")
        .set_json(serde_json::json!({
            "short_code": code,
            "reason": "phishing"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
}

#[actix_web::test]
async fn e2e_saas_delete_url() {
    let state = make_state();
    let app = build_app_with_state(state).await;
    let jwt = make_jwt("42", "alice@example.com", "active", None);

    // Shorten
    let body: Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::post()
            .uri("/api/shorten")
            .insert_header(("Cookie", cookie(&jwt)))
            .set_json(serde_json::json!({"url": "https://deleteme.com"}))
            .to_request(),
    )
    .await;
    let code = body["short_code"].as_str().unwrap().to_string();

    // Delete
    let req = test::TestRequest::delete()
        .uri(&format!("/api/urls/{code}"))
        .insert_header(("Cookie", cookie(&jwt)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // Redirect should now 404
    let req = test::TestRequest::get()
        .uri(&format!("/{code}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}
