//! In-process token buckets for the noisy-neighbour limits: per user, per
//! token, and a tighter one per client address for login and signup. Per
//! process, which is the right scope for a single-node self-host install; behind
//! several replicas each enforces its own share.
// ponytail: not tower_governor. Its per-key config and keyed extractors are more
// surface than three limits need; swap it in if limits ever become per-org.

use std::collections::HashMap;
use std::net::IpAddr;
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
    /// Peers whose `X-Forwarded-For` is believed (`TRUSTED_PROXIES`).
    pub trusted_proxies: Vec<Cidr>,
}

/// An address range such as `172.16.0.0/12`; a bare address is a single host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cidr {
    net: u128,
    mask: u128,
    v6: bool,
}

impl Cidr {
    pub fn parse(s: &str) -> Option<Cidr> {
        let (addr, bits) = match s.trim().split_once('/') {
            Some((a, b)) => (a, Some(b.parse::<u32>().ok()?)),
            None => (s.trim(), None),
        };
        let ip: IpAddr = addr.parse().ok()?;
        let (v, width) = bits_of(ip);
        let prefix = bits.unwrap_or(width);
        if prefix > width {
            return None;
        }
        let mask = if prefix == 0 { 0 } else { (u128::MAX << (width - prefix)) & (u128::MAX >> (128 - width)) };
        Some(Cidr { net: v & mask, mask, v6: width == 128 })
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        let (v, width) = bits_of(ip);
        (width == 128) == self.v6 && v & self.mask == self.net
    }
}

fn bits_of(ip: IpAddr) -> (u128, u32) {
    match ip {
        IpAddr::V4(a) => (u128::from(u32::from(a)), 32),
        IpAddr::V6(a) => (u128::from(a), 128),
    }
}

/// Loopback plus the private ranges the Docker bridge networks are carved from.
const DEFAULT_TRUSTED_PROXIES: &str = "127.0.0.0/8,10.0.0.0/8,172.16.0.0/12,192.168.0.0/16,::1/128,fc00::/7";

fn parse_cidrs(list: &str) -> Vec<Cidr> {
    list.split(',').filter(|s| !s.trim().is_empty()).filter_map(Cidr::parse).collect()
}

/// The client behind `peer`: with an untrusted (or unknown) peer, the peer
/// itself and `X-Forwarded-For` is ignored; with a trusted one, the right-most
/// `X-Forwarded-For` entry that is not itself a trusted proxy.
pub fn client_addr(peer: Option<IpAddr>, forwarded_for: Option<&str>, trusted: &[Cidr]) -> String {
    let is_trusted = |ip: IpAddr| trusted.iter().any(|c| c.contains(ip));
    let Some(peer) = peer else { return "unknown".into() };
    if !is_trusted(peer) {
        return peer.to_string();
    }
    forwarded_for
        .into_iter()
        .flat_map(|v| v.rsplit(','))
        .filter_map(|s| s.trim().parse::<IpAddr>().ok())
        .find(|ip| !is_trusted(*ip))
        .unwrap_or(peer)
        .to_string()
}

impl Default for RateConfig {
    fn default() -> Self {
        RateConfig { user_per_min: 1200, token_per_min: 600, auth_per_min: 20, trusted_proxies: parse_cidrs(DEFAULT_TRUSTED_PROXIES) }
    }
}

impl RateConfig {
    /// `RATE_LIMIT_USER_PER_MIN`, `RATE_LIMIT_TOKEN_PER_MIN`, `RATE_LIMIT_AUTH_PER_MIN`, `TRUSTED_PROXIES`.
    pub fn from_env() -> Self {
        let d = RateConfig::default();
        let get = |key: &str, default: u32| std::env::var(key).ok().and_then(|v| v.trim().parse().ok()).unwrap_or(default);
        RateConfig {
            user_per_min: get("RATE_LIMIT_USER_PER_MIN", d.user_per_min),
            token_per_min: get("RATE_LIMIT_TOKEN_PER_MIN", d.token_per_min),
            auth_per_min: get("RATE_LIMIT_AUTH_PER_MIN", d.auth_per_min),
            // unset or blank: the defaults; any value with no valid range (say `none`) trusts nobody
            trusted_proxies: match std::env::var("TRUSTED_PROXIES") {
                Ok(v) if !v.trim().is_empty() => parse_cidrs(&v),
                _ => d.trusted_proxies,
            },
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

    #[test]
    fn forwarded_for_is_honoured_only_from_trusted_peers_right_most_untrusted() {
        let trusted = RateConfig::default().trusted_proxies;
        let ip = |s: &str| Some(s.parse::<IpAddr>().unwrap());
        // a public peer cannot spoof
        assert_eq!(client_addr(ip("8.8.8.8"), Some("1.2.3.4"), &trusted), "8.8.8.8");
        // the frontend proxy (private) vouches for the header
        assert_eq!(client_addr(ip("172.18.0.5"), Some("203.0.113.9"), &trusted), "203.0.113.9");
        // a client-supplied left entry is skipped: right-most untrusted wins
        assert_eq!(client_addr(ip("172.18.0.5"), Some("6.6.6.6, 203.0.113.9, 10.0.0.2"), &trusted), "203.0.113.9");
        assert_eq!(client_addr(ip("172.18.0.5"), None, &trusted), "172.18.0.5");
        assert_eq!(client_addr(None, Some("1.2.3.4"), &trusted), "unknown");
        assert!(Cidr::parse("fc00::/7").unwrap().contains("fd12::1".parse().unwrap()));
        assert!(!Cidr::parse("10.0.0.0/8").unwrap().contains("::1".parse().unwrap()));
    }
}
