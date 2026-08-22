# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Rust URL Shortener (RUS) - A URL shortening service with JWT authentication, SQLite persistence, and click tracking. Built with Actix-web.

## Build Modes

RUS supports two build modes controlled by Cargo feature flags:

### Standalone Mode (default)
Full-featured URL shortener with built-in user management:
- User registration and login with JWT authentication
- Password hashing with Argon2id (legacy bcrypt hashes migrated on login)
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
# Dev server — Traefik-routed (see "Per-Developer Instances" below)
just dev                               # Standalone mode (default)
just dev saas                          # SaaS mode
just dev-stop                          # Stop containers
just dev-clean                         # Remove containers and volumes

# Local dev server — cargo-watch on localhost:4001
just dev-local                         # Start with hot-reload
just dev-local-stop                    # Stop containers
just dev-local-clean                   # Remove containers and volumes

# Build, test, lint
just build                             # Release build (standalone)
just build-saas                        # Release build (saas)
just test                              # Run tests (standalone)
just test-saas                         # Run tests (saas)
just test-js                           # Static page tests (static/tests, Node container)
just lint                              # Clippy (standalone)
just fmt                               # Format code
just pre-commit                        # Every CI check: the seven cargo steps plus the static page tests
```

## Architecture

### Backend (modular structure)
- **Framework**: Actix-web 4.4 with Tokio async runtime
- **Database**: SQLite via rusqlite (bundled)
- **Auth (standalone)**: JWT tokens with Argon2id password hashing
- **Auth (saas)**: Cookie-based auth from parent application
- **Storage**: `./data/rus.db` locally (auto-created), `/data/rus.db` in Docker (set via `ENV DB_PATH`)

### Source Structure
```
src/
├── main.rs           # Entry point, route configuration
├── config.rs         # Environment-based configuration
├── db.rs             # Database connection and schema
├── models.rs         # Data models and request/response types
├── security.rs       # Password validation, account lockout (standalone only)
├── location_alert.rs # New-sign-in-country detection, trusted-proxy gate (both modes)
├── login_approval.rs # New-country sign-in gate, approval page and API (both modes)
├── mailer.rs         # Security alert email via SMTP, TLS by default (both modes)
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
- JWT stored in localStorage (standalone); saas authenticates with the `rus_session` cookie instead
- Pages: index.html (landing), login.html, signup.html, dashboard.html, admin.html, setup.html, report.html, 404.html, maintenance.html
- k9f3x2m7.js (auth.js) handles token management
- dashboard.html carries an Account section over `GET`/`PATCH /api/me`: the security alert address (standalone only, RUS-17) and the new-location alert opt-out (both modes, RUS-18). Its `apiFetch` helper picks the bearer token or the cookie from the `auth_mode` that `/api/config` reports, so one page serves both legs
- `static/tests/` covers that page logic (RUS-20). `dom.mjs` extracts a page's real `<script>` tags, resolving `k9f3x2m7.js` back to `auth.js` the way `serve_auth_js` does, and evaluates them in a `node:vm` context wired to a stub DOM, `fetch` and `localStorage`, so the tests drive the shipped page rather than a copy of its logic. Covered: the Account section painting and its changed-fields-only PATCH in both auth modes, the bearer-versus-cookie split, the 400 path, and signup's optional address
- Run them with `just test-js`, or as the eighth step of `just pre-commit`; CI runs the same entry point in `.forgejo/workflows/check.yml`. `static/tests/run.mjs` is that entry point: it discovers the `*.test.mjs` files itself and exits non-zero when fewer than the expected number of tests report, because `node --test` exits 0 when its arguments match no file

### API Structure
- **Public**: `/api/register`, `/api/login`, `/api/login-approval` (GET, POST), `/{short_code}` (redirect)
- **Protected** (Bearer token): `/api/me` (GET, PATCH), `/api/shorten`, `/api/urls`, `/api/stats/{code}`, `/api/urls/{code}` (DELETE), `/api/urls/{code}/name` (PATCH)

### Key Implementation Details
- Short codes: 6 chars (A-Za-z0-9), collision-checked
- JWT claims: `sub` (username), `user_id`, `exp`
- Database: Single Mutex-wrapped connection (not production-grade)
- Password hashing: Argon2id with default parameters (legacy bcrypt verified and rehashed on login)

## Environment Variables

