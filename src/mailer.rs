//! Outbound email for security notifications (RUS-7).
//!
//! Minimal on purpose: one message type, built with `format!` because this
//! crate has no template engine. RUS-11 routes the alert to the account owner's
//! own address when it has one, and only falls back to the shared operator
//! mailbox when it does not, so the person whose credentials may be compromised
//! is the one who hears about it. Sending is gated on `MailConfig::smtp_ready`
//! plus a resolvable recipient, so an unconfigured deployment logs the message
//! instead of sending it and a login never depends on mail working. The
//! connection is encrypted unless `SMTP_TLS_MODE=none` explicitly opts out
//! (RUS-16).

use chrono::Utc;
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::Address;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::config::{MailConfig, SmtpTlsMode};

/// Plain-text body sent to the account owner: a personal security notice.
const NEW_SIGNIN_LOCATION_OWNER_BODY: &str = "\
New sign-in to your RUS account from a new country

Your RUS account \"{username}\" was signed in to from a country you have not signed in from before.

Country: {country}
When: {timestamp}
IP address: {ip_address}
Device: {device}

If this was you, no action is needed.

If this was not you, someone else may have access to your account. Change your password now and review your active sessions.
";

/// Plain-text body sent to the operator mailbox when the account has no address
/// of its own. It must name which account signed in, since the reader is not
/// the owner.
const NEW_SIGNIN_LOCATION_OPERATOR_BODY: &str = "\
New sign-in to a RUS account from a new country

A sign-in to the RUS account \"{username}\" was detected from a country that account has not signed in from before.

This account has no email address of its own, so this notice went to the operator mailbox instead of its owner.

Account: {username}
Country: {country}
When: {timestamp}
IP address: {ip_address}
Device: {device}

If this was the account owner, no action is needed.

If this sign-in is not recognised, someone else may have access to the account. Reset that account's password now and review its active sessions.
";

/// Plain-text body asking the account owner to release a held sign-in.
const LOGIN_APPROVAL_OWNER_BODY: &str = "\
Approve the sign-in to your RUS account from {country}

A sign-in to your RUS account \"{username}\" was attempted from a country you have not signed in from before. It is being held and no session was created.

Country: {country}
When: {timestamp}
IP address: {ip_address}
Device: {device}

If this was you, open this link within {expiry_minutes} minutes to finish signing in:

{approval_url}

The link works once and then stops working.

If this was not you, do nothing. The attempt expires on its own and no session is created. Change your password, because whoever tried it already had your credentials.
";

/// Plain-text body sent to the operator mailbox when the held account has no
/// address of its own. Whoever reads it is approving on the owner's behalf, so
/// it names the account and says so.
const LOGIN_APPROVAL_OPERATOR_BODY: &str = "\
Approve the sign-in to RUS account {username} from {country}

A sign-in to the RUS account \"{username}\" was attempted from a country that account has not signed in from before. It is being held and no session was created.

This account has no email address of its own, so this request went to the operator mailbox instead of its owner.

Account: {username}
Country: {country}
When: {timestamp}
IP address: {ip_address}
Device: {device}

Open this link within {expiry_minutes} minutes to release the sign-in, but only after confirming with the account owner that it was them:

{approval_url}

The link works once and then stops working. Opening it signs that browser in as {username}.

If the sign-in is not recognised, do nothing. It expires on its own.
";

/// Who a new-location alert is addressed to. The variant also picks the
/// wording: the owner gets a personal notice, the operator gets one naming
/// which account signed in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AlertRecipient {
    /// The account's own address (`users.email`).
    Owner(String),
    /// The shared `SECURITY_ALERT_EMAIL` mailbox.
    Operator(String),
}

impl AlertRecipient {
    pub fn address(&self) -> &str {
        match self {
            Self::Owner(address) | Self::Operator(address) => address,
        }
    }

    /// Label for logs, so an operator can see which route a login took.
    fn route(&self) -> &'static str {
        match self {
            Self::Owner(_) => "account_owner",
            Self::Operator(_) => "operator",
        }
    }
}

