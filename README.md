# rus

**Rust URL Shortener** - A fast, secure URL shortening service built with Rust and Actix-web. Supports two deployment modes: standalone with built-in auth, or SaaS for integration with a parent application.

![URL Shortener Homepage](/assets/screenshot.png)

## Features

- **JWT Authentication** - Secure user registration and login with Argon2id password hashing
- **SQLite Persistence** - Reliable data storage with SQLite (bundled, zero setup)
- **Click Tracking** - Per-click history with configurable retention and analytics
- **QR Code Generation** - Generate QR codes for shortened URLs (PNG and SVG)
- **Custom Names** - Give your shortened URLs memorable names
- **URL Management** - Create, rename, delete, and monitor URLs
- **Admin Panel** - User management and abuse report review
- **Abuse Reporting** - Public abuse reporting for malicious URLs
- **Account Security** - Login attempt tracking with configurable lockout
- **Rate Limiting** - Built-in request rate limiting via actix-governor
- **Refresh Tokens** - Seamless token refresh without re-login
- **Dual Build Modes** - Standalone or SaaS deployment
- **Docker Support** - Multi-stage Dockerfile with dependency caching

## Build Modes

### Standalone (default)

Full-featured URL shortener with built-in user management:
- User registration and login with JWT authentication
- Password hashing with Argon2id (with automatic bcrypt migration)
- Admin user management
- Account lockout protection
- Refresh token rotation

```bash
cargo build --release --features standalone
```

### SaaS

Lightweight version for integration with a parent application:
- No built-in user management (uses external auth via `access_token` cookie)
- User identity extracted from parent app's JWT
- No registration/login routes
- Dashboard redirects to parent app if no valid session

```bash
cargo build --release --no-default-features --features saas
```

## Prerequisites

- Rust 1.93 or higher (or use Docker)

## Installation

1. Clone the repository:
```bash
git clone https://github.com/joshrandall8478/rus.git
cd rus
```

2. Copy and edit the environment file:
```bash
cp .env.standalone .env
# Edit .env and set JWT_SECRET
```

3. Build and run:
```bash
cargo build --release
cargo run --release
```

The application starts on `http://localhost:4001`.

### Docker Deployment

Each developer gets their own HTTPS instance at `https://{USER}-rus.a8n.run` (where `{USER}` is your OS username), routed via Traefik:

```bash
just dev                 # Start standalone instance
just dev saas            # Start SaaS mode instance
just dev-stop            # Stop instance
```

For local development without Traefik (cargo-watch with hot-reload on localhost:4001):

```bash
just dev-local           # Start local dev server
just dev-local-stop      # Stop local dev server
```

## Usage

### Web Interface

1. **Sign Up** - Create an account at `/signup.html`
2. **Log In** - Authenticate at `/login.html`
3. **Dashboard** - Manage your URLs at `/dashboard.html`:
   - Shorten new URLs
   - View click statistics and history
   - Generate QR codes
   - Rename URLs with custom names
   - Copy short URLs to clipboard
   - Delete URLs
4. **Admin** - Manage users and abuse reports at `/admin.html` (admin only)
5. **Report** - Report abusive URLs at `/report.html`

### API Endpoints

#### Public

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/register` | Register a new user (standalone only) |
| `POST` | `/api/login` | Login, returns JWT + refresh token (standalone only) |
| `POST` | `/api/token/refresh` | Refresh an expired JWT (standalone only) |
| `GET` | `/{short_code}` | Redirect to original URL |
| `POST` | `/api/report` | Report an abusive URL |

#### Protected (Bearer Token)

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/me` | Current account: username, admin flag, alert opt-out |
| `PATCH` | `/api/me` | Update account settings (`notify_new_location`) |
| `POST` | `/api/shorten` | Shorten a URL |
| `GET` | `/api/urls` | List user's URLs |
| `GET` | `/api/stats/{code}` | Get URL statistics |
| `GET` | `/api/stats/{code}/clicks` | Get click history |
| `DELETE` | `/api/urls/{code}` | Delete a URL |
| `PATCH` | `/api/urls/{code}/name` | Rename a URL |
| `GET` | `/api/qr/{code}` | Generate QR code (PNG) |
| `GET` | `/api/qr/{code}/svg` | Generate QR code (SVG) |
| `GET` | `/api/config` | Get public configuration |

