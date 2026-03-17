pub mod jwt;
pub mod middleware;

use actix_web::{HttpMessage, HttpRequest};
use crate::models::Claims;

/// Extract claims from request (helper function)
pub fn get_claims(req: &HttpRequest) -> Option<Claims> {
    req.extensions().get::<Claims>().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;

    #[test]
    fn get_claims_returns_none_when_no_claims() {
        let req = TestRequest::default().to_http_request();
        assert!(get_claims(&req).is_none());
    }

    #[test]
    fn get_claims_returns_inserted_claims() {
        let req = TestRequest::default().to_http_request();
        let claims = Claims {
            sub: "alice".to_string(),
            user_id: 42,
            is_admin: false,
            exp: 9999999999,
        };
        req.extensions_mut().insert(claims.clone());

        let extracted = get_claims(&req).unwrap();
        assert_eq!(extracted.sub, "alice");
        assert_eq!(extracted.user_id, 42);
        assert!(!extracted.is_admin);
    }

    #[test]
    fn get_claims_returns_admin_flag() {
        let req = TestRequest::default().to_http_request();
        let claims = Claims {
            sub: "admin".to_string(),
            user_id: 1,
            is_admin: true,
            exp: 9999999999,
        };
        req.extensions_mut().insert(claims);

        let extracted = get_claims(&req).unwrap();
        assert!(extracted.is_admin);
    }
}
