//! The one tool registry consumed by both the MCP server and the REST
//! surface. Ported from `tools/registry.py`. Merges: generic cache tools
//! (`tools::generic`) + per-domain tools (`entities`, `vaults`, `cache`),
//! wired up in [`register_all`]. Per-source tools (`sources::registry`)
//! aren't ported yet -- that module doesn't exist on this side.
//!
//! Async end to end, same reason as the Python version: handlers need to be
//! `async fn` for axum concurrency, same shape works for an MCP server.
//!
//! Audit trail: every call through [`call`] for a tool NOT in
//! [`READ_ONLY_TOOLS`] writes one `audit_log` row (owner, tool name, a
//! redacted/truncated summary of the args, outcome, timestamp) -- this is
//! the single choke point every tool call (REST and MCP) already goes
//! through, so hooking it in here means no per-tool instrumentation. A
//! read-only tool returning an `{"error": ...}` value (rather than failing)
//! still counts as "ok" for logging purposes since it's excluded from
//! logging entirely either way.

use surrealdb::types::SurrealValue;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use serde_json::{json, Value};
use surrealdb::types::RecordId;
use crate::rid::RecordIdExt;
use tracing::{field::Empty, Instrument};

use crate::audit::{self, Event};
use crate::authz;
use crate::db::Db;
use crate::scopes;
use crate::store;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::state::AppState;

/// A registered tool handler: owner-scoped, takes the raw JSON args object,
/// returns a JSON result (an `{"error": ...}` value on a handled failure, or
/// an `Err` for anything that should surface as a 500 / propagate).
pub type ToolFn =
    Arc<dyn for<'a> Fn(&'a AppState, &'a RecordId, Value) -> BoxFuture<'a, AppResult<Value>> + Send + Sync>;

pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

#[derive(Clone)]
pub struct ToolSpec {
    pub read_only: bool,
    pub schema: Value,
    pub handler: ToolFn,
}

/// Tools that only read data -- excluded from the audit log. Everything
/// else registered (now or later) is assumed mutating and gets logged; this
/// is a small explicit allowlist rather than a generic classifier, since the
/// tool count is small and known (mirrors the Python module's docstring).
pub const READ_ONLY_TOOLS: &[&str] = &[
    "docs",
    "search",
    "get",
    "list",
    "links",
    "recall",
    "reflect",
    "entities_search",
    "entities_get",
    "entities_graph",
    "vault_list",
    "vault_members",
];

pub fn is_read_only(name: &str) -> bool {
    READ_ONLY_TOOLS.contains(&name)
}

/// Tools that irreversibly remove data or access -- flagged to MCP clients
/// (`destructiveHint`) so they can ask before running them.
pub fn is_destructive(name: &str) -> bool {
    matches!(name, "memory_delete" | "entity_delete" | "entity_merge" | "vault_delete" | "vault_remove_member" | "vault_leave")
}

/// Tools over the user's raw synced source records, which belong to no vault:
/// a vault-restricted token may not use them.
fn reads_source_records(name: &str) -> bool {
    matches!(name, "search" | "get" | "list" | "links")
}

/// What each tool does, for the model choosing between them (chat agent and
/// MCP clients alike). Every registered tool must have one -- see tests.
pub fn description(name: &str) -> &'static str {
    match name {
        "docs" => "Read Eunomia's own documentation: how to install, connect agents, every tool, vaults/memories/recall concepts, deployment. No arguments lists the docs; `topic` returns one in full.",
        "search" => "Search the user's synced source records (transactions, meetings, messages, ...) by keyword. Supports filtering by source, record type and time range. Returns record ids usable with `get` and `links`.",
        "get" => "Fetch one synced source record by id, with its full content.",
        "list" => "List synced source records, optionally filtered by type and field values, sorted and paginated. Use for browsing rather than searching.",
        "links" => "List the records linked to a given record (e.g. a meeting's attendees, a transaction's merchant), optionally only one relation type.",
        "recall" => "Retrieve the memories most relevant to a question from the user's memory graph, ranked by combining semantic, keyword, graph and recency signals within a token budget. Use this first when answering questions about people, organisations, places or past events.",
        "reflect" => "Answer a question by synthesizing from recalled memories, with numbered citations. Answers only from what is already remembered; use `recall` to inspect the raw memories instead.",
        "entities_search" => "Find entities (people, organisations, locations, repositories, files, symbols) by name. Returns entity ids for `entities_get`, `memory_write` and `code_relate`.",
        "entities_get" => "Get one entity with its memories (facts and consolidated observations) and its relations to other entities.",
        "entities_graph" => "Get the entity relationship graph (nodes and edges), optionally restricted to some entity kinds or a vault.",
        "code_entity_upsert" => "Find-or-create a code entity -- a repository, file or symbol -- matched case-insensitively by name, with a short summary. `parent_id` links a file to its repository or a symbol to its file. Returns the entity id.",
        "code_relate" => "Record a labelled relation between two entities, e.g. symbol `calls` symbol, file `imports` file.",
        "memory_write" => "Remember a fact about a person, organisation, location or code entity (created if new). Type `world` for objective facts, `experience` for things that happened, `observation` for stable patterns.",
        "consolidate_observations" => "Fold an entity's new raw facts into its consolidated observation (or do it for every entity with new facts when `subject_id` is omitted).",
        "memory_update" => "Edit one memory by id: change its `text`, and/or switch a raw fact between `world` and `experience`. To revise an entity's consolidated belief, `memory_write` with type `observation` instead.",
        "entity_update" => "Edit an entity's `name`, `aliases` (replaces the list) and/or `summary`.",
        "memory_delete" => "Delete one memory by id. Irreversible.",
        "entity_delete" => "Delete an entity and its memories and relations. Irreversible.",
        "entity_merge" => "Merge a duplicate entity (`loser_id`) into another of the same kind (`winner_id`): memories and relations move to the winner, the loser's name becomes an alias, and the loser is deleted.",
        "vault_create" => "Create a vault (a shared scope for entities and memories); the caller becomes its owner.",
        "vault_list" => "List the vaults the user belongs to, with their role in each. The personal vault is the default scope for every other tool.",
        "vault_clone" => "Copy a vault's entities and memories into a new vault owned by the caller.",
        "vault_merge" => "Merge two vaults you belong to into a NEW vault: both are copied into it (the originals are never changed), and entities with the same kind and name are folded together -- aliases unioned, identical facts kept once, relations deduplicated.",
        "vault_invite" => "Invite a registered user by email to a vault (owner only). They must accept the invitation before getting access.",
        "vault_members" => "List a vault's members and their roles.",
        "vault_remove_member" => "Remove a member from a vault, or withdraw a pending invitation (owner only). Cannot remove the last owner.",
        "vault_leave" => "Leave a vault. The last owner can't leave while others remain.",
        "vault_rename" => "Rename a vault (owner only).",
        "vault_delete" => "Delete a vault and all memberships (owner only). Irreversible.",
        _ => "",
    }
}

