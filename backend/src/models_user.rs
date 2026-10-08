//! User accounts: registration, password auth, personal API tokens.
//! Ported from `app/models_user.py`. Passwords are hashed with bcrypt;
//! personal API tokens are random strings, only their SHA-256 hash is ever
//! stored.

use surrealdb::types::SurrealValue;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use surrealdb::types::{Datetime, RecordId};

use crate::pool::{ControlDb, OrgId};
use crate::state::AppState;
use crate::store;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::scopes;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct User {
    pub id: RecordId,
    pub email: String,
    /// The org whose database this user's requests run against: their oldest membership.
    pub org: OrgId,
}

#[derive(Debug, Deserialize, SurrealValue)]
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

/// The user's home org (their oldest membership).
pub async fn org_of(control: &ControlDb, user: &RecordId) -> AppResult<OrgId> {
    #[derive(Deserialize, SurrealValue)]
    struct Row {
        org: RecordId,
    }
    let mut res = store::control::ORG_OF_USER.on(control).bind(("user", user.clone())).await?;
    let row: Option<Row> = res.take::<Vec<Row>>(0)?.into_iter().next();
    row.and_then(|r| crate::rid::key_string(&r.org.key).and_then(|k| OrgId::parse(&k)))
        .ok_or_else(|| AppError::coded(ErrorCode::TenantNotFound, "This account does not belong to an organisation."))
}

/// Fills in a user's org for a credential that resolved to `id` and `email`.
pub async fn load_user(control: &ControlDb, id: RecordId, email: String) -> AppResult<User> {
    let org = org_of(control, &id).await?;
    Ok(User { id, email, org })
}

/// `EUNOMIA_SIGNUP_ORG`: `join` (default) puts a new user in the install's one org, as a member;
/// `personal` gives every signup an org of their own. The first user of a fresh install always
/// creates the instance's org (`Default`) and owns it.
fn signup_personal() -> bool {
    std::env::var("EUNOMIA_SIGNUP_ORG").is_ok_and(|v| v.eq_ignore_ascii_case("personal"))
}

#[derive(Deserialize, SurrealValue)]
struct TenantListRow {
    org: RecordId,
    status: String,
}

async fn assign_org(state: &AppState, user: &RecordId, email: &str, personal: bool) -> AppResult<OrgId> {
    // ponytail: one signup at a time per process; org creation is rare and the lock keeps "first user creates the org" true.
    let _one_at_a_time = crate::tx::lock("org.signup").await;
    let mut res = store::tenant::LIST.on(&state.control).await?;
    let rows: Vec<TenantListRow> = res.take(0)?;
    let existing = rows
        .into_iter()
        .filter(|r| r.status == "ready")
        .find_map(|r| crate::rid::key_string(&r.org.key).and_then(|k| OrgId::parse(&k)));
    let (org, role) = match existing {
        Some(org) if !personal => (org, "member"),
        found => {
            let Some(p) = &state.provisioner else {
                return Err(AppError::coded(ErrorCode::TenantProvisioningDisabled, "This server cannot create organisations."));
            };
            let org = OrgId::new();
            p.provision_org(&state.control, org, if found.is_none() { "Default" } else { email }).await?;
            (org, "owner")
        }
    };
    store::control::MEMBERSHIP_ADD
        .on(&state.control)
        .bind(("user", user.clone()))
        .bind(("org", org.record()))
        .bind(("role", role))
        .await?
        .check()?;
    Ok(org)
}

pub async fn register_user(state: &AppState, email: &str, password: &str) -> AppResult<User> {
    register_user_with(state, email, password, signup_personal()).await
}

/// [`register_user`] with the org choice explicit: `personal` gives the user an org of their own,
/// otherwise they join the install's org.
pub async fn register_user_with(state: &AppState, email: &str, password: &str, personal: bool) -> AppResult<User> {
    let db = &state.control;
    let email = &normalize_email(email);
    validate_credentials(email, password)?;

    // Accounts created before emails were normalized may be mixed-case; the
    // unique index alone wouldn't catch "Alice@x.com" vs "alice@x.com".
    let mut res = store::control::AUTH_USER_ID_BY_EMAIL
        .on(db)
        .bind(("email", email.to_string()))
        .await?;
    #[derive(Deserialize, SurrealValue)]
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

    let mut res = store::control::AUTH_USER_CREATE
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

    let setup = async {
        let org = assign_org(state, &row.id, email, personal).await?;
        let orgdb = state.pool.for_org(&org).await?;
        crate::vaults::service::create_personal_vault(&orgdb, &row.id).await?;
        AppResult::Ok(org)
    }
    .await;
    match setup {
        Ok(org) => Ok(User { id: row.id, email: row.email, org }),
        Err(e) => {
            // do not leave an account that cannot log in
            let _ = store::control::AUTH_USER_DELETE.on(db).bind(("id", row.id.clone())).await;
            Err(e)
        }
    }
}

