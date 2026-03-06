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

pub async fn serve_auth_js() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("application/javascript; charset=utf-8")
        .body(include_str!("../../static/auth.js")))
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
