//! DEV-300: the fleet-standard `SETUP_DEFAULT_ADMIN` bootstrap.
//!
//! menkent (`api/src/main.rs`), bunyip (`bunyip-api/src/main.rs`), eform
//! (`src/main.rs`) and lets-chat (`server/src/setup_admin.rs`) all read one
//! `SETUP_DEFAULT_ADMIN=email:password` variable and create an admin while no
//! admin exists. rus keeps the same single variable so the fleet stays
//! greppable; rusty-links' three-variable `SETUP_DEFAULT_ADMIN_EMAIL` /
//! `_PASSWORD` / `_NAME` spelling is the outlier and is deliberately not copied.
//!
//! **Dev gate.** Seeding happens on a debug build only, the same detection
//! `routes::configure_app` already uses to mount `/dev/seed-session`. Every
//! image rus ships is `cargo build --release` (`oci-build/Dockerfile`), so a
//! deployed binary cannot seed whatever its environment says.
//!
//! **Standalone only.** The saas leg has no local password to seed, and
//! `oidc::jit::load_or_provision` rewrites `users.is_admin` from the OP's
//! `role` claim on every login, so a seeded admin there would be demoted by the
//! first sign-in. That leg refuses with a log line rather than writing a row
//! the next login erases.
//!
//! **Interaction with the first-user rule.** `handlers::auth::register` grants
//! admin to the first registered account (`user_count == 0`). A seeded admin
//! occupies that slot on purpose: the developer signs in as it instead of
//! racing to register first, and the next registration is an ordinary user.
//! `pages::check_setup_required` reports `false` for the same reason.

#[cfg(feature = "standalone")]
use rusqlite::params;
use rusqlite::Connection;

use crate::db::AppState;

/// What a `SETUP_DEFAULT_ADMIN` value resolves to, before any database work.
#[derive(Clone, PartialEq, Eq)]
pub enum Seed {
    /// Unset or blank: seed nothing, say nothing.
    Skip,
    /// Set but unusable. Carries the operator-facing reason.
    Refused(String),
    /// Seed this admin if none exists.
    #[cfg(feature = "standalone")]
    Admin {
        username: String,
        email: String,
        password: String,
    },
}

/// Written by hand rather than derived: this enum exists to carry a credential,
/// so a `{:?}` of it anywhere near a log must not publish the password.
impl std::fmt::Debug for Seed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Seed::Skip => write!(f, "Skip"),
            Seed::Refused(why) => f.debug_tuple("Refused").field(why).finish(),
            #[cfg(feature = "standalone")]
            Seed::Admin {
                username, email, ..
            } => f
                .debug_struct("Admin")
                .field("username", username)
                .field("email", email)
                .field("password", &"<redacted>")
                .finish(),
        }
    }
}

/// Pure half of [`ensure_default_admin`], so both sides of the dev gate are
/// reachable from a test: a test binary is always a debug build, so the release
/// branch exists only because `dev_build` is an argument.
pub fn decide(dev_build: bool, raw: Option<&str>) -> Seed {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Seed::Skip;
    };
    // Checked after the presence test so a production deployment that sets the
    // variable by mistake gets a log line instead of silence.
    if !dev_build {
        return Seed::Refused(
            "SETUP_DEFAULT_ADMIN is set on a release build; refusing to seed an admin".to_string(),
        );
    }
    parse(raw)
}

/// The saas leg never seeds: there is no local password to seed, and
/// `oidc::jit::load_or_provision` overwrites `users.is_admin` from the OP's
/// `role` claim on every login, so the row would be demoted by the first
/// sign-in.
#[cfg(feature = "saas")]
fn parse(_raw: &str) -> Seed {
    Seed::Refused(
        "SETUP_DEFAULT_ADMIN is set on a saas build; sign-in and the admin role come from the \
         OIDC provider, so nothing is seeded"
            .to_string(),
    )
}

#[cfg(feature = "standalone")]
fn parse(raw: &str) -> Seed {
    let Some((email, password)) = raw.split_once(':') else {
        return Seed::Refused("SETUP_DEFAULT_ADMIN must be in format 'email:password'".to_string());
    };
    let password = password.trim();

    // The same address and password rules POST /api/register applies, so the
    // seeded row is shaped like a registered one and its credential is one the
    // app would accept if it were typed into the signup form.
    let email = match crate::mailer::normalize_account_email(email) {
        Ok(Some(value)) => value,
        Ok(None) => {
            return Seed::Refused(
                "SETUP_DEFAULT_ADMIN needs an email address before the ':'".to_string(),
            )
        }
        Err(why) => return Seed::Refused(format!("SETUP_DEFAULT_ADMIN email is unusable: {why}")),
    };
    if let Err(why) = crate::security::validate_password(password) {
        return Seed::Refused(format!("SETUP_DEFAULT_ADMIN password is unusable: {why}"));
    }
    let Some(username) = derive_username(&email) else {
        return Seed::Refused(format!(
            "SETUP_DEFAULT_ADMIN cannot derive a username from {email:?}: the part before the '@' \
             must leave at least 3 characters"
        ));
    };

    Seed::Admin {
        username,
        email,
        password: password.to_string(),
    }
}

