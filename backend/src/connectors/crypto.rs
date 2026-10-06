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
//! Like the Python version, an empty encryption key degrades to a static
//! fallback (here, an all-zero key) rather than failing outright -- reachable
//! only in test harnesses or local-dev setups that skipped `ENCRYPTION_KEY`.

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

const NONCE_LEN: usize = 12;

/// Derives a 32-byte AES-256 key from the configured `ENCRYPTION_KEY`.
/// A 32-byte base64url-encoded key is used as-is; anything else (including
/// the empty-string fallback) is hashed down to 32 bytes with SHA-256.
fn derive_key(key: &str) -> [u8; 32] {
    if key.is_empty() {
        return [0u8; 32];
    }
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
    URL_SAFE_NO_PAD.encode(out)
}

/// Decrypts a value produced by [`encrypt`]. Mirrors the Python `decrypt()`:
/// an empty input round-trips to an empty string.
pub fn decrypt(key: &str, value: &str) -> AppResult<String> {
    if value.is_empty() {
        return Ok(String::new());
    }
    let cipher = cipher_for(key);

    let raw = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|e| AppError::internal(format!("invalid ciphertext encoding: {e}")))?;
    if raw.len() < NONCE_LEN {
        return Err(AppError::internal("invalid ciphertext: too short"));
    }
    let (nonce_bytes, ciphertext) = raw.split_at(NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| AppError::internal("decryption failed"))?;
    String::from_utf8(plaintext).map_err(|e| AppError::internal(format!("decrypted value is not valid utf-8: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_with_a_real_key() {
        let key = "my-test-encryption-key";
        let ciphertext = encrypt(key, "super-secret-token");
        assert_ne!(ciphertext, "super-secret-token");
        assert_eq!(decrypt(key, &ciphertext).unwrap(), "super-secret-token");
    }

    #[test]
    fn roundtrips_with_empty_key_fallback() {
        let ciphertext = encrypt("", "a-value");
        assert_eq!(decrypt("", &ciphertext).unwrap(), "a-value");
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
