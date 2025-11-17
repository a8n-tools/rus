# Chunk 7: Refresh Token Database Schema

## Context
Building on health check. We need to store refresh tokens in the database for the JWT refresh mechanism.

## Goal
Create the refresh_tokens table and related data structures.

## Prompt

```text
I have a Rust URL shortener with health check endpoint. Now add refresh token database schema.

In AppState::new() execute_batch(), add the refresh_tokens table:

```sql
CREATE TABLE IF NOT EXISTS refresh_tokens (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL,
    token TEXT NOT NULL UNIQUE,
    expires_at DATETIME NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(userID) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_refresh_tokens_token ON refresh_tokens(token);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_id ON refresh_tokens(user_id);
```

Add these data structures for the refresh token API:

```rust
#[derive(Serialize, Deserialize)]
struct RefreshRequest {
    refresh_token: String,
}

#[derive(Serialize, Deserialize)]
struct RefreshResponse {
    token: String,
    refresh_token: String,
}
```

Update AuthResponse to include refresh_token:
```rust
#[derive(Serialize, Deserialize)]
struct AuthResponse {
    token: String,
    refresh_token: String,  // ADD THIS FIELD
    username: String,
}
```

The table structure:
- id: Auto-increment primary key
- user_id: Links to users table, cascades on delete
- token: The actual refresh token string (unique)
- expires_at: When this token expires
- created_at: When token was created

Indexes for performance:
- idx_refresh_tokens_token: Fast lookup by token value
- idx_refresh_tokens_user_id: Fast lookup/cleanup by user

This is just schema and structs - actual token generation comes next.
```

## Expected Output
- refresh_tokens table created on startup
- RefreshRequest struct
- RefreshResponse struct
- AuthResponse updated with refresh_token field
- Proper indexes for performance
