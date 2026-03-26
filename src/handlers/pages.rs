use actix_web::{web, HttpResponse, Result};
#[cfg(feature = "saas")]
use actix_web::HttpRequest;

use crate::db::AppState;
#[cfg(feature = "standalone")]
use crate::models::SetupCheckResponse;
use crate::models::{ConfigResponse, HealthResponse, VersionResponse};
#[cfg(feature = "saas")]
use super::saas_auth::get_user_from_cookie;

/// Serve static HTML pages
pub async fn index() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/index.html")))
}

#[cfg(feature = "standalone")]
pub async fn login_page() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/login.html")))
}

#[cfg(feature = "standalone")]
pub async fn signup_page() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/signup.html")))
}

/// Dashboard page for standalone mode - no cookie check needed
#[cfg(feature = "standalone")]
pub async fn dashboard_page() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/dashboard.html")))
}

/// Dashboard page for SaaS mode - verifies access_token cookie signature
#[cfg(feature = "saas")]
pub async fn dashboard_page(req: HttpRequest, data: web::Data<AppState>) -> Result<HttpResponse> {
    // Verify JWT signature and check claims
    if get_user_from_cookie(&req, &data.config.saas_jwt_secret).is_none() {
        let return_to = format!("{}/dashboard.html", data.config.host_url);
        let redirect = url::Url::parse_with_params(
            &data.config.saas_login_url,
            &[("redirect", return_to.as_str())],
        )
        .unwrap_or_else(|_| url::Url::parse(&data.config.saas_login_url).unwrap());
        return Ok(HttpResponse::Found()
            .append_header(("Location", redirect.to_string()))
            .finish());
    }

    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/dashboard.html")))
}

#[cfg(feature = "standalone")]
pub async fn setup_page() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/setup.html")))
}

#[cfg(feature = "standalone")]
pub async fn admin_page() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/admin.html")))
}

pub async fn report_page() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/report.html")))
}

pub async fn serve_css() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/css; charset=utf-8")
        .body(include_str!("../../static/styles.css")))
}

pub async fn serve_theme_js() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("application/javascript; charset=utf-8")
        .body(include_str!("../../static/theme.js")))
}

pub async fn serve_auth_js() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("application/javascript; charset=utf-8")
        .body(include_str!("../../static/auth.js")))
}

#[cfg(feature = "saas")]
pub async fn serve_saas_refresh_js() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("application/javascript; charset=utf-8")
        .body(include_str!("../../static/saas-refresh.js")))
}

/// Health check endpoint for monitoring and Docker health checks
pub async fn health_check(data: web::Data<AppState>) -> Result<HttpResponse> {
    let uptime = data.start_time.elapsed().as_secs();

    Ok(HttpResponse::Ok().json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: uptime,
    }))
}

/// Public config endpoint for frontend
pub async fn get_config(data: web::Data<AppState>) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(ConfigResponse {
        host_url: data.config.host_url.clone(),
        max_url_length: data.config.max_url_length,
        #[cfg(feature = "standalone")]
        auth_mode: "standalone".to_string(),
        #[cfg(feature = "saas")]
        auth_mode: "saas".to_string(),
        #[cfg(feature = "standalone")]
        allow_registration: data.config.allow_registration,
        #[cfg(feature = "saas")]
        login_url: data.config.saas_login_url.clone(),
        #[cfg(feature = "saas")]
        logout_url: data.config.saas_logout_url.clone(),
        #[cfg(feature = "saas")]
        refresh_url: data.config.saas_refresh_url.clone(),
        #[cfg(feature = "saas")]
        maintenance_mode: data.maintenance_mode.load(std::sync::atomic::Ordering::SeqCst),
        #[cfg(feature = "saas")]
        maintenance_message: data.maintenance_message.read().unwrap().clone(),
    }))
}

/// Check if initial setup is required (no users exist) - standalone only
#[cfg(feature = "standalone")]
pub async fn check_setup_required(data: web::Data<AppState>) -> Result<HttpResponse> {
    let db = data.db.lock().unwrap_or_else(|e| e.into_inner());

    let user_count: i64 = db
        .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
        .unwrap_or(0);

    Ok(HttpResponse::Ok().json(SetupCheckResponse {
        setup_required: user_count == 0,
    }))
}

