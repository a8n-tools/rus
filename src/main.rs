use actix_web::{middleware, web, App, HttpServer};
use tracing::info;
use tracing_subscriber::{fmt, EnvFilter};

// Import from the `rus` library instead of re-declaring its modules here: a
// second `mod` tree compiles and runs the whole unit suite twice (RUS-24).
use rus::config::Config;
use rus::db::AppState;
#[cfg(feature = "saas")]
use rus::handlers::maintenance_guard;
#[cfg(feature = "saas")]
use rus::oidc;
use rus::{location_alert, routes, setup_admin};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Load environment variables from .env file
    dotenvy::dotenv().ok();

    // Initialize structured logging
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,rus=debug")),
        )
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .init();

    // Load configuration from environment
    let config = Config::from_env();

    // Print startup banner with configuration
    config.print_banner();

    // RUS-12: install the trusted-proxy set before any request is served.
    location_alert::init_trusted_proxies(config.trusted_proxy_cidrs.clone());

    let bind_host = config.host.clone();
    let bind_port = config.port;

    // Initialize database connection
    let app_state = web::Data::new(AppState::new(config).expect(
        "Failed to connect to database. Check that DB_PATH is set to a valid, writable location.",
    ));

    info!("Database connection established");

    // DEV-300: the fleet-standard SETUP_DEFAULT_ADMIN bootstrap. A no-op unless
    // this is a debug build, which no shipped image is.
    setup_admin::ensure_default_admin(&app_state);

    // Build the OIDC verifier + RP state once and share across workers.
    #[cfg(feature = "saas")]
    let oidc_state = {
        use std::sync::Arc;
        let verifier = Arc::new(oidc::OidcVerifier::new(app_state.config.oidc.clone()));
        web::Data::new(oidc::OidcRpState::new(
            app_state.config.oidc.clone(),
            verifier,
        ))
    };

    info!(host = %bind_host, port = bind_port, "Starting server");

    HttpServer::new(move || {
        let app = App::new()
            .app_data(app_state.clone())
            .wrap(tracing_actix_web::TracingLogger::default())
            .wrap(
                middleware::DefaultHeaders::new()
                    .add(("X-Content-Type-Options", "nosniff"))
                    .add(("X-Frame-Options", "DENY"))
                    .add(("X-XSS-Protection", "1; mode=block"))
                    .add(("Referrer-Policy", "strict-origin-when-cross-origin")),
            );

        #[cfg(feature = "saas")]
        let app = app.app_data(oidc_state.clone());

        // RUS-21: the one route table, shared with the integration tests, so
        // no test can assert a routing property this binary does not have.
        let app = app.configure(routes::configure_app);

        // Maintenance guard: outermost middleware, registered last so it wraps
        // everything the table above mounted.
        #[cfg(feature = "saas")]
        let app = app.wrap(actix_web::middleware::from_fn(maintenance_guard));

        app
    })
    .bind((bind_host.as_str(), bind_port))?
    .run()
    .await
}
