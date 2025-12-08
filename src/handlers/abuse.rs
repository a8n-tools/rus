use actix_web::{web, HttpRequest, HttpResponse, Result};
use chrono::Utc;
use rusqlite::params;

use crate::auth::get_claims;
use crate::db::AppState;
use crate::models::{AbuseReport, ResolveReportRequest, SubmitReportRequest};

/// Public endpoint to submit an abuse report
pub async fn submit_abuse_report(
    data: web::Data<AppState>,
    req: web::Json<SubmitReportRequest>,
) -> Result<HttpResponse> {
    // Validate input
    if req.short_code.is_empty() {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Short code cannot be empty"
        })));
    }

    if req.reason.is_empty() {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Reason cannot be empty"
        })));
    }

    // Validate email format if provided
    if let Some(email) = &req.reporter_email {
        if !email.is_empty() && !email.contains('@') {
            return Ok(HttpResponse::BadRequest().json(serde_json::json!({
                "error": "Invalid email format"
            })));
        }
    }

    let db = data.db.lock().unwrap();

    // Check if short code exists
    let url_exists: bool = db
        .query_row(
            "SELECT COUNT(*) FROM urls WHERE short_code = ?1",
            params![&req.short_code],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .unwrap_or(false);

    if !url_exists {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "Short code not found"
        })));
    }

    // Insert the report
    match db.execute(
        "INSERT INTO abuse_reports (short_code, reporter_email, reason, description) VALUES (?1, ?2, ?3, ?4)",
        params![
            &req.short_code,
            req.reporter_email.as_deref(),
            &req.reason,
            req.description.as_deref()
        ],
    ) {
        Ok(_) => Ok(HttpResponse::Created().json(serde_json::json!({
            "message": "Report submitted successfully. Thank you for helping keep our service safe."
        }))),
        Err(_) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Failed to submit report"
        }))),
    }
}

/// Admin endpoint to list all abuse reports
pub async fn admin_list_reports(data: web::Data<AppState>) -> Result<HttpResponse> {
    let db = data.db.lock().unwrap();

    let mut stmt = db
        .prepare(
            "SELECT
            ar.id, ar.short_code, ar.reporter_email, ar.reason, ar.description,
            ar.status, ar.created_at, ar.resolved_at, ar.resolved_by,
            u.original_url, usr.username as url_owner_username, usr.userID as url_owner_id
         FROM abuse_reports ar
         LEFT JOIN urls u ON ar.short_code = u.short_code
         LEFT JOIN users usr ON u.user_id = usr.userID
         ORDER BY
            CASE ar.status
                WHEN 'pending' THEN 1
                WHEN 'resolved' THEN 2
                ELSE 3
            END,
            ar.created_at DESC",
        )
        .map_err(|_| actix_web::error::ErrorInternalServerError("Database error"))?;

    let reports: Vec<AbuseReport> = stmt
        .query_map([], |row| {
            Ok(AbuseReport {
                id: row.get(0)?,
                short_code: row.get(1)?,
                reporter_email: row.get(2)?,
                reason: row.get(3)?,
                description: row.get(4)?,
                status: row.get(5)?,
                created_at: row.get(6)?,
                resolved_at: row.get(7)?,
                resolved_by: row.get(8)?,
                original_url: row.get(9)?,
                url_owner_username: row.get(10)?,
                url_owner_id: row.get(11)?,
            })
        })
        .map_err(|_| actix_web::error::ErrorInternalServerError("Database error"))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(HttpResponse::Ok().json(reports))
}

/// Admin endpoint to resolve an abuse report
pub async fn admin_resolve_report(
    data: web::Data<AppState>,
    report_id: web::Path<i64>,
    req: web::Json<ResolveReportRequest>,
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

    // Get report details
    let report_result: rusqlite::Result<(String, String)> = db.query_row(
        "SELECT ar.short_code, ar.status
         FROM abuse_reports ar
         WHERE ar.id = ?1",
        params![*report_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    );

    let (short_code, status) = match report_result {
        Ok(data) => data,
        Err(_) => {
            return Ok(HttpResponse::NotFound().json(serde_json::json!({
                "error": "Report not found"
            })));
        }
    };

    if status != "pending" {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Report has already been resolved"
        })));
    }

    let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    match req.action.as_str() {
        "dismiss" => {
            // Just mark as resolved
            let _ = db.execute(
                "UPDATE abuse_reports SET status = 'dismissed', resolved_at = ?1, resolved_by = ?2 WHERE id = ?3",
                params![&now, claims.user_id, *report_id],
            );

            Ok(HttpResponse::Ok().json(serde_json::json!({
                "message": "Report dismissed"
            })))
        }
        "delete_url" => {
            // Delete the URL and mark report as resolved
            let _ = db.execute(
                "DELETE FROM urls WHERE short_code = ?1",
                params![&short_code],
            );

            let _ = db.execute(
                "UPDATE abuse_reports SET status = 'resolved', resolved_at = ?1, resolved_by = ?2 WHERE id = ?3",
                params![&now, claims.user_id, *report_id],
            );

            Ok(HttpResponse::Ok().json(serde_json::json!({
                "message": "URL deleted and report resolved"
            })))
        }
        "ban_user" => {
            // Get the user ID who owns this URL
            let user_id_result: rusqlite::Result<i64> = db.query_row(
                "SELECT user_id FROM urls WHERE short_code = ?1",
                params![&short_code],
                |row| row.get(0),
            );

            if let Ok(user_id) = user_id_result {
                // Don't allow banning admin users
                let is_admin: bool = db
                    .query_row(
                        "SELECT is_admin FROM users WHERE userID = ?1",
                        params![user_id],
                        |row| row.get::<_, i32>(0),
                    )
                    .map(|v| v != 0)
                    .unwrap_or(false);

                if is_admin {
                    return Ok(HttpResponse::BadRequest().json(serde_json::json!({
                        "error": "Cannot ban admin users"
                    })));
                }

                // Delete the user (CASCADE will delete all their URLs)
                let _ = db.execute("DELETE FROM users WHERE userID = ?1", params![user_id]);

                let _ = db.execute(
                    "UPDATE abuse_reports SET status = 'resolved', resolved_at = ?1, resolved_by = ?2 WHERE id = ?3",
                    params![&now, claims.user_id, *report_id],
                );

                Ok(HttpResponse::Ok().json(serde_json::json!({
                    "message": "User banned, all URLs deleted, and report resolved"
                })))
            } else {
                Ok(HttpResponse::NotFound().json(serde_json::json!({
                    "error": "URL or user not found"
                })))
            }
        }
        _ => Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Invalid action. Must be 'dismiss', 'delete_url', or 'ban_user'"
        }))),
    }
}
