//! Retry helper for SurrealDB's optimistic transactions. A commit that loses a
//! read/write race fails with a retryable error; re-running the whole closure
//! (which re-reads) is the fix. See foundation-plan 3.5.

use std::future::Future;
use std::time::Duration;

use rand::Rng;
use surrealdb::types::QueryError;
use tokio::sync::{Mutex, MutexGuard};

/// Attempts per call. The plan says 3; a commit conflict can repeat when two
/// writers overlap, and each retry is a tiny transaction, so 5 costs nothing
/// when uncontended.
pub const MAX_ATTEMPTS: u32 = 5;

/// Deterministic record key from `parts`: a 20-char `[a-z0-9]` string with a
/// leading letter (like SurrealDB's generated ids, and never all digits).
pub fn stable_key(tag: char, parts: &str) -> String {
    use sha2::{Digest, Sha256};
    let hex: String = Sha256::digest(parts.as_bytes()).iter().take(10).map(|b| format!("{b:02x}")).collect();
    format!("{tag}{}", &hex[..19])
}

const STRIPES: usize = 64;
static LOCKS: [Mutex<()>; STRIPES] = [const { Mutex::const_new(()) }; STRIPES];

/// In-process write lock striped by `key`. Optimistic retry alone starves under
/// a sustained writer on one entity: a memory insert is slow (full-text index
/// work), so a retried transaction keeps overlapping the other writer's commit
/// (measured: ~2% of 800 writes exhausted 10 attempts). Serialising same-key
/// writers inside the process removes that; the transaction and retry still
/// guard other processes and stripe collisions.
// ponytail: single-node (self-host) assumption; a multi-node deployment relies on retry alone.
pub async fn lock(key: &str) -> MutexGuard<'static, ()> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut h);
    LOCKS[(h.finish() as usize) % STRIPES].lock().await
}

/// The single definition of a commit-time read/write conflict (also used by
/// `AppError`'s mapping). 3.x reports it as `QueryError::TransactionConflict` over the wire, but
/// the embedded engine (and 2.x-style servers) only carry the message text ("...This transaction
/// can be retried"), so match both. `tests/store.rs` provokes a real one.
pub fn is_conflict(err: &surrealdb::Error) -> bool {
    err.query_details() == Some(&QueryError::TransactionConflict) || err.to_string().contains("can be retried")
}

/// Worth retrying: a conflict, or a unique-index/record-exists violation (a
/// concurrent writer created the row we were about to, so the retry's re-read
/// finds it).
pub fn is_retryable(err: &surrealdb::Error) -> bool {
    if is_conflict(err) {
        return true;
    }
    let msg = err.to_string();
    err.is_already_exists() || msg.contains("already contains") || msg.contains("already exists")
}

/// Run `f` up to `MAX_ATTEMPTS` times, retrying only when `is_retryable`, with
/// a small jittered backoff. `f` must be safe to re-run from scratch.
pub async fn with_retry<T, F, Fut>(mut f: F) -> Result<T, surrealdb::Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, surrealdb::Error>>,
{
    let mut attempt = 1;
    loop {
        match f().await {
            Err(e) if attempt < MAX_ATTEMPTS && is_retryable(&e) => {
                let ms = rand::thread_rng().gen_range(1..=(5u64 << attempt).min(100));
                tokio::time::sleep(Duration::from_millis(ms)).await;
                attempt += 1;
            }
            other => return other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn conflict() -> surrealdb::Error {
        surrealdb::Error::query("conflict".into(), QueryError::TransactionConflict)
    }

    fn other() -> surrealdb::Error {
        surrealdb::Error::query("Found NONE for field `x`".into(), None)
    }

    #[test]
    fn detects_variant_and_text() {
        assert!(is_retryable(&conflict()));
        let remote = surrealdb::Error::internal(
            "Failed to commit transaction due to a read or write conflict. This transaction can be retried".into(),
        );
        assert!(is_retryable(&remote));
        assert!(!is_retryable(&other()));
    }

    #[tokio::test]
    async fn retries_conflict_then_succeeds() {
        let calls = AtomicU32::new(0);
        let out = with_retry(|| async {
            if calls.fetch_add(1, Ordering::SeqCst) < 2 { Err(conflict()) } else { Ok(7) }
        })
        .await;
        assert_eq!(out.unwrap(), 7);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn does_not_retry_other_errors() {
        let calls = AtomicU32::new(0);
        let out: Result<(), _> = with_retry(|| async {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(other())
        })
        .await;
        assert!(out.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn gives_up_after_max_attempts() {
        let calls = AtomicU32::new(0);
        let out: Result<(), _> = with_retry(|| async {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(conflict())
        })
        .await;
        assert!(out.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), MAX_ATTEMPTS);
    }
}
