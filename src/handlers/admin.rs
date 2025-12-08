use actix_web::{web, HttpRequest, HttpResponse, Result};
use rusqlite::params;

use crate::auth::get_claims;
use crate::db::AppState;
use crate::models::{AdminStatsResponse, UserInfo};

/// Admin endpoint to list all users
pub async fn admin_list_users(data: web::Data<AppState>) -> Result<HttpResponse> {
    let db = data.db.lock().unwrap();

    let mut stmt = db
        .prepare(
            "SELECT u.userID, u.username, u.is_admin, u.created_at,
                (SELECT COUNT(*) FROM urls WHERE user_id = u.userID) as url_count
         FROM users u
         ORDER BY u.created_at DESC",
        )
        .map_err(|_| actix_web::error::ErrorInternalServerError("Database error"))?;

    let users: Vec<UserInfo> = stmt
        .query_map([], |row| {
            Ok(UserInfo {
                user_id: row.get(0)?,
                username: row.get(1)?,
                is_admin: row.get::<_, i32>(2)? != 0,
                created_at: row.get(3)?,
                url_count: row.get(4)?,
            })
        })
        .map_err(|_| actix_web::error::ErrorInternalServerError("Database error"))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(HttpResponse::Ok().json(users))
}

/// Admin endpoint to delete a user
pub async fn admin_delete_user(
    data: web::Data<AppState>,
    user_id: web::Path<i64>,
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

    // Prevent admin from deleting themselves
    if claims.user_id == *user_id {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Cannot delete your own account"
        })));
    }

    let db = data.db.lock().unwrap();

    // Delete the user (CASCADE will handle related records)
    match db.execute("DELETE FROM users WHERE userID = ?1", params![*user_id]) {
        Ok(rows_affected) => {
            if rows_affected > 0 {
                Ok(HttpResponse::Ok().json(serde_json::json!({
                    "message": "User deleted successfully"
                })))
            } else {
                Ok(HttpResponse::NotFound().json(serde_json::json!({
                    "error": "User not found"
                })))
            }
        }
        Err(_) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Failed to delete user"
        }))),
    }
}

/// Admin endpoint to promote a user to admin
pub async fn admin_promote_user(
    data: web::Data<AppState>,
    user_id: web::Path<i64>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    let _claims = match get_claims(&http_req) {
        Some(c) => c,
        None => {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized"
            })));
        }
    };

    let db = data.db.lock().unwrap();

    // Check if user exists and is not already an admin
    let user_check: rusqlite::Result<(String, i32)> = db.query_row(
        "SELECT username, is_admin FROM users WHERE userID = ?1",
        params![*user_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    );

    match user_check {
        Ok((username, is_admin)) => {
            if is_admin != 0 {
                return Ok(HttpResponse::BadRequest().json(serde_json::json!({
                    "error": "User is already an admin"
                })));
            }

            // Promote user to admin
            match db.execute(
                "UPDATE users SET is_admin = 1 WHERE userID = ?1",
                params![*user_id],
            ) {
                Ok(_) => Ok(HttpResponse::Ok().json(serde_json::json!({
                    "message": format!("User '{}' promoted to admin successfully", username)
                }))),
                Err(_) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                    "error": "Failed to promote user"
                }))),
            }
        }
        Err(_) => Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "User not found"
        }))),
    }
}

/// Admin endpoint to get system statistics
pub async fn admin_get_stats(data: web::Data<AppState>) -> Result<HttpResponse> {
    let db = data.db.lock().unwrap();

    let total_users: i64 = db
        .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
        .unwrap_or(0);

    let total_urls: i64 = db
        .query_row("SELECT COUNT(*) FROM urls", [], |row| row.get(0))
        .unwrap_or(0);

    let total_clicks: i64 = db
        .query_row("SELECT SUM(clicks) FROM urls", [], |row| row.get(0))
        .unwrap_or(0);

    Ok(HttpResponse::Ok().json(AdminStatsResponse {
        total_users,
        total_urls,
        total_clicks,
    }))
}