### Required (standalone mode only)
```
JWT_SECRET=<base64-encoded-32-bytes>
```

### Optional (both modes)
```
DB_PATH=./data/rus.db       # Database location (Docker default: /data/rus.db)
APP_HOST=0.0.0.0            # Bind address (avoids collision with system HOST variable)
APP_PORT=4001               # Server port
HOST_URL=http://localhost:4001  # Public URL for shortened links
MAX_URL_LENGTH=2048         # Maximum URL length
CLICK_RETENTION_DAYS=30     # Days to retain click history
TRUSTED_PROXY_CIDRS=        # Peers allowed to set the forwarded IP and country headers
RUST_LOG=info,rus=debug     # Log level filter (default: info,rus=debug)
```

### Security alerts (both modes)
```
SECURITY_ALERT_EMAIL=          # Fallback mailbox for an account with no address
LOGIN_LOCATION_ALERTS_ENABLED=true  # Alert kill switch (default: true; false/0/no disables)
LOGIN_APPROVAL_ENABLED=false   # Hold a new-country sign-in for approval (default: off; only "true" enables)
SMTP_HOST=                     # SMTP_HOST and SMTP_FROM_EMAIL are required before
SMTP_FROM_EMAIL=               # anything can be sent; otherwise it is logged instead
SMTP_USERNAME=
SMTP_PASSWORD=
SMTP_FROM_NAME=
SMTP_TLS_MODE=starttls         # starttls (default, encrypted) | tls | none
SMTP_PORT=                     # Override; starttls 587, tls 465, none 25
```

The new-sign-in-location alert is routed to the account owner (RUS-11), in this precedence: the account's own `users.email` when set (a personal security notice), else `SECURITY_ALERT_EMAIL` (which still names which account signed in, since its reader is not the owner), else the would-be alert is logged and nothing is sent. An unconfigured SMTP transport also falls to the log-only case, so a login is never failed by mail configuration. The per-account `notify_new_location` opt-out is evaluated first and suppresses the alert on every route. The resolution lives in `mailer::resolve_recipient`, and the decision (opt-out, then recipient) in `location_alert::alert_route`.

`users.email` is nullable in both schemas: saas ships it and populates it from the OP identity on every login (`src/oidc/jit.rs`), standalone gets it from an idempotent `ALTER TABLE` in `src/db.rs` and the account sets its own via `POST /api/register` (optional `email` field) or `PATCH /api/me`. Blank stores NULL rather than an empty string, and a stored value is re-validated on read, because SSO writes `""` when the OP omits the claim.

The country comes from the `X-IPCountry` header injected by the reverse proxy's geoblock middleware, not an in-process geoip database, so with no such edge no country resolves and no alert fires.

### New-country sign-in gate (both modes)

`LOGIN_APPROVAL_ENABLED=true` upgrades the RUS-7 alert from a notice to a gate (RUS-19): a sign-in from a country the account has not used before is held, no session is minted, and a single-use emailed link releases it. Only the exact string `true` enables it; every other value leaves it off, because a misfiring gate is a lockout rather than noise. The decision lives in `src/login_approval.rs` (`gate_decision`, `gate_login`), the suspicion predicate is shared with the alert (`location_alert::is_new_country`), and the page plus API are `static/approve-login.html`, `GET /api/login-approval` and `POST /api/login-approval`.

Never held, and each of these has a route-level test in both legs: a first-ever sign-in (no prior country), a sign-in whose country does not resolve, a sign-in from the same country, and anything at all while the switch is off. A held sign-in writes `pending_login_approvals` before the mail goes out, stores only a SHA-256 of a 256-bit token, expires in 15 minutes, and is claimed by a guarded `UPDATE ... WHERE token_hash = ? AND consumed_at IS NULL AND expires_at > ?` whose affected-row count decides a concurrent race. GET validates, POST claims, so a link scanner cannot burn the owner's link. `last_login_country` is written only once a sign-in completes, so an unapproved attempt cannot seed a familiar country. The per-account `notify_new_location` opt-out deliberately does NOT disable the gate.

Deliverability is part of the decision: with no recipient resolvable (no account address and no `SECURITY_ALERT_EMAIL`) or no SMTP configured, the sign-in is ALLOWED and logged loudly rather than held, because a gate whose link can never arrive is a permanent lockout. A delivery that is configured and merely fails is different and answers 500 with nothing issued.

