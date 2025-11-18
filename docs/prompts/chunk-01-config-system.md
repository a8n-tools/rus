# Chunk 1: Foundation - Configuration System

## Context
We're building a production-ready URL shortener in Rust using Actix-web. The current implementation has hardcoded values. We need a centralized configuration system that reads from environment variables with sensible defaults.

## Goal
Create a Config struct that manages all application settings, loaded from environment variables with defaults.

## Prompt

```text
I have a Rust URL shortener using Actix-web. I need to add a centralized configuration system.

Current state:
- JWT secret is read directly from env with hardcoded fallback
- Token expiry is hardcoded as const TOKEN_EXPIRY_HOURS: i64 = 24
- Database path is hardcoded as "./data/rus.db"
- Host/port are hardcoded

Create a Config struct with these fields and defaults:
- jwt_secret: String (from JWT_SECRET env, warn if using default)
- jwt_expiry_hours: i64 (from JWT_EXPIRY env, default 1)
- refresh_token_expiry_days: i64 (from REFRESH_TOKEN_EXPIRY env, default 7)
- max_url_length: usize (from MAX_URL_LENGTH env, default 2048)
- account_lockout_attempts: i32 (from ACCOUNT_LOCKOUT_ATTEMPTS env, default 5)
- account_lockout_duration_minutes: i64 (from ACCOUNT_LOCKOUT_DURATION env, default 30)
- click_retention_days: i64 (from CLICK_RETENTION_DAYS env, default 30)
- host_url: String (from HOST_URL env, default "http://localhost:8080")
- db_path: String (from DB_PATH env, default "./data/rus.db")
- host: String (from HOST env, default "0.0.0.0")
- port: u16 (from PORT env, default 8080)

Implement:
1. Config struct with all fields
2. impl Config with from_env() method that:
   - Reads each env var
   - Parses to correct type
   - Uses default on parse failure
   - Prints warning for missing JWT_SECRET
3. Update AppState to hold Config (not just db)
4. Update main() to:
   - Create config via Config::from_env()
   - Print startup banner showing all config values
   - Pass config to AppState::new()
5. Update AppState::new() to accept Config and store it

Remove the old const TOKEN_EXPIRY_HOURS and get_jwt_secret() function. The JWT secret should come from config.

Make sure all existing code still compiles - we're just adding the config infrastructure, not using it yet.
```

## Expected Output
- Config struct defined
- from_env() implementation
- AppState updated to include config
- Startup logs showing configuration
- No functional changes yet (preparation step)
