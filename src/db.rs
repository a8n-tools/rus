use rusqlite::Connection;
use std::sync::Mutex;

use crate::config::Config;

/// Application state containing database connection and configuration
pub struct AppState {
    pub db: Mutex<Connection>,
    pub config: Config,
    pub start_time: std::time::Instant,
}

impl AppState {
    /// Create new application state with database connection
    pub fn new(config: Config) -> rusqlite::Result<Self> {
        // Create data directory if it doesn't exist
        if let Some(parent) = std::path::Path::new(&config.db_path).parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let conn = Connection::open(&config.db_path)?;

        // Initialize database schema
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

        Ok(AppState {
            db: Mutex::new(conn),
            config,
            start_time: std::time::Instant::now(),
        })
    }
}
