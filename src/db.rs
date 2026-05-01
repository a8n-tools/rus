use chrono::{Duration, Utc};
use rusqlite::{params, Connection};
use std::sync::Mutex;
#[cfg(feature = "saas")]
use std::sync::atomic::AtomicBool;
#[cfg(feature = "saas")]
use std::sync::RwLock;

use crate::config::Config;

/// Application state containing database connection and configuration
pub struct AppState {
    pub db: Mutex<Connection>,
    pub config: Config,
    pub start_time: std::time::Instant,
    #[cfg(feature = "saas")]
    pub maintenance_mode: AtomicBool,
    #[cfg(feature = "saas")]
    pub maintenance_message: RwLock<Option<String>>,
}

impl AppState {
    /// Create new application state with database connection
    pub fn new(config: Config) -> rusqlite::Result<Self> {
        // Create data directory if it doesn't exist
        if let Some(parent) = std::path::Path::new(&config.db_path).parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let conn = Connection::open(&config.db_path)?;

        // Enable foreign key enforcement (SQLite has this off by default)
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;

        // Initialize database schema based on feature
        #[cfg(feature = "standalone")]
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS users (
                userID INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL UNIQUE,
                password TEXT NOT NULL,
                is_admin INTEGER NOT NULL DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS urls (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                original_url TEXT NOT NULL,
                short_code TEXT NOT NULL UNIQUE,
                name TEXT,
                clicks INTEGER DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (user_id) REFERENCES users(userID) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS click_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url_id INTEGER NOT NULL,
                clicked_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (url_id) REFERENCES urls(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS refresh_tokens (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                token TEXT NOT NULL UNIQUE,
                expires_at DATETIME NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (user_id) REFERENCES users(userID) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS login_attempts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL,
                attempted_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                success INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS abuse_reports (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                short_code TEXT NOT NULL,
                reporter_email TEXT,
                reason TEXT NOT NULL,
                description TEXT,
                status TEXT NOT NULL DEFAULT 'pending',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                resolved_at DATETIME,
                resolved_by INTEGER,
                FOREIGN KEY (resolved_by) REFERENCES users(userID) ON DELETE SET NULL
            );

            CREATE INDEX IF NOT EXISTS idx_short_code ON urls(short_code);
            CREATE INDEX IF NOT EXISTS idx_user_id ON urls(user_id);
            CREATE INDEX IF NOT EXISTS idx_click_history_url_id ON click_history(url_id);
            CREATE INDEX IF NOT EXISTS idx_click_history_clicked_at ON click_history(clicked_at);
            CREATE INDEX IF NOT EXISTS idx_refresh_tokens_token ON refresh_tokens(token);
            CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_id ON refresh_tokens(user_id);
            CREATE INDEX IF NOT EXISTS idx_login_attempts_username ON login_attempts(username);
            CREATE INDEX IF NOT EXISTS idx_login_attempts_attempted_at ON login_attempts(attempted_at);
            CREATE INDEX IF NOT EXISTS idx_abuse_reports_short_code ON abuse_reports(short_code);
            CREATE INDEX IF NOT EXISTS idx_abuse_reports_status ON abuse_reports(status);
            CREATE INDEX IF NOT EXISTS idx_abuse_reports_created_at ON abuse_reports(created_at);
            "
        )?;

        // SaaS mode: simplified schema without user management tables.
        // The `users` table here uses an AUTOINCREMENT PK, distinct from the SaaS
        // identity (`saas_user_id` UUID) which is stored alongside.
        #[cfg(feature = "saas")]
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS users (
                userID INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL UNIQUE,
                password TEXT NOT NULL DEFAULT '',
                is_admin INTEGER NOT NULL DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                saas_user_id TEXT,
                email TEXT,
                suspended_at TEXT,
                session_version INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS urls (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                original_url TEXT NOT NULL,
                short_code TEXT NOT NULL UNIQUE,
                name TEXT,
                clicks INTEGER DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (user_id) REFERENCES users(userID) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS click_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url_id INTEGER NOT NULL,
                clicked_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (url_id) REFERENCES urls(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS abuse_reports (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                short_code TEXT NOT NULL,
                reporter_email TEXT,
                reason TEXT NOT NULL,
                description TEXT,
                status TEXT NOT NULL DEFAULT 'pending',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                resolved_at DATETIME,
                resolved_by INTEGER
            );

            CREATE TABLE IF NOT EXISTS rp_sessions (
                id            TEXT PRIMARY KEY,
                state         TEXT NOT NULL UNIQUE,
                nonce         TEXT NOT NULL,
                code_verifier TEXT NOT NULL,
                return_to     TEXT,
                created_at    TEXT NOT NULL,
                expires_at    TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS user_sessions (
                id                 TEXT PRIMARY KEY,
                session_token_hash BLOB NOT NULL UNIQUE,
                user_id            INTEGER NOT NULL REFERENCES users(userID) ON DELETE CASCADE,
                session_version    INTEGER NOT NULL,
                auth_via_oidc      INTEGER NOT NULL DEFAULT 0,
                created_at         TEXT NOT NULL,
                expires_at         TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_short_code ON urls(short_code);
            CREATE INDEX IF NOT EXISTS idx_user_id ON urls(user_id);
            CREATE INDEX IF NOT EXISTS idx_click_history_url_id ON click_history(url_id);
            CREATE INDEX IF NOT EXISTS idx_click_history_clicked_at ON click_history(clicked_at);
            CREATE INDEX IF NOT EXISTS idx_abuse_reports_short_code ON abuse_reports(short_code);
            CREATE INDEX IF NOT EXISTS idx_abuse_reports_status ON abuse_reports(status);
            CREATE INDEX IF NOT EXISTS idx_abuse_reports_created_at ON abuse_reports(created_at);
            CREATE INDEX IF NOT EXISTS idx_user_sessions_user_id ON user_sessions(user_id);
            CREATE INDEX IF NOT EXISTS idx_user_sessions_expires ON user_sessions(expires_at);
            CREATE INDEX IF NOT EXISTS idx_rp_sessions_expires ON rp_sessions(expires_at);
            "
        )?;

        // SaaS mode: best-effort migration to add SSO columns to a pre-existing
        // users table (silently ignore "duplicate column name" errors).
        #[cfg(feature = "saas")]
        {
            for stmt in [
                "ALTER TABLE users ADD COLUMN saas_user_id TEXT",
                "ALTER TABLE users ADD COLUMN email TEXT",
                "ALTER TABLE users ADD COLUMN suspended_at TEXT",
                "ALTER TABLE users ADD COLUMN session_version INTEGER NOT NULL DEFAULT 0",
            ] {
                if let Err(e) = conn.execute(stmt, []) {
                    let msg = e.to_string();
                    if !msg.contains("duplicate column name") {
                        tracing::debug!(stmt = %stmt, error = %msg, "SSO column migration skipped");
                    }
                }
            }
            // Index requires saas_user_id; create it after the migration block so a
            // pre-existing users table (added the column above) doesn't trip CREATE INDEX.
            conn.execute_batch(
                "CREATE UNIQUE INDEX IF NOT EXISTS idx_users_saas_user_id
                     ON users(saas_user_id) WHERE saas_user_id IS NOT NULL;"
            )?;
        }

        Ok(AppState {
            db: Mutex::new(conn),
            config,
            start_time: std::time::Instant::now(),
            #[cfg(feature = "saas")]
            maintenance_mode: AtomicBool::new(false),
            #[cfg(feature = "saas")]
            maintenance_message: RwLock::new(None),
        })
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appstate_new_creates_tables() {
        let cfg = crate::testing::test_config();
        let state = AppState::new(cfg).expect("AppState::new should succeed with :memory:");
        let db = state.db.lock().unwrap();

        // Verify core tables exist
        let tables: Vec<String> = {
            let mut stmt = db
                .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .unwrap();
            stmt.query_map([], |row| row.get(0))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect()
        };

        assert!(tables.contains(&"users".to_string()));
        assert!(tables.contains(&"urls".to_string()));
        assert!(tables.contains(&"click_history".to_string()));
        assert!(tables.contains(&"abuse_reports".to_string()));
    }

    #[cfg(feature = "standalone")]
    #[test]
    fn appstate_standalone_has_extra_tables() {
        let cfg = crate::testing::test_config();
        let state = AppState::new(cfg).unwrap();
        let db = state.db.lock().unwrap();

        let tables: Vec<String> = {
            let mut stmt = db
                .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .unwrap();
            stmt.query_map([], |row| row.get(0))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect()
        };

        assert!(tables.contains(&"refresh_tokens".to_string()));
        assert!(tables.contains(&"login_attempts".to_string()));
    }

    #[test]
    fn appstate_indexes_created() {
        let cfg = crate::testing::test_config();
        let state = AppState::new(cfg).unwrap();
        let db = state.db.lock().unwrap();

        let indexes: Vec<String> = {
            let mut stmt = db
                .prepare("SELECT name FROM sqlite_master WHERE type='index' AND name LIKE 'idx_%'")
                .unwrap();
            stmt.query_map([], |row| row.get(0))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect()
        };

        assert!(indexes.contains(&"idx_short_code".to_string()));
        assert!(indexes.contains(&"idx_user_id".to_string()));
        assert!(indexes.contains(&"idx_click_history_url_id".to_string()));
        assert!(indexes.contains(&"idx_click_history_clicked_at".to_string()));
    }

    #[test]
    fn appstate_foreign_keys_enabled() {
        let cfg = crate::testing::test_config();
        let state = AppState::new(cfg).unwrap();
        let db = state.db.lock().unwrap();

        let fk: i32 = db
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1);
    }

    #[test]
    fn appstate_new_is_idempotent() {
        // Calling new twice with :memory: should both succeed (CREATE IF NOT EXISTS)
        let cfg1 = crate::testing::test_config();
        let _s1 = AppState::new(cfg1).unwrap();
        let cfg2 = crate::testing::test_config();
        let _s2 = AppState::new(cfg2).unwrap();
    }

    #[test]
    fn cleanup_old_clicks_removes_expired_entries() {
        let state = crate::testing::make_test_state();
        let db = state.db.lock().unwrap();

        // Insert a user and URL
        db.execute(
            "INSERT INTO users (username, password) VALUES ('testuser', 'pass')",
            [],
        )
        .unwrap();
        let user_id = db.last_insert_rowid();
        db.execute(
            "INSERT INTO urls (user_id, original_url, short_code) VALUES (?1, 'https://example.com', 'abc123')",
            params![user_id],
        )
        .unwrap();
        let url_id = db.last_insert_rowid();

        // Insert an old click (60 days ago)
        let old_time = (Utc::now() - Duration::days(60))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        db.execute(
            "INSERT INTO click_history (url_id, clicked_at) VALUES (?1, ?2)",
            params![url_id, old_time],
        )
        .unwrap();

        // Insert a recent click
        let recent_time = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        db.execute(
            "INSERT INTO click_history (url_id, clicked_at) VALUES (?1, ?2)",
            params![url_id, recent_time],
        )
        .unwrap();

        let count_before: i64 = db
            .query_row("SELECT COUNT(*) FROM click_history", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count_before, 2);

        // Cleanup with 30-day retention
        cleanup_old_clicks(&db, 30);

        let count_after: i64 = db
            .query_row("SELECT COUNT(*) FROM click_history", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count_after, 1);
    }

    #[test]
    fn cleanup_old_clicks_keeps_all_when_within_retention() {
        let state = crate::testing::make_test_state();
        let db = state.db.lock().unwrap();

        db.execute(
            "INSERT INTO users (username, password) VALUES ('testuser', 'pass')",
            [],
        )
        .unwrap();
        let user_id = db.last_insert_rowid();
        db.execute(
            "INSERT INTO urls (user_id, original_url, short_code) VALUES (?1, 'https://example.com', 'abc123')",
            params![user_id],
        )
        .unwrap();
        let url_id = db.last_insert_rowid();

        // Insert clicks within retention
        let recent_time = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        db.execute(
            "INSERT INTO click_history (url_id, clicked_at) VALUES (?1, ?2)",
            params![url_id, recent_time],
        )
        .unwrap();

        cleanup_old_clicks(&db, 30);

        let count: i64 = db
            .query_row("SELECT COUNT(*) FROM click_history", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn cleanup_old_clicks_noop_on_empty_table() {
        let state = crate::testing::make_test_state();
        let db = state.db.lock().unwrap();
        cleanup_old_clicks(&db, 30); // should not panic
    }

    #[test]
    fn start_time_is_recent() {
        let state = crate::testing::make_test_state();
        assert!(state.start_time.elapsed().as_secs() < 5);
    }

    #[cfg(feature = "saas")]
    #[test]
    fn saas_maintenance_mode_defaults_to_false() {
        let state = crate::testing::make_test_state();
        assert!(!state.maintenance_mode.load(std::sync::atomic::Ordering::SeqCst));
        assert!(state.maintenance_message.read().unwrap().is_none());
    }
}
