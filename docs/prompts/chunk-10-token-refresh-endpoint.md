# Chunk 10: Token Refresh Endpoint

## Context
Building on refresh token storage. We need an endpoint to exchange refresh tokens for new access tokens.

## Goal
Implement POST /api/refresh endpoint with token rotation.

## Prompt

```text
I have refresh tokens being issued. Now implement the refresh endpoint.

Update decode_jwt() to use config secret (if not already done):
```rust
fn decode_jwt(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_ref()),
        &Validation::default(),
    )?;
    Ok(token_data.claims)
}
```

Create refresh_token handler:
```rust
async fn refresh_token(
    data: web::Data<AppState>,
    req: web::Json<RefreshRequest>,
) -> Result<HttpResponse> {
    let db = data.db.lock().unwrap();

    // Find and validate refresh token
    let token_result: rusqlite::Result<(i64, i64, String)> = db.query_row(
        "SELECT rt.id, rt.user_id, u.username FROM refresh_tokens rt
         JOIN users u ON rt.user_id = u.userID
         WHERE rt.token = ?1 AND rt.expires_at > datetime('now')",
        params![&req.refresh_token],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    );

    match token_result {
        Ok((token_id, user_id, username)) => {
            // Delete old refresh token (rotation)
            let _ = db.execute("DELETE FROM refresh_tokens WHERE id = ?1", params![token_id]);

            // Create new JWT token
            let token = match create_jwt(&username, user_id, &data.config.jwt_secret, data.config.jwt_expiry_hours) {
                Ok(t) => t,
                Err(_) => {
                    return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                        "error": "Failed to create token"
                    })));
                }
            };

            // Create new refresh token (rotation)
            let new_refresh_token = generate_refresh_token();
            let expires_at = Utc::now() + Duration::days(data.config.refresh_token_expiry_days);
            let expires_at_str = expires_at.format("%Y-%m-%d %H:%M:%S").to_string();

            let _ = db.execute(
                "INSERT INTO refresh_tokens (user_id, token, expires_at) VALUES (?1, ?2, ?3)",
                params![user_id, &new_refresh_token, &expires_at_str],
            );

            Ok(HttpResponse::Ok().json(RefreshResponse {
                token,
                refresh_token: new_refresh_token,
            }))
        }
        Err(_) => Ok(HttpResponse::Unauthorized().json(serde_json::json!({
            "error": "Invalid or expired refresh token"
        }))),
    }
}
```

Register the route in main() - PUBLIC route (no auth middleware):
```rust
.route("/api/refresh", web::post().to(refresh_token))
```

Place it AFTER /api/login and BEFORE the authenticated /api scope.

Key security features:
1. Token rotation: Old refresh token deleted, new one issued
2. Expiry check: Only valid if expires_at > now
3. User validation: Joins with users table to get username
4. Returns both new access token and new refresh token

This prevents refresh token reuse attacks.
```

## Expected Output
- decode_jwt() uses config secret
- refresh_token handler with rotation
- /api/refresh route registered (public)
- Validates token expiry
- Deletes old token on use
- Issues new token pair
