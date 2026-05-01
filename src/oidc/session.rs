//! Local BFF session - opaque random token in an HttpOnly cookie, hashed
//! before storage in `user_sessions`.

use actix_web::{
    body::MessageBody,
    dev::{Payload, ServiceRequest, ServiceResponse},
    error::ErrorUnauthorized,
    middleware::Next,
    web, FromRequest, HttpMessage, HttpRequest,
};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::future::{ready, Ready};

use crate::db::AppState;

pub const RUS_SESSION_COOKIE: &str = "rus_session";

/// Identity attached to the request after `AuthenticatedUser` resolves.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AuthenticatedUser {
    pub user_id: i64,
    pub username: String,
    pub is_admin: bool,
    pub auth_via_oidc: bool,
}

pub fn hash_session_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

/// Resolve a raw session cookie value to `AuthenticatedUser`, applying expiry,
/// `session_version` and `suspended_at` checks.
///
/// Returns `Ok(None)` when the session is missing, expired, or invalidated;
/// returns `Err(_)` only on actual database failures.
pub fn lookup_session(
    db: &Connection,
    session_token: &str,
) -> rusqlite::Result<Option<AuthenticatedUser>> {
    let token_hash = hash_session_token(session_token);

    type SessionRow = (i64, String, i32, i32, String, i32, Option<String>, i32);
    let row: Option<SessionRow> = db
        .query_row(
            "SELECT us.user_id, u.username, u.is_admin, us.auth_via_oidc,
                    us.expires_at, us.session_version,
                    u.suspended_at, u.session_version
             FROM user_sessions us
             JOIN users u ON u.userID = us.user_id
             WHERE us.session_token_hash = ?1",
            params![token_hash.as_slice()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i32>(2)?,
                    row.get::<_, i32>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i32>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, i32>(7)?,
                ))
            },
        )
        .optional()?;

    let Some((
        user_id,
        username,
        is_admin,
        auth_via_oidc,
        expires_at,
        session_version,
        suspended_at,
        user_session_version,
    )) = row
    else {
        return Ok(None);
    };

    // expires_at is RFC3339; chrono parses it.
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(&expires_at) {
        if parsed.with_timezone(&Utc) < Utc::now() {
            return Ok(None);
        }
    }

    if session_version != user_session_version {
        return Ok(None);
    }
    if suspended_at.is_some() {
        return Ok(None);
    }

    Ok(Some(AuthenticatedUser {
        user_id,
        username,
        is_admin: is_admin != 0,
        auth_via_oidc: auth_via_oidc != 0,
    }))
}

/// Middleware: require a valid OIDC BFF session for the wrapped scope.
/// On failure returns 401 (JSON for `/api/*`, plain otherwise).
pub async fn require_session(
    req: ServiceRequest,
    next: Next<impl MessageBody + 'static>,
) -> Result<ServiceResponse<actix_web::body::BoxBody>, actix_web::Error> {
    let state = req
        .app_data::<web::Data<AppState>>()
        .expect("AppState not found")
        .clone();

    let user = req
        .request()
        .cookie(RUS_SESSION_COOKIE)
        .and_then(|c| {
            let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
            lookup_session(&db, c.value()).ok().flatten()
        });

    match user {
        Some(u) => {
            req.extensions_mut().insert(u);
            Ok(next.call(req).await?.map_into_boxed_body())
        }
        None => Ok(req.into_response(
            actix_web::HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized",
                "redirect": "/oauth2/login",
            })),
        )),
    }
}

impl FromRequest for AuthenticatedUser {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        // Already inserted by an upstream middleware.
        if let Some(u) = req.extensions().get::<AuthenticatedUser>().cloned() {
            return ready(Ok(u));
        }

        let Some(cookie) = req.cookie(RUS_SESSION_COOKIE) else {
            return ready(Err(ErrorUnauthorized("missing session cookie")));
        };

        let Some(state) = req.app_data::<web::Data<AppState>>() else {
            return ready(Err(actix_web::error::ErrorInternalServerError(
                "AppState missing",
            )));
        };

        let resolved = {
            let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
            lookup_session(&db, cookie.value())
        };

