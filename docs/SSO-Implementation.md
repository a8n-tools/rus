# SSO-Implementation

Implementation prompts for adding OIDC-based SSO to **rus** (Actix-web URL shortener), modeled after the canonical pattern used in the sibling projects.

## Reference repos

- **`../saas`** - the **identity provider** (OIDC OP). Issues Ed25519-signed `at+jwt` access tokens, ID tokens, back-channel logout tokens, and lifecycle event tokens. Hosts JWKS at `/.well-known/jwks.json`.
- **`../rusty-links`** - the **canonical consumer**. Same dual-mode build (`standalone` / `saas`) as rus, also Rust. Closest reference - copy from here unless noted otherwise.
- **`../dmarc-reporter`** - second consumer, slightly different (SeaORM, multi-tenant). Use as a sanity check, not a primary reference.

## Pattern at a glance

| Concern | Canonical choice |
|---|---|
| Flow | OIDC Authorization Code + PKCE (S256) |
| ID/access token signing | EdDSA (Ed25519) - keys via JWKS |
| Access token type | RFC 9068 `at+jwt`, `aud = <consumer audience URL>` |
| Local session | Opaque random token (32 bytes b64url) in HttpOnly cookie, SHA-256 hashed in DB |
| Cookie name | App-specific (`rl_session` in rusty-links) - use `rus_session` |
| Cookie attrs | `HttpOnly`, `Secure` (when redirect_uri starts with `https://`), `SameSite=Lax`, `Path=/` |
| User table change | `saas_user_id` (UUID, unique nullable), `suspended_at`, `session_version INT` |
| Session invalidation | Increment `users.session_version`; sessions store snapshot at issue time; mismatch = invalid |
| JIT provisioning | On callback: link existing user by email if `saas_user_id IS NULL`, else insert |
| Forbidden conditions | `!email_verified`, `!has_member_access`, `suspended_at IS NOT NULL` |
| Lifecycle events | `POST /oauth2/lifecycle-event` - `user.suspended`, `user.unsuspended`, `user.deleted`, `entitlement.revoked`, `entitlement.granted` |
| Back-channel logout | `POST /oauth2/backchannel-logout` per OIDC BCL 1.0 |
| JWKS cache | In-memory, TTL configurable (default 300s), lazy refresh on unknown `kid` |
| OIDC enabled toggle | Empty `OIDC_ISSUER` env var disables all `/oauth2/*` routes (404) |

## rus-specific deviations from rusty-links

These are forced by the existing rus stack. Stay aware of them when copying.

1. **Framework**: rus is **Actix-web 4**, rusty-links is **Axum**. Translate extractors (`State`, `Query`, `Form`, `CookieJar`) to Actix equivalents (`web::Data`, `web::Query`, `web::Form`, `actix_web::cookie::Cookie`).
2. **Database**: rus is **SQLite via rusqlite** (sync, single `Mutex<Connection>`), not Postgres + sqlx. Translate:
   - `UUID` -> `TEXT` (store as string).
   - `BYTEA` -> `BLOB`.
   - `TIMESTAMPTZ` -> `TEXT` (RFC3339) - rus already uses chrono `DateTime<Utc>`.
   - `gen_random_uuid()` -> generate in Rust with `uuid::Uuid::new_v4()`.
   - `NOW()` -> `Utc::now()` in Rust before binding.
   - `RETURNING` is supported in modern SQLite; if avoiding it, do a SELECT after UPDATE.
3. **Existing partial SaaS mode**: `src/handlers/saas_auth.rs` and the `saas` feature in `Cargo.toml` implement the **legacy HS256 shared-cookie pattern** (`SAAS_JWT_SECRET`). This is the deprecated approach. **Delete it** and replace with OIDC RP/RS as described below. Also remove the now-unused `hmac`, `sha2` (keep if used by JWKS code), `hex` optional deps if no longer referenced.
4. **`uuid` crate is not yet a dep** - add it.
5. rusty-links uses async sqlx; rus uses sync rusqlite. The Actix handler can `web::block(...)` for DB access if needed, but the existing rus codebase already runs sync queries inline under the `Mutex`. Match the existing style.

