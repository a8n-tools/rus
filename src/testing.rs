//! Shared test utilities - compiled only when running `cargo test`.

use crate::config::Config;
use crate::db::AppState;
use actix_web::web;

/// JWT secret used in standalone tests (must be >32 chars for HS256).
#[cfg(feature = "standalone")]
pub const TEST_JWT_SECRET: &str = "test-secret-at-least-32-chars-ok!";

/// Webhook HMAC secret used in SaaS tests.
#[cfg(feature = "saas")]
pub const TEST_WEBHOOK_SECRET: &str = "test-webhook-secret-32-chars-pad!";

/// Standard test password meeting all complexity requirements.
#[cfg(feature = "standalone")]
pub const TEST_PASSWORD: &str = "TestPass1!";

/// Create a Config suitable for testing (in-memory SQLite).
pub fn test_config() -> Config {
    Config {
        max_url_length: 2048,
        click_retention_days: 30,
        host_url: "http://localhost:4001".to_string(),
        db_path: ":memory:".to_string(),
        host: "127.0.0.1".to_string(),
        port: 4001,
        mail: crate::config::MailConfig::default(),
        trusted_proxy_cidrs: Vec::new(),
        #[cfg(feature = "standalone")]
        jwt_secret: TEST_JWT_SECRET.to_string(),
        #[cfg(feature = "standalone")]
        jwt_expiry_hours: 1,
        #[cfg(feature = "standalone")]
        refresh_token_expiry_days: 7,
        #[cfg(feature = "standalone")]
        account_lockout_attempts: 5,
        #[cfg(feature = "standalone")]
        account_lockout_duration_minutes: 30,
        #[cfg(feature = "standalone")]
        allow_registration: true,
        #[cfg(feature = "saas")]
        webhook_secret: TEST_WEBHOOK_SECRET.to_string(),
        #[cfg(feature = "saas")]
        oidc: crate::config::OidcConfig {
            issuer: String::new(),
            audience: "http://localhost:4001/api".to_string(),
            jwks_url: String::new(),
            jwks_cache_ttl: 300,
            client_id: "test-client".to_string(),
            client_secret: "test-secret".to_string(),
            redirect_uri: "http://localhost:4001/oauth2/callback".to_string(),
            post_logout_redirect_uri: "http://localhost:4001/".to_string(),
            leeway_seconds: 30,
            lifecycle_jti_cache_ttl: 300,
            session_ttl_seconds: 1_209_600,
        },
    }
}

/// Create an AppState backed by an in-memory SQLite database.
pub fn make_test_state() -> web::Data<AppState> {
    web::Data::new(AppState::new(test_config()).expect("Failed to create test AppState"))
}

/// Insert a user directly into the DB (bypasses password hashing).
/// Returns the new user_id.
#[cfg(feature = "standalone")]
pub fn insert_test_user(state: &web::Data<AppState>, username: &str, is_admin: bool) -> i64 {
    let db = state.db.lock().unwrap();
    db.execute(
        "INSERT INTO users (username, password, is_admin) VALUES (?1, 'placeholder', ?2)",
        rusqlite::params![username, is_admin as i32],
    )
    .expect("insert_test_user failed");
    db.last_insert_rowid()
}

/// Create a JWT token for use in standalone tests.
#[cfg(feature = "standalone")]
pub fn make_test_token(username: &str, user_id: i64, is_admin: bool) -> String {
    crate::auth::jwt::create_jwt(username, user_id, is_admin, TEST_JWT_SECRET, 1)
        .expect("make_test_token failed")
}

/// Insert a URL directly for standalone tests. Returns the new row id.
#[cfg(feature = "standalone")]
pub fn insert_test_url(
    state: &web::Data<AppState>,
    user_id: i64,
    original_url: &str,
    short_code: &str,
) -> i64 {
    let db = state.db.lock().unwrap();
    db.execute(
        "INSERT INTO urls (user_id, original_url, short_code) VALUES (?1, ?2, ?3)",
        rusqlite::params![user_id, original_url, short_code],
    )
    .expect("insert_test_url failed");
    db.last_insert_rowid()
}

