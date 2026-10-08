//! Authentication: a browser JWT cookie or a personal API token header, both
//! resolving to the same [`User`]. Ported from `app/auth.py`.

use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use surrealdb::RecordId;

use crate::db::Db;
use crate::store;
use crate::error::AppError;
use crate::models_user::{self, User};
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "eunomia_session";

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    email: String,
    sid: String,
}

/// Mirrors PyJWT's `jwt.decode` default: no `exp` claim is issued (sessions
/// are revoked server-side, not time-boxed), so don't require one.
fn validation() -> Validation {
    let mut v = Validation::new(Algorithm::HS256);
    v.required_spec_claims.clear();
    v.validate_exp = false;
    v
}

pub fn create_session_jwt(user: &User, sid: &str, secret: &str) -> Result<String, AppError> {
    let claims = Claims { sub: user.id.to_string(), email: user.email.clone(), sid: sid.to_string() };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
        .map_err(|e| AppError::internal(e.to_string()))
}

pub async fn start_session(
    db: &Db,
    secret: &str,
    user: &User,
    user_agent: Option<&str>,
) -> Result<String, AppError> {
    let sid = models_user::generate_token();
    let ua: String = user_agent.unwrap_or("").chars().take(300).collect();
    store::app::AUTH_SESSION_CREATE.on(db)
        .bind(("owner", user.id.clone()))
        .bind(("sid", sid.clone()))
        .bind(("user_agent", ua))
        .await?;
    create_session_jwt(user, &sid, secret)
}

async fn user_from_jwt(db: &Db, secret: &str, token: &str) -> Option<User> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation(),
    )
    .ok()?;
    let claims = data.claims;
    let rid: RecordId = claims.sub.parse().ok()?;

    #[derive(Deserialize)]
    struct SessionRow {
        id: RecordId,
    }
    let mut res = store::app::AUTH_SESSION_FIND
        .on(db)
        .bind(("sid", claims.sid.clone()))
        .bind(("owner", rid.clone()))
        .await
        .ok()?;
    let rows: Vec<SessionRow> = res.take(0).ok()?;
    let row = rows.into_iter().next()?;

    let _ = store::app::AUTH_SESSION_TOUCH
        .on(db)
        .bind(("id", row.id))
        .await;

    Some(User { id: rid, email: claims.email })
}

pub async fn revoke_session_by_jwt(db: &Db, secret: &str, token: &str) {
    let Ok(data) = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation(),
    ) else {
        return;
    };
    let _ = store::app::AUTH_SESSION_REVOKE_BY_SID
        .on(db)
        .bind(("sid", data.claims.sid))
        .await;
}

/// Axum extractor: resolves the authenticated user from a Bearer API token or
/// the session cookie. Rejects with 401 if neither is present/valid.
impl<S> FromRequestParts<S> for User
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);

        if let Some(auth_header) = parts.headers.get(axum::http::header::AUTHORIZATION)
            && let Ok(value) = auth_header.to_str()
                && let Some(token) = value.strip_prefix("Bearer ").or_else(|| value.strip_prefix("bearer "))
                    && let Some(user) = models_user::verify_api_token(&app_state.db, token).await? {
                        crate::telemetry::record_user(&user.id.to_string());
                        return Ok(user);
                    }

        if let Some(cookie_header) = parts.headers.get(axum::http::header::COOKIE)
            && let Ok(cookie_str) = cookie_header.to_str()
                && let Some(session_token) = extract_cookie(cookie_str, SESSION_COOKIE)
                    && let Some(user) =
                        user_from_jwt(&app_state.db, &app_state.settings.jwt_secret, &session_token).await
                    {
                        crate::telemetry::record_user(&user.id.to_string());
                        return Ok(user);
                    }

        Err(AppError::unauthorized("Not authenticated."))
    }
}

fn extract_cookie(header: &str, name: &str) -> Option<String> {
    header.split(';').find_map(|part| {
        let part = part.trim();
        let (key, value) = part.split_once('=')?;
        (key == name).then(|| value.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_user() -> User {
        User { id: "user:abc123".parse().unwrap(), email: "a@example.com".to_string() }
    }

    #[test]
    fn jwt_roundtrips_claims() {
        let user = test_user();
        let token = create_session_jwt(&user, "sid-1", "test-secret").unwrap();
        let data = decode::<Claims>(
            &token,
            &DecodingKey::from_secret(b"test-secret"),
            &validation(),
        )
        .unwrap();
        assert_eq!(data.claims.sub, user.id.to_string());
        assert_eq!(data.claims.email, user.email);
        assert_eq!(data.claims.sid, "sid-1");
    }

    #[test]
    fn jwt_rejects_wrong_secret() {
        let user = test_user();
        let token = create_session_jwt(&user, "sid-1", "test-secret").unwrap();
        let result = decode::<Claims>(
            &token,
            &DecodingKey::from_secret(b"wrong-secret"),
            &validation(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn extract_cookie_finds_named_cookie_among_several() {
        let header = "foo=bar; eunomia_session=the-token; baz=qux";
        assert_eq!(extract_cookie(header, SESSION_COOKIE), Some("the-token".to_string()));
    }

    #[test]
    fn extract_cookie_returns_none_when_absent() {
        let header = "foo=bar; baz=qux";
        assert_eq!(extract_cookie(header, SESSION_COOKIE), None);
    }
}
