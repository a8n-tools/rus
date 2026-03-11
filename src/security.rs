use chrono::{Duration, Utc};
use rusqlite::{params, Connection};

/// Validate password complexity requirements
pub fn validate_password(password: &str) -> Result<(), String> {
    if password.len() < 8 {
        return Err("Password must be at least 8 characters long".to_string());
    }

    if !password.chars().any(|c| c.is_uppercase()) {
        return Err("Password must contain at least one uppercase letter".to_string());
    }

    if !password.chars().any(|c| c.is_numeric()) {
        return Err("Password must contain at least one number".to_string());
    }

    if !password.chars().any(|c| !c.is_alphanumeric()) {
        return Err("Password must contain at least one special character".to_string());
    }

    Ok(())
}

/// Check if account is locked due to too many failed login attempts
pub fn is_account_locked(
    db: &Connection,
    username: &str,
    max_attempts: i32,
    lockout_minutes: i64,
) -> bool {
    let cutoff = Utc::now() - Duration::minutes(lockout_minutes);
    let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();

    let failed_attempts: i32 = db
        .query_row(
            "SELECT COUNT(*) FROM login_attempts WHERE username = ?1 AND success = 0 AND attempted_at > ?2",
            params![username, cutoff_str],
            |row| row.get(0),
        )
        .unwrap_or(0);

    failed_attempts >= max_attempts
}

/// Record a login attempt (success or failure) for tracking
pub fn record_login_attempt(db: &Connection, username: &str, success: bool) {
    let _ = db.execute(
        "INSERT INTO login_attempts (username, success) VALUES (?1, ?2)",
        params![username, success as i32],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE login_attempts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL,
                attempted_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                success INTEGER NOT NULL DEFAULT 0
            );",
        )
        .unwrap();
        conn
    }

    // --- validate_password ---

    #[test]
    fn valid_password_passes() {
        assert!(validate_password("SecurePass1!").is_ok());
    }

    #[test]
    fn password_too_short_rejected() {
        let err = validate_password("Ab1!").unwrap_err();
        assert!(err.contains("8 characters"), "unexpected: {err}");
    }

    #[test]
    fn password_no_uppercase_rejected() {
        let err = validate_password("password1!").unwrap_err();
        assert!(err.contains("uppercase"), "unexpected: {err}");
    }

    #[test]
    fn password_no_number_rejected() {
        let err = validate_password("Password!").unwrap_err();
        assert!(err.contains("number"), "unexpected: {err}");
    }

    #[test]
    fn password_no_special_char_rejected() {
        let err = validate_password("Password1").unwrap_err();
        assert!(err.contains("special"), "unexpected: {err}");
    }

    #[test]
    fn password_exactly_8_chars_passes() {
        assert!(validate_password("Secure1!").is_ok());
    }

    // --- is_account_locked / record_login_attempt ---

    #[test]
    fn account_not_locked_with_no_attempts() {
        let conn = setup_db();
        assert!(!is_account_locked(&conn, "alice", 5, 30));
    }

    #[test]
    fn account_not_locked_below_threshold() {
        let conn = setup_db();
        for _ in 0..4 {
            record_login_attempt(&conn, "alice", false);
        }
        assert!(!is_account_locked(&conn, "alice", 5, 30));
    }

    #[test]
    fn account_locked_at_threshold() {
        let conn = setup_db();
        for _ in 0..5 {
            record_login_attempt(&conn, "alice", false);
        }
        assert!(is_account_locked(&conn, "alice", 5, 30));
    }

    #[test]
    fn account_locked_above_threshold() {
        let conn = setup_db();
        for _ in 0..7 {
            record_login_attempt(&conn, "alice", false);
        }
        assert!(is_account_locked(&conn, "alice", 5, 30));
    }

    #[test]
    fn successful_logins_not_counted_toward_lockout() {
        let conn = setup_db();
        for _ in 0..4 {
            record_login_attempt(&conn, "alice", false);
        }
        record_login_attempt(&conn, "alice", true); // success — should not count
        assert!(!is_account_locked(&conn, "alice", 5, 30));
    }

    #[test]
    fn lockout_is_per_username() {
        let conn = setup_db();
        for _ in 0..5 {
            record_login_attempt(&conn, "eve", false);
        }
        // alice is not locked even though eve is
        assert!(!is_account_locked(&conn, "alice", 5, 30));
    }

    #[test]
    fn record_login_attempt_stores_success() {
        let conn = setup_db();
        record_login_attempt(&conn, "alice", true);
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM login_attempts WHERE username='alice' AND success=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn record_login_attempt_stores_failure() {
        let conn = setup_db();
        record_login_attempt(&conn, "alice", false);
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM login_attempts WHERE username='alice' AND success=0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}
