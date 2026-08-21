//! RUS-7: detect a significant login-location change and notify the operator.
//!
//! Ported from menkent (MKT-61) by way of storefront (SF-28) and rusty-links
//! (LINKS-27). The country is resolved at the edge from the reverse proxy's
//! `X-IPCountry` header rather than an in-process geoip database, so there is
//! nothing to provision or license. Granularity is country-level.
//!
//! The header carries the same trust level as the forwarded IP, so RUS-12 gates
//! both on the socket peer: `X-Forwarded-For`, `X-Real-IP`, and `X-IPCountry`
//! are read only when the peer is a configured trusted proxy (see
//! `TRUSTED_PROXY_CIDRS`), and are ignored otherwise. Behind the geoblock edge
//! they are authoritative; off it (a direct client, no configured proxy, an
//! unset header) the country is `None` and no alert ever fires, so a forged
//! header can neither raise a false alarm nor suppress a real one.
//!
//! RUS-11 routes the notice to the account owner: it goes to the account's own
//! `users.email` when set (populated from the OP identity in saas mode, from
//! the account itself in standalone), falls back to the shared operator mailbox
//! `SECURITY_ALERT_EMAIL` when the account has no address, and is logged rather
//! than sent when neither exists or SMTP is unconfigured. The per-account
//! `notify_new_location` opt-out is evaluated before any of that, so it
//! suppresses the alert on every route.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use actix_web::http::header::HeaderMap;
use actix_web::{web, HttpRequest};
use ipnetwork::IpNetwork;
use rusqlite::{params, Connection};

use crate::config::MailConfig;
use crate::db::AppState;
use crate::mailer::AlertRecipient;

/// Best-effort cap of one alert per account per country per day. The durable
/// dedup is `last_login_country`; this only blunts a burst within one process.
static ALERT_DEDUP: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();

const ALERT_DEDUP_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// RUS-12: CIDRs whose socket peers may set `X-Forwarded-For`, `X-Real-IP`, and
/// `X-IPCountry`. Installed once at startup by [`init_trusted_proxies`]; unset
/// means empty, so forwarded headers are never trusted unless an operator
/// opts in.
static TRUSTED_PROXIES: OnceLock<Vec<IpNetwork>> = OnceLock::new();

/// Install the trusted-proxy CIDRs (from `Config::trusted_proxy_cidrs`). Call
/// once during startup, before the server accepts requests; a later call is
/// ignored so a request can never swap the trust set mid-flight.
pub fn init_trusted_proxies(cidrs: Vec<IpNetwork>) {
    let _ = TRUSTED_PROXIES.set(cidrs);
}

fn trusted_proxies() -> &'static [IpNetwork] {
    TRUSTED_PROXIES.get().map_or(&[], Vec::as_slice)
}

fn is_trusted(ip: IpAddr, trusted: &[IpNetwork]) -> bool {
    trusted.iter().any(|net| net.contains(ip))
}

/// The socket peer address, the one input a client cannot forge.
fn peer_ip(req: &HttpRequest) -> Option<IpAddr> {
    req.peer_addr().map(|addr| addr.ip())
}

/// The client IP, honoring the forwarding headers only from a trusted proxy.
pub fn client_ip(req: &HttpRequest) -> Option<String> {
    resolve_client_ip(peer_ip(req), req.headers(), trusted_proxies()).map(|ip| ip.to_string())
}

/// Resolve the client IP from the socket peer plus the forwarded headers.
///
/// The peer is the only non-forgeable input, so it gates everything: a
/// forwarded header is read only when the peer itself sits in `trusted`. With
/// no trusted proxies configured a forged `X-Forwarded-For` / `X-Real-IP` is
/// ignored entirely. Inside a proxy chain the rightmost entry not belonging to
/// a trusted proxy is the client, since anything further left was supplied by
/// the client and can be forged.
pub fn resolve_client_ip(
    peer: Option<IpAddr>,
    headers: &HeaderMap,
    trusted: &[IpNetwork],
) -> Option<IpAddr> {
    let peer = peer?;

    if !is_trusted(peer, trusted) {
        return Some(peer);
    }

    if let Some(forwarded) = headers.get("X-Forwarded-For").and_then(|v| v.to_str().ok()) {
        let client = forwarded
            .split(',')
            .filter_map(|entry| entry.trim().parse::<IpAddr>().ok())
            .rev()
            .find(|ip| !is_trusted(*ip, trusted));
        if let Some(ip) = client {
            return Some(ip);
        }
    }

    if let Some(ip) = headers
        .get("X-Real-IP")
        .and_then(|v| v.to_str().ok())
        .and_then(|value| value.trim().parse().ok())
    {
        return Some(ip);
    }

    Some(peer)
}

