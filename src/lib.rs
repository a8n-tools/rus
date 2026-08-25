//! Rust URL Shortener library crate.
//!
//! This module re-exports the core components so that integration tests
//! (in `tests/`) can build test applications without duplicating module
//! declarations.

#[cfg(feature = "standalone")]
pub mod auth;
pub mod config;
pub mod db;
pub mod handlers;
pub mod location_alert;
pub mod login_approval;
pub mod mailer;
pub mod models;
#[cfg(feature = "saas")]
pub mod oidc;
pub mod routes;
#[cfg(feature = "standalone")]
pub mod security;
pub mod url;

// `test` covers the library's own unit tests; `testing` (set by the self
// dev-dependency) puts the same module in the library an integration target
// links, so both share one fixture (RUS-31).
#[cfg(any(test, feature = "testing"))]
pub mod testing;
