use actix_web::{web, HttpMessage};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use crate::config::Config;
use crate::db::AppState;
use crate::auth::jwt::decode_jwt;

/// JWT validator middleware for protected routes
pub async fn jwt_validator(
    req: actix_web::dev::ServiceRequest,
    credentials: BearerAuth,
) -> Result<actix_web::dev::ServiceRequest, (actix_web::Error, actix_web::dev::ServiceRequest)> {
    let token = credentials.token();

    // Get the secret from app state
    let secret = req
        .app_data::<web::Data<AppState>>()
        .map(|state| state.config.jwt_secret.clone())
        .unwrap_or_else(Config::get_jwt_secret);

    match decode_jwt(token, &secret) {
        Ok(claims) => {
            req.extensions_mut().insert(claims);
            Ok(req)
        }
        Err(_) => Err((
            actix_web::error::ErrorUnauthorized("Invalid token"),
            req,
        )),
    }
}

/// Admin validator middleware (requires valid JWT with admin flag)
pub async fn admin_validator(
    req: actix_web::dev::ServiceRequest,
    credentials: BearerAuth,
) -> Result<actix_web::dev::ServiceRequest, (actix_web::Error, actix_web::dev::ServiceRequest)> {
    let token = credentials.token();

    // Get the secret from app state
    let secret = req
        .app_data::<web::Data<AppState>>()
        .map(|state| state.config.jwt_secret.clone())
        .unwrap_or_else(Config::get_jwt_secret);

    match decode_jwt(token, &secret) {
        Ok(claims) => {
            if !claims.is_admin {
                return Err((
                    actix_web::error::ErrorForbidden("Admin access required"),
                    req,
                ));
            }
            req.extensions_mut().insert(claims);
            Ok(req)
        }
        Err(_) => Err((
            actix_web::error::ErrorUnauthorized("Invalid token"),
            req,
        )),
    }
}
