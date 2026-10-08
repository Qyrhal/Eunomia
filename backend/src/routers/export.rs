//! Data export: a single JSON download of the caller's own entities (all
//! kinds, with their memory + relations) and chat history. Ported from
//! `app/routers/export.py`. `entities/service.py`, `vaults/service.py` and
//! `chat/service.py` haven't been ported to Rust yet, so the queries those
//! modules would run are written directly here (scoped to the caller's
//! personal vault, same as the Python default).
//!
//! Scope cut carried over from the Python version: cache records (raw
//! synced data) are left out -- re-derivable via a source re-sync.

use std::collections::HashMap;

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use surrealdb::{Datetime, RecordId};

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::models_user::User;
use crate::state::AppState;

const KINDS: [&str; 6] = ["person", "organisation", "location", "repository", "file", "symbol"];

pub fn router() -> Router<AppState> {
    Router::new().route("/export", get(export_data))
}

#[derive(Debug, Deserialize)]
struct EntityRow {
    id: RecordId,
    #[serde(default)]
    name: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    summary: String,
    owner: Option<RecordId>,
}

#[derive(Debug, Deserialize)]
struct MemoryRow {
    id: RecordId,
    #[serde(default)]
    text: String,
    owner: Option<RecordId>,
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    proof_count: i64,
    status: Option<String>,
    created_at: Option<Datetime>,
    updated_at: Option<Datetime>,
}

#[derive(Debug, Deserialize)]
struct RelationRow {
    id: RecordId,
    #[serde(rename = "in")]
    in_: RecordId,
    out: RecordId,
    #[serde(default)]
    label: String,
    owner: Option<RecordId>,
    created_at: Option<Datetime>,
}

#[derive(Debug, Deserialize)]
struct ChatMessageRow {
    id: RecordId,
    thread_id: RecordId,
    role: String,
    content: String,
    #[serde(default)]
    tool_calls: Option<Vec<Value>>,
    #[serde(default)]
    tool_call_id: Option<String>,
    created_at: Option<Datetime>,
}

#[derive(Debug, Deserialize)]
struct UserEmailRow {
    id: RecordId,
    email: String,
}

async fn fetch_emails(db: &Db, ids: &[RecordId]) -> AppResult<HashMap<String, String>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut dedup: Vec<RecordId> = Vec::new();
    for id in ids {
        if !dedup.contains(id) {
            dedup.push(id.clone());
        }
    }
    let mut res = db
        .query("SELECT id, email FROM user WHERE id IN $ids")
        .bind(("ids", dedup))
        .await?;
    let rows: Vec<UserEmailRow> = res.take(0)?;
    Ok(rows.into_iter().map(|r| (r.id.to_string(), r.email)).collect())
}

fn datetime_str(d: &Option<Datetime>) -> Value {
    json!(d)
}