/// The email local part, reduced to the characters `POST /api/register`
/// accepts. `None` when too little survives to be a valid username.
#[cfg(feature = "standalone")]
fn derive_username(email: &str) -> Option<String> {
    let username: String = email
        .split('@')
        .next()
        .unwrap_or_default()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    (username.chars().count() >= 3).then_some(username)
}

/// Startup entry point: resolve the gate from this build and this environment,
/// then seed.
pub fn ensure_default_admin(state: &AppState) {
    let raw = std::env::var("SETUP_DEFAULT_ADMIN").ok();
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    seed_default_admin(&db, cfg!(debug_assertions), raw.as_deref());
}

/// Both inputs of the gate are arguments, so a test can drive the release-build
/// branch against a real database and prove it writes nothing.
///
/// Idempotent: a second boot finds the admin the first one seeded and skips, so
/// it never creates a duplicate and never errors. Every outcome is logged.
// The saas leg never reaches a write, so `db` is unused there by construction.
#[cfg_attr(feature = "saas", allow(unused_variables))]
pub fn seed_default_admin(db: &Connection, dev_build: bool, raw: Option<&str>) {
    match decide(dev_build, raw) {
        Seed::Skip => {}
        Seed::Refused(why) => tracing::error!(target: "setup_admin", "{why}"),
        #[cfg(feature = "standalone")]
        Seed::Admin {
            username,
            email,
            password,
        } => insert_admin(db, &username, &email, &password),
    }
}

#[cfg(feature = "standalone")]
fn insert_admin(db: &Connection, username: &str, email: &str, password: &str) {
    let admins: i64 = match db.query_row("SELECT COUNT(*) FROM users WHERE is_admin = 1", [], |r| {
        r.get(0)
    }) {
        Ok(count) => count,
        Err(error) => {
            tracing::error!(target: "setup_admin", error = %error, "could not count admins; skipping SETUP_DEFAULT_ADMIN");
            return;
        }
    };
    if admins > 0 {
        tracing::info!(target: "setup_admin", "admin user(s) already exist, skipping SETUP_DEFAULT_ADMIN");
        return;
    }

    let Some(username) = free_username(db, username) else {
        tracing::error!(target: "setup_admin", %username, "no free username near this one, or the lookup failed; skipping SETUP_DEFAULT_ADMIN");
        return;
    };
    let Ok(hash) = crate::handlers::auth::hash_password(password) else {
        tracing::error!(target: "setup_admin", "password hashing failed; skipping SETUP_DEFAULT_ADMIN");
        return;
    };

    match db.execute(
        "INSERT INTO users (username, password, is_admin, email) VALUES (?1, ?2, 1, ?3)",
        params![&username, &hash, email],
    ) {
        Ok(_) => tracing::warn!(
            target: "setup_admin",
            user_id = db.last_insert_rowid(), %username, %email,
            "DEV ONLY: seeded the default admin from SETUP_DEFAULT_ADMIN"
        ),
        Err(error) => tracing::error!(
            target: "setup_admin", error = %error,
            "failed to seed the default admin from SETUP_DEFAULT_ADMIN"
        ),
    }
}