## Files to study before starting

Read these in full before writing any code:

- `../rusty-links/src/config.rs:1-250` - `OidcConfig` struct + `from_env` parsing including all defaults and the validation rules (issuer set => credentials required; JWKS must be HTTPS).
- `../rusty-links/src/auth/oidc_rs.rs` (entire file, 453 lines) - JWKS cache, token verifiers (`at+jwt`, `JWT` ID token, `logout+jwt`, `lifecycle-event+jwt`), Ed25519 SPKI PEM reconstruction.
- `../rusty-links/src/auth/oidc_rp/mod.rs` (entire file, 720 lines) - login, callback, logout, backchannel-logout, lifecycle-event handlers, session token issuance, dev-only seed-session.
- `../rusty-links/src/auth/oidc_rp/jit.rs` (entire file, 149 lines) - JIT provisioning with link-existing-by-email and admin sync.
- `../rusty-links/src/auth/middleware.rs:62-150` - `AuthenticatedUser` extractor pattern with session lookup, expiry, `session_version`, and `suspended_at` checks.
- `../rusty-links/migrations/20260417000009_add_sso_fields.sql`, `..._00010_create_rp_sessions.sql`, `..._00011_create_user_sessions.sql`, `..._00012_add_auth_via_oidc_to_user_sessions.sql` - schema additions.
- `../saas/api/src/handlers/oidc.rs` and `../saas/api/src/services/jwt.rs` - to confirm the exact claim shapes the consumer must accept.

---

## Implementation prompts

Each section below is a self-contained prompt. Run them in order. Each one ends in a buildable, testable state.

### Prompt 1: Add dependencies and remove legacy SaaS auth

> Goal: prepare rus for OIDC SSO by adding the necessary crates and removing the deprecated HS256 cookie auth in `src/handlers/saas_auth.rs`.
>
> Tasks:
> 1. In `Cargo.toml`, change the `saas` feature to depend on the OIDC stack instead of HMAC. Add these deps (gated `optional = true` and add to the `saas` feature list):
>    - `uuid = { version = "1", features = ["v4", "serde"] }`
>    - `reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }`
>    - `urlencoding = "2"`
>    - `moka = { version = "0.12", features = ["future"] }`
>    - Keep `sha2`, drop `hmac` and `hex` from the `saas` feature.
>    - Keep `jsonwebtoken = "9"` (already present, used for both modes).
> 2. Delete `src/handlers/saas_auth.rs`.
> 3. Remove its `mod` declaration and any imports/registrations from `src/handlers/mod.rs` and `src/main.rs`.
> 4. Delete the `SAAS_JWT_SECRET`, `SAAS_LOGIN_URL`, `SAAS_LOGOUT_URL`, `SAAS_MEMBERSHIP_URL`, `SAAS_REFRESH_URL` reads from `src/config.rs` if present; we will replace them with `OIDC_*` vars.
> 5. Build with `cargo build --no-default-features --features saas` and confirm it compiles (handlers will be missing routes; that is fine - we will reintroduce them in Prompt 4).

### Prompt 2: Add SSO database schema