/// Insert a SaaS user directly into the local DB. Returns the new user_id.
#[cfg(feature = "saas")]
pub fn insert_saas_user(
    state: &web::Data<AppState>,
    username: &str,
    saas_user_id: &str,
    is_admin: bool,
) -> i64 {
    let db = state.db.lock().unwrap();
    db.execute(
        "INSERT INTO users (username, password, is_admin, saas_user_id, email)
         VALUES (?1, '!sso:no-password', ?2, ?3, ?4)",
        rusqlite::params![
            username,
            is_admin as i32,
            saas_user_id,
            format!("{username}@example.com")
        ],
    )
    .expect("insert_saas_user failed");
    db.last_insert_rowid()
}

/// Create a BFF session for the given user and return the raw cookie value.
#[cfg(feature = "saas")]
pub fn make_saas_session(state: &web::Data<AppState>, user_id: i64) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    use chrono::Utc;
    use rand::RngCore;
    use sha2::{Digest, Sha256};

    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    let token = URL_SAFE_NO_PAD.encode(buf);
    let token_hash = Sha256::digest(token.as_bytes()).to_vec();

    let db = state.db.lock().unwrap();
    let session_version: i32 = db
        .query_row(
            "SELECT session_version FROM users WHERE userID = ?1",
            rusqlite::params![user_id],
            |r| r.get(0),
        )
        .expect("user not found");

    let now = Utc::now();
    let expires = now + chrono::Duration::hours(1);
    db.execute(
        "INSERT INTO user_sessions (id, session_token_hash, user_id, session_version, auth_via_oidc, created_at, expires_at)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6)",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            token_hash,
            user_id,
            session_version,
            now.to_rfc3339(),
            expires.to_rfc3339()
        ],
    )
    .expect("insert user_sessions failed");

    token
}

/// Compute HMAC-SHA256 signature for a webhook payload body.
#[cfg(feature = "saas")]
pub fn sign_webhook_payload(body: &[u8], secret: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC key error");
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

/// Build an `IdTokenClaims` fixture for JIT tests.
#[cfg(feature = "saas")]
pub fn id_claims(
    sub: &str,
    email: Option<&str>,
    email_verified: bool,
    has_member_access: bool,
    role: Option<&str>,
) -> crate::oidc::verifier::IdTokenClaims {
    crate::oidc::verifier::IdTokenClaims {
        iss: "https://idp.example.com".to_string(),
        sub: sub.to_string(),
        aud: serde_json::json!("rus-test-client"),
        exp: 0,
        iat: 0,
        nonce: Some("test-nonce".to_string()),
        email: email.map(String::from),
        email_verified: Some(email_verified),
        name: None,
        role: role.map(String::from),
        has_member_access: Some(has_member_access),
    }
}

/// Insert a URL directly for SaaS tests. Returns the new row id.
#[cfg(feature = "saas")]
pub fn insert_saas_url(
    state: &web::Data<AppState>,
    user_id: i64,
    original_url: &str,
    short_code: &str,
) -> i64 {
    let db = state.db.lock().unwrap();
    db.execute(
        "INSERT INTO urls (user_id, original_url, short_code) VALUES (?1, ?2, ?3)",
        rusqlite::params![user_id, original_url, short_code],
    )
    .expect("insert_saas_url failed");
    db.last_insert_rowid()
}

// ── RUS-22: a stubbed OP and a mail sink for the saas callback ───────────────

/// Throwaway Ed25519 seed the stub OP signs ID tokens with, plus the public
/// half its JWKS publishes. Generated once for this suite and used nowhere
/// else: the point of the stub is that the app fetches this key and checks the
/// signature against it, so an accepted token is one that really verified.
#[cfg(feature = "saas")]
const STUB_OP_SEED: &str = "yqRLDtZtJH7Z-PKz_obMU9T4CWkZF1lTSKAOpKwK0a8";
#[cfg(feature = "saas")]
const STUB_OP_PUBLIC_X: &str = "nvCDQ2VIdxSWs7aObE-XBgEIBve6Jspp0V31DrRxVo8";

/// A second seed, never published in the stub's JWKS. Signing with it under the
/// published `kid` is how the suite proves the signature check is on: the
/// verifier finds a key for the `kid` and has to reject the signature itself.
#[cfg(feature = "saas")]
const STUB_OP_FOREIGN_SEED: &str = "5HhyHEkA0qAwznobcHiNRsGVEsXLVKEeNhJ1JsTEIYA";

/// The `kid` both seeds sign under, and the only one in the stub's JWKS.
#[cfg(feature = "saas")]
pub const STUB_OP_KID: &str = "stub-op-ed25519";

/// PKCS#8 v1 DER for an Ed25519 seed: a fixed 16-byte header, then the seed.
/// jsonwebtoken hands the DER straight to ring, which accepts the v1 template.
#[cfg(feature = "saas")]
fn ed25519_pkcs8_der(seed_b64url: &str) -> Vec<u8> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    let seed = URL_SAFE_NO_PAD
        .decode(seed_b64url)
        .expect("stub OP seed is base64url");
    let mut der = vec![
        0x30, 0x2E, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2B, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20,
    ];
    der.extend_from_slice(&seed);
    der
}

