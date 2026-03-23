use actix_web::{web, HttpRequest, HttpResponse, Result};
use rand::Rng;
use rusqlite::params;
use tracing::{debug, error, info};

#[cfg(feature = "standalone")]
use crate::auth::get_claims;
#[cfg(feature = "saas")]
use actix_web::HttpMessage;
#[cfg(feature = "saas")]
use super::saas_auth::SaasUserClaims;
use crate::db::AppState;
use crate::models::{
    ClickHistoryEntry, ClickStats, ShortenRequest, ShortenResponse, UpdateUrlNameRequest,
    UrlEntry,
};
use crate::url::{generate_qr_code_png, generate_qr_code_svg, generate_short_code, validate_url};

/// Helper to get user_id from request based on mode
#[cfg(feature = "standalone")]
fn get_user_id(http_req: &HttpRequest) -> Option<i64> {
    get_claims(http_req).map(|c| c.user_id)
}

#[cfg(feature = "saas")]
fn get_user_id(http_req: &HttpRequest) -> Option<i64> {
    // In SaaS mode with middleware, claims are stored in request extensions
    http_req
        .extensions()
        .get::<SaasUserClaims>()
        .map(|c| c.user_id)
}

/// Protected API endpoint to shorten a URL
pub async fn shorten_url(
    data: web::Data<AppState>,
    req_payload: web::Json<ShortenRequest>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    // Get user_id from JWT (standalone) or cookie (SaaS)
    let user_id = match get_user_id(&http_req) {
        Some(id) => id,
        None => {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized"
            })));
        }
    };

    // Validate URL
    if req_payload.url.is_empty() {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "URL cannot be empty"
        })));
    }

    // Comprehensive URL validation
    if let Err(error_message) = validate_url(&req_payload.url, data.config.max_url_length) {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": error_message
        })));
    }

    let db = data.db.lock().unwrap_or_else(|e| e.into_inner());

    // Check if URL is already shortened by this user
    let mut stmt = db
        .prepare("SELECT short_code FROM urls WHERE user_id = ?1 AND original_url = ?2")
        .map_err(|e| {
            error!(error = %e, "shorten_url: DB prepare failed");
            actix_web::error::ErrorInternalServerError("Database error")
        })?;

    if let Ok(short_code) = stmt.query_row(
        params![user_id, &req_payload.url],
        |row| row.get::<_, String>(0),
    ) {
        return Ok(HttpResponse::Ok().json(ShortenResponse {
            short_code: short_code.clone(),
            short_url: format!("{}/{}", data.config.host_url, short_code),
            original_url: req_payload.url.clone(),
        }));
    }

    // Generate a unique short code
    let mut short_code = generate_short_code();
    loop {
        let exists: bool = db
            .query_row(
                "SELECT COUNT(*) FROM urls WHERE short_code = ?1",
                params![&short_code],
                |row| row.get(0),
            )
            .map(|count: i64| count > 0)
            .unwrap_or(false);

        if !exists {
            break;
        }
        short_code = generate_short_code();
    }

    // Insert URL into database
    match db.execute(
        "INSERT INTO urls (user_id, original_url, short_code) VALUES (?1, ?2, ?3)",
        params![user_id, &req_payload.url, &short_code],
    ) {
        Ok(_) => {
            info!(user_id, short_code = %short_code, "URL shortened");
            Ok(HttpResponse::Ok().json(ShortenResponse {
                short_code: short_code.clone(),
                short_url: format!("{}/{}", data.config.host_url, short_code),
                original_url: req_payload.url.clone(),
            }))
        }
        Err(e) => {
            error!(user_id, error = %e, "Failed to insert shortened URL");
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to create short URL"
            })))
        }
    }
}

