# Chunk 9: Auth Response with Refresh Tokens

## Context
Building on token generation. Now we issue refresh tokens during registration and login.

## Goal
Update register() and login() to generate and return refresh tokens.

## Prompt

```text
I have refresh token generation. Now integrate it into auth responses.

Update create_jwt() to use config values (if not already done):
```rust
fn create_jwt(username: &str, user_id: i64, secret: &str, expiry_hours: i64) -> Result<String, jsonwebtoken::errors::Error> {
    let expiration = Utc::now()
        .checked_add_signed(Duration::hours(expiry_hours))
        .expect("valid timestamp")
        .timestamp();

    let claims = Claims {
        sub: username.to_owned(),
        user_id,
        exp: expiration as usize,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_ref()),
    )
}
```

In register() handler, after creating JWT token and before returning:
```rust
// Create refresh token
let refresh_token = generate_refresh_token();
let expires_at = Utc::now() + Duration::days(data.config.refresh_token_expiry_days);
let expires_at_str = expires_at.format("%Y-%m-%d %H:%M:%S").to_string();

let _ = db.execute(
    "INSERT INTO refresh_tokens (user_id, token, expires_at) VALUES (?1, ?2, ?3)",
    params![user_id, &refresh_token, &expires_at_str],
);

Ok(HttpResponse::Created().json(AuthResponse {
    token,
    refresh_token,
    username: req.username.clone(),
}))
```

In login() handler, after successful password verification:
```rust
// Create refresh token
let refresh_token = generate_refresh_token();
let expires_at = Utc::now() + Duration::days(data.config.refresh_token_expiry_days);
let expires_at_str = expires_at.format("%Y-%m-%d %H:%M:%S").to_string();

let _ = db.execute(
    "INSERT INTO refresh_tokens (user_id, token, expires_at) VALUES (?1, ?2, ?3)",
    params![user_id, &refresh_token, &expires_at_str],
);

Ok(HttpResponse::Ok().json(AuthResponse {
    token,
    refresh_token,
    username,
}))
```

Update JWT creation calls to use config:
```rust
create_jwt(&req.username, user_id, &data.config.jwt_secret, data.config.jwt_expiry_hours)
```

Both register and login now:
1. Create access token (JWT)
2. Generate refresh token
3. Calculate expiry date from config
4. Store refresh token in database
5. Return both tokens to client
```

## Expected Output
- create_jwt() uses config parameters
- register() returns refresh_token
- login() returns refresh_token
- Tokens stored in database with expiry
- Uses config for expiry days