async fn export_data(State(state): State<AppState>, user: User) -> AppResult<Response> {
    let db = &state.db;
    // the caller's own personal vault (active membership only, never one
    // they were merely invited to or joined)
    let vault = crate::vaults::service::default_vault_id(db, &user.id).await?;

    // Pass 1: gather every entity + its memory/relations, collecting the
    // owner ids we'll need emails for.
    struct Bundle {
        kind: &'static str,
        row: EntityRow,
        memories: Vec<MemoryRow>,
        outgoing: Vec<RelationRow>,
        incoming: Vec<RelationRow>,
    }

    let mut bundles: Vec<Bundle> = Vec::new();
    let mut owner_ids: Vec<RecordId> = Vec::new();

    for kind in KINDS {
        let query = format!("SELECT * FROM {kind} WHERE vault = $vault ORDER BY name");
        let mut res = db.query(query).bind(("vault", vault.clone())).await?;
        let rows: Vec<EntityRow> = res.take(0)?;

        for row in rows {
            if let Some(o) = &row.owner {
                owner_ids.push(o.clone());
            }

            let mut mem_res = db
                .query("SELECT * FROM memory WHERE subject = $id ORDER BY created_at DESC")
                .bind(("id", row.id.clone()))
                .await?;
            let memories: Vec<MemoryRow> = mem_res.take(0)?;
            for m in &memories {
                if let Some(o) = &m.owner {
                    owner_ids.push(o.clone());
                }
            }

            let mut out_res = db
                .query("SELECT * FROM relates_to WHERE in = $id")
                .bind(("id", row.id.clone()))
                .await?;
            let outgoing: Vec<RelationRow> = out_res.take(0)?;

            let mut in_res = db
                .query("SELECT * FROM relates_to WHERE out = $id")
                .bind(("id", row.id.clone()))
                .await?;
            let incoming: Vec<RelationRow> = in_res.take(0)?;

            for r in outgoing.iter().chain(incoming.iter()) {
                if let Some(o) = &r.owner {
                    owner_ids.push(o.clone());
                }
            }

            bundles.push(Bundle { kind, row, memories, outgoing, incoming });
        }
    }

    let emails = fetch_emails(db, &owner_ids).await?;
    let email_for = |id: &Option<RecordId>| -> Value {
        id.as_ref()
            .and_then(|i| emails.get(&i.to_string()))
            .map(|e| json!(e))
            .unwrap_or(Value::Null)
    };

    let entities_out: Vec<Value> = bundles
        .into_iter()
        .map(|b| {
            let memory: Vec<Value> = b
                .memories
                .iter()
                .map(|m| {
                    json!({
                        "id": m.id.to_string(),
                        "text": m.text,
                        "type": m.kind,
                        "proof_count": m.proof_count,
                        "status": m.status,
                        "created_at": datetime_str(&m.created_at),
                        "updated_at": datetime_str(&m.updated_at),
                        "owner_email": email_for(&m.owner),
                    })
                })
                .collect();

            let relations: Vec<Value> = b
                .outgoing
                .iter()
                .map(|r| {
                    json!({
                        "id": r.id.to_string(),
                        "in": r.in_.to_string(),
                        "out": r.out.to_string(),
                        "label": r.label,
                        "direction": "out",
                        "created_at": datetime_str(&r.created_at),
                        "owner_email": email_for(&r.owner),
                    })
                })
                .chain(b.incoming.iter().map(|r| {
                    json!({
                        "id": r.id.to_string(),
                        "in": r.in_.to_string(),
                        "out": r.out.to_string(),
                        "label": r.label,
                        "direction": "in",
                        "created_at": datetime_str(&r.created_at),
                        "owner_email": email_for(&r.owner),
                    })
                }))
                .collect();

            json!({
                "id": b.row.id.to_string(),
                "kind": b.kind,
                "name": b.row.name,
                "aliases": b.row.aliases,
                "summary": b.row.summary,
                "owner_email": email_for(&b.row.owner),
                "memory": memory,
                "relations": relations,
            })
        })
        .collect();

    let mut chat_res = db
        .query("SELECT * FROM chat_message WHERE owner = $owner ORDER BY created_at")
        .bind(("owner", user.id.clone()))
        .await?;
    let chat_rows: Vec<ChatMessageRow> = chat_res.take(0)?;
    let chat_history: Vec<Value> = chat_rows
        .iter()
        .map(|m| {
            json!({
                "id": m.id.to_string(),
                "thread_id": m.thread_id.to_string(),
                "role": m.role,
                "content": m.content,
                "tool_calls": m.tool_calls,
                "tool_call_id": m.tool_call_id,
                "created_at": datetime_str(&m.created_at),
            })
        })
        .collect();

    let document = json!({
        "exported_at": Utc::now().to_rfc3339(),
        "user": { "id": user.id.to_string(), "email": user.email },
        "entities": entities_out,
        "chat_history": chat_history,
    });

    let body = serde_json::to_string_pretty(&document).map_err(|e| AppError::internal(e.to_string()))?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json".to_string()),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"eunomia-export.json\"".to_string()),
        ],
        body,
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datetime_str_is_null_when_absent() {
        assert_eq!(datetime_str(&None), Value::Null);
    }

    #[test]
    fn kinds_match_the_python_entity_kind_set() {
        assert_eq!(KINDS, ["person", "organisation", "location", "repository", "file", "symbol"]);
    }
}