/// The country the edge resolved this request to.
///
/// Accepted only from a trusted-proxy peer (RUS-12) and only as an ISO-3166-1
/// alpha-2 code; anything else (an untrusted peer, absent, empty, a sentinel, a
/// three-letter code) resolves to `None` and never raises an alert.
pub fn client_country(req: &HttpRequest) -> Option<String> {
    resolve_client_country(peer_ip(req), req.headers(), trusted_proxies())
}

/// Resolve the edge country, gated on the same trusted peer as the client IP.
///
/// An untrusted peer sets this header itself, so believing it would let a
/// direct client either fake a foreign sign-in or pin every login to one
/// country and silence the alert.
pub fn resolve_client_country(
    peer: Option<IpAddr>,
    headers: &HeaderMap,
    trusted: &[IpNetwork],
) -> Option<String> {
    if !peer.is_some_and(|ip| is_trusted(ip, trusted)) {
        return None;
    }

    headers
        .get("X-IPCountry")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_ascii_uppercase())
        .filter(|value| value.len() == 2 && value.bytes().all(|b| b.is_ascii_alphabetic()))
}

/// The requesting device, from the User-Agent header, truncated for storage.
pub fn device_info(req: &HttpRequest) -> Option<String> {
    req.headers()
        .get("User-Agent")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(256).collect())
}

/// Whether a login warrants a new-location alert.
///
/// A change is significant only when a prior country is known and the new one
/// differs. A first-ever login (no `previous`) and a repeat from the same
/// country are both silent; an unresolved `current` never alerts; and an
/// account that has opted out is never alerted on.
pub fn should_alert(
    notify_new_location: bool,
    previous: Option<&str>,
    current: Option<&str>,
) -> bool {
    notify_new_location
        && matches!((previous, current), (Some(prev), Some(curr)) if !prev.eq_ignore_ascii_case(curr))
}

/// True the first time this account is alerted about this country within a day.
fn allow_alert(user_id: i64, country: &str) -> bool {
    let cache = ALERT_DEDUP.get_or_init(|| Mutex::new(HashMap::new()));
    let mut seen = cache.lock().unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
    // Pruning here also bounds the map, so it cannot grow without limit.
    seen.retain(|_, at| now.duration_since(*at) < ALERT_DEDUP_TTL);
    let key = format!("{user_id}:{country}");
    if seen.contains_key(&key) {
        return false;
    }
    seen.insert(key, now);
    true
}

/// The per-account inputs the alert decision needs.
#[derive(Clone, Debug)]
struct AccountAlertInfo {
    username: String,
    /// The account's own address, or `None` when it has never set one.
    email: Option<String>,
    last_country: Option<String>,
    notify_new_location: bool,
}

