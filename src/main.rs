use actix_web::{middleware, web, App, HttpServer};
use actix_web_httpauth::middleware::HttpAuthentication;

mod auth;
mod config;
mod db;
mod handlers;
mod models;
mod security;
mod url;

use auth::middleware::{admin_validator, jwt_validator};
use config::Config;
use db::AppState;
use handlers::*;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Load environment variables from .env file
    dotenvy::dotenv().ok();

    // Load configuration from environment
    let config = Config::from_env();

    // Print startup banner with configuration
    config.print_banner();

    let bind_host = config.host.clone();
    let bind_port = config.port;

    // Initialize database connection
    let app_state = web::Data::new(
        AppState::new(config)
            .expect("Failed to connect to database. Make sure the SQLite container is running and ./data/rus.db exists.")
    );

    println!("✓ Database connection established");
    println!("🚀 Starting server on {}:{}", bind_host, bind_port);

    HttpServer::new(move || {
        let auth = HttpAuthentication::bearer(jwt_validator);
        let admin_auth = HttpAuthentication::bearer(admin_validator);

        App::new()
            .app_data(app_state.clone())
            .wrap(middleware::Logger::default())
            // Public API routes - MUST BE BEFORE scoped /api routes
            .route("/api/register", web::post().to(register))
            .route("/api/login", web::post().to(login))
            .route("/api/refresh", web::post().to(refresh_token))
            .route("/api/config", web::get().to(get_config))
            .route("/api/version", web::get().to(get_version))
            .route("/api/setup/required", web::get().to(check_setup_required))
            .route("/api/report-abuse", web::post().to(submit_abuse_report))
            // Admin-only routes - MUST BE BEFORE /api scope
            .service(
                web::scope("/api/admin")
                    .wrap(admin_auth)
                    .route("/users", web::get().to(admin_list_users))
                    .route("/users/{user_id}", web::delete().to(admin_delete_user))
                    .route("/users/{user_id}/promote", web::post().to(admin_promote_user))
                    .route("/stats", web::get().to(admin_get_stats))
                    .route("/reports", web::get().to(admin_list_reports))
                    .route("/reports/{report_id}", web::post().to(admin_resolve_report))
            )
            // Protected routes (require authentication)
            .service(
                web::scope("/api")
                    .wrap(auth)
                    .route("/me", web::get().to(get_current_user))
                    .route("/shorten", web::post().to(shorten_url))
                    .route("/stats/{code}", web::get().to(get_stats))
                    .route("/urls", web::get().to(get_user_urls))
                    .route("/urls/{code}", web::delete().to(delete_url))
                    .route("/urls/{code}/name", web::patch().to(update_url_name))
                    .route("/urls/{code}/clicks", web::get().to(get_click_history))
                    .route("/urls/{code}/qr/{format}", web::get().to(get_qr_code))
            )
            // Public page routes
            .route("/", web::get().to(index))
            .route("/login.html", web::get().to(login_page))
            .route("/signup.html", web::get().to(signup_page))
            .route("/dashboard.html", web::get().to(dashboard_page))
            .route("/setup.html", web::get().to(setup_page))
            .route("/admin.html", web::get().to(admin_page))
            .route("/styles.css", web::get().to(serve_css))
            .route("/auth.js", web::get().to(serve_auth_js))
            .route("/health", web::get().to(health_check))
            // Catch-all route for short code redirects (MUST BE LAST)
            .route("/{code}", web::get().to(redirect_url))
    })
    .bind((bind_host.as_str(), bind_port))?
    .run()
    .await
}