/// Public endpoint to redirect to the original URL
pub async fn redirect_url(
    data: web::Data<AppState>,
    code: web::Path<String>,
) -> Result<HttpResponse> {
    let db = data.db.lock().unwrap_or_else(|e| e.into_inner());

    // Get URL ID and original URL
    let result: rusqlite::Result<(i64, String)> = db.query_row(
        "SELECT id, original_url FROM urls WHERE short_code = ?1",
        params![code.as_str()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    );

    match result {
        Ok((url_id, original_url)) => {
            // Increment click count (legacy counter)
            let _ = db.execute(
                "UPDATE urls SET clicks = clicks + 1 WHERE id = ?1",
                params![url_id],
            );

            // Record click in history
            let _ = db.execute(
                "INSERT INTO click_history (url_id) VALUES (?1)",
                params![url_id],
            );

            // Cleanup old clicks periodically (1% chance)
            if rand::thread_rng().gen_range(0..100) == 0 {
                crate::db::cleanup_old_clicks(&db, data.config.click_retention_days);
            }

            debug!(short_code = %code.as_str(), "Redirect");
            Ok(HttpResponse::Found()
                .append_header(("Location", original_url))
                .finish())
        }
        Err(_) => {
            debug!(short_code = %code.as_str(), "Redirect failed: code not found");
            let html = include_str!("../../static/404.html");
            Ok(HttpResponse::NotFound()
                .content_type("text/html; charset=utf-8")
                .body(html))
        }
    }
}

/// Protected API endpoint to get URL statistics
pub async fn get_stats(
    data: web::Data<AppState>,
    code: web::Path<String>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    // Get user_id from JWT (standalone) or cookie (SaaS)
    let user_id = match get_user_id(&http_req) {
        Some(id) => id,
        None => {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized"
            })));
        }
    };

    let db = data.db.lock().unwrap_or_else(|e| e.into_inner());

    // Get URL entry for this user
    let result: rusqlite::Result<UrlEntry> = db.query_row(
        "SELECT original_url, short_code, name, clicks FROM urls WHERE short_code = ?1 AND user_id = ?2",
        params![code.as_str(), user_id],
        |row| {
            Ok(UrlEntry {
                original_url: row.get(0)?,
                short_code: row.get(1)?,
                name: row.get(2)?,
                clicks: row.get(3)?,
            })
        },
    );

    match result {
        Ok(entry) => Ok(HttpResponse::Ok().json(entry)),
        Err(_) => Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "Short URL not found or not owned by you"
        }))),
    }
}

/// Protected endpoint to get all URLs for the current user
pub async fn get_user_urls(
    data: web::Data<AppState>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    let user_id = match get_user_id(&http_req) {
        Some(id) => id,
        None => {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized"
            })));
        }
    };

    let db = data.db.lock().unwrap_or_else(|e| e.into_inner());

    let mut stmt = db
        .prepare(
            "SELECT original_url, short_code, name, clicks FROM urls WHERE user_id = ?1 ORDER BY created_at DESC",
        )
        .map_err(|_| actix_web::error::ErrorInternalServerError("Database error"))?;

    let urls: Vec<UrlEntry> = stmt
        .query_map(params![user_id], |row| {
            Ok(UrlEntry {
                original_url: row.get(0)?,
                short_code: row.get(1)?,
                name: row.get(2)?,
                clicks: row.get(3)?,
            })
        })
        .map_err(|_| actix_web::error::ErrorInternalServerError("Database error"))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(HttpResponse::Ok().json(urls))
}

/// Protected endpoint to delete a URL
pub async fn delete_url(
    data: web::Data<AppState>,
    code: web::Path<String>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    let user_id = match get_user_id(&http_req) {
        Some(id) => id,
        None => {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized"
            })));
        }
    };

    let db = data.db.lock().unwrap_or_else(|e| e.into_inner());

    // Delete the URL only if it belongs to the current user
    match db.execute(
        "DELETE FROM urls WHERE short_code = ?1 AND user_id = ?2",
        params![code.as_str(), user_id],
    ) {
        Ok(rows_affected) => {
            if rows_affected > 0 {
                info!(user_id, short_code = %code.as_str(), "URL deleted");
                Ok(HttpResponse::Ok().json(serde_json::json!({
                    "message": "URL deleted successfully"
                })))
            } else {
                Ok(HttpResponse::NotFound().json(serde_json::json!({
                    "error": "Short URL not found or not owned by you"
                })))
            }
        }
        Err(e) => {
            error!(user_id, short_code = %code.as_str(), error = %e, "Failed to delete URL");
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to delete URL"
            })))
        }
    }
}

/// Protected endpoint to update URL name
pub async fn update_url_name(
    data: web::Data<AppState>,
    code: web::Path<String>,
    req_payload: web::Json<UpdateUrlNameRequest>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    let user_id = match get_user_id(&http_req) {
        Some(id) => id,
        None => {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized"
            })));
        }
    };

    let db = data.db.lock().unwrap_or_else(|e| e.into_inner());

    // Update the URL name only if it belongs to the current user
    match db.execute(
        "UPDATE urls SET name = ?1 WHERE short_code = ?2 AND user_id = ?3",
        params![
            req_payload.name.as_deref(),
            code.as_str(),
            user_id
        ],
    ) {
        Ok(rows_affected) => {
            if rows_affected > 0 {
                Ok(HttpResponse::Ok().json(serde_json::json!({
                    "message": "URL name updated successfully"
                })))
            } else {
                Ok(HttpResponse::NotFound().json(serde_json::json!({
                    "error": "Short URL not found or not owned by you"
                })))
            }
        }
        Err(_) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Failed to update URL name"
        }))),
    }
}

