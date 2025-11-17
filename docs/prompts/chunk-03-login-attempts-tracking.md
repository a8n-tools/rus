# Chunk 3: Login Attempts Database Tracking

## Context
Building on previous chunks. We need to track login attempts in the database to enable account lockout (next chunk).

## Goal
Create the login_attempts table and functions to record attempts.

## Prompt

```text
I have a Rust URL shortener with password validation. Now I need to track login attempts for account lockout.

Add database schema for login attempts. In the AppState::new() function where we initialize tables with execute_batch(), add:

```sql
CREATE TABLE IF NOT EXISTS login_attempts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL,
    attempted_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    success INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_login_attempts_username ON login_attempts(username);
CREATE INDEX IF NOT EXISTS idx_login_attempts_attempted_at ON login_attempts(attempted_at);
```

Create a record_login_attempt() function:
1. Takes: db: &Connection, username: &str, success: bool
2. Returns: nothing (fire and forget, ignore errors)
3. Inserts a row with:
   - username: the provided username
   - success: 1 if true, 0 if false (SQLite doesn't have bool)
   - attempted_at: uses DEFAULT CURRENT_TIMESTAMP

Add this line to record attempts:
```rust
fn record_login_attempt(db: &Connection, username: &str, success: bool) {
    let _ = db.execute(
        "INSERT INTO login_attempts (username, success) VALUES (?1, ?2)",
        params![username, success as i32],
    );
}
```

Integrate into login() handler:
1. On successful login (password verified): record_login_attempt(&db, &req.username, true)
2. On failed login (wrong password): record_login_attempt(&db, &req.username, false)
3. On user not found: record_login_attempt(&db, &req.username, false)

The recording should happen BEFORE returning the response. This is just tracking - lockout logic comes in the next chunk.

Make sure to import chrono::Duration if not already imported.
```

## Expected Output
- login_attempts table created on startup
- record_login_attempt() function
- All login attempts (success and failure) are recorded
- Database has indexes for efficient querying
