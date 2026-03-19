use actix_web::{web, HttpRequest, HttpResponse, Result};
use rusqlite::params;
use tracing::{info, warn};

use crate::auth::get_claims;
use crate::db::AppState;
use crate::models::{AdminStatsResponse, UserInfo};

/// Admin endpoint to list all users
pub async fn admin_list_users(data: web::Data<AppState>) -> Result<HttpResponse> {
    let db = data.db.lock().unwrap_or_else(|e| e.into_inner());

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
        warn!(admin_user_id = claims.user_id, "Admin attempted to delete own account");
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Cannot delete your own account"
        })));
    }

    let db = data.db.lock().unwrap_or_else(|e| e.into_inner());

    // Delete the user (CASCADE will handle related records)
    match db.execute("DELETE FROM users WHERE userID = ?1", params![*user_id]) {
        Ok(rows_affected) => {
            if rows_affected > 0 {
                info!(admin_user_id = claims.user_id, deleted_user_id = *user_id, "Admin deleted user");
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

    let db = data.db.lock().unwrap_or_else(|e| e.into_inner());

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
                Ok(_) => {
                    info!(admin_user_id = _claims.user_id, promoted_user_id = *user_id, username = %username, "Admin promoted user to admin");
                    Ok(HttpResponse::Ok().json(serde_json::json!({
                        "message": format!("User '{}' promoted to admin successfully", username)
                    })))
                }
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
    let db = data.db.lock().unwrap_or_else(|e| e.into_inner());

    let total_users: i64 = db
        .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
        .unwrap_or(0);

    let total_urls: i64 = db
        .query_row("SELECT COUNT(*) FROM urls", [], |row| row.get(0))
        .unwrap_or(0);

    let total_clicks: i64 = db
        .query_row("SELECT COALESCE(SUM(clicks), 0) FROM urls", [], |row| row.get(0))
        .unwrap_or(0);

    Ok(HttpResponse::Ok().json(AdminStatsResponse {
        total_users,
        total_urls,
        total_clicks,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};
    use actix_web_httpauth::middleware::HttpAuthentication;
    use crate::auth::middleware::admin_validator;
    use crate::testing::{insert_test_url, insert_test_user, make_test_state, make_test_token};
    use serde_json::Value;

    macro_rules! setup_app {
        ($state:expr) => {{
            let admin_auth = HttpAuthentication::bearer(admin_validator);
            test::init_service(
                App::new()
                    .app_data($state.clone())
                    .service(
                        web::scope("/api/admin")
                            .wrap(admin_auth)
                            .route("/users", web::get().to(admin_list_users))
                            .route("/users/{user_id}", web::delete().to(admin_delete_user))
                            .route("/users/{user_id}/promote", web::post().to(admin_promote_user))
                            .route("/stats", web::get().to(admin_get_stats)),
                    ),
            )
            .await
        }};
    }

    // --- admin_list_users ---

    #[actix_web::test]
    async fn list_users_returns_all_users() {
        let state = make_test_state();
        insert_test_user(&state, "alice", true);
        insert_test_user(&state, "bob", false);
        let token = make_test_token("alice", 1, true);
        let app = setup_app!(state);

        let req = test::TestRequest::get()
            .uri("/api/admin/users")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let body: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(body.as_array().unwrap().len(), 2);
    }

    #[actix_web::test]
    async fn list_users_non_admin_returns_403() {
        let state = make_test_state();
        let uid = insert_test_user(&state, "alice", false);
        let token = make_test_token("alice", uid, false);
        let app = setup_app!(state);

        let req = test::TestRequest::get()
            .uri("/api/admin/users")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 403);
    }

    #[actix_web::test]
    async fn list_users_unauthenticated_returns_401() {
        let state = make_test_state();
        let app = setup_app!(state);
        let req = test::TestRequest::get()
            .uri("/api/admin/users")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn list_users_includes_url_count() {
        let state = make_test_state();
        let uid = insert_test_user(&state, "alice", true);
        insert_test_url(&state, uid, "https://example.com", "abc123");
        let token = make_test_token("alice", uid, true);
        let app = setup_app!(state);

        let req = test::TestRequest::get()
            .uri("/api/admin/users")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let body: Value = test::call_and_read_body_json(&app, req).await;
        let alice = &body.as_array().unwrap()[0];
        assert_eq!(alice["url_count"], 1);
    }

    // --- admin_delete_user ---

    #[actix_web::test]
    async fn delete_user_success() {
        let state = make_test_state();
        let uid_admin = insert_test_user(&state, "admin", true);
        let uid_bob = insert_test_user(&state, "bob", false);
        let token = make_test_token("admin", uid_admin, true);
        let app = setup_app!(state);

        let req = test::TestRequest::delete()
            .uri(&format!("/api/admin/users/{uid_bob}"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        // Verify user is gone
        let count: i64 = {
            let db = state.db.lock().unwrap();
            db.query_row("SELECT COUNT(*) FROM users WHERE userID=?1", [uid_bob], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(count, 0);
    }

    #[actix_web::test]
    async fn delete_user_cannot_delete_self() {
        let state = make_test_state();
        let uid = insert_test_user(&state, "admin", true);
        let token = make_test_token("admin", uid, true);
        let app = setup_app!(state);

        let req = test::TestRequest::delete()
            .uri(&format!("/api/admin/users/{uid}"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn delete_user_cascades_their_urls() {
        let state = make_test_state();
        let uid_admin = insert_test_user(&state, "admin", true);
        let uid_bob = insert_test_user(&state, "bob", false);
        insert_test_url(&state, uid_bob, "https://bob.com", "bbb222");
        let token = make_test_token("admin", uid_admin, true);
        let app = setup_app!(state);

        test::call_service(
            &app,
            test::TestRequest::delete()
                .uri(&format!("/api/admin/users/{uid_bob}"))
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request(),
        )
        .await;

        let url_count: i64 = {
            let db = state.db.lock().unwrap();
            db.query_row("SELECT COUNT(*) FROM urls WHERE user_id=?1", [uid_bob], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(url_count, 0);
    }

    // --- admin_promote_user ---

    #[actix_web::test]
    async fn promote_user_to_admin() {
        let state = make_test_state();
        let uid_admin = insert_test_user(&state, "admin", true);
        let uid_bob = insert_test_user(&state, "bob", false);
        let token = make_test_token("admin", uid_admin, true);
        let app = setup_app!(state);

        let req = test::TestRequest::post()
            .uri(&format!("/api/admin/users/{uid_bob}/promote"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        let is_admin: i32 = {
            let db = state.db.lock().unwrap();
            db.query_row(
                "SELECT is_admin FROM users WHERE userID=?1",
                [uid_bob],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(is_admin, 1);
    }

    #[actix_web::test]
    async fn promote_already_admin_returns_400() {
        let state = make_test_state();
        let uid = insert_test_user(&state, "admin", true);
        let token = make_test_token("admin", uid, true);
        let app = setup_app!(state);

        let req = test::TestRequest::post()
            .uri(&format!("/api/admin/users/{uid}/promote"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn promote_nonexistent_user_returns_404() {
        let state = make_test_state();
        let uid = insert_test_user(&state, "admin", true);
        let token = make_test_token("admin", uid, true);
        let app = setup_app!(state);

        let req = test::TestRequest::post()
            .uri("/api/admin/users/9999/promote")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);
    }

    // --- admin_get_stats ---

    #[actix_web::test]
    async fn stats_returns_correct_counts() {
        let state = make_test_state();
        let uid = insert_test_user(&state, "admin", true);
        insert_test_user(&state, "bob", false);
        insert_test_url(&state, uid, "https://example.com", "abc123");
        let token = make_test_token("admin", uid, true);
        let app = setup_app!(state);

        let req = test::TestRequest::get()
            .uri("/api/admin/stats")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let body: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(body["total_users"], 2);
        assert_eq!(body["total_urls"], 1);
        assert_eq!(body["total_clicks"], 0);
    }
}