/// Get application version
pub async fn get_version() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(VersionResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};
    use serde_json::Value;

    macro_rules! setup_app {
        () => {{
            let state = crate::testing::make_test_state();
            test::init_service(
                App::new()
                    .app_data(state.clone())
                    .route("/", web::get().to(index))
                    .route("/report.html", web::get().to(report_page))
                    .route("/styles.css", web::get().to(serve_css))
                    .route("/k9f3x2m7.js", web::get().to(serve_auth_js))
                    .route("/health", web::get().to(health_check))
                    .route("/api/config", web::get().to(get_config))
                    .route("/api/version", web::get().to(get_version)),
            )
            .await
        }};
    }

    // --- health_check ---

    #[actix_web::test]
    async fn health_check_returns_200() {
        let app = setup_app!();
        let req = test::TestRequest::get().uri("/health").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn health_check_returns_healthy_status() {
        let app = setup_app!();
        let req = test::TestRequest::get().uri("/health").to_request();
        let body: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(body["status"], "healthy");
        assert!(body["version"].is_string());
        assert!(body["uptime_seconds"].is_number());
    }

    // --- get_config ---

    #[actix_web::test]
    async fn get_config_returns_200() {
        let app = setup_app!();
        let req = test::TestRequest::get().uri("/api/config").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn get_config_returns_expected_fields() {
        let app = setup_app!();
        let req = test::TestRequest::get().uri("/api/config").to_request();
        let body: Value = test::call_and_read_body_json(&app, req).await;
        assert!(body["host_url"].is_string());
        assert!(body["max_url_length"].is_number());
        assert!(body["auth_mode"].is_string());
    }

    #[cfg(feature = "standalone")]
    #[actix_web::test]
    async fn get_config_standalone_auth_mode() {
        let app = setup_app!();
        let req = test::TestRequest::get().uri("/api/config").to_request();
        let body: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(body["auth_mode"], "standalone");
        assert!(body.get("allow_registration").is_some());
    }

    #[cfg(feature = "saas")]
    #[actix_web::test]
    async fn get_config_saas_auth_mode() {
        let app = setup_app!();
        let req = test::TestRequest::get().uri("/api/config").to_request();
        let body: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(body["auth_mode"], "saas");
        assert!(body.get("login_url").is_some());
        assert!(body.get("logout_url").is_some());
    }

    // --- get_version ---

    #[actix_web::test]
    async fn get_version_returns_version_string() {
        let app = setup_app!();
        let req = test::TestRequest::get().uri("/api/version").to_request();
        let body: Value = test::call_and_read_body_json(&app, req).await;
        assert!(body["version"].is_string());
        assert!(!body["version"].as_str().unwrap().is_empty());
    }

    // --- static page handlers ---

    #[actix_web::test]
    async fn index_returns_200_with_html() {
        let app = setup_app!();
        let req = test::TestRequest::get().uri("/").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "text/html; charset=utf-8"
        );
    }

    #[actix_web::test]
    async fn index_body_contains_html() {
        let app = setup_app!();
        let req = test::TestRequest::get().uri("/").to_request();
        let body = test::call_and_read_body(&app, req).await;
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        assert!(body_str.contains("<html") || body_str.contains("<!DOCTYPE") || body_str.contains("<!doctype"));
    }

    #[actix_web::test]
    async fn report_page_returns_200_with_html() {
        let app = setup_app!();
        let req = test::TestRequest::get().uri("/report.html").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "text/html; charset=utf-8"
        );
    }

    #[actix_web::test]
    async fn serve_css_returns_200_with_css_content_type() {
        let app = setup_app!();
        let req = test::TestRequest::get().uri("/styles.css").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "text/css; charset=utf-8"
        );
    }

    #[actix_web::test]
    async fn serve_css_body_is_not_empty() {
        let app = setup_app!();
        let req = test::TestRequest::get().uri("/styles.css").to_request();
        let body = test::call_and_read_body(&app, req).await;
        assert!(!body.is_empty());
    }

    #[actix_web::test]
    async fn serve_auth_js_returns_200_with_js_content_type() {
        let app = setup_app!();
        let req = test::TestRequest::get().uri("/k9f3x2m7.js").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "application/javascript; charset=utf-8"
        );
    }

    #[actix_web::test]
    async fn serve_auth_js_body_is_not_empty() {
        let app = setup_app!();
        let req = test::TestRequest::get().uri("/k9f3x2m7.js").to_request();
        let body = test::call_and_read_body(&app, req).await;
        assert!(!body.is_empty());
    }

    // --- standalone-only page handlers ---

    #[cfg(feature = "standalone")]
    mod standalone {
        use super::*;

        macro_rules! setup_standalone_app {
            () => {{
                let state = crate::testing::make_test_state();
                test::init_service(
                    App::new()
                        .app_data(state.clone())
                        .route("/login.html", web::get().to(login_page))
                        .route("/signup.html", web::get().to(signup_page))
                        .route("/dashboard.html", web::get().to(dashboard_page))
                        .route("/setup.html", web::get().to(setup_page))
                        .route("/admin.html", web::get().to(admin_page))
                        .route("/api/setup/required", web::get().to(check_setup_required)),
                )
                .await
            }};
        }

        #[actix_web::test]
        async fn login_page_returns_200_html() {
            let app = setup_standalone_app!();
            let req = test::TestRequest::get().uri("/login.html").to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200);
            assert_eq!(
                resp.headers().get("content-type").unwrap(),
                "text/html; charset=utf-8"
            );
        }

        #[actix_web::test]
        async fn signup_page_returns_200_html() {
            let app = setup_standalone_app!();
            let req = test::TestRequest::get().uri("/signup.html").to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200);
            assert_eq!(
                resp.headers().get("content-type").unwrap(),
                "text/html; charset=utf-8"
            );
        }

        #[actix_web::test]
        async fn dashboard_page_returns_200_html() {
            let app = setup_standalone_app!();
            let req = test::TestRequest::get().uri("/dashboard.html").to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200);
            assert_eq!(
                resp.headers().get("content-type").unwrap(),
                "text/html; charset=utf-8"
            );
        }

        #[actix_web::test]
        async fn setup_page_returns_200_html() {
            let app = setup_standalone_app!();
            let req = test::TestRequest::get().uri("/setup.html").to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200);
            assert_eq!(
                resp.headers().get("content-type").unwrap(),
                "text/html; charset=utf-8"
            );
        }

        #[actix_web::test]
        async fn admin_page_returns_200_html() {
            let app = setup_standalone_app!();
            let req = test::TestRequest::get().uri("/admin.html").to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200);
            assert_eq!(
                resp.headers().get("content-type").unwrap(),
                "text/html; charset=utf-8"
            );
        }

        #[actix_web::test]
        async fn check_setup_required_true_when_no_users() {
            let app = setup_standalone_app!();
            let req = test::TestRequest::get().uri("/api/setup/required").to_request();
            let body: Value = test::call_and_read_body_json(&app, req).await;
            assert_eq!(body["setup_required"], true);
        }

        #[actix_web::test]
        async fn check_setup_required_false_when_users_exist() {
            let state = crate::testing::make_test_state();
            crate::testing::insert_test_user(&state, "admin", true);
            let app = test::init_service(
                App::new()
                    .app_data(state)
                    .route("/api/setup/required", web::get().to(check_setup_required)),
            )
            .await;

            let req = test::TestRequest::get().uri("/api/setup/required").to_request();
            let body: Value = test::call_and_read_body_json(&app, req).await;
            assert_eq!(body["setup_required"], false);
        }
    }

    // --- saas-only page handlers ---

    #[cfg(feature = "saas")]
    mod saas {
        use super::*;

        #[actix_web::test]
        async fn dashboard_redirects_without_valid_cookie() {
            let state = crate::testing::make_test_state();
            let app = test::init_service(
                App::new()
                    .app_data(state)
                    .route("/dashboard.html", web::get().to(dashboard_page)),
            )
            .await;

            let req = test::TestRequest::get().uri("/dashboard.html").to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 302);
            let location = resp.headers().get("Location").unwrap().to_str().unwrap();
            assert!(location.contains("login") || location.contains("app.example.com"));
        }

        #[actix_web::test]
        async fn dashboard_returns_html_with_valid_cookie() {
            let state = crate::testing::make_test_state();
            let jwt = crate::testing::make_saas_jwt("42", "alice@example.com", "active", None);
            let app = test::init_service(
                App::new()
                    .app_data(state)
                    .route("/dashboard.html", web::get().to(dashboard_page)),
            )
            .await;

            let req = test::TestRequest::get()
                .uri("/dashboard.html")
                .insert_header(("Cookie", format!("access_token={jwt}")))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200);
            assert_eq!(
                resp.headers().get("content-type").unwrap(),
                "text/html; charset=utf-8"
            );
        }

        #[actix_web::test]
        async fn serve_saas_refresh_js_returns_200() {
            let state = crate::testing::make_test_state();
            let app = test::init_service(
                App::new()
                    .app_data(state)
                    .route("/saas-refresh.js", web::get().to(serve_saas_refresh_js)),
            )
            .await;

            let req = test::TestRequest::get().uri("/saas-refresh.js").to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200);
            assert_eq!(
                resp.headers().get("content-type").unwrap(),
                "application/javascript; charset=utf-8"
            );
        }
    }
}
