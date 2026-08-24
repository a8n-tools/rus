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
- **Account Settings** - Dashboard controls for the security alert address and the new-location alert opt-out
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
| `GET` | `/api/login-approval?token=` | Describe a sign-in held by the new-country gate, without claiming the link |
| `POST` | `/api/login-approval` | Claim the link and complete the held sign-in |
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
│   ├── login_approval.rs    # New-country sign-in gate, approval page and API
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
│   ├── dashboard.html       # URL management and account settings
│   ├── admin.html           # Admin panel
│   ├── report.html          # Abuse report form
│   ├── setup.html           # Initial setup page
│   ├── approve-login.html   # Approve a held new-country sign-in
│   ├── maintenance.html     # Maintenance-mode page (saas)
│   ├── 404.html             # Custom 404 error page
│   ├── theme.js             # Theme and contrast toggles
│   ├── styles.css           # Global styles
│   ├── auth.js              # Authentication utilities
│   └── tests/               # Node test harness for the pages (just test-js)
├── oci-build/
│   ├── setup.nu             # Nushell build script
│   └── get-tags.nu          # Image tag derivation from git describe
├── scripts/
│   └── check-cargo-tests-ran.nu  # Fails a cargo test run that tested nothing
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
| `LOGIN_APPROVAL_ENABLED` | Hold a new-country sign-in until it is approved by email. Only the exact value `true` enables it | `false` (off) |
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

In saas mode the address comes from the OP identity and is refreshed on every login, so there is nothing to set by hand. In standalone mode an account sets its own: optionally at registration (`POST /api/register` accepts an `email` field, and the signup form carries an optional input that sends it only when filled in), and at any time afterwards with `PATCH /api/me` (`{"email": "you@example.com"}`). A blank value clears it back to unset. `GET /api/me` returns the current value. The address is stored trimmed and lowercased and is not verified, so it receives security mail as soon as it is set. The browser surface is the Account section of the dashboard, which loads the stored address from `GET /api/me` and saves it with `PATCH /api/me`. That field is shown in standalone mode only: in saas mode the section explains instead that the address comes from the identity provider, since `PATCH /api/me` there ignores it.

The per-account `notify_new_location` opt-out is checked before any of this, so an account that has opted out is never alerted whichever way the message would have been routed.

The country comes from the `X-IPCountry` header injected by the reverse proxy's geoblock middleware, and is read only when the socket peer is listed in `TRUSTED_PROXY_CIDRS`. With no trusted proxy configured, or with a client connecting directly, no country resolves and no alert fires.

Mail is encrypted by default: `SMTP_TLS_MODE` defaults to `starttls`, so a deployment that sets nothing sends over an encrypted connection. `starttls` upgrades the connection on port 587, `tls` uses implicit TLS on port 465, and `none` is plaintext, kept only for a trusted loopback or sidecar relay and logging a warning naming the host whenever it is used. An unrecognised value warns and falls back to `starttls`. Each mode supplies its own default port, so `SMTP_PORT` is an override for a non-standard relay rather than a required setting. TLS is provided by rustls, so building needs no OpenSSL.

#### Requiring approval for a new-country sign-in (both modes)

The alert above is after the fact: it tells you a sign-in already happened. Someone who has taken over the mailbox reads that notice and deletes it, so on its own it does not stop them. Setting `LOGIN_APPROVAL_ENABLED=true` turns the same signal into a gate: a sign-in from a country the account has not used before is **held**, no session is created, and an emailed single-use link releases it.

**The switch is off by default and only the exact value `true` turns it on.** Every other value, including a typo, leaves it off, because this control can hold a real user out of their own account. It is the one variable in this file whose falsy spelling is deliberately not forgiving.

What is never held, in either mode:

- A first-ever sign-in, because the account has no prior country to differ from. Without this the first account ever created could never sign in.
- A sign-in whose country does not resolve, which is every sign-in when there is no geoblock edge in front of rus or `TRUSTED_PROXY_CIDRS` is unset. Without this those deployments would brick themselves.
- A sign-in from the same country as last time, compared case-insensitively.
- Anything at all while the switch is off.

The country used is the same one the alert uses (`X-IPCountry` from a trusted-proxy peer), and the comparison is the same predicate, so the alert and the gate can never disagree about what is suspicious.

When a sign-in is held: a row is written first, then the mail goes out, so a link that arrives always has a row behind it. The link carries a 256-bit token, is stored only as a SHA-256 hash, works exactly once, and expires 15 minutes after it is issued. Opening the link only *shows* what is being held. A separate confirmation claims it, so a mail gateway that fetches URLs out of messages cannot burn the link before its owner sees it. Approving signs the **approving** browser in; the browser that triggered the hold is never handed anything. `last_login_country` is written only once a sign-in actually completes, so an attempt that is never approved cannot make its country look familiar next time.

Who gets the link follows the same precedence as the alert: the account's own address when it has one, otherwise the `SECURITY_ALERT_EMAIL` operator mailbox, which then approves on the owner's behalf and is told so in the message.

**If no link could be delivered, the sign-in is allowed rather than held.** That covers an account with no address and no operator mailbox configured, and any deployment with no `SMTP_HOST` / `SMTP_FROM_EMAIL` at all. A gate whose approval can never arrive is a permanent lockout, so it degrades to the alert it was built on and logs a warning naming what to configure. This is deliberately different from a delivery that is configured and merely *fails*: that answers the sign-in with a 500 and issues nothing, because retrying is the fix.

The per-account `notify_new_location` opt-out does **not** disable the gate. That preference is written from an authenticated session, so honouring it here would let anyone holding a session switch off the control that defends the account against them, and would leave an opted-out account held with no mail to release it.

