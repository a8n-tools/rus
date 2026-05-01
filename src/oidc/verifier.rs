//! OIDC Resource Server - validates `at+jwt` access tokens, ID tokens,
//! back-channel logout tokens, and lifecycle event tokens issued by the
//! SaaS OIDC provider.
//!
//! Keys are fetched from the IdP's JWKS endpoint on first use and cached in
//! memory for `OidcConfig::jwks_cache_ttl` seconds.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::Utc;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::OidcConfig;

#[derive(Debug)]
pub enum OidcError {
    /// Token is structurally invalid or fails an explicit non-signature check.
    InvalidToken(String),
    /// Token rejected by signature/expiry validation - caller may retry once
    /// after a JWKS refresh in case the signing key has rotated.
    Rejected,
    /// JWKS fetch or parse failed.
    Jwks(String),
    /// OIDC is misconfigured.
    Configuration(String),
}

impl std::fmt::Display for OidcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OidcError::InvalidToken(m) => write!(f, "token invalid: {m}"),
            OidcError::Rejected => write!(f, "token rejected"),
            OidcError::Jwks(m) => write!(f, "JWKS error: {m}"),
            OidcError::Configuration(m) => write!(f, "OIDC misconfigured: {m}"),
        }
    }
}

impl std::error::Error for OidcError {}

// ── JWKS types ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct JwksResponse {
    keys: Vec<JwkEntry>,
}

#[derive(Debug, Deserialize)]
struct JwkEntry {
    kty: String,
    #[serde(rename = "use")]
    key_use: Option<String>,
    kid: String,
    crv: Option<String>,
    x: Option<String>,
}

// ── Token claim types ─────────────────────────────────────────────────────────

/// Claims from a validated RFC 9068 `at+jwt` access token.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct AtClaims {
    pub sub: String,
    pub iss: String,
    pub aud: serde_json::Value,
    pub scope: Option<String>,
    pub jti: String,
    pub exp: i64,
    pub iat: i64,
}

/// Claims from an OIDC ID token (`typ: JWT`).
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct IdTokenClaims {
    pub iss: String,
    pub sub: String,
    pub aud: serde_json::Value,
    pub exp: i64,
    pub iat: i64,
    pub nonce: Option<String>,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub name: Option<String>,
    /// `"admin"` or `"subscriber"` - synced to `users.is_admin`.
    pub role: Option<String>,
    pub has_member_access: Option<bool>,
}

/// Claims from an OIDC Back-Channel Logout token (`typ: logout+jwt`).
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct LogoutTokenClaims {
    pub iss: String,
    pub aud: serde_json::Value,
    pub iat: i64,
    pub jti: String,
    pub sub: Option<String>,
    pub sid: Option<String>,
    pub events: serde_json::Value,
}

/// Inner payload of a lifecycle event.
#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct LifecycleEventPayload {
    pub subject: LifecycleSubject,
    #[serde(rename = "type")]
    pub event_type: String,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LifecycleSubject {
    pub id: String,
}

/// Claims from a lifecycle-event token (`typ: lifecycle-event+jwt`).
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct LifecycleTokenClaims {
    pub iss: String,
    pub aud: serde_json::Value,
    pub iat: i64,
    pub jti: String,
    pub events: HashMap<String, LifecycleEventPayload>,
}

impl LifecycleTokenClaims {
    const LIFECYCLE_EVENT_KEY: &'static str = "https://schemas.a8n.tools/event/user-lifecycle";

    pub fn lifecycle_event(&self) -> Option<&LifecycleEventPayload> {
        self.events.get(Self::LIFECYCLE_EVENT_KEY)
    }
}

// ── JWKS cache ────────────────────────────────────────────────────────────────

struct JwksCache {
    keys: HashMap<String, DecodingKey>,
    refreshed_at: chrono::DateTime<Utc>,
}

// ── OidcVerifier ──────────────────────────────────────────────────────────────

