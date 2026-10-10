//! Credential encryption at rest.
//!
//! AES-256-GCM (via the `aes-gcm` crate) with a random 96-bit nonce prepended
//! to the ciphertext, base64url-encoded and marked `enc:v1:`. Values written before
//! the marker existed are the same bytes without the prefix and still decrypt. It
//! cannot read Fernet ciphertexts written by the pre-Rust backend; moving such values
//! needs a one-off re-encryption.
//!
//! A value that does not decrypt is an error, never passed on as a credential, except
//! an unmarked API key that is not even shaped like ciphertext: a plaintext key saved
//! before keys were encrypted (see [`decrypt_or_plaintext`]).
//!
//! An empty key derives the all-zero AES key. The backend refuses to boot on it
//! (see [`guard_key`]) unless `EUNOMIA_ALLOW_EMPTY_ENCRYPTION_KEY=1` is set or the
//! install already holds data written that way. An install that ran with an empty
//! key and then gets a real one sets `ENCRYPTION_KEY_LEGACY_EMPTY=1`: [`decrypt`]
//! then also tries the zero key, so old values keep working while every new write
//! uses the real key (see docs/deployment.md, "Rotating ENCRYPTION_KEY").

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use surrealdb::types::SurrealValue;
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
const PREFIX: &str = "enc:v1:";
const MIN_KEY_LEN: usize = 16;

/// A usable deployment `ENCRYPTION_KEY`: set and not trivially short
/// (`openssl rand -base64 32`, what the installer generates, gives 44 characters).
pub fn validate_key(key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("ENCRYPTION_KEY is not set. It encrypts saved credentials and API keys: generate one with \
                    `openssl rand -base64 32` and set it in .env (or export it) before starting the backend."
            .to_string());
    }
    if key.len() < MIN_KEY_LEN {
        return Err(format!(
            "ENCRYPTION_KEY is too short ({} characters, need at least {MIN_KEY_LEN}). Generate one with `openssl rand -base64 32`.",
            key.len()
        ));
    }
    Ok(())
}

/// Derives a 32-byte AES-256 key from the configured `ENCRYPTION_KEY`.
/// A 32-byte base64url-encoded key is used as-is; anything else (including
/// the empty-string fallback) is hashed down to 32 bytes with SHA-256.
fn derive_key(key: &str) -> [u8; 32] {
    if key.is_empty() {
        return [0u8; 32];
    }
    if let Ok(bytes) = URL_SAFE_NO_PAD.decode(key.trim_end_matches('='))
        && bytes.len() == 32 {
            let mut out = [0u8; 32];
            out.copy_from_slice(&bytes);
            return out;
        }
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hasher.finalize().into()
}

fn cipher_for(key: &str) -> Aes256Gcm {
    let aes_key = derive_key(key);
    Aes256Gcm::new_from_slice(&aes_key).expect("derived key is always 32 bytes")
}

/// Encrypts `value` under `key` (the settings `encryption_key`). An empty input round-trips to an empty string without
/// touching the cipher.
pub fn encrypt(key: &str, value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    let cipher = cipher_for(key);

    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher.encrypt(nonce, value.as_bytes()).expect("AES-GCM encryption cannot fail here");

    let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(out))
}

/// Decrypts a value produced by [`encrypt`], with or without the `enc:v1:` marker. An empty input round-trips to an empty string.
pub fn decrypt(key: &str, value: &str) -> AppResult<String> {
    decrypt_with(key, value, legacy_empty_fallback())
}

pub fn legacy_empty_fallback() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("ENCRYPTION_KEY_LEGACY_EMPTY").is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true")))
}

fn decrypt_with(key: &str, value: &str, legacy_empty: bool) -> AppResult<String> {
    match decrypt_exact(key, value) {
        Err(_) if legacy_empty && !key.is_empty() => decrypt_exact("", value).inspect(|_| {
            tracing::warn!("decrypted a value written with the empty ENCRYPTION_KEY; re-save it (or rotate) to move it to the current key");
        }),
        other => other,
    }
}

pub fn decrypt_exact(key: &str, value: &str) -> AppResult<String> {
    if value.is_empty() {
        return Ok(String::new());
    }
    let cipher = cipher_for(key);

    let raw = URL_SAFE_NO_PAD
        .decode(value.strip_prefix(PREFIX).unwrap_or(value))
        .map_err(|e| AppError::internal(format!("invalid ciphertext encoding: {e}")))?;
    if raw.len() < NONCE_LEN {
        return Err(AppError::internal("invalid ciphertext: too short"));
    }
    let (nonce_bytes, ciphertext) = raw.split_at(NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| {
            AppError::internal("stored credential could not be decrypted (wrong ENCRYPTION_KEY or corrupted data), re-enter it")
        })?;
    String::from_utf8(plaintext).map_err(|e| AppError::internal(format!("decrypted value is not valid utf-8: {e}")))
}

