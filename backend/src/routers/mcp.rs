//! MCP server over the Streamable HTTP transport, at `/mcp`.
//!
//! Stateless: every POST carries one JSON-RPC message (or a batch, for
//! 2025-03-26 clients) and gets a plain `application/json` reply -- no
//! sessions and no server-initiated stream, so GET/DELETE are 405. Exposes
//! the same tool registry the chat agent uses (`tools::registry`), so every
//! call is owner-scoped and audit-logged exactly like REST/chat calls.
//!
//! Auth is a personal API token (`Authorization: Bearer ...`), never the
//! browser session cookie -- a page the user visits can't drive tools.

use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};

use crate::models_user::{self, User};
use crate::state::AppState;
use crate::tools::registry;

const SUPPORTED_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26"];
const DEFAULT_VERSION: &str = "2025-06-18";

const INSTRUCTIONS: &str = "Eunomia is the user's personal memory: entities (people, organisations, \
locations, code) with remembered facts, plus records synced from their connected sources. \
Use `recall` (or `reflect` for a cited answer) before answering questions about the user's world, \
`search`/`get` for raw synced records, and `memory_write` to remember new facts. \
Everything is scoped to the user's personal vault unless a `vault_id` is given.";

pub fn router() -> Router<AppState> {
    Router::new().route("/mcp", post(handle).get(method_not_allowed).delete(method_not_allowed))
}

async fn method_not_allowed() -> Response {
    (StatusCode::METHOD_NOT_ALLOWED, [(header::ALLOW, "POST")]).into_response()
}

async fn handle(State(state): State<AppState>, headers: HeaderMap, body: String) -> Response {
    if !origin_allowed(&headers, &state.settings.cors_allowed_origins) {
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    if let Some(v) = headers.get("mcp-protocol-version").and_then(|v| v.to_str().ok()) {
        if !SUPPORTED_VERSIONS.contains(&v) {
            return (StatusCode::BAD_REQUEST, format!("unsupported MCP-Protocol-Version {v}")).into_response();
        }
    }
    let Some(user) = bearer_user(&state, &headers).await else {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
            Json(json!({ "error": "missing or invalid API token -- create one on the Eunomia dashboard" })),
        )
            .into_response();
    };

    let Ok(message) = serde_json::from_str::<Value>(&body) else {
        return (StatusCode::BAD_REQUEST, Json(error_response(Value::Null, -32700, "Parse error"))).into_response();
    };

    let replies: Vec<Value> = match &message {
        Value::Array(batch) if !batch.is_empty() => {
            let mut out = Vec::new();
            for m in batch {
                if let Some(r) = handle_message(&state, &user, m).await {
                    out.push(r);
                }
            }
            if out.is_empty() {
                return StatusCode::ACCEPTED.into_response();
            }
            return Json(Value::Array(out)).into_response();
        }
        m => handle_message(&state, &user, m).await.into_iter().collect(),
    };
    match replies.into_iter().next() {
        Some(reply) => Json(reply).into_response(),
        // Notifications and client responses get no body.
        None => StatusCode::ACCEPTED.into_response(),
    }
}

async fn bearer_user(state: &AppState, headers: &HeaderMap) -> Option<User> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let token = value.strip_prefix("Bearer ").or_else(|| value.strip_prefix("bearer "))?;
    models_user::verify_api_token(&state.db, token.trim()).await.ok().flatten()
}

/// DNS-rebinding guard the spec requires: non-browser clients send no
/// Origin; a browser must be same-origin (directly or via the frontend's
/// proxy, which forwards the original host) or an allowed CORS origin.
fn origin_allowed(headers: &HeaderMap, cors_allowed_origins: &str) -> bool {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        return true;
    };
    if cors_allowed_origins.split(',').map(str::trim).any(|o| o == origin) {
        return true;
    }
    let origin_host = origin.split_once("://").map(|(_, h)| h).unwrap_or(origin);
    ["x-forwarded-host", "host"]
        .iter()
        .filter_map(|h| headers.get(*h).and_then(|v| v.to_str().ok()))
        .any(|host| host == origin_host)
}

/// One JSON-RPC message in, `Some(response)` for a request, `None` for a
/// notification or a response from the client.
async fn handle_message(state: &AppState, user: &User, message: &Value) -> Option<Value> {
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        // A client response (result/error) needs no reply; anything else is malformed.
        if message.get("result").is_some() || message.get("error").is_some() {
            return None;
        }
        return Some(error_response(message.get("id").cloned().unwrap_or(Value::Null), -32600, "Invalid Request"));
    };
    let id = message.get("id").cloned()?; // notification: nothing to send back
    let params = message.get("params").cloned().unwrap_or(Value::Null);

    Some(match method {
        "initialize" => result_response(id, initialize_result(&params)),
        "ping" => result_response(id, json!({})),
        "tools/list" => result_response(id, json!({ "tools": tool_list() })),
        "tools/call" => call_tool(state, user, id, &params).await,
        _ => error_response(id, -32601, &format!("Method not found: {method}")),
    })
}