// args keys that look like secrets -- redacted rather than written to the
// audit log's args_summary.
const SECRET_KEY_HINTS: &[&str] = &["password", "token", "secret", "credential", "api_key", "apikey"];
const ARGS_SUMMARY_MAX: usize = 500;

fn summarize_args(args: &Value) -> String {
    let mut redacted = serde_json::Map::new();
    if let Some(obj) = args.as_object() {
        for (k, v) in obj {
            let lower = k.to_lowercase();
            if SECRET_KEY_HINTS.iter().any(|hint| lower.contains(hint)) {
                redacted.insert(k.clone(), json!("***"));
            } else {
                redacted.insert(k.clone(), v.clone());
            }
        }
    }
    let mut summary = Value::Object(redacted).to_string();
    if summary.chars().count() > ARGS_SUMMARY_MAX {
        summary = summary.chars().take(ARGS_SUMMARY_MAX).collect::<String>() + "\u{2026}";
    }
    summary
}

/// The owner-facing `audit_log` row (what `GET /api/audit` shows) plus the
/// append-only `audit_event` row with the actor, the outcome code and the trace id.
async fn record_audit(db: &Db, owner: &RecordId, tool_name: &str, args: &Value, outcome: &str, code: &str) {
    let summary = summarize_args(args);
    let _ = store::cache::RECORD_AUDIT
        .on(db)
        .bind(("owner", owner.clone()))
        .bind(("tool_name", tool_name.to_string()))
        .bind(("args_summary", summary.clone()))
        .bind(("outcome", outcome.to_string()))
        .await;
    record_event(db, owner, tool_name, code, &summary).await;
}

async fn record_event(db: &Db, owner: &RecordId, tool_name: &str, code: &str, detail: &str) {
    let actor = authz::current().map(|c| c.actor).unwrap_or(authz::Actor { kind: "user", id: owner.to_string() });
    let action = format!("tool.{tool_name}");
    audit::record(db, Event { user: Some(owner), actor: &actor, action: &action, target: "", outcome: code, detail }).await;
}

/// The process-wide tool registry. A `OnceLock<HashMap<..>>` rather than
/// Python's import-time `_EXTRA` dict + three merge sources (`generic.IMPLS`,
/// `sources.registry.tool_registry()`, `_EXTRA`) computed fresh on every
/// `all_tools()` call -- Rust has no import-time side effects to piggyback
/// on, so registration happens once, explicitly, via `register_all` below.
static REGISTRY: OnceLock<HashMap<&'static str, ToolSpec>> = OnceLock::new();

/// Used by anything adding tools before first use (mirrors Python's
/// `register_tool`, called at import time there; called once from
/// `register_all` here).
pub fn register(registry: &mut HashMap<&'static str, ToolSpec>, name: &'static str, schema: Value, handler: ToolFn) {
    registry.insert(name, ToolSpec { read_only: is_read_only(name), schema, handler });
}

/// Builds the full tool map by delegating to `register_all`.
fn build_registry() -> HashMap<&'static str, ToolSpec> {
    let mut registry = HashMap::new();
    register_all(&mut registry);
    registry
}

/// A JSON-decode failure for a tool's `args` object -- mirrors a Python tool
/// raising `TypeError`/`ValueError` on bad agent input, which `@safe` (see
/// `tools/generic.py`) turns into `{"error": ...}` rather than a 500.
fn bad_args(e: serde_json::Error) -> Value {
    json!({ "error": format!("invalid arguments: {e}") })
}

