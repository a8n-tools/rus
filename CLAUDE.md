# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Rust URL Shortener (RUS) - A URL shortening service with JWT authentication, SQLite persistence, and click tracking. Built with Actix-web.

## Common Commands

```bash
# Development
cargo run                    # Build and run (debug mode)
cargo build --release        # Production build
cargo test                   # Run tests
cargo clippy                 # Lint code
cargo fmt                    # Format code

# Docker deployment
docker compose up --build    # Build and start
docker compose down          # Stop containers

# Database initialization (first time)
docker compose -f compose-sqlite.yml run sqlite
```

## Architecture

### Backend (src/main.rs - single file)
- **Framework**: Actix-web 4.4 with Tokio async runtime
- **Database**: SQLite via rusqlite (bundled)
- **Auth**: JWT tokens (24hr expiry) with bcrypt password hashing
- **Storage**: `./data/rus.db` (auto-created)

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

Required in `.env`:
```
JWT_SECRET=<base64-encoded-32-bytes>
```

Optional:
```
DB_PATH=./data/rus.db    # Database location
HOST=0.0.0.0             # Bind address
PORT=8080                # Server port
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