> Goal: add SQLite schema for SSO. rus uses rusqlite with a single `Mutex<Connection>` and inline schema in `src/db.rs`.
>
> Mirror the canonical Postgres schema from `../rusty-links/migrations/20260417000009..00012` but in SQLite syntax. Append to `src/db.rs`'s schema bootstrap (or a new schema migration block) the following, gated only when running in `saas` mode (or unconditionally - it is harmless in standalone):
>
> ```sql
> ALTER TABLE users ADD COLUMN saas_user_id TEXT;          -- UUID as text
> ALTER TABLE users ADD COLUMN suspended_at TEXT;          -- RFC3339 datetime
> ALTER TABLE users ADD COLUMN session_version INTEGER NOT NULL DEFAULT 0;
> CREATE UNIQUE INDEX IF NOT EXISTS users_saas_user_id_unique
>     ON users(saas_user_id) WHERE saas_user_id IS NOT NULL;
>
> CREATE TABLE IF NOT EXISTS rp_sessions (
>     id            TEXT PRIMARY KEY,           -- UUID v4
>     state         TEXT NOT NULL UNIQUE,
>     nonce         TEXT NOT NULL,
>     code_verifier TEXT NOT NULL,
>     return_to     TEXT,
>     created_at    TEXT NOT NULL,
>     expires_at    TEXT NOT NULL
> );
> CREATE INDEX IF NOT EXISTS rp_sessions_expires ON rp_sessions(expires_at);
>
> CREATE TABLE IF NOT EXISTS user_sessions (
>     id                 TEXT PRIMARY KEY,        -- UUID v4
>     session_token_hash BLOB NOT NULL UNIQUE,    -- SHA-256(cookie)
>     user_id            INTEGER NOT NULL REFERENCES users(userID) ON DELETE CASCADE,
>     session_version    INTEGER NOT NULL,
>     auth_via_oidc      INTEGER NOT NULL DEFAULT 0,  -- bool
>     created_at         TEXT NOT NULL,
>     expires_at         TEXT NOT NULL
> );
> CREATE INDEX IF NOT EXISTS user_sessions_user_id ON user_sessions(user_id);
> CREATE INDEX IF NOT EXISTS user_sessions_expires ON user_sessions(expires_at);
> ```
>
> Note: the rus `users` table uses an integer PK named `userID`. Keep that. The `saas_user_id` foreign-side identity from the SaaS is a UUID stored as TEXT - do not try to make it the local PK.
>
> Wrap the `ALTER TABLE` calls so they no-op if the column already exists (rusqlite returns an error you can match on `"duplicate column name"`), since SQLite has no `ADD COLUMN IF NOT EXISTS`.
>
> Verify by running `just dev-local` and inspecting the schema with `sqlite3 ./data/rus.db ".schema users"`.

### Prompt 3: Add `OidcConfig` to `src/config.rs`

> Goal: parse `OIDC_*` env vars into a typed config, gated on the `saas` feature.
>
> Copy the `OidcConfig` struct and its parsing from `../rusty-links/src/config.rs:5-36` and `:172-250` verbatim (adjust types as needed - it is plain `String`/`u64`).
>
> Add it as a `#[cfg(feature = "saas")] pub oidc: OidcConfig` field on rus's `Config`. Add a `pub fn enabled(&self) -> bool { !self.issuer.is_empty() }` method.
>
> Env vars (all `OIDC_*`):
> - `OIDC_ISSUER` (e.g. `https://api.a8n.tools`) - empty string disables all OIDC routes.
> - `OIDC_AUDIENCE` (default `<HOST_URL>/api`) - the `aud` claim expected in `at+jwt`.
> - `OIDC_JWKS_URL` (default derived: `<issuer>/.well-known/jwks.json`).
> - `OIDC_JWKS_CACHE_TTL` (default 300).
> - `OIDC_CLIENT_ID`, `OIDC_CLIENT_SECRET` (also accept secret from `/run/secrets/oidc_client_secret`).
> - `OIDC_REDIRECT_URI` (default `<HOST_URL>/oauth2/callback`).
> - `OIDC_POST_LOGOUT_REDIRECT_URI` (default `<HOST_URL>/`).
> - `OIDC_LEEWAY_SECONDS` (default 30).
> - `OIDC_LIFECYCLE_JTI_CACHE_TTL` (default 300).
> - `OIDC_SESSION_TTL_SECONDS` (default 1_209_600 = 14 days).
>
> Apply the same fail-fast validations as rusty-links:
> - issuer set but client_id/secret missing => return Err.
> - jwks_url not HTTPS and not `http://localhost` => return Err.
>
> Update `.env.saas` (the example file rus already ships) to list every new var with comments. **Remove the old `SAAS_*` vars from it.**
>
> Update `CLAUDE.md`'s "Environment Variables" section to document the new SaaS-mode vars.

