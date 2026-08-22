//! RUS-19: hold a sign-in from a country the account has not used before until
//! the owner releases it from an emailed single-use link.
//!
//! RUS-7 alerts after the fact, which does not stop an attacker who already
//! reads the mailbox: they see the notice and delete it. This turns the same
//! signal into a gate. There is exactly one suspicion predicate in the crate
//! (`location_alert::is_new_country`), so the alert and the gate can never
//! disagree about what is suspicious, and every completed-sign-in site routes
//! through [`gate_login`] before any credential is minted.
//!
//! Three properties this module exists to keep, in order of how badly getting
//! them wrong hurts:
//!
//! 1. A first-ever sign-in (no prior country) is never held, or the first
//!    account ever created could never sign in.
//! 2. An unresolved country is never held, or every deployment without the
//!    geoblock edge bricks itself, since with no `X-IPCountry` the country is
//!    always `None`.
//! 3. The kill switch (`LOGIN_APPROVAL_ENABLED`) is off unless a deployment
//!    spells "true" exactly, because this control can lock out a real user.
//!
//! Deliverability is part of the decision, not an afterthought: a gate whose
//! approval link cannot be delivered is a permanent lockout. When no recipient
//! resolves at all, or SMTP is unconfigured, the sign-in is allowed through and
//! the RUS-7 alert carries the signal instead. A transport failure after that
//! check is different: delivery IS configured and merely failed, so the sign-in
//! fails closed and the user retries.
//!
//! The approval page and its API are deliberately reachable with no session:
//! the person opening them has not signed in yet, by construction. They are
//! registered from [`configure_routes`], outside the authenticated `/api`
//! scope in both feature legs, and the tests below assert that from the route
//! layer rather than by calling the handlers directly.

use actix_governor::{Governor, GovernorConfigBuilder};
use actix_web::{http::StatusCode, web, HttpRequest, HttpResponse};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::config::{Config, MailConfig};
use crate::db::AppState;
use crate::location_alert::{self, AccountAlertInfo};
use crate::mailer::{AlertRecipient, LoginApprovalMail};

/// How long an emailed approval link stays claimable.
///
/// Long enough to leave the browser, wait for delivery and read the mail;
/// short enough that a link sitting in a mailbox is not a durable credential.
pub const APPROVAL_TTL_MINUTES: i64 = 15;

/// Where the emailed link lands. A flat `.html` path like every other page, so
/// it is served by the same route shape and sits above the short-code catch-all.
pub const APPROVAL_PAGE_PATH: &str = "/approve-login.html";

/// The API the page validates a link against (GET) and releases through (POST).
pub const APPROVAL_API_PATH: &str = "/api/login-approval";

/// Whether a sign-in that has otherwise succeeded must be approved first.
///
/// Reuses the RUS-7 signal verbatim, so a first-ever sign-in and an unresolved
/// country are never held. Takes no account of `notify_new_location`: that
/// preference is written from an authenticated session, so honouring it here
/// would let anyone holding a session switch off the control that defends
/// against them, and would leave an opted-out account held with no mail to
/// release it.
pub fn should_require_approval(
    approval_enabled: bool,
    previous: Option<&str>,
    current: Option<&str>,
) -> bool {
    approval_enabled && location_alert::is_new_country(previous, current)
}

/// What one sign-in's gate evaluation decided.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GateDecision {
    /// Nothing to hold: switch off, no prior country, unresolved country, or
    /// the same country as last time.
    Allow,
    /// Hold-worthy, but no approval could ever be delivered, so holding would
    /// lock the account out with no way back in. The sign-in completes and the
    /// RUS-7 alert carries the signal.
    AllowUndeliverable,
    /// Hold the sign-in and mail this recipient a single-use release link.
    Hold(AlertRecipient),
}

/// Decide the gate for one sign-in, and to whom the release link would go.
///
/// Recipient resolution is RUS-11's, so an account with no address of its own
/// falls back to the `SECURITY_ALERT_EMAIL` operator mailbox rather than being
/// held with nowhere to send the link.
pub fn gate_decision(
    mail: &MailConfig,
    previous: Option<&str>,
    current: Option<&str>,
    account_email: Option<&str>,
) -> GateDecision {
    if !should_require_approval(mail.login_approval_enabled, previous, current) {
        return GateDecision::Allow;
    }
    match crate::mailer::resolve_recipient(mail, account_email) {
        Some(recipient) if mail.smtp_ready() => GateDecision::Hold(recipient),
        _ => GateDecision::AllowUndeliverable,
    }
}

/// The gate decision plus everything the hold needs to act on it.
pub(crate) struct GateContext {
    pub(crate) decision: GateDecision,
    /// The country this request resolved to, `None` off the geoblock edge.
    pub(crate) country: Option<String>,
    pub(crate) info: AccountAlertInfo,
}

