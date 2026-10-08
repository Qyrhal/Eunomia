//! Mirrors the parsing the MCP router does on a request body (`routers/mcp.rs`
//! keeps its handlers private): body -> Value -> batch or single message ->
//! method/id/params extraction -> tool lookup -> pretty-print. Must never panic.
#![no_main]
use libfuzzer_sys::fuzz_target;
use serde_json::Value;

fn message(m: &Value) {
    let _ = m.get("method").and_then(Value::as_str);
    let _ = m.get("id").cloned();
    let params = m.get("params").cloned().unwrap_or(Value::Null);
    if let Some(name) = params.get("name").and_then(Value::as_str) {
        let _ = eunomia_backend::tools::registry::all_tools().contains_key(name);
    }
    let _ = serde_json::to_string_pretty(&params);
}

fuzz_target!(|data: &[u8]| {
    let Ok(body) = std::str::from_utf8(data) else { return };
    let Ok(v) = serde_json::from_str::<Value>(body) else { return };
    match &v {
        Value::Array(batch) => batch.iter().for_each(message),
        m => message(m),
    }
});