/// Normalize an account email for storage and delivery: trimmed, lowercased,
/// with blank meaning "not set" (stored as NULL) rather than an empty string.
///
/// Lives here rather than in `security.rs` because that module is standalone
/// only, while both feature legs read this column to route an alert.
pub fn normalize_account_email(raw: &str) -> Result<Option<String>, String> {
    let value = raw.trim().to_ascii_lowercase();
    if value.is_empty() {
        return Ok(None);
    }
    // The parse is part of the check, not just the shape: an address that
    // lettre cannot build is one the alert could never be delivered to.
    let valid = value
        .split_once('@')
        .is_some_and(|(local, domain)| !local.is_empty() && !domain.is_empty())
        && !value.contains(char::is_whitespace)
        && value.parse::<Address>().is_ok();
    if !valid {
        return Err("Email address must look like name@example.com".to_string());
    }
    Ok(Some(value))
}

/// A stored address, ignored when blank or malformed. SSO can write an empty
/// string when the OP omits the claim on a repeat login, so a stored value is
/// re-checked rather than trusted.
fn usable_address(raw: &str) -> Option<String> {
    normalize_account_email(raw).ok().flatten()
}

/// Resolve who a new-location alert goes to (RUS-11 precedence): the account's
/// own address when set, otherwise the operator mailbox, otherwise nobody.
pub fn resolve_recipient(mail: &MailConfig, account_email: Option<&str>) -> Option<AlertRecipient> {
    if let Some(address) = account_email.and_then(usable_address) {
        return Some(AlertRecipient::Owner(address));
    }
    mail.security_alert_email
        .as_deref()
        .and_then(usable_address)
        .map(AlertRecipient::Operator)
}

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

/// Render the alert body for whoever it is addressed to. Split out so the
/// wording and interpolation are unit-testable without a mail server.
fn new_signin_location_body(
    recipient: &AlertRecipient,
    username: &str,
    country: &str,
    ip: &str,
    device: Option<&str>,
    timestamp: &str,
) -> String {
    let template = match recipient {
        AlertRecipient::Owner(_) => NEW_SIGNIN_LOCATION_OWNER_BODY,
        AlertRecipient::Operator(_) => NEW_SIGNIN_LOCATION_OPERATOR_BODY,
    };
    template
        .replace("{username}", username)
        .replace("{country}", country)
        .replace("{timestamp}", timestamp)
        .replace("{ip_address}", ip)
        .replace("{device}", device.unwrap_or("unknown"))
}

/// Subject line, phrased for whoever it is addressed to.
fn new_signin_location_subject(
    recipient: &AlertRecipient,
    username: &str,
    country: &str,
) -> String {
    match recipient {
        AlertRecipient::Owner(_) => format!("New sign-in to your RUS account from {country}"),
        AlertRecipient::Operator(_) => {
            format!("New sign-in to RUS account {username} from {country}")
        }
    }
}

