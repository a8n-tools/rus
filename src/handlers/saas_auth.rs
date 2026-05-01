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
        return Ok(req.into_response(
            HttpResponse::ServiceUnavailable().json(serde_json::json!({
                "error": "Service under maintenance",
                "maintenance": true,
                "message": message,
            })),
        ));
    }

    let html = include_str!("../../static/maintenance.html")
        .replace("{{MAINTENANCE_MESSAGE}}", &message);
    Ok(req.into_response(
        HttpResponse::ServiceUnavailable()
            .content_type("text/html; charset=utf-8")
            .body(html),
    ))
}
