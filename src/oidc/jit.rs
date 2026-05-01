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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{id_claims, make_test_state};

    const SUB_A: &str = "11111111-1111-1111-1111-111111111111";
    const SUB_B: &str = "22222222-2222-2222-2222-222222222222";

    #[test]
    fn first_login_provisions_user() {
        let state = make_test_state();
        let claims = id_claims(SUB_A, Some("alice@example.com"), true, true, None);
        let db = state.db.lock().unwrap();
        let p = load_or_provision(&db, &claims).expect("provision");
        assert!(p.user_id > 0);
        assert!(!p.is_admin);
        assert_eq!(p.session_version, 0);

        let (uname, saas_id, is_admin): (String, String, i32) = db
            .query_row(
                "SELECT username, saas_user_id, is_admin FROM users WHERE userID = ?1",
                params![p.user_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(uname, "alice");
        assert_eq!(saas_id, SUB_A);
        assert_eq!(is_admin, 0);
    }

    #[test]
    fn admin_role_sets_is_admin() {
        let state = make_test_state();
        let claims = id_claims(SUB_A, Some("admin@example.com"), true, true, Some("admin"));
        let db = state.db.lock().unwrap();
        let p = load_or_provision(&db, &claims).unwrap();
        assert!(p.is_admin);
    }

    #[test]
    fn second_login_returns_existing_user() {
        let state = make_test_state();
        let claims = id_claims(SUB_A, Some("alice@example.com"), true, true, None);
        let db = state.db.lock().unwrap();
        let p1 = load_or_provision(&db, &claims).unwrap();
        let p2 = load_or_provision(&db, &claims).unwrap();
        assert_eq!(p1.user_id, p2.user_id);
        let count: i64 = db
            .query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn admin_status_synced_on_each_login() {
        let state = make_test_state();
        let db = state.db.lock().unwrap();
        let _ = load_or_provision(
            &db,
            &id_claims(SUB_A, Some("u@example.com"), true, true, None),
        )
        .unwrap();
        // Promotes
        let _ = load_or_provision(
            &db,
            &id_claims(SUB_A, Some("u@example.com"), true, true, Some("admin")),
        )
        .unwrap();
        let is_admin: i32 = db
            .query_row(
                "SELECT is_admin FROM users WHERE saas_user_id = ?1",
                params![SUB_A],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(is_admin, 1);
        // Demotes
        let _ = load_or_provision(
            &db,
            &id_claims(SUB_A, Some("u@example.com"), true, true, None),
        )
        .unwrap();
        let is_admin: i32 = db
            .query_row(
                "SELECT is_admin FROM users WHERE saas_user_id = ?1",
                params![SUB_A],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(is_admin, 0);
    }

    #[test]
    fn email_not_verified_is_forbidden_first_login() {
        let state = make_test_state();
        let claims = id_claims(SUB_A, Some("u@example.com"), false, true, None);
        let db = state.db.lock().unwrap();
        match load_or_provision(&db, &claims) {
            Err(JitError::Forbidden(m)) => assert!(m.to_lowercase().contains("verify")),
            other => panic!("expected Forbidden, got {other:?}"),
        }
    }

    #[test]
    fn no_membership_is_forbidden_first_login() {
        let state = make_test_state();
        let claims = id_claims(SUB_A, Some("u@example.com"), true, false, None);
        let db = state.db.lock().unwrap();
        match load_or_provision(&db, &claims) {
            Err(JitError::Forbidden(m)) => assert!(m.to_lowercase().contains("membership")),
            other => panic!("expected Forbidden, got {other:?}"),
        }
    }

    #[test]
    fn no_membership_blocks_existing_user() {
        let state = make_test_state();
        let db = state.db.lock().unwrap();
        load_or_provision(
            &db,
            &id_claims(SUB_A, Some("u@example.com"), true, true, None),
        )
        .unwrap();
        match load_or_provision(
            &db,
            &id_claims(SUB_A, Some("u@example.com"), true, false, None),
        ) {
            Err(JitError::Forbidden(_)) => {}
            other => panic!("expected Forbidden, got {other:?}"),
        }
    }

    #[test]
    fn suspended_existing_user_is_forbidden() {
        let state = make_test_state();
        let db = state.db.lock().unwrap();
        load_or_provision(
            &db,
            &id_claims(SUB_A, Some("u@example.com"), true, true, None),
        )
        .unwrap();
        db.execute(
            "UPDATE users SET suspended_at = '2026-01-01T00:00:00Z' WHERE saas_user_id = ?1",
            params![SUB_A],
        )
        .unwrap();
        match load_or_provision(
            &db,
            &id_claims(SUB_A, Some("u@example.com"), true, true, None),
        ) {
            Err(JitError::Forbidden(m)) => assert!(m.to_lowercase().contains("suspended")),
            other => panic!("expected Forbidden, got {other:?}"),
        }
    }

    #[test]
    fn invalid_sub_uuid_is_internal_error() {
        let state = make_test_state();
        let db = state.db.lock().unwrap();
        let mut claims = id_claims("not-a-uuid", Some("u@example.com"), true, true, None);
        claims.sub = "not-a-uuid".into();
        match load_or_provision(&db, &claims) {
            Err(JitError::Internal(m)) => assert!(m.contains("sub")),
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn missing_email_first_login_is_internal_error() {
        let state = make_test_state();
        let db = state.db.lock().unwrap();
        let claims = id_claims(SUB_A, None, true, true, None);
        match load_or_provision(&db, &claims) {
            Err(JitError::Internal(m)) => assert!(m.contains("email")),
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn links_existing_standalone_account_by_email() {
        let state = make_test_state();
        let db = state.db.lock().unwrap();
        // Pre-existing standalone user
        db.execute(
            "INSERT INTO users (username, password, email) VALUES ('alice', 'hash', 'alice@example.com')",
            [],
        )
        .unwrap();
        let pre_id = db.last_insert_rowid();

        let claims = id_claims(SUB_A, Some("alice@example.com"), true, true, None);
        let p = load_or_provision(&db, &claims).unwrap();
        assert_eq!(p.user_id, pre_id, "should reuse the standalone row, not insert");
        let count: i64 = db
            .query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        let saas_id: String = db
            .query_row(
                "SELECT saas_user_id FROM users WHERE userID = ?1",
                params![pre_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(saas_id, SUB_A);
    }

    #[test]
    fn username_collision_bumps_suffix() {
        let state = make_test_state();
        let db = state.db.lock().unwrap();
        // First user takes username "alice"
        load_or_provision(
            &db,
            &id_claims(SUB_A, Some("alice@example.com"), true, true, None),
        )
        .unwrap();
        // Second user, different sub, same email local-part -> should get "alice_1"
        let p2 = load_or_provision(
            &db,
            &id_claims(SUB_B, Some("alice@other.example"), true, true, None),
        )
        .unwrap();
        let uname: String = db
            .query_row(
                "SELECT username FROM users WHERE userID = ?1",
                params![p2.user_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(uname, "alice_1");
    }
}