#### Admin (Bearer Token, admin users only)

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/admin/users` | List all users |
| `DELETE` | `/api/admin/users/{id}` | Delete a user |
| `PATCH` | `/api/admin/users/{id}/admin` | Toggle admin status |
| `GET` | `/api/admin/reports` | List abuse reports |
| `PATCH` | `/api/admin/reports/{id}` | Resolve an abuse report |

## Example Usage

### Using cURL

Register:
```bash
curl -X POST http://localhost:4001/api/register \
  -H "Content-Type: application/json" \
  -d '{"username":"test","password":"password123"}'
```

Login and save token:
```bash
TOKEN=$(curl -s -X POST http://localhost:4001/api/login \
  -H "Content-Type: application/json" \
  -d '{"username":"test","password":"password123"}' | jq -r '.token')
```

Shorten a URL:
```bash
curl -X POST http://localhost:4001/api/shorten \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"url":"https://github.com/joshrandall8478/rus"}'
```

Get your URLs:
```bash
curl http://localhost:4001/api/urls \
  -H "Authorization: Bearer $TOKEN"
```

## Project Structure

```
rus/
├── src/
│   ├── main.rs              # Entry point, route configuration
│   ├── config.rs            # Environment-based configuration
│   ├── db.rs                # Database connection and schema
│   ├── models.rs            # Data models and request/response types
│   ├── security.rs          # Password validation, account lockout
│   ├── location_alert.rs    # New-sign-in-country detection, trusted-proxy gate
│   ├── mailer.rs            # Security alert email via SMTP
│   ├── auth/
│   │   ├── mod.rs
│   │   ├── jwt.rs           # JWT creation and validation
│   │   └── middleware.rs    # Auth middleware
│   ├── handlers/
│   │   ├── mod.rs
│   │   ├── auth.rs          # Registration, login (standalone)
│   │   ├── admin.rs         # User management (standalone)
│   │   ├── abuse.rs         # Abuse reporting
│   │   ├── pages.rs         # Static page serving
│   │   ├── saas_auth.rs     # Cookie-based auth (saas)
│   │   └── urls.rs          # URL CRUD, redirect, statistics
│   └── url/
│       ├── mod.rs
│       ├── shortener.rs     # Short code generation
│       └── qr.rs            # QR code generation
├── static/
│   ├── index.html           # Landing page
│   ├── login.html           # Login page
│   ├── signup.html          # Registration page
│   ├── dashboard.html       # URL management dashboard
│   ├── admin.html           # Admin panel
│   ├── report.html          # Abuse report form
│   ├── setup.html           # Initial setup page
│   ├── 404.html             # Custom 404 error page
│   ├── styles.css           # Global styles
│   └── auth.js              # Authentication utilities
├── oci-build/
│   ├── setup.nu             # Nushell build script
│   └── get-tags.nu          # Image tag derivation from git describe
├── data/
│   └── rus.db               # SQLite database (auto-created)
├── Cargo.toml
├── Dockerfile
├── compose.yml
├── compose.dev.yml          # Per-developer Traefik compose
├── justfile                 # Task runner recipes
├── .env.standalone          # Env template for standalone mode
└── .env.saas                # Env template for saas mode
```

## Environment Variables

### Shared (both modes)

| Variable | Description | Default |
|----------|-------------|---------|
| `DB_PATH` | Path to SQLite database file | `./data/rus.db` |
| `HOST` | Server bind address | `0.0.0.0` |
| `APP_PORT` | Server port | `4001` |
| `HOST_URL` | Public URL for shortened links | `http://localhost:4001` |
| `MAX_URL_LENGTH` | Maximum URL length | `2048` |
| `CLICK_RETENTION_DAYS` | Days to retain click history | `30` |
| `TRUSTED_PROXY_CIDRS` | Peers allowed to set the forwarded IP and country headers (comma-separated CIDRs or bare IPs) | unset (trust nothing) |
| `RUST_LOG` | Log level | `info` |

`TRUSTED_PROXY_CIDRS` gates `X-Forwarded-For`, `X-Real-IP`, and `X-IPCountry` on the socket peer, the one input a client cannot forge. When the peer is not in the list all three headers are ignored and the peer address is used, so a client that reaches the app directly can neither spoof its IP nor spoof or suppress the new-sign-in-location alert. When the peer is trusted, the rightmost `X-Forwarded-For` entry that is not itself a trusted proxy wins, falling back to `X-Real-IP` and then to the peer. Entries that do not parse log a warning and are skipped rather than failing startup.

