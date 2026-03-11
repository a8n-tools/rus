use actix_web::{HttpMessage, HttpRequest};
use jsonwebtoken::{decode, DecodingKey, Validation};

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
            eprintln!("saas_auth: no access_token cookie found");
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
            eprintln!("saas_auth: JWT decode failed: {e}");
            return None;
        }
    };

    let payload = token_data.claims;
    eprintln!("saas_auth: JWT decoded successfully, payload: {payload}");

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
        Some(id) => eprintln!("saas_auth: extracted user_id: {id}"),
        None => {
            eprintln!("saas_auth: could not extract user_id from payload");
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

    // Reject if membership is canceled
    if membership_status.as_deref() == Some("canceled") {
        eprintln!("saas_auth: membership canceled, rejecting");
        return None;
    }

    eprintln!("saas_auth: authentication successful, user_id={user_id}, email={email:?}, membership={membership_status:?}, is_admin={is_admin}");
    Some(SaasUserClaims {
        user_id,
        email,
        membership_status,
        is_admin,
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
                    eprintln!("saas_auth: failed to provision user: {e}");
                    actix_web::error::ErrorInternalServerError("Failed to provision user")
                })?;
            }

            req.extensions_mut().insert(claims);
            next.call(req).await
        }
        None => Err(actix_web::error::ErrorUnauthorized("Invalid or missing authentication")),
    }
}