/// Read the account name, its own address, last-known country, and opt-out flag.
///
/// The `email` column exists in both schemas (saas ships it; RUS-11 migrates it
/// into standalone), so this one query serves both feature legs.
fn get_login_location(
    conn: &Connection,
    user_id: i64,
) -> rusqlite::Result<Option<AccountAlertInfo>> {
    let row = conn
        .query_row(
            "SELECT username, email, last_login_country, notify_new_location
             FROM users WHERE userID = ?1",
            params![user_id],
            |row| {
                Ok(AccountAlertInfo {
                    username: row.get(0)?,
                    email: row.get(1)?,
                    last_country: row.get(2)?,
                    notify_new_location: row.get::<_, i64>(3)? != 0,
                })
            },
        )
        .map(Some);

    match row {
        Ok(found) => Ok(found),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

/// What one login's alert evaluation decided.
#[derive(Clone, Debug, PartialEq, Eq)]
enum AlertRoute {
    /// No alert at all: no country change, or the account opted out.
    Silent,
    /// Alert, addressed to this recipient.
    Notify(AlertRecipient),
    /// Alert-worthy, but there is nothing to deliver it with: log only.
    LogOnly,
}

/// Decide whether this login alerts and, if so, who hears about it.
///
/// The opt-out is checked first, so it suppresses the alert whichever way it
/// would otherwise have been routed.
fn alert_route(mail: &MailConfig, info: &AccountAlertInfo, current: Option<&str>) -> AlertRoute {
    if !should_alert(
        info.notify_new_location,
        info.last_country.as_deref(),
        current,
    ) {
        return AlertRoute::Silent;
    }
    match crate::mailer::resolve_recipient(mail, info.email.as_deref()) {
        Some(recipient) if mail.smtp_ready() => AlertRoute::Notify(recipient),
        _ => AlertRoute::LogOnly,
    }
}

/// Record the country this account most recently signed in from.
fn update_last_login_country(
    conn: &Connection,
    user_id: i64,
    country: &str,
) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE users SET last_login_country = ?1 WHERE userID = ?2",
        params![country, user_id],
    )
}

/// Read this account's new-location alert preference (RUS-15). `None` when the
/// account does not exist.
pub fn get_notify_new_location(conn: &Connection, user_id: i64) -> rusqlite::Result<Option<bool>> {
    let row = conn
        .query_row(
            "SELECT notify_new_location FROM users WHERE userID = ?1",
            params![user_id],
            |row| Ok(row.get::<_, i64>(0)? != 0),
        )
        .map(Some);

    match row {
        Ok(found) => Ok(found),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Persist this account's new-location alert preference (RUS-15). Returns the
/// number of rows changed, so 0 means the account does not exist.
pub fn set_notify_new_location(
    conn: &Connection,
    user_id: i64,
    enabled: bool,
) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE users SET notify_new_location = ?1 WHERE userID = ?2",
        params![enabled as i64, user_id],
    )
}

/// Evaluate the new-location alert off the login hot path.
///
/// Resolves the country from the request and, when the global kill switch is on
/// and a country is resolvable, spawns a detached task that compares it against
/// the account's last-known country, emails the operator on a significant
/// change, and records the new country for next time. Every failure inside the
/// task is logged, never surfaced, so the alert can never fail or slow a login.
pub fn spawn_new_location_check(state: &web::Data<AppState>, user_id: i64, req: &HttpRequest) {
    if !state.config.mail.login_location_alerts_enabled {
        return;
    }
    let Some(country) = client_country(req) else {
        return;
    };
    let state = state.clone();
    let ip = client_ip(req).unwrap_or_else(|| "unknown".to_string());
    let device = device_info(req);

    // Local spawn: rusqlite's connection is not Sync, and actix runs a
    // current-thread runtime per worker, so the task starts once this handler
    // has returned and released the database lock.
    actix_web::rt::spawn(async move {
        if let Err(error) =
            maybe_notify_new_location(&state, user_id, &country, &ip, device.as_deref()).await
        {
            tracing::warn!(
                user_id,
                error = %error,
                "RUS-7: new-location alert failed"
            );
        }
    });
}