/// Validates tokens from the SaaS OIDC provider. Shared via `Arc`; cloning is cheap.
#[derive(Clone)]
pub struct OidcVerifier {
    pub config: OidcConfig,
    pub http: reqwest::Client,
    cache: Arc<RwLock<Option<JwksCache>>>,
}

impl OidcVerifier {
    pub fn new(config: OidcConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("failed to build HTTP client"),
            cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Validate an OIDC ID token (`typ: JWT`).
    pub async fn verify_id_token(
        &self,
        token: &str,
        expected_nonce: &str,
    ) -> Result<IdTokenClaims, OidcError> {
        let header = jsonwebtoken::decode_header(token)
            .map_err(|e| OidcError::InvalidToken(format!("header parse failed: {e}")))?;

        if header.typ.as_deref() != Some("JWT") && header.typ.is_some() {
            return Err(OidcError::InvalidToken("ID token typ must be JWT".into()));
        }

        let kid = header
            .kid
            .as_deref()
            .ok_or_else(|| OidcError::InvalidToken("ID token missing kid".into()))?
            .to_string();

        let claims = self.try_validate_id_token(token, &kid, expected_nonce).await;
        if let Err(OidcError::Rejected) = &claims {
            self.refresh_jwks().await?;
            return self.try_validate_id_token(token, &kid, expected_nonce).await;
        }
        claims
    }

    /// Validate an `at+jwt` Bearer token. Returns claims on success.
    #[allow(dead_code)]
    pub async fn verify_access_token(&self, token: &str) -> Result<AtClaims, OidcError> {
        let header = jsonwebtoken::decode_header(token)
            .map_err(|e| OidcError::InvalidToken(format!("header parse failed: {e}")))?;

        if header.typ.as_deref() != Some("at+jwt") {
            return Err(OidcError::InvalidToken("access token typ must be at+jwt".into()));
        }

        let kid = header
            .kid
            .as_deref()
            .ok_or_else(|| OidcError::InvalidToken("access token missing kid".into()))?
            .to_string();

        let claims = self.try_validate_at(token, &kid).await;
        if let Err(OidcError::Rejected) = &claims {
            self.refresh_jwks().await?;
            return self.try_validate_at(token, &kid).await;
        }
        claims
    }

    /// Validate an OIDC Back-Channel Logout token (`typ: logout+jwt`).
    pub async fn verify_logout_token(&self, token: &str) -> Result<LogoutTokenClaims, OidcError> {
        let header = jsonwebtoken::decode_header(token)
            .map_err(|e| OidcError::InvalidToken(format!("header parse failed: {e}")))?;

        if header.typ.as_deref() != Some("logout+jwt") {
            return Err(OidcError::InvalidToken(
                "logout token typ must be logout+jwt".into(),
            ));
        }

        let kid = header
            .kid
            .as_deref()
            .ok_or_else(|| OidcError::InvalidToken("logout token missing kid".into()))?
            .to_string();

        let claims = self
            .try_validate_event_token::<LogoutTokenClaims>(token, &kid)
            .await;
        let claims = match claims {
            Err(OidcError::Rejected) => {
                self.refresh_jwks().await?;
                self.try_validate_event_token::<LogoutTokenClaims>(token, &kid)
                    .await?
            }
            other => other?,
        };

        self.validate_event_iat(claims.iat)?;

        const BACKCHANNEL_LOGOUT_EVENT: &str =
            "http://schemas.openid.net/event/backchannel-logout";
        if claims.events.get(BACKCHANNEL_LOGOUT_EVENT).is_none() {
            return Err(OidcError::InvalidToken(
                "logout token missing backchannel-logout event".into(),
            ));
        }

        Ok(claims)
    }

    /// Validate a lifecycle-event token (`typ: lifecycle-event+jwt`).
    pub async fn verify_lifecycle_token(
        &self,
        token: &str,
    ) -> Result<LifecycleTokenClaims, OidcError> {
        let header = jsonwebtoken::decode_header(token)
            .map_err(|e| OidcError::InvalidToken(format!("header parse failed: {e}")))?;

        if header.typ.as_deref() != Some("lifecycle-event+jwt") {
            return Err(OidcError::InvalidToken(
                "lifecycle token typ must be lifecycle-event+jwt".into(),
            ));
        }

        let kid = header
            .kid
            .as_deref()
            .ok_or_else(|| OidcError::InvalidToken("lifecycle token missing kid".into()))?
            .to_string();

        let claims = self
            .try_validate_event_token::<LifecycleTokenClaims>(token, &kid)
            .await;
        let claims = match claims {
            Err(OidcError::Rejected) => {
                self.refresh_jwks().await?;
                self.try_validate_event_token::<LifecycleTokenClaims>(token, &kid)
                    .await?
            }
            other => other?,
        };

        self.validate_event_iat(claims.iat)?;
        Ok(claims)
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    async fn try_validate_at(&self, token: &str, kid: &str) -> Result<AtClaims, OidcError> {
        self.ensure_cache().await?;
        let guard = self.cache.read().await;
        let cache = guard
            .as_ref()
            .ok_or_else(|| OidcError::Configuration("JWKS cache empty after refresh".into()))?;
        let decoding_key = cache.keys.get(kid).ok_or(OidcError::Rejected)?;

        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.set_issuer(&[&self.config.issuer]);
        validation.set_audience(&[&self.config.audience]);
        validation.validate_exp = true;
        validation.leeway = self.config.leeway_seconds;

        jsonwebtoken::decode::<AtClaims>(token, decoding_key, &validation)
            .map(|d| d.claims)
            .map_err(|_| OidcError::Rejected)
    }

    async fn try_validate_id_token(
        &self,
        token: &str,
        kid: &str,
        expected_nonce: &str,
    ) -> Result<IdTokenClaims, OidcError> {
        self.ensure_cache().await?;
        let guard = self.cache.read().await;
        let cache = guard
            .as_ref()
            .ok_or_else(|| OidcError::Configuration("JWKS cache empty after refresh".into()))?;
        let decoding_key = cache.keys.get(kid).ok_or(OidcError::Rejected)?;

        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.set_issuer(&[&self.config.issuer]);
        validation.set_audience(&[&self.config.client_id]);
        validation.validate_exp = true;
        validation.leeway = self.config.leeway_seconds;

        let claims = jsonwebtoken::decode::<IdTokenClaims>(token, decoding_key, &validation)
            .map(|d| d.claims)
            .map_err(|e| OidcError::InvalidToken(format!("ID token verification failed: {e}")))?;

        match claims.nonce.as_deref() {
            Some(n) if n == expected_nonce => {}
            Some(_) => return Err(OidcError::InvalidToken("ID token nonce mismatch".into())),
            None => return Err(OidcError::InvalidToken("ID token missing nonce".into())),
        }

        Ok(claims)
    }

    async fn try_validate_event_token<T>(&self, token: &str, kid: &str) -> Result<T, OidcError>
    where
        T: serde::de::DeserializeOwned,
    {
        self.ensure_cache().await?;
        let guard = self.cache.read().await;
        let cache = guard
            .as_ref()
            .ok_or_else(|| OidcError::Configuration("JWKS cache empty after refresh".into()))?;
        let decoding_key = cache.keys.get(kid).ok_or(OidcError::Rejected)?;

        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.set_issuer(&[&self.config.issuer]);
        validation.set_audience(&[&self.config.client_id]);
        validation.validate_exp = false;
        validation.leeway = self.config.leeway_seconds;
        validation.required_spec_claims.remove("exp");

        jsonwebtoken::decode::<T>(token, decoding_key, &validation)
            .map(|d| d.claims)
            .map_err(|e| OidcError::InvalidToken(format!("event token verification failed: {e}")))
    }

    fn validate_event_iat(&self, iat: i64) -> Result<(), OidcError> {
        const EVENT_TOKEN_WINDOW_SECS: i64 = 120;
        let age = Utc::now().timestamp() - iat;
        if age < -30 {
            return Err(OidcError::InvalidToken(
                "event token issued in the future".into(),
            ));
        }
        if age > EVENT_TOKEN_WINDOW_SECS {
            return Err(OidcError::InvalidToken("event token too old".into()));
        }
        Ok(())
    }

    async fn ensure_cache(&self) -> Result<(), OidcError> {
        let needs_refresh = {
            let guard = self.cache.read().await;
            match guard.as_ref() {
                None => true,
                Some(c) => {
                    (Utc::now() - c.refreshed_at).num_seconds()
                        > self.config.jwks_cache_ttl as i64
                }
            }
        };
        if needs_refresh {
            self.refresh_jwks().await?;
        }
        Ok(())
    }

    async fn refresh_jwks(&self) -> Result<(), OidcError> {
        let jwks_url = &self.config.jwks_url;
        if jwks_url.is_empty() {
            return Err(OidcError::Configuration("OIDC_JWKS_URL not configured".into()));
        }

        let resp: JwksResponse = self
            .http
            .get(jwks_url)
            .send()
            .await
            .map_err(|e| OidcError::Jwks(format!("JWKS fetch failed: {e}")))?
            .json()
            .await
            .map_err(|e| OidcError::Jwks(format!("JWKS parse failed: {e}")))?;

        let mut keys = HashMap::new();
        for entry in &resp.keys {
            if entry.kty != "OKP" {
                continue;
            }
            if entry.crv.as_deref() != Some("Ed25519") {
                continue;
            }
            if entry.key_use.as_deref().is_some_and(|u| u != "sig") {
                continue;
            }
            let Some(x) = &entry.x else { continue };

            match ed25519_spki_pem_from_x(x) {
                Ok(pem) => match DecodingKey::from_ed_pem(pem.as_bytes()) {
                    Ok(key) => {
                        keys.insert(entry.kid.clone(), key);
                    }
                    Err(e) => {
                        tracing::warn!(kid = %entry.kid, error = %e, "failed to parse JWKS key");
                    }
                },
                Err(e) => {
                    tracing::warn!(kid = %entry.kid, error = %e, "failed to reconstruct SPKI PEM");
                }
            }
        }

        if keys.is_empty() {
            return Err(OidcError::Jwks(
                "JWKS response contained no usable Ed25519 keys".into(),
            ));
        }

        let mut guard = self.cache.write().await;
        *guard = Some(JwksCache {
            keys,
            refreshed_at: Utc::now(),
        });

        Ok(())
    }
}

// ── Ed25519 SPKI PEM reconstruction from JWK `x` ─────────────────────────────

/// Build a SubjectPublicKeyInfo PEM string from a base64url-encoded 32-byte Ed25519 key.
///
/// SPKI DER for Ed25519 has a fixed 12-byte header followed by the 32-byte raw key:
///   30 2A 30 05 06 03 2B 65 70 03 21 00 <32 bytes>
fn ed25519_spki_pem_from_x(x_b64url: &str) -> Result<String, String> {
    let key_bytes = URL_SAFE_NO_PAD
        .decode(x_b64url)
        .map_err(|e| format!("base64url decode failed: {e}"))?;

    if key_bytes.len() != 32 {
        return Err(format!(
            "expected 32-byte Ed25519 key, got {} bytes",
            key_bytes.len()
        ));
    }

    let header: [u8; 12] = [
        0x30, 0x2A, 0x30, 0x05, 0x06, 0x03, 0x2B, 0x65, 0x70, 0x03, 0x21, 0x00,
    ];
    let mut der = Vec::with_capacity(44);
    der.extend_from_slice(&header);
    der.extend_from_slice(&key_bytes);

    let b64 = base64::engine::general_purpose::STANDARD.encode(&der);
    Ok(format!(
        "-----BEGIN PUBLIC KEY-----\n{b64}\n-----END PUBLIC KEY-----\n"
    ))
}
