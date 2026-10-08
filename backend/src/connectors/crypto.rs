//! Credential encryption at rest. Ported from `connectors/crypto.py`.
//!
//! **NOT BYTE-COMPATIBLE WITH THE PYTHON IMPLEMENTATION.** The Python backend
//! encrypts with `cryptography.fernet.Fernet`, which produces a versioned
//! token built from AES-128-CBC + HMAC-SHA256. This Rust port instead uses
//! AES-256-GCM (via the `aes-gcm` crate) with a random 96-bit nonce prepended
//! to the ciphertext, all base64url-encoded. A ciphertext produced by one
//! implementation CANNOT be decrypted by the other. If existing
//! Fernet-encrypted `credentials_encrypted` / `openai_api_key_encrypted`
//! values need to move from the Python-managed database into this backend,
//! that is a data migration (decrypt with Python, re-encrypt with this
//! module) and is explicitly out of scope here.
//!
//! There is no fallback key: the backend refuses to start without a valid
//! `ENCRYPTION_KEY` (see [`validate_key`], called from `main`).
//!
//! Stored format: `enc:v1:` + base64url(nonce || ciphertext). Values written
//! before the marker existed are the same bytes without the prefix and still
//! decrypt. Anything that doesn't decrypt is an error -- never passed on as
//! a credential -- except an unprefixed API key that isn't even shaped like
//! ciphertext, which is a plaintext key saved before keys were encrypted
//! (see [`decrypt_or_plaintext`]). Saving a key again rewrites it as `enc:v1:`.

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
const PREFIX: &str = "enc:v1:";
const MIN_KEY_LEN: usize = 16;

/// The deployment's `ENCRYPTION_KEY` must be set and not trivially short
/// (`openssl rand -base64 32`, what the installer generates, gives 44
/// characters; the floor is lower so existing hand-set keys keep working).
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
/// A 32-byte base64url-encoded key is used as-is; anything else is hashed
/// down to 32 bytes with SHA-256.
fn derive_key(key: &str) -> [u8; 32] {
    if let Ok(bytes) = URL_SAFE_NO_PAD.decode(key.trim_end_matches('=')) {
        if bytes.len() == 32 {
            let mut out = [0u8; 32];
            out.copy_from_slice(&bytes);
            return out;
        }
    }
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hasher.finalize().into()
}

fn cipher_for(key: &str) -> Aes256Gcm {
    let aes_key = derive_key(key);
    Aes256Gcm::new_from_slice(&aes_key).expect("derived key is always 32 bytes")
}

/// Encrypts `value` under `key` (the settings `encryption_key`). Mirrors the
/// Python `encrypt()`: an empty input round-trips to an empty string without
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

/// Decrypts a value produced by [`encrypt`] (with or without the `enc:v1:`
/// marker). An empty input round-trips to an empty string; a wrong key or
/// corrupted ciphertext is an error.
pub fn decrypt(key: &str, value: &str) -> AppResult<String> {
    if value.is_empty() {
        return Ok(String::new());
    }
    open(key, value.strip_prefix(PREFIX).unwrap_or(value))
}

fn open(key: &str, value: &str) -> AppResult<String> {
    let cipher = cipher_for(key);

    let raw = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|e| AppError::internal(format!("invalid ciphertext encoding: {e}")))?;
    if raw.len() < NONCE_LEN {
        return Err(AppError::internal("invalid ciphertext: too short"));
    }
    let (nonce_bytes, ciphertext) = raw.split_at(NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher.decrypt(nonce, ciphertext).map_err(|_| {
        AppError::internal("stored credential could not be decrypted (wrong ENCRYPTION_KEY or corrupted data) -- re-enter it")
    })?;
    String::from_utf8(plaintext).map_err(|e| AppError::internal(format!("decrypted value is not valid utf-8: {e}")))
}

/// For the OpenAI API key, which early builds stored as plaintext: a marked
/// value must decrypt; an unmarked one is pre-marker ciphertext (which must
/// decrypt whenever it is shaped like ciphertext) or a legacy plaintext key.
pub fn decrypt_or_plaintext(key: &str, value: &str) -> AppResult<String> {
    if value.is_empty() || value.starts_with(PREFIX) {
        return decrypt(key, value);
    }
    match open(key, value) {
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
    fn roundtrips_with_a_real_key() {
        let ciphertext = encrypt(KEY, "super-secret-token");
        assert_ne!(ciphertext, "super-secret-token");
        assert_eq!(decrypt(KEY, &ciphertext).unwrap(), "super-secret-token");
    }

    #[test]
    fn empty_value_roundtrips_to_empty_string_without_touching_the_cipher() {
        assert_eq!(encrypt(KEY, ""), "");
        assert_eq!(decrypt(KEY, "").unwrap(), "");
    }

    #[test]
    fn ciphertext_is_not_deterministic_due_to_random_nonce() {
        let a = encrypt(KEY, "same-value");
        let b = encrypt(KEY, "same-value");
        assert_ne!(a, b);
        assert_eq!(decrypt(KEY, &a).unwrap(), "same-value");
        assert_eq!(decrypt(KEY, &b).unwrap(), "same-value");
    }

    #[test]
    fn wrong_key_fails_to_decrypt() {
        let ciphertext = encrypt(KEY, "secret");
        assert!(decrypt(OTHER_KEY, &ciphertext).is_err());
    }

    #[test]
    fn garbage_ciphertext_is_rejected() {
        assert!(decrypt(KEY, "not-valid-base64-or-ciphertext!!").is_err());
    }
}