### Prompt 4: Port the OIDC Resource Server (token verifier)

> Goal: introduce `src/auth/oidc_rs.rs` containing `OidcVerifier`, JWKS fetching, and the four token validators (`at+jwt`, ID token, logout+jwt, lifecycle-event+jwt).
>
> Copy `../rusty-links/src/auth/oidc_rs.rs` essentially verbatim. It is framework-agnostic (only depends on `reqwest`, `jsonwebtoken`, `chrono`, `tokio::sync::RwLock`). Make it `#[cfg(feature = "saas")]`.
>
> Replace `crate::error::AppError` with whatever rus's error type is (rus likely uses `actix_web::Error` or a custom enum - check `src/handlers/*.rs`). Define an `enum OidcError` if none exists.
>
> Make `OidcVerifier::new(config)` build a single `reqwest::Client` with a 10s timeout and store it. Hold the verifier in `Arc` and stuff it into `web::Data` so handlers can clone cheaply.
>
> Confirm `cargo build --no-default-features --features saas` succeeds.

### Prompt 5: Port the OIDC Relying Party (BFF) handlers

> Goal: introduce `src/auth/oidc_rp/mod.rs` and `src/auth/oidc_rp/jit.rs` with the five handlers (`login`, `callback`, `logout`, `backchannel_logout`, `lifecycle_event`) plus the dev-only `dev_seed_session` and `dev_logout` (gated `#[cfg(debug_assertions)]`).
>
> Source: `../rusty-links/src/auth/oidc_rp/mod.rs` (720 lines) and `jit.rs` (149 lines). Translate from Axum to Actix-web:
>
> - Axum `State<T>` -> `web::Data<T>`.
> - Axum `Query<T>`, `Form<T>` -> `web::Query<T>`, `web::Form<T>`.
> - `axum_extra::extract::cookie::CookieJar` -> Actix `HttpRequest::cookie(name)` for reads, `HttpResponse::add_cookie` for writes. Build `actix_web::cookie::Cookie::build(name, value).http_only(true).secure(secure).same_site(SameSite::Lax).path("/").max_age(Duration::seconds(ttl as i64)).finish()`.
> - `Redirect::to(&url).into_response()` -> `HttpResponse::SeeOther().append_header(("Location", url)).finish()`.
> - `IntoResponse`/`Response` -> `actix_web::HttpResponse` or `Result<HttpResponse, Error>`.
> - sqlx queries -> rusqlite via the existing `Mutex<Connection>` pattern in rus. Wrap blocking work in `web::block(move || { ... })` if it's hot, or call inline matching the existing rus style.
>
> Cookie name: use `rus_session` (not `rl_session`).
> Default post-callback redirect: `/dashboard` (rus uses `/dashboard`, not `/links`).
>
> Schema differences to handle in queries:
> - rus `users` PK is `userID` (INTEGER), not `id` (UUID). The `user_sessions.user_id` FK is INTEGER. The `saas_user_id` lookup returns the rus integer userID, not a UUID.
> - Bind `Vec<u8>` for `BLOB` columns instead of `&[u8]`.
>
> Keep the link-existing-user-by-email fallback in `jit.rs` - rus has standalone-mode users that may want to migrate.
>
> The lifecycle JTI cache is a `moka::future::Cache<String, ()>` with TTL = `config.lifecycle_jti_cache_ttl`. Wrap in `Arc`.
>
> Make sure each handler short-circuits with `HttpResponse::NotFound().finish()` when `!config.enabled()`.

