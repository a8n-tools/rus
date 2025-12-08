use actix_web::{web, HttpRequest, HttpResponse, Result};
use rand::Rng;
use rusqlite::params;

use crate::auth::get_claims;
use crate::db::AppState;
use crate::models::{
    ClickHistoryEntry, ClickStats, ShortenRequest, ShortenResponse, UpdateUrlNameRequest,
    UrlEntry,
};
use crate::security::cleanup_old_clicks;
use crate::url::{generate_qr_code_png, generate_qr_code_svg, generate_short_code, validate_url};

/// Protected API endpoint to shorten a URL
pub async fn shorten_url(
    data: web::Data<AppState>,
    req_payload: web::Json<ShortenRequest>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    // Get user claims from JWT
    let claims = match get_claims(&http_req) {
        Some(c) => c,
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

    let db = data.db.lock().unwrap();

    // Check if URL is already shortened by this user
    let mut stmt = db
        .prepare("SELECT short_code FROM urls WHERE user_id = ?1 AND original_url = ?2")
        .map_err(|_| actix_web::error::ErrorInternalServerError("Database error"))?;

    if let Ok(short_code) = stmt.query_row(
        params![claims.user_id, &req_payload.url],
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
        params![claims.user_id, &req_payload.url, &short_code],
    ) {
        Ok(_) => Ok(HttpResponse::Ok().json(ShortenResponse {
            short_code: short_code.clone(),
            short_url: format!("{}/{}", data.config.host_url, short_code),
            original_url: req_payload.url.clone(),
        })),
        Err(_) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Failed to create short URL"
        }))),
    }
}

/// Public endpoint to redirect to the original URL
pub async fn redirect_url(
    data: web::Data<AppState>,
    code: web::Path<String>,
) -> Result<HttpResponse> {
    let db = data.db.lock().unwrap();

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
                cleanup_old_clicks(&db, data.config.click_retention_days);
            }

            Ok(HttpResponse::Found()
                .append_header(("Location", original_url))
                .finish())
        }
        Err(_) => {
            // Serve the 404 page
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
    // Get user claims from JWT
    let claims = match get_claims(&http_req) {
        Some(c) => c,
        None => {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized"
            })));
        }
    };

    let db = data.db.lock().unwrap();

    // Get URL entry for this user
    let result: rusqlite::Result<UrlEntry> = db.query_row(
        "SELECT original_url, short_code, name, clicks FROM urls WHERE short_code = ?1 AND user_id = ?2",
        params![code.as_str(), claims.user_id],
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
    let claims = match get_claims(&http_req) {
        Some(c) => c,
        None => {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized"
            })));
        }
    };

    let db = data.db.lock().unwrap();

    let mut stmt = db
        .prepare(
            "SELECT original_url, short_code, name, clicks FROM urls WHERE user_id = ?1 ORDER BY created_at DESC",
        )
        .map_err(|_| actix_web::error::ErrorInternalServerError("Database error"))?;

    let urls: Vec<UrlEntry> = stmt
        .query_map(params![claims.user_id], |row| {
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
    let claims = match get_claims(&http_req) {
        Some(c) => c,
        None => {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized"
            })));
        }
    };

    let db = data.db.lock().unwrap();

    // Delete the URL only if it belongs to the current user
    match db.execute(
        "DELETE FROM urls WHERE short_code = ?1 AND user_id = ?2",
        params![code.as_str(), claims.user_id],
    ) {
        Ok(rows_affected) => {
            if rows_affected > 0 {
                Ok(HttpResponse::Ok().json(serde_json::json!({
                    "message": "URL deleted successfully"
                })))
            } else {
                Ok(HttpResponse::NotFound().json(serde_json::json!({
                    "error": "Short URL not found or not owned by you"
                })))
            }
        }
        Err(_) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Failed to delete URL"
        }))),
    }
}

/// Protected endpoint to update URL name
pub async fn update_url_name(
    data: web::Data<AppState>,
    code: web::Path<String>,
    req_payload: web::Json<UpdateUrlNameRequest>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    let claims = match get_claims(&http_req) {
        Some(c) => c,
        None => {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized"
            })));
        }
    };

    let db = data.db.lock().unwrap();

    // Update the URL name only if it belongs to the current user
    match db.execute(
        "UPDATE urls SET name = ?1 WHERE short_code = ?2 AND user_id = ?3",
        params![
            req_payload.name.as_deref(),
            code.as_str(),
            claims.user_id
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
    let claims = match get_claims(&http_req) {
        Some(c) => c,
        None => {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized"
            })));
        }
    };

    let db = data.db.lock().unwrap();

    // First verify ownership
    let url_id: rusqlite::Result<i64> = db.query_row(
        "SELECT id FROM urls WHERE short_code = ?1 AND user_id = ?2",
        params![code.as_str(), claims.user_id],
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

    let claims = match get_claims(&http_req) {
        Some(c) => c,
        None => {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized"
            })));
        }
    };

    let db = data.db.lock().unwrap();

    // Verify ownership
    let exists: bool = db
        .query_row(
            "SELECT COUNT(*) FROM urls WHERE short_code = ?1 AND user_id = ?2",
            params![&code, claims.user_id],
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
