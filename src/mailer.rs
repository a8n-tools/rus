//! Outbound email for security notifications (RUS-7).
//!
//! Minimal on purpose: one message type, built with `format!` because this
//! crate has no template engine. Users here have no address of their own, so
//! the alert is an operator notification naming which account signed in.
//! Delivery is gated on `MailConfig::deliverable`, so an unconfigured
//! deployment logs the message instead of sending it and a login never depends
//! on mail working. The connection is encrypted unless `SMTP_TLS_MODE=none`
//! explicitly opts out (RUS-16).

use chrono::Utc;
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::config::{MailConfig, SmtpTlsMode};

/// Plain-text body of the new-sign-in-location alert.
const NEW_SIGNIN_LOCATION_BODY: &str = "\
New sign-in to a RUS account from a new country

A sign-in to the RUS account \"{username}\" was detected from a country that account has not signed in from before.

Account: {username}
Country: {country}
When: {timestamp}
IP address: {ip_address}
Device: {device}

If this was the account owner, no action is needed.

If this sign-in is not recognised, someone else may have access to the account. Reset that account's password now and review its active sessions.
";

/// Which lettre constructor a TLS mode selects. Split out so the mapping is
/// unit-testable without reaching into the opaque transport type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransportKind {
    /// `relay`: implicit TLS, default port 465.
    Relay,
    /// `starttls_relay`: STARTTLS upgrade, default port 587.
    StarttlsRelay,
    /// `builder_dangerous`: no encryption, default port 25.
    Dangerous,
}

fn transport_kind(mode: SmtpTlsMode) -> TransportKind {
    match mode {
        SmtpTlsMode::Tls => TransportKind::Relay,
        SmtpTlsMode::Starttls => TransportKind::StarttlsRelay,
        SmtpTlsMode::None => TransportKind::Dangerous,
    }
}

fn smtp_transport(mail: &MailConfig) -> Result<AsyncSmtpTransport<Tokio1Executor>, String> {
    let host = mail
        .smtp_host
        .clone()
        .ok_or_else(|| "SMTP host is missing".to_string())?;
    let mut builder = match transport_kind(mail.smtp_tls_mode) {
        TransportKind::Relay => AsyncSmtpTransport::<Tokio1Executor>::relay(&host)
            .map_err(|error| format!("SMTP TLS transport setup failed: {error}"))?,
        TransportKind::StarttlsRelay => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host)
            .map_err(|error| format!("SMTP STARTTLS transport setup failed: {error}"))?,
        TransportKind::Dangerous => {
            tracing::warn!(
                smtp_host = %host,
                "SMTP_TLS_MODE=none: mail is sent unencrypted, only safe for a trusted local relay"
            );
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&host)
        }
    };
    // relay/starttls_relay already set the right default port for their mode,
    // so an explicit SMTP_PORT stays an override.
    if let Some(port) = mail.smtp_port {
        builder = builder.port(port);
    }
    if let (Some(username), Some(password)) =
        (mail.smtp_username.clone(), mail.smtp_password.clone())
    {
        builder = builder.credentials(Credentials::new(username, password));
    }
    Ok(builder.build())
}

/// Render the alert body. Split out so the interpolation is unit-testable
/// without a mail server.
fn new_signin_location_body(
    username: &str,
    country: &str,
    ip: &str,
    device: Option<&str>,
    timestamp: &str,
) -> String {
    NEW_SIGNIN_LOCATION_BODY
        .replace("{username}", username)
        .replace("{country}", country)
        .replace("{timestamp}", timestamp)
        .replace("{ip_address}", ip)
        .replace("{device}", device.unwrap_or("unknown"))
}