/// Sign `claims` as an ID token under [`STUB_OP_KID`]. EdDSA because that is
/// what `OidcVerifier` validates with, and `typ: JWT` because it checks that.
#[cfg(feature = "saas")]
fn sign_stub_id_token(claims: &serde_json::Value, seed: &str) -> String {
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::EdDSA);
    header.kid = Some(STUB_OP_KID.to_string());
    jsonwebtoken::encode(
        &header,
        claims,
        &jsonwebtoken::EncodingKey::from_ed_der(&ed25519_pkcs8_der(seed)),
    )
    .expect("stub ID token signs")
}

/// A stubbed OIDC provider on a loopback ephemeral port (RUS-22).
///
/// Serves exactly what `oidc::rp::callback` calls: the token endpoint it POSTs
/// the authorization code to, and the JWKS the verifier fetches the signing key
/// from. No discovery document, because nothing in the callback reads one: the
/// token URL is built from the issuer and the JWKS URL comes from config.
#[cfg(feature = "saas")]
pub struct StubOp {
    /// `OIDC_ISSUER` for this stub. The token endpoint hangs off it.
    pub issuer: String,
    /// `OIDC_JWKS_URL` for this stub.
    pub jwks_url: String,
    id_token: std::sync::Arc<std::sync::Mutex<String>>,
    token_forms: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    server: tokio::task::JoinHandle<()>,
}

