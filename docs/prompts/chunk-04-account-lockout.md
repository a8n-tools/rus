# Chunk 4: Account Lockout Enforcement

## Context
Building on login attempts tracking. Now we enforce lockout based on failed attempts.

## Goal
Check for account lockout before allowing login, using config values for thresholds.

## Prompt

```text
I have login attempts being tracked in the database. Now implement account lockout logic.

Create an is_account_locked() function:
1. Takes: db: &Connection, username: &str, max_attempts: i32, lockout_minutes: i64
2. Returns: bool (true if locked, false if not)
3. Logic:
   - Calculate cutoff time: current time minus lockout_minutes
   - Count failed attempts (success = 0) for this username since cutoff
   - Return true if count >= max_attempts

```rust
fn is_account_locked(db: &Connection, username: &str, max_attempts: i32, lockout_minutes: i64) -> bool {
    let cutoff = Utc::now() - Duration::minutes(lockout_minutes);
    let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();

    let failed_attempts: i32 = db.query_row(
        "SELECT COUNT(*) FROM login_attempts WHERE username = ?1 AND success = 0 AND attempted_at > ?2",
        params![username, cutoff_str],
        |row| row.get(0),
    ).unwrap_or(0);

    failed_attempts >= max_attempts
}
```

Integrate into login() handler at the VERY BEGINNING:
1. After acquiring the database lock
2. Before checking if user exists
3. Call is_account_locked() with config values from data.config
4. If locked, return HTTP 429 (TooManyRequests) with JSON:
   ```json
   {
     "error": "Account locked due to too many failed attempts. Try again in X minutes."
   }
   ```
   Where X is the lockout duration from config

The check must happen BEFORE any database lookup of the user, so attackers can't use timing to determine if username exists.

Make sure the login handler has access to data.config for the lockout parameters:
- data.config.account_lockout_attempts
- data.config.account_lockout_duration_minutes
```

## Expected Output
- is_account_locked() function
- Lockout check at start of login
- HTTP 429 response when locked
- Uses config values (not hardcoded)
- Descriptive error message with wait time
