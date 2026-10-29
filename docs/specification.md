# rus Specification

## 1. Purpose and scope

rus is a URL shortener with a small browser front end. Signed-in users create short links, view their links and click
history, download QR codes, and rename or delete links. Anyone can follow a short link and anyone can report a link as
abusive. Administrators review reports and manage users.

It is built in one of two modes selected at compile time with a Cargo feature (section 2). It does not:

- Check where a link points at redirect time beyond the validation applied when the link is created.
- Store link contents or serve any content other than redirects, its own pages and QR images.
- Resolve geolocation itself; the country of a sign-in comes from a header set by a trusted proxy.

## 2. Architecture

A single Rust package (edition 2021) that builds a library `rus` and one binary entry point (`src/main.rs`) compiled as
two binaries by feature:

| Feature                | Binary     | Authentication                                                                   |
| ---------------------- | ---------- | -------------------------------------------------------------------------------- |
| `standalone` (default) | `rus`      | Local accounts with argon2 password hashes, JWT access tokens and refresh tokens. |
| `saas`                 | `rus-saas` | OpenID Connect: the app is a relying party (browser sessions in an HttpOnly cookie) and a resource server. |

`testing` is a feature that compiles test helpers into the library.

Tech stack: actix-web 4, actix-governor (rate limiting), rusqlite with bundled SQLite (single connection behind a mutex),
jsonwebtoken, argon2 (and bcrypt for legacy hashes), lettre-style SMTP mail, qrcode and image, tracing, dotenvy.

Components:

- `src/main.rs`: loads `.env`, initializes logging, builds `Config`, opens the database, mounts the route table, applies
  response headers, runs the HTTP server.
- `src/routes.rs`: the single route table used by the binary and by the integration tests.
- `src/handlers`: URL, auth, abuse, admin, page and webhook handlers.
- `src/auth`: JWT creation and validation and the bearer middlewares (standalone).
- `src/oidc`: relying party, token verifier, session store, just-in-time provisioning (saas).
- `src/location_alert.rs`, `src/login_approval.rs`, `src/mailer.rs`: new-country sign-in alert and approval gate.
- `src/db.rs`, `src/models.rs`, `src/security.rs`, `src/url`: storage, types, password and lockout rules, short code and
  URL validation, QR generation.
- `static/`: HTML, CSS and JavaScript served by the app.

