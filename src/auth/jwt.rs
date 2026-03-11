use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};

use crate::models::Claims;

/// Create a JWT token for a user
pub fn create_jwt(
    username: &str,
    user_id: i64,
    is_admin: bool,
    secret: &str,
    expiry_hours: i64,
) -> Result<String, jsonwebtoken::errors::Error> {
    let expiration = Utc::now()
        .checked_add_signed(Duration::hours(expiry_hours))
        .expect("valid timestamp")
        .timestamp();

    let claims = Claims {
        sub: username.to_owned(),
        user_id,
        is_admin,
        exp: expiration as usize,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_ref()),
    )
}

/// Decode and validate a JWT token
pub fn decode_jwt(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_ref()),
        &Validation::default(),
    )?;
    Ok(token_data.claims)
}

/// Generate a cryptographically secure refresh token
pub fn generate_refresh_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret-at-least-32-chars-ok!";

    // --- create_jwt / decode_jwt ---

    #[test]
    fn create_and_decode_round_trip() {
        let token = create_jwt("alice", 42, false, SECRET, 1).unwrap();
        let claims = decode_jwt(&token, SECRET).unwrap();
        assert_eq!(claims.sub, "alice");
        assert_eq!(claims.user_id, 42);
        assert!(!claims.is_admin);
    }

    #[test]
    fn admin_flag_preserved() {
        let token = create_jwt("admin", 1, true, SECRET, 1).unwrap();
        let claims = decode_jwt(&token, SECRET).unwrap();
        assert!(claims.is_admin);
    }

    #[test]
    fn decode_fails_with_wrong_secret() {
        let token = create_jwt("alice", 1, false, SECRET, 1).unwrap();
        assert!(decode_jwt(&token, "wrong-secret-entirely-different").is_err());
    }

    #[test]
    fn decode_fails_for_expired_token() {
        use chrono::{Duration, Utc};
        use jsonwebtoken::{encode, EncodingKey, Header};
        use crate::models::Claims;

        let exp = (Utc::now() - Duration::hours(1)).timestamp() as usize;
        let claims = Claims { sub: "alice".into(), user_id: 1, is_admin: false, exp };
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(SECRET.as_ref()),
        )
        .unwrap();
        assert!(decode_jwt(&token, SECRET).is_err());
    }

    #[test]
    fn decode_fails_for_garbage_token() {
        assert!(decode_jwt("not.a.jwt", SECRET).is_err());
    }

    // --- generate_refresh_token ---

    #[test]
    fn refresh_token_is_url_safe_base64() {
        let token = generate_refresh_token();
        assert!(
            token.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_'),
            "token contains non-URL-safe chars: {token}"
        );
    }

    #[test]
    fn refresh_token_has_expected_length() {
        // 32 bytes → base64url-no-pad → ceil(32*4/3) = 43 chars
        assert_eq!(generate_refresh_token().len(), 43);
    }

    #[test]
    fn refresh_tokens_are_unique() {
        let t1 = generate_refresh_token();
        let t2 = generate_refresh_token();
        assert_ne!(t1, t2);
    }
}