/// Evaluate the gate for `user_id` against this request.
///
/// The single entry point every completed-sign-in site calls, so the standalone
/// login and the saas OIDC callback cannot drift apart. Takes an already-held
/// connection rather than locking, because the standalone login path holds the
/// database mutex across its whole handler and this must not deadlock it.
/// `None` means the account no longer exists, which the caller treats as a
/// failed sign-in.
pub(crate) fn gate_login(
    db: &Connection,
    mail: &MailConfig,
    user_id: i64,
    req: &HttpRequest,
) -> Option<GateContext> {
    let info = location_alert::get_login_location(db, user_id)
        .ok()
        .flatten()?;
    let country = location_alert::client_country(req);
    let decision = gate_decision(
        mail,
        info.last_country.as_deref(),
        country.as_deref(),
        info.email.as_deref(),
    );
    Some(GateContext {
        decision,
        country,
        info,
    })
}

/// A 256-bit approval token. Only its hash is ever stored.
///
/// base64url without padding, so the value is safe in a query string with no
/// percent-encoding step to get wrong.
pub fn generate_approval_token() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

/// SHA-256 of an approval token, as stored in `token_hash`.
pub fn hash_approval_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

/// The link mailed to the owner, landing on the approval page.
pub fn build_approval_url(host_url: &str, token: &str) -> String {
    format!(
        "{}{}?token={}",
        host_url.trim_end_matches('/'),
        APPROVAL_PAGE_PATH,
        token
    )
}

/// Why a token did not yield a held sign-in. Kept apart from "not found" on
/// purpose: an owner who waited too long needs to be told to sign in again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalFailure {
    NotFound,
    AlreadyUsed,
    Expired,
}

impl ApprovalFailure {
    /// HTTP status and message. 410 for a link that existed and is finished
    /// with, 404 for one that never existed.
    fn response(self) -> (StatusCode, &'static str) {
        match self {
            Self::NotFound => (StatusCode::NOT_FOUND, "This approval link is not valid."),
            Self::AlreadyUsed => (
                StatusCode::GONE,
                "This approval link has already been used.",
            ),
            Self::Expired => (
                StatusCode::GONE,
                "This approval link has expired. Sign in again to get a new one.",
            ),
        }
    }
}

/// What the approval page shows before the owner commits to releasing.
#[derive(Clone, Debug)]
pub struct PendingApproval {
    pub user_id: i64,
    pub country: String,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: String,
    pub expires_at: String,
}

/// Drop rows past their expiry. Best effort: a leftover row is harmless because
/// every read re-checks the expiry, so a failure here is not worth a log line.
fn sweep_expired(db: &Connection, now: DateTime<Utc>) {
    let _ = db.execute(
        "DELETE FROM pending_login_approvals WHERE expires_at <= ?1",
        params![now.to_rfc3339()],
    );
}