Every response carries `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `X-XSS-Protection: 1; mode=block`
and `Referrer-Policy: strict-origin-when-cross-origin`. Requests are logged through `tracing-actix-web`.

## 3. Functional requirements

### 3.1 Short links

- A signed-in user submits a URL and receives a short code of 6 characters from `A-Z`, `a-z`, `0-9`. A code that already
  exists is regenerated until unused.
- Submitting a URL the same user has already shortened returns the existing code.
- A URL is accepted only if it is no longer than `MAX_URL_LENGTH` (default 2048), parses, uses `http` or `https`, and does
  not contain `javascript:`, `data:`, `file:`, `vbscript:` or `about:` (case-insensitive).
- `GET /{code}` answers `302` with `Location` set to the original URL, increments the link's click counter and records a
  row in click history. An unknown code answers `404` with an HTML page.
- About 1 in 100 redirects deletes click history older than `CLICK_RETENTION_DAYS` (default 30).
- A user can list their links (newest first), read stats for one, rename one, delete one, read its click history (the
  total counter and the newest 1000 click timestamps) and download its QR code as `png` or `svg`. All of these are limited
  to links the user owns; another user's code answers `404`.

### 3.2 Accounts (standalone)

- Registration requires a username of at least 3 characters from letters, digits, `_` and `-`, and a password of at least
  8 characters with an uppercase letter, a digit and a special character. An optional email is stored for security notices.
- The first registered account is an administrator. When `ALLOW_REGISTRATION` is false, registration is refused once any
  user exists.
- Login returns an access token and a refresh token. Repeated failures lock the account for a period
  (`ACCOUNT_LOCKOUT_ATTEMPTS`, `ACCOUNT_LOCKOUT_DURATION`). A legacy bcrypt hash is re-hashed to argon2 on successful login.
- A refresh token is exchanged for a new session; an unknown or expired one answers `401`.
- A user can read and update their own profile (email and the new-location alert opt-out).
- `GET /api/setup/required` reports whether no user exists yet.

### 3.3 Accounts (saas)

- Sign-in goes through the OpenID provider with the authorization code flow and PKCE. The callback creates a local
  session; the session token is an opaque random value in the `rus_session` HttpOnly cookie, stored hashed.
- A local user is provisioned on first sign-in from the token's subject; the administrator flag follows the provider's
  `role` claim on every sign-in.
- The app accepts back-channel logout tokens and lifecycle event tokens from the provider, de-duplicated by token id.
- A signed webhook turns maintenance mode on or off. While it is on, requests other than the allowlisted paths, OIDC
  routes, webhook routes and administrator sessions answer `503`.

### 3.4 Sign-in location alert and approval gate

- If SMTP is configured, a sign-in from a country the account has not used before sends an alert to the account's email,
  or to `SECURITY_ALERT_EMAIL` when the account has none. Otherwise the alert is logged. An account can opt out.
- When `LOGIN_APPROVAL_ENABLED` is exactly `true`, such a sign-in is held until the owner opens a single-use emailed link
  (valid 15 minutes). A first-ever sign-in and a sign-in with an unresolved country are never held. When no recipient or
  no SMTP is configured the sign-in is allowed through.
- The country is read from `X-IPCountry`, and the client address from `X-Forwarded-For` / `X-Real-IP`, only when the
  socket peer is within `TRUSTED_PROXY_CIDRS`.

### 3.5 Abuse reports

- Anyone can report a short code with a reason, optional description and optional reporter email. The code must exist; an
  empty code or reason, or an email without `@`, answers `400`; an unknown code answers `404`; success answers `201`.
- In standalone mode administrators list reports and mark each `resolved` or `dismissed` (only a `pending` report can be
  resolved).

### 3.6 Administration (standalone)

- Administrators list users, delete a user (not themselves), promote a user to administrator (an existing administrator
  answers `400`), and read aggregate statistics.
- In a debug build, `SETUP_DEFAULT_ADMIN=email:password` seeds an administrator while none exists (standalone only).

## 4. Interfaces

### 4.1 HTTP routes

Rate limits are per peer: 5 requests per minute (burst 5) on register and login; 30 requests per minute (burst 30) on the
abuse report and login-approval endpoints.

| Route                                  | Method       | Mode        | Auth                     | Purpose                                   |
| -------------------------------------- | ------------ | ----------- | ------------------------ | ----------------------------------------- |
| `/api/register`                        | POST         | standalone  | none                     | Create an account.                        |
| `/api/login`                           | POST         | standalone  | none                     | Sign in.                                  |
| `/api/refresh`                         | POST         | standalone  | refresh token            | Issue a new session.                      |
| `/api/setup/required`                  | GET          | standalone  | none                     | Whether first-run setup is needed.        |
| `/api/config`                          | GET          | both        | none                     | Public front-end config (auth mode, limits, registration flag; in saas the login and logout URLs, OIDC enabled flag, maintenance state). |
| `/api/version`                         | GET          | both        | none                     | Version information.                      |
| `/health`                              | GET          | both        | none                     | Status, version, git tag and hash, build date, uptime. |
| `/api/report-abuse`                    | POST         | both        | none                     | Submit an abuse report.                   |
| `/approve-login.html`, `/api/login-approval` | GET, POST | both       | emailed single-use token | Release a held sign-in.                   |
| `/api/admin/users`                     | GET          | standalone  | bearer, admin            | List users.                               |
| `/api/admin/users/{user_id}`           | DELETE       | standalone  | bearer, admin            | Delete a user.                            |
| `/api/admin/users/{user_id}/promote`   | POST         | standalone  | bearer, admin            | Promote a user.                           |
| `/api/admin/stats`                     | GET          | standalone  | bearer, admin            | Aggregate counts.                         |
| `/api/admin/reports`                   | GET          | standalone  | bearer, admin            | List abuse reports.                       |
| `/api/admin/reports/{report_id}`       | POST         | standalone  | bearer, admin            | Resolve or dismiss a report.              |
| `/api/me`                              | GET, PATCH   | both        | bearer (standalone) or session cookie (saas) | Own profile.          |
| `/api/shorten`                         | POST         | both        | as above                 | Create a short link.                      |
| `/api/urls`                            | GET          | both        | as above                 | List own links.                           |
| `/api/urls/{code}`                     | DELETE       | both        | as above                 | Delete a link.                            |
| `/api/urls/{code}/name`                | PATCH        | both        | as above                 | Rename a link.                            |
| `/api/urls/{code}/clicks`              | GET          | both        | as above                 | Click history.                            |
| `/api/urls/{code}/qr/{format}`         | GET          | both        | as above                 | QR code, `png` or `svg`.                  |
| `/api/stats/{code}`                    | GET          | both        | as above                 | Stats for one link.                       |
| `/oauth2/login`, `/oauth2/callback`, `/oauth2/logout` | GET | saas   | none / provider          | OIDC relying party flow.                  |
| `/oauth2/backchannel-logout`, `/oauth2/lifecycle-event` | POST | saas | provider-signed token  | Provider events.                          |
| `/webhooks/maintenance`                | POST         | saas        | HMAC signature header `X-Webhook-Signature` | Toggle maintenance mode. |
| `/dev/seed-session`, `/dev/logout`     | GET          | saas, debug builds only | none        | Local development sessions.               |
| `/`, `/dashboard.html`, `/report.html`, `/styles.css`, `/theme.js`, `/k9f3x2m7.js` | GET | both | none (dashboard redirects to login in saas without a session) | Pages and assets. |
| `/login.html`, `/signup.html`, `/setup.html`, `/admin.html` | GET | standalone | none | Pages.                |
| `/{code}`                              | GET          | both        | none                     | Redirect (registered last).               |

Errors from API handlers are JSON `{"error": "<message>"}` with a matching status code.

## 5. Data model

SQLite file at `DB_PATH`. Tables are created at startup with `CREATE TABLE IF NOT EXISTS`, and columns added later are
applied with `ALTER TABLE ... ADD COLUMN` at startup; there is no migrations directory.

| Table                    | Mode       | Key fields                                                                                  |
| ------------------------ | ---------- | ------------------------------------------------------------------------------------------- |
| `users`                  | both       | `userID`, `username` (unique), `password`, `is_admin`, `email`, `last_login_country`, `notify_new_location`; saas adds `saas_user_id` (unique), `suspended_at`, `session_version`. |
| `urls`                   | both       | `id`, `user_id`, `original_url`, `short_code` (unique), `name`, `clicks`, `created_at`.      |
| `click_history`          | both       | `id`, `url_id` (cascade), `clicked_at`.                                                      |
| `abuse_reports`          | both       | `id`, `short_code`, `reporter_email`, `reason`, `description`, `status` (default `pending`), `created_at`, `resolved_at`, `resolved_by`. |
| `refresh_tokens`         | standalone | `user_id`, `token` (unique), `expires_at`.                                                   |
| `login_attempts`         | standalone | `username`, `attempted_at`, `success`.                                                       |
| `rp_sessions`            | saas       | Authorization-flow state including the PKCE `code_verifier`, with `expires_at`.              |
| `user_sessions`          | saas       | `user_id`, `session_token_hash` (unique blob), `expires_at`.                                  |
| `pending_login_approvals`| both       | `user_id`, `token_hash` (unique), `country`, `ip`, `user_agent`, `created_at`, `expires_at`, `consumed_at`. |

## 6. Configuration

Environment variables, optionally from a `.env` file. Unparseable numeric values fall back to the default.

| Variable                                  | Mode       | Default                  | Meaning                                              |
| ----------------------------------------- | ---------- | ------------------------ | ---------------------------------------------------- |
| `APP_HOST`, `APP_PORT`                    | both       | `0.0.0.0`, `4001`        | Bind address.                                        |
| `HOST_URL`                                | both       | `http://localhost:4001`  | Public base URL; also the base of OIDC defaults.     |
| `DB_PATH`                                 | both       | `./data/rus.db`          | SQLite file.                                         |
| `MAX_URL_LENGTH`                          | both       | 2048                     | Longest accepted URL.                                |
| `CLICK_RETENTION_DAYS`                    | both       | 30                       | Click history retention.                             |
| `TRUSTED_PROXY_CIDRS`                     | both       | empty                    | CIDRs or addresses whose forwarded headers are trusted; unparseable entries are dropped with a warning. |
| `JWT_SECRET`                              | standalone | built-in placeholder     | JWT signing secret; a warning is logged when unset.  |
| `JWT_EXPIRY`                              | standalone | 1 (hours)                | Access token lifetime.                               |
| `REFRESH_TOKEN_EXPIRY`                    | standalone | 7 (days)                 | Refresh token lifetime.                              |
| `ACCOUNT_LOCKOUT_ATTEMPTS`                | standalone | 5                        | Failed logins before lockout.                        |
| `ACCOUNT_LOCKOUT_DURATION`                | standalone | 30 (minutes)             | Lockout window.                                      |
| `ALLOW_REGISTRATION`                      | standalone | true                     | Allow registration after the first user.             |
| `SETUP_DEFAULT_ADMIN`                     | standalone | unset                    | `email:password`, honored in debug builds only.      |
| `OIDC_ISSUER`                             | saas       | empty (OIDC disabled)    | Issuer URL.                                          |
| `OIDC_AUDIENCE`                           | saas       | `<HOST_URL>/api`         | Expected `aud` of access tokens.                     |
| `OIDC_JWKS_URL`                           | saas       | derived from issuer      | Key set URL.                                         |
| `OIDC_JWKS_CACHE_TTL`                     | saas       | 300 s                    | Key cache lifetime.                                  |
| `OIDC_CLIENT_ID`, `OIDC_CLIENT_SECRET`    | saas       | empty                    | Relying-party client credentials.                    |
| `OIDC_REDIRECT_URI`                       | saas       | `<HOST_URL>/oauth2/callback` | Callback URL.                                    |
| `OIDC_POST_LOGOUT_REDIRECT_URI`           | saas       | `<HOST_URL>/`            | Post-logout URL.                                     |
| `OIDC_LEEWAY_SECONDS`                     | saas       | 30                       | Clock-skew allowance.                                |
| `OIDC_LIFECYCLE_JTI_CACHE_TTL`            | saas       | 300 s                    | Event de-duplication window.                         |
| `OIDC_SESSION_TTL_SECONDS`                | saas       | 1209600 (14 days)        | Browser session lifetime.                            |
| `WEBHOOK_SECRET`                          | saas       | empty                    | HMAC secret for the maintenance webhook.             |
| `SMTP_HOST`, `SMTP_PORT`, `SMTP_USERNAME`, `SMTP_PASSWORD`, `SMTP_FROM_EMAIL`, `SMTP_FROM_NAME` | both | unset | Outbound mail. Mail is sent only when a host and sender are set. |
| `SMTP_TLS_MODE`                           | both       | `starttls`               | `starttls`, `tls` or `none`; an unrecognized value warns and uses `starttls`. |
| `SECURITY_ALERT_EMAIL`                    | both       | unset                    | Fallback recipient for sign-in alerts.               |
| `LOGIN_LOCATION_ALERTS_ENABLED`           | both       | on                       | Off when `0`, `false` or `no`.                       |
| `LOGIN_APPROVAL_ENABLED`                  | both       | off                      | On only when exactly `true` (case-insensitive).      |
| `RUST_LOG`                                | both       | `info,rus=debug`         | Log filter.                                          |