### Prompt 6: Add the session extractor (`AuthenticatedUser`)

> Goal: an Actix `FromRequest` extractor that resolves the `rus_session` cookie to a user_id, applying expiry, `session_version`, and `suspended_at` checks.
>
> Source: `../rusty-links/src/auth/middleware.rs:62-150` and the helper `get_user_from_session` in `../rusty-links/src/auth/oidc_rp/mod.rs:552-592`.
>
> In Actix the pattern is:
>
> ```rust
> impl FromRequest for AuthenticatedUser {
>     type Error = actix_web::Error;
>     type Future = Pin<Box<dyn Future<Output = Result<Self, Error>>>>;
>     fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future { ... }
> }
> ```
>
> Resolve order:
> 1. Read `rus_session` cookie. If absent => `401`.
> 2. SHA-256 the cookie value, look it up in `user_sessions`.
> 3. JOIN to `users` to fetch `users.session_version` and `users.suspended_at`.
> 4. Reject if expired, version mismatch, or suspended.
> 5. Return `AuthenticatedUser { user_id, auth_via_oidc, is_admin }`.
>
> Update protected handlers in `src/handlers/urls.rs`, `admin.rs`, `pages.rs` to use this extractor in `saas` mode. In `standalone` mode keep using the existing JWT bearer extractor. Use a `cfg`-gated type alias if helpful so call sites do not branch.
>
> Unauthenticated requests to dashboard pages should redirect to `/oauth2/login?return_to=<original-path>`.

### Prompt 7: Wire routes into `main.rs`

> Goal: register the OIDC RP and dev routes in saas mode, and remove the legacy saas auth routes.
>
> In `src/main.rs`, when the `saas` feature is enabled:
>
> 1. Build `OidcVerifier` from `config.oidc` and put it in `web::Data`.
> 2. Build the JTI cache (`moka::future::Cache`) and put it in `web::Data`.
> 3. Register routes (only when `config.oidc.enabled()`):
>    - `GET  /oauth2/login`
>    - `GET  /oauth2/callback`
>    - `GET  /oauth2/logout`
>    - `POST /oauth2/backchannel-logout`
>    - `POST /oauth2/lifecycle-event`
>    - `GET  /dev/seed-session`  (only `#[cfg(debug_assertions)]`)
>    - `GET  /dev/logout`        (only `#[cfg(debug_assertions)]`)
> 4. The dashboard route should redirect unauthenticated users to `/oauth2/login`, replacing whatever the deleted `saas_auth.rs` did.
>
> Make sure `cargo build --release --no-default-features --features saas` and `cargo build --release --features standalone` both succeed.

### Prompt 8: Update frontend templates for SaaS mode

> Goal: hide standalone login/signup forms when running in SaaS mode and route the "Login" button to `/oauth2/login`.
>
> Inspect `static/login.html`, `static/signup.html`, `static/dashboard.html`, and `static/k9f3x2m7.js`. The current SaaS code path expects a parent-app cookie. With OIDC:
>
> - The login button on the landing page should link to `/oauth2/login`.
> - The dashboard's "Logout" button should hit `/oauth2/logout`.
> - In SaaS mode, hide the standalone signup link entirely.
> - The dashboard's auth check no longer needs to call `/api/me` with a Bearer token in saas mode - the session cookie is sent automatically. Update the JS accordingly, or have the dashboard handler render a server-side 302 to `/oauth2/login` when there is no session.
>
> If the binary serves the same static directory in both modes, conditionally toggle UI via a small `/api/auth-mode` endpoint that returns `{"mode": "standalone" | "saas"}` and have JS branch on it.

### Prompt 9: Compose / dev-environment wiring

