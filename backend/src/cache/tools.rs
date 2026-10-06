//! `recall`/`reflect` tool-facing wrappers, owner-scoped.
//!
//! Deferred: `cache/tools.py`'s `register_tool(...)` / `@safe` plumbing
//! (wiring these into the Python backend's MCP tool registry, via
//! `tools/registry.py` and `tools/generic.py::safe`) is NOT ported here --
//! it's tool-registration glue, not business logic, and this crate has no
//! MCP registry to register into yet (`src/tools/registry.rs` exists for
//! `tools::generic`'s handful of inlined cache queries, but there's no
//! router/dispatcher wired up for a `recall`/`reflect` tool surface in this
//! pass). What's ported is the one real piece of logic `tools.py` added on
//! top of `recall`/`reflect`: coercing a `[since, until]` JSON array into the
//! `(since, until)` tuple `cache::recall::recall` expects, and shaping the
//! `recall` response as `{"results": [...]}`.
//!
//! Ported from `cache/tools.py`.

use serde_json::{json, Value};
use surrealdb::RecordId;

use crate::cache::recall::{self, MemoryType};
use crate::cache::reflect;
use crate::config::Settings;
use crate::db::Db;
use crate::error::AppResult;

/// Mirrors `cache/tools.py::recall`'s `time_range = tuple(time_range) if
/// time_range else None` coercion -- a 2-element `[since, until]` JSON array
/// in, an `(since, until)` tuple out.
fn time_range_tuple(time_range: Option<&[String]>) -> Option<(&str, &str)> {
    match time_range {
        Some([since, until]) => Some((since.as_str(), until.as_str())),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn recall_tool(
    db: &Db,
    settings: &Settings,
    owner: &RecordId,
    query: &str,
    time_range: Option<&[String]>,
    limit: usize,
    max_tokens: Option<usize>,
    types: Option<&[MemoryType]>,
    vault_id: Option<&RecordId>,
) -> AppResult<Value> {
    let results = recall::recall(db, settings, owner, query, time_range_tuple(time_range), limit, max_tokens, types, vault_id).await?;
    Ok(json!({ "results": results }))
}

pub async fn reflect_tool(
    db: &Db,
    settings: &Settings,
    owner: &RecordId,
    query: &str,
    vault_id: Option<&RecordId>,
    limit: usize,
) -> AppResult<Value> {
    reflect::reflect(db, settings, owner, query, vault_id, limit).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_range_tuple_converts_two_element_array() {
        let v = vec!["2024-01-01".to_string(), "2024-02-01".to_string()];
        assert_eq!(time_range_tuple(Some(&v)), Some(("2024-01-01", "2024-02-01")));
    }

    #[test]
    fn time_range_tuple_is_none_when_absent_or_wrong_length() {
        assert_eq!(time_range_tuple(None), None);
        let one = vec!["2024-01-01".to_string()];
        assert_eq!(time_range_tuple(Some(&one)), None);
    }
}