**Any deployment behind a reverse proxy must set this.** rus runs behind Traefik on a private Docker network, so an unset list makes every client look like the proxy and, because an untrusted peer never yields a country, silently disables the sign-in-location alert. The `.env.standalone` and `.env.saas` templates ship with the private ranges (`10.0.0.0/8,172.16.0.0/12,192.168.0.0/16,fd00::/8`) already set; a compose deployment that supplies its environment directly has to set the same value itself. Startup logs a warning whenever the list is empty.

### Security alerts (both modes)

| Variable | Description | Default |
|----------|-------------|---------|
| `SECURITY_ALERT_EMAIL` | Fallback mailbox for an account with no address of its own; unset means log-only for those accounts | unset |
| `LOGIN_LOCATION_ALERTS_ENABLED` | New-country sign-in alert kill switch (`false`/`0`/`no` disables) | `true` |
| `SMTP_HOST` | SMTP relay hostname | unset |
| `SMTP_FROM_EMAIL` | Sender address | unset |
| `SMTP_FROM_NAME` | Sender display name | unset |
| `SMTP_USERNAME` | SMTP auth username | unset |
| `SMTP_PASSWORD` | SMTP auth password | unset |
| `SMTP_TLS_MODE` | Connection encryption: `starttls`, `tls`, or `none` | `starttls` |
| `SMTP_PORT` | Port override; each TLS mode has its own default | mode default |

A new-sign-in-location alert is addressed to the account owner, in this order:

1. The account's own address (`users.email`) when it has one. The message is a personal security notice.
2. Otherwise `SECURITY_ALERT_EMAIL`, the shared operator mailbox. That message names which account signed in, since its reader is not the owner.
3. Otherwise, or whenever SMTP is unconfigured, the would-be alert is written to the log and nothing is sent. A login is never failed or slowed by mail configuration.

`SMTP_HOST` and `SMTP_FROM_EMAIL` must be set before anything can be sent at all. `SECURITY_ALERT_EMAIL` is only the fallback: an account with its own address is notified without one configured.

In saas mode the address comes from the OP identity and is refreshed on every login, so there is nothing to set by hand. In standalone mode an account sets its own: optionally at registration (`POST /api/register` accepts an `email` field), and at any time afterwards with `PATCH /api/me` (`{"email": "you@example.com"}`). A blank value clears it back to unset. `GET /api/me` returns the current value. The address is stored trimmed and lowercased and is not verified, so it receives security mail as soon as it is set. The browser field that drives these endpoints from the dashboard is tracked in RUS-17.

The per-account `notify_new_location` opt-out is checked before any of this, so an account that has opted out is never alerted whichever way the message would have been routed.

The country comes from the `X-IPCountry` header injected by the reverse proxy's geoblock middleware, and is read only when the socket peer is listed in `TRUSTED_PROXY_CIDRS`. With no trusted proxy configured, or with a client connecting directly, no country resolves and no alert fires.

Mail is encrypted by default: `SMTP_TLS_MODE` defaults to `starttls`, so a deployment that sets nothing sends over an encrypted connection. `starttls` upgrades the connection on port 587, `tls` uses implicit TLS on port 465, and `none` is plaintext, kept only for a trusted loopback or sidecar relay and logging a warning naming the host whenever it is used. An unrecognised value warns and falls back to `starttls`. Each mode supplies its own default port, so `SMTP_PORT` is an override for a non-standard relay rather than a required setting. TLS is provided by rustls, so building needs no OpenSSL.

#### Opting out (both modes)

Every account carries `notify_new_location`, an opt-out that is on by default and is checked before an alert is routed anywhere, so an account that has opted out is never alerted on. `GET /api/me` returns the current value and `PATCH /api/me` changes it: `{"notify_new_location": false}` turns the alerts off and `{"notify_new_location": true}` turns them back on. The account is always the one holding the session (the bearer token in standalone mode, the `rus_session` cookie in saas mode), so a user id in the request body is ignored and cannot flip another account's setting. Omitting the key means "not submitted" and leaves the stored value alone, while a non-boolean value is rejected with a 400 rather than coerced. The dashboard control that drives this from the browser is tracked in RUS-18.

