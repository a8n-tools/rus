use actix_web::{web, HttpMessage};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use crate::config::Config;
use crate::db::AppState;
use crate::auth::jwt::decode_jwt;

/// JWT validator middleware for protected routes
pub async fn jwt_validator(
    req: actix_web::dev::ServiceRequest,
    credentials: BearerAuth,
) -> Result<actix_web::dev::ServiceRequest, (actix_web::Error, actix_web::dev::ServiceRequest)> {
    let token = credentials.token();

    // Get the secret from app state
    let secret = req
        .app_data::<web::Data<AppState>>()
        .map(|state| state.config.jwt_secret.clone())
        .unwrap_or_else(Config::get_jwt_secret);

    match decode_jwt(token, &secret) {
        Ok(claims) => {
            req.extensions_mut().insert(claims);
            Ok(req)
        }
        Err(_) => Err((
            actix_web::error::ErrorUnauthorized("Invalid token"),
            req,
        )),
    }
}

/// Admin validator middleware (requires valid JWT with admin flag)
pub async fn admin_validator(
    req: actix_web::dev::ServiceRequest,
    credentials: BearerAuth,
) -> Result<actix_web::dev::ServiceRequest, (actix_web::Error, actix_web::dev::ServiceRequest)> {
    let token = credentials.token();

    // Get the secret from app state
    let secret = req
        .app_data::<web::Data<AppState>>()
        .map(|state| state.config.jwt_secret.clone())
        .unwrap_or_else(Config::get_jwt_secret);

    match decode_jwt(token, &secret) {
        Ok(claims) => {
            if !claims.is_admin {
                return Err((
                    actix_web::error::ErrorForbidden("Admin access required"),
                    req,
                ));
            }
            req.extensions_mut().insert(claims);
            Ok(req)
        }
        Err(_) => Err((
            actix_web::error::ErrorUnauthorized("Invalid token"),
            req,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, web, App, HttpResponse};
    use actix_web_httpauth::middleware::HttpAuthentication;
    use crate::testing::{make_test_state, make_test_token, insert_test_user};

    async fn dummy_handler() -> HttpResponse {
        HttpResponse::Ok().body("ok")
    }

    // --- jwt_validator ---

    #[actix_web::test]
    async fn jwt_validator_accepts_valid_token() {
        let state = make_test_state();
        let uid = insert_test_user(&state, "alice", false);
        let token = make_test_token("alice", uid, false);
        let auth = HttpAuthentication::bearer(jwt_validator);
        let app = test::init_service(
            App::new()
                .app_data(state)
                .service(web::scope("/api").wrap(auth).route("/test", web::get().to(dummy_handler))),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/api/test")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn jwt_validator_rejects_invalid_token() {
        let state = make_test_state();
        let auth = HttpAuthentication::bearer(jwt_validator);
        let app = test::init_service(
            App::new()
                .app_data(state)
                .service(web::scope("/api").wrap(auth).route("/test", web::get().to(dummy_handler))),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/api/test")
            .insert_header(("Authorization", "Bearer garbage.token.here"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn jwt_validator_rejects_missing_token() {
        let state = make_test_state();
        let auth = HttpAuthentication::bearer(jwt_validator);
        let app = test::init_service(
            App::new()
                .app_data(state)
                .service(web::scope("/api").wrap(auth).route("/test", web::get().to(dummy_handler))),
        )
        .await;

        let req = test::TestRequest::get().uri("/api/test").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn jwt_validator_inserts_claims_into_extensions() {
        let state = make_test_state();
        let uid = insert_test_user(&state, "alice", false);
        let token = make_test_token("alice", uid, false);
        let auth = HttpAuthentication::bearer(jwt_validator);

        async fn claims_handler(req: actix_web::HttpRequest) -> HttpResponse {
            let claims = crate::auth::get_claims(&req);
            match claims {
                Some(c) => HttpResponse::Ok().json(serde_json::json!({
                    "sub": c.sub,
                    "user_id": c.user_id,
                    "is_admin": c.is_admin,
                })),
                None => HttpResponse::InternalServerError().finish(),
            }
        }

        let app = test::init_service(
            App::new()
                .app_data(state)
                .service(web::scope("/api").wrap(auth).route("/test", web::get().to(claims_handler))),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/api/test")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let body: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(body["sub"], "alice");
        assert_eq!(body["user_id"], uid);
        assert_eq!(body["is_admin"], false);
    }

    // --- admin_validator ---

    #[actix_web::test]
    async fn admin_validator_accepts_admin_token() {
        let state = make_test_state();
        let uid = insert_test_user(&state, "admin", true);
        let token = make_test_token("admin", uid, true);
        let auth = HttpAuthentication::bearer(admin_validator);
        let app = test::init_service(
            App::new()
                .app_data(state)
                .service(web::scope("/admin").wrap(auth).route("/test", web::get().to(dummy_handler))),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/admin/test")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn admin_validator_rejects_non_admin_token() {
        let state = make_test_state();
        let uid = insert_test_user(&state, "alice", false);
        let token = make_test_token("alice", uid, false);
        let auth = HttpAuthentication::bearer(admin_validator);
        let app = test::init_service(
            App::new()
                .app_data(state)
                .service(web::scope("/admin").wrap(auth).route("/test", web::get().to(dummy_handler))),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/admin/test")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 403);
    }

    #[actix_web::test]
    async fn admin_validator_rejects_invalid_token() {
        let state = make_test_state();
        let auth = HttpAuthentication::bearer(admin_validator);
        let app = test::init_service(
            App::new()
                .app_data(state)
                .service(web::scope("/admin").wrap(auth).route("/test", web::get().to(dummy_handler))),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/admin/test")
            .insert_header(("Authorization", "Bearer garbage"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn admin_validator_inserts_admin_claims() {
        let state = make_test_state();
        let uid = insert_test_user(&state, "admin", true);
        let token = make_test_token("admin", uid, true);
        let auth = HttpAuthentication::bearer(admin_validator);

        async fn claims_handler(req: actix_web::HttpRequest) -> HttpResponse {
            let claims = crate::auth::get_claims(&req);
            match claims {
                Some(c) => HttpResponse::Ok().json(serde_json::json!({
                    "is_admin": c.is_admin,
                })),
                None => HttpResponse::InternalServerError().finish(),
            }
        }

        let app = test::init_service(
            App::new()
                .app_data(state)
                .service(web::scope("/admin").wrap(auth).route("/test", web::get().to(claims_handler))),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/admin/test")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let body: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(body["is_admin"], true);
    }

    #[actix_web::test]
    async fn jwt_validator_rejects_expired_token() {
        use jsonwebtoken::{encode, EncodingKey, Header};
        use crate::models::Claims;

        let state = make_test_state();
        let exp = (chrono::Utc::now() - chrono::Duration::hours(1)).timestamp() as usize;
        let claims = Claims {
            sub: "alice".into(),
            user_id: 1,
            is_admin: false,
            exp,
        };
        let expired_token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(crate::testing::TEST_JWT_SECRET.as_ref()),
        )
        .unwrap();

        let auth = HttpAuthentication::bearer(jwt_validator);
        let app = test::init_service(
            App::new()
                .app_data(state)
                .service(web::scope("/api").wrap(auth).route("/test", web::get().to(dummy_handler))),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/api/test")
            .insert_header(("Authorization", format!("Bearer {expired_token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }
}
