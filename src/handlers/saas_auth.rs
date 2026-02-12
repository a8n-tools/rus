use actix_web::HttpRequest;
use base64::Engine;

/// SaaS user claims extracted from access_token cookie
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SaasUserClaims {
    pub user_id: i64,
    pub email: Option<String>,
    pub membership_status: Option<String>,
}

/// Extract user claims from access_token cookie (SaaS mode)
pub fn get_user_from_cookie(req: &HttpRequest) -> Option<SaasUserClaims> {
    let cookie = req.cookie("access_token")?;
    let parts: Vec<&str> = cookie.value().split('.').collect();

    if parts.len() != 3 {
        return None;
    }

    // Try URL-safe Base64 first, then standard
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(parts[1]))
        .ok()?;

    let payload: serde_json::Value = serde_json::from_slice(&bytes).ok()?;

    // Extract user_id from JWT payload
    // The parent app's JWT may have user_id as "sub", "user_id", or "id"
    let user_id = payload
        .get("user_id")
        .and_then(|v| v.as_i64())
        .or_else(|| payload.get("sub").and_then(|v| v.as_str()?.parse().ok()))
        .or_else(|| payload.get("id").and_then(|v| v.as_i64()))?;

    let email = payload.get("email").and_then(|v| v.as_str()).map(String::from);
    let membership_status = payload
        .get("membership_status")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Reject if membership is canceled
    if membership_status.as_deref() == Some("canceled") {
        return None;
    }

    Some(SaasUserClaims {
        user_id,
        email,
        membership_status,
    })
}
