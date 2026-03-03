use actix_web::{HttpMessage, HttpRequest};
use jsonwebtoken::{decode, DecodingKey, Validation};

/// SaaS user claims extracted from access_token cookie
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SaasUserClaims {
    pub user_id: i64,
    pub email: Option<String>,
    pub membership_status: Option<String>,
}

/// Extract and verify user claims from access_token cookie (SaaS mode)
pub fn get_user_from_cookie(req: &HttpRequest, secret: &str) -> Option<SaasUserClaims> {
    let cookie = req.cookie("access_token")?;
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

    let token_data = decode::<serde_json::Value>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .ok()?;

    let payload = token_data.claims;

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
            req.extensions_mut().insert(claims);
            next.call(req).await
        }
        None => Err(actix_web::error::ErrorUnauthorized("Invalid or missing authentication")),
    }
}
