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

/// A unique-index / record-exists violation.
pub fn is_duplicate(err: &surrealdb::Error) -> bool {
    let msg = err.to_string();
    err.is_already_exists() || msg.contains("already contains") || msg.contains("already exists")
}

/// Worth retrying by default: a commit-time conflict only. A duplicate is the
/// caller's answer (a taken email retried five times is still taken); see [`with_retry_dup`].
pub fn is_retryable(err: &surrealdb::Error) -> bool {
    is_conflict(err)
}

/// Run `f` up to `MAX_ATTEMPTS` times, retrying only on a commit conflict, with
/// a small jittered backoff. `f` must be safe to re-run from scratch.
pub async fn with_retry<T, F, Fut>(f: F) -> Result<T, surrealdb::Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, surrealdb::Error>>,
{
    retry(f, is_retryable).await
}

/// [`with_retry`] that also retries a unique violation. Only for a closure that
/// re-reads first (get-or-create, upsert by deterministic id): a concurrent writer
/// created the row, so the retry finds it instead of inserting.
pub async fn with_retry_dup<T, F, Fut>(f: F) -> Result<T, surrealdb::Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, surrealdb::Error>>,
{
    retry(f, |e| is_retryable(e) || is_duplicate(e)).await
}

async fn retry<T, F, Fut>(mut f: F, retryable: impl Fn(&surrealdb::Error) -> bool) -> Result<T, surrealdb::Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, surrealdb::Error>>,
{
    let mut attempt = 1;
    loop {
        match f().await {
            Err(e) if attempt < MAX_ATTEMPTS && retryable(&e) => {
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
    async fn duplicate_retries_only_when_opted_in() {
        let dup = || surrealdb::Error::query("Database index `u` already contains 'x'".into(), None);
        let calls = AtomicU32::new(0);
        let out: Result<(), _> = with_retry(|| async {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(dup())
        })
        .await;
        assert!(out.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let out: Result<(), _> = with_retry_dup(|| async {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(dup())
        })
        .await;
        assert!(out.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1 + MAX_ATTEMPTS);
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
