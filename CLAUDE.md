# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Rust URL Shortener (RUS) - A URL shortening service with JWT authentication, SQLite persistence, and click tracking. Built with Actix-web.

## Build Modes

RUS supports two build modes controlled by Cargo feature flags:

### Standalone Mode (default)
Full-featured URL shortener with built-in user management:
- User registration and login with JWT authentication
- Password hashing with bcrypt
- Admin user management
- Account lockout protection

```bash
cargo build --release --features standalone
# Binary: target/release/rus
```

### SaaS Mode
Lightweight version designed for integration with a parent SaaS application:
- No built-in user management (uses external auth via access_token cookie)
- User identity extracted from parent app's JWT cookie
- No registration/login routes
- Dashboard redirects to parent app if no valid session

```bash
cargo build --release --no-default-features --features saas
# Binary: target/release/rus-saas
```

## Common Commands

```bash
# Development (standalone mode - default)
cargo run                              # Build and run (debug mode)
cargo build --release                  # Production build
cargo test                             # Run tests
cargo clippy                           # Lint code
cargo fmt                              # Format code

# Development (saas mode)
cargo run --no-default-features --features saas
cargo build --release --no-default-features --features saas
cargo test --no-default-features --features saas

# Docker deployment (standalone - default)
docker build --build-arg BUILD_MODE=standalone -t rus:standalone .
docker compose up --build              # Build and start

# Docker deployment (saas)
docker build --build-arg BUILD_MODE=saas -t rus:saas .

docker compose down                    # Stop containers
```

## Architecture

### Backend (modular structure)
- **Framework**: Actix-web 4.4 with Tokio async runtime
- **Database**: SQLite via rusqlite (bundled)
- **Auth (standalone)**: JWT tokens with bcrypt password hashing
- **Auth (saas)**: Cookie-based auth from parent application
- **Storage**: `./data/rus.db` (auto-created)

### Source Structure
```
src/
├── main.rs           # Entry point, route configuration
├── config.rs         # Environment-based configuration
├── db.rs             # Database connection and schema
├── models.rs         # Data models and request/response types
├── security.rs       # Password validation, account lockout (standalone only)
├── auth/             # JWT handling (standalone only)
│   ├── mod.rs
│   ├── jwt.rs
│   └── middleware.rs
├── handlers/
│   ├── mod.rs
│   ├── auth.rs       # Registration, login (standalone only)
│   ├── admin.rs      # User management (standalone only)
│   ├── abuse.rs      # Abuse reporting
│   ├── pages.rs      # Static page serving
│   ├── saas_auth.rs  # Cookie-based auth (saas only)
│   └── urls.rs       # URL shortening, redirect, statistics
└── url/
    ├── mod.rs
    ├── shortener.rs  # Short code generation
    └── qr.rs         # QR code generation
```

### Frontend (static/)
- Vanilla HTML/CSS/JS (no frameworks)
- JWT stored in localStorage
- Pages: index.html (landing), login.html, signup.html, dashboard.html
- auth.js handles token management

### API Structure
- **Public**: `/api/register`, `/api/login`, `/{short_code}` (redirect)
- **Protected** (Bearer token): `/api/shorten`, `/api/urls`, `/api/stats/{code}`, `/api/urls/{code}` (DELETE), `/api/urls/{code}/name` (PATCH)

### Key Implementation Details
- Short codes: 6 chars (A-Za-z0-9), collision-checked
- JWT claims: `sub` (username), `user_id`, `exp`
- Database: Single Mutex-wrapped connection (not production-grade)
- Password cost: bcrypt DEFAULT_COST (12 rounds)

## Environment Variables

### Required (standalone mode only)
```
JWT_SECRET=<base64-encoded-32-bytes>
```

### Optional (both modes)
```
DB_PATH=./data/rus.db       # Database location
HOST=0.0.0.0                # Bind address
PORT=8080                   # Server port
HOST_URL=http://localhost:8080  # Public URL for shortened links
MAX_URL_LENGTH=2048         # Maximum URL length
CLICK_RETENTION_DAYS=30     # Days to retain click history
ALLOW_REGISTRATION=true     # Allow public signups (default: true)
```

### Standalone-only options
```
JWT_EXPIRY=1                # JWT expiry in hours (default: 1)
REFRESH_TOKEN_EXPIRY=7      # Refresh token expiry in days (default: 7)
ACCOUNT_LOCKOUT_ATTEMPTS=5  # Failed attempts before lockout (default: 5)
ACCOUNT_LOCKOUT_DURATION=30 # Lockout duration in minutes (default: 30)
```

## Database Schema

**users**: userID, username (unique), password (hashed), created_at
**urls**: id, user_id (FK), original_url, short_code (unique indexed), name, clicks, created_at

## Testing API

```bash
# Register
curl -X POST http://localhost:8080/api/register \
  -H "Content-Type: application/json" \
  -d '{"username":"test","password":"password123"}'

# Login (returns token)
curl -X POST http://localhost:8080/api/login \
  -H "Content-Type: application/json" \
  -d '{"username":"test","password":"password123"}'

# Shorten URL (protected)
curl -X POST http://localhost:8080/api/shorten \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer {TOKEN}" \
  -d '{"url":"https://example.com"}'
```

## Build System

The Docker build uses a unified `Dockerfile` with a `setup.nu` script:

- **Dockerfile**: Multi-stage build with BuildKit cache mounts for fast incremental builds. Uses `rust:<version>-alpine` for building and `alpine:<version>` for runtime.
- **oci-build/setup.nu**: Nushell script that runs inside the builder stage. Installs build deps, runs `cargo build` with the correct feature flags, and copies the binary to `/build/output/app`.
- **BUILD_MODE**: ARG that selects `standalone` (default) or `saas` mode.
- **Cache mounts**: Cargo registry, git, and target directory are cached across builds via `--mount=type=cache`.

Build args: `RUST_VERSION`, `ALPINE_VERSION`, `NU_VERSION`, `BUILD_MODE`.