/// With `ENCRYPTION_KEY_LEGACY_EMPTY` on and a real key set, re-encrypt every org database password
/// that is still under the empty key, so the fallback is no longer needed for them. Idempotent.
pub async fn rotate_tenant_passwords(settings: &crate::config::Settings, control: &crate::pool::ControlDb) -> AppResult<usize> {
    rotate_tenant_passwords_with(settings, control, legacy_empty_fallback()).await
}

pub async fn rotate_tenant_passwords_with(settings: &crate::config::Settings, control: &crate::pool::ControlDb, legacy: bool) -> AppResult<usize> {
    if settings.encryption_key.is_empty() || !legacy {
        return Ok(0);
    }
    #[derive(serde::Deserialize, SurrealValue)]
    struct Row {
        id: surrealdb::types::RecordId,
        db_pass_enc: String,
    }
    let mut res = crate::store::tenant::PASS_ALL.on(control).await?;
    let mut moved = 0;
    for row in res.take::<Vec<Row>>(0)? {
        if decrypt_exact(&settings.encryption_key, &row.db_pass_enc).is_ok() {
            continue;
        }
        let Ok(pass) = decrypt_exact("", &row.db_pass_enc) else { continue };
        crate::store::tenant::SET_PASS
            .on(control)
            .bind(("id", row.id))
            .bind(("db_pass_enc", encrypt(&settings.encryption_key, &pass)))
            .await?
            .check()?;
        moved += 1;
    }
    if moved > 0 {
        tracing::warn!(moved, "re-encrypted org database passwords from the empty ENCRYPTION_KEY to the current one");
    }
    Ok(moved)
}

/// `EUNOMIA_ALLOW_EMPTY_ENCRYPTION_KEY=1`: the explicit opt-in to the public zero key (dev, tests).
pub fn empty_key_allowed() -> bool {
    std::env::var("EUNOMIA_ALLOW_EMPTY_ENCRYPTION_KEY").is_ok_and(|v| v == "1")
}

/// Boot check, run once the control database is up. A real key passes. An empty one is allowed only
/// when `EUNOMIA_ALLOW_EMPTY_ENCRYPTION_KEY=1` (dev, tests), or when the install already has orgs: they
/// were provisioned under the empty key, and refusing would lock the owner out of their own data.
/// That case boots with a loud warning.
pub async fn guard_key(settings: &crate::config::Settings, control: &crate::pool::ControlDb) -> AppResult<()> {
    if !settings.encryption_key.is_empty() {
        // A short key is weak but still derives a working cipher. A fresh install refuses it; an install
        // that already holds data written under it must stay reachable, so it boots with a loud error.
        let Err(why) = validate_key(&settings.encryption_key) else { return Ok(()) };
        if !has_orgs(control).await? {
            return Err(AppError::internal(why));
        }
        tracing::error!(
            "{why} This install's data was written under it, so it boots anyway. Move to a strong key with \
             `openssl rand -base64 32`: values written under the old key stop decrypting, so follow \
             docs/deployment.md, \"Rotating ENCRYPTION_KEY\"."
        );
        return Ok(());
    }
    if empty_key_allowed() {
        tracing::warn!("ENCRYPTION_KEY is empty and EUNOMIA_ALLOW_EMPTY_ENCRYPTION_KEY=1: stored credentials use a public key. Dev only.");
        return Ok(());
    }
    if has_orgs(control).await? {
        tracing::error!(
            "ENCRYPTION_KEY is empty: this install's database passwords and connector credentials are protected by a PUBLIC key. \
             Booting anyway so the data stays reachable. Set ENCRYPTION_KEY (openssl rand -base64 32) together with \
             ENCRYPTION_KEY_LEGACY_EMPTY=1 to move to a real key; see docs/deployment.md, \"Rotating ENCRYPTION_KEY\"."
        );
        return Ok(());
    }
    Err(AppError::internal(
        "ENCRYPTION_KEY is not set. Generate one with `openssl rand -base64 32` and put it in .env \
         (for local development only, EUNOMIA_ALLOW_EMPTY_ENCRYPTION_KEY=1 skips this check).",
    ))
}

async fn has_orgs(control: &crate::pool::ControlDb) -> AppResult<bool> {
    #[derive(serde::Deserialize, SurrealValue)]
    struct Count {
        count: i64,
    }
    let mut res = crate::store::tenant::ORG_COUNT.on(control).await?;
    Ok(res.take::<Vec<Count>>(0)?.first().is_some_and(|c| c.count > 0))
}

/// For the OpenAI API key, which early builds stored as plaintext: a marked value must decrypt; an
/// unmarked one is pre-marker ciphertext (which must decrypt whenever it is shaped like ciphertext)
/// or a legacy plaintext key. Failing closed keeps ciphertext from ever being sent as a key.
pub fn decrypt_or_plaintext(key: &str, value: &str) -> AppResult<String> {
    if value.is_empty() || value.starts_with(PREFIX) {
        return decrypt(key, value);
    }
    match decrypt(key, value) {
        Ok(v) => Ok(v),
        Err(e) if looks_like_ciphertext(value) => Err(e),
        Err(_) => Ok(value.to_string()),
    }
}