#### Recovering when the approval mail cannot arrive

Both of these work without any mail at all. Use them if a real user is held and the link is not reaching them.

1. **Turn the gate off globally.** Set `LOGIN_APPROVAL_ENABLED=false` (or remove it) and restart the container. Sign-ins complete as they did before RUS-19; the new-location alert keeps running.
2. **Clear one account's prior country.** `sqlite3 /data/rus.db "UPDATE users SET last_login_country = NULL WHERE username = 'alice';"` Their next sign-in is a first-ever one, which is never held, and it records a fresh baseline. This leaves the gate on for everyone else.

Option 2 is the per-account lever and needs no restart. Neither depends on SMTP, on the operator mailbox, or on the held row itself. Pending rows expire on their own after 15 minutes and are swept whenever a new one is written, so there is nothing to clean up by hand.

#### Opting out (both modes)

Every account carries `notify_new_location`, an opt-out that is on by default and is checked before an alert is routed anywhere, so an account that has opted out is never alerted on. `GET /api/me` returns the current value and `PATCH /api/me` changes it: `{"notify_new_location": false}` turns the alerts off and `{"notify_new_location": true}` turns them back on. The account is always the one holding the session (the bearer token in standalone mode, the `rus_session` cookie in saas mode), so a user id in the request body is ignored and cannot flip another account's setting. Omitting the key means "not submitted" and leaves the stored value alone, while a non-boolean value is rejected with a 400 rather than coerced. The browser control is a checkbox in the Account section of the dashboard, in both modes. It is painted from `GET /api/me` on load rather than from an assumed default, it submits only the fields the user actually changed (so saving the toggle never sends `email` and cannot disturb the address, and vice versa), and it repaints from the response body rather than from what was sent, so a rejected write leaves the checkbox on the value the server still holds and shows the API error inline.

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
| `OIDC_ISSUER` | OP issuer URL; empty disables the `/oauth2/*` routes | *(empty)* |
| `OIDC_CLIENT_ID` | Client id; **required** once `OIDC_ISSUER` is set | *(empty)* |
| `OIDC_CLIENT_SECRET` | Client secret, or mount it at `/run/secrets/oidc_client_secret`; **required** once `OIDC_ISSUER` is set | *(empty)* |
| `OIDC_AUDIENCE` | Expected `aud` claim on `at+jwt` tokens | `<HOST_URL>/api` |
| `OIDC_JWKS_URL` | Signing-key set | `<issuer>/.well-known/jwks.json` |
| `OIDC_JWKS_CACHE_TTL` | JWKS cache lifetime in seconds | `300` |
| `OIDC_REDIRECT_URI` | Authorization-code callback | `<HOST_URL>/oauth2/callback` |
| `OIDC_POST_LOGOUT_REDIRECT_URI` | Where the OP returns after logout | `<HOST_URL>/` |
| `OIDC_LEEWAY_SECONDS` | Clock-skew tolerance in seconds | `30` |
| `OIDC_LIFECYCLE_JTI_CACHE_TTL` | Idempotency window for lifecycle and logout events | `300` |
| `OIDC_SESSION_TTL_SECONDS` | `rus_session` cookie lifetime | `1209600` (14 days) |
| `WEBHOOK_SECRET` | HMAC-SHA256 key validating `/webhooks/maintenance` | *(empty; signatures never validate)* |

The saas leg signs no JWT of its own, so it has no counterpart to `JWT_SECRET`: the session arrives from the OP in the `rus_session` cookie, and both `JWT_SECRET` readers in `src/config.rs` are standalone-only.

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

### pending_login_approvals (both modes)
- `id` - Primary key
- `user_id` - Foreign key to users, cascading on delete
- `token_hash` - SHA-256 of the approval token, unique. The raw token is never stored
- `country` - Country the held sign-in came from
- `ip` - Client IP of the held sign-in
- `user_agent` - Device string of the held sign-in
- `created_at` - When the hold was placed (RFC 3339)
- `expires_at` - 15 minutes after that; a row past it is never claimable
- `consumed_at` - Set by the guarded update that claims the token, so a second click finds nothing to claim

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
just test                    # Guarded cargo tests, standalone leg only
just test-saas               # Guarded cargo tests, saas leg only
just test-js                 # Static page tests (static/tests, runs in a Node container)
just lint                    # Clippy
just fmt                     # Format
just pre-commit              # Every CI check: fmt, clippy and build per leg, the guarded cargo tests, the static page tests
```

`just test-js` and the matching `just pre-commit` step run node's built-in test
runner inside a pinned `node:24-alpine` container, so no Node install is needed
on the host. The tests evaluate each page's real inline script against a stub
DOM and `fetch`; `.forgejo/workflows/check.yml` runs the same entry point.

Both test harnesses exit 0 on a run that collected nothing, so both are held to
a minimum pass count. `scripts/check-cargo-tests-ran.nu` runs the cargo tests
for every feature leg and fails on a missing `test result:` line, zero passed,
any ignored or filtered-out case, or a total under the leg's floor: 270
standalone and 205 saas for the `--lib` scope `just pre-commit` uses, 285 and
205 for the all-targets scope CI and the single-leg recipes use. The legs come
from the `[[bin]]` `required-features` in `Cargo.toml`, so a new build mode is
covered as soon as its binary lands, and the guard fails if any recipe in the
`justfile` or any step in `check.yml` reaches the test harness outside it.
`just test` and `just test-saas` go through it too, with `--leg <name>`, which
errors listing the known legs rather than selecting nothing and still holds the
one leg it ran to that leg's floor. `static/tests/run.mjs` applies the same idea
to the page tests.

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