pub async fn authenticate(db: &ControlDb, email: &str, password: &str) -> AppResult<Option<User>> {
    // ponytail: string::lowercase() scan, no index -- add a normalized-email index if user counts get large
    let mut res = store::control::AUTH_USER_BY_EMAIL
        .on(db)
        .bind(("email", normalize_email(email)))
        .await?;
    let rows: Vec<UserRow> = res.take(0)?;
    let Some(row) = rows.into_iter().next() else { return Ok(None) };

    let ok = bcrypt::verify(password, &row.password_hash).unwrap_or(false);
    if !ok {
        return Ok(None);
    }
    Ok(Some(load_user(db, row.id, row.email).await?))
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

#[derive(Debug, Deserialize, SurrealValue)]
struct ApiTokenRow {
    id: RecordId,
    name: String,
}

/// A full-scope, non-expiring token, as every token was before scopes existed.
pub async fn create_api_token(db: &ControlDb, owner: &RecordId, name: &str) -> AppResult<ApiTokenCreated> {
    create_api_token_with(db, owner, name, &all_scope_names(), None, None).await
}

pub async fn create_api_token_with(
    db: &ControlDb,
    owner: &RecordId,
    name: &str,
    scopes: &[String],
    vault: Option<&RecordId>,
    expires_at: Option<Datetime>,
) -> AppResult<ApiTokenCreated> {
    let token = generate_token();
    let hash = hash_token(&token);

    let mut res = store::control::AUTH_TOKEN_CREATE
        .on(db)
        .bind(("owner", owner.clone()))
        .bind(("name", name.to_string()))
        .bind(("hash", hash))
        .bind(("scopes", scopes.to_vec()))
        .bind(("vault", vault.cloned()))
        .bind(("expires_at", expires_at))
        .await?;
    let rows: Vec<ApiTokenRow> = res.take(0)?;
    let row = rows.into_iter().next().ok_or_else(|| AppError::internal("insert returned no row"))?;

    Ok(ApiTokenCreated { id: row.id, name: row.name, token, scopes: scopes.to_vec(), vault: vault.cloned(), expires_at })
}

fn all_scope_names() -> Vec<String> {
    scopes::ALL.iter().map(|s| s.to_string()).collect()
}

#[derive(Debug, Deserialize, SurrealValue, Serialize)]
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

pub async fn list_api_tokens(db: &ControlDb, owner: &RecordId) -> AppResult<Vec<ApiTokenSummary>> {
    let mut res = store::control::AUTH_TOKEN_LIST
        .on(db)
        .bind(("owner", owner.clone()))
        .await?;
    Ok(res.take(0)?)
}

pub async fn revoke_api_token(db: &ControlDb, owner: &RecordId, token_id: &RecordId) -> AppResult<bool> {
    let mut res = store::control::AUTH_TOKEN_DELETE
        .on(db)
        .bind(("id", token_id.clone()))
        .bind(("owner", owner.clone()))
        .await?;
    #[derive(Deserialize, SurrealValue)]
    struct Gone {
        #[allow(dead_code)]
        id: RecordId,
    }
    Ok(!res.take::<Vec<Gone>>(0)?.is_empty())
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

pub async fn check_api_token(db: &ControlDb, token: &str) -> AppResult<TokenCheck> {
    #[derive(Deserialize, SurrealValue)]
    struct TokenRow {
        id: RecordId,
        owner: RecordId,
        #[serde(default = "all_scope_names")]
        #[surreal(default = "all_scope_names")]
        scopes: Vec<String>,
        #[serde(default)]
        #[surreal(default)]
        vault: Option<RecordId>,
        #[serde(default)]
        #[surreal(default)]
        expired: bool,
    }
    let hash = hash_token(token);
    let mut res = store::control::AUTH_TOKEN_BY_HASH
        .on(db)
        .bind(("hash", hash))
        .await?;
    let rows: Vec<TokenRow> = res.take(0)?;
    let Some(row) = rows.into_iter().next() else { return Ok(TokenCheck::Unknown) };

    if row.expired {
        return Ok(TokenCheck::Expired);
    }

    // Best-effort bump; failure here must not block auth.
    let _ = store::control::AUTH_TOKEN_TOUCH
        .on(db)
        .bind(("id", row.id.clone()))
        .await;

    let owner_row: Option<UserRow> = store::get_control(db, &row.owner).await?;
    Ok(match owner_row {
        Some(r) => TokenCheck::Valid(Box::new(VerifiedToken {
            user: load_user(db, r.id, r.email).await?,
            token_id: row.id,
            scopes: row.scopes.into_iter().filter(|s| scopes::is_known(s)).collect(),
            vault: row.vault,
        })),
        None => TokenCheck::Unknown,
    })
}

/// The token's user, or `None` if it is unknown or expired. Callers that need
/// the scopes or the expiry error use [`check_api_token`].
pub async fn verify_api_token(db: &ControlDb, token: &str) -> AppResult<Option<User>> {
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
