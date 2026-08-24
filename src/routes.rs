//! RUS-21: the application's route table, defined once.
//!
//! `src/main.rs` and `tests/integration_standalone.rs` used to declare the
//! table separately, so the test app could assert a routing property the real
//! binary did not have. That is how auto-buyer's AB-67 shipped its approval
//! route behind the auth middleware with a green suite. Both callers now go
//! through [`configure_app`], which makes the drift structurally impossible
//! rather than merely detectable.
//!
//! Order is the load-bearing part: every public row is registered before the
//! guarded `/api` scope, and the `/{code}` catch-all is registered last
//! because it matches any single-segment path. The rows both feature legs
//! share are written once, so a change to one cannot land in one leg and miss
//! the other; only the rows that genuinely differ carry a `#[cfg]`.

use actix_governor::{Governor, GovernorConfigBuilder};
use actix_web::web;
#[cfg(feature = "standalone")]
use actix_web_httpauth::middleware::HttpAuthentication;

#[cfg(feature = "standalone")]
use crate::auth::middleware::{admin_validator, jwt_validator};
use crate::handlers::*;
use crate::login_approval;
#[cfg(feature = "saas")]
use crate::oidc;

/// Register every route the application serves, in mount order.
///
/// The caller supplies `app_data` and the outermost middleware (the saas
/// `maintenance_guard` wraps the whole `App`, so it cannot live here).
pub fn configure_app(cfg: &mut web::ServiceConfig) {
    // Strict for the auth endpoints (5 requests per minute), shared by
    // register and login, so one peer's bucket covers both.
    #[cfg(feature = "standalone")]
    let strict_rate_limit = GovernorConfigBuilder::default()
        .seconds_per_request(12)
        .burst_size(5)
        .finish()
        .expect("strict rate limit config is valid");

    // Moderate for the public endpoints (30 requests per minute).
    let moderate_rate_limit = GovernorConfigBuilder::default()
        .seconds_per_request(2)
        .burst_size(30)
        .finish()
        .expect("moderate rate limit config is valid");

    // ── Public rows, all of them ahead of the guarded /api scope ─────────────

    #[cfg(feature = "saas")]
    cfg.route(
        "/webhooks/maintenance",
        web::post().to(handle_maintenance_webhook),
    );

    #[cfg(feature = "standalone")]
    cfg.service(
        web::resource("/api/register")
            .wrap(Governor::new(&strict_rate_limit))
            .route(web::post().to(register)),
    )
    .service(
        web::resource("/api/login")
            .wrap(Governor::new(&strict_rate_limit))
            .route(web::post().to(login)),
    )
    .route("/api/refresh", web::post().to(refresh_token))
    .route("/api/setup/required", web::get().to(check_setup_required));

    cfg.route("/api/config", web::get().to(get_config))
        .route("/api/version", web::get().to(get_version));

    // RUS-19: the approval page and its API, mounted here on purpose. Whoever
    // follows the emailed link has no session yet, so these must sit above the
    // guarded /api scope and above the short-code catch-all.
    login_approval::configure_routes(cfg);

    cfg.service(
        web::resource("/api/report-abuse")
            .wrap(Governor::new(&moderate_rate_limit))
            .route(web::post().to(submit_abuse_report)),
    );

    // Admin-only routes, ahead of the /api scope so it cannot answer first.
    #[cfg(feature = "standalone")]
    cfg.service(
        web::scope("/api/admin")
            .wrap(HttpAuthentication::bearer(admin_validator))
            .route("/users", web::get().to(admin_list_users))
            .route("/users/{user_id}", web::delete().to(admin_delete_user))
            .route(
                "/users/{user_id}/promote",
                web::post().to(admin_promote_user),
            )
            .route("/stats", web::get().to(admin_get_stats))
            .route("/reports", web::get().to(admin_list_reports))
            .route("/reports/{report_id}", web::post().to(admin_resolve_report)),
    );

    // ── The guarded /api scope ───────────────────────────────────────────────
    // Its rows are the same in both legs; the guard and the two /me handlers
    // are not, so only those carry a cfg.

    let guarded = web::scope("/api");
    #[cfg(feature = "standalone")]
    let guarded = guarded.wrap(HttpAuthentication::bearer(jwt_validator));
    #[cfg(feature = "saas")]
    let guarded = guarded.wrap(actix_web::middleware::from_fn(oidc::require_session));
    #[cfg(feature = "standalone")]
    let guarded = guarded
        .route("/me", web::get().to(get_current_user))
        .route("/me", web::patch().to(update_current_user));
    #[cfg(feature = "saas")]
    let guarded = guarded
        .route("/me", web::get().to(saas_me))
        .route("/me", web::patch().to(saas_update_me));

    cfg.service(
        guarded
            .route("/shorten", web::post().to(shorten_url))
            .route("/stats/{code}", web::get().to(get_stats))
            .route("/urls", web::get().to(get_user_urls))
            .route("/urls/{code}", web::delete().to(delete_url))
            .route("/urls/{code}/name", web::patch().to(update_url_name))
            .route("/urls/{code}/clicks", web::get().to(get_click_history))
            .route("/urls/{code}/qr/{format}", web::get().to(get_qr_code)),
    );

    // ── OIDC relying party (saas) ────────────────────────────────────────────

    #[cfg(feature = "saas")]
    {
        cfg.route("/oauth2/login", web::get().to(oidc::rp::login))
            .route("/oauth2/callback", web::get().to(oidc::rp::callback))
            .route("/oauth2/logout", web::get().to(oidc::rp::logout))
            .route(
                "/oauth2/backchannel-logout",
                web::post().to(oidc::rp::backchannel_logout),
            )
            .route(
                "/oauth2/lifecycle-event",
                web::post().to(oidc::rp::lifecycle_event),
            );

        // Dev-only seed-session for local testing.
        #[cfg(debug_assertions)]
        cfg.route(
            "/dev/seed-session",
            web::get().to(oidc::rp::dev_seed_session),
        )
        .route("/dev/logout", web::get().to(oidc::rp::dev_logout));
    }

    // ── Pages and assets ─────────────────────────────────────────────────────

    cfg.route("/", web::get().to(index))
        .route("/dashboard.html", web::get().to(dashboard_page))
        .route("/report.html", web::get().to(report_page));

    #[cfg(feature = "standalone")]
    cfg.route("/login.html", web::get().to(login_page))
        .route("/signup.html", web::get().to(signup_page))
        .route("/setup.html", web::get().to(setup_page))
        .route("/admin.html", web::get().to(admin_page));

    cfg.route("/styles.css", web::get().to(serve_css))
        .route("/k9f3x2m7.js", web::get().to(serve_auth_js))
        .route("/theme.js", web::get().to(serve_theme_js))
        .route("/health", web::get().to(health_check))
        // Catch-all for short-code redirects. MUST BE LAST: it matches any
        // single-segment path, so anything below it becomes unreachable.
        .route("/{code}", web::get().to(redirect_url));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::make_test_state;
    // Imported under another name: `use actix_web::test` would shadow the
    // built-in `#[test]` attribute for the synchronous test below.
    use actix_web::{test as http_test, App};

    /// The rate-limited rows use the peer-IP key extractor, which answers 500
    /// when a `TestRequest` carries no peer address.
    const PEER: &str = "127.0.0.1:34567";

    /// The real `main.rs`, read at compile time so the assertion below checks
    /// the file the binary is built from and not a copy of it.
    const MAIN_RS: &str = include_str!("main.rs");

    /// Every row inside the guarded `/api` scope, one path per row. Requested
    /// with GET because the scope's guard runs before its inner routing, so
    /// the method does not change the answer.
    const GUARDED_PATHS: [&str; 8] = [
        "/api/me",
        "/api/shorten",
        "/api/stats/abc",
        "/api/urls",
        "/api/urls/abc",
        "/api/urls/abc/name",
        "/api/urls/abc/clicks",
        "/api/urls/abc/qr/png",
    ];

    macro_rules! real_app {
        () => {
            http_test::init_service(
                App::new()
                    .app_data(make_test_state())
                    .configure(configure_app),
            )
            .await
        };
    }

    async fn status(
        app: &impl actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse,
            Error = actix_web::Error,
        >,
        path: &str,
    ) -> actix_web::http::StatusCode {
        let req = http_test::TestRequest::get()
            .uri(path)
            .peer_addr(PEER.parse().unwrap())
            .to_request();
        http_test::call_service(app, req).await.status()
    }

    /// The control the reachability assertions below are measured against: the
    /// guard on this scope is real, and it covers every row in it.
    #[actix_web::test]
    async fn every_guarded_row_needs_credentials() {
        let app = real_app!();
        for path in GUARDED_PATHS {
            assert_eq!(
                status(&app, path).await,
                401,
                "{path} answered without credentials"
            );
        }
    }

    /// AB-67's defect, asserted against the table the binary mounts: behind
    /// the guard these would 401, and below the catch-all they would 404.
    #[actix_web::test]
    async fn the_approval_routes_are_reachable_with_no_session() {
        let app = real_app!();
        assert_eq!(
            status(&app, login_approval::APPROVAL_PAGE_PATH).await,
            200,
            "the emailed link must not need a session, nor be swallowed by /{{code}}"
        );
        // An unknown token is a 404 from the handler, not a 401 from a guard.
        let uri = format!("{}?token=nosuchtoken", login_approval::APPROVAL_API_PATH);
        assert_eq!(status(&app, &uri).await, 404);
    }

    /// The public `/api` rows sit above the guarded scope, so the guard must
    /// not answer them first.
    #[actix_web::test]
    async fn the_public_api_rows_are_not_swallowed_by_the_guard() {
        let app = real_app!();
        for path in ["/api/config", "/api/version"] {
            assert_eq!(status(&app, path).await, 200, "{path} hit the guard");
        }
    }

    /// The catch-all matches any single-segment path, so a page registered
    /// after it would answer 404 from `redirect_url` instead of itself. Only
    /// paths both legs answer 200 are listed: saas redirects `/dashboard.html`
    /// to the OP when there is no session, which would prove nothing here.
    #[actix_web::test]
    async fn the_catch_all_is_mounted_below_the_pages() {
        let app = real_app!();
        for path in ["/", "/health", "/report.html", "/styles.css", "/theme.js"] {
            assert_eq!(status(&app, path).await, 200, "{path} hit /{{code}}");
        }
        assert_eq!(status(&app, "/nosuchcode").await, 404);
    }

    /// The binary is a `test = false` target (RUS-24, RUS-27), so nothing can
    /// exercise its wiring; this is the one thing left to assert about it.
    /// RUS-19 checked the mount ORDER inside `main.rs`, which is redundant now
    /// that the order lives here and the tests above run against it in both
    /// legs. What is not redundant is that `main.rs` still goes through this
    /// table instead of declaring a second one.
    #[test]
    fn main_builds_its_table_from_this_module() {
        assert!(
            MAIN_RS.contains(".configure(routes::configure_app)"),
            "main.rs must mount the shared table"
        );
        for declaration in [".route(", "web::scope(", "web::resource("] {
            assert!(
                !MAIN_RS.contains(declaration),
                "main.rs declares `{declaration}` of its own, which can drift from this table"
            );
        }
    }
}
