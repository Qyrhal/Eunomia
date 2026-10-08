//! Failure capsules: a redacted, size-capped record of a request or tool call that failed, keyed
//! by trace id, so `eunomia replay <trace_id>` can re-run it (docs/debugging.md).
//!
//! Recorded when a registry tool fails with anything but `validation.invalid`, and when a route
//! answers 5xx. Secrets never reach the table: [`redact`] masks secret-looking keys and
//! [`scrub`] masks bearer tokens and long token-like strings in any text.

use surrealdb::types::SurrealValue;
use axum::{
    body::Body,
    extract::{MatchedPath, Request, State},
    middleware::Next,
    response::Response,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use surrealdb::types::RecordId;
use crate::rid::RecordIdExt;

use crate::auth::Authn;
use crate::db::Db;
use crate::error::{AppResult, ErrorCode};
use crate::models_user::User;
use crate::state::AppState;
use crate::store;

/// Hard cap on one stored capsule (its text fields together).
pub const MAX_CAPSULE_BYTES: usize = 16 * 1024;
const MAX_SOURCE_CHARS: usize = 2000;
const KEEP_DAYS: &str = "7d";
const KEEP_ROWS: i64 = 1000;
const SECRET_HINTS: &[&str] = &["token", "secret", "password", "key", "authorization", "cookie"];

/// The code and raw cause of a failed response, attached by `AppError::into_response` for [`capture`].
#[derive(Clone)]
pub struct FailureInfo {
    pub code: ErrorCode,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Capsule {
    pub trace_id: String,
    /// `tool` or `route`.
    pub kind: String,
    /// The tool name, or `METHOD /matched/route` for a route.
    pub name: String,
    /// The caller's record id, if the request was authenticated.
    pub user: Option<String>,
    /// Tool arguments, or `{method, uri, body}` for a route. Secrets are masked.
    #[schema(value_type = Object)]
    pub args: Value,
    pub code: String,
    pub status: u16,
    /// The raw error text. Never sent to ordinary clients; admins only.
    pub source: String,
    pub version: String,
    /// True when `args` was cut to fit the size cap (not replayable).
    pub truncated: bool,
    pub created_at: String,
}

pub struct Failure {
    pub kind: &'static str,
    pub name: String,
    pub user: Option<String>,
    pub args: Value,
    pub code: ErrorCode,
    pub status: u16,
    pub source: String,
}

/// Masks the value of every secret-looking key, at any depth, and scrubs every string.
pub fn redact(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, v)| {
                    let lower = k.to_lowercase();
                    let hit = SECRET_HINTS.iter().any(|h| lower.contains(h));
                    (k.clone(), if hit { json!("***") } else { redact(v) })
                })
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(redact).collect()),
        Value::String(s) => Value::String(scrub(s)),
        other => other.clone(),
    }
}

/// Masks the word after `Bearer` and any run of 40 or more token characters (API tokens, JWTs, hashes).
pub fn scrub(s: &str) -> String {
    let is_delim = |c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    let (mut out, mut after_bearer) = (String::with_capacity(s.len()), false);
    for piece in s.split_inclusive(is_delim) {
        let (word, tail) = match piece.char_indices().last() {
            Some((i, c)) if is_delim(c) => piece.split_at(i),
            _ => (piece, ""),
        };
        if !word.is_empty() {
            out.push_str(if after_bearer || word.len() >= 40 { "***" } else { word });
            after_bearer = word.eq_ignore_ascii_case("bearer");
        }
        out.push_str(tail);
    }
    out
}