/// Write the pending row. Called before the mail goes out, so a link that
/// arrives always has a row behind it.
#[allow(clippy::too_many_arguments)]
fn insert_pending(
    db: &Connection,
    user_id: i64,
    token_hash: &[u8],
    country: &str,
    ip: Option<&str>,
    user_agent: Option<&str>,
    now: DateTime<Utc>,
    expires_at: DateTime<Utc>,
) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO pending_login_approvals
         (user_id, token_hash, country, ip, user_agent, created_at, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            user_id,
            token_hash,
            country,
            ip,
            user_agent,
            now.to_rfc3339(),
            expires_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

/// Read a held sign-in without consuming it, so a link preview is safe.
pub fn get_approval(
    db: &Connection,
    token: &str,
    now: DateTime<Utc>,
) -> Result<PendingApproval, ApprovalFailure> {
    type Row = (
        i64,
        String,
        Option<String>,
        Option<String>,
        String,
        String,
        Option<String>,
    );
    let row: Option<Row> = db
        .query_row(
            "SELECT user_id, country, ip, user_agent, created_at, expires_at, consumed_at
             FROM pending_login_approvals WHERE token_hash = ?1",
            params![hash_approval_token(token)],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .optional()
        .map_err(|error| {
            tracing::error!(error = %error, "RUS-19: pending approval lookup failed");
            ApprovalFailure::NotFound
        })?;

    let Some((user_id, country, ip, user_agent, created_at, expires_at, consumed_at)) = row else {
        return Err(ApprovalFailure::NotFound);
    };
    if consumed_at.is_some() {
        return Err(ApprovalFailure::AlreadyUsed);
    }
    if is_expired(&expires_at, now) {
        return Err(ApprovalFailure::Expired);
    }

    Ok(PendingApproval {
        user_id,
        country,
        ip,
        user_agent,
        created_at,
        expires_at,
    })
}

/// An unparseable timestamp counts as expired: a row nobody can date is a row
/// nobody gets to sign in with.
fn is_expired(expires_at: &str, now: DateTime<Utc>) -> bool {
    match DateTime::parse_from_rfc3339(expires_at) {
        Ok(parsed) => parsed.with_timezone(&Utc) <= now,
        Err(_) => true,
    }
}

/// Claim a held sign-in, once.
///
/// The guarded UPDATE decides the race: two concurrent clicks both read an
/// unconsumed row, but only one of them matches `consumed_at IS NULL`, and the
/// loser's affected-row count is 0. The account comes off the claimed row and
/// never off the request, so a token can only ever release its own sign-in.
pub fn consume_approval(
    db: &Connection,
    token: &str,
    now: DateTime<Utc>,
) -> Result<PendingApproval, ApprovalFailure> {
    let pending = get_approval(db, token, now)?;

    let claimed = db
        .execute(
            "UPDATE pending_login_approvals
             SET consumed_at = ?1
             WHERE token_hash = ?2 AND consumed_at IS NULL AND expires_at > ?1",
            params![now.to_rfc3339(), hash_approval_token(token)],
        )
        .map_err(|error| {
            tracing::error!(error = %error, "RUS-19: pending approval claim failed");
            ApprovalFailure::NotFound
        })?;

    if claimed != 1 {
        return Err(ApprovalFailure::AlreadyUsed);
    }
    Ok(pending)
}

/// Hold this sign-in and mail the owner a single-use release link.
///
/// The row is written before the mail is sent, so a link that arrives always
/// has a row behind it, whereas a row nobody can reach simply expires. An `Err`
/// means the sign-in must fail rather than complete ungated.
pub(crate) async fn request_login_approval(
    state: &web::Data<AppState>,
    user_id: i64,
    context: &GateContext,
    recipient: &AlertRecipient,
    req: &HttpRequest,
) -> Result<(), String> {
    let Some(country) = context.country.as_deref() else {
        return Err("no country resolved for a held sign-in".to_string());
    };
    let ip = location_alert::client_ip(req);
    let device = location_alert::device_info(req);

    let token = generate_approval_token();
    let now = Utc::now();
    let expires_at = now + Duration::minutes(APPROVAL_TTL_MINUTES);

    {
        let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
        sweep_expired(&db, now);
        insert_pending(
            &db,
            user_id,
            &hash_approval_token(&token),
            country,
            ip.as_deref(),
            device.as_deref(),
            now,
            expires_at,
        )
        .map_err(|error| error.to_string())?;
    }

    tracing::warn!(
        user_id,
        username = %context.info.username,
        country = %country,
        "RUS-19: sign-in from a previously unseen country held for approval"
    );

    crate::mailer::send_login_approval_request(
        &state.config.mail,
        LoginApprovalMail {
            recipient,
            username: &context.info.username,
            country,
            ip: ip.as_deref().unwrap_or("unknown"),
            device: device.as_deref(),
            approval_url: &build_approval_url(&state.config.host_url, &token),
            expiry_minutes: APPROVAL_TTL_MINUTES,
        },
    )
    .await
}

/// The 202 a held standalone sign-in answers with: no token, no cookie, and an
/// explicit flag the login page branches on. The saas leg redirects the browser
/// to the same waiting page instead, because its sign-in is a browser flow.
#[cfg(feature = "standalone")]
pub fn held_response() -> HttpResponse {
    HttpResponse::Accepted().json(serde_json::json!({
        "approval_required": true,
        "expires_in_minutes": APPROVAL_TTL_MINUTES,
        "message": "This sign-in is from a country you have not used before. Check your email and open the approval link to finish signing in.",
    }))
}

/// The 500 a sign-in answers with when the approval mail could not be sent.
/// Fails closed: delivery is configured and merely failed, so a retry is the
/// fix and nothing is issued in the meantime.
#[cfg(feature = "standalone")]
pub fn hold_failed_response() -> HttpResponse {
    HttpResponse::InternalServerError().json(serde_json::json!({
        "error": "Could not send the approval email for this sign-in. Please try again."
    }))
}

// ── HTTP surface ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ApprovalTokenQuery {
    token: Option<String>,
}

#[derive(Deserialize)]
pub struct ApproveRequest {
    token: String,
}

fn failure_response(failure: ApprovalFailure) -> HttpResponse {
    let (status, message) = failure.response();
    HttpResponse::build(status).json(serde_json::json!({ "error": message }))
}

/// The page the emailed link lands on. Public by construction: whoever opens it
/// has not signed in, which is the whole point of the hold.
pub async fn approve_login_page() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../static/approve-login.html"))
}

