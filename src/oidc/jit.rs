//! Just-In-Time provisioning for OIDC-authenticated users.
//!
//! Maps the SaaS user identity (`sub` claim, a UUID) onto the local rus
//! `users` table. The local PK remains an INTEGER `userID`; `saas_user_id`
//! stores the upstream UUID for later joins.

use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use super::verifier::IdTokenClaims;

#[derive(Debug)]
#[allow(dead_code)]
pub struct ProvisionedUser {
    pub user_id: i64,
    pub is_admin: bool,
    pub session_version: i32,
}

#[derive(Debug)]
pub enum JitError {
    Forbidden(String),
    Internal(String),
}

impl std::fmt::Display for JitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JitError::Forbidden(m) => write!(f, "forbidden: {m}"),
            JitError::Internal(m) => write!(f, "internal error: {m}"),
        }
    }
}

impl std::error::Error for JitError {}

impl From<rusqlite::Error> for JitError {
    fn from(value: rusqlite::Error) -> Self {
        JitError::Internal(format!("db error: {value}"))
    }
}

/// Load an existing local user by their SaaS `sub`, or provision one on first login.
///
/// Returns `Forbidden` if the user is suspended or lacks membership access.
pub fn load_or_provision(
    db: &Connection,
    id_claims: &IdTokenClaims,
) -> Result<ProvisionedUser, JitError> {
    let saas_uuid: Uuid = id_claims
        .sub
        .parse()
        .map_err(|_| JitError::Internal("Invalid sub claim in ID token".into()))?;
    let saas_uuid_str = saas_uuid.to_string();

    let is_admin = id_claims.role.as_deref() == Some("admin");

    // Try to load by saas_user_id first (common path after first login).
    let existing: Option<(i64, Option<String>, i32)> = db
        .query_row(
            "SELECT userID, suspended_at, session_version FROM users WHERE saas_user_id = ?1",
            params![&saas_uuid_str],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i32>(2)?,
                ))
            },
        )
        .optional()?;

    if let Some((user_id, suspended_at, session_version)) = existing {
        if suspended_at.is_some() {
            return Err(JitError::Forbidden(
                "Your account has been suspended. Contact support.".into(),
            ));
        }
        if !id_claims.has_member_access.unwrap_or(false) {
            return Err(JitError::Forbidden(
                "An active a8n.tools membership is required to access this app. \
                 Please upgrade your plan at a8n.tools."
                    .into(),
            ));
        }

        let email = id_claims.email.as_deref().unwrap_or("");
        db.execute(
            "UPDATE users SET email = ?1, is_admin = ?2 WHERE userID = ?3",
            params![email, is_admin as i32, user_id],
        )?;

        return Ok(ProvisionedUser {
            user_id,
            is_admin,
            session_version,
        });
    }

    // First-time login.
    if !id_claims.email_verified.unwrap_or(false) {
        return Err(JitError::Forbidden(
            "Please verify your email on a8n.tools before logging in.".into(),
        ));
    }

    if !id_claims.has_member_access.unwrap_or(false) {
        return Err(JitError::Forbidden(
            "An active a8n.tools membership is required to access this app. \
             Please upgrade your plan at a8n.tools."
                .into(),
        ));
    }

    let email = id_claims
        .email
        .as_deref()
        .ok_or_else(|| JitError::Internal("ID token missing email claim".into()))?;

    // Try linking to an existing standalone account by email before inserting.
    let linked: Option<(i64, i32)> = db
        .query_row(
            "SELECT userID, session_version FROM users WHERE email = ?1 AND saas_user_id IS NULL",
            params![email],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i32>(1)?)),
        )
        .optional()?;

    if let Some((user_id, session_version)) = linked {
        db.execute(
            "UPDATE users SET saas_user_id = ?1, is_admin = ?2 WHERE userID = ?3",
            params![&saas_uuid_str, is_admin as i32, user_id],
        )?;
        tracing::info!(user_id, saas_user_id = %saas_uuid, "Linked standalone account to SSO identity");
        return Ok(ProvisionedUser {
            user_id,
            is_admin,
            session_version,
        });
    }

    // Username derivation: email local-part, with collision fallback to `saas_<short>`.
    let base_username = id_claims
        .name
        .as_deref()
        .map(|n| n.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| email.split('@').next().unwrap_or("user").to_string());

    // Try the derived username, then numeric suffixes, falling back to a
    // saas-uuid-derived name. The INSERT itself is the race-safe arbiter:
    // a concurrent first-login that picked the same username will lose the
    // UNIQUE race and we'll bump the suffix and retry.
    let mut username = base_username.clone();
    let mut suffix: u32 = 0;
    let user_id = loop {
        match db.execute(
            "INSERT INTO users (username, password, is_admin, saas_user_id, email, session_version)
             VALUES (?1, '!sso:no-password', ?2, ?3, ?4, 0)",
            params![&username, is_admin as i32, &saas_uuid_str, email],
        ) {
            Ok(_) => break db.last_insert_rowid(),
            Err(e) => {
                let msg = e.to_string();
                let unique_violation = msg.contains("UNIQUE constraint failed");
                if !unique_violation {
                    return Err(JitError::Internal(format!("user insert failed: {msg}")));
                }
                suffix += 1;
                if suffix > 1000 {
                    // Last-resort guaranteed-unique fallback.
                    username = format!("sso_{}", &saas_uuid_str[..8]);
                    db.execute(
                        "INSERT INTO users (username, password, is_admin, saas_user_id, email, session_version)
                         VALUES (?1, '!sso:no-password', ?2, ?3, ?4, 0)",
                        params![&username, is_admin as i32, &saas_uuid_str, email],
                    )?;
                    break db.last_insert_rowid();
                }
                username = format!("{base_username}_{suffix}");
            }
        }
    };

    tracing::info!(user_id, saas_user_id = %saas_uuid, username = %username, "SSO user JIT-provisioned");

    Ok(ProvisionedUser {
        user_id,
        is_admin,
        session_version: 0,
    })
}
