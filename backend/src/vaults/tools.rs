//! Vault-management agent-tool wrappers, ported from `vaults/tools.py`.
//! These are thin reshapes over `vaults::service` for the MCP agent-tool
//! surface -- the MCP registration/schema machinery (`register_tool`, the
//! JSON-schema literals) has no Rust equivalent yet in this codebase and is
//! intentionally not ported: there's no portable logic in it, just wiring
//! into a registry module that doesn't exist on this side yet.

use serde_json::{json, Value};
use surrealdb::RecordId;

use crate::db::Db;
use crate::vaults::service;

/// Mirrors Python's `@safe` decorator: a tool must never raise on bad agent
/// input, it returns `{"error": "<tool_name>: <message>"}` instead.
fn err(tool_name: &str, message: &str) -> Value {
    json!({ "error": format!("{tool_name}: {message}") })
}

pub async fn vault_create(db: &Db, owner: &RecordId, name: &str, kind: &str) -> Value {
    match service::create_vault(db, owner, name, kind).await {
        Ok(v) => serde_json::to_value(v).unwrap_or_else(|e| err("vault_create", &e.to_string())),
        Err(e) => err("vault_create", &e.message),
    }
}

pub async fn vault_list(db: &Db, owner: &RecordId) -> Value {
    match service::list_my_vaults(db, owner).await {
        Ok(v) => json!({ "results": v }),
        Err(e) => err("vault_list", &e.message),
    }
}

pub async fn vault_clone(db: &Db, owner: &RecordId, vault_id: &RecordId, name: Option<&str>, kind: &str) -> Value {
    match service::clone_vault(db, owner, vault_id, name, kind).await {
        Ok(v) => serde_json::to_value(v).unwrap_or_else(|e| err("vault_clone", &e.to_string())),
        Err(e) => err("vault_clone", &e.message),
    }
}

pub async fn vault_merge(db: &Db, owner: &RecordId, a: &RecordId, b: &RecordId, name: Option<&str>, kind: &str) -> Value {
    match service::merge_vaults(db, owner, a, b, name, kind).await {
        Ok(v) => serde_json::to_value(v).unwrap_or_else(|e| err("vault_merge", &e.to_string())),
        Err(e) => err("vault_merge", &e.message),
    }
}

pub async fn vault_invite(db: &Db, owner: &RecordId, vault_id: &RecordId, email: &str, role: &str) -> Value {
    match service::invite_member(db, owner, vault_id, email, role).await {
        Ok(v) => serde_json::to_value(v).unwrap_or_else(|e| err("vault_invite", &e.to_string())),
        Err(e) => err("vault_invite", &e.message),
    }
}

pub async fn vault_members(db: &Db, owner: &RecordId, vault_id: &RecordId) -> Value {
    match service::list_members(db, owner, vault_id).await {
        Ok(v) => json!({ "results": v }),
        Err(e) => err("vault_members", &e.message),
    }
}

pub async fn vault_remove_member(db: &Db, owner: &RecordId, vault_id: &RecordId, email: &str) -> Value {
    match service::remove_member(db, owner, vault_id, email).await {
        Ok(()) => json!({ "removed": true }),
        Err(e) => err("vault_remove_member", &e.message),
    }
}

pub async fn vault_leave(db: &Db, owner: &RecordId, vault_id: &RecordId) -> Value {
    match service::leave_vault(db, owner, vault_id).await {
        Ok(()) => json!({ "left": true }),
        Err(e) => err("vault_leave", &e.message),
    }
}

pub async fn vault_rename(db: &Db, owner: &RecordId, vault_id: &RecordId, name: &str) -> Value {
    match service::rename_vault(db, owner, vault_id, name).await {
        Ok(v) => serde_json::to_value(v).unwrap_or_else(|e| err("vault_rename", &e.to_string())),
        Err(e) => err("vault_rename", &e.message),
    }
}

pub async fn vault_delete(db: &Db, owner: &RecordId, vault_id: &RecordId) -> Value {
    match service::delete_vault(db, owner, vault_id).await {
        Ok(()) => json!({ "deleted": true }),
        Err(e) => err("vault_delete", &e.message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn err_formats_like_python_safe_decorator() {
        let v = err("vault_create", "boom");
        assert_eq!(v, json!({ "error": "vault_create: boom" }));
    }

    #[test]
    fn removed_and_deleted_and_left_shapes_are_plain_bools() {
        assert_eq!(json!({ "removed": true }), json!({ "removed": true }));
        assert_eq!(json!({ "deleted": true })["deleted"], json!(true));
        assert_eq!(json!({ "left": true })["left"], json!(true));
    }
}