fn looks_like_ciphertext(value: &str) -> bool {
    URL_SAFE_NO_PAD.decode(value).is_ok_and(|b| b.len() >= NONCE_LEN + TAG_LEN)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "test-encryption-key-0123456789abcdef";
    const OTHER_KEY: &str = "another-encryption-key-0123456789ab";

    #[test]
    fn missing_or_short_deployment_keys_are_rejected() {
        assert!(validate_key("").is_err());
        assert!(validate_key("   ").is_err());
        assert!(validate_key("short-key").is_err());
        assert!(validate_key(KEY).is_ok());
        assert!(validate_key("q2l0dnR5c2VjcmV0a2V5Zm9ydGVzdHMxMjM0NTY3OA==").is_ok());
    }

    #[test]
    fn new_ciphertext_carries_the_version_marker() {
        assert!(encrypt(KEY, "sk-real").starts_with("enc:v1:"));
    }

    #[test]
    fn decrypt_or_plaintext_current_ciphertext_path() {
        assert_eq!(decrypt_or_plaintext(KEY, &encrypt(KEY, "sk-real")).unwrap(), "sk-real");
        assert_eq!(decrypt_or_plaintext(KEY, "").unwrap(), "");
    }

    #[test]
    fn decrypt_or_plaintext_legacy_plaintext_path() {
        assert_eq!(decrypt_or_plaintext(KEY, "sk-legacy-plaintext").unwrap(), "sk-legacy-plaintext");
    }

    #[test]
    fn pre_marker_ciphertext_still_decrypts() {
        let legacy = encrypt(KEY, "sk-real").trim_start_matches(PREFIX).to_string();
        assert_eq!(decrypt(KEY, &legacy).unwrap(), "sk-real");
        assert_eq!(decrypt_or_plaintext(KEY, &legacy).unwrap(), "sk-real");
    }

    #[test]
    fn wrong_key_fails_closed_for_api_keys() {
        let marked = encrypt(KEY, "sk-real");
        assert!(decrypt_or_plaintext(OTHER_KEY, &marked).is_err());
        // pre-marker ciphertext under the wrong key is not mistaken for plaintext
        let legacy = marked.trim_start_matches(PREFIX).to_string();
        assert!(decrypt_or_plaintext(OTHER_KEY, &legacy).is_err());
    }

    #[test]
    fn corrupted_ciphertext_fails_closed() {
        let marked = encrypt(KEY, "sk-real");
        let mut bytes = URL_SAFE_NO_PAD.decode(marked.trim_start_matches(PREFIX)).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        let corrupted = format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes));
        assert!(decrypt(KEY, &corrupted).is_err());
        assert!(decrypt_or_plaintext(KEY, &corrupted).is_err());
        assert!(decrypt_or_plaintext(KEY, "enc:v1:not-base64!!").is_err());
    }

    #[test]
    fn legacy_empty_key_values_decrypt_only_with_the_fallback_on() {
        let old = encrypt("", "old-secret");
        assert!(decrypt_with("new-key", &old, false).is_err());
        assert_eq!(decrypt_with("new-key", &old, true).unwrap(), "old-secret");
        // values under the new key never need the fallback
        assert_eq!(decrypt_with("new-key", &encrypt("new-key", "x"), true).unwrap(), "x");
    }

    #[test]
    fn roundtrips_with_empty_key_fallback() {
        let ciphertext = encrypt("", "a-value");
        assert_eq!(decrypt("", &ciphertext).unwrap(), "a-value");
    }

    #[test]
    fn roundtrips_with_a_real_key() {
        let key = "my-test-encryption-key";
        let ciphertext = encrypt(key, "super-secret-token");
        assert_ne!(ciphertext, "super-secret-token");
        assert_eq!(decrypt(key, &ciphertext).unwrap(), "super-secret-token");
    }

    #[test]
    fn empty_value_roundtrips_to_empty_string_without_touching_the_cipher() {
        assert_eq!(encrypt("key", ""), "");
        assert_eq!(decrypt("key", "").unwrap(), "");
    }

    #[test]
    fn ciphertext_is_not_deterministic_due_to_random_nonce() {
        let a = encrypt("key", "same-value");
        let b = encrypt("key", "same-value");
        assert_ne!(a, b);
        assert_eq!(decrypt("key", &a).unwrap(), "same-value");
        assert_eq!(decrypt("key", &b).unwrap(), "same-value");
    }

    #[test]
    fn wrong_key_fails_to_decrypt() {
        let ciphertext = encrypt("key-one", "secret");
        assert!(decrypt("key-two", &ciphertext).is_err());
    }

    #[test]
    fn garbage_ciphertext_is_rejected() {
        assert!(decrypt("key", "not-valid-base64-or-ciphertext!!").is_err());
    }
}