fn bad_id(field: &str, value: &str) -> Value {
    json!({ "error": format!("invalid {field}: {value:?}") })
}

/// Parses a tool argument string into a `RecordId`, yielding an `{"error":
/// ...}` value (not an `Err`) on failure -- same "never raise on bad agent
/// input" contract every tool in the Python registry follows via `@safe`.
fn parse_rid(field: &str, value: &str) -> Result<RecordId, Value> {
    crate::rid::parse(value).map_err(|_| bad_id(field, value))
}

fn parse_opt_rid(field: &str, value: &Option<String>) -> Result<Option<RecordId>, Value> {
    match value {
        None => Ok(None),
        Some(v) => parse_rid(field, v).map(Some),
    }
}

/// Converts a typed service-layer result into the tool-call `Value` shape:
/// `Ok` serializes to JSON, `Err` becomes `{"error": <message>}` -- mirrors
/// Python's `@safe` decorator (`tools/generic.py`), which never lets a
/// handled service error raise past the registry.
fn to_tool_value<T: serde::Serialize>(result: AppResult<T>) -> Value {
    match result {
        Ok(v) => serde_json::to_value(v).unwrap_or_else(|e| json!({ "error": e.to_string() })),
        Err(e) => e.to_tool_value(),
    }
}

/// Registers every tool the Python registry exposes (`tools/registry.py`'s
/// `all_tools()`, merging `tools/generic.py`, `entities/tools.py`,
/// `cache/tools.py`, `vaults/tools.py`): the four generic cache tools, the
/// entity-memory tools, `recall`/`reflect`, and vault management. Per-source
/// tools (`sources::registry`) aren't wired in yet -- that module doesn't
/// exist on the Rust side.
fn register_all(registry: &mut HashMap<&'static str, ToolSpec>) {
    #[derive(serde::Deserialize, Default)]
    struct DocsArgs {
        #[serde(default)]
        topic: Option<String>,
    }
    register(
        registry,
        "docs",
        json!({
            "type": "object",
            "properties": {
                "topic": {"type": "string", "description": "quickstart, installation, agents, concepts or deployment; omit to list"},
            },
        }),
        Arc::new(|_state, _owner, args| {
            Box::pin(async move {
                let a: DocsArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                let list = || crate::docs::DOCS.iter().map(|d| json!({ "topic": d.slug, "title": d.title })).collect::<Vec<_>>();
                Ok(match a.topic {
                    None => json!({ "docs": list() }),
                    Some(t) => match crate::docs::find(&t) {
                        Some(d) => json!({ "topic": d.slug, "title": d.title, "markdown": d.body }),
                        None => json!({ "error": format!("no doc called {t:?}"), "docs": list() }),
                    },
                })
            })
        }),
    );

    use crate::cache::recall::MemoryType;
    use crate::cache::tools as cache_tools;
    use crate::entities::service::KINDS;
    use crate::entities::tools as entities_tools;
    use crate::tools::generic;
    use crate::vaults::tools as vaults_tools;

    // -- tools::generic: search, get, list, links (read-only) ---------------

    #[derive(serde::Deserialize, Default)]
    struct SearchArgs {
        query: String,
        #[serde(default)]
        sources: Option<Vec<String>>,
        #[serde(default)]
        types: Option<Vec<String>>,
        #[serde(default)]
        since: Option<String>,
        #[serde(default)]
        until: Option<String>,
        #[serde(default = "default_mode")]
        mode: String,
        #[serde(default = "default_search_limit")]
        limit: i64,
        #[serde(default)]
        offset: i64,
    }
    fn default_mode() -> String {
        "hybrid".to_string()
    }
    fn default_search_limit() -> i64 {
        20
    }

    register(
        registry,
        "search",
        json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "sources": {"type": "array", "items": {"type": "string"}},
                "types": {"type": "array", "items": {"type": "string"}},
                "since": {"type": "string", "description": "ISO 8601"},
                "until": {"type": "string", "description": "ISO 8601"},
                "mode": {"type": "string", "enum": ["keyword", "semantic", "hybrid"]},
                "limit": {"type": "integer"},
                "offset": {"type": "integer"},
            },
            "required": ["query"],
        }),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: SearchArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                generic::search(
                    &state.db,
                    &state.settings,
                    owner,
                    &a.query,
                    a.sources.as_deref(),
                    a.types.as_deref(),
                    a.since.as_deref(),
                    a.until.as_deref(),
                    &a.mode,
                    a.limit,
                    a.offset,
                )
                .await
            })
        }),
    );

    #[derive(serde::Deserialize)]
    struct IdArgs {
        id: String,
    }

    register(
        registry,
        "get",
        json!({"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]}),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: IdArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                generic::get(&state.db, owner, &a.id).await
            })
        }),
    );

    #[derive(serde::Deserialize, Default)]
    struct ListArgs {
        #[serde(rename = "type", default)]
        type_: Option<String>,
        #[serde(default)]
        filters: Option<Value>,
        #[serde(default = "default_sort")]
        sort: String,
        #[serde(default = "default_list_limit")]
        limit: i64,
        #[serde(default)]
        offset: i64,
    }
    fn default_sort() -> String {
        "-occurred_at".to_string()
    }
    fn default_list_limit() -> i64 {
        50
    }

    register(
        registry,
        "list",
        json!({
            "type": "object",
            "properties": {
                "type": {"type": "string"},
                "filters": {"type": "object"},
                "sort": {"type": "string"},
                "limit": {"type": "integer"},
                "offset": {"type": "integer"},
            },
        }),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: ListArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                let filters = a.filters.as_ref().and_then(|v| v.as_object());
                generic::list(&state.db, owner, a.type_.as_deref(), filters, &a.sort, a.limit, a.offset).await
            })
        }),
    );

    #[derive(serde::Deserialize)]
    struct LinksArgs {
        id: String,
        #[serde(default)]
        rel: Option<String>,
    }

    register(
        registry,
        "links",
        json!({
            "type": "object",
            "properties": {"id": {"type": "string"}, "rel": {"type": "string"}},
            "required": ["id"],
        }),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: LinksArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                generic::links(&state.db, owner, &a.id, a.rel.as_deref()).await
            })
        }),
    );

    // -- cache::tools: recall, reflect (read-only) ---------------------------

    #[derive(serde::Deserialize, Default)]
    struct RecallArgs {
        query: String,
        #[serde(default)]
        time_range: Option<Vec<String>>,
        #[serde(default = "default_recall_limit")]
        limit: usize,
        #[serde(default)]
        max_tokens: Option<usize>,
        #[serde(default)]
        types: Option<Vec<MemoryType>>,
        #[serde(default)]
        vault_id: Option<String>,
    }
    fn default_recall_limit() -> usize {
        20
    }

    register(
        registry,
        "recall",
        json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "time_range": {
                    "type": "array",
                    "items": {"type": "string"},
                    "minItems": 2,
                    "maxItems": 2,
                    "description": "[since, until] ISO 8601",
                },
                "limit": {"type": "integer"},
                "max_tokens": {"type": "integer"},
                "types": {
                    "type": "array",
                    "items": {"type": "string", "enum": ["world", "experience", "observation"]},
                    "description": "filter memory-sourced results by memory.type",
                },
                "vault_id": {"type": "string", "description": "recall from this vault instead of your personal one"},
            },
            "required": ["query"],
        }),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: RecallArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                let vault_id = match parse_opt_rid("vault_id", &a.vault_id) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                cache_tools::recall_tool(
                    &state.db,
                    &state.settings,
                    owner,
                    &a.query,
                    a.time_range.as_deref(),
                    a.limit,
                    a.max_tokens,
                    a.types.as_deref(),
                    vault_id.as_ref(),
                )
                .await
            })
        }),
    );

    #[derive(serde::Deserialize)]
    struct ReflectArgs {
        query: String,
        #[serde(default)]
        vault_id: Option<String>,
        #[serde(default = "default_reflect_limit")]
        limit: usize,
    }
    fn default_reflect_limit() -> usize {
        10
    }

    register(
        registry,
        "reflect",
        json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "vault_id": {"type": "string", "description": "reflect over this vault instead of your personal one"},
                "limit": {"type": "integer", "description": "how many recalled memories to consider"},
            },
            "required": ["query"],
        }),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: ReflectArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                let vault_id = match parse_opt_rid("vault_id", &a.vault_id) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                cache_tools::reflect_tool(&state.db, &state.settings, owner, &a.query, vault_id.as_ref(), a.limit).await
            })
        }),
    );

    // -- entities::tools (read-only + mutating) ------------------------------

    #[derive(serde::Deserialize, Default)]
    struct EntitiesSearchArgs {
        query: String,
        #[serde(default)]
        kind: Option<String>,
        #[serde(default = "default_entities_search_limit")]
        limit: usize,
        #[serde(default)]
        offset: usize,
        #[serde(default)]
        vault_id: Option<String>,
    }
    fn default_entities_search_limit() -> usize {
        50
    }

    register(
        registry,
        "entities_search",
        json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "kind": {"type": "string", "enum": KINDS},
                "limit": {"type": "integer"},
                "offset": {"type": "integer"},
                "vault_id": {"type": "string", "description": "search this vault instead of your personal one"},
            },
            "required": ["query"],
        }),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: EntitiesSearchArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                let vault_id = match parse_opt_rid("vault_id", &a.vault_id) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                let result = entities_tools::entities_search(
                    &state.db,
                    owner,
                    &a.query,
                    a.kind.as_deref(),
                    a.limit,
                    a.offset,
                    vault_id.as_ref(),
                )
                .await;
                Ok(to_tool_value(result))
            })
        }),
    );

    register(
        registry,
        "entities_get",
        json!({"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]}),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: IdArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                let rid = match parse_rid("id", &a.id) {
                    Ok(r) => r,
                    Err(e) => return Ok(e),
                };
                match entities_tools::entities_get(&state.db, owner, &rid).await {
                    Ok(Some(entity)) => Ok(serde_json::to_value(entity).unwrap_or_else(|e| json!({ "error": e.to_string() }))),
                    Ok(None) => Ok(crate::error::AppError::coded(crate::error::ErrorCode::EntityNotFound, "not found").to_tool_value()),
                    Err(e) => Ok(e.to_tool_value()),
                }
            })
        }),
    );

    #[derive(serde::Deserialize, Default)]
    struct EntitiesGraphArgs {
        #[serde(default)]
        kinds: Option<Vec<String>>,
        #[serde(default)]
        vault_id: Option<String>,
    }

    register(
        registry,
        "entities_graph",
        json!({
            "type": "object",
            "properties": {
                "kinds": {
                    "type": "array",
                    "items": {"type": "string", "enum": KINDS},
                    "description": "restrict to these entity kinds (e.g. just the code kinds); omit for the full graph",
                },
                "vault_id": {"type": "string", "description": "use this vault instead of your personal one"},
            },
        }),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: EntitiesGraphArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                let vault_id = match parse_opt_rid("vault_id", &a.vault_id) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                let result = entities_tools::entities_graph(&state.db, owner, a.kinds.as_deref(), vault_id.as_ref()).await;
                Ok(to_tool_value(result))
            })
        }),
    );

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

    #[derive(serde::Deserialize)]
    struct MemoryWriteArgs {
        subject_name: String,
        subject_kind: String,
        text: String,
        #[serde(default)]
        source_record_id: Option<String>,
        #[serde(rename = "type", default = "default_memory_write_type")]
        mem_type: String,
        #[serde(default)]
        vault_id: Option<String>,
    }
    fn default_memory_write_type() -> String {
        "world".to_string()
    }

    register(
        registry,
        "memory_write",
        json!({
            "type": "object",
            "properties": {
                "subject_name": {"type": "string"},
                "subject_kind": {"type": "string", "enum": KINDS},
                "text": {"type": "string"},
                "source_record_id": {"type": "string"},
                "type": {"type": "string", "enum": ["world", "experience", "observation"]},
                "vault_id": {"type": "string", "description": "write into this vault instead of your personal one"},
            },
            "required": ["subject_name", "subject_kind", "text"],
        }),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: MemoryWriteArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                let vault_id = match parse_opt_rid("vault_id", &a.vault_id) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                let result = entities_tools::memory_write(
                    &state.db,
                    owner,
                    &a.subject_name,
                    &a.subject_kind,
                    &a.text,
                    a.source_record_id.as_deref(),
                    &a.mem_type,
                    vault_id.as_ref(),
                )
                .await;
                Ok(to_tool_value(result))
            })
        }),
    );

    #[derive(serde::Deserialize, Default)]
    struct ConsolidateArgs {
        #[serde(default)]
        subject_id: Option<String>,
    }

    register(
        registry,
        "consolidate_observations",
        json!({
            "type": "object",
            "properties": {
                "subject_id": {"type": "string", "description": "consolidate just this entity; omit for all"},
            },
        }),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: ConsolidateArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                let subject_id = match parse_opt_rid("subject_id", &a.subject_id) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                let result =
                    entities_tools::consolidate_observations(&state.db, &state.settings, owner, subject_id.as_ref()).await;
                Ok(to_tool_value(result))
            })
        }),
    );

    #[derive(serde::Deserialize)]
    struct MemoryUpdateArgs {
        memory_id: String,
        #[serde(default)]
        text: Option<String>,
        #[serde(rename = "type", default)]
        mem_type: Option<String>,
    }

    register(
        registry,
        "memory_update",
        json!({
            "type": "object",
            "properties": {
                "memory_id": {"type": "string"},
                "text": {"type": "string"},
                "type": {"type": "string", "enum": ["world", "experience"]},
            },
            "required": ["memory_id"],
        }),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: MemoryUpdateArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                let rid = match parse_rid("memory_id", &a.memory_id) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                let result =
                    entities_tools::memory_update(&state.db, owner, &rid, a.text.as_deref(), a.mem_type.as_deref()).await;
                Ok(to_tool_value(result.and_then(|m| m.ok_or_else(|| crate::error::AppError::coded(crate::error::ErrorCode::MemoryNotFound, "memory not found")))))
            })
        }),
    );

    #[derive(serde::Deserialize)]
    struct EntityUpdateArgs {
        entity_id: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        aliases: Option<Vec<String>>,
        #[serde(default)]
        summary: Option<String>,
    }

    register(
        registry,
        "entity_update",
        json!({
            "type": "object",
            "properties": {
                "entity_id": {"type": "string"},
                "name": {"type": "string"},
                "aliases": {"type": "array", "items": {"type": "string"}},
                "summary": {"type": "string"},
            },
            "required": ["entity_id"],
        }),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: EntityUpdateArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                let rid = match parse_rid("entity_id", &a.entity_id) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                let result = entities_tools::entity_update(
                    &state.db, owner, &rid, a.name.as_deref(), a.aliases, a.summary.as_deref(),
                )
                .await;
                Ok(to_tool_value(result.and_then(|e| e.ok_or_else(|| crate::error::AppError::coded(crate::error::ErrorCode::EntityNotFound, "entity not found")))))
            })
        }),
    );

    #[derive(serde::Deserialize)]
    struct MemoryIdArgs {
        memory_id: String,
    }

    register(
        registry,
        "memory_delete",
        json!({"type": "object", "properties": {"memory_id": {"type": "string"}}, "required": ["memory_id"]}),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: MemoryIdArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                let rid = match parse_rid("memory_id", &a.memory_id) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                let result = entities_tools::memory_delete(&state.db, owner, &rid).await;
                Ok(to_tool_value(result.map(|deleted| json!({ "deleted": deleted }))))
            })
        }),
    );

    #[derive(serde::Deserialize)]
    struct EntityIdArgs {
        entity_id: String,
    }

    register(
        registry,
        "entity_delete",
        json!({"type": "object", "properties": {"entity_id": {"type": "string"}}, "required": ["entity_id"]}),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: EntityIdArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                let rid = match parse_rid("entity_id", &a.entity_id) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                let result = entities_tools::entity_delete(&state.db, owner, &rid).await;
                Ok(to_tool_value(result.map(|deleted| json!({ "deleted": deleted }))))
            })
        }),
    );

    #[derive(serde::Deserialize)]
    struct EntityMergeArgs {
        winner_id: String,
        loser_id: String,
    }

    register(
        registry,
        "entity_merge",
        json!({
            "type": "object",
            "properties": {
                "winner_id": {"type": "string", "description": "the entity to keep"},
                "loser_id": {"type": "string", "description": "the duplicate to merge in and delete"},
            },
            "required": ["winner_id", "loser_id"],
        }),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: EntityMergeArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                let winner_id = match parse_rid("winner_id", &a.winner_id) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                let loser_id = match parse_rid("loser_id", &a.loser_id) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                let result = entities_tools::entity_merge(&state.db, owner, &winner_id, &loser_id).await;
                Ok(to_tool_value(result))
            })
        }),
    );

    // -- vaults::tools (read-only + mutating) --------------------------------

    fn default_vault_kind() -> String {
        "org".to_string()
    }

    #[derive(serde::Deserialize)]
    struct VaultCreateArgs {
        name: String,
        #[serde(default = "default_vault_kind")]
        kind: String,
    }

    register(
        registry,
        "vault_create",
        json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "kind": {"type": "string", "enum": ["org", "personal"], "description": "default org"},
            },
            "required": ["name"],
        }),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: VaultCreateArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                Ok(vaults_tools::vault_create(&state.db, owner, &a.name, &a.kind).await)
            })
        }),
    );

    register(
        registry,
        "vault_list",
        json!({"type": "object", "properties": {}}),
        Arc::new(|state, owner, _args| Box::pin(async move { Ok(vaults_tools::vault_list(&state.db, owner).await) })),
    );

    #[derive(serde::Deserialize)]
    struct VaultCloneArgs {
        vault_id: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default = "default_vault_kind")]
        kind: String,
    }

    register(
        registry,
        "vault_clone",
        json!({
            "type": "object",
            "properties": {
                "vault_id": {"type": "string", "description": "vault to deep-copy (you must be a member)"},
                "name": {"type": "string", "description": "default: \"<source name> (copy)\""},
                "kind": {"type": "string", "enum": ["org", "personal"], "description": "default org"},
            },
            "required": ["vault_id"],
        }),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: VaultCloneArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                let vault_id = match parse_rid("vault_id", &a.vault_id) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                Ok(vaults_tools::vault_clone(&state.db, owner, &vault_id, a.name.as_deref(), &a.kind).await)
            })
        }),
    );

    #[derive(serde::Deserialize)]
    struct VaultMergeArgs {
        vault_id_a: String,
        vault_id_b: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default = "default_vault_kind")]
        kind: String,
    }

    register(
        registry,
        "vault_merge",
        json!({
            "type": "object",
            "properties": {
                "vault_id_a": {"type": "string", "description": "first vault (you must be a member)"},
                "vault_id_b": {"type": "string", "description": "second vault (you must be a member); folded into the first on duplicates"},
                "name": {"type": "string", "description": "default: \"<A> + <B>\""},
                "kind": {"type": "string", "enum": ["org", "personal"], "description": "default org"},
            },
            "required": ["vault_id_a", "vault_id_b"],
        }),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: VaultMergeArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                let va = match parse_rid("vault_id_a", &a.vault_id_a) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                let vb = match parse_rid("vault_id_b", &a.vault_id_b) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                Ok(vaults_tools::vault_merge(&state.db, owner, &va, &vb, a.name.as_deref(), &a.kind).await)
            })
        }),
    );

    fn default_vault_role() -> String {
        "member".to_string()
    }

    #[derive(serde::Deserialize)]
    struct VaultInviteArgs {
        vault_id: String,
        email: String,
        #[serde(default = "default_vault_role")]
        role: String,
    }

    register(
        registry,
        "vault_invite",
        json!({
            "type": "object",
            "properties": {
                "vault_id": {"type": "string"},
                "email": {"type": "string"},
                "role": {"type": "string", "enum": ["owner", "member"], "description": "default member"},
            },
            "required": ["vault_id", "email"],
        }),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: VaultInviteArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                let vault_id = match parse_rid("vault_id", &a.vault_id) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                Ok(vaults_tools::vault_invite(&state.db, owner, &vault_id, &a.email, &a.role).await)
            })
        }),
    );

    #[derive(serde::Deserialize)]
    struct VaultIdArgs {
        vault_id: String,
    }

    register(
        registry,
        "vault_members",
        json!({"type": "object", "properties": {"vault_id": {"type": "string"}}, "required": ["vault_id"]}),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: VaultIdArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                let vault_id = match parse_rid("vault_id", &a.vault_id) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                Ok(vaults_tools::vault_members(&state.db, owner, &vault_id).await)
            })
        }),
    );

    #[derive(serde::Deserialize)]
    struct VaultRemoveMemberArgs {
        vault_id: String,
        email: String,
    }

    register(
        registry,
        "vault_remove_member",
        json!({
            "type": "object",
            "properties": {"vault_id": {"type": "string"}, "email": {"type": "string"}},
            "required": ["vault_id", "email"],
        }),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: VaultRemoveMemberArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                let vault_id = match parse_rid("vault_id", &a.vault_id) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                Ok(vaults_tools::vault_remove_member(&state.db, owner, &vault_id, &a.email).await)
            })
        }),
    );

    register(
        registry,
        "vault_leave",
        json!({"type": "object", "properties": {"vault_id": {"type": "string"}}, "required": ["vault_id"]}),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: VaultIdArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                let vault_id = match parse_rid("vault_id", &a.vault_id) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                Ok(vaults_tools::vault_leave(&state.db, owner, &vault_id).await)
            })
        }),
    );

    #[derive(serde::Deserialize)]
    struct VaultRenameArgs {
        vault_id: String,
        name: String,
    }

    register(
        registry,
        "vault_rename",
        json!({
            "type": "object",
            "properties": {"vault_id": {"type": "string"}, "name": {"type": "string"}},
            "required": ["vault_id", "name"],
        }),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: VaultRenameArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                let vault_id = match parse_rid("vault_id", &a.vault_id) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                Ok(vaults_tools::vault_rename(&state.db, owner, &vault_id, &a.name).await)
            })
        }),
    );

    register(
        registry,
        "vault_delete",
        json!({"type": "object", "properties": {"vault_id": {"type": "string"}}, "required": ["vault_id"]}),
        Arc::new(|state, owner, args| {
            Box::pin(async move {
                let a: VaultIdArgs = match serde_json::from_value(args) {
                    Ok(a) => a,
                    Err(e) => return Ok(bad_args(e)),
                };
                let vault_id = match parse_rid("vault_id", &a.vault_id) {
                    Ok(v) => v,
                    Err(e) => return Ok(e),
                };
                Ok(vaults_tools::vault_delete(&state.db, owner, &vault_id).await)
            })
        }),
    );
}

