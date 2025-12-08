pub mod jwt;
pub mod middleware;

use actix_web::{HttpMessage, HttpRequest};
use crate::models::Claims;

/// Extract claims from request (helper function)
pub fn get_claims(req: &HttpRequest) -> Option<Claims> {
    req.extensions().get::<Claims>().cloned()
}