#[cfg(feature = "saas")]
impl StubOp {
    /// Bind the stub and start serving. Call from inside an actix runtime.
    pub async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("stub OP binds a loopback port");
        let addr = listener.local_addr().expect("stub OP has an address");
        let id_token = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let token_forms = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let served = id_token.clone();
        let seen = token_forms.clone();
        // One connection at a time: the callback's two requests are sequential
        // and every response closes, so nothing queues behind an idle socket.
        let server = actix_web::rt::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                serve_stub_op(stream, &served, &seen).await;
            }
        });

        Self {
            issuer: format!("http://{addr}"),
            jwks_url: format!("http://{addr}/jwks"),
            id_token,
            token_forms,
            server,
        }
    }

    /// The app's OIDC configuration pointed at this stub. Everything else comes
    /// from [`test_config`], so the fixture cannot drift from the rest.
    pub fn oidc_config(&self) -> crate::config::OidcConfig {
        crate::config::OidcConfig {
            issuer: self.issuer.clone(),
            jwks_url: self.jwks_url.clone(),
            ..test_config().oidc
        }
    }

    /// The claims a signed-in user arrives with. `email: None` omits the claim
    /// entirely, which is the RUS-11 case where the OP sends no address.
    pub fn claims(&self, sub: &str, nonce: &str, email: Option<&str>) -> serde_json::Value {
        let now = chrono::Utc::now().timestamp();
        let mut claims = serde_json::json!({
            "iss": self.issuer,
            "sub": sub,
            "aud": test_config().oidc.client_id,
            "iat": now,
            "exp": now + 300,
            "nonce": nonce,
            "email_verified": true,
            "has_member_access": true,
            "role": "subscriber",
        });
        if let Some(address) = email {
            claims["email"] = serde_json::json!(address);
        }
        claims
    }

    /// Serve `claims` from the token endpoint, signed with the published key.
    pub fn issue_id_token(&self, claims: &serde_json::Value) {
        *self.id_token.lock().unwrap() = sign_stub_id_token(claims, STUB_OP_SEED);
    }

    /// Serve `claims` signed with a key that is NOT in the JWKS, under the
    /// published `kid`. A callback that accepts this is not checking signatures.
    pub fn issue_forged_id_token(&self, claims: &serde_json::Value) {
        *self.id_token.lock().unwrap() = sign_stub_id_token(claims, STUB_OP_FOREIGN_SEED);
    }

    /// The form bodies the token endpoint received, oldest first.
    pub fn token_requests(&self) -> Vec<String> {
        self.token_forms.lock().unwrap().clone()
    }
}

#[cfg(feature = "saas")]
impl Drop for StubOp {
    fn drop(&mut self) {
        self.server.abort();
    }
}

/// Answer one HTTP request: the token endpoint, the JWKS, or 404.
#[cfg(feature = "saas")]
async fn serve_stub_op(
    mut stream: tokio::net::TcpStream,
    id_token: &std::sync::Mutex<String>,
    token_forms: &std::sync::Mutex<Vec<String>>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut raw: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 1024];
    let head_end = loop {
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(n) => raw.extend_from_slice(&chunk[..n]),
        }
        if let Some(at) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
    };

    let head = String::from_utf8_lossy(&raw[..head_end]).to_string();
    let want: usize = head
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse().ok())
        .unwrap_or(0);
    while raw.len() < head_end + want {
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => raw.extend_from_slice(&chunk[..n]),
        }
    }

    let path = head.split_whitespace().nth(1).unwrap_or("/").to_string();
    let body = if path.starts_with("/oauth2/token") {
        let form = String::from_utf8_lossy(&raw[head_end..]).to_string();
        token_forms.lock().unwrap().push(form);
        let token = id_token.lock().unwrap().clone();
        serde_json::json!({
            "access_token": "stub-access-token",
            "token_type": "Bearer",
            "expires_in": 300,
            "id_token": token,
        })
        .to_string()
    } else if path.starts_with("/jwks") {
        serde_json::json!({
            "keys": [{
                "kty": "OKP",
                "crv": "Ed25519",
                "use": "sig",
                "alg": "EdDSA",
                "kid": STUB_OP_KID,
                "x": STUB_OP_PUBLIC_X,
            }]
        })
        .to_string()
    } else {
        let _ = stream
            .write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
            .await;
        return;
    };

    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

/// A loopback SMTP sink that captures whole messages (RUS-22).
///
/// Just enough dialogue for lettre to deliver: every verb gets a canned reply
/// and DATA keeps the message, so a test can read the approval link the app
/// actually mailed instead of seeding a token it made up itself.
#[cfg(feature = "saas")]
pub struct StubSmtp {
    /// The ephemeral port to point `SMTP_PORT` at.
    pub port: u16,
    messages: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    server: tokio::task::JoinHandle<()>,
}