/// Compare the login country against the account's last-known, alert on a
/// significant change (subject to the opt-out and a best-effort
/// once-per-country-per-day cap), then record the new country. The last-country
/// write is the durable dedup: a repeat from the same country is silent on the
/// next login regardless of the in-memory cap.
async fn maybe_notify_new_location(
    state: &web::Data<AppState>,
    user_id: i64,
    country: &str,
    ip: &str,
    device: Option<&str>,
) -> Result<(), String> {
    let found = {
        let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
        get_login_location(&db, user_id).map_err(|e| e.to_string())?
    };

    let Some(info) = found else {
        return Ok(());
    };

    let route = alert_route(&state.config.mail, &info, Some(country));
    // The dedup cap is consumed only once an alert is warranted, so an opted-out
    // or unchanged login never burns the account's daily slot.
    if route != AlertRoute::Silent && allow_alert(user_id, country) {
        tracing::warn!(
            user_id,
            username = %info.username,
            country = %country,
            "RUS-7: new sign-in from a previously unseen country"
        );
        match route {
            AlertRoute::Notify(recipient) => {
                crate::mailer::send_new_signin_location_alert(
                    &state.config.mail,
                    &recipient,
                    &info.username,
                    country,
                    ip,
                    device,
                )
                .await?;
            }
            // No recipient or no SMTP: the notice still has to leave a trace.
            // Silent is excluded by the guard above; logging it beats panicking
            // in a task whose whole point is never to break a login.
            _ => {
                let (subject, body) =
                    crate::mailer::undeliverable_alert_text(&info.username, country, ip, device);
                crate::mailer::log_undelivered_alert(&info.username, &subject, &body);
            }
        }
    }

    {
        let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
        update_last_login_country(&db, user_id, country).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;

    /// Build a bare HeaderMap for the peer-gated resolvers.
    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut req = TestRequest::default();
        for (name, value) in pairs {
            req = req.insert_header((*name, *value));
        }
        req.to_http_request().headers().clone()
    }

    /// Build a request with a socket peer, as actix sees it off the wire.
    fn request_from(peer: &str, pairs: &[(&str, &str)]) -> HttpRequest {
        let mut req = TestRequest::default().peer_addr(peer.parse().unwrap());
        for (name, value) in pairs {
            req = req.insert_header((*name, *value));
        }
        req.to_http_request()
    }

    fn ip(value: &str) -> IpAddr {
        value.parse().unwrap()
    }

    fn cidrs(list: &[&str]) -> Vec<IpNetwork> {
        list.iter().map(|entry| entry.parse().unwrap()).collect()
    }

    // A repeat login from the same country is not a change (case-insensitive:
    // the edge may vary casing between logins).
    #[test]
    fn same_country_does_not_alert() {
        assert!(!should_alert(true, Some("US"), Some("US")));
        assert!(!should_alert(true, Some("us"), Some("US")));
    }

    // A country change alerts.
    #[test]
    fn country_change_alerts() {
        assert!(should_alert(true, Some("US"), Some("DE")));
    }

    // An account's first-ever login (no prior country) is never flagged.
    #[test]
    fn first_login_does_not_alert() {
        assert!(!should_alert(true, None, Some("US")));
    }

    // A private / unresolvable IP yields no country, so no alert (no panic).
    #[test]
    fn unresolved_country_does_not_alert() {
        assert!(!should_alert(true, Some("US"), None));
        assert!(!should_alert(true, None, None));
    }

    // The per-account opt-out suppresses the alert even on a real change.
    #[test]
    fn opt_out_suppresses_alert() {
        assert!(!should_alert(false, Some("US"), Some("DE")));
    }

    // A trusted proxy's edge header is normalized to an uppercase alpha-2 code.
    #[test]
    fn client_country_reads_and_normalizes_the_edge_header() {
        assert_eq!(
            resolve_client_country(
                Some(ip("10.1.2.3")),
                &headers(&[("X-IPCountry", "us")]),
                &cidrs(&["10.0.0.0/8"]),
            ),
            Some("US".to_string())
        );
    }

    // No edge header (a direct client, no geoblock) means no country, so the
    // feature degrades to no alert.
    #[test]
    fn client_country_is_none_when_header_absent() {
        assert_eq!(
            resolve_client_country(Some(ip("10.1.2.3")), &headers(&[]), &cidrs(&["10.0.0.0/8"])),
            None
        );
    }

    // Empty, sentinel, or wrong-shaped values are rejected rather than treated
    // as a country, even from a trusted proxy.
    #[test]
    fn client_country_rejects_malformed_values() {
        for bad in ["", "nil", "U", "USA", "1A", "  "] {
            assert_eq!(
                resolve_client_country(
                    Some(ip("10.1.2.3")),
                    &headers(&[("X-IPCountry", bad)]),
                    &cidrs(&["10.0.0.0/8"]),
                ),
                None,
                "expected {bad:?} to be rejected"
            );
        }
    }

    // RUS-12: an untrusted peer sets the country itself, so it must resolve to
    // None and never raise or suppress an alert.
    #[test]
    fn client_country_is_none_from_an_untrusted_peer() {
        assert_eq!(
            resolve_client_country(
                Some(ip("203.0.113.5")),
                &headers(&[("X-IPCountry", "DE")]),
                &cidrs(&["10.0.0.0/8"]),
            ),
            None
        );
        // Nothing configured trusts nothing, which is the shipped default.
        assert_eq!(
            resolve_client_country(
                Some(ip("10.1.2.3")),
                &headers(&[("X-IPCountry", "DE")]),
                &[]
            ),
            None
        );
    }

    // No peer at all (no socket address) trusts nothing.
    #[test]
    fn client_country_is_none_without_a_peer() {
        assert_eq!(
            resolve_client_country(
                None,
                &headers(&[("X-IPCountry", "DE")]),
                &cidrs(&["10.0.0.0/8"]),
            ),
            None
        );
    }

    // RUS-12: with no trusted proxies, forged forwarded headers must not move
    // the resolved IP off the socket peer.
    #[test]
    fn resolve_client_ip_ignores_forwarded_headers_without_trusted_proxies() {
        let h = headers(&[
            ("X-Forwarded-For", "9.9.9.9, 8.8.8.8"),
            ("X-Real-IP", "7.7.7.7"),
        ]);
        assert_eq!(
            resolve_client_ip(Some(ip("203.0.113.5")), &h, &[]),
            Some(ip("203.0.113.5"))
        );
    }

    // A peer outside the trusted set is ignored just the same.
    #[test]
    fn resolve_client_ip_ignores_forwarded_headers_from_untrusted_peer() {
        let h = headers(&[("X-Forwarded-For", "9.9.9.9")]);
        assert_eq!(
            resolve_client_ip(Some(ip("203.0.113.5")), &h, &cidrs(&["10.0.0.0/8"])),
            Some(ip("203.0.113.5"))
        );
    }

    // A trusted proxy's X-Forwarded-For is believed.
    #[test]
    fn resolve_client_ip_honors_forwarded_for_from_trusted_proxy() {
        let h = headers(&[("X-Forwarded-For", "203.0.113.5")]);
        assert_eq!(
            resolve_client_ip(Some(ip("10.1.2.3")), &h, &cidrs(&["10.0.0.0/8"])),
            Some(ip("203.0.113.5"))
        );
    }

    // A client that prepends its own entries cannot hide behind them: the
    // rightmost untrusted entry is the one the trusted proxy observed.
    #[test]
    fn resolve_client_ip_takes_rightmost_untrusted_entry() {
        let h = headers(&[("X-Forwarded-For", "1.2.3.4, 203.0.113.5, 10.9.9.9")]);
        assert_eq!(
            resolve_client_ip(Some(ip("10.1.2.3")), &h, &cidrs(&["10.0.0.0/8"])),
            Some(ip("203.0.113.5"))
        );
    }

    // X-Real-IP is the fallback when there is no X-Forwarded-For.
    #[test]
    fn resolve_client_ip_falls_back_to_real_ip() {
        let h = headers(&[("X-Real-IP", "203.0.113.9")]);
        assert_eq!(
            resolve_client_ip(Some(ip("10.1.2.3")), &h, &cidrs(&["10.0.0.0/8"])),
            Some(ip("203.0.113.9"))
        );
    }

    // Every entry trusted and no X-Real-IP: nothing untrusted was observed, so
    // fall back to the peer rather than inventing a client.
    #[test]
    fn resolve_client_ip_falls_back_to_peer_when_all_entries_trusted() {
        let h = headers(&[("X-Forwarded-For", "10.9.9.9, 10.8.8.8")]);
        assert_eq!(
            resolve_client_ip(Some(ip("10.1.2.3")), &h, &cidrs(&["10.0.0.0/8"])),
            Some(ip("10.1.2.3"))
        );
    }

    // No peer means no IP; there is nothing non-forgeable left to trust.
    #[test]
    fn resolve_client_ip_is_none_without_a_peer() {
        let h = headers(&[("X-Forwarded-For", "203.0.113.5")]);
        assert_eq!(resolve_client_ip(None, &h, &cidrs(&["10.0.0.0/8"])), None);
    }

    // IPv6 proxies (the fd00::/8 ingress case) work the same way.
    #[test]
    fn resolve_client_ip_supports_ipv6_proxies() {
        let h = headers(&[("X-Forwarded-For", "203.0.113.5")]);
        assert_eq!(
            resolve_client_ip(Some(ip("fd00::5")), &h, &cidrs(&["fd00::/8"])),
            Some(ip("203.0.113.5"))
        );
    }

    // End to end through HttpRequest: with the process-wide trust set unset (no
    // TRUSTED_PROXY_CIDRS), a direct client's forged headers buy it nothing.
    #[test]
    fn request_readers_ignore_forged_headers_by_default() {
        let req = request_from(
            "203.0.113.5:44321",
            &[
                ("X-Forwarded-For", "9.9.9.9"),
                ("X-Real-IP", "7.7.7.7"),
                ("X-IPCountry", "DE"),
            ],
        );
        assert_eq!(client_ip(&req), Some("203.0.113.5".to_string()));
        assert_eq!(client_country(&req), None);
    }

    // The User-Agent becomes the device string, and its absence is None.
    #[test]
    fn device_info_reads_the_user_agent() {
        let req = TestRequest::default()
            .insert_header(("User-Agent", "Firefox"))
            .to_http_request();
        assert_eq!(device_info(&req), Some("Firefox".to_string()));
        assert_eq!(device_info(&TestRequest::default().to_http_request()), None);
    }

    // At most one alert per account per country per day; a different country is
    // a distinct event, and so is a different account.
    #[test]
    fn alerts_are_deduped_per_account_and_country() {
        // A high base id keeps these keys clear of the other tests' ids.
        let user = 900_001;
        assert!(allow_alert(user, "DE"));
        assert!(!allow_alert(user, "DE"));
        assert!(allow_alert(user, "US"));
        assert!(allow_alert(user + 1, "DE"));
    }

    /// Insert a bare user row. Written in SQL rather than through the testing
    /// helpers because those are per-feature, and these columns exist in both
    /// the standalone and saas schemas.
    fn insert_user(conn: &Connection, username: &str) -> i64 {
        conn.execute(
            "INSERT INTO users (username, password) VALUES (?1, '')",
            params![username],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    // RUS-15: the preference defaults to on and both directions persist.
    #[test]
    fn notify_new_location_persists_both_ways() {
        let state = crate::testing::make_test_state();
        let db = state.db.lock().unwrap();
        let user_id = insert_user(&db, "alice");

        assert_eq!(
            get_notify_new_location(&db, user_id).unwrap(),
            Some(true),
            "alerts default to on"
        );

        assert_eq!(set_notify_new_location(&db, user_id, false).unwrap(), 1);
        assert_eq!(get_notify_new_location(&db, user_id).unwrap(), Some(false));

        assert_eq!(set_notify_new_location(&db, user_id, true).unwrap(), 1);
        assert_eq!(get_notify_new_location(&db, user_id).unwrap(), Some(true));
    }

    // A write only ever touches the account it names, so one account's opt-out
    // cannot move another's.
    #[test]
    fn notify_new_location_write_touches_one_account() {
        let state = crate::testing::make_test_state();
        let db = state.db.lock().unwrap();
        let alice = insert_user(&db, "alice");
        let bob = insert_user(&db, "bob");

        set_notify_new_location(&db, alice, false).unwrap();
        assert_eq!(get_notify_new_location(&db, alice).unwrap(), Some(false));
        assert_eq!(get_notify_new_location(&db, bob).unwrap(), Some(true));
    }

    // A missing account reads as None and writes nothing, rather than erroring.
    #[test]
    fn notify_new_location_of_unknown_user_is_none() {
        let state = crate::testing::make_test_state();
        let db = state.db.lock().unwrap();
        assert_eq!(get_notify_new_location(&db, 987_654).unwrap(), None);
        assert_eq!(set_notify_new_location(&db, 987_654, false).unwrap(), 0);
    }

    // The stored preference is the same column the alert decision reads, so
    // opting out silences a genuine country change and opting back in restores it.
    #[test]
    fn stored_preference_drives_the_alert_decision() {
        let state = crate::testing::make_test_state();
        let db = state.db.lock().unwrap();
        let user_id = insert_user(&db, "alice");
        update_last_login_country(&db, user_id, "US").unwrap();

        set_notify_new_location(&db, user_id, false).unwrap();
        let info = get_login_location(&db, user_id).unwrap().unwrap();
        assert!(!should_alert(
            info.notify_new_location,
            info.last_country.as_deref(),
            Some("DE")
        ));

        set_notify_new_location(&db, user_id, true).unwrap();
        let info = get_login_location(&db, user_id).unwrap().unwrap();
        assert!(should_alert(
            info.notify_new_location,
            info.last_country.as_deref(),
            Some("DE")
        ));
    }

    // The columns round-trip, and the opt-out default is on.
    #[test]
    fn login_location_round_trips_through_the_database() {
        let state = crate::testing::make_test_state();
        let db = state.db.lock().unwrap();
        let user_id = insert_user(&db, "alice");

        let info = get_login_location(&db, user_id).unwrap().unwrap();
        assert_eq!(info.username, "alice");
        assert_eq!(
            info.last_country, None,
            "a new account has no prior country"
        );
        assert_eq!(info.email, None, "a new account has no address");
        assert!(info.notify_new_location, "alerts default to on");

        update_last_login_country(&db, user_id, "DE").unwrap();
        let info = get_login_location(&db, user_id).unwrap().unwrap();
        assert_eq!(info.last_country, Some("DE".to_string()));
    }

    // A missing account resolves to None rather than erroring the task.
    #[test]
    fn login_location_of_unknown_user_is_none() {
        let state = crate::testing::make_test_state();
        let db = state.db.lock().unwrap();
        assert!(get_login_location(&db, 987_654).unwrap().is_none());
    }

    // RUS-11: the account's address is read from the users table in both
    // schemas, so the alert can be addressed to its owner.
    #[test]
    fn account_email_is_read_from_the_database() {
        let state = crate::testing::make_test_state();
        let db = state.db.lock().unwrap();
        let user_id = insert_user(&db, "alice");
        db.execute(
            "UPDATE users SET email = 'alice@example.com' WHERE userID = ?1",
            params![user_id],
        )
        .unwrap();

        let info = get_login_location(&db, user_id).unwrap().unwrap();
        assert_eq!(info.email, Some("alice@example.com".to_string()));
    }

    fn mail_with(operator: Option<&str>, smtp: bool) -> MailConfig {
        MailConfig {
            smtp_host: smtp.then(|| "smtp.example.com".to_string()),
            smtp_from_email: smtp.then(|| "alerts@example.com".to_string()),
            security_alert_email: operator.map(String::from),
            ..MailConfig::default()
        }
    }

    fn account(email: Option<&str>, notify: bool) -> AccountAlertInfo {
        AccountAlertInfo {
            username: "alice".to_string(),
            email: email.map(String::from),
            last_country: Some("US".to_string()),
            notify_new_location: notify,
        }
    }

    // RUS-11 precedence, first rule: an account with an address hears about its
    // own sign-in, even when an operator mailbox is also configured.
    #[test]
    fn route_prefers_the_account_owner() {
        assert_eq!(
            alert_route(
                &mail_with(Some("operator@example.com"), true),
                &account(Some("alice@example.com"), true),
                Some("DE"),
            ),
            AlertRoute::Notify(AlertRecipient::Owner("alice@example.com".into()))
        );
    }

    // Second rule: no address on the account falls back to the operator mailbox.
    #[test]
    fn route_falls_back_to_the_operator_mailbox() {
        for stored in [None, Some(""), Some("   ")] {
            assert_eq!(
                alert_route(
                    &mail_with(Some("operator@example.com"), true),
                    &account(stored, true),
                    Some("DE"),
                ),
                AlertRoute::Notify(AlertRecipient::Operator("operator@example.com".into())),
                "expected {stored:?} to fall back"
            );
        }
    }

    // Third rule: neither address configured, or no SMTP to send with, logs the
    // would-be alert instead of sending or erroring.
    #[test]
    fn route_logs_when_there_is_no_way_to_deliver() {
        assert_eq!(
            alert_route(&mail_with(None, true), &account(None, true), Some("DE")),
            AlertRoute::LogOnly,
            "no recipient anywhere"
        );
        assert_eq!(
            alert_route(
                &mail_with(Some("operator@example.com"), false),
                &account(Some("alice@example.com"), true),
                Some("DE"),
            ),
            AlertRoute::LogOnly,
            "an address but no SMTP transport"
        );
    }

    // The opt-out wins over every routing case, including the ones that would
    // otherwise have reached the account owner.
    #[test]
    fn opt_out_suppresses_every_route() {
        let cases = [
            (
                mail_with(Some("operator@example.com"), true),
                Some("alice@example.com"),
            ),
            (mail_with(Some("operator@example.com"), true), None),
            (mail_with(None, true), None),
            (mail_with(None, false), Some("alice@example.com")),
        ];
        for (mail, email) in cases {
            assert_eq!(
                alert_route(&mail, &account(email, false), Some("DE")),
                AlertRoute::Silent,
                "opt-out must suppress with operator={:?} account={email:?}",
                mail.security_alert_email
            );
        }
    }

    // A login that is not a country change stays silent whatever is configured.
    #[test]
    fn route_is_silent_without_a_country_change() {
        let mail = mail_with(Some("operator@example.com"), true);
        assert_eq!(
            alert_route(&mail, &account(Some("alice@example.com"), true), Some("US")),
            AlertRoute::Silent,
            "same country"
        );
        assert_eq!(
            alert_route(&mail, &account(Some("alice@example.com"), true), None),
            AlertRoute::Silent,
            "unresolved country"
        );
    }

    // End to end through the database: a completely unconfigured deployment
    // must not error a login, whichever route the alert would have taken.
    #[actix_web::test]
    async fn unconfigured_alert_never_errors_a_login() {
        let state = crate::testing::make_test_state();
        let user_id = {
            let db = state.db.lock().unwrap();
            let user_id = insert_user(&db, "route_e2e_user");
            db.execute(
                "UPDATE users SET last_login_country = 'US' WHERE userID = ?1",
                params![user_id],
            )
            .unwrap();
            user_id
        };

        let result =
            maybe_notify_new_location(&state, user_id, "DE", "203.0.113.7", Some("Firefox")).await;
        assert!(
            result.is_ok(),
            "a missing mail config must not fail a login"
        );

        let db = state.db.lock().unwrap();
        let info = get_login_location(&db, user_id).unwrap().unwrap();
        assert_eq!(
            info.last_country,
            Some("DE".to_string()),
            "the new country is recorded even when the alert is only logged"
        );
    }

    // RUS-11: in saas mode the address the OP put on the account is the one the
    // alert is addressed to, with no separate column and nothing to set by hand.
    #[cfg(feature = "saas")]
    #[actix_web::test]
    async fn saas_alert_uses_the_oidc_populated_address() {
        let state = crate::testing::make_test_state();
        let db = state.db.lock().unwrap();

        let claims = crate::testing::id_claims(
            "33333333-3333-3333-3333-333333333333",
            Some("sso-user@example.com"),
            true,
            true,
            None,
        );
        let provisioned = crate::oidc::jit::load_or_provision(&db, &claims).unwrap();

        let info = get_login_location(&db, provisioned.user_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            info.email,
            Some("sso-user@example.com".to_string()),
            "the JIT provision stores the OP's address"
        );
        assert_eq!(
            alert_route(
                &mail_with(Some("operator@example.com"), true),
                &AccountAlertInfo {
                    last_country: Some("US".to_string()),
                    ..info
                },
                Some("DE"),
            ),
            AlertRoute::Notify(AlertRecipient::Owner("sso-user@example.com".into())),
            "the OIDC address wins over the operator mailbox"
        );
    }
}
