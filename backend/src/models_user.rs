//! User accounts: registration, password auth, personal API tokens.
//! Ported from `app/models_user.py`. Passwords are hashed with bcrypt;
//! personal API tokens are random strings, only their SHA-256 hash is ever
//! stored.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use surrealdb::RecordId;

use crate::db::Db;
use crate::error::{AppError, AppResult};

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

pub async fn register_user(db: &Db, email: &str, password: &str) -> AppResult<User> {
    let password_hash = bcrypt::hash(password, bcrypt::DEFAULT_COST)
        .map_err(|e| AppError::internal(e.to_string()))?;

    let mut res = db
        .query("CREATE user SET email = $email, password_hash = $password_hash RETURN AFTER")
        .bind(("email", email.to_string()))
        .bind(("password_hash", password_hash))
        .await?;
    let rows: Vec<UserRow> = res.take(0)?;
    let row = rows.into_iter().next().ok_or_else(|| AppError::internal("insert returned no row"))?;

    crate::vaults::service::create_personal_vault(db, &row.id).await?;

    Ok(User { id: row.id, email: row.email })
}

pub async fn authenticate(db: &Db, email: &str, password: &str) -> AppResult<Option<User>> {
    let mut res = db
        .query("SELECT * FROM user WHERE email = $email LIMIT 1")
        .bind(("email", email.to_string()))
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
}

#[derive(Debug, Deserialize)]
struct ApiTokenRow {
    id: RecordId,
    name: String,
}

pub async fn create_api_token(db: &Db, owner: &RecordId, name: &str) -> AppResult<ApiTokenCreated> {
    let token = generate_token();
    let hash = hash_token(&token);

    let mut res = db
        .query("CREATE api_token SET owner = $owner, name = $name, token_hash = $hash RETURN AFTER")
        .bind(("owner", owner.clone()))
        .bind(("name", name.to_string()))
        .bind(("hash", hash))
        .await?;
    let rows: Vec<ApiTokenRow> = res.take(0)?;
    let row = rows.into_iter().next().ok_or_else(|| AppError::internal("insert returned no row"))?;

    Ok(ApiTokenCreated { id: row.id, name: row.name, token })
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ApiTokenSummary {
    pub id: RecordId,
    pub name: String,
    pub created_at: surrealdb::Datetime,
    pub last_used_at: Option<surrealdb::Datetime>,
}

pub async fn list_api_tokens(db: &Db, owner: &RecordId) -> AppResult<Vec<ApiTokenSummary>> {
    let mut res = db
        .query(
            "SELECT id, name, created_at, last_used_at FROM api_token \
             WHERE owner = $owner ORDER BY created_at DESC",
        )
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

pub async fn verify_api_token(db: &Db, token: &str) -> AppResult<Option<User>> {
    #[derive(Deserialize)]
    struct TokenRow {
        id: RecordId,
        owner: RecordId,
    }
    let hash = hash_token(token);
    let mut res = db
        .query("SELECT * FROM api_token WHERE token_hash = $hash LIMIT 1")
        .bind(("hash", hash))
        .await?;
    let rows: Vec<TokenRow> = res.take(0)?;
    let Some(row) = rows.into_iter().next() else { return Ok(None) };

    // Best-effort bump; failure here must not block auth.
    let _ = db
        .query("UPDATE $id SET last_used_at = time::now()")
        .bind(("id", row.id))
        .await;

    let owner_row: Option<UserRow> = db.select(row.owner).await?;
    Ok(owner_row.map(|r| User { id: r.id, email: r.email }))
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
