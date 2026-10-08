//! Settings → HTTPS: same marker pattern as `update.rs`. This web-facing
//! process never touches docker or `.env`: it writes the desired state to
//! `https.json` in `settings.update_status_dir`, and the `updater` service
//! (scripts/auto-update.sh) validates it again, starts or removes the `caddy`
//! service, and reports progress in `https-status.json`.

use std::path::PathBuf;

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::models_user::User;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/https/status", get(get_status))
        .route("/https", post(request_https))
}

/// A public DNS name Let's Encrypt can issue for: dot-separated labels of
/// letters/digits/hyphens (not at either end of a label), ending in an
/// alphabetic TLD, so no IPs or bare hostnames. Mirrors `valid_domain` in
/// scripts/auto-update.sh.
fn valid_domain(domain: &str) -> bool {
    let labels: Vec<&str> = domain.split('.').collect();
    let Some(tld) = labels.last() else { return false };
    domain.len() <= 253
        && labels.len() >= 2
        && (2..=63).contains(&tld.len())
        && tld.chars().all(|c| c.is_ascii_alphabetic())
        && labels.iter().all(|l| {
            (1..=63).contains(&l.len())
                && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                && !l.starts_with('-')
                && !l.ends_with('-')
        })
}

/// Mirrors `valid_email` in scripts/auto-update.sh.
fn valid_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else { return false };
    email.len() <= 254
        && (1..=64).contains(&local.len())
        && local.chars().all(|c| c.is_ascii_alphanumeric() || "._%+-".contains(c))
        && valid_domain(domain)
}

#[derive(Deserialize)]
struct HttpsRequest {
    enabled: bool,
    #[serde(default)]
    domain: String,
    #[serde(default)]
    email: String,
}

/// The request as written to `https.json` (one key per line, which the
/// updater's shell parser relies on), or a 400-worthy message.
fn request_file(req: &HttpsRequest) -> Result<String, &'static str> {
    if !req.enabled {
        return Ok(serde_json::to_string_pretty(&json!({ "enabled": false, "domain": "", "email": "" })).unwrap());
    }
    let (domain, email) = (req.domain.trim().to_ascii_lowercase(), req.email.trim());
    if !valid_domain(&domain) {
        return Err("Enter a domain name like eunomia.example.com (no http://, no port, no IP address).");
    }
    if !valid_email(email) {
        return Err("Enter a valid email address for Let's Encrypt.");
    }
    Ok(serde_json::to_string_pretty(&json!({ "enabled": true, "domain": domain, "email": email })).unwrap())
}

fn status_dir(state: &AppState) -> PathBuf {
    PathBuf::from(&state.settings.update_status_dir)
}

fn read_json(path: PathBuf) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// `{"configured", "state": off|pending|active|error, "domain", "message",
/// "checked_at"}`. A request the updater hasn't picked up yet already reads
/// as its outcome-to-be (pending, or off).
async fn get_status(State(state): State<AppState>, _user: User) -> AppResult<Json<Value>> {
    let dir = status_dir(&state);
    if !dir.exists() {
        return Ok(Json(json!({ "configured": false })));
    }
    let mut status = read_json(dir.join("https-status.json")).unwrap_or_else(|| json!({ "state": "off" }));
    if let Some(req) = read_json(dir.join("https.json")) {
        let enabled = req["enabled"] == json!(true);
        status = json!({
            "state": if enabled { "pending" } else { "off" },
            "domain": if enabled { req["domain"].clone() } else { json!("") },
            "message": null,
        });
    }
    status["configured"] = json!(true);
    Ok(Json(status))
}

async fn request_https(
    State(state): State<AppState>,
    _user: User,
    Json(req): Json<HttpsRequest>,
) -> AppResult<Json<Value>> {
    let body = request_file(&req).map_err(AppError::bad_request)?;
    let dir = status_dir(&state);
    if !dir.exists() {
        return Ok(Json(json!({ "configured": false })));
    }
    std::fs::write(dir.join("https.json"), body).map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(json!({ "configured": true, "requested": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domains() {
        for ok in ["eunomia.example.com", "a.io", "my-host.example.co.uk", "x1.dev"] {
            assert!(valid_domain(ok), "{ok}");
        }
        for bad in [
            "", "localhost", "127.0.0.1", "example.c0m", "-a.example.com", "a-.example.com", "a..com",
            "https://a.com", "a.com:443", "a.com/x", "a b.com", "evil.com;rm -rf /", "x$(id).com", "a#b.com",
            "a&b.com", "a\"b.com", "a.com\n",
        ] {
            assert!(!valid_domain(bad), "{bad:?}");
        }
        assert!(!valid_domain(&format!("{}.com", "a".repeat(64))));
    }

    #[test]
    fn emails() {
        for ok in ["me@example.com", "first.last+tag@mail.example.org"] {
            assert!(valid_email(ok), "{ok}");
        }
        for bad in ["", "me", "me@localhost", "@example.com", "me @example.com", "a#b@x.com", "a&b@x.com", "me@x.com;reboot", "a\"b@x.com"] {
            assert!(!valid_email(bad), "{bad:?}");
        }
    }

    #[test]
    fn request_file_is_one_key_per_line_and_normalised() {
        let req = HttpsRequest { enabled: true, domain: " Eunomia.Example.com ".into(), email: "me@example.com".into() };
        let file = request_file(&req).unwrap();
        assert!(file.lines().any(|l| l.trim() == "\"domain\": \"eunomia.example.com\","));
        assert!(file.lines().any(|l| l.trim() == "\"enabled\": true,"));
    }

    #[test]
    fn request_file_rejects_bad_input_but_not_a_disable() {
        let bad = HttpsRequest { enabled: true, domain: "evil.com;reboot".into(), email: "me@example.com".into() };
        assert!(request_file(&bad).is_err());
        let bad = HttpsRequest { enabled: true, domain: "eunomia.example.com".into(), email: "nope".into() };
        assert!(request_file(&bad).is_err());
        let off = HttpsRequest { enabled: false, domain: "junk;".into(), email: String::new() };
        assert!(request_file(&off).unwrap().contains("\"enabled\": false"));
    }
}
