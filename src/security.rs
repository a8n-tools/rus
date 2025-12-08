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

/// Cleanup old click history records
pub fn cleanup_old_clicks(db: &Connection, retention_days: i64) {
    let cutoff = Utc::now() - Duration::days(retention_days);
    let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();

    let _ = db.execute(
        "DELETE FROM click_history WHERE clicked_at < ?1",
        params![cutoff_str],
    );
}
