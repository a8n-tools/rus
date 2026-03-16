pub mod abuse;
#[cfg(feature = "standalone")]
pub mod admin;
#[cfg(feature = "standalone")]
pub mod auth;
pub mod pages;
#[cfg(feature = "saas")]
pub mod saas_auth;
#[cfg(feature = "saas")]
pub mod webhook;
#[cfg(feature = "saas")]
pub use saas_auth::{maintenance_guard, saas_cookie_validator, saas_me};
#[cfg(feature = "saas")]
pub use webhook::handle_maintenance_webhook;
pub mod urls;

// Re-export handlers for easier importing
#[cfg(feature = "standalone")]
pub use abuse::{admin_list_reports, admin_resolve_report};
pub use abuse::submit_abuse_report;
#[cfg(feature = "standalone")]
pub use admin::{admin_delete_user, admin_get_stats, admin_list_users, admin_promote_user};
#[cfg(feature = "standalone")]
pub use auth::{get_current_user, login, refresh_token, register};
#[cfg(feature = "standalone")]
pub use pages::{
    admin_page, check_setup_required, login_page, setup_page, signup_page,
};
pub use pages::{dashboard_page, get_config, get_version, health_check, index, report_page, serve_auth_js, serve_css};
#[cfg(feature = "saas")]
pub use pages::serve_saas_refresh_js;
pub use urls::{
    delete_url, get_click_history, get_qr_code, get_stats, get_user_urls, redirect_url,
    shorten_url, update_url_name,
};