fn cut(s: &str, max: usize) -> &str {
    let mut end = max.min(s.len());
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Stores a capsule for the current trace. Never fails the caller: a capsule that cannot be written is logged.
pub async fn record(db: &Db, f: Failure) {
    let trace_id = crate::telemetry::current_trace_id();
    let source: String = scrub(&f.source).chars().take(MAX_SOURCE_CHARS).collect();
    let version = crate::config::APP_VERSION;
    let used = trace_id.len() + f.name.len() + f.user.as_ref().map_or(0, String::len) + source.len() + version.len() + 256;
    let args = redact(&f.args).to_string();
    let budget = MAX_CAPSULE_BYTES.saturating_sub(used);
    let (args, truncated) = if args.len() > budget { (cut(&args, budget).to_string(), true) } else { (args, false) };

    let res: Result<(), String> = async {
        store::capsules::INSERT
        .on(db)
        .bind(("id", RecordId::from_table_key("failure_capsule", trace_id.clone())))
        .bind(("trace_id", trace_id))
        .bind(("kind", f.kind))
        .bind(("name", f.name))
        .bind(("user", f.user))
        .bind(("args", args))
        .bind(("code", f.code.as_str()))
        .bind(("status", f.status as i64))
        .bind(("source", source))
        .bind(("version", version))
        .bind(("truncated", truncated))
        .await
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;
        Ok(())
    }
    .await;
    if let Err(e) = res {
        tracing::warn!(error = %e, "recording a failure capsule failed");
    }
}

#[derive(Deserialize, SurrealValue)]
struct Row {
    trace_id: String,
    kind: String,
    name: String,
    user: Option<String>,
    args: String,
    code: String,
    status: i64,
    source: String,
    version: String,
    truncated: bool,
    created_at: String,
}

pub async fn get(db: &Db, trace_id: &str) -> AppResult<Option<Capsule>> {
    let mut res = store::capsules::GET.on(db).bind(("trace_id", trace_id.to_string())).await?;
    let rows: Vec<Row> = res.take(0)?;
    Ok(rows.into_iter().next().map(|r| Capsule {
        args: serde_json::from_str(&r.args).unwrap_or(Value::Null),
        trace_id: r.trace_id,
        kind: r.kind,
        name: r.name,
        user: r.user,
        code: r.code,
        status: r.status as u16,
        source: r.source,
        version: r.version,
        truncated: r.truncated,
        created_at: r.created_at,
    }))
}

/// Deletes capsules older than `age` (a SurrealQL duration such as `7d`), then all but the `max` newest.
pub async fn prune(db: &Db, age: &str, max: i64) -> AppResult<()> {
    store::capsules::PRUNE_OLD.on(db).bind(("age", age.to_string())).await?.check()?;
    store::capsules::PRUNE_EXCESS.on(db).bind(("max", max)).await?.check()?;
    Ok(())
}

/// The retention the `prune_capsules` job applies: 7 days or 1000 rows.
pub async fn prune_default(db: &Db) -> AppResult<()> {
    prune(db, KEEP_DAYS, KEEP_ROWS).await
}

/// The instance's first user, who owns the `prune_capsules` job rows.
pub async fn first_user(db: &Db) -> AppResult<Option<RecordId>> {
    #[derive(Deserialize, SurrealValue)]
    struct R {
        id: RecordId,
    }
    let mut res = store::capsules::FIRST_USER.on(db).await?;
    Ok(res.take::<Vec<R>>(0)?.into_iter().next().map(|r| r.id))
}

/// Admin = the instance's first user, or an email listed in `EUNOMIA_ADMIN_EMAILS` (comma separated).
pub async fn is_admin(db: &Db, user: &User) -> AppResult<bool> {
    let listed = std::env::var("EUNOMIA_ADMIN_EMAILS")
        .unwrap_or_default()
        .split(',')
        .any(|e| !e.trim().is_empty() && e.trim().eq_ignore_ascii_case(&user.email));
    Ok(listed || first_user(db).await?.as_ref() == Some(&user.id))
}

/// Records a capsule for every 5xx response. Sits inside the gate (so the caller is known) and
/// buffers small request bodies so the route can be replayed.
pub async fn capture(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let uri = req.uri().to_string();
    let route = req.extensions().get::<MatchedPath>().map_or_else(|| req.uri().path().to_string(), |m| m.as_str().to_string());
    let user = req.extensions().get::<Authn>().map(|a| a.user.id.to_string());
    let small = req
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
        .is_some_and(|n| n > 0 && n <= MAX_CAPSULE_BYTES);

    let (parts, body) = req.into_parts();
    let (body, saved) = if small {
        match axum::body::to_bytes(body, MAX_CAPSULE_BYTES).await {
            Ok(b) => (Body::from(b.clone()), Some(b)),
            Err(_) => (Body::empty(), None),
        }
    } else {
        (body, None)
    };
    let resp = next.run(Request::from_parts(parts, body)).await;

    if resp.status().is_server_error() {
        let body = saved.map_or(Value::Null, |b| serde_json::from_slice(&b).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&b).into_owned())));
        let info = resp.extensions().get::<FailureInfo>().cloned();
        let (code, source) = info.map_or((ErrorCode::Internal, String::new()), |i| (i.code, i.source));
        record(
            &state.db,
            Failure {
                kind: "route",
                name: format!("{method} {route}"),
                user,
                args: json!({ "method": method.as_str(), "uri": uri, "body": body }),
                code,
                status: resp.status().as_u16(),
                source,
            },
        )
        .await;
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_keys_at_any_depth_and_bearer_strings() {
        let v = json!({ "q": "hi", "API_Key": "a", "nested": { "Authorization": "x", "list": [{ "cookie": "y" }] },
                        "note": "sent Bearer abc123 to host", "long": "x".repeat(50) });
        let s = redact(&v).to_string();
        for leak in ["\"a\"", "\"x\"", "\"y\"", "abc123", &"x".repeat(50)] {
            assert!(!s.contains(leak), "{leak} leaked in {s}");
        }
        assert!(s.contains("\"hi\"") && s.contains("Bearer ***") && s.contains("to host"));
    }

    #[test]
    fn scrub_keeps_short_words_and_punctuation() {
        assert_eq!(scrub("db error: person:abc not found."), "db error: person:abc not found.");
    }
}