/// Describe a held sign-in without consuming it.
///
/// GET never claims the token: mail gateways and link scanners fetch URLs out
/// of messages, and a GET that consumed would burn the owner's only link before
/// they ever saw it.
pub async fn lookup_approval(
    data: web::Data<AppState>,
    query: web::Query<ApprovalTokenQuery>,
) -> HttpResponse {
    let Some(token) = query.token.as_deref().filter(|t| !t.is_empty()) else {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "An approval token is required."
        }));
    };

    let found = {
        let db = data.db.lock().unwrap_or_else(|e| e.into_inner());
        get_approval(&db, token, Utc::now())
    };

    match found {
        Ok(pending) => HttpResponse::Ok().json(serde_json::json!({
            "valid": true,
            "approval": {
                "country": pending.country,
                "ip": pending.ip,
                "user_agent": pending.user_agent,
                "requested_at": pending.created_at,
                "expires_at": pending.expires_at,
            },
        })),
        Err(failure) => failure_response(failure),
    }
}

/// Release a held sign-in and establish the session on the browser that asked.
///
/// The approving browser is the one that gets signed in, which is deliberate:
/// the owner reading the mail is the party being trusted, and the browser that
/// triggered the hold is never handed anything.
pub async fn approve_login(
    data: web::Data<AppState>,
    body: web::Json<ApproveRequest>,
) -> HttpResponse {
    let db = data.db.lock().unwrap_or_else(|e| e.into_inner());

    let pending = match consume_approval(&db, &body.token, Utc::now()) {
        Ok(pending) => pending,
        Err(failure) => return failure_response(failure),
    };

    let response = match complete_session(&db, &data.config, pending.user_id) {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(
                user_id = pending.user_id,
                error = %error,
                "RUS-19: approved sign-in could not be established"
            );
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to complete the approved sign-in."
            }));
        }
    };

    // Only now, with the sign-in actually complete, does this country become
    // familiar. An attempt that is never approved cannot seed it.
    if let Err(error) =
        location_alert::update_last_login_country(&db, pending.user_id, &pending.country)
    {
        tracing::warn!(
            user_id = pending.user_id,
            error = %error,
            "RUS-19: recording the approved sign-in country failed"
        );
    }

    tracing::warn!(
        user_id = pending.user_id,
        country = %pending.country,
        "RUS-19: held sign-in released by the account owner"
    );

    response
}