**Recovery, both mail-independent:** set `LOGIN_APPROVAL_ENABLED=false` and restart (global), or `UPDATE users SET last_login_country = NULL WHERE username = '...'` so the next sign-in is a first-ever one (per account, no restart). Documented in README under "Recovering when the approval mail cannot arrive".

**The approval page and its API must stay mounted outside every guard.** `main.rs` calls `.configure(login_approval::configure_routes)` before the guarded `/api` scope and before the `/{code}` catch-all in BOTH legs, and `maintenance_guard` allowlists both paths. Whoever follows the emailed link has no session by construction. auto-buyer's AB-67 shipped the same feature with these routes behind its auth middleware and the gate became a lockout, invisible because its tests called the handlers directly. `login_approval::tests` therefore asserts reachability through a real guarded app in both legs, and `main_mounts_the_approval_routes_ahead_of_every_guard` reads `src/main.rs` itself to check the mount order.

### Trusted proxies (both modes)

`TRUSTED_PROXY_CIDRS` is a comma-separated list of CIDRs or bare IPs whose socket peers may set `X-Forwarded-For`, `X-Real-IP`, and `X-IPCountry`. The socket peer is the only input a client cannot forge, so it gates all three: an untrusted peer's forwarded headers are ignored and its peer address is used, and its `X-IPCountry` resolves to `None`, so a direct client can neither spoof its IP nor spoof or suppress the sign-in-location alert. A trusted peer's `X-Forwarded-For` is walked right to left for the rightmost entry that is not itself a trusted proxy, then `X-Real-IP`, then the peer. Entries that do not parse log a warning and are skipped. The resolvers live in `src/location_alert.rs` (`resolve_client_ip`, `resolve_client_country`); the parse and the `Config` field live in `src/config.rs`; `main.rs` installs the set once at startup via `init_trusted_proxies`.

**Empty means trust nothing, which breaks a proxied deployment if left unset.** rus runs behind Traefik on a private Docker network, so an unset list collapses every client to the proxy address and silently disables the sign-in-location alert (an untrusted peer never yields a country). `.env.standalone` and `.env.saas` ship with the private ranges (`10.0.0.0/8,172.16.0.0/12,192.168.0.0/16,fd00::/8`) set; a compose deployment that supplies its environment directly must set the same value. Startup logs a warning whenever the list is empty.

### Account alert opt-out (both modes)

`users.notify_new_location` is a per-account opt-out for the new-sign-in-location alert, on by default and checked before the alert is routed anywhere. It is not an environment variable: the account changes it itself with `PATCH /api/me` (`{"notify_new_location": false}` off, `true` on), and reads it back from `GET /api/me`. Both feature legs carry the pair (`update_current_user` / `get_current_user` in `src/handlers/auth.rs`, `saas_update_me` / `saas_me` in `src/handlers/saas_auth.rs`), and both derive the account from the session rather than the request body, so a user id in the payload is ignored. An absent key means "not submitted" and leaves the stored value alone; an explicit `false` persists; a non-boolean is a 400 rather than a coercion. The queries sit with the rest of the alert's SQL in `src/location_alert.rs` (`get_notify_new_location`, `set_notify_new_location`). The browser control is the Account section of `static/dashboard.html`, which loads both account fields from `GET /api/me` and submits only the ones the user changed, so a toggle save never carries `email` and cannot clear the address.

### Standalone-only options
```
JWT_EXPIRY=1                # JWT expiry in hours (default: 1)
REFRESH_TOKEN_EXPIRY=7      # Refresh token expiry in days (default: 7)
ACCOUNT_LOCKOUT_ATTEMPTS=5  # Failed attempts before lockout (default: 5)
ACCOUNT_LOCKOUT_DURATION=30 # Lockout duration in minutes (default: 30)
ALLOW_REGISTRATION=true     # Allow public signups (default: true)
```