        match resolved {
            Ok(Some(user)) => {
                req.extensions_mut().insert(user.clone());
                ready(Ok(user))
            }
            Ok(None) => ready(Err(ErrorUnauthorized("invalid or expired session"))),
            Err(e) => {
                tracing::warn!(error = %e, "session lookup failed");
                ready(Err(actix_web::error::ErrorInternalServerError(
                    "session lookup failed",
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{insert_saas_user, make_saas_session, make_test_state};

    const SUB: &str = "11111111-1111-1111-1111-111111111111";

    #[test]
    fn lookup_returns_user_for_valid_session() {
        let state = make_test_state();
        let uid = insert_saas_user(&state, "alice", SUB, false);
        let token = make_saas_session(&state, uid);
        let db = state.db.lock().unwrap();
        let user = lookup_session(&db, &token).unwrap().expect("session valid");
        assert_eq!(user.user_id, uid);
        assert_eq!(user.username, "alice");
        assert!(!user.is_admin);
        assert!(user.auth_via_oidc);
    }

    #[test]
    fn lookup_returns_none_for_unknown_token() {
        let state = make_test_state();
        let db = state.db.lock().unwrap();
        assert!(lookup_session(&db, "totally-fake-token").unwrap().is_none());
    }

    #[test]
    fn lookup_uses_hash_not_plaintext() {
        let state = make_test_state();
        let uid = insert_saas_user(&state, "alice", SUB, false);
        let token = make_saas_session(&state, uid);
        let db = state.db.lock().unwrap();
        // Plaintext token should NEVER appear in the DB.
        let stored: Vec<u8> = db
            .query_row(
                "SELECT session_token_hash FROM user_sessions WHERE user_id = ?1",
                rusqlite::params![uid],
                |r| r.get(0),
            )
            .unwrap();
        assert_ne!(stored.as_slice(), token.as_bytes());
        assert_eq!(stored.len(), 32, "must be SHA-256 = 32 bytes");
    }

    #[test]
    fn lookup_returns_none_when_session_expired() {
        let state = make_test_state();
        let uid = insert_saas_user(&state, "alice", SUB, false);
        let token = make_saas_session(&state, uid);
        let db = state.db.lock().unwrap();
        // Expire the session
        db.execute(
            "UPDATE user_sessions SET expires_at = '2020-01-01T00:00:00Z' WHERE user_id = ?1",
            rusqlite::params![uid],
        )
        .unwrap();
        assert!(lookup_session(&db, &token).unwrap().is_none());
    }

    #[test]
    fn lookup_returns_none_on_session_version_mismatch() {
        let state = make_test_state();
        let uid = insert_saas_user(&state, "alice", SUB, false);
        let token = make_saas_session(&state, uid);
        let db = state.db.lock().unwrap();
        // Bump the user's session_version (simulate back-channel logout)
        db.execute(
            "UPDATE users SET session_version = session_version + 1 WHERE userID = ?1",
            rusqlite::params![uid],
        )
        .unwrap();
        assert!(lookup_session(&db, &token).unwrap().is_none());
    }

    #[test]
    fn lookup_returns_none_when_user_suspended() {
        let state = make_test_state();
        let uid = insert_saas_user(&state, "alice", SUB, false);
        let token = make_saas_session(&state, uid);
        let db = state.db.lock().unwrap();
        db.execute(
            "UPDATE users SET suspended_at = '2026-01-01T00:00:00Z' WHERE userID = ?1",
            rusqlite::params![uid],
        )
        .unwrap();
        assert!(lookup_session(&db, &token).unwrap().is_none());
    }

    #[test]
    fn admin_flag_propagates() {
        let state = make_test_state();
        let uid = insert_saas_user(&state, "admin", SUB, true);
        let token = make_saas_session(&state, uid);
        let db = state.db.lock().unwrap();
        let user = lookup_session(&db, &token).unwrap().unwrap();
        assert!(user.is_admin);
    }

    #[actix_web::test]
    async fn require_session_middleware_rejects_no_cookie() {
        use actix_web::{test, web, App};
        let state = make_test_state();
        let app = test::init_service(
            App::new().app_data(state).service(
                web::scope("/api")
                    .wrap(actix_web::middleware::from_fn(require_session))
                    .route("/ping", web::get().to(|| async { "pong" })),
            ),
        )
        .await;
        let req = test::TestRequest::get().uri("/api/ping").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn require_session_middleware_admits_valid_session() {
        use actix_web::{test, web, App};
        let state = make_test_state();
        let uid = insert_saas_user(&state, "alice", SUB, false);
        let token = make_saas_session(&state, uid);
        let app = test::init_service(
            App::new().app_data(state.clone()).service(
                web::scope("/api")
                    .wrap(actix_web::middleware::from_fn(require_session))
                    .route("/ping", web::get().to(|| async { "pong" })),
            ),
        )
        .await;
        let req = test::TestRequest::get()
            .uri("/api/ping")
            .insert_header(("Cookie", format!("{RUS_SESSION_COOKIE}={token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }
}
