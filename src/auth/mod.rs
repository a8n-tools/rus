pub mod jwt;
pub mod middleware;

use actix_web::{HttpMessage, HttpRequest};
use chrono::{Duration, Utc};
use rusqlite::{params, Connection};

use crate::auth::jwt::{create_jwt, generate_refresh_token};
use crate::config::Config;
use crate::models::{AuthResponse, Claims};

/// Extract claims from request (helper function)
pub fn get_claims(req: &HttpRequest) -> Option<Claims> {
    req.extensions().get::<Claims>().cloned()
}

/// Mint the credentials a completed standalone sign-in hands back: an access
/// JWT plus a stored refresh token.
///
/// The single place a standalone session is established (RUS-19). Login,
/// registration and the approval route all come through here, so the approval
/// gate cannot be bypassed by a path that mints its own tokens. Refreshing an
/// existing token is deliberately not routed here: it continues a session that
/// was already established rather than starting one.
pub fn establish_session(
    db: &Connection,
    config: &Config,
    username: &str,
    user_id: i64,
    is_admin: bool,
) -> Result<AuthResponse, String> {
    let token = create_jwt(
        username,
        user_id,
        is_admin,
        &config.jwt_secret,
        config.jwt_expiry_hours,
    )
    .map_err(|error| format!("Failed to create token: {error}"))?;

    let refresh_token = generate_refresh_token();
    let expires_at = (Utc::now() + Duration::days(config.refresh_token_expiry_days))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    db.execute(
        "INSERT INTO refresh_tokens (user_id, token, expires_at) VALUES (?1, ?2, ?3)",
        params![user_id, &refresh_token, &expires_at],
    )
    .map_err(|error| format!("Failed to store refresh token: {error}"))?;

    Ok(AuthResponse {
        token,
        refresh_token,
        username: username.to_string(),
    })
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
    fn establish_session_returns_a_usable_token_and_stores_the_refresh() {
        let state = crate::testing::make_test_state();
        let user_id = crate::testing::insert_test_user(&state, "alice", false);
        let db = state.db.lock().unwrap();

        let session = establish_session(&db, &state.config, "alice", user_id, false).unwrap();

        let claims =
            crate::auth::jwt::decode_jwt(&session.token, &state.config.jwt_secret).unwrap();
        assert_eq!(claims.user_id, user_id);
        let stored: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM refresh_tokens WHERE user_id = ?1 AND token = ?2",
                rusqlite::params![user_id, &session.refresh_token],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stored, 1);
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