> Goal: get the OIDC consumer working against the local `../saas` instance.
>
> 1. Look at `../saas`'s `compose.yml` (or equivalent) to find the local issuer URL it advertises (typically `http://localhost:18080` for dev or `https://api.a8n.tools` for shared dev).
> 2. Register rus as an OIDC client in the saas dev seed data. Use a stable test client UUID; copy from `../rusty-links/src/config.rs:383` test fixture as a template (`a8000000-0000-0000-0000-0000000000XX`). Pick the next free `XX` (check `../saas` seed data and `../dmarc-reporter` config). The redirect URI must match exactly what rus advertises - for per-developer Traefik that is `https://${USER}-rus.a8n.run/oauth2/callback`; for `dev-local` it is `http://localhost:4001/oauth2/callback`.
> 3. In rus's `compose.dev.yml`, add `OIDC_*` env vars. Wire `OIDC_CLIENT_SECRET` via Docker secret to `/run/secrets/oidc_client_secret` (the config code already reads that path as a fallback).
> 4. In rus's `compose.yml` (cargo-watch local-dev), add the same `OIDC_*` env. Use `http://localhost:18080` (or whatever the saas dev compose advertises) so JWKS works without HTTPS validation tripping.
> 5. Ensure both rus and saas are on the same Docker network (`network-traefik-public` per the user's global Docker convention) so JWKS fetches resolve.

### Prompt 10: Verification checklist

> Goal: end-to-end sanity check before declaring done.
>
> 1. **Build matrix**: `cargo build --release --features standalone` and `cargo build --release --no-default-features --features saas` both succeed; `cargo clippy --no-default-features --features saas` is clean.
> 2. **Standalone mode is unchanged**: existing tests in `src/testing.rs` (and any integration tests) still pass.
> 3. **OIDC disabled by default**: with `OIDC_ISSUER=""` (or unset), all `/oauth2/*` routes return 404. The dashboard is reachable only via the dev seed-session in debug builds.
> 4. **Login round-trip**: `just dev saas`, open the dashboard, click Login, complete the saas login, land back on `/dashboard` with a `rus_session` cookie. Hit a protected API endpoint (e.g. `POST /api/shorten`) without a Bearer token - it should succeed via the cookie session.
> 5. **JIT provisioning**: a brand-new saas user (never seen by rus before) hits `/oauth2/login`, completes login, and a row appears in `users` with `saas_user_id` set, `password` = `'!sso:no-password'`, `session_version = 0`.
> 6. **Link existing standalone account**: pre-create a user with email `e@x.com` in standalone mode, then log in via SSO with the same email - the existing row should get `saas_user_id` set rather than a new row being inserted.
> 7. **Membership gate**: log in as a saas user with `has_member_access = false` - rus should redirect to `/login?error=access_denied&...`.
> 8. **Suspension**: simulate `POST /oauth2/lifecycle-event` with a `user.suspended` token (use the saas dev tooling). The user's existing sessions should immediately stop working (`session_version` mismatch).
> 9. **Logout**: `GET /oauth2/logout` clears the cookie, removes the `user_sessions` row, and redirects via the saas `end_session_endpoint`.
> 10. **JWKS rotation**: rotate the saas signing key, hit a token signed with the new key - rus should refresh its JWKS cache automatically (lazy refresh on unknown `kid`).

---

## Out of scope (intentionally)

- **OIDC discovery autoconfiguration**: do not fetch `/.well-known/openid-configuration` at startup. The canonical pattern derives URLs from `OIDC_ISSUER` directly. Match it.
- **Refresh tokens**: the BFF session lives 14 days; we do not store or rotate the OIDC refresh token. Match rusty-links.
- **Multi-tenancy**: rus is single-tenant. Do not import the `tenants` model from dmarc-reporter.
- **Webhook receiver**: rus has `src/handlers/webhook.rs` already; do not conflate it with `lifecycle-event`. Lifecycle events are signed JWTs, not HMAC-signed webhooks.
- **Removing standalone mode**: keep both build modes working. The `saas` feature replaces only the auth layer; URL shortening logic stays shared.