### Standalone only

| Variable | Description | Default |
|----------|-------------|---------|
| `JWT_SECRET` | Base64-encoded 32-byte secret for JWT signing | **Required** |
| `JWT_EXPIRY` | JWT expiry in hours | `1` |
| `REFRESH_TOKEN_EXPIRY` | Refresh token expiry in days | `7` |
| `ACCOUNT_LOCKOUT_ATTEMPTS` | Failed attempts before lockout | `5` |
| `ACCOUNT_LOCKOUT_DURATION` | Lockout duration in minutes | `30` |
| `ALLOW_REGISTRATION` | Allow public signups | `true` |

### SaaS only

| Variable | Description | Default |
|----------|-------------|---------|
| `SAAS_JWT_SECRET` | JWT secret for validating parent app tokens | **Required** |

## Database Schema

### users (standalone only)
- `userID` - Primary key
- `username` - Unique username
- `password` - Argon2id hashed password (legacy bcrypt hashes migrated on login)
- `is_admin` - Admin flag (0/1)
- `created_at` - Account creation timestamp
- `last_login_country` - Country of the most recent resolved sign-in, for new-location detection
- `notify_new_location` - Whether sign-ins to this account raise a new-location alert (default 1); the account sets it via `PATCH /api/me`

### urls
- `id` - Primary key
- `user_id` - Foreign key to users
- `original_url` - The original long URL
- `short_code` - Unique 6-character code (indexed)
- `name` - Optional custom name
- `clicks` - Click counter
- `created_at` - URL creation timestamp

### click_history
- `id` - Primary key
- `url_id` - Foreign key to urls
- `clicked_at` - Click timestamp

### refresh_tokens (standalone only)
- `id` - Primary key
- `user_id` - Foreign key to users
- `token` - Unique refresh token
- `expires_at` - Expiry timestamp

### login_attempts (standalone only)
- `id` - Primary key
- `username` - Attempted username
- `attempted_at` - Attempt timestamp
- `success` - Whether login succeeded (0/1)

### abuse_reports
- `id` - Primary key
- `short_code` - Reported URL code
- `reporter_email` - Optional reporter email
- `reason` - Report reason
- `description` - Optional description
- `status` - Report status (pending/resolved)
- `created_at`, `resolved_at`, `resolved_by`

## Technology Stack

- **[Actix-web](https://actix.rs/)** - High-performance web framework
- **[SQLite](https://www.sqlite.org/)** - Embedded database via rusqlite (bundled)
- **[jsonwebtoken](https://github.com/Keats/jsonwebtoken)** - JWT authentication
- **[Argon2id](https://en.wikipedia.org/wiki/Argon2)** - Password hashing (standalone)
- **[actix-governor](https://github.com/AaronErber/actix-governor)** - Rate limiting
- **[qrcode](https://github.com/kennytm/qrcode-rust)** - QR code generation
- **[Serde](https://serde.rs/)** - Serialization/deserialization
- **[Tokio](https://tokio.rs/)** - Async runtime

## Development

```bash
just dev                     # Traefik-routed instance (standalone)
just dev saas                # Traefik-routed instance (saas)
just dev-local               # Local dev with hot-reload
just test                    # Run tests (standalone)
just test-saas               # Run tests (saas)
just lint                    # Clippy
just fmt                     # Format
```

### Short Code Generation
- 6-character alphanumeric codes (A-Z, a-z, 0-9)
- 62^6 = ~56.8 billion possible combinations
- Collision detection ensures unique codes

## Security

- JWT-based authentication with short-lived tokens
- Argon2id password hashing (with transparent bcrypt migration on login)
- Refresh token rotation
- Account lockout after configurable failed attempts
- Rate limiting on API endpoints
- Protected API endpoints with user-scoped access
- SQL injection prevention via parameterized queries
- Foreign key enforcement enabled
- Outbound alert mail encrypted by default (STARTTLS; plaintext only by explicit opt-in)

## Contributing

Contributions are welcome! Feel free to:
- Report bugs
- Suggest features
- Submit pull requests

## License

This project is open source and available under the MIT License.

---

**Made with Rust**
