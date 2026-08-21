//! RUS-7: detect a significant login-location change and notify the operator.
//!
//! Ported from menkent (MKT-61) by way of storefront (SF-28) and rusty-links
//! (LINKS-27). The country is resolved at the edge from the reverse proxy's
//! `X-IPCountry` header rather than an in-process geoip database, so there is
//! nothing to provision or license. Granularity is country-level.
//!
//! The header carries the same trust level as the forwarded IP, which this app
//! believes as-is. Behind the geoblock edge it is authoritative; with no edge
//! (a private IP, a direct client, an unset header) the country is `None` and
//! no alert ever fires, so the feature degrades cleanly rather than raising a
//! false alarm.
//!
//! Accounts here have no email address of their own, so the alert goes to one
//! operator mailbox (`SECURITY_ALERT_EMAIL`) and names the account involved.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use actix_web::{web, HttpRequest};
use rusqlite::{params, Connection};

use crate::db::AppState;

/// Best-effort cap of one alert per account per country per day. The durable
/// dedup is `last_login_country`; this only blunts a burst within one process.
static ALERT_DEDUP: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();

const ALERT_DEDUP_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// The client IP, preferring the forwarding headers, then the socket peer.
pub fn client_ip(req: &HttpRequest) -> Option<String> {
    let headers = req.headers();
    if let Some(forwarded) = headers.get("X-Forwarded-For").and_then(|v| v.to_str().ok()) {
        if let Some(ip) = forwarded.split(',').next().map(str::trim) {
            if !ip.is_empty() {
                return Some(ip.to_string());
            }
        }
    }

    if let Some(ip) = headers
        .get("X-Real-IP")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|ip| !ip.is_empty())
    {
        return Some(ip.to_string());
    }

    req.peer_addr().map(|addr| addr.ip().to_string())
}

/// The country the edge resolved this request to.
///
/// Accepted only as an ISO-3166-1 alpha-2 code; anything else (absent, empty,
/// a sentinel, a three-letter code) resolves to `None` and never raises an
/// alert.
pub fn client_country(req: &HttpRequest) -> Option<String> {
    req.headers()
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

/// Read the account name, last-known country, and opt-out flag for a user.
fn get_login_location(
    conn: &Connection,
    user_id: i64,
) -> rusqlite::Result<Option<(String, Option<String>, bool)>> {
    let row = conn
        .query_row(
            "SELECT username, last_login_country, notify_new_location FROM users WHERE userID = ?1",
            params![user_id],
            |row| {
                let username: String = row.get(0)?;
                let country: Option<String> = row.get(1)?;
                let notify: i64 = row.get(2)?;
                Ok((username, country, notify != 0))
            },
        )
        .map(Some);

    match row {
        Ok(found) => Ok(found),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
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

    let Some((username, previous, notify_new_location)) = found else {
        return Ok(());
    };

    if should_alert(notify_new_location, previous.as_deref(), Some(country))
        && allow_alert(user_id, country)
    {
        tracing::warn!(
            user_id,
            username = %username,
            country = %country,
            "RUS-7: new sign-in from a previously unseen country"
        );
        crate::mailer::send_new_signin_location_alert(
            &state.config.mail,
            &username,
            country,
            ip,
            device,
        )
        .await?;
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

    // The edge header is normalized to an uppercase alpha-2 code.
    #[test]
    fn client_country_reads_and_normalizes_the_edge_header() {
        let req = TestRequest::default()
            .insert_header(("X-IPCountry", "us"))
            .to_http_request();
        assert_eq!(client_country(&req), Some("US".to_string()));
    }

    // No edge header (a direct client, no geoblock) means no country, so the
    // feature degrades to no alert.
    #[test]
    fn client_country_is_none_when_header_absent() {
        let req = TestRequest::default().to_http_request();
        assert_eq!(client_country(&req), None);
    }

    // Empty, sentinel, or wrong-shaped values are rejected rather than treated
    // as a country.
    #[test]
    fn client_country_rejects_malformed_values() {
        for bad in ["", "nil", "U", "USA", "1A", "  "] {
            let req = TestRequest::default()
                .insert_header(("X-IPCountry", bad))
                .to_http_request();
            assert_eq!(
                client_country(&req),
                None,
                "expected {bad:?} to be rejected"
            );
        }
    }

    // The forwarded header wins over the socket peer, and the leftmost entry is
    // the client.
    #[test]
    fn client_ip_prefers_the_forwarded_header() {
        let req = TestRequest::default()
            .insert_header(("X-Forwarded-For", "203.0.113.7, 10.0.0.1"))
            .to_http_request();
        assert_eq!(client_ip(&req), Some("203.0.113.7".to_string()));
    }

    // X-Real-IP is the fallback when there is no X-Forwarded-For.
    #[test]
    fn client_ip_falls_back_to_real_ip() {
        let req = TestRequest::default()
            .insert_header(("X-Real-IP", "203.0.113.9"))
            .to_http_request();
        assert_eq!(client_ip(&req), Some("203.0.113.9".to_string()));
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

    // The columns round-trip, and the opt-out default is on.
    #[test]
    fn login_location_round_trips_through_the_database() {
        let state = crate::testing::make_test_state();
        let db = state.db.lock().unwrap();
        let user_id = insert_user(&db, "alice");

        let (username, previous, notify) = get_login_location(&db, user_id).unwrap().unwrap();
        assert_eq!(username, "alice");
        assert_eq!(previous, None, "a new account has no prior country");
        assert!(notify, "alerts default to on");

        update_last_login_country(&db, user_id, "DE").unwrap();
        let (_, previous, _) = get_login_location(&db, user_id).unwrap().unwrap();
        assert_eq!(previous, Some("DE".to_string()));
    }

    // A missing account resolves to None rather than erroring the task.
    #[test]
    fn login_location_of_unknown_user_is_none() {
        let state = crate::testing::make_test_state();
        let db = state.db.lock().unwrap();
        assert!(get_login_location(&db, 987_654).unwrap().is_none());
    }
}
