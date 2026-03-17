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
pub mod models;
#[cfg(feature = "standalone")]
pub mod security;
pub mod url;

#[cfg(test)]
pub mod testing;