/// Mint the leg's session for an approved sign-in. Both legs delegate to their
/// one session-establishing helper rather than repeating it here.
#[cfg(feature = "standalone")]
fn complete_session(
    db: &Connection,
    config: &Config,
    user_id: i64,
) -> Result<HttpResponse, String> {
    let (username, is_admin): (String, i64) = db
        .query_row(
            "SELECT username, is_admin FROM users WHERE userID = ?1",
            params![user_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| error.to_string())?;

    let session = crate::auth::establish_session(db, config, &username, user_id, is_admin != 0)?;
    Ok(HttpResponse::Ok().json(session))
}

#[cfg(feature = "saas")]
fn complete_session(
    db: &Connection,
    config: &Config,
    user_id: i64,
) -> Result<HttpResponse, String> {
    let token =
        crate::oidc::session::establish_session(db, user_id, config.oidc.session_ttl_seconds, true)
            .map_err(|error| error.to_string())?;

    let secure = config.oidc.redirect_uri.starts_with("https://");
    Ok(HttpResponse::Ok()
        .cookie(crate::oidc::session::build_session_cookie(
            &token,
            config.oidc.session_ttl_seconds,
            secure,
        ))
        .json(serde_json::json!({ "success": true, "redirect": "/dashboard.html" })))
}

/// Register the approval page and its API.
///
/// One definition for both feature legs and for the tests, so the paths, the
/// methods and the rate limit cannot drift. The caller must mount this OUTSIDE
/// any authenticated scope and before the short-code catch-all: the person
/// following the emailed link has no session, and an approval route behind the
/// session check turns the gate into a lockout. `main.rs` mounts it before the
/// guarded `/api` scope in both legs, and the tests below assert both halves.
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    // Same budget as the other public endpoints (30/minute). Guessing a 256-bit
    // token is hopeless; this is only here to bound the cost of trying.
    let rate_limit = GovernorConfigBuilder::default()
        .seconds_per_request(2)
        .burst_size(30)
        .finish()
        .expect("approval rate limit config is valid");

    cfg.route(APPROVAL_PAGE_PATH, web::get().to(approve_login_page))
        .service(
            web::resource(APPROVAL_API_PATH)
                .wrap(Governor::new(&rate_limit))
                .route(web::get().to(lookup_approval))
                .route(web::post().to(approve_login)),
        );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::make_test_state;
    // Imported under another name: `use actix_web::test` would shadow the
    // built-in `#[test]` attribute for the synchronous tests below.
    use actix_web::{test as http_test, App};

    /// The real `main.rs`, read at compile time so the wiring assertions below
    /// check the file the binary is built from and not a copy of it.
    const MAIN_RS: &str = include_str!("main.rs");

    const PEER: &str = "127.0.0.1:34567";

    fn deliverable_mail() -> MailConfig {
        MailConfig {
            smtp_host: Some("127.0.0.1".to_string()),
            // Port 1 is never listening, so a send fails immediately rather
            // than hanging a test on a network timeout.
            smtp_port: Some(1),
            smtp_from_email: Some("no-reply@example.com".to_string()),
            login_approval_enabled: true,
            ..MailConfig::default()
        }
    }

    /// Trust the loopback so route-level tests can resolve a country at all.
    /// `init_trusted_proxies` is a `OnceLock`, so repeated calls with the same
    /// set are harmless and the first one wins.
    fn init_test_proxies() {
        location_alert::init_trusted_proxies(vec![
            "127.0.0.0/8".parse().unwrap(),
            "::1/128".parse().unwrap(),
        ]);
    }

    fn state_with_mail(mail: MailConfig) -> web::Data<AppState> {
        let mut config = crate::testing::test_config();
        config.mail = mail;
        web::Data::new(AppState::new(config).unwrap())
    }

    fn insert_gated_user(state: &web::Data<AppState>, username: &str) -> i64 {
        let db = state.db.lock().unwrap();
        db.execute(
            "INSERT INTO users (username, password) VALUES (?1, 'placeholder')",
            params![username],
        )
        .unwrap();
        db.last_insert_rowid()
    }

    fn seed_pending(state: &web::Data<AppState>, user_id: i64, minutes: i64) -> String {
        let token = generate_approval_token();
        let now = Utc::now();
        let db = state.db.lock().unwrap();
        insert_pending(
            &db,
            user_id,
            &hash_approval_token(&token),
            "DE",
            Some("203.0.113.9"),
            Some("curl/8"),
            now,
            now + Duration::minutes(minutes),
        )
        .unwrap();
        token
    }

    // ── The three properties ─────────────────────────────────────────────────

    #[test]
    fn a_first_ever_sign_in_is_never_held() {
        let mail = deliverable_mail();
        assert_eq!(
            gate_decision(&mail, None, Some("DE"), Some("alice@example.com")),
            GateDecision::Allow
        );
        assert!(!should_require_approval(true, None, Some("DE")));
    }

    #[test]
    fn an_unresolved_country_is_never_held() {
        let mail = deliverable_mail();
        assert_eq!(
            gate_decision(&mail, Some("US"), None, Some("alice@example.com")),
            GateDecision::Allow
        );
        assert!(!should_require_approval(true, Some("US"), None));
        // Neither side known is the off-the-edge case, and must also be silent.
        assert!(!should_require_approval(true, None, None));
    }

    #[test]
    fn the_kill_switch_defaults_off() {
        let mail = MailConfig::default();
        assert!(!mail.login_approval_enabled);
        assert_eq!(
            gate_decision(&mail, Some("US"), Some("DE"), Some("alice@example.com")),
            GateDecision::Allow
        );
    }

    // ── The rest of the decision ─────────────────────────────────────────────

    #[test]
    fn the_same_country_is_never_held_whatever_its_case() {
        let mail = deliverable_mail();
        assert_eq!(
            gate_decision(&mail, Some("us"), Some("US"), Some("alice@example.com")),
            GateDecision::Allow
        );
    }

    #[test]
    fn a_new_country_holds_and_addresses_the_owner() {
        let mail = deliverable_mail();
        assert_eq!(
            gate_decision(&mail, Some("US"), Some("DE"), Some("alice@example.com")),
            GateDecision::Hold(AlertRecipient::Owner("alice@example.com".to_string()))
        );
    }

    #[test]
    fn an_account_with_no_address_falls_back_to_the_operator_mailbox() {
        let mail = MailConfig {
            security_alert_email: Some("security@example.com".to_string()),
            ..deliverable_mail()
        };
        assert_eq!(
            gate_decision(&mail, Some("US"), Some("DE"), None),
            GateDecision::Hold(AlertRecipient::Operator("security@example.com".to_string()))
        );
    }

    #[test]
    fn no_recipient_at_all_allows_rather_than_locking_out() {
        let mail = deliverable_mail();
        assert_eq!(
            gate_decision(&mail, Some("US"), Some("DE"), None),
            GateDecision::AllowUndeliverable
        );
    }

    #[test]
    fn unconfigured_smtp_allows_rather_than_locking_out() {
        let mail = MailConfig {
            smtp_host: None,
            smtp_from_email: None,
            login_approval_enabled: true,
            ..MailConfig::default()
        };
        assert_eq!(
            gate_decision(&mail, Some("US"), Some("DE"), Some("alice@example.com")),
            GateDecision::AllowUndeliverable
        );
    }

    #[test]
    fn the_alert_opt_out_does_not_disable_the_gate() {
        let state = state_with_mail(deliverable_mail());
        let user_id = insert_gated_user(&state, "alice");
        {
            let db = state.db.lock().unwrap();
            db.execute(
                "UPDATE users SET last_login_country = 'US', email = 'alice@example.com',
                 notify_new_location = 0 WHERE userID = ?1",
                params![user_id],
            )
            .unwrap();
        }
        init_test_proxies();
        let req = http_test::TestRequest::default()
            .peer_addr(PEER.parse().unwrap())
            .insert_header(("X-IPCountry", "DE"))
            .to_http_request();

        let db = state.db.lock().unwrap();
        let context = gate_login(&db, &state.config.mail, user_id, &req).unwrap();
        assert!(!context.info.notify_new_location);
        assert!(matches!(context.decision, GateDecision::Hold(_)));
    }

    // ── Token handling ───────────────────────────────────────────────────────

    #[test]
    fn the_raw_token_is_never_stored() {
        let state = state_with_mail(deliverable_mail());
        let user_id = insert_gated_user(&state, "alice");
        let token = seed_pending(&state, user_id, 15);

        let db = state.db.lock().unwrap();
        let stored: Vec<u8> = db
            .query_row(
                "SELECT token_hash FROM pending_login_approvals WHERE user_id = ?1",
                params![user_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, hash_approval_token(&token));
        assert_ne!(stored, token.as_bytes());
        assert_eq!(stored.len(), 32);
    }

    #[test]
    fn a_token_is_consumable_exactly_once() {
        let state = state_with_mail(deliverable_mail());
        let user_id = insert_gated_user(&state, "alice");
        let token = seed_pending(&state, user_id, 15);
        let db = state.db.lock().unwrap();

        let first = consume_approval(&db, &token, Utc::now()).unwrap();
        assert_eq!(first.user_id, user_id);
        assert_eq!(
            consume_approval(&db, &token, Utc::now()).unwrap_err(),
            ApprovalFailure::AlreadyUsed
        );
    }

    #[test]
    fn an_expired_token_is_rejected_by_both_the_read_and_the_claim() {
        let state = state_with_mail(deliverable_mail());
        let user_id = insert_gated_user(&state, "alice");
        let token = seed_pending(&state, user_id, -1);
        let db = state.db.lock().unwrap();

        assert_eq!(
            get_approval(&db, &token, Utc::now()).unwrap_err(),
            ApprovalFailure::Expired
        );
        assert_eq!(
            consume_approval(&db, &token, Utc::now()).unwrap_err(),
            ApprovalFailure::Expired
        );
    }

    #[test]
    fn an_unknown_token_is_rejected() {
        let state = state_with_mail(deliverable_mail());
        let db = state.db.lock().unwrap();
        assert_eq!(
            consume_approval(&db, &generate_approval_token(), Utc::now()).unwrap_err(),
            ApprovalFailure::NotFound
        );
    }

    #[test]
    fn a_token_only_ever_releases_its_own_account() {
        let state = state_with_mail(deliverable_mail());
        let alice = insert_gated_user(&state, "alice");
        let mallory = insert_gated_user(&state, "mallory");
        let alice_token = seed_pending(&state, alice, 15);
        let mallory_token = seed_pending(&state, mallory, 15);
        let db = state.db.lock().unwrap();

        // The account comes off the claimed row, so neither token can ever name
        // the other's.
        assert_eq!(
            consume_approval(&db, &alice_token, Utc::now())
                .unwrap()
                .user_id,
            alice
        );
        assert_eq!(
            consume_approval(&db, &mallory_token, Utc::now())
                .unwrap()
                .user_id,
            mallory
        );
    }

    #[test]
    fn a_link_is_built_against_the_page_the_app_serves() {
        let url = build_approval_url("https://rus.example.com/", "abc-123_XY");
        assert_eq!(
            url,
            "https://rus.example.com/approve-login.html?token=abc-123_XY"
        );
        assert!(url.contains(APPROVAL_PAGE_PATH));
    }

    #[test]
    fn a_generated_token_carries_256_bits() {
        let token = generate_approval_token();
        assert_eq!(URL_SAFE_NO_PAD.decode(&token).unwrap().len(), 32);
        assert_ne!(token, generate_approval_token());
    }

    // ── Reachability: the AB-71 trap ─────────────────────────────────────────
    //
    // auto-buyer shipped this feature with its approval route behind the app's
    // auth middleware, so the emailed link redirected to login and the API
    // 401'd. It was invisible because the tests called the handlers directly.
    // These build the guarded scope the real app builds and go through it.

    /// An app shaped like `main.rs`: the approval routes mounted before a
    /// guarded `/api` scope, with a control route inside that scope.
    macro_rules! guarded_app {
        ($state:expr, $guard:expr) => {
            http_test::init_service(
                App::new()
                    .app_data($state)
                    .configure(configure_routes)
                    .service(web::scope("/api").wrap($guard).route(
                        "/me",
                        web::get().to(|| async { HttpResponse::Ok().body("me") }),
                    )),
            )
            .await
        };
    }

    /// The unauthenticated control: whatever the guard is, it must reject.
    #[actix_web::test]
    async fn the_guarded_scope_really_is_guarded_in_this_test_app() {
        let state = make_test_state();
        #[cfg(feature = "standalone")]
        let app = guarded_app!(
            state,
            actix_web_httpauth::middleware::HttpAuthentication::bearer(
                crate::auth::middleware::jwt_validator
            )
        );
        #[cfg(feature = "saas")]
        let app = guarded_app!(
            state,
            actix_web::middleware::from_fn(crate::oidc::require_session)
        );

        let req = http_test::TestRequest::get()
            .uri("/api/me")
            .peer_addr(PEER.parse().unwrap())
            .to_request();
        assert_eq!(http_test::call_service(&app, req).await.status(), 401);
    }

    #[actix_web::test]
    async fn the_approval_page_is_reachable_with_no_session() {
        let state = make_test_state();
        #[cfg(feature = "standalone")]
        let app = guarded_app!(
            state,
            actix_web_httpauth::middleware::HttpAuthentication::bearer(
                crate::auth::middleware::jwt_validator
            )
        );
        #[cfg(feature = "saas")]
        let app = guarded_app!(
            state,
            actix_web::middleware::from_fn(crate::oidc::require_session)
        );

        let req = http_test::TestRequest::get()
            .uri(APPROVAL_PAGE_PATH)
            .peer_addr(PEER.parse().unwrap())
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            200,
            "the emailed link must not need a session"
        );
    }

    #[actix_web::test]
    async fn the_approval_api_is_reachable_with_no_session() {
        let state = state_with_mail(deliverable_mail());
        let user_id = insert_gated_user(&state, "alice");
        let token = seed_pending(&state, user_id, 15);
        #[cfg(feature = "standalone")]
        let app = guarded_app!(
            state,
            actix_web_httpauth::middleware::HttpAuthentication::bearer(
                crate::auth::middleware::jwt_validator
            )
        );
        #[cfg(feature = "saas")]
        let app = guarded_app!(
            state,
            actix_web::middleware::from_fn(crate::oidc::require_session)
        );

        // GET validates without consuming.
        let req = http_test::TestRequest::get()
            .uri(&format!("{APPROVAL_API_PATH}?token={token}"))
            .peer_addr(PEER.parse().unwrap())
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            200,
            "the page must be able to validate a link"
        );

        // POST claims it and hands back a session.
        let req = http_test::TestRequest::post()
            .uri(APPROVAL_API_PATH)
            .peer_addr(PEER.parse().unwrap())
            .set_json(serde_json::json!({ "token": token }))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200, "approving must not need a session");
    }

    #[actix_web::test]
    async fn approving_establishes_a_session_and_records_the_country() {
        let state = state_with_mail(deliverable_mail());
        let user_id = insert_gated_user(&state, "alice");
        let token = seed_pending(&state, user_id, 15);
        let probe = state.clone();
        #[cfg(feature = "standalone")]
        let app = guarded_app!(
            state,
            actix_web_httpauth::middleware::HttpAuthentication::bearer(
                crate::auth::middleware::jwt_validator
            )
        );
        #[cfg(feature = "saas")]
        let app = guarded_app!(
            state,
            actix_web::middleware::from_fn(crate::oidc::require_session)
        );

        let req = http_test::TestRequest::post()
            .uri(APPROVAL_API_PATH)
            .peer_addr(PEER.parse().unwrap())
            .set_json(serde_json::json!({ "token": token }))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        #[cfg(feature = "standalone")]
        {
            let body: serde_json::Value = http_test::read_body_json(resp).await;
            assert!(body["token"].as_str().is_some_and(|t| !t.is_empty()));
        }
        #[cfg(feature = "saas")]
        {
            let cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
            assert!(cookie.contains(crate::oidc::RUS_SESSION_COOKIE));
        }

        let db = probe.db.lock().unwrap();
        let country: Option<String> = db
            .query_row(
                "SELECT last_login_country FROM users WHERE userID = ?1",
                params![user_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(country.as_deref(), Some("DE"));
    }

    #[actix_web::test]
    async fn a_spent_link_is_gone_for_the_second_click() {
        let state = state_with_mail(deliverable_mail());
        let user_id = insert_gated_user(&state, "alice");
        let token = seed_pending(&state, user_id, 15);
        #[cfg(feature = "standalone")]
        let app = guarded_app!(
            state,
            actix_web_httpauth::middleware::HttpAuthentication::bearer(
                crate::auth::middleware::jwt_validator
            )
        );
        #[cfg(feature = "saas")]
        let app = guarded_app!(
            state,
            actix_web::middleware::from_fn(crate::oidc::require_session)
        );

        for expected in [200, 410] {
            let req = http_test::TestRequest::post()
                .uri(APPROVAL_API_PATH)
                .peer_addr(PEER.parse().unwrap())
                .set_json(serde_json::json!({ "token": token }))
                .to_request();
            assert_eq!(http_test::call_service(&app, req).await.status(), expected);
        }
    }

    // ── Route-level gate evaluation ──────────────────────────────────────────
    //
    // This mounts the exact entry point both legs call behind a real route, so
    // the header and peer resolution that feed the decision are exercised
    // through actix in both legs. Each leg also covers its own real sign-in
    // route: `handlers::auth` for the standalone login, and `oidc::rp::tests`
    // for the saas callback, driven against a stubbed OP (RUS-22).

    async fn gate_probe(data: web::Data<AppState>, req: HttpRequest) -> HttpResponse {
        let db = data.db.lock().unwrap_or_else(|e| e.into_inner());
        let user_id: i64 = db
            .query_row("SELECT userID FROM users LIMIT 1", [], |row| row.get(0))
            .unwrap();
        let context = gate_login(&db, &data.config.mail, user_id, &req).unwrap();
        HttpResponse::Ok().json(serde_json::json!({
            "held": matches!(context.decision, GateDecision::Hold(_)),
            "country": context.country,
        }))
    }

    async fn run_gate_probe(
        last_country: Option<&str>,
        country_header: Option<&str>,
    ) -> serde_json::Value {
        init_test_proxies();
        let state = state_with_mail(deliverable_mail());
        let user_id = insert_gated_user(&state, "alice");
        {
            let db = state.db.lock().unwrap();
            db.execute(
                "UPDATE users SET email = 'alice@example.com', last_login_country = ?1
                 WHERE userID = ?2",
                params![last_country, user_id],
            )
            .unwrap();
        }
        let app = http_test::init_service(
            App::new()
                .app_data(state)
                .route("/probe", web::get().to(gate_probe)),
        )
        .await;

        let mut req = http_test::TestRequest::get()
            .uri("/probe")
            .peer_addr(PEER.parse().unwrap());
        if let Some(value) = country_header {
            req = req.insert_header(("X-IPCountry", value));
        }
        http_test::call_and_read_body_json(&app, req.to_request()).await
    }

    #[actix_web::test]
    async fn route_level_a_first_ever_sign_in_is_never_held() {
        let body = run_gate_probe(None, Some("DE")).await;
        assert_eq!(body["country"], "DE");
        assert_eq!(body["held"], false);
    }

    #[actix_web::test]
    async fn route_level_an_unresolved_country_is_never_held() {
        let body = run_gate_probe(Some("US"), None).await;
        assert!(body["country"].is_null());
        assert_eq!(body["held"], false);
    }

    #[actix_web::test]
    async fn route_level_a_new_country_is_held() {
        let body = run_gate_probe(Some("US"), Some("DE")).await;
        assert_eq!(body["held"], true);
    }

    // ── Wiring: where main.rs mounts all of this ─────────────────────────────

    /// Byte offsets of every occurrence of `needle` in `main.rs`.
    fn positions(needle: &str) -> Vec<usize> {
        MAIN_RS.match_indices(needle).map(|(i, _)| i).collect()
    }

    /// The reachability tests above prove the routes work outside a guarded
    /// scope; this proves `main.rs` actually mounts them there, in both legs.
    /// Together they are what AB-67 was missing.
    #[test]
    fn main_mounts_the_approval_routes_ahead_of_every_guard() {
        let mounted = positions("configure(login_approval::configure_routes)");
        let guarded_scopes = positions("web::scope(\"/api\")");
        let catch_all = positions("route(\"/{code}\"");

        assert_eq!(mounted.len(), 2, "one mount per feature leg");
        assert_eq!(guarded_scopes.len(), 2, "one guarded /api scope per leg");
        assert_eq!(catch_all.len(), 2, "one short-code catch-all per leg");

        for (leg, (mount, scope)) in mounted.iter().zip(&guarded_scopes).enumerate() {
            assert!(
                mount < scope,
                "leg {leg}: the approval routes must be mounted before the guarded /api scope, \
                 or the session check answers them first"
            );
        }
        for (leg, (mount, code)) in mounted.iter().zip(&catch_all).enumerate() {
            assert!(
                mount < code,
                "leg {leg}: the approval page must be mounted before /{{code}}, or the \
                 short-code redirect swallows it"
            );
        }
    }
}