pub fn all_tools() -> &'static HashMap<&'static str, ToolSpec> {
    REGISTRY.get_or_init(build_registry)
}

/// The owner's own audit log, newest first -- backs `GET /api/audit` (an
/// admin/REST concern, not an agent-facing tool; an agent auditing its own
/// writes isn't a real use case this codebase needs yet).
pub async fn list_audit(db: &Db, owner: &RecordId, limit: i64, offset: i64) -> AppResult<Value> {
    #[derive(serde::Deserialize, SurrealValue)]
    struct AuditRow {
        id: RecordId,
        tool_name: String,
        args_summary: String,
        outcome: String,
        created_at: Option<surrealdb::types::Datetime>,
    }
    #[derive(serde::Deserialize, SurrealValue)]
    struct CountRow {
        count: i64,
    }

    let mut res = store::cache::AUDIT_PAGE
        .on(db)
        .bind(("owner", owner.clone()))
        .bind(("limit", limit))
        .bind(("offset", offset))
        .await?;
    let rows: Vec<AuditRow> = res.take(0)?;

    let mut tres = store::cache::AUDIT_COUNT.on(db).bind(("owner", owner.clone())).await?;
    let total_rows: Vec<CountRow> = tres.take(0)?;
    let total = total_rows.first().map(|r| r.count).unwrap_or(0);

    let results: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.id.to_string(),
                "tool_name": r.tool_name,
                "args_summary": r.args_summary,
                "outcome": r.outcome,
                "created_at": &r.created_at,
            })
        })
        .collect();
    let has_more = offset + (results.len() as i64) < total;
    Ok(json!({ "results": results, "total": total, "has_more": has_more }))
}

