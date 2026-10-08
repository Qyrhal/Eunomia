//! Code entities (repository, file, symbol) and the relations between them.
//! Part of the tool registry (see `registry/mod.rs`); one `register` call per tool.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;

use super::{register, bad_args, parse_rid, parse_opt_rid, to_tool_value, ToolSpec};
use crate::entities::tools as entities_tools;

pub(super) fn register_all(registry: &mut HashMap<&'static str, ToolSpec>) {
    #[derive(serde::Deserialize)]
    struct CodeEntityUpsertArgs {
        kind: String,
        name: String,
        #[serde(default)]
        parent_id: Option<String>,
        #[serde(default)]
        summary: Option<String>,
        #[serde(default)]
        vault_id: Option<String>,
    }

    register(
        registry,
        "code_entity_upsert",
        json!({
            "type": "object",
            "properties": {
                "kind": {"type": "string", "enum": ["repository", "file", "symbol"]},
                "name": {"type": "string"},
                "parent_id": {"type": "string", "description": "e.g. the repository a file belongs to"},
                "summary": {"type": "string"},
                "vault_id": {"type": "string", "description": "write into this vault instead of your personal one"},
            },
            "required": ["kind", "name"],
        }),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: CodeEntityUpsertArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                let parent_id = match parse_opt_rid("parent_id", &a.parent_id) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                let vault_id = match parse_opt_rid("vault_id", &a.vault_id) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                let result = entities_tools::code_entity_upsert(
                    &state.db,
                    owner,
                    &a.kind,
                    &a.name,
                    parent_id.as_ref(),
                    a.summary.as_deref(),
                    vault_id.as_ref(),
                )
                .await;
                Ok(to_tool_value(result))
            })
        }),
    );

    #[derive(serde::Deserialize)]
    struct CodeRelateArgs {
        from_id: String,
        to_id: String,
        label: String,
        #[serde(default)]
        source_record_id: Option<String>,
    }

    register(
        registry,
        "code_relate",
        json!({
            "type": "object",
            "properties": {
                "from_id": {"type": "string"},
                "to_id": {"type": "string"},
                "label": {"type": "string"},
                "source_record_id": {"type": "string"},
            },
            "required": ["from_id", "to_id", "label"],
        }),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: CodeRelateArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                let from_id = match parse_rid("from_id", &a.from_id) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                let to_id = match parse_rid("to_id", &a.to_id) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                let result = entities_tools::code_relate(
                    &state.db,
                    owner,
                    &from_id,
                    &to_id,
                    &a.label,
                    a.source_record_id.as_deref(),
                )
                .await;
                Ok(to_tool_value(result))
            })
        }),
    );
}
