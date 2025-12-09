pub mod abuse;
pub mod admin;
pub mod auth;
pub mod pages;
pub mod urls;

// Re-export handlers for easier importing
pub use abuse::{admin_list_reports, admin_resolve_report, submit_abuse_report};
pub use admin::{admin_delete_user, admin_get_stats, admin_list_users, admin_promote_user};
pub use auth::{get_current_user, login, refresh_token, register};
pub use pages::{
    admin_page, check_setup_required, dashboard_page, get_config, get_version, health_check, index,
    login_page, serve_auth_js, serve_css, setup_page, signup_page,
};
pub use urls::{
    delete_url, get_click_history, get_qr_code, get_stats, get_user_urls, redirect_url,
    shorten_url, update_url_name,
};