/// Calls a registered tool by name, applying the audit-log side effect for
/// anything not in [`READ_ONLY_TOOLS`]. Returns `{"error": "unknown tool
/// ..."}`` for a name that isn't registered, matching the Python version
/// rather than a 404 -- the REST router is the one that turns an unknown
/// name into a 404 before ever calling this.
pub async fn call(state: &AppState, owner: &RecordId, name: &str, args: Value) -> AppResult<Value> {
    let tools = all_tools();
    let Some(spec) = tools.get(name) else {
        return Ok(AppError::coded(ErrorCode::ToolNotFound, format!("unknown tool {name}")).to_tool_value());
    };

    let needed = authz::require_scope(scopes::for_tool(name, is_read_only(name))).and_then(|()| {
        if reads_source_records(name) { authz::require_unrestricted() } else { Ok(()) }
    });
    if let Err(e) = needed {
        record_event(&state.db, owner, name, e.code.as_str(), "").await;
        return Err(e);
    }

    let vault = args.get("vault_id").and_then(Value::as_str).unwrap_or("personal").to_string();
    let span = tracing::info_span!("tool.call", tool = name, vault = %vault, outcome = Empty, duration_ms = Empty);
    let started = Instant::now();
    let mut result = run_tool(state, owner, name, spec, args).instrument(span.clone()).await;

    // Every failed tool value carries a stable `code` and the request's `trace_id`.
    if let Ok(Value::Object(map)) = &mut result
        && map.contains_key("error")
    {
        map.entry("code").or_insert_with(|| ErrorCode::ValidationInvalid.as_str().into());
        map.entry("trace_id").or_insert_with(|| crate::telemetry::current_trace_id().into());
    }
    let outcome = match &result {
        Ok(v) if v.get("error").is_some() => v.get("code").and_then(Value::as_str).unwrap_or("error"),
        Ok(_) => "ok",
        Err(e) => e.code.as_str(),
    };
    span.record("outcome", outcome);
    span.record("duration_ms", started.elapsed().as_secs_f64() * 1000.0);
    span.in_scope(|| tracing::info!(outcome, "tool call"));
    result
}