### SaaS-only options (OIDC SSO + webhook)
```
# OIDC SSO (BFF Authorization Code + PKCE flow)
OIDC_ISSUER=https://api.a8n.tools          # Empty disables /oauth2/* routes
OIDC_AUDIENCE=<HOST_URL>/api               # `aud` claim in at+jwt tokens
OIDC_JWKS_URL=                             # Default: <issuer>/.well-known/jwks.json
OIDC_JWKS_CACHE_TTL=300                    # JWKS cache TTL (seconds)
OIDC_CLIENT_ID=                            # Required when OIDC_ISSUER is set
OIDC_CLIENT_SECRET=                        # Or mount at /run/secrets/oidc_client_secret
OIDC_REDIRECT_URI=<HOST_URL>/oauth2/callback
OIDC_POST_LOGOUT_REDIRECT_URI=<HOST_URL>/
OIDC_LEEWAY_SECONDS=30                     # Clock-skew tolerance
OIDC_LIFECYCLE_JTI_CACHE_TTL=300           # Idempotency window for lifecycle/logout events
OIDC_SESSION_TTL_SECONDS=1209600           # `rus_session` cookie lifetime (14 days)

# Maintenance webhook (HMAC-SHA256 signed; previously reused SAAS_JWT_SECRET)
WEBHOOK_SECRET=                            # Required to validate /webhooks/maintenance
```

The legacy `SAAS_JWT_SECRET`, `SAAS_LOGIN_URL`, `SAAS_LOGOUT_URL`, `SAAS_MEMBERSHIP_URL`, and `SAAS_REFRESH_URL` env vars from the deprecated cookie-JWT path have been removed.

**Important:** When adding or changing environment variables, update both `.env.standalone` and `.env.saas` to keep them in sync. Shared variables go in both files; mode-specific variables go only in the relevant file.

## Database Schema

**users**: userID, username (unique), password (hashed), created_at, last_login_country, notify_new_location (per-account alert opt-out, set via `PATCH /api/me`)
**urls**: id, user_id (FK), original_url, short_code (unique indexed), name, clicks, created_at

## Testing API

```bash
# Register
curl -X POST http://localhost:4001/api/register \
  -H "Content-Type: application/json" \
  -d '{"username":"test","password":"password123"}'

# Login (returns token)
curl -X POST http://localhost:4001/api/login \
  -H "Content-Type: application/json" \
  -d '{"username":"test","password":"password123"}'

# Shorten URL (protected)
curl -X POST http://localhost:4001/api/shorten \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer {TOKEN}" \
  -d '{"url":"https://example.com"}'
```

## Per-Developer Instances

Each developer gets their own instance at `https://{USER}-rus.a8n.run`, where `{USER}` is your OS username. This uses `compose.dev.yml` with Traefik for HTTPS routing.

**Prerequisites:**
- The `network-traefik-public` Docker network must exist (`docker network create network-traefik-public`)
- Traefik must be running and configured with the `cert-cloudflare` resolver
- DNS wildcard for `*.a8n.run` pointing to the host

**How it works:**
- `just dev` builds the production image (not cargo-watch) and starts it behind Traefik
- `just dev saas` does the same but builds in SaaS mode and copies `.env.saas`
- `HOST_URL` is automatically set to `https://{USER}-rus.a8n.run`
- Data is isolated per-developer in the `rus-data-{USER}` volume

**If you don't have Traefik**, use `just dev-local` for a localhost:4001 setup with cargo-watch hot-reload.

### Docker Compose Files
- **`compose.dev.yml`** — Per-developer Traefik instance (production Dockerfile, `oci-build/Dockerfile`)
- **`compose.yml`** — Local dev with cargo-watch (dev Dockerfile, `./Dockerfile`)

## Build System

A single `Dockerfile` builds both modes via `BUILD_MODE` ARG (`standalone` default, `saas`). The binary is copied to `/app/app` inside the container regardless of mode.

- **`docker build -t rus .`** — standalone image
- **`docker build --build-arg BUILD_MODE=saas -t rus-saas .`** — saas image

### Supporting scripts
- **`oci-build/setup.nu`**: Nushell build script (alternative to Dockerfile inline builds). Accepts `standalone` or `saas` as argument.
- **`oci-build/get-tags.nu`**: Derives image tags from `git describe`.

### CI
`.forgejo/workflows/check.yml` runs fmt, clippy, build and tests for both feature legs plus the static page tests on every push and pull request; the runner's Node is the `node24` binary, and the step fails rather than skipping when no runtime resolves. `.forgejo/workflows/build-oci-image.yml` builds both `rus` and `rus-saas` images in parallel via a matrix strategy.

### Container Directory Layout
- `/app` — binary and static files (`WORKDIR`)
- `/data` — persistent database storage (mount volume here)
- `/config` — reserved mount point for future configuration files (e.g., `/config/.env`)