/// Email the operator that an account was signed in to from a new country.
///
/// Returns `Ok` without sending when SMTP or the operator address is
/// unconfigured (log mode), so the caller cannot fail a login on missing mail
/// configuration.
pub async fn send_new_signin_location_alert(
    mail: &MailConfig,
    username: &str,
    country: &str,
    ip: &str,
    device: Option<&str>,
) -> Result<(), String> {
    let subject = format!("New sign-in to RUS account {username} from {country}");
    let body = new_signin_location_body(username, country, ip, device, &Utc::now().to_rfc3339());

    if !mail.deliverable() {
        tracing::warn!(
            delivery_mode = "log",
            username = %username,
            subject,
            "New sign-in location alert NOT sent: delivery in log mode. Set SMTP_HOST, SMTP_FROM_EMAIL, and SECURITY_ALERT_EMAIL to enable SMTP."
        );
        tracing::info!(
            delivery_mode = "log",
            username = %username,
            body = %body,
            "New sign-in location alert body (log mode)"
        );
        return Ok(());
    }

    let from_email = mail
        .smtp_from_email
        .clone()
        .ok_or_else(|| "SMTP sender email is missing".to_string())?;
    let to_email = mail
        .security_alert_email
        .clone()
        .ok_or_else(|| "Security alert email is missing".to_string())?;

    let from_mailbox = Mailbox::new(
        mail.smtp_from_name.clone(),
        from_email
            .parse()
            .map_err(|error| format!("Invalid SMTP from address: {error}"))?,
    );
    let to_mailbox = to_email
        .parse()
        .map_err(|error| format!("Invalid security alert address: {error}"))?;
    let message = Message::builder()
        .from(from_mailbox)
        .to(Mailbox::new(None, to_mailbox))
        .subject(subject)
        .body(body)
        .map_err(|error| format!("New sign-in location alert build failed: {error}"))?;

    match smtp_transport(mail)?.send(message).await {
        Ok(_) => {
            tracing::info!(
                delivery_mode = "smtp",
                delivered = true,
                username = %username,
                "New sign-in location alert delivered"
            );
            Ok(())
        }
        Err(error) => {
            tracing::error!(
                delivery_mode = "smtp",
                delivered = false,
                username = %username,
                error = %error,
                "New sign-in location alert delivery failed"
            );
            Err(format!(
                "New sign-in location alert delivery failed: {error}"
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured() -> MailConfig {
        MailConfig {
            smtp_host: Some("smtp.example.com".into()),
            smtp_from_email: Some("alerts@example.com".into()),
            security_alert_email: Some("operator@example.com".into()),
            ..MailConfig::default()
        }
    }

    // Log mode is the unconfigured default: no SMTP host, sender, or operator
    // address means the alert is logged, never sent, and never errors a login.
    #[actix_web::test]
    async fn unconfigured_mail_logs_instead_of_sending() {
        let mail = MailConfig::default();
        assert!(!mail.deliverable());
        let result =
            send_new_signin_location_alert(&mail, "alice", "DE", "203.0.113.7", None).await;
        assert!(result.is_ok());
    }

    // An operator address alone is not enough to attempt delivery, and neither
    // is SMTP alone: all three parts are required.
    #[test]
    fn deliverable_requires_host_sender_and_operator_address() {
        let mut mail = MailConfig {
            smtp_host: Some("smtp.example.com".into()),
            ..MailConfig::default()
        };
        assert!(!mail.deliverable());
        mail.smtp_from_email = Some("alerts@example.com".into());
        assert!(!mail.deliverable());
        mail.security_alert_email = Some("operator@example.com".into());
        assert!(mail.deliverable());

        let operator_only = MailConfig {
            security_alert_email: Some("operator@example.com".into()),
            ..MailConfig::default()
        };
        assert!(!operator_only.deliverable());
    }

    // A configured operator mailbox is who the alert names as recipient.
    #[test]
    fn configured_sample_is_deliverable() {
        assert!(configured().deliverable());
    }

    // The body carries the account, country, IP, and device the alert is about.
    #[test]
    fn body_interpolates_every_placeholder() {
        let body = new_signin_location_body(
            "alice",
            "DE",
            "203.0.113.7",
            Some("Firefox"),
            "2026-08-21T00:00:00Z",
        );
        assert!(body.contains("Account: alice"));
        assert!(body.contains("Country: DE"));
        assert!(body.contains("IP address: 203.0.113.7"));
        assert!(body.contains("Device: Firefox"));
        assert!(!body.contains('{'));
    }

    // An absent User-Agent renders as a placeholder rather than an empty field.
    #[test]
    fn body_without_device_says_unknown() {
        let body =
            new_signin_location_body("alice", "DE", "203.0.113.7", None, "2026-08-21T00:00:00Z");
        assert!(body.contains("Device: unknown"));
    }

    // Each configured mode picks its own lettre constructor.
    #[test]
    fn tls_mode_selects_its_transport() {
        assert_eq!(
            transport_kind(SmtpTlsMode::Tls),
            TransportKind::Relay,
            "tls means implicit TLS on 465"
        );
        assert_eq!(
            transport_kind(SmtpTlsMode::Starttls),
            TransportKind::StarttlsRelay,
            "starttls means an upgrade on 587"
        );
        assert_eq!(
            transport_kind(SmtpTlsMode::None),
            TransportKind::Dangerous,
            "none keeps the plaintext escape hatch"
        );
    }

    // An unconfigured deployment gets an encrypted transport, not plaintext.
    #[test]
    fn default_config_selects_starttls_transport() {
        assert_eq!(
            transport_kind(MailConfig::default().smtp_tls_mode),
            TransportKind::StarttlsRelay
        );
    }

    // The parsed env value drives the constructor end to end.
    #[test]
    fn parsed_mode_string_selects_its_transport() {
        for (value, expected) in [
            ("TLS", TransportKind::Relay),
            ("StartTLS", TransportKind::StarttlsRelay),
            ("none", TransportKind::Dangerous),
            ("bogus", TransportKind::StarttlsRelay),
        ] {
            assert_eq!(
                transport_kind(SmtpTlsMode::parse(value)),
                expected,
                "SMTP_TLS_MODE={value}"
            );
        }
    }

    // Every mode builds a usable transport; TLS setup must not error offline.
    #[test]
    fn every_mode_builds_a_transport() {
        for mode in [SmtpTlsMode::Starttls, SmtpTlsMode::Tls, SmtpTlsMode::None] {
            let mail = MailConfig {
                smtp_tls_mode: mode,
                smtp_port: Some(2525),
                smtp_username: Some("user".into()),
                smtp_password: Some("secret".into()),
                ..configured()
            };
            assert!(
                smtp_transport(&mail).is_ok(),
                "mode {mode:?} failed to build"
            );
        }
    }
}