#[cfg(feature = "saas")]
impl StubSmtp {
    /// Bind the sink and start serving. Call from inside an actix runtime.
    pub async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("stub SMTP binds a loopback port");
        let port = listener
            .local_addr()
            .expect("stub SMTP has an address")
            .port();
        let messages = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let captured = messages.clone();
        let server = actix_web::rt::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                serve_stub_smtp(stream, &captured).await;
            }
        });

        Self {
            port,
            messages,
            server,
        }
    }

    /// Mail configuration that delivers to this sink, with the RUS-19 gate on.
    /// Plaintext because the sink speaks no TLS, which is what `SMTP_TLS_MODE=none`
    /// exists for. The RUS-7 alert is off so the capture holds approval mail only.
    pub fn mail_config(&self) -> crate::config::MailConfig {
        crate::config::MailConfig {
            smtp_host: Some("127.0.0.1".to_string()),
            smtp_port: Some(self.port),
            smtp_from_email: Some("no-reply@example.com".to_string()),
            smtp_tls_mode: crate::config::SmtpTlsMode::None,
            login_location_alerts_enabled: false,
            login_approval_enabled: true,
            ..crate::config::MailConfig::default()
        }
    }

    /// Every message delivered so far, oldest first, decoded so a URL in the
    /// body reads the way the app wrote it.
    pub fn messages(&self) -> Vec<String> {
        self.messages
            .lock()
            .unwrap()
            .iter()
            .map(|message| decode_quoted_printable(message))
            .collect()
    }
}

/// Undo quoted-printable when the message says it used it: soft line breaks
/// first, then the `=XX` escapes, since lettre writes `?token=` as `?token=3D`.
/// Bodies here are ASCII, so a decoded byte is a char.
#[cfg(feature = "saas")]
fn decode_quoted_printable(message: &str) -> String {
    if !message
        .to_ascii_lowercase()
        .contains("content-transfer-encoding: quoted-printable")
    {
        return message.to_string();
    }
    let joined = message.replace("=\r\n", "").replace("=\n", "");
    let mut decoded = String::with_capacity(joined.len());
    let mut chars = joined.chars();
    while let Some(c) = chars.next() {
        if c != '=' {
            decoded.push(c);
            continue;
        }
        let escape: String = chars.by_ref().take(2).collect();
        match u8::from_str_radix(&escape, 16) {
            Ok(byte) => decoded.push(byte as char),
            Err(_) => {
                decoded.push('=');
                decoded.push_str(&escape);
            }
        }
    }
    decoded
}

#[cfg(feature = "saas")]
impl Drop for StubSmtp {
    fn drop(&mut self) {
        self.server.abort();
    }
}

/// Speak one SMTP session, keeping whatever arrives between DATA and its
/// terminating dot.
#[cfg(feature = "saas")]
async fn serve_stub_smtp(stream: tokio::net::TcpStream, messages: &std::sync::Mutex<Vec<String>>) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    if writer.write_all(b"220 stub-smtp ESMTP\r\n").await.is_err() {
        return;
    }

    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let verb = line
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();

        if verb == "QUIT" {
            let _ = writer.write_all(b"221 Bye\r\n").await;
            return;
        }
        if verb == "DATA" {
            if writer.write_all(b"354 End data\r\n").await.is_err() {
                return;
            }
            let mut message = String::new();
            loop {
                let mut data = String::new();
                match reader.read_line(&mut data).await {
                    Ok(0) | Err(_) => return,
                    Ok(_) => {}
                }
                if data.trim_end() == "." {
                    break;
                }
                message.push_str(&data);
            }
            messages.lock().unwrap().push(message);
            if writer.write_all(b"250 Ok: queued\r\n").await.is_err() {
                return;
            }
            continue;
        }
        // EHLO, HELO, MAIL, RCPT, RSET, NOOP: nothing here needs to say no.
        if writer.write_all(b"250 Ok\r\n").await.is_err() {
            return;
        }
    }
}
