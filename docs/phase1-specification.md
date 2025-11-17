# Phase 1 Specification: Production-Ready Self-Hosted URL Shortener

## Overview
Production-ready URL shortening service optimized for home lab/self-hosted environments with low volume but high reliability and security.

## Deployment
- **Platform**: Docker with docker-compose
- **Restart Policy**: Auto-restart on crash
- **Database**: Single-file SQLite (existing approach)
- **Backups**: Manual/on-demand

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `JWT_SECRET` | *required* | Base64-encoded 32-byte secret |
| `HOST_URL` | `http://localhost:8080` | Public-facing URL for display |
| `DB_PATH` | `./data/rus.db` | Database location |
| `HOST` | `0.0.0.0` | Bind address |
| `PORT` | `8080` | Server port |
| `JWT_EXPIRY` | `1` | Access token expiry (hours) |
| `REFRESH_TOKEN_EXPIRY` | `7` | Refresh token expiry (days) |
| `MAX_URL_LENGTH` | `2048` | Maximum URL character length |
| `ACCOUNT_LOCKOUT_ATTEMPTS` | `5` | Failed attempts before lockout |
| `ACCOUNT_LOCKOUT_DURATION` | `30` | Lockout duration (minutes) |
| `CLICK_RETENTION_DAYS` | `30` | Click history retention |

## Security Features

### Authentication Hardening
- **Account Lockout**: 5 failed attempts → 30 minute lockout (configurable)
- **Password Requirements**: 8+ characters, at least one uppercase, one number, one special character
- **JWT**: 1-hour access tokens with 7-day refresh tokens (configurable)
- **Refresh Token Flow**: Secure token refresh without re-authentication

### Input Validation
- **URL Length**: Maximum 2048 characters (configurable)
- **Allowed Schemes**: `http://` and `https://` only
- **Blocked Schemes**: `javascript:`, `data:`, `file://`, and other dangerous schemes

## Reliability Features
- Docker auto-restart on crash
- Graceful shutdown handling
- Health check endpoint

## Backend Enhancements

### New API Endpoints
- `POST /api/refresh` - Refresh access token
- `GET /api/urls/{code}/qr/{format}` - Generate QR code with Rust logo branding (png or svg)
- `GET /api/urls/{code}/clicks` - Click history with timestamps
- `GET /health` - Health check endpoint
- `GET /api/config` - Public configuration (HOST_URL, max URL length)

### Click Tracking
- Store individual clicks with timestamps
- Automatic cleanup of clicks older than retention period
- Aggregation support for daily/weekly views

## Frontend Requirements

### Design System
- **Color Scheme**: Black and light rusty orange (Rust-themed)
- **Approach**: Vanilla HTML/CSS/JS (no frameworks)
- **Responsive**: Mobile-first, fully responsive design
- **Navigation**: Uniform navbar across all pages

### Dashboard Features
- Display all URL fields: short code, name, clicks, created_at
- Sorting options: by date, clicks, name
- Filtering options
- Click history visualizations:
  - Line chart
  - Bar chart
  - Table view
- Time range selection for analytics

### QR Code Generation
- Server-side generation (Rust library)
- Rust logo branded in center
- Standard size output
- Downloadable as PNG and SVG
- On-demand generation from dashboard

### URL Display
- Use `HOST_URL` environment variable for displaying full shortened URLs
- Copy-to-clipboard functionality

## Database Schema Updates

### New Table: click_history
```sql
CREATE TABLE IF NOT EXISTS click_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    url_id INTEGER NOT NULL,
    clicked_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (url_id) REFERENCES urls(id) ON DELETE CASCADE
);
```

### New Table: refresh_tokens
```sql
CREATE TABLE IF NOT EXISTS refresh_tokens (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL,
    token TEXT NOT NULL UNIQUE,
    expires_at DATETIME NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(userID) ON DELETE CASCADE
);
```

### New Table: login_attempts
```sql
CREATE TABLE IF NOT EXISTS login_attempts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL,
    attempted_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    success INTEGER NOT NULL DEFAULT 0
);
```

### New Indexes
```sql
CREATE INDEX IF NOT EXISTS idx_click_history_url_id ON click_history(url_id);
CREATE INDEX IF NOT EXISTS idx_click_history_clicked_at ON click_history(clicked_at);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_token ON refresh_tokens(token);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_id ON refresh_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_login_attempts_username ON login_attempts(username);
CREATE INDEX IF NOT EXISTS idx_login_attempts_attempted_at ON login_attempts(attempted_at);
```
