//! User accounts: registration, password auth, personal API tokens.
//! Ported from `app/models_user.py`. Passwords are hashed with bcrypt;
//! personal API tokens are random strings, only their SHA-256 hash is ever
//! stored.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use surrealdb::{Datetime, RecordId};

use crate::db::Db;
use crate::store;
use crate::error::{AppError, AppResult};
use crate::scopes;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct User {
    pub id: RecordId,
    pub email: String,
}

#[derive(Debug, Deserialize)]
struct UserRow {
    id: RecordId,
    email: String,
    password_hash: String,
}

pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

/// Expects an already-normalized email. bcrypt silently ignores everything
/// past 72 bytes, so longer passwords are rejected rather than truncated.
pub fn validate_credentials(email: &str, password: &str) -> AppResult<()> {
    let valid_email = match email.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
                && !email.contains(char::is_whitespace)
        }
        None => false,
    };
    if !valid_email {
        return Err(AppError::bad_request("Enter a valid email address."));
    }
    if password.chars().count() < 8 {
        return Err(AppError::bad_request("Password must be at least 8 characters."));
    }
    if password.len() > 72 {
        return Err(AppError::bad_request("Password must be at most 72 bytes."));
    }
    Ok(())
}

pub async fn register_user(db: &Db, email: &str, password: &str) -> AppResult<User> {
    let email = &normalize_email(email);
    validate_credentials(email, password)?;

    // Accounts created before emails were normalized may be mixed-case; the
    // unique index alone wouldn't catch "Alice@x.com" vs "alice@x.com".
    let mut res = store::app::AUTH_USER_ID_BY_EMAIL
        .on(db)
        .bind(("email", email.to_string()))
        .await?;
    #[derive(Deserialize)]
    struct IdRow {
        #[allow(dead_code)]
        id: RecordId,
    }
    let existing: Vec<IdRow> = res.take(0)?;
    if !existing.is_empty() {
        return Err(AppError::coded(crate::error::ErrorCode::AuthEmailTaken, "A user with that email already exists."));
    }

    let password_hash = bcrypt::hash(password, bcrypt::DEFAULT_COST)
        .map_err(|e| AppError::internal(e.to_string()))?;

    let mut res = store::app::AUTH_USER_CREATE
        .on(db)
        .bind(("email", email.to_string()))
        .bind(("password_hash", password_hash))
        .await?
        .check()
        .map_err(|e| {
            // the unique email index is the real guard; the SELECT above is only the friendly path
            if e.to_string().contains("already contains") {
                AppError::new(axum::http::StatusCode::CONFLICT, "A user with that email already exists.")
            } else {
                e.into()
            }
        })?;
    let rows: Vec<UserRow> = res.take(0)?;
    let row = rows.into_iter().next().ok_or_else(|| AppError::internal("insert returned no row"))?;

    crate::vaults::service::create_personal_vault(db, &row.id).await?;

    Ok(User { id: row.id, email: row.email })
}

pub async fn authenticate(db: &Db, email: &str, password: &str) -> AppResult<Option<User>> {
    // ponytail: string::lowercase() scan, no index -- add a normalized-email index if user counts get large
    let mut res = store::app::AUTH_USER_BY_EMAIL
        .on(db)
        .bind(("email", normalize_email(email)))
        .await?;
    let rows: Vec<UserRow> = res.take(0)?;
    let Some(row) = rows.into_iter().next() else { return Ok(None) };

    let ok = bcrypt::verify(password, &row.password_hash).unwrap_or(false);
    if !ok {
        return Ok(None);
    }
    Ok(Some(User { id: row.id, email: row.email }))
}

#[derive(Debug, Serialize)]
pub struct ApiTokenCreated {
    pub id: RecordId,
    pub name: String,
    pub token: String,
    pub scopes: Vec<String>,
    pub vault: Option<RecordId>,
    pub expires_at: Option<Datetime>,
}

#[derive(Debug, Deserialize)]
struct ApiTokenRow {
    id: RecordId,
    name: String,
}

/// A full-scope, non-expiring token, as every token was before scopes existed.
pub async fn create_api_token(db: &Db, owner: &RecordId, name: &str) -> AppResult<ApiTokenCreated> {
    create_api_token_with(db, owner, name, &all_scope_names(), None, None).await
}

pub async fn create_api_token_with(
    db: &Db,
    owner: &RecordId,
    name: &str,
    scopes: &[String],
    vault: Option<&RecordId>,
    expires_at: Option<Datetime>,
) -> AppResult<ApiTokenCreated> {
    let token = generate_token();
    let hash = hash_token(&token);

    let mut res = store::app::AUTH_TOKEN_CREATE
        .on(db)
        .bind(("owner", owner.clone()))
        .bind(("name", name.to_string()))
        .bind(("hash", hash))
        .bind(("scopes", scopes.to_vec()))
        .bind(("vault", vault.cloned()))
        .bind(("expires_at", expires_at.clone()))
        .await?;
    let rows: Vec<ApiTokenRow> = res.take(0)?;
    let row = rows.into_iter().next().ok_or_else(|| AppError::internal("insert returned no row"))?;

    Ok(ApiTokenCreated { id: row.id, name: row.name, token, scopes: scopes.to_vec(), vault: vault.cloned(), expires_at })
}