/// First free username at or near `base`, mirroring the collision loop in
/// `oidc::jit::load_or_provision`. `None` when every candidate is taken.
#[cfg(feature = "standalone")]
fn free_username(db: &Connection, base: &str) -> Option<String> {
    std::iter::once(base.to_string())
        .chain((2..=5u32).map(|n| format!("{base}-{n}")))
        .find(|candidate| {
            db.query_row(
                "SELECT COUNT(*) FROM users WHERE username = ?1",
                params![candidate],
                |r| r.get::<_, i64>(0),
            )
            .is_ok_and(|taken| taken == 0)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::make_test_state;

    /// One value, driven through every gate below, so only the gate differs.
    const CONFIGURED: &str = "admin@a8n.run:Admin1234!";

    fn user_count(db: &Connection) -> i64 {
        db.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))
            .expect("count users")
    }

    fn admin_count(db: &Connection) -> i64 {
        db.query_row("SELECT COUNT(*) FROM users WHERE is_admin = 1", [], |r| {
            r.get(0)
        })
        .expect("count admins")
    }

    /// The gate that keeps a production deployment safe. Same value, same empty
    /// database; only the build profile differs, and nothing may be written.
    #[test]
    fn a_release_build_writes_nothing_even_when_configured() {
        assert!(matches!(decide(false, Some(CONFIGURED)), Seed::Refused(_)));

        let state = make_test_state();
        let db = state.db.lock().unwrap();
        seed_default_admin(&db, false, Some(CONFIGURED));
        assert_eq!(user_count(&db), 0, "a release build seeded a user");
    }

    #[test]
    fn an_unset_or_blank_value_is_silent_on_both_sides_of_the_gate() {
        for raw in [None, Some(""), Some("   ")] {
            assert_eq!(decide(true, raw), Seed::Skip, "{raw:?}");
            assert_eq!(decide(false, raw), Seed::Skip, "{raw:?}");
        }

        let state = make_test_state();
        let db = state.db.lock().unwrap();
        for raw in [None, Some(""), Some("   ")] {
            seed_default_admin(&db, true, raw);
        }
        assert_eq!(user_count(&db), 0);
    }

    /// The password is a credential, so `{:?}` must never publish it. Guards
    /// the manual `Debug` against someone replacing it with a derive.
    #[test]
    fn the_debug_rendering_never_carries_the_password() {
        let rendered = format!("{:?}", decide(true, Some(CONFIGURED)));
        assert!(!rendered.contains("Admin1234!"), "got {rendered}");
    }

    #[cfg(feature = "standalone")]
    mod standalone {
        use super::*;

        #[test]
        fn dev_mode_seeds_exactly_one_admin() {
            let state = make_test_state();
            let db = state.db.lock().unwrap();
            seed_default_admin(&db, true, Some(CONFIGURED));

            assert_eq!(admin_count(&db), 1, "exactly one admin");
            let (username, email, hash): (String, String, String) = db
                .query_row(
                    "SELECT username, email, password FROM users WHERE is_admin = 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .expect("the seeded row");
            assert_eq!(
                username, "admin",
                "username derives from the email local part"
            );
            assert_eq!(email, "admin@a8n.run");
            assert!(
                hash.starts_with("$argon2"),
                "the password must be hashed the way register hashes it, got {hash}"
            );
        }

        #[test]
        fn a_second_boot_neither_duplicates_nor_errors() {
            let state = make_test_state();
            let db = state.db.lock().unwrap();
            for _ in 0..3 {
                seed_default_admin(&db, true, Some(CONFIGURED));
            }
            assert_eq!(user_count(&db), 1, "still one row after three boots");
            assert_eq!(admin_count(&db), 1);
        }

        #[test]
        fn an_existing_admin_is_left_alone() {
            let state = make_test_state();
            crate::testing::insert_test_user(&state, "boss", true);
            let db = state.db.lock().unwrap();
            seed_default_admin(&db, true, Some(CONFIGURED));

            assert_eq!(user_count(&db), 1, "no second admin seeded");
            assert_eq!(admin_count(&db), 1);
        }

        /// A dev database with users but no admin is exactly the state the seed
        /// is for, and the derived username may already be taken.
        #[test]
        fn a_taken_username_is_suffixed_rather_than_colliding() {
            let state = make_test_state();
            crate::testing::insert_test_user(&state, "admin", false);
            let db = state.db.lock().unwrap();
            seed_default_admin(&db, true, Some(CONFIGURED));

            assert_eq!(admin_count(&db), 1);
            let username: String = db
                .query_row("SELECT username FROM users WHERE is_admin = 1", [], |r| {
                    r.get(0)
                })
                .expect("the seeded row");
            assert_eq!(username, "admin-2");
        }

        #[test]
        fn a_malformed_value_writes_nothing() {
            // No separator; blank email; an address the alert could never
            // reach; a password register itself would reject; a local part
            // with nothing left to make a username from.
            let malformed = [
                "admin@a8n.run",
                ":Admin1234!",
                "admin:Admin1234!",
                "admin@a8n.run:admin1234",
                "a b@a8n.run:Admin1234!",
                "ab@a8n.run:Admin1234!",
            ];
            let state = make_test_state();
            let db = state.db.lock().unwrap();
            for raw in malformed {
                assert!(
                    matches!(decide(true, Some(raw)), Seed::Refused(_)),
                    "{raw:?} was accepted"
                );
                seed_default_admin(&db, true, Some(raw));
            }
            assert_eq!(user_count(&db), 0);
        }

        /// The username is derived from the normalized address, so it inherits
        /// the lowercasing, and a character register would reject becomes '-'.
        #[test]
        fn the_email_half_becomes_the_email_and_the_username() {
            assert_eq!(
                decide(true, Some(" Dev.User@Example.test : Admin1234! ")),
                Seed::Admin {
                    username: "dev-user".to_string(),
                    email: "dev.user@example.test".to_string(),
                    password: "Admin1234!".to_string(),
                }
            );
        }

        /// A password may contain the separator: only the first ':' splits.
        #[test]
        fn only_the_first_colon_splits() {
            assert_eq!(
                decide(true, Some("admin@a8n.run:Pa:ss1234!")),
                Seed::Admin {
                    username: "admin".to_string(),
                    email: "admin@a8n.run".to_string(),
                    password: "Pa:ss1234!".to_string(),
                }
            );
        }
    }

    /// The saas leg refuses on a dev build too: `oidc::jit::load_or_provision`
    /// rewrites `is_admin` from the OP role claim on every login, so a seeded
    /// admin would be demoted by the first sign-in.
    #[cfg(feature = "saas")]
    #[test]
    fn the_saas_leg_refuses_even_on_a_dev_build() {
        assert!(matches!(decide(true, Some(CONFIGURED)), Seed::Refused(_)));

        let state = make_test_state();
        let db = state.db.lock().unwrap();
        seed_default_admin(&db, true, Some(CONFIGURED));
        assert_eq!(user_count(&db), 0, "the saas leg seeded a user");
        assert_eq!(admin_count(&db), 0);
    }
}