async fn run_tool(state: &AppState, owner: &RecordId, name: &str, spec: &ToolSpec, args: Value) -> AppResult<Value> {
    if is_read_only(name) {
        return (spec.handler)(state, owner, args).await;
    }

    let result = (spec.handler)(state, owner, args.clone()).await;
    let (outcome, code) = match &result {
        Ok(v) if v.get("error").is_some() => ("error", v.get("code").and_then(Value::as_str).unwrap_or("error")),
        Ok(_) => ("ok", "ok"),
        Err(e) => ("error", e.code.as_str()),
    };
    record_audit(&state.db, owner, name, &args, outcome, code).await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_the_intended_scope() {
        for name in all_tools().keys() {
            let want = if is_read_only(name) {
                scopes::MEMORY_READ
            } else if name.starts_with("vault_") {
                scopes::VAULTS_ADMIN
            } else {
                scopes::MEMORY_WRITE
            };
            assert_eq!(scopes::for_tool(name, is_read_only(name)), want, "{name}");
        }
    }

    #[test]
    fn read_only_tools_match_python_set_exactly() {
        let expected = [
            "docs",
            "search",
            "get",
            "list",
            "links",
            "recall",
            "reflect",
            "entities_search",
            "entities_get",
            "entities_graph",
            "vault_list",
            "vault_members",
        ];
        assert_eq!(READ_ONLY_TOOLS.len(), expected.len());
        for name in expected {
            assert!(is_read_only(name), "{name} should be read-only");
        }
    }

    #[test]
    fn mutating_tool_names_are_not_read_only() {
        for name in ["vault_create", "vault_delete", "vault_invite", "vault_rename", "unknown_tool"] {
            assert!(!is_read_only(name), "{name} should not be read-only");
        }
    }

    #[test]
    fn every_registered_tool_has_a_description() {
        for name in all_tools().keys() {
            assert!(!description(name).is_empty(), "{name} needs a description in registry::description");
        }
    }

    #[test]
    fn destructive_tools_are_never_read_only() {
        for name in all_tools().keys().filter(|n| is_destructive(n)) {
            assert!(!is_read_only(name), "{name}");
        }
    }

    #[test]
    fn summarize_args_redacts_secret_looking_keys() {
        let args = json!({ "password": "hunter2", "api_key": "abc", "query": "hello" });
        let summary = summarize_args(&args);
        assert!(!summary.contains("hunter2"));
        assert!(!summary.contains("abc"));
        assert!(summary.contains("hello"));
        assert!(summary.contains("***"));
    }

    #[test]
    fn summarize_args_truncates_long_summaries() {
        let long_value = "x".repeat(1000);
        let args = json!({ "q": long_value });
        let summary = summarize_args(&args);
        assert!(summary.chars().count() <= ARGS_SUMMARY_MAX + 1);
        assert!(summary.ends_with('\u{2026}'));
    }

    #[test]
    fn summarize_args_handles_non_object_args() {
        let summary = summarize_args(&Value::Null);
        assert_eq!(summary, "{}");
    }

    #[test]
    fn registry_has_every_python_tool_wired_up() {
        let expected = [
            "docs", "search", "get", "list", "links", "recall", "reflect", "entities_search", "entities_get",
            "entities_graph", "code_entity_upsert", "code_relate", "memory_write", "consolidate_observations",
            "memory_update", "entity_update", "memory_delete", "entity_delete", "entity_merge", "vault_create", "vault_list", "vault_clone", "vault_merge",
            "vault_invite", "vault_members", "vault_remove_member", "vault_leave", "vault_rename", "vault_delete",
        ];
        let tools = all_tools();
        for name in expected {
            assert!(tools.contains_key(name), "{name} should be registered");
        }
        assert_eq!(tools.len(), expected.len());
    }
}