## 7. Security

- Passwords are hashed with argon2; legacy bcrypt hashes are verified and upgraded. Password complexity is enforced at
  registration (section 3.2).
- Standalone protected routes require a bearer JWT; `/api/admin` additionally requires the admin claim. Saas protected
  routes require a valid server-side session; the session token is random, stored hashed, and sent only in an HttpOnly
  cookie.
- Account lockout and per-peer rate limits apply to login and registration.
- Redirect targets are validated at creation (scheme and blocked patterns).
- Forwarded client address and country headers are honored only from configured trusted proxies.
- The maintenance webhook is authenticated by an HMAC signature; OIDC tokens are verified against the provider's keys.
- The approval link token is single use, stored hashed and expires after 15 minutes.
- Ownership checks limit every per-link API to the owner.

## 8. Integrations and dependencies

- An OpenID Connect provider (saas): authorization, token, key set, back-channel logout and lifecycle events.
- An SMTP server (optional) for sign-in alerts and approval links.
- A reverse proxy that supplies `X-IPCountry` and forwarded address headers (optional).

## 9. Deployment and operations

- `oci-build/Dockerfile` builds with `BUILD_MODE=standalone` (default) or `BUILD_MODE=saas` and produces a minimal Alpine
  image that runs as an unprivileged user, sets `DB_PATH=/data/rus.db`, exposes port 4001 and runs `/app/app`.
- `compose.dev.yml` and `compose.yml` define the development service (port 4001 published on loopback by default, a named
  data volume at `/data`, source mounted for rebuilds).
- `/health` is the liveness endpoint.
- Tasks run through `just`; `.env.standalone` and `.env.saas` are the per-mode environment templates.

## 10. Non-functional behavior

- Logging: structured `tracing` output; the level comes from `RUST_LOG`.
- Startup prints a configuration banner; failure to open the database stops startup.
- All database access goes through one SQLite connection guarded by a mutex.
