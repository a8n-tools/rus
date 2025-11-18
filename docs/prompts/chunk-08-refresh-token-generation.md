# Chunk 8: Refresh Token Generation

## Context
Building on refresh token schema. We need to generate secure random tokens and store them.

## Goal
Create function to generate cryptographically secure refresh tokens.

## Prompt

```text
I have refresh token schema in place. Now implement token generation.

First, ensure base64 dependency in Cargo.toml:
```toml
base64 = "0.22"
```

Import at top of main.rs:
```rust
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
```

Create generate_refresh_token() function:
```rust
fn generate_refresh_token() -> String {
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    BASE64.encode(&bytes)
}
```

This generates:
- 32 random bytes (256 bits of entropy)
- Base64 encoded for safe storage/transmission
- Approximately 44 characters long

The token should be:
- Cryptographically random (using rand::thread_rng())
- URL-safe when base64 encoded
- Unique enough that collisions are effectively impossible

We're using the standard base64 encoding. The STANDARD engine from base64 0.22 provides this.

This function is a pure utility - no database interaction yet. We'll wire it into the auth flow in the next chunk.

Make sure rand is already imported (it should be from short code generation).
```

## Expected Output
- base64 crate dependency
- BASE64 engine import
- generate_refresh_token() function
- Returns 44-character base64 string
- Uses cryptographically secure randomness