/// Email `recipient` that an account was signed in to from a new country.
///
/// Returns `Ok` without sending when SMTP is unconfigured (log mode), so the
/// caller cannot fail a login on missing mail configuration.
pub async fn send_new_signin_location_alert(
    mail: &MailConfig,
    recipient: &AlertRecipient,
    username: &str,
    country: &str,
    ip: &str,
    device: Option<&str>,
) -> Result<(), String> {
    let subject = new_signin_location_subject(recipient, username, country);
    let body = new_signin_location_body(
        recipient,
        username,
        country,
        ip,
        device,
        &Utc::now().to_rfc3339(),
    );

    if !mail.smtp_ready() {
        log_undelivered_alert(username, &subject, &body);
        return Ok(());
    }

    let from_email = mail
        .smtp_from_email
        .clone()
        .ok_or_else(|| "SMTP sender email is missing".to_string())?;

    let from_mailbox = Mailbox::new(
        mail.smtp_from_name.clone(),
        from_email
            .parse()
            .map_err(|error| format!("Invalid SMTP from address: {error}"))?,
    );
    let to_mailbox = recipient
        .address()
        .parse()
        .map_err(|error| format!("Invalid alert recipient address: {error}"))?;
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
                route = recipient.route(),
                username = %username,
                "New sign-in location alert delivered"
            );
            Ok(())
        }
        Err(error) => {
            tracing::error!(
                delivery_mode = "smtp",
                delivered = false,
                route = recipient.route(),
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

/// Everything the approval request needs to render and address itself. A struct
/// rather than a parameter list because the two are always filled in together.
pub struct LoginApprovalMail<'a> {
    pub recipient: &'a AlertRecipient,
    pub username: &'a str,
    pub country: &'a str,
    pub ip: &'a str,
    pub device: Option<&'a str>,
    pub approval_url: &'a str,
    pub expiry_minutes: i64,
}

/// Render the approval-request body for whoever it is addressed to. Split out
/// so the wording and interpolation are unit-testable without a mail server.
fn login_approval_body(request: &LoginApprovalMail<'_>, timestamp: &str) -> String {
    let template = match request.recipient {
        AlertRecipient::Owner(_) => LOGIN_APPROVAL_OWNER_BODY,
        AlertRecipient::Operator(_) => LOGIN_APPROVAL_OPERATOR_BODY,
    };
    template
        .replace("{username}", request.username)
        .replace("{country}", request.country)
        .replace("{timestamp}", timestamp)
        .replace("{ip_address}", request.ip)
        .replace("{device}", request.device.unwrap_or("unknown"))
        .replace("{approval_url}", request.approval_url)
        .replace("{expiry_minutes}", &request.expiry_minutes.to_string())
}

/// Subject line for a held sign-in, phrased for whoever it is addressed to.
fn login_approval_subject(request: &LoginApprovalMail<'_>) -> String {
    match request.recipient {
        AlertRecipient::Owner(_) => format!(
            "Approve the sign-in to your RUS account from {}",
            request.country
        ),
        AlertRecipient::Operator(_) => format!(
            "Approve the sign-in to RUS account {} from {}",
            request.username, request.country
        ),
    }
}

/// Email `recipient` a single-use link that releases a held sign-in (RUS-19).
///
/// Unlike the after-the-fact alert this one is load-bearing: the sign-in stays
/// held until the link is opened, so an `Err` here must fail the sign-in rather
/// than let it through ungated. The caller checks `smtp_ready` before holding
/// at all, so reaching this with no transport is a bug, not a configuration.
pub async fn send_login_approval_request(
    mail: &MailConfig,
    request: LoginApprovalMail<'_>,
) -> Result<(), String> {
    let subject = login_approval_subject(&request);
    let body = login_approval_body(&request, &Utc::now().to_rfc3339());

    if !mail.smtp_ready() {
        return Err("SMTP is not configured, so no approval link could be sent".to_string());
    }

    let from_email = mail
        .smtp_from_email
        .clone()
        .ok_or_else(|| "SMTP sender email is missing".to_string())?;

    let from_mailbox = Mailbox::new(
        mail.smtp_from_name.clone(),
        from_email
            .parse()
            .map_err(|error| format!("Invalid SMTP from address: {error}"))?,
    );
    let to_mailbox = request
        .recipient
        .address()
        .parse()
        .map_err(|error| format!("Invalid approval recipient address: {error}"))?;
    let message = Message::builder()
        .from(from_mailbox)
        .to(Mailbox::new(None, to_mailbox))
        .subject(subject)
        .body(body)
        .map_err(|error| format!("Sign-in approval request build failed: {error}"))?;

    match smtp_transport(mail)?.send(message).await {
        Ok(_) => {
            tracing::info!(
                delivery_mode = "smtp",
                delivered = true,
                route = request.recipient.route(),
                username = %request.username,
                "Sign-in approval request delivered"
            );
            Ok(())
        }
        Err(error) => {
            tracing::error!(
                delivery_mode = "smtp",
                delivered = false,
                route = request.recipient.route(),
                username = %request.username,
                error = %error,
                "Sign-in approval request delivery failed"
            );
            Err(format!("Sign-in approval request delivery failed: {error}"))
        }
    }
}

/// Log an alert that cannot be sent, so the signal is never silently dropped.
/// Used for both an unconfigured SMTP transport and an account with no
/// recipient to route to (RUS-11).
pub fn log_undelivered_alert(username: &str, subject: &str, body: &str) {
    tracing::warn!(
        delivery_mode = "log",
        username = %username,
        subject = %subject,
        "New sign-in location alert NOT sent: delivery in log mode. Set SMTP_HOST and SMTP_FROM_EMAIL, and give the account an email address or set SECURITY_ALERT_EMAIL, to enable SMTP."
    );
    tracing::info!(
        delivery_mode = "log",
        username = %username,
        body = %body,
        "New sign-in location alert body (log mode)"
    );
}

/// Render the log-mode text for an alert with no recipient at all, so the
/// would-be notice still reaches the operator's logs (RUS-11).
pub fn undeliverable_alert_text(
    username: &str,
    country: &str,
    ip: &str,
    device: Option<&str>,
) -> (String, String) {
    // No recipient resolved, so the operator wording is the honest one: whoever
    // reads the log is not the account owner.
    let recipient = AlertRecipient::Operator(String::new());
    (
        new_signin_location_subject(&recipient, username, country),
        new_signin_location_body(
            &recipient,
            username,
            country,
            ip,
            device,
            &Utc::now().to_rfc3339(),
        ),
    )
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

    fn owner() -> AlertRecipient {
        AlertRecipient::Owner("alice@example.com".into())
    }

    fn operator() -> AlertRecipient {
        AlertRecipient::Operator("operator@example.com".into())
    }

    // Log mode is the unconfigured default: no SMTP host or sender means the
    // alert is logged, never sent, and never errors a login.
    #[actix_web::test]
    async fn unconfigured_mail_logs_instead_of_sending() {
        let mail = MailConfig::default();
        assert!(!mail.smtp_ready());
        let result =
            send_new_signin_location_alert(&mail, &owner(), "alice", "DE", "203.0.113.7", None)
                .await;
        assert!(result.is_ok());
    }

    // The transport needs a host and a sender; the recipient is resolved
    // separately, so an operator mailbox alone is not enough (RUS-11).
    #[test]
    fn smtp_ready_requires_host_and_sender() {
        let mut mail = MailConfig {
            smtp_host: Some("smtp.example.com".into()),
            ..MailConfig::default()
        };
        assert!(!mail.smtp_ready());
        mail.smtp_from_email = Some("alerts@example.com".into());
        assert!(mail.smtp_ready(), "no operator mailbox is still sendable");

        let operator_only = MailConfig {
            security_alert_email: Some("operator@example.com".into()),
            ..MailConfig::default()
        };
        assert!(!operator_only.smtp_ready());
    }

    #[test]
    fn configured_sample_is_smtp_ready() {
        assert!(configured().smtp_ready());
    }

    // RUS-11 precedence: the account's own address wins over the operator
    // mailbox.
    #[test]
    fn account_address_routes_to_the_owner() {
        assert_eq!(
            resolve_recipient(&configured(), Some("Alice@Example.com ")),
            Some(AlertRecipient::Owner("alice@example.com".into())),
            "a set address wins, normalized"
        );
    }

    // An account with no address of its own falls back to the operator mailbox.
    #[test]
    fn missing_account_address_falls_back_to_the_operator() {
        assert_eq!(
            resolve_recipient(&configured(), None),
            Some(operator()),
            "unset falls back"
        );
        // SSO writes an empty string when the OP omits the claim on a repeat
        // login, and a legacy row can hold junk; neither counts as an address.
        for stored in ["", "   ", "not-an-address"] {
            assert_eq!(
                resolve_recipient(&configured(), Some(stored)),
                Some(operator()),
                "expected {stored:?} to fall back"
            );
        }
    }

    // Neither an account address nor an operator mailbox means no recipient at
    // all, so the alert is logged rather than sent.
    #[test]
    fn no_address_anywhere_resolves_to_no_recipient() {
        let no_operator = MailConfig {
            security_alert_email: None,
            ..configured()
        };
        assert_eq!(resolve_recipient(&no_operator, None), None);
        assert_eq!(resolve_recipient(&no_operator, Some("  ")), None);
        // A malformed operator mailbox is no better than an unset one.
        let bad_operator = MailConfig {
            security_alert_email: Some("nonsense".into()),
            ..configured()
        };
        assert_eq!(resolve_recipient(&bad_operator, None), None);
    }

    // The owner's notice is written in the second person and never says the
    // alert went somewhere else.
    #[test]
    fn owner_body_reads_as_a_personal_notice() {
        let body = new_signin_location_body(
            &owner(),
            "alice",
            "DE",
            "203.0.113.7",
            Some("Firefox"),
            "2026-08-21T00:00:00Z",
        );
        assert!(body.contains("Your RUS account \"alice\""));
        assert!(body.contains("If this was you, no action is needed."));
        assert!(!body.contains("operator mailbox"));
        assert!(!body.contains('{'));
        assert_eq!(
            new_signin_location_subject(&owner(), "alice", "DE"),
            "New sign-in to your RUS account from DE"
        );
    }

    // The operator copy must still name which account signed in, since its
    // reader is not the owner.
    #[test]
    fn operator_body_names_the_account() {
        let body = new_signin_location_body(
            &operator(),
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
        assert_eq!(
            new_signin_location_subject(&operator(), "alice", "DE"),
            "New sign-in to RUS account alice from DE"
        );
    }

    // The log-mode text for an account with no recipient still names it, since
    // whoever reads the log is not the owner.
    #[test]
    fn undeliverable_text_names_the_account() {
        let (subject, body) = undeliverable_alert_text("alice", "DE", "203.0.113.7", None);
        assert_eq!(subject, "New sign-in to RUS account alice from DE");
        assert!(body.contains("Account: alice"));
        assert!(!body.contains('{'));
    }

    // An absent User-Agent renders as a placeholder rather than an empty field.
    #[test]
    fn body_without_device_says_unknown() {
        for recipient in [owner(), operator()] {
            let body = new_signin_location_body(
                &recipient,
                "alice",
                "DE",
                "203.0.113.7",
                None,
                "2026-08-21T00:00:00Z",
            );
            assert!(body.contains("Device: unknown"), "for {recipient:?}");
        }
    }

    // Validation is deliberately minimal: trim, lowercase, require an @ with
    // something either side.
    #[test]
    fn normalize_accepts_a_good_address() {
        assert_eq!(
            normalize_account_email("  Alice@Example.COM "),
            Ok(Some("alice@example.com".to_string()))
        );
    }

    // Blank is "not set", stored as NULL, not an error.
    #[test]
    fn normalize_treats_blank_as_unset() {
        for blank in ["", "   ", "\t\n"] {
            assert_eq!(normalize_account_email(blank), Ok(None), "for {blank:?}");
        }
    }

    // Anything without a usable @ is rejected rather than stored.
    #[test]
    fn normalize_rejects_malformed_addresses() {
        for bad in ["alice", "@example.com", "alice@", "@", "alice example.com"] {
            assert!(
                normalize_account_email(bad).is_err(),
                "expected {bad:?} to be rejected"
            );
        }
    }

    // Whatever is stored must be addressable, or the alert would fail at send
    // time instead of at the input that set it.
    #[test]
    fn normalize_rejects_what_the_mailer_cannot_address() {
        for bad in ["alice@@example.com", "ali:ce@example.com", "alice@exa,mple"] {
            assert!(
                normalize_account_email(bad).is_err(),
                "expected {bad:?} to be rejected"
            );
        }
        // Anything accepted round-trips into a mailbox the builder will take.
        let stored = normalize_account_email("Alice@Example.com")
            .unwrap()
            .unwrap();
        assert!(stored.parse::<Address>().is_ok());
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