fn all_scope_names() -> Vec<String> {
    scopes::ALL.iter().map(|s| s.to_string()).collect()
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ApiTokenSummary {
    pub id: RecordId,
    pub name: String,
    pub created_at: Datetime,
    pub last_used_at: Option<Datetime>,
    #[serde(default = "all_scope_names")]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub vault: Option<RecordId>,
    #[serde(default)]
    pub expires_at: Option<Datetime>,
}

pub async fn list_api_tokens(db: &Db, owner: &RecordId) -> AppResult<Vec<ApiTokenSummary>> {
    let mut res = store::app::AUTH_TOKEN_LIST
        .on(db)
        .bind(("owner", owner.clone()))
        .await?;
    Ok(res.take(0)?)
}

pub async fn revoke_api_token(db: &Db, owner: &RecordId, token_id: &RecordId) -> AppResult<bool> {
    #[derive(Deserialize)]
    struct Row {
        owner: RecordId,
    }
    let row: Option<Row> = db.select(token_id.clone()).await?;
    let Some(row) = row else { return Ok(false) };
    if &row.owner != owner {
        return Ok(false);
    }
    let _: Option<Row> = db.delete(token_id.clone()).await?;
    Ok(true)
}

/// A token that exists, has not expired and belongs to a live user.
pub struct VerifiedToken {
    pub user: User,
    pub token_id: RecordId,
    pub scopes: Vec<String>,
    pub vault: Option<RecordId>,
}

pub enum TokenCheck {
    Unknown,
    Expired,
    Valid(Box<VerifiedToken>),
}

pub async fn check_api_token(db: &Db, token: &str) -> AppResult<TokenCheck> {
    #[derive(Deserialize)]
    struct TokenRow {
        id: RecordId,
        owner: RecordId,
        #[serde(default = "all_scope_names")]
        scopes: Vec<String>,
        #[serde(default)]
        vault: Option<RecordId>,
        #[serde(default)]
        expired: bool,
    }
    let hash = hash_token(token);
    let mut res = store::app::AUTH_TOKEN_BY_HASH
        .on(db)
        .bind(("hash", hash))
        .await?;
    let rows: Vec<TokenRow> = res.take(0)?;
    let Some(row) = rows.into_iter().next() else { return Ok(TokenCheck::Unknown) };

    if row.expired {
        return Ok(TokenCheck::Expired);
    }

    // Best-effort bump; failure here must not block auth.
    let _ = store::app::AUTH_TOKEN_TOUCH
        .on(db)
        .bind(("id", row.id.clone()))
        .await;

    let owner_row: Option<UserRow> = db.select(row.owner).await?;
    Ok(match owner_row {
        Some(r) => TokenCheck::Valid(Box::new(VerifiedToken {
            user: User { id: r.id, email: r.email },
            token_id: row.id,
            scopes: row.scopes.into_iter().filter(|s| scopes::is_known(s)).collect(),
            vault: row.vault,
        })),
        None => TokenCheck::Unknown,
    })
}

/// The token's user, or `None` if it is unknown or expired. Callers that need
/// the scopes or the expiry error use [`check_api_token`].
pub async fn verify_api_token(db: &Db, token: &str) -> AppResult<Option<User>> {
    Ok(match check_api_token(db, token).await? {
        TokenCheck::Valid(v) => Some(v.user),
        _ => None,
    })
}

/// Mirrors Python's `secrets.token_urlsafe(32)`: 32 random bytes, base64url, no padding.
pub fn generate_token() -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_email_trims_and_lowercases() {
        assert_eq!(normalize_email("  Alice@Example.COM "), "alice@example.com");
    }

    #[test]
    fn validate_credentials_accepts_reasonable_input() {
        assert!(validate_credentials("a@b.co", "password1").is_ok());
        assert!(validate_credentials("a@b.co", &"x".repeat(72)).is_ok());
    }

    #[test]
    fn validate_credentials_rejects_bad_emails() {
        for e in ["", "notanemail", "@b.co", "a@", "a@b", "a@.co", "a@b.", "a b@c.co"] {
            assert!(validate_credentials(e, "password1").is_err(), "accepted {e:?}");
        }
    }

    #[test]
    fn validate_credentials_rejects_short_and_overlong_passwords() {
        assert!(validate_credentials("a@b.co", "").is_err());
        assert!(validate_credentials("a@b.co", "1234567").is_err());
        assert!(validate_credentials("a@b.co", &"x".repeat(73)).is_err());
        // 8 multibyte chars is >= 8 characters even though it's 24 bytes.
        assert!(validate_credentials("a@b.co", "ééééééééé").is_ok());
    }

    #[test]
    fn hash_token_is_deterministic_and_sha256() {
        let a = hash_token("my-secret-token");
        let b = hash_token("my-secret-token");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
        assert_ne!(a, hash_token("different-token"));
    }

    #[test]
    fn generate_token_is_url_safe_and_unique() {
        let a = generate_token();
        let b = generate_token();
        assert_ne!(a, b);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }
}
