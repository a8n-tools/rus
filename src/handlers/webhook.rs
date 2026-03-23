use actix_web::{web, HttpRequest, HttpResponse, Result};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::sync::atomic::Ordering;

use crate::db::AppState;

type HmacSha256 = Hmac<Sha256>;

#[derive(serde::Deserialize)]
struct MaintenancePayload {
    event: String,
    #[allow(dead_code)]
    slug: Option<String>,
    maintenance_mode: bool,
    maintenance_message: Option<String>,
    #[allow(dead_code)]
    timestamp: Option<String>,
}

pub async fn handle_maintenance_webhook(
    req: HttpRequest,
    body: web::Bytes,
    state: web::Data<AppState>,
) -> Result<HttpResponse> {
    // Read signature header
    let signature = match req.headers().get("X-Webhook-Signature") {
        Some(val) => val
            .to_str()
            .map_err(|_| actix_web::error::ErrorUnauthorized("Invalid signature header"))?,
        None => {
            return Err(actix_web::error::ErrorUnauthorized(
                "Missing X-Webhook-Signature header",
            ))
        }
    };

    // Compute HMAC-SHA256 of raw body
    let mut mac = HmacSha256::new_from_slice(state.config.saas_jwt_secret.as_bytes())
        .map_err(|_| actix_web::error::ErrorInternalServerError("HMAC key error"))?;
    mac.update(&body);

    // Decode the hex signature and verify
    let sig_bytes =
        hex::decode(signature).map_err(|_| actix_web::error::ErrorUnauthorized("Invalid signature format"))?;
    mac.verify_slice(&sig_bytes)
        .map_err(|_| actix_web::error::ErrorUnauthorized("Invalid signature"))?;

    // Deserialize payload
    let payload: MaintenancePayload = serde_json::from_slice(&body)
        .map_err(|e| actix_web::error::ErrorBadRequest(format!("Invalid payload: {e}")))?;

    // Validate event type
    if payload.event != "maintenance_mode_changed" {
        return Err(actix_web::error::ErrorBadRequest(format!(
            "Unsupported event: {}",
            payload.event
        )));
    }

    // Update state
    state
        .maintenance_mode
        .store(payload.maintenance_mode, Ordering::SeqCst);
    {
        let mut msg = state.maintenance_message.write().unwrap();
        *msg = if payload.maintenance_mode {
            payload.maintenance_message
        } else {
            None
        };
    }

    tracing::info!(maintenance_mode = payload.maintenance_mode, "Maintenance mode updated via webhook");

    Ok(HttpResponse::Ok().json(serde_json::json!({ "status": "ok" })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, web, App};
    use crate::testing::{make_test_state, sign_webhook_payload, TEST_SAAS_SECRET};
    use serde_json::json;

    fn build_app(
        state: web::Data<AppState>,
    ) -> impl std::future::Future<
        Output = impl actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<impl actix_web::body::MessageBody>,
            Error = actix_web::Error,
        >,
    > {
        test::init_service(
            App::new()
                .app_data(state)
                .route(
                    "/webhooks/maintenance",
                    web::post().to(handle_maintenance_webhook),
                ),
        )
    }

    #[actix_web::test]
    async fn valid_signature_enables_maintenance() {
        let state = make_test_state();
        let app = build_app(state.clone()).await;

        let payload = json!({
            "event": "maintenance_mode_changed",
            "slug": "rus",
            "maintenance_mode": true,
            "maintenance_message": "Upgrading database",
            "timestamp": "2026-03-12T15:30:45Z"
        });
        let body = serde_json::to_vec(&payload).unwrap();
        let sig = sign_webhook_payload(&body, TEST_SAAS_SECRET);

        let req = test::TestRequest::post()
            .uri("/webhooks/maintenance")
            .insert_header(("X-Webhook-Signature", sig))
            .insert_header(("Content-Type", "application/json"))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        assert!(state.maintenance_mode.load(Ordering::SeqCst));
        assert_eq!(
            *state.maintenance_message.read().unwrap(),
            Some("Upgrading database".to_string())
        );
    }

    #[actix_web::test]
    async fn valid_signature_disables_maintenance() {
        let state = make_test_state();
        // Pre-enable maintenance
        state.maintenance_mode.store(true, Ordering::SeqCst);
        *state.maintenance_message.write().unwrap() = Some("down".to_string());

        let app = build_app(state.clone()).await;

        let payload = json!({
            "event": "maintenance_mode_changed",
            "slug": "rus",
            "maintenance_mode": false,
            "timestamp": "2026-03-12T16:00:00Z"
        });
        let body = serde_json::to_vec(&payload).unwrap();
        let sig = sign_webhook_payload(&body, TEST_SAAS_SECRET);

        let req = test::TestRequest::post()
            .uri("/webhooks/maintenance")
            .insert_header(("X-Webhook-Signature", sig))
            .insert_header(("Content-Type", "application/json"))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        assert!(!state.maintenance_mode.load(Ordering::SeqCst));
        assert_eq!(*state.maintenance_message.read().unwrap(), None);
    }

    #[actix_web::test]
    async fn missing_signature_returns_401() {
        let state = make_test_state();
        let app = build_app(state).await;

        let payload = json!({"event": "maintenance_mode_changed", "maintenance_mode": true});
        let body = serde_json::to_vec(&payload).unwrap();

        let req = test::TestRequest::post()
            .uri("/webhooks/maintenance")
            .insert_header(("Content-Type", "application/json"))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn invalid_signature_returns_401() {
        let state = make_test_state();
        let app = build_app(state).await;

        let payload = json!({"event": "maintenance_mode_changed", "maintenance_mode": true});
        let body = serde_json::to_vec(&payload).unwrap();

        let req = test::TestRequest::post()
            .uri("/webhooks/maintenance")
            .insert_header(("X-Webhook-Signature", "deadbeef"))
            .insert_header(("Content-Type", "application/json"))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn wrong_event_type_returns_400() {
        let state = make_test_state();
        let app = build_app(state).await;

        let payload = json!({"event": "user_deleted", "maintenance_mode": true});
        let body = serde_json::to_vec(&payload).unwrap();
        let sig = sign_webhook_payload(&body, TEST_SAAS_SECRET);

        let req = test::TestRequest::post()
            .uri("/webhooks/maintenance")
            .insert_header(("X-Webhook-Signature", sig))
            .insert_header(("Content-Type", "application/json"))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn malformed_json_body_returns_400() {
        let state = make_test_state();
        let app = build_app(state).await;

        let body = b"this is not json";
        let sig = sign_webhook_payload(body, TEST_SAAS_SECRET);

        let req = test::TestRequest::post()
            .uri("/webhooks/maintenance")
            .insert_header(("X-Webhook-Signature", sig))
            .insert_header(("Content-Type", "application/json"))
            .set_payload(body.to_vec())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn maintenance_message_cleared_on_disable() {
        let state = make_test_state();
        let app = build_app(state.clone()).await;

        // Enable maintenance with a message
        let enable_payload = json!({
            "event": "maintenance_mode_changed",
            "maintenance_mode": true,
            "maintenance_message": "Upgrading"
        });
        let enable_body = serde_json::to_vec(&enable_payload).unwrap();
        let enable_sig = sign_webhook_payload(&enable_body, TEST_SAAS_SECRET);

        test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/webhooks/maintenance")
                .insert_header(("X-Webhook-Signature", enable_sig))
                .insert_header(("Content-Type", "application/json"))
                .set_payload(enable_body)
                .to_request(),
        )
        .await;
        assert!(state.maintenance_mode.load(Ordering::SeqCst));
        assert_eq!(
            *state.maintenance_message.read().unwrap(),
            Some("Upgrading".to_string())
        );

        // Disable maintenance
        let disable_payload = json!({
            "event": "maintenance_mode_changed",
            "maintenance_mode": false
        });
        let disable_body = serde_json::to_vec(&disable_payload).unwrap();
        let disable_sig = sign_webhook_payload(&disable_body, TEST_SAAS_SECRET);

        test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/webhooks/maintenance")
                .insert_header(("X-Webhook-Signature", disable_sig))
                .insert_header(("Content-Type", "application/json"))
                .set_payload(disable_body)
                .to_request(),
        )
        .await;
        assert!(!state.maintenance_mode.load(Ordering::SeqCst));
        assert_eq!(*state.maintenance_message.read().unwrap(), None);
    }
}