fn negotiate_version(requested: Option<&str>) -> &'static str {
    requested
        .and_then(|r| SUPPORTED_VERSIONS.iter().find(|v| **v == r).copied())
        .unwrap_or(DEFAULT_VERSION)
}

fn initialize_result(params: &Value) -> Value {
    json!({
        "protocolVersion": negotiate_version(params.get("protocolVersion").and_then(Value::as_str)),
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "eunomia", "title": "Eunomia", "version": crate::config::APP_VERSION },
        "instructions": INSTRUCTIONS,
    })
}

fn tool_list() -> Vec<Value> {
    let mut names: Vec<&&str> = registry::all_tools().keys().collect();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let spec = &registry::all_tools()[*name];
            let schema = if spec.schema.is_object() { spec.schema.clone() } else { json!({ "type": "object", "properties": {} }) };
            json!({
                "name": name,
                "description": registry::description(name),
                "inputSchema": schema,
                "annotations": {
                    "readOnlyHint": spec.read_only,
                    "destructiveHint": registry::is_destructive(name),
                    "openWorldHint": false,
                },
            })
        })
        .collect()
}

async fn call_tool(state: &AppState, user: &User, id: Value, params: &Value) -> Value {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return error_response(id, -32602, "tools/call needs a tool `name`");
    };
    if !registry::all_tools().contains_key(name) {
        return error_response(id, -32602, &format!("Unknown tool: {name}"));
    }
    let args = match params.get("arguments") {
        None | Some(Value::Null) => json!({}),
        Some(v) => v.clone(),
    };

    // Tool failures are results the model should see (isError), not
    // protocol errors.
    let (value, is_error) = match registry::call(state, &user.id, name, args).await {
        Ok(v) => {
            let is_error = v.get("error").is_some();
            (v, is_error)
        }
        Err(e) => (json!({ "error": e.message }), true),
    };
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    result_response(id, json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }))
}

fn result_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn negotiates_supported_versions_and_falls_back() {
        assert_eq!(negotiate_version(Some("2025-03-26")), "2025-03-26");
        assert_eq!(negotiate_version(Some("2025-11-25")), "2025-11-25");
        assert_eq!(negotiate_version(Some("1999-01-01")), DEFAULT_VERSION);
        assert_eq!(negotiate_version(None), DEFAULT_VERSION);
    }

    #[test]
    fn tool_list_exposes_every_tool_with_schema_and_annotations() {
        let tools = tool_list();
        assert_eq!(tools.len(), registry::all_tools().len());
        for t in &tools {
            assert!(!t["description"].as_str().unwrap().is_empty());
            assert_eq!(t["inputSchema"]["type"], "object");
            assert!(t["annotations"]["readOnlyHint"].is_boolean());
        }
        let delete = tools.iter().find(|t| t["name"] == "vault_delete").unwrap();
        assert_eq!(delete["annotations"]["destructiveHint"], true);
        let search = tools.iter().find(|t| t["name"] == "search").unwrap();
        assert_eq!(search["annotations"]["readOnlyHint"], true);
    }

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, HeaderValue::from_static(v));
        }
        h
    }

    #[test]
    fn origin_check_allows_non_browsers_same_origin_and_cors_list() {
        let cors = "http://localhost:3000";
        assert!(origin_allowed(&headers(&[]), cors));
        assert!(origin_allowed(&headers(&[("origin", "http://localhost:3000")]), cors));
        assert!(origin_allowed(&headers(&[("origin", "http://10.0.0.5:3000"), ("x-forwarded-host", "10.0.0.5:3000")]), cors));
        assert!(origin_allowed(&headers(&[("origin", "http://localhost:8001"), ("host", "localhost:8001")]), cors));
        assert!(!origin_allowed(&headers(&[("origin", "https://evil.example"), ("host", "localhost:8001")]), cors));
    }

    #[test]
    fn initialize_advertises_tools_and_echoes_version() {
        let r = initialize_result(&json!({ "protocolVersion": "2025-03-26" }));
        assert_eq!(r["protocolVersion"], "2025-03-26");
        assert!(r["capabilities"]["tools"].is_object());
        assert_eq!(r["serverInfo"]["name"], "eunomia");
    }
}
