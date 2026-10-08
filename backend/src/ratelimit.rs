//! In-process token buckets for the noisy-neighbour limits: per user, per
//! token, and a tighter one per client address for login and signup. Per
//! process, which is the right scope for a single-node self-host install; behind
//! several replicas each enforces its own share.
// ponytail: not tower_governor. Its per-key config and keyed extractors are more
// surface than three limits need; swap it in if limits ever become per-org.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct RateConfig {
    /// Requests per minute per signed-in user (0 turns the limit off).
    pub user_per_min: u32,
    /// Requests per minute per personal access token.
    pub token_per_min: u32,
    /// Login, signup and failed-credential attempts per minute per client address.
    pub auth_per_min: u32,
}

impl Default for RateConfig {
    fn default() -> Self {
        RateConfig { user_per_min: 1200, token_per_min: 600, auth_per_min: 20 }
    }
}

impl RateConfig {
    /// `RATE_LIMIT_USER_PER_MIN`, `RATE_LIMIT_TOKEN_PER_MIN`, `RATE_LIMIT_AUTH_PER_MIN`.
    pub fn from_env() -> Self {
        let d = RateConfig::default();
        let get = |key: &str, default: u32| std::env::var(key).ok().and_then(|v| v.trim().parse().ok()).unwrap_or(default);
        RateConfig {
            user_per_min: get("RATE_LIMIT_USER_PER_MIN", d.user_per_min),
            token_per_min: get("RATE_LIMIT_TOKEN_PER_MIN", d.token_per_min),
            auth_per_min: get("RATE_LIMIT_AUTH_PER_MIN", d.auth_per_min),
        }
    }
}

struct Bucket {
    tokens: f64,
    last: Instant,
}

#[derive(Default)]
pub struct RateLimiter {
    buckets: Mutex<HashMap<String, Bucket>>,
}

const PRUNE_AT: usize = 20_000;

impl RateLimiter {
    /// Takes one token from `key`'s bucket (capacity and refill: `per_min` per minute).
    /// `Err(seconds)` is how long until a token is available.
    pub fn check(&self, key: &str, per_min: u32) -> Result<(), u64> {
        if per_min == 0 {
            return Ok(());
        }
        let cap = f64::from(per_min);
        let rate = cap / 60.0;
        let now = Instant::now();
        let mut map = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        if map.len() > PRUNE_AT {
            // an idle bucket has refilled; dropping it changes nothing
            map.retain(|_, b| now.duration_since(b.last) < Duration::from_secs(120));
        }
        let b = map.entry(key.to_string()).or_insert(Bucket { tokens: cap, last: now });
        b.tokens = (b.tokens + now.duration_since(b.last).as_secs_f64() * rate).min(cap);
        b.last = now;
        if b.tokens >= 1.0 {
            b.tokens -= 1.0;
            Ok(())
        } else {
            Err(((1.0 - b.tokens) / rate).ceil().max(1.0) as u64)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_the_burst_then_limits_with_a_retry_hint() {
        let rl = RateLimiter::default();
        for _ in 0..3 {
            assert!(rl.check("k", 3).is_ok());
        }
        let wait = rl.check("k", 3).unwrap_err();
        assert!((1..=20).contains(&wait), "{wait}");
        // another key is unaffected, and 0 means unlimited
        assert!(rl.check("other", 3).is_ok());
        for _ in 0..50 {
            assert!(rl.check("free", 0).is_ok());
        }
    }
}
