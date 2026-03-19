use actix_web::{web, HttpResponse, Result};
#[cfg(feature = "standalone")]
use actix_web::HttpRequest;
#[cfg(feature = "standalone")]
use chrono::Utc;
use rusqlite::params;
use tracing::info;

#[cfg(feature = "standalone")]
use crate::auth::get_claims;
use crate::db::AppState;
#[cfg(feature = "standalone")]
use crate::models::{AbuseReport, ResolveReportRequest};
use crate::models::SubmitReportRequest;

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

    let db = data.db.lock().unwrap_or_else(|e| e.into_inner());

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
        Ok(_) => {
            info!(short_code = %req.short_code, "Abuse report submitted");
            Ok(HttpResponse::Created().json(serde_json::json!({
                "message": "Report submitted successfully. Thank you for helping keep our service safe."
            })))
        }
        Err(_) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Failed to submit report"
        }))),
    }
}

/// Admin endpoint to list all abuse reports - standalone only
#[cfg(feature = "standalone")]
pub async fn admin_list_reports(data: web::Data<AppState>) -> Result<HttpResponse> {
    let db = data.db.lock().unwrap_or_else(|e| e.into_inner());

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

/// Admin endpoint to resolve an abuse report - standalone only
#[cfg(feature = "standalone")]
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

    let db = data.db.lock().unwrap_or_else(|e| e.into_inner());

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

            info!(report_id = *report_id, action = "dismiss", admin_user_id = claims.user_id, "Abuse report resolved");
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

            info!(report_id = *report_id, action = "delete_url", admin_user_id = claims.user_id, short_code = %short_code, "Abuse report resolved");
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

                info!(report_id = *report_id, action = "ban_user", admin_user_id = claims.user_id, banned_user_id = user_id, "Abuse report resolved");
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

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};
    use serde_json::Value;

    #[cfg(feature = "standalone")]
    use {
        actix_web_httpauth::middleware::HttpAuthentication,
        crate::auth::middleware::admin_validator,
        crate::testing::{insert_test_url, insert_test_user, make_test_state, make_test_token},
    };

    #[cfg(feature = "saas")]
    use crate::testing::make_test_state;

    macro_rules! setup_abuse_app {
        ($state:expr) => {{
            #[cfg(feature = "standalone")]
            {
                let admin_auth = HttpAuthentication::bearer(admin_validator);
                test::init_service(
                    App::new()
                        .app_data($state.clone())
                        .route("/api/report-abuse", web::post().to(submit_abuse_report))
                        .service(
                            web::scope("/api/admin")
                                .wrap(admin_auth)
                                .route("/reports", web::get().to(admin_list_reports))
                                .route("/reports/{id}", web::post().to(admin_resolve_report)),
                        ),
                )
                .await
            }
            #[cfg(feature = "saas")]
            {
                test::init_service(
                    App::new()
                        .app_data($state.clone())
                        .route("/api/report-abuse", web::post().to(submit_abuse_report)),
                )
                .await
            }
        }};
    }

    // --- submit_abuse_report ---

    #[actix_web::test]
    async fn submit_report_for_existing_url_returns_201() {
        let state = make_test_state();
        #[cfg(feature = "standalone")]
        {
            let uid = insert_test_user(&state, "alice", false);
            insert_test_url(&state, uid, "https://bad.com", "bad123");
        }
        #[cfg(feature = "saas")]
        {
            let db = state.db.lock().unwrap();
            db.execute(
                "INSERT INTO users (userID, username, password) VALUES (1, 'u', '')",
                [],
            )
            .unwrap();
            db.execute(
                "INSERT INTO urls (user_id, original_url, short_code) VALUES (1, 'https://bad.com', 'bad123')",
                [],
            )
            .unwrap();
        }
        let app = setup_abuse_app!(state);

        let req = test::TestRequest::post()
            .uri("/api/report-abuse")
            .set_json(serde_json::json!({
                "short_code": "bad123",
                "reason": "spam",
                "reporter_email": "reporter@example.com"
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    #[actix_web::test]
    async fn submit_report_for_missing_url_returns_404() {
        let state = make_test_state();
        let app = setup_abuse_app!(state);

        let req = test::TestRequest::post()
            .uri("/api/report-abuse")
            .set_json(serde_json::json!({
                "short_code": "noexist",
                "reason": "spam"
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);
    }

    #[actix_web::test]
    async fn submit_report_empty_short_code_returns_400() {
        let state = make_test_state();
        let app = setup_abuse_app!(state);

        let req = test::TestRequest::post()
            .uri("/api/report-abuse")
            .set_json(serde_json::json!({"short_code": "", "reason": "spam"}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn submit_report_empty_reason_returns_400() {
        let state = make_test_state();
        let app = setup_abuse_app!(state);

        let req = test::TestRequest::post()
            .uri("/api/report-abuse")
            .set_json(serde_json::json!({"short_code": "abc123", "reason": ""}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn submit_report_invalid_email_returns_400() {
        let state = make_test_state();
        let app = setup_abuse_app!(state);

        let req = test::TestRequest::post()
            .uri("/api/report-abuse")
            .set_json(serde_json::json!({
                "short_code": "abc123",
                "reason": "spam",
                "reporter_email": "not-an-email"
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    // --- admin_list_reports / admin_resolve_report (standalone only) ---

    #[cfg(feature = "standalone")]
    #[actix_web::test]
    async fn admin_can_list_reports() {
        let state = make_test_state();
        let uid = insert_test_user(&state, "admin", true);
        insert_test_url(&state, uid, "https://bad.com", "bad123");
        let token = make_test_token("admin", uid, true);
        let app = setup_abuse_app!(state);

        // Submit a report first
        test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/report-abuse")
                .set_json(serde_json::json!({"short_code": "bad123", "reason": "spam"}))
                .to_request(),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/api/admin/reports")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let body: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(body.as_array().unwrap().len(), 1);
        assert_eq!(body[0]["short_code"], "bad123");
    }

    #[cfg(feature = "standalone")]
    #[actix_web::test]
    async fn admin_can_dismiss_report() {
        let state = make_test_state();
        let uid = insert_test_user(&state, "admin", true);
        insert_test_url(&state, uid, "https://bad.com", "bad123");
        let token = make_test_token("admin", uid, true);
        let app = setup_abuse_app!(state);

        test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/report-abuse")
                .set_json(serde_json::json!({"short_code": "bad123", "reason": "spam"}))
                .to_request(),
        )
        .await;

        let report_id: i64 = {
            let db = state.db.lock().unwrap();
            db.query_row("SELECT id FROM abuse_reports LIMIT 1", [], |r| r.get(0))
                .unwrap()
        };

        let req = test::TestRequest::post()
            .uri(&format!("/api/admin/reports/{report_id}"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(serde_json::json!({"action": "dismiss"}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        let status: String = {
            let db = state.db.lock().unwrap();
            db.query_row(
                "SELECT status FROM abuse_reports WHERE id=?1",
                [report_id],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(status, "dismissed");
    }

    #[cfg(feature = "standalone")]
    #[actix_web::test]
    async fn admin_can_delete_url_via_report() {
        let state = make_test_state();
        let uid = insert_test_user(&state, "admin", true);
        insert_test_url(&state, uid, "https://bad.com", "bad123");
        let token = make_test_token("admin", uid, true);
        let app = setup_abuse_app!(state);

        test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/report-abuse")
                .set_json(serde_json::json!({"short_code": "bad123", "reason": "spam"}))
                .to_request(),
        )
        .await;

        let report_id: i64 = {
            let db = state.db.lock().unwrap();
            db.query_row("SELECT id FROM abuse_reports LIMIT 1", [], |r| r.get(0))
                .unwrap()
        };

        test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&format!("/api/admin/reports/{report_id}"))
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(serde_json::json!({"action": "delete_url"}))
                .to_request(),
        )
        .await;

        let url_count: i64 = {
            let db = state.db.lock().unwrap();
            db.query_row(
                "SELECT COUNT(*) FROM urls WHERE short_code='bad123'",
                [],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(url_count, 0);
    }

    #[cfg(feature = "standalone")]
    #[actix_web::test]
    async fn resolving_already_resolved_report_returns_400() {
        let state = make_test_state();
        let uid = insert_test_user(&state, "admin", true);
        insert_test_url(&state, uid, "https://bad.com", "bad123");
        let token = make_test_token("admin", uid, true);
        let app = setup_abuse_app!(state);

        test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/report-abuse")
                .set_json(serde_json::json!({"short_code": "bad123", "reason": "spam"}))
                .to_request(),
        )
        .await;

        let report_id: i64 = {
            let db = state.db.lock().unwrap();
            db.query_row("SELECT id FROM abuse_reports LIMIT 1", [], |r| r.get(0))
                .unwrap()
        };

        // Dismiss once
        test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&format!("/api/admin/reports/{report_id}"))
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(serde_json::json!({"action": "dismiss"}))
                .to_request(),
        )
        .await;

        // Dismiss again — should fail
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&format!("/api/admin/reports/{report_id}"))
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(serde_json::json!({"action": "dismiss"}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 400);
    }

    #[cfg(feature = "standalone")]
    #[actix_web::test]
    async fn resolve_invalid_action_returns_400() {
        let state = make_test_state();
        let uid = insert_test_user(&state, "admin", true);
        insert_test_url(&state, uid, "https://bad.com", "bad123");
        let token = make_test_token("admin", uid, true);
        let app = setup_abuse_app!(state);

        test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/report-abuse")
                .set_json(serde_json::json!({"short_code": "bad123", "reason": "spam"}))
                .to_request(),
        )
        .await;

        let report_id: i64 = {
            let db = state.db.lock().unwrap();
            db.query_row("SELECT id FROM abuse_reports LIMIT 1", [], |r| r.get(0))
                .unwrap()
        };

        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&format!("/api/admin/reports/{report_id}"))
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(serde_json::json!({"action": "invalid_action"}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 400);
    }
}