/// Protected endpoint to get click history
pub async fn get_click_history(
    data: web::Data<AppState>,
    code: web::Path<String>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    let user_id = match get_user_id(&http_req) {
        Some(id) => id,
        None => {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized"
            })));
        }
    };

    let db = data.db.lock().unwrap_or_else(|e| e.into_inner());

    // First verify ownership
    let url_id: rusqlite::Result<i64> = db.query_row(
        "SELECT id FROM urls WHERE short_code = ?1 AND user_id = ?2",
        params![code.as_str(), user_id],
        |row| row.get(0),
    );

    let url_id = match url_id {
        Ok(id) => id,
        Err(_) => {
            return Ok(HttpResponse::NotFound().json(serde_json::json!({
                "error": "Short URL not found or not owned by you"
            })));
        }
    };

    // Get total clicks from counter
    let total_clicks: u64 = db
        .query_row(
            "SELECT clicks FROM urls WHERE id = ?1",
            params![url_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // Get click history (limited to recent 1000)
    let mut stmt = db
        .prepare(
            "SELECT clicked_at FROM click_history WHERE url_id = ?1 ORDER BY clicked_at DESC LIMIT 1000",
        )
        .map_err(|_| actix_web::error::ErrorInternalServerError("Database error"))?;

    let history: Vec<ClickHistoryEntry> = stmt
        .query_map(params![url_id], |row| {
            Ok(ClickHistoryEntry {
                clicked_at: row.get(0)?,
            })
        })
        .map_err(|_| actix_web::error::ErrorInternalServerError("Database error"))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(HttpResponse::Ok().json(ClickStats {
        total_clicks,
        history,
    }))
}

/// Protected endpoint to generate and download QR codes
pub async fn get_qr_code(
    data: web::Data<AppState>,
    path: web::Path<(String, String)>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    let (code, format) = path.into_inner();

    let user_id = match get_user_id(&http_req) {
        Some(id) => id,
        None => {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized"
            })));
        }
    };

    let db = data.db.lock().unwrap_or_else(|e| e.into_inner());

    // Verify ownership
    let exists: bool = db
        .query_row(
            "SELECT COUNT(*) FROM urls WHERE short_code = ?1 AND user_id = ?2",
            params![&code, user_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .unwrap_or(false);

    if !exists {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "Short URL not found or not owned by you"
        })));
    }

    // Get the actual host from the request
    let host = http_req.connection_info().host().to_string();

    let scheme = if http_req.connection_info().scheme() == "https" {
        "https"
    } else {
        "http"
    };

    let full_url = format!("{}://{}/{}", scheme, host, code);
    drop(db); // Release lock before heavy computation

    match format.as_str() {
        "png" => match generate_qr_code_png(&full_url) {
            Ok(png_bytes) => Ok(HttpResponse::Ok()
                .content_type("image/png")
                .append_header((
                    "Content-Disposition",
                    format!("attachment; filename=\"{}.png\"", code),
                ))
                .body(png_bytes)),
            Err(e) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Failed to generate QR code: {}", e)
            }))),
        },
        "svg" => match generate_qr_code_svg(&full_url) {
            Ok(svg_string) => Ok(HttpResponse::Ok()
                .content_type("image/svg+xml")
                .append_header((
                    "Content-Disposition",
                    format!("attachment; filename=\"{}.svg\"", code),
                ))
                .body(svg_string)),
            Err(e) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Failed to generate QR code: {}", e)
            }))),
        },
        _ => Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Invalid format. Use 'png' or 'svg'"
        }))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};
    use serde_json::Value;

    // -------------------------------------------------------------------------
    // Standalone tests
    // -------------------------------------------------------------------------
    #[cfg(feature = "standalone")]
    mod standalone {
        use super::*;
        use actix_web_httpauth::middleware::HttpAuthentication;
        use crate::auth::middleware::jwt_validator;
        use crate::testing::{
            insert_test_url, insert_test_user, make_test_state, make_test_token,
        };

        macro_rules! setup_app {
            ($state:expr) => {{
                let jwt = HttpAuthentication::bearer(jwt_validator);
                test::init_service(
                    App::new()
                        .app_data($state.clone())
                        .service(
                            web::scope("/api")
                                .wrap(jwt)
                                .route("/shorten", web::post().to(shorten_url))
                                .route("/urls", web::get().to(get_user_urls))
                                .route("/urls/{code}", web::delete().to(delete_url))
                                .route("/urls/{code}/name", web::patch().to(update_url_name))
                                .route("/stats/{code}", web::get().to(get_stats))
                                .route("/urls/{code}/clicks", web::get().to(get_click_history))
                                .route("/urls/{code}/qr/{format}", web::get().to(get_qr_code)),
                        )
                        .route("/{code}", web::get().to(redirect_url)),
                )
                .await
            }};
        }

        // --- shorten_url ---

        #[actix_web::test]
        async fn shorten_url_success() {
            let state = make_test_state();
            let uid = insert_test_user(&state, "alice", false);
            let token = make_test_token("alice", uid, false);
            let app = setup_app!(state);

            let req = test::TestRequest::post()
                .uri("/api/shorten")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(serde_json::json!({"url": "https://example.com"}))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200);

            let body: Value = test::read_body_json(resp).await;
            assert!(body["short_code"].is_string());
            assert_eq!(body["original_url"], "https://example.com");
        }

        #[actix_web::test]
        async fn shorten_url_short_code_is_6_chars() {
            let state = make_test_state();
            let uid = insert_test_user(&state, "alice", false);
            let token = make_test_token("alice", uid, false);
            let app = setup_app!(state);

            let req = test::TestRequest::post()
                .uri("/api/shorten")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(serde_json::json!({"url": "https://example.com"}))
                .to_request();
            let body: Value = test::call_and_read_body_json(&app, req).await;
            assert_eq!(body["short_code"].as_str().unwrap().len(), 6);
        }

        #[actix_web::test]
        async fn shorten_same_url_twice_returns_same_code() {
            let state = make_test_state();
            let uid = insert_test_user(&state, "alice", false);
            let token = make_test_token("alice", uid, false);
            let app = setup_app!(state);

            let shorten = |app: &_| {
                test::TestRequest::post()
                    .uri("/api/shorten")
                    .insert_header(("Authorization", format!("Bearer {token}")))
                    .set_json(serde_json::json!({"url": "https://example.com"}))
                    .to_request()
            };
            let b1: Value = test::call_and_read_body_json(&app, shorten(&app)).await;
            let b2: Value = test::call_and_read_body_json(&app, shorten(&app)).await;
            assert_eq!(b1["short_code"], b2["short_code"]);
        }

        #[actix_web::test]
        async fn shorten_url_empty_returns_400() {
            let state = make_test_state();
            let uid = insert_test_user(&state, "alice", false);
            let token = make_test_token("alice", uid, false);
            let app = setup_app!(state);

            let req = test::TestRequest::post()
                .uri("/api/shorten")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(serde_json::json!({"url": ""}))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 400);
        }

        #[actix_web::test]
        async fn shorten_invalid_url_returns_400() {
            let state = make_test_state();
            let uid = insert_test_user(&state, "alice", false);
            let token = make_test_token("alice", uid, false);
            let app = setup_app!(state);

            let req = test::TestRequest::post()
                .uri("/api/shorten")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(serde_json::json!({"url": "ftp://bad-scheme.com"}))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 400);
        }

        #[actix_web::test]
        async fn shorten_url_without_auth_returns_401() {
            let state = make_test_state();
            let app = setup_app!(state);
            let req = test::TestRequest::post()
                .uri("/api/shorten")
                .set_json(serde_json::json!({"url": "https://example.com"}))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 401);
        }

        // --- redirect_url ---

        #[actix_web::test]
        async fn redirect_existing_code_returns_302() {
            let state = make_test_state();
            let uid = insert_test_user(&state, "alice", false);
            insert_test_url(&state, uid, "https://example.com", "abc123");
            let app = setup_app!(state);

            let req = test::TestRequest::get().uri("/abc123").to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 302);
            assert_eq!(
                resp.headers().get("Location").unwrap(),
                "https://example.com"
            );
        }

        #[actix_web::test]
        async fn redirect_nonexistent_code_returns_404() {
            let state = make_test_state();
            let app = setup_app!(state);
            let req = test::TestRequest::get().uri("/xxxxxx").to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 404);
        }

        #[actix_web::test]
        async fn redirect_increments_click_counter() {
            let state = make_test_state();
            let uid = insert_test_user(&state, "alice", false);
            insert_test_url(&state, uid, "https://example.com", "abc123");
            let app = setup_app!(state);

            // Visit twice
            for _ in 0..2 {
                test::call_service(
                    &app,
                    test::TestRequest::get().uri("/abc123").to_request(),
                )
                .await;
            }

            let clicks: i64 = {
                let db = state.db.lock().unwrap();
                db.query_row(
                    "SELECT clicks FROM urls WHERE short_code = 'abc123'",
                    [],
                    |r| r.get(0),
                )
                .unwrap()
            };
            assert_eq!(clicks, 2);
        }

        // --- get_user_urls ---

        #[actix_web::test]
        async fn get_user_urls_empty_list() {
            let state = make_test_state();
            let uid = insert_test_user(&state, "alice", false);
            let token = make_test_token("alice", uid, false);
            let app = setup_app!(state);

            let req = test::TestRequest::get()
                .uri("/api/urls")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request();
            let body: Value = test::call_and_read_body_json(&app, req).await;
            assert_eq!(body, serde_json::json!([]));
        }

        #[actix_web::test]
        async fn get_user_urls_returns_own_urls_only() {
            let state = make_test_state();
            let uid_a = insert_test_user(&state, "alice", false);
            let uid_b = insert_test_user(&state, "bob", false);
            insert_test_url(&state, uid_a, "https://alice.com", "aaa111");
            insert_test_url(&state, uid_b, "https://bob.com", "bbb222");
            let token = make_test_token("alice", uid_a, false);
            let app = setup_app!(state);

            let req = test::TestRequest::get()
                .uri("/api/urls")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request();
            let body: Value = test::call_and_read_body_json(&app, req).await;
            let arr = body.as_array().unwrap();
            assert_eq!(arr.len(), 1);
            assert_eq!(arr[0]["short_code"], "aaa111");
        }

        // --- get_stats ---

        #[actix_web::test]
        async fn get_stats_for_own_url() {
            let state = make_test_state();
            let uid = insert_test_user(&state, "alice", false);
            insert_test_url(&state, uid, "https://example.com", "abc123");
            let token = make_test_token("alice", uid, false);
            let app = setup_app!(state);

            let req = test::TestRequest::get()
                .uri("/api/stats/abc123")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200);
            let body: Value = test::read_body_json(resp).await;
            assert_eq!(body["short_code"], "abc123");
        }

        #[actix_web::test]
        async fn get_stats_for_other_users_url_returns_404() {
            let state = make_test_state();
            let uid_a = insert_test_user(&state, "alice", false);
            let uid_b = insert_test_user(&state, "bob", false);
            insert_test_url(&state, uid_b, "https://bob.com", "bbb222");
            let token = make_test_token("alice", uid_a, false);
            let app = setup_app!(state);

            let req = test::TestRequest::get()
                .uri("/api/stats/bbb222")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 404);
        }

        // --- delete_url ---

        #[actix_web::test]
        async fn delete_own_url_succeeds() {
            let state = make_test_state();
            let uid = insert_test_user(&state, "alice", false);
            insert_test_url(&state, uid, "https://example.com", "del123");
            let token = make_test_token("alice", uid, false);
            let app = setup_app!(state);

            let req = test::TestRequest::delete()
                .uri("/api/urls/del123")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200);
        }

        #[actix_web::test]
        async fn delete_other_users_url_returns_404() {
            let state = make_test_state();
            let uid_a = insert_test_user(&state, "alice", false);
            let uid_b = insert_test_user(&state, "bob", false);
            insert_test_url(&state, uid_b, "https://bob.com", "bbb222");
            let token = make_test_token("alice", uid_a, false);
            let app = setup_app!(state);

            let req = test::TestRequest::delete()
                .uri("/api/urls/bbb222")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 404);
        }

        #[actix_web::test]
        async fn delete_nonexistent_url_returns_404() {
            let state = make_test_state();
            let uid = insert_test_user(&state, "alice", false);
            let token = make_test_token("alice", uid, false);
            let app = setup_app!(state);

            let req = test::TestRequest::delete()
                .uri("/api/urls/xxxxxx")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 404);
        }

        // --- update_url_name ---

        #[actix_web::test]
        async fn update_name_success() {
            let state = make_test_state();
            let uid = insert_test_user(&state, "alice", false);
            insert_test_url(&state, uid, "https://example.com", "abc123");
            let token = make_test_token("alice", uid, false);
            let app = setup_app!(state);

            let req = test::TestRequest::patch()
                .uri("/api/urls/abc123/name")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(serde_json::json!({"name": "My Link"}))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200);
        }

        #[actix_web::test]
        async fn update_name_clears_name_when_null() {
            let state = make_test_state();
            let uid = insert_test_user(&state, "alice", false);
            insert_test_url(&state, uid, "https://example.com", "abc123");
            let token = make_test_token("alice", uid, false);
            let app = setup_app!(state);

            let req = test::TestRequest::patch()
                .uri("/api/urls/abc123/name")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(serde_json::json!({"name": null}))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200);
        }

        #[actix_web::test]
        async fn update_name_for_other_users_url_returns_404() {
            let state = make_test_state();
            let uid_a = insert_test_user(&state, "alice", false);
            let uid_b = insert_test_user(&state, "bob", false);
            insert_test_url(&state, uid_b, "https://bob.com", "bbb222");
            let token = make_test_token("alice", uid_a, false);
            let app = setup_app!(state);

            let req = test::TestRequest::patch()
                .uri("/api/urls/bbb222/name")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(serde_json::json!({"name": "Stolen"}))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 404);
        }

        // --- get_click_history ---

        #[actix_web::test]
        async fn click_history_empty_initially() {
            let state = make_test_state();
            let uid = insert_test_user(&state, "alice", false);
            insert_test_url(&state, uid, "https://example.com", "abc123");
            let token = make_test_token("alice", uid, false);
            let app = setup_app!(state);

            let req = test::TestRequest::get()
                .uri("/api/urls/abc123/clicks")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request();
            let body: Value = test::call_and_read_body_json(&app, req).await;
            assert_eq!(body["total_clicks"], 0);
            assert_eq!(body["history"], serde_json::json!([]));
        }

        #[actix_web::test]
        async fn click_history_reflects_redirects() {
            let state = make_test_state();
            let uid = insert_test_user(&state, "alice", false);
            insert_test_url(&state, uid, "https://example.com", "abc123");
            let token = make_test_token("alice", uid, false);
            let app = setup_app!(state);

            // Two redirects
            for _ in 0..2 {
                test::call_service(
                    &app,
                    test::TestRequest::get().uri("/abc123").to_request(),
                )
                .await;
            }

            let req = test::TestRequest::get()
                .uri("/api/urls/abc123/clicks")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request();
            let body: Value = test::call_and_read_body_json(&app, req).await;
            assert_eq!(body["total_clicks"], 2);
            assert_eq!(body["history"].as_array().unwrap().len(), 2);
        }

        #[actix_web::test]
        async fn click_history_for_other_users_url_returns_404() {
            let state = make_test_state();
            let uid_a = insert_test_user(&state, "alice", false);
            let uid_b = insert_test_user(&state, "bob", false);
            insert_test_url(&state, uid_b, "https://bob.com", "bbb222");
            let token = make_test_token("alice", uid_a, false);
            let app = setup_app!(state);

            let req = test::TestRequest::get()
                .uri("/api/urls/bbb222/clicks")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 404);
        }

        // --- get_qr_code ---

        #[actix_web::test]
        async fn qr_code_png_success() {
            let state = make_test_state();
            let uid = insert_test_user(&state, "alice", false);
            insert_test_url(&state, uid, "https://example.com", "abc123");
            let token = make_test_token("alice", uid, false);
            let app = setup_app!(state);

            let req = test::TestRequest::get()
                .uri("/api/urls/abc123/qr/png")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200);
            assert_eq!(resp.headers().get("content-type").unwrap(), "image/png");
        }

        #[actix_web::test]
        async fn qr_code_svg_success() {
            let state = make_test_state();
            let uid = insert_test_user(&state, "alice", false);
            insert_test_url(&state, uid, "https://example.com", "abc123");
            let token = make_test_token("alice", uid, false);
            let app = setup_app!(state);

            let req = test::TestRequest::get()
                .uri("/api/urls/abc123/qr/svg")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200);
            assert_eq!(resp.headers().get("content-type").unwrap(), "image/svg+xml");
        }

        #[actix_web::test]
        async fn qr_code_invalid_format_returns_400() {
            let state = make_test_state();
            let uid = insert_test_user(&state, "alice", false);
            insert_test_url(&state, uid, "https://example.com", "abc123");
            let token = make_test_token("alice", uid, false);
            let app = setup_app!(state);

            let req = test::TestRequest::get()
                .uri("/api/urls/abc123/qr/gif")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 400);
        }

        #[actix_web::test]
        async fn delete_url_removes_click_history() {
            let state = make_test_state();
            let uid = insert_test_user(&state, "alice", false);
            insert_test_url(&state, uid, "https://example.com", "clk001");
            let token = make_test_token("alice", uid, false);
            let app = setup_app!(state);

            // Create a click via redirect
            test::call_service(
                &app,
                test::TestRequest::get().uri("/clk001").to_request(),
            )
            .await;

            // Delete the URL
            let req = test::TestRequest::delete()
                .uri("/api/urls/clk001")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request();
            test::call_service(&app, req).await;

            // Verify click history is also gone (FK cascade)
            let history_count: i64 = {
                let db = state.db.lock().unwrap();
                db.query_row("SELECT COUNT(*) FROM click_history", [], |r| r.get(0))
                    .unwrap()
            };
            assert_eq!(history_count, 0);
        }

        #[actix_web::test]
        async fn qr_code_nonexistent_url_returns_404() {
            let state = make_test_state();
            let uid = insert_test_user(&state, "alice", false);
            let token = make_test_token("alice", uid, false);
            let app = setup_app!(state);

            let req = test::TestRequest::get()
                .uri("/api/urls/xxxxxx/qr/png")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 404);
        }

        #[actix_web::test]
        async fn shorten_different_users_same_url_get_different_codes() {
            let state = make_test_state();
            let uid_a = insert_test_user(&state, "alice", false);
            let uid_b = insert_test_user(&state, "bob", false);
            let token_a = make_test_token("alice", uid_a, false);
            let token_b = make_test_token("bob", uid_b, false);
            let app = setup_app!(state);

            let body_a: Value = test::call_and_read_body_json(
                &app,
                test::TestRequest::post()
                    .uri("/api/shorten")
                    .insert_header(("Authorization", format!("Bearer {token_a}")))
                    .set_json(serde_json::json!({"url": "https://shared.com"}))
                    .to_request(),
            )
            .await;

            let body_b: Value = test::call_and_read_body_json(
                &app,
                test::TestRequest::post()
                    .uri("/api/shorten")
                    .insert_header(("Authorization", format!("Bearer {token_b}")))
                    .set_json(serde_json::json!({"url": "https://shared.com"}))
                    .to_request(),
            )
            .await;

            assert_ne!(body_a["short_code"], body_b["short_code"]);
        }

        #[actix_web::test]
        async fn update_name_nonexistent_url_returns_404() {
            let state = make_test_state();
            let uid = insert_test_user(&state, "alice", false);
            let token = make_test_token("alice", uid, false);
            let app = setup_app!(state);

            let req = test::TestRequest::patch()
                .uri("/api/urls/xxxxxx/name")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(serde_json::json!({"name": "Ghost"}))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 404);
        }

        #[actix_web::test]
        async fn shorten_url_too_long_returns_400() {
            let state = make_test_state();
            let uid = insert_test_user(&state, "alice", false);
            let token = make_test_token("alice", uid, false);
            let app = setup_app!(state);

            let long_url = format!("https://example.com/{}", "a".repeat(2048));
            let req = test::TestRequest::post()
                .uri("/api/shorten")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(serde_json::json!({"url": long_url}))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 400);
        }

        #[actix_web::test]
        async fn shorten_javascript_url_returns_400() {
            let state = make_test_state();
            let uid = insert_test_user(&state, "alice", false);
            let token = make_test_token("alice", uid, false);
            let app = setup_app!(state);

            let req = test::TestRequest::post()
                .uri("/api/shorten")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(serde_json::json!({"url": "javascript:alert(1)"}))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 400);
        }

        #[actix_web::test]
        async fn redirect_records_click_history() {
            let state = make_test_state();
            let uid = insert_test_user(&state, "alice", false);
            insert_test_url(&state, uid, "https://example.com", "hist01");
            let app = setup_app!(state);

            test::call_service(
                &app,
                test::TestRequest::get().uri("/hist01").to_request(),
            )
            .await;

            let history_count: i64 = {
                let db = state.db.lock().unwrap();
                db.query_row(
                    "SELECT COUNT(*) FROM click_history WHERE url_id = (SELECT id FROM urls WHERE short_code = 'hist01')",
                    [],
                    |r| r.get(0),
                )
                .unwrap()
            };
            assert_eq!(history_count, 1);
        }

        #[actix_web::test]
        async fn qr_code_for_other_users_url_returns_404() {
            let state = make_test_state();
            let uid_a = insert_test_user(&state, "alice", false);
            let uid_b = insert_test_user(&state, "bob", false);
            insert_test_url(&state, uid_b, "https://bob.com", "bbb222");
            let token = make_test_token("alice", uid_a, false);
            let app = setup_app!(state);

            let req = test::TestRequest::get()
                .uri("/api/urls/bbb222/qr/png")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 404);
        }
    }

    // -------------------------------------------------------------------------
    // SaaS tests
    // -------------------------------------------------------------------------
    #[cfg(feature = "saas")]
    mod saas {
        use super::*;
        use crate::handlers::saas_auth::saas_cookie_validator;
        use crate::testing::{insert_saas_url, insert_saas_user, make_saas_jwt, make_test_state};

        macro_rules! setup_app {
            ($state:expr) => {{
                test::init_service(
                    App::new()
                        .app_data($state.clone())
                        .service(
                            web::scope("/api")
                                .wrap(actix_web::middleware::from_fn(saas_cookie_validator))
                                .route("/shorten", web::post().to(shorten_url))
                                .route("/urls", web::get().to(get_user_urls))
                                .route("/urls/{code}", web::delete().to(delete_url))
                                .route("/stats/{code}", web::get().to(get_stats)),
                        )
                        .route("/{code}", web::get().to(redirect_url)),
                )
                .await
            }};
        }

        fn cookie(jwt: &str) -> String {
            format!("access_token={jwt}")
        }

        #[actix_web::test]
        async fn shorten_url_with_valid_member_cookie() {
            let state = make_test_state();
            let app = setup_app!(state);
            let jwt = make_saas_jwt("42", "alice@example.com", "active", None);

            let req = test::TestRequest::post()
                .uri("/api/shorten")
                .insert_header(("Cookie", cookie(&jwt)))
                .set_json(serde_json::json!({"url": "https://example.com"}))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200);
        }

        #[actix_web::test]
        async fn shorten_url_without_cookie_returns_401() {
            let state = make_test_state();
            let app = setup_app!(state);

            let req = test::TestRequest::post()
                .uri("/api/shorten")
                .set_json(serde_json::json!({"url": "https://example.com"}))
                .to_request();
            let resp = test::try_call_service(&app, req).await;
            assert!(resp.is_err() || resp.unwrap().status() == 401);
        }

        #[actix_web::test]
        async fn non_member_gets_403_with_redirect() {
            let state = make_test_state();
            let app = setup_app!(state);
            let jwt = make_saas_jwt("99", "noone@example.com", "none", None);

            let req = test::TestRequest::post()
                .uri("/api/shorten")
                .insert_header(("Cookie", cookie(&jwt)))
                .set_json(serde_json::json!({"url": "https://example.com"}))
                .to_request();
            let resp = test::try_call_service(&app, req).await;
            assert!(resp.is_err() || resp.unwrap().status() == 403);
        }

        #[actix_web::test]
        async fn admin_bypasses_membership_check() {
            let state = make_test_state();
            let app = setup_app!(state);
            let jwt = make_saas_jwt("1", "admin@example.com", "none", Some("admin"));

            let req = test::TestRequest::post()
                .uri("/api/shorten")
                .insert_header(("Cookie", cookie(&jwt)))
                .set_json(serde_json::json!({"url": "https://example.com"}))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200);
        }

        #[actix_web::test]
        async fn grace_period_member_allowed() {
            let state = make_test_state();
            let app = setup_app!(state);
            let jwt = make_saas_jwt("55", "grace@example.com", "grace_period", None);

            let req = test::TestRequest::post()
                .uri("/api/shorten")
                .insert_header(("Cookie", cookie(&jwt)))
                .set_json(serde_json::json!({"url": "https://example.com"}))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200);
        }

        #[actix_web::test]
        async fn canceled_member_gets_403() {
            let state = make_test_state();
            let app = setup_app!(state);
            let jwt = make_saas_jwt("77", "old@example.com", "canceled", None);

            let req = test::TestRequest::post()
                .uri("/api/shorten")
                .insert_header(("Cookie", cookie(&jwt)))
                .set_json(serde_json::json!({"url": "https://example.com"}))
                .to_request();
            let resp = test::try_call_service(&app, req).await;
            assert!(resp.is_err() || resp.unwrap().status() == 403);
        }

        #[actix_web::test]
        async fn saas_delete_own_url() {
            let state = make_test_state();
            insert_saas_user(&state, 42, "alice@example.com", false);
            insert_saas_url(&state, 42, "https://alice.com", "del001");
            let app = setup_app!(state);
            let jwt = make_saas_jwt("42", "alice@example.com", "active", None);

            let req = test::TestRequest::delete()
                .uri("/api/urls/del001")
                .insert_header(("Cookie", cookie(&jwt)))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200);

            let count: i64 = {
                let db = state.db.lock().unwrap();
                db.query_row(
                    "SELECT COUNT(*) FROM urls WHERE short_code='del001'",
                    [],
                    |r| r.get(0),
                )
                .unwrap()
            };
            assert_eq!(count, 0);
        }

        #[actix_web::test]
        async fn saas_stats_for_own_url() {
            let state = make_test_state();
            insert_saas_user(&state, 42, "alice@example.com", false);
            insert_saas_url(&state, 42, "https://alice.com", "sta001");
            let app = setup_app!(state);
            let jwt = make_saas_jwt("42", "alice@example.com", "active", None);

            let req = test::TestRequest::get()
                .uri("/api/stats/sta001")
                .insert_header(("Cookie", cookie(&jwt)))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200);
            let body: Value = test::read_body_json(resp).await;
            assert_eq!(body["short_code"], "sta001");
        }

        #[actix_web::test]
        async fn saas_redirect_works() {
            let state = make_test_state();
            insert_saas_user(&state, 42, "alice@example.com", false);
            insert_saas_url(&state, 42, "https://alice.com", "red001");
            let app = setup_app!(state);

            let req = test::TestRequest::get().uri("/red001").to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 302);
            assert_eq!(
                resp.headers().get("Location").unwrap(),
                "https://alice.com"
            );
        }

        #[actix_web::test]
        async fn saas_user_can_only_see_own_urls() {
            let state = make_test_state();
            insert_saas_user(&state, 42, "alice@example.com", false);
            insert_saas_user(&state, 43, "bob@example.com", false);
            insert_saas_url(&state, 42, "https://alice.com", "aaa111");
            insert_saas_url(&state, 43, "https://bob.com", "bbb222");
            let app = setup_app!(state);
            let jwt = make_saas_jwt("42", "alice@example.com", "active", None);

            let req = test::TestRequest::get()
                .uri("/api/urls")
                .insert_header(("Cookie", cookie(&jwt)))
                .to_request();
            let body: Value = test::call_and_read_body_json(&app, req).await;
            let arr = body.as_array().unwrap();
            assert_eq!(arr.len(), 1);
            assert_eq!(arr[0]["short_code"], "aaa111");
        }
    }
}
