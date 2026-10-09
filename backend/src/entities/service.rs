//! Entity-memory graph: person/organisation/location/repository/file/symbol +
//! memory + relates_to. Vault-scoped CRUD/query layer, same pattern as
//! `vaults::service` -- except the scope is a *vault* (`vaults::service`),
//! not a single owner, so entities and memories can be shared by every
//! member of a vault (a personal vault, an org vault, or a vault shared
//! person-to-person -- see `db.rs`'s schema comment).
//!
//! Every write still records `owner` (who wrote it, for audit/display) but
//! access control is entirely vault membership. Callers that don't pass a
//! `vault_id` get the caller's own personal vault.
//!
//! Dedup for people/orgs/locations is name-or-alias, case-insensitive, per
//! vault. `repository`/`file`/`symbol` are a second, separate entity map
//! (code entities an external agent maps while working in a codebase) that
//! lives in the same graph as person/organisation/location and can be
//! cross-linked to them via `relates_to`.
//!
//! Ported from `entities/service.py`. Unlike the Python version's `_as_rid`
//! (accepts either a `RecordID` or a string), every function here already
//! deals in `RecordId` -- callers (routers, tools) parse path/user-supplied
//! ids first, same convention as `vaults::service`.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use surrealdb::{Datetime, RecordId};

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::vaults::service as vaults_service;

/// Entity kinds this module manages -- mirrors `entities/service.py`'s
/// `KINDS` tuple and `vaults::service::ENTITY_KINDS`.
pub const KINDS: &[&str] = &["person", "organisation", "location", "repository", "file", "symbol"];

/// Code-graph subset of `KINDS`, mirrors `entities/service.py`'s `CodeKind`.
pub const CODE_KINDS: &[&str] = &["repository", "file", "symbol"];

/// Validates `kind` against `KINDS`, returning the matching static str --
/// used everywhere a kind is interpolated into a raw SurrealDB query (table
/// names can't be bound parameters), so an unrecognized kind can never reach
/// string-formatted SQL. The Python version doesn't do this (it trusts the
/// `Literal["person", ...]` type hint, which isn't enforced at runtime) --
/// this check is a safety addition for the Rust port, not a behavior change
/// for any caller that already validates kind (every router/tool call site
/// does).
pub fn kind_table(kind: &str) -> AppResult<&'static str> {
    KINDS.iter().find(|k| **k == kind).copied().ok_or_else(|| AppError::bad_request(format!("unknown kind {kind:?}")))
}

fn owner_key_string(owner: &RecordId) -> String {
    String::try_from(owner.key().clone()).unwrap_or_else(|_| owner.to_string())
}

/// The internal `cache_record` RecordId for a record's caller-facing id --
/// mirrors `entities/service.py`'s `_cache_record_rid` (owner-id prefix is
/// internal).
fn cache_record_rid(owner: &RecordId, record_id: &str) -> RecordId {
    RecordId::from_table_key("cache_record", format!("{}:{}", owner_key_string(owner), record_id))
}

// ---------------------------------------------------------------------------
// Row / output shapes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct EntityRow {
    id: RecordId,
    #[serde(default)]
    owner: Option<RecordId>,
    vault: RecordId,
    #[serde(default)]
    name: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntityOut {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub summary: String,
    pub vault: String,
}

fn entity_out(kind: &str, row: &EntityRow) -> EntityOut {
    EntityOut {
        id: row.id.to_string(),
        kind: kind.to_string(),
        name: row.name.clone(),
        aliases: row.aliases.clone(),
        summary: row.summary.clone(),
        vault: row.vault.to_string(),
    }
}

#[derive(Debug, Clone, Deserialize)]
struct MemoryRow {
    id: RecordId,
    #[serde(default)]
    owner: Option<RecordId>,
    vault: RecordId,
    subject: RecordId,
    #[serde(default)]
    text: String,
    #[serde(default)]
    source: Option<RecordId>,
    created_at: Datetime,
    #[serde(rename = "type", default = "default_memory_type")]
    mem_type: String,
    #[serde(default)]
    proof_count: i64,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    source_memories: Option<Vec<RecordId>>,
    #[serde(default)]
    updated_at: Option<Datetime>,
    #[serde(default)]
    version: i64,
}

fn default_memory_type() -> String {
    "world".to_string()
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryOut {
    pub id: String,
    pub owner: Option<String>,
    pub vault: String,
    pub subject: String,
    pub text: String,
    pub source: Option<String>,
    pub created_at: Datetime,
    #[serde(rename = "type")]
    pub mem_type: String,
    pub proof_count: i64,
    pub status: Option<String>,
    pub source_memories: Option<Vec<String>>,
    pub updated_at: Option<Datetime>,
    pub version: i64,
    pub owner_email: Option<String>,
}

fn memory_out(row: &MemoryRow, owner_email: Option<String>) -> MemoryOut {
    MemoryOut {
        id: row.id.to_string(),
        owner: row.owner.as_ref().map(|o| o.to_string()),
        vault: row.vault.to_string(),
        subject: row.subject.to_string(),
        text: row.text.clone(),
        source: row.source.as_ref().map(|s| s.to_string()),
        created_at: row.created_at.clone(),
        mem_type: row.mem_type.clone(),
        proof_count: row.proof_count,
        status: row.status.clone(),
        source_memories: row.source_memories.as_ref().map(|v| v.iter().map(|r| r.to_string()).collect()),
        updated_at: row.updated_at.clone(),
        version: row.version,
        owner_email,
    }
}

#[derive(Debug, Clone, Deserialize)]
struct RelationRow {
    id: RecordId,
    #[serde(rename = "in")]
    in_: RecordId,
    #[serde(rename = "out")]
    out_: RecordId,
    #[serde(default)]
    label: String,
    #[serde(default)]
    source: Option<RecordId>,
    #[serde(default)]
    owner: Option<RecordId>,
    #[serde(default)]
    created_at: Option<Datetime>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RelationOut {
    pub id: String,
    #[serde(rename = "in")]
    pub in_: String,
    #[serde(rename = "out")]
    pub out_: String,
    pub label: String,
    pub source: Option<String>,
    pub owner: Option<String>,
    pub created_at: Option<Datetime>,
    pub direction: Option<String>,
    pub owner_email: Option<String>,
}

fn relation_out(row: &RelationRow, direction: Option<&str>, owner_email: Option<String>) -> RelationOut {
    RelationOut {
        id: row.id.to_string(),
        in_: row.in_.to_string(),
        out_: row.out_.to_string(),
        label: row.label.clone(),
        source: row.source.as_ref().map(|s| s.to_string()),
        owner: row.owner.as_ref().map(|o| o.to_string()),
        created_at: row.created_at.clone(),
        direction: direction.map(str::to_string),
        owner_email,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EntityDetail {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub summary: String,
    pub owner_email: Option<String>,
    pub memory: Vec<MemoryOut>,
    pub relations: Vec<RelationOut>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListEntitiesOut {
    pub results: Vec<EntityOut>,
    pub total: usize,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphNode {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub owner_email: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub label: String,
    pub owner_email: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphOut {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WriteMemoryOut {
    pub entity: EntityOut,
    pub memory: MemoryOut,
    /// Earlier facts about the subject this one made no longer true (now
    /// left out of recall). Filled in by the `memory_write` tool.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub superseded: Vec<String>,
    /// When the write created a new entity: existing ones of the same kind
    /// whose name shares a word with it -- candidates for `entity_merge`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub possible_duplicates: Vec<EntityOut>,
}

// ---------------------------------------------------------------------------
// Scope / access helpers
// ---------------------------------------------------------------------------

/// `vault_id`, membership-checked, or `owner`'s personal vault when omitted --
/// the one place every public function in this module resolves its scope.
async fn resolve_vault(db: &Db, owner: &RecordId, vault_id: Option<&RecordId>) -> AppResult<RecordId> {
    match vault_id {
        None => vaults_service::default_vault_id(db, owner).await,
        Some(v) => {
            vaults_service::require_membership(db, owner, v).await?;
            Ok(v.clone())
        }
    }
}

/// Whether `owner` may read/write a row in `vault` -- member of its vault.
/// Not-a-member is treated the same as not-found everywhere in this module.
async fn accessible(db: &Db, owner: &RecordId, vault: &RecordId) -> AppResult<bool> {
    let ids = vaults_service::accessible_vault_ids(db, owner).await?;
    Ok(ids.contains(vault))
}

/// Batch-resolve `user` RecordIds to emails, for attributing who wrote what
/// in a shared vault -- one query per call site rather than N+1 lookups.
async fn emails_for(db: &Db, user_ids: Vec<Option<RecordId>>) -> AppResult<HashMap<String, String>> {
    let ids: Vec<RecordId> = {
        let mut seen = HashSet::new();
        user_ids
            .into_iter()
            .flatten()
            .filter(|id| seen.insert(id.clone()))
            .collect()
    };
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    #[derive(Deserialize)]
    struct Row {
        id: RecordId,
        email: String,
    }
    let mut res = db.query("SELECT id, email FROM user WHERE id IN $ids").bind(("ids", ids)).await?;
    let rows: Vec<Row> = res.take(0)?;
    Ok(rows.into_iter().map(|r| (r.id.to_string(), r.email)).collect())
}

/// Whether `rid` points into an entity table. Any other row with a `vault`
/// field (a `memory`, a `vault_member`) must never be read, edited or deleted
/// as an "entity".
pub fn is_entity_id(rid: &RecordId) -> bool {
    KINDS.contains(&rid.table())
}

async fn select_entity(db: &Db, rid: &RecordId) -> AppResult<Option<EntityRow>> {
    if !is_entity_id(rid) {
        return Ok(None);
    }
    let row: Option<EntityRow> = db.select(rid.clone()).await?;
    Ok(row)
}

/// Same guard for memory ids.
async fn select_memory(db: &Db, rid: &RecordId) -> AppResult<Option<MemoryRow>> {
    if rid.table() != "memory" {
        return Ok(None);
    }
    let row: Option<MemoryRow> = db.select(rid.clone()).await?;
    Ok(row)
}

/// Relations and merges stay inside one vault: an edge from a shared vault's
/// entity into someone's personal one would show its label and endpoint to
/// every member, and deleting/merging in one vault would rewrite the other.
fn same_vault(a: &EntityRow, b: &EntityRow) -> AppResult<()> {
    if a.vault != b.vault {
        return Err(AppError::bad_request("both entities must be in the same vault"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Core CRUD
// ---------------------------------------------------------------------------

/// Serialises this process's find-then-create of entities and observations.
/// The UNIQUE indexes are the cross-process guarantee, but SurrealDB 2.3 has
/// been seen to let two simultaneous CREATEs through one of them (about one
/// run in five of the concurrency e2e test), so writes from this server also
/// queue here. Held only around the lookup and the write -- a few ms.
static ENTITY_WRITE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static OBSERVATION_WRITE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Whether a failed write lost a race to a concurrent one: a UNIQUE index
/// rejected it, or SurrealDB aborted the conflicting transaction.
fn is_write_conflict(e: &surrealdb::Error) -> bool {
    let msg = e.to_string();
    msg.contains("already contains") || msg.contains("can be retried")
}

/// The `table` entity in `vault` whose name or an alias is `needle`
/// (lowercased), a name match first -- both lookups index-backed.
async fn find_entity(db: &Db, table: &str, vault: &RecordId, needle: &str) -> AppResult<Option<EntityRow>> {
    let mut res = db
        .query(format!(
            "SELECT * FROM {table} WHERE vault = $vault AND name_key = $needle LIMIT 1; \
             SELECT * FROM {table} WITH INDEX {table}_alias_keys_idx WHERE alias_keys CONTAINS $needle AND vault = $vault \
             ORDER BY created_at LIMIT 1;"
        ))
        .bind(("vault", vault.clone()))
        .bind(("needle", needle.to_string()))
        .await?;
    let by_name: Vec<EntityRow> = res.take(0)?;
    let by_alias: Vec<EntityRow> = res.take(1)?;
    Ok(by_name.into_iter().chain(by_alias).next())
}

/// Find-or-create a `kind` entity in the resolved vault, matched
/// case-insensitively against existing `name`/`aliases`. New aliases are
/// merged onto a match rather than creating a duplicate row. Concurrent
/// calls for one name return one entity: the `(vault, name_key)` UNIQUE
/// index rejects the second CREATE, which then returns the winner's row.
pub async fn upsert_entity(
    db: &Db,
    owner: &RecordId,
    kind: &str,
    name: &str,
    aliases: Option<Vec<String>>,
    vault_id: Option<&RecordId>,
) -> AppResult<EntityOut> {
    Ok(upsert_entity_created(db, owner, kind, name, aliases, vault_id).await?.0)
}

/// `"Jane Doe (Acme/JD)"` -> `("Jane Doe", ["Acme", "JD"])`: a name with a
/// parenthesised suffix of aliases/affiliations.
fn split_decorated(name: &str) -> Option<(&str, Vec<String>)> {
    let (base, extra) = name.strip_suffix(')')?.rsplit_once('(')?;
    let base = base.trim();
    let extra: Vec<String> = extra.split(['/', ',', ';']).map(str::trim).filter(|a| !a.is_empty()).map(String::from).collect();
    (!base.is_empty() && !extra.is_empty()).then_some((base, extra))
}

/// [`upsert_entity`], plus whether the entity was created just now. A
/// decorated name (see [`split_decorated`]) whose base name already exists
/// resolves to that entity, the suffix parts becoming aliases.
pub async fn upsert_entity_created(
    db: &Db,
    owner: &RecordId,
    kind: &str,
    name: &str,
    aliases: Option<Vec<String>>,
    vault_id: Option<&RecordId>,
) -> AppResult<(EntityOut, bool)> {
    let table = kind_table(kind)?;
    let vault = resolve_vault(db, owner, vault_id).await?;
    let mut aliases = aliases.unwrap_or_default();
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::bad_request("entity name can't be empty"));
    }
    let needle = name.to_lowercase();

    let guard = ENTITY_WRITE.lock().await;
    let mut found = find_entity(db, table, &vault, &needle).await?;
    if found.is_none() {
        if let Some((base, extra)) = split_decorated(name) {
            found = find_entity(db, table, &vault, &base.to_lowercase()).await?;
            if found.is_some() {
                aliases.extend(extra);
            }
        }
    }
    if found.is_none() {
        let mut created = db
            .query(format!(
                "CREATE {table} SET owner = $owner, vault = $vault, name = $name, aliases = $aliases RETURN AFTER"
            ))
            .bind(("owner", owner.clone()))
            .bind(("vault", vault.clone()))
            .bind(("name", name.to_string()))
            .bind(("aliases", aliases.clone()))
            .await?;
        match created.take::<Vec<EntityRow>>(0) {
            Ok(rows) => {
                let row = rows.into_iter().next().ok_or_else(|| AppError::internal("entity insert returned no row"))?;
                return Ok((entity_out(table, &row), true));
            }
            Err(e) if is_write_conflict(&e) => found = find_entity(db, table, &vault, &needle).await?,
            Err(e) => return Err(e.into()),
        }
    }
    drop(guard);
    let row = found.ok_or_else(|| AppError::internal("entity vanished during a concurrent write"))?;

    let existing: HashSet<String> = row.aliases.iter().cloned().collect();
    if aliases.iter().all(|a| existing.contains(a)) {
        return Ok((entity_out(table, &row), false));
    }
    let mut updated = db
        .query("UPDATE $id SET aliases = array::sort(array::union(aliases, $aliases)), updated_at = time::now() RETURN AFTER")
        .bind(("id", row.id.clone()))
        .bind(("aliases", aliases))
        .await?;
    let updated_rows: Vec<EntityRow> = updated.take(0)?;
    let updated_row = updated_rows.into_iter().next().ok_or_else(|| AppError::internal("entity update returned no row"))?;
    Ok((entity_out(table, &updated_row), false))
}

/// Other `kind` entities in the entity's vault whose name or an alias shares
/// a word (3+ letters) with its name -- likely the same thing under another
/// name ("Dave" / "David Smith" share nothing; "David" / "David Smith" do).
pub async fn possible_duplicates(db: &Db, entity: &EntityOut) -> AppResult<Vec<EntityOut>> {
    let table = kind_table(&entity.kind)?;
    let words: Vec<String> = entity
        .name
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3)
        .map(String::from)
        .collect();
    if words.is_empty() {
        return Ok(Vec::new());
    }
    let id: RecordId = entity.id.parse().map_err(|_| AppError::internal("entity id did not round-trip"))?;
    let vault: RecordId = entity.vault.parse().map_err(|_| AppError::internal("vault id did not round-trip"))?;
    let mut res = db
        .query(format!(
            "SELECT * FROM {table} WHERE vault = $vault AND id != $id \
             AND (string::words(name_key) CONTAINSANY $words OR alias_keys CONTAINSANY $words) LIMIT 5"
        ))
        .bind(("vault", vault))
        .bind(("id", id))
        .bind(("words", words))
        .await?;
    let rows: Vec<EntityRow> = res.take(0)?;
    Ok(rows.iter().map(|r| entity_out(table, r)).collect())
}

/// Ids of the entities in `vault` with a fact whose text matches `query`
/// (full-text, so a handle or username only ever written in a fact is found).
pub async fn subjects_mentioning(db: &Db, vault: &RecordId, query: &str) -> AppResult<HashSet<String>> {
    let mut res = db
        .query("SELECT VALUE subject FROM memory WHERE vault = $vault AND text @@ $q LIMIT 200")
        .bind(("vault", vault.clone()))
        .bind(("q", query.to_string()))
        .await?;
    let ids: Vec<RecordId> = res.take(0)?;
    Ok(ids.iter().map(|r| r.to_string()).collect())
}

/// `source_record_id` is optional -- an automatic extraction path always ties
/// a memory back to the cache_record it was pulled from, but a programmatic
/// write (`write_memory`) may be a fact an agent knows directly, with no
/// backing cache_record.
///
/// `mem_type` defaults to "world"; callers may pass "experience" or
/// "observation" (the latter is written directly by `consolidate.rs` rather
/// than through this function in practice).
///
/// The memory's vault is always `subject_id`'s vault (not a separate
/// choice) -- returns a 403 if `owner` isn't a member of it.
///
/// Staleness: writing a new raw fact ("world"/"experience") for a subject
/// invalidates that subject's existing observation, if any -- marked here
/// rather than only in `write_memory` so the auto-extraction path also
/// triggers it.
pub async fn add_memory(
    db: &Db,
    owner: &RecordId,
    subject_id: &RecordId,
    text: &str,
    source_record_id: Option<&str>,
    mem_type: &str,
) -> AppResult<MemoryOut> {
    let subject_row = select_entity(db, subject_id)
        .await?
        .ok_or_else(|| AppError::bad_request(format!("subject entity not found: {subject_id}")))?;
    if !accessible(db, owner, &subject_row.vault).await? {
        return Err(AppError::new(
            axum::http::StatusCode::FORBIDDEN,
            format!("not a member of {}'s vault", subject_row.vault),
        ));
    }

    // One observation per subject: writing another one -- e.g. an MCP agent
    // doing the consolidating -- revises it in place.
    if mem_type == "observation" {
        return save_observation(db, owner, &subject_row.vault, subject_id, text, None).await;
    }

    let source = source_record_id.map(|r| cache_record_rid(owner, r));
    let q = db
        .query(
            "CREATE memory SET owner = $owner, vault = $vault, subject = $subject, text = $text, \
             type = $type, source = $source RETURN AFTER",
        )
        .bind(("owner", owner.clone()))
        .bind(("vault", subject_row.vault.clone()))
        .bind(("subject", subject_id.clone()))
        .bind(("text", text.to_string()))
        .bind(("type", mem_type.to_string()))
        .bind(("source", source));
    let mut res = q.await?;
    let rows: Vec<MemoryRow> = res.take(0)?;
    let memory = rows.into_iter().next().ok_or_else(|| AppError::internal("memory insert returned no row"))?;

    if mem_type != "observation" {
        db.query(r#"UPDATE memory SET status = "stale" WHERE subject = $subject AND type = "observation""#)
            .bind(("subject", subject_id.clone()))
            .await?;
    }

    Ok(memory_out(&memory, None))
}

/// Creates or revises `subject`'s one observation, marking it fresh.
/// `lineage` (the raw facts it was built from) replaces `source_memories` and
/// `proof_count` when given; an agent-written belief (`None`) leaves them.
/// The `memory_observation_unique` index makes a concurrent second CREATE
/// fail; that writer then revises the row that won instead.
pub(crate) async fn save_observation(
    db: &Db,
    owner: &RecordId,
    vault: &RecordId,
    subject: &RecordId,
    text: &str,
    lineage: Option<Vec<RecordId>>,
) -> AppResult<MemoryOut> {
    let set_lineage = if lineage.is_some() { ", source_memories = $lineage, proof_count = $proof" } else { "" };
    let update = format!(
        "UPDATE memory SET text = $text, version += 1, status = \"fresh\", updated_at = time::now(){set_lineage} \
         WHERE subject = $subject AND type = \"observation\" RETURN AFTER"
    );
    let create = format!(
        "CREATE memory SET owner = $owner, vault = $vault, subject = $subject, text = $text, \
         type = \"observation\", status = \"fresh\"{set_lineage} RETURN AFTER"
    );
    let proof = lineage.as_ref().map(|l| l.len() as i64);
    let _guard = OBSERVATION_WRITE.lock().await;
    for attempt in 0..3 {
        for sql in [&update, &create] {
            let mut res = db
                .query(sql.as_str())
                .bind(("owner", owner.clone()))
                .bind(("vault", vault.clone()))
                .bind(("subject", subject.clone()))
                .bind(("text", text.to_string()))
                .bind(("lineage", lineage.clone()))
                .bind(("proof", proof))
                .await?;
            match res.take::<Vec<MemoryRow>>(0) {
                Ok(rows) => {
                    if let Some(row) = rows.into_iter().next() {
                        return Ok(memory_out(&row, None));
                    }
                }
                Err(e) if attempt < 2 && is_write_conflict(&e) => break,
                Err(e) => return Err(e.into()),
            }
        }
    }
    Err(AppError::internal("observation write kept conflicting"))
}

/// Programmatic memory write -- a direct path for an agent to record a fact
/// about a person/organisation/location via a tool call. Finds-or-creates
/// the subject entity by reusing `upsert_entity`'s dedupe logic, then
/// records the memory against it.
pub async fn write_memory(
    db: &Db,
    owner: &RecordId,
    subject_name: &str,
    subject_kind: &str,
    text: &str,
    source_record_id: Option<&str>,
    mem_type: &str,
    vault_id: Option<&RecordId>,
) -> AppResult<WriteMemoryOut> {
    let (entity, created) = upsert_entity_created(db, owner, subject_kind, subject_name, None, vault_id).await?;
    let entity_rid: RecordId =
        entity.id.parse().map_err(|_| AppError::internal("entity id did not round-trip"))?;
    let memory = add_memory(db, owner, &entity_rid, text, source_record_id, mem_type).await?;
    let possible_duplicates = if created { possible_duplicates(db, &entity).await? } else { Vec::new() };
    Ok(WriteMemoryOut { entity, memory, superseded: Vec::new(), possible_duplicates })
}

/// RELATE two entities, idempotent on the (in, out, label) unique index -- a
/// duplicate relation is a no-op that returns the existing edge. Both
/// endpoints must be in the same vault, one `owner` belongs to.
pub async fn add_relation(
    db: &Db,
    owner: &RecordId,
    from_id: &RecordId,
    to_id: &RecordId,
    label: &str,
    source_record_id: Option<&str>,
) -> AppResult<RelationOut> {
    let (Some(in_row), Some(out_row)) = (select_entity(db, from_id).await?, select_entity(db, to_id).await?) else {
        return Err(AppError::new(axum::http::StatusCode::FORBIDDEN, "not a member of both entities' vaults"));
    };
    if !accessible(db, owner, &in_row.vault).await? || !accessible(db, owner, &out_row.vault).await? {
        return Err(AppError::new(axum::http::StatusCode::FORBIDDEN, "not a member of both entities' vaults"));
    }
    same_vault(&in_row, &out_row)?;

    if let Some(existing) = find_relation(db, from_id, to_id, label).await? {
        return Ok(relation_out(&existing, None, None));
    }

    let source = source_record_id.map(|r| cache_record_rid(owner, r));
    let res = db
        .query("RELATE $in->relates_to->$out SET label = $label, owner = $owner, source = $source RETURN AFTER")
        .bind(("in", from_id.clone()))
        .bind(("out", to_id.clone()))
        .bind(("label", label.to_string()))
        .bind(("owner", owner.clone()))
        .bind(("source", source))
        .await;

    match res {
        Ok(mut res) => {
            let rows: Vec<RelationRow> = res.take(0)?;
            match rows.into_iter().next() {
                Some(row) => Ok(relation_out(&row, None, None)),
                None => find_relation(db, from_id, to_id, label)
                    .await?
                    .map(|r| relation_out(&r, None, None))
                    .ok_or_else(|| AppError::internal("RELATE returned no row")),
            }
        }
        // Could be a lost race against a concurrent RELATE for the same edge
        // (recoverable -- fetch what won) or a genuine failure. Only swallow
        // the former; re-raise if the fallback SELECT also finds nothing.
        Err(e) => match find_relation(db, from_id, to_id, label).await? {
            Some(row) => Ok(relation_out(&row, None, None)),
            None => Err(AppError::from(e)),
        },
    }
}

async fn find_relation(db: &Db, from_id: &RecordId, to_id: &RecordId, label: &str) -> AppResult<Option<RelationRow>> {
    let mut res = db
        .query("SELECT * FROM relates_to WHERE in = $in AND out = $out AND label = $label LIMIT 1")
        .bind(("in", from_id.clone()))
        .bind(("out", to_id.clone()))
        .bind(("label", label.to_string()))
        .await?;
    let rows: Vec<RelationRow> = res.take(0)?;
    Ok(rows.into_iter().next())
}

/// Delete one `memory` row, vault-scoped. Returns `false` (no-op) if it
/// doesn't exist or `owner` isn't a member of its vault, rather than
/// erroring -- mirrors `get_entity`'s not-found-is-None convention.
pub async fn delete_memory(db: &Db, owner: &RecordId, memory_id: &RecordId) -> AppResult<bool> {
    let row = select_memory(db, memory_id).await?;
    let Some(row) = row else { return Ok(false) };
    if !accessible(db, owner, &row.vault).await? {
        return Ok(false);
    }
    // A deleted raw fact leaves the subject's observation's lineage and makes
    // it stale (rebuilt from the surviving facts on the next consolidation);
    // an observation built from nothing but this fact is deleted with it.
    db.query(
        "BEGIN TRANSACTION; \
         DELETE $id; \
         IF $raw { \
             DELETE memory WHERE subject = $subject AND type = \"observation\" AND source_memories = [$id]; \
             UPDATE memory SET status = \"stale\", updated_at = time::now(), \
                 source_memories = IF source_memories THEN array::complement(source_memories, [$id]) ELSE NONE END \
                 WHERE subject = $subject AND type = \"observation\"; \
         }; \
         COMMIT TRANSACTION;",
    )
    .bind(("id", memory_id.clone()))
    .bind(("subject", row.subject.clone()))
    .bind(("raw", row.mem_type != "observation"))
    .await?
    .check()?;
    Ok(true)
}

/// Memory types a raw fact can be switched between by [`update_memory`];
/// `observation` is the consolidated belief and keeps its type.
const RAW_MEMORY_TYPES: &[&str] = &["world", "experience"];

/// Validates an edit request: something to change, and a legal type change
/// for a memory that is currently `current_type`.
fn check_memory_edit(current_type: &str, text: Option<&str>, new_type: Option<&str>) -> AppResult<()> {
    if text.is_none() && new_type.is_none() {
        return Err(AppError::bad_request("nothing to update: pass `text` and/or `type`"));
    }
    if text.is_some_and(|t| t.trim().is_empty()) {
        return Err(AppError::bad_request("memory text can't be empty"));
    }
    if let Some(t) = new_type {
        if current_type == "observation" || !RAW_MEMORY_TYPES.contains(&t) {
            return Err(AppError::bad_request(
                "`type` can only switch a fact between world and experience; observations keep theirs",
            ));
        }
    }
    Ok(())
}

/// Edit one memory's text and/or type (world <-> experience), vault-scoped,
/// not-found-is-None like `get_entity`. Bumps `version`. Editing a raw fact
/// marks its subject's observation stale, same as writing a new one.
pub async fn update_memory(
    db: &Db,
    owner: &RecordId,
    memory_id: &RecordId,
    text: Option<&str>,
    new_type: Option<&str>,
) -> AppResult<Option<MemoryOut>> {
    let row = select_memory(db, memory_id).await?;
    let Some(row) = row else { return Ok(None) };
    if !accessible(db, owner, &row.vault).await? {
        return Ok(None);
    }
    check_memory_edit(&row.mem_type, text, new_type)?;

    let mut set = vec!["version = version + 1", "updated_at = time::now()"];
    if text.is_some() {
        set.push("text = $text");
        if row.mem_type != "observation" {
            set.push("status = NONE"); // an edited fact is current again
        }
    }
    if new_type.is_some() {
        set.push("type = $type");
    }
    let mut q = db.query(format!("UPDATE $id SET {} RETURN AFTER", set.join(", "))).bind(("id", memory_id.clone()));
    if let Some(t) = text {
        q = q.bind(("text", t.to_string()));
    }
    if let Some(t) = new_type {
        q = q.bind(("type", t.to_string()));
    }
    let mut res = q.await?;
    let rows: Vec<MemoryRow> = res.take(0)?;
    let updated = rows.into_iter().next().ok_or_else(|| AppError::internal("memory update returned no row"))?;

    if updated.mem_type != "observation" {
        db.query(r#"UPDATE memory SET status = "stale" WHERE subject = $subject AND type = "observation""#)
            .bind(("subject", updated.subject.clone()))
            .await?;
    }
    Ok(Some(memory_out(&updated, None)))
}

/// Edit an entity's own fields -- name/aliases/summary -- vault-scoped, same
/// not-found-is-None convention as `get_entity`. Only the fields passed
/// (`Some`) are updated.
pub async fn update_entity(
    db: &Db,
    owner: &RecordId,
    entity_id: &RecordId,
    name: Option<&str>,
    aliases: Option<Vec<String>>,
    summary: Option<&str>,
) -> AppResult<Option<EntityOut>> {
    let Some(mut row) = select_entity(db, entity_id).await? else { return Ok(None) };
    if !accessible(db, owner, &row.vault).await? {
        return Ok(None);
    }

    let mut set_clauses: Vec<&str> = Vec::new();
    if name.is_some() {
        set_clauses.push("name = $name");
    }
    if aliases.is_some() {
        set_clauses.push("aliases = $aliases");
    }
    if summary.is_some() {
        set_clauses.push("summary = $summary");
    }

    if !set_clauses.is_empty() {
        set_clauses.push("updated_at = time::now()");
        let query = format!("UPDATE $id SET {} RETURN AFTER", set_clauses.join(", "));
        let mut q = db.query(query).bind(("id", entity_id.clone()));
        if let Some(n) = name {
            q = q.bind(("name", n.trim().to_string()));
        }
        if let Some(a) = aliases {
            q = q.bind(("aliases", a));
        }
        if let Some(s) = summary {
            q = q.bind(("summary", s.to_string()));
        }
        let mut res = q.await?;
        let rows: Vec<EntityRow> = match res.take(0) {
            Err(e) if e.to_string().contains("already contains") => {
                return Err(AppError::bad_request(format!(
                    "another {} in this vault is already named {:?}; merge them instead",
                    entity_id.table(),
                    name.unwrap_or_default()
                )))
            }
            r => r?,
        };
        row = rows.into_iter().next().ok_or_else(|| AppError::internal("entity update returned no row"))?;
    }

    Ok(Some(entity_out(entity_id.table(), &row)))
}

/// Delete an entity and everything hanging off it: its `memory` rows and its
/// `relates_to` edges in both directions. Vault-scoped, same
/// not-found-is-false convention as `delete_memory`.
pub async fn delete_entity(db: &Db, owner: &RecordId, entity_id: &RecordId) -> AppResult<bool> {
    let Some(row) = select_entity(db, entity_id).await? else { return Ok(false) };
    if !accessible(db, owner, &row.vault).await? {
        return Ok(false);
    }
    db.query("DELETE memory WHERE subject = $id").bind(("id", entity_id.clone())).await?;
    db.query("DELETE relates_to WHERE in = $id OR out = $id").bind(("id", entity_id.clone())).await?;
    db.query("DELETE $id").bind(("id", entity_id.clone())).await?;
    Ok(true)
}

/// Merge `loser_id` into `winner_id` -- for two entities of the same `kind`
/// that turned out to be duplicates. Reassigns the loser's `memory` rows and
/// `relates_to` edges (both directions) to the winner, adds the loser's name
/// and aliases as aliases of the winner, then deletes the loser -- all in one
/// transaction ([`merge_rows`]). If both had an observation they become one
/// stale observation, rebuilt on the next consolidation. Returns the
/// winner's row after the merge.
///
/// Errors (400) if the two ids are the same, of different kinds or vaults, or not
/// found / `owner` isn't a member of either one's vault.
pub async fn merge_entities(
    db: &Db,
    owner: &RecordId,
    winner_id: &RecordId,
    loser_id: &RecordId,
) -> AppResult<EntityOut> {
    if winner_id == loser_id {
        return Err(AppError::bad_request("winner_id and loser_id must refer to different entities"));
    }
    if winner_id.table() != loser_id.table() {
        return Err(AppError::bad_request(format!(
            "cannot merge entities of different kinds: {:?} vs {:?}",
            winner_id.table(),
            loser_id.table()
        )));
    }

    let winner = select_entity(db, winner_id)
        .await?
        .ok_or_else(|| AppError::bad_request(format!("winner entity not found: {winner_id}")))?;
    if !accessible(db, owner, &winner.vault).await? {
        return Err(AppError::bad_request(format!("winner entity not found: {winner_id}")));
    }
    let loser = select_entity(db, loser_id)
        .await?
        .ok_or_else(|| AppError::bad_request(format!("loser entity not found: {loser_id}")))?;
    if !accessible(db, owner, &loser.vault).await? {
        return Err(AppError::bad_request(format!("loser entity not found: {loser_id}")));
    }
    same_vault(&winner, &loser)?;

    merge_rows(db, winner_id, loser_id, loser_names(&loser)).await?;
    let winner = select_entity(db, winner_id).await?.ok_or_else(|| AppError::internal("winner vanished during merge"))?;
    Ok(entity_out(winner_id.table(), &winner))
}

/// What a merged-away entity leaves on the winner: its name and aliases.
fn loser_names(loser: &EntityRow) -> Vec<String> {
    std::iter::once(&loser.name).chain(&loser.aliases).filter(|n| !n.is_empty()).cloned().collect()
}

/// Folds every observation of `$winner` and `$loser` into the oldest one,
/// owned by `$winner`: texts joined, lineages unioned, marked stale so the
/// next consolidation rebuilds it from the merged raw facts. Runs inside a
/// caller's transaction (it frees the unique observation slot first).
const FOLD_OBSERVATIONS: &str = "\
    LET $obs = (SELECT id, text, source_memories, created_at FROM array::union( \
        (SELECT VALUE id FROM memory WHERE subject = $winner AND type = \"observation\"), \
        (SELECT VALUE id FROM memory WHERE subject = $loser AND type = \"observation\")) ORDER BY created_at, id); \
    IF array::len($obs) > 1 { \
        DELETE array::slice($obs.id, 1); \
        UPDATE $obs[0].id SET subject = $winner, text = array::join($obs.text, \"\\n\\n\"), version += 1, \
            source_memories = array::distinct(array::flatten($obs.map(|$o| $o.source_memories ?? []))), \
            status = \"stale\", updated_at = time::now(); \
    };";

/// Moves everything of `loser` onto `winner` (same kind and vault, already
/// checked) and deletes `loser`, in ONE transaction: memories (observations
/// folded, see [`FOLD_OBSERVATIONS`]), `relates_to` edges in both directions
/// (re-created on the winner unless it already has that exact edge -- the
/// only conflict that is skipped; edges between the two are dropped), and
/// `add_aliases` onto the winner. Any failing statement rolls all of it
/// back, so a merge never half-happens. Every read is an index or graph
/// lookup, keeping the transaction's read set (and so its chance of
/// clashing with concurrent writes) small.
async fn merge_rows(db: &Db, winner: &RecordId, loser: &RecordId, add_aliases: Vec<String>) -> AppResult<()> {
    db.query(format!(
        "BEGIN TRANSACTION; {FOLD_OBSERVATIONS} \
         UPDATE memory SET subject = $winner WHERE subject = $loser; \
         UPDATE memory SET status = \"stale\", updated_at = time::now() WHERE subject = $winner AND type = \"observation\"; \
         FOR $e IN (SELECT * FROM $loser->relates_to WHERE out NOT IN [$winner, $loser]) {{ \
             IF array::len(SELECT id FROM relates_to WHERE in = $winner AND out = $e.out AND label = $e.label) = 0 {{ \
                 LET $o = $e.out; \
                 RELATE $winner->relates_to->$o SET label = $e.label, owner = $e.owner, source = $e.source, created_at = $e.created_at; \
             }}; \
         }}; \
         FOR $e IN (SELECT * FROM $loser<-relates_to WHERE in NOT IN [$winner, $loser]) {{ \
             IF array::len(SELECT id FROM relates_to WHERE in = $e.in AND out = $winner AND label = $e.label) = 0 {{ \
                 LET $i = $e.in; \
                 RELATE $i->relates_to->$winner SET label = $e.label, owner = $e.owner, source = $e.source, created_at = $e.created_at; \
             }}; \
         }}; \
         DELETE array::union((SELECT VALUE id FROM $loser->relates_to), (SELECT VALUE id FROM $loser<-relates_to)); \
         UPDATE $winner SET aliases = array::sort(array::union(aliases, $aliases)), updated_at = time::now(); \
         DELETE $loser; \
         COMMIT TRANSACTION;"
    ))
    .bind(("winner", winner.clone()))
    .bind(("loser", loser.clone()))
    .bind(("aliases", add_aliases))
    .await?
    .check()?;
    Ok(())
}

/// Folds rows that would violate one of `db::UNIQUE_STATEMENTS`' indexes on
/// `table`, so the index can be defined: entities sharing a vault and
/// case-insensitive name are merged into the oldest ([`merge_rows`]), and a
/// subject's several observations into one stale one. Runs at startup until
/// the index exists; a no-op once it does.
pub async fn dedupe(db: &Db, table: &str) -> AppResult<()> {
    let index = crate::db::UNIQUE_STATEMENTS
        .iter()
        .find(|(t, _)| *t == table)
        .and_then(|(_, sql)| sql.split_whitespace().nth(5))
        .ok_or_else(|| AppError::internal(format!("no unique index for {table}")))?;
    let mut res = db.query(format!("INFO FOR TABLE {table}")).await?;
    let info: Option<serde_json::Value> = res.take(0)?;
    if info.as_ref().and_then(|i| i.pointer(&format!("/indexes/{index}"))).is_some() {
        return Ok(());
    }

    #[derive(Deserialize)]
    struct Group {
        ids: Vec<RecordId>,
    }
    #[derive(Deserialize)]
    struct Group1 {
        id: RecordId,
    }
    if table == "memory" {
        db.query("UPDATE memory SET type = type WHERE type = \"observation\" AND obs_subject = NONE").await?.check()?;
        let mut res = db
            .query(
                "SELECT subject, array::group(id) AS ids FROM memory WHERE type = \"observation\" GROUP BY subject",
            )
            .await?;
        let groups: Vec<Group> = res.take(0)?;
        #[derive(Deserialize)]
        struct Subject {
            subject: RecordId,
        }
        for g in groups.into_iter().filter(|g| g.ids.len() > 1) {
            let s: Option<Subject> = db.select(g.ids[0].clone()).await?;
            let Some(s) = s else { continue };
            db.query(format!("BEGIN TRANSACTION; {FOLD_OBSERVATIONS} COMMIT TRANSACTION;"))
                .bind(("winner", s.subject.clone()))
                .bind(("loser", s.subject))
                .await?
                .check()?;
        }
        return Ok(());
    }

    let table = kind_table(table)?;
    db.query(format!("UPDATE {table} SET name = name WHERE name_key = NONE OR alias_keys = NONE")).await?.check()?;
    let mut res = db
        .query(format!(
            "SELECT vault, name_key, array::group(id) AS ids FROM {table} GROUP BY vault, name_key"
        ))
        .await?;
    let groups: Vec<Group> = res.take(0)?;
    for g in groups.into_iter().filter(|g| g.ids.len() > 1) {
        // the oldest one wins
        let mut res = db.query("SELECT id, created_at FROM $ids ORDER BY created_at, id").bind(("ids", g.ids)).await?;
        let ids: Vec<RecordId> = res.take::<Vec<Group1>>(0)?.into_iter().map(|r| r.id).collect();
        let Some((winner, losers)) = ids.split_first() else { continue };
        for loser in losers {
            let names = select_entity(db, loser).await?.map(|r| loser_names(&r)).unwrap_or_default();
            merge_rows(db, winner, loser, names).await?;
        }
    }
    Ok(())
}

/// An entity's row plus its `memory` entries and `relates_to` edges in both
/// directions, or `None` if it doesn't exist / `owner` isn't a member of its
/// vault. Every row carries `owner_email` -- who wrote it -- since in a
/// shared vault that's no longer implied by who's asking.
pub async fn get_entity(db: &Db, owner: &RecordId, entity_id: &RecordId) -> AppResult<Option<EntityDetail>> {
    let Some(row) = select_entity(db, entity_id).await? else { return Ok(None) };
    if !accessible(db, owner, &row.vault).await? {
        return Ok(None);
    }

    // Only rows in the entity's own vault: a cross-vault edge or memory left
    // over from before relations/merges were confined to one vault stays hidden.
    // Edges are read through the graph (`$id->relates_to`): with a plain
    // `WHERE out = $id AND in.vault = $vault` SurrealDB 2.3 plans `in.vault`
    // as a lookup on the vault/name indexes and returns nothing.
    let mut mem_res = db
        .query("SELECT * FROM memory WITH INDEX memory_subject_idx WHERE subject = $id AND vault = $vault ORDER BY created_at DESC")
        .bind(("id", entity_id.clone()))
        .bind(("vault", row.vault.clone()))
        .await?;
    let memories: Vec<MemoryRow> = mem_res.take(0)?;

    let mut out_res = db
        .query("SELECT * FROM $id->relates_to WHERE out.vault = $vault")
        .bind(("id", entity_id.clone()))
        .bind(("vault", row.vault.clone()))
        .await?;
    let outgoing: Vec<RelationRow> = out_res.take(0)?;
    let mut in_res = db
        .query("SELECT * FROM $id<-relates_to WHERE in.vault = $vault")
        .bind(("id", entity_id.clone()))
        .bind(("vault", row.vault.clone()))
        .await?;
    let incoming: Vec<RelationRow> = in_res.take(0)?;

    let mut owner_ids: Vec<Option<RecordId>> = vec![row.owner.clone()];
    owner_ids.extend(memories.iter().map(|m| m.owner.clone()));
    owner_ids.extend(outgoing.iter().map(|r| r.owner.clone()));
    owner_ids.extend(incoming.iter().map(|r| r.owner.clone()));
    let emails = emails_for(db, owner_ids).await?;

    let memory_out_rows: Vec<MemoryOut> = memories
        .iter()
        .map(|m| memory_out(m, m.owner.as_ref().and_then(|o| emails.get(&o.to_string()).cloned())))
        .collect();
    let mut relations: Vec<RelationOut> = outgoing
        .iter()
        .map(|r| relation_out(r, Some("out"), r.owner.as_ref().and_then(|o| emails.get(&o.to_string()).cloned())))
        .collect();
    relations.extend(
        incoming
            .iter()
            .map(|r| relation_out(r, Some("in"), r.owner.as_ref().and_then(|o| emails.get(&o.to_string()).cloned()))),
    );

    Ok(Some(EntityDetail {
        id: entity_id.to_string(),
        kind: entity_id.table().to_string(),
        name: row.name.clone(),
        aliases: row.aliases.clone(),
        summary: row.summary.clone(),
        owner_email: row.owner.as_ref().and_then(|o| emails.get(&o.to_string()).cloned()),
        memory: memory_out_rows,
        relations,
    }))
}

/// Entities across `kind` (or all `KINDS`) in the resolved vault, ordered by
/// name within each kind. Offset-paginated across the combined (all-kinds)
/// result -- `limit=None` returns everything from `offset` onward. Returns
/// `{results, total, has_more}`.
pub async fn list_entities(
    db: &Db,
    owner: &RecordId,
    kind: Option<&str>,
    vault_id: Option<&RecordId>,
    limit: Option<usize>,
    offset: usize,
) -> AppResult<ListEntitiesOut> {
    let vault = resolve_vault(db, owner, vault_id).await?;
    let kinds: Vec<&str> = match kind {
        Some(k) => vec![kind_table(k)?],
        None => KINDS.to_vec(),
    };

    let mut rows_by_kind: Vec<(&str, EntityRow)> = Vec::new();
    for k in kinds {
        let mut res =
            db.query(format!("SELECT * FROM {k} WHERE vault = $vault ORDER BY name")).bind(("vault", vault.clone())).await?;
        let rows: Vec<EntityRow> = res.take(0)?;
        rows_by_kind.extend(rows.into_iter().map(|r| (k, r)));
    }

    let total = rows_by_kind.len();
    let page: Vec<(&str, EntityRow)> = match limit {
        None => rows_by_kind.into_iter().skip(offset).collect(),
        Some(l) => rows_by_kind.into_iter().skip(offset).take(l).collect(),
    };
    let results: Vec<EntityOut> = page.iter().map(|(k, r)| entity_out(k, r)).collect();
    let has_more = offset + results.len() < total;

    Ok(ListEntitiesOut { results, total, has_more })
}

/// `{nodes, edges}` -- the full entity graph for the dashboard's
/// force-directed view (resolved vault, or `owner`'s personal vault), or a
/// subgraph restricted to `kinds`.
///
/// v1 limitation, accepted as-is (matches the Python version): an edge is
/// only included when BOTH endpoints are in the requested kind set.
pub async fn graph(
    db: &Db,
    owner: &RecordId,
    kinds: Option<&[String]>,
    vault_id: Option<&RecordId>,
) -> AppResult<GraphOut> {
    let vault = resolve_vault(db, owner, vault_id).await?;
    let requested: Vec<&str> = match kinds {
        Some(ks) => {
            let mut out = Vec::with_capacity(ks.len());
            for k in ks {
                out.push(kind_table(k)?);
            }
            out
        }
        None => KINDS.to_vec(),
    };

    let mut rows_by_kind: Vec<(&str, EntityRow)> = Vec::new();
    for k in &requested {
        let mut res =
            db.query(format!("SELECT * FROM {k} WHERE vault = $vault")).bind(("vault", vault.clone())).await?;
        let rows: Vec<EntityRow> = res.take(0)?;
        rows_by_kind.extend(rows.into_iter().map(|r| (*k, r)));
    }

    let emails = emails_for(db, rows_by_kind.iter().map(|(_, r)| r.owner.clone()).collect()).await?;
    let nodes: Vec<GraphNode> = rows_by_kind
        .iter()
        .map(|(k, r)| GraphNode {
            id: r.id.to_string(),
            kind: k.to_string(),
            name: r.name.clone(),
            owner_email: r.owner.as_ref().and_then(|o| emails.get(&o.to_string()).cloned()),
        })
        .collect();
    let ids: Vec<RecordId> = rows_by_kind.iter().map(|(_, r)| r.id.clone()).collect();

    let mut edges: Vec<GraphEdge> = Vec::new();
    if !ids.is_empty() {
        // `in IN $ids` doesn't match against a union-typed `record<a|b|c>`
        // field reliably -- `$ids CONTAINS field` does.
        let mut res = db
            .query("SELECT * FROM relates_to WHERE $ids CONTAINS in AND $ids CONTAINS out")
            .bind(("ids", ids))
            .await?;
        let rows: Vec<RelationRow> = res.take(0)?;
        let edge_emails = emails_for(db, rows.iter().map(|r| r.owner.clone()).collect()).await?;
        edges = rows
            .iter()
            .map(|r| GraphEdge {
                source: r.in_.to_string(),
                target: r.out_.to_string(),
                label: r.label.clone(),
                owner_email: r.owner.as_ref().and_then(|o| edge_emails.get(&o.to_string()).cloned()),
            })
            .collect();
    }

    Ok(GraphOut { nodes, edges })
}

/// Expected `relates_to` label for a code-entity's parent pairing --
/// file->repository = "part_of", symbol->file = "defined_in". Mirrors
/// `entities/service.py`'s `_PARENT_LABELS`.
fn parent_label(kind: &str, parent_kind: &str) -> &'static str {
    match (kind, parent_kind) {
        ("file", "repository") => "part_of",
        ("symbol", "file") => "defined_in",
        _ => "part_of",
    }
}

/// Find-or-create a `repository`/`file`/`symbol` entity -- the write path an
/// external agent calls as it maps a codebase, parallel to `write_memory`'s
/// pattern for person/organisation/location.
///
/// `parent_id`, if given, auto-creates a `relates_to` edge from this entity
/// to the parent, labelled by the expected kind pairing. This is an
/// agent-facing API, so a mismatched pairing isn't an error -- it just falls
/// back to the generic "part_of" label rather than hard-failing.
pub async fn upsert_code_entity(
    db: &Db,
    owner: &RecordId,
    kind: &str,
    name: &str,
    parent_id: Option<&RecordId>,
    summary: Option<&str>,
    vault_id: Option<&RecordId>,
) -> AppResult<EntityOut> {
    if !CODE_KINDS.contains(&kind) {
        return Err(AppError::bad_request(format!("unknown code kind {kind:?}")));
    }
    let mut entity = upsert_entity(db, owner, kind, name, None, vault_id).await?;
    let entity_rid: RecordId = entity.id.parse().map_err(|_| AppError::internal("entity id did not round-trip"))?;

    if let Some(summary) = summary {
        let mut updated = db
            .query("UPDATE $id SET summary = $summary, updated_at = time::now() RETURN AFTER")
            .bind(("id", entity_rid.clone()))
            .bind(("summary", summary.to_string()))
            .await?;
        let rows: Vec<EntityRow> = updated.take(0)?;
        let row = rows.into_iter().next().ok_or_else(|| AppError::internal("entity update returned no row"))?;
        entity = entity_out(kind, &row);
    }

    if let Some(parent_id) = parent_id {
        let label = parent_label(kind, parent_id.table());
        add_relation(db, owner, &entity_rid, parent_id, label, None).await?;
    }

    Ok(entity)
}

#[cfg(test)]
mod tests {
    use super::check_memory_edit;

    #[test]
    fn memory_edit_needs_something_to_change() {
        assert!(check_memory_edit("world", None, None).is_err());
        assert!(check_memory_edit("world", Some("new"), None).is_ok());
        assert!(check_memory_edit("world", None, Some("experience")).is_ok());
    }

    #[test]
    fn memory_edit_rejects_blank_text_and_observation_type_changes() {
        assert!(check_memory_edit("world", Some("  "), None).is_err());
        assert!(check_memory_edit("world", None, Some("observation")).is_err());
        assert!(check_memory_edit("world", None, Some("bogus")).is_err());
        assert!(check_memory_edit("observation", None, Some("world")).is_err());
        assert!(check_memory_edit("observation", Some("revised belief"), None).is_ok());
    }

    use super::*;

    #[test]
    fn decorated_names_split_into_base_and_aliases() {
        assert_eq!(split_decorated("Jane Doe (Acme/JD)"), Some(("Jane Doe", vec!["Acme".into(), "JD".into()])));
        assert_eq!(split_decorated("Jane Doe(Acme, JD; jd2)").unwrap().1, vec!["Acme", "JD", "jd2"]);
        assert_eq!(split_decorated("Jane Doe"), None);
        assert_eq!(split_decorated("(Acme)"), None);
        assert_eq!(split_decorated("Jane Doe ()"), None);
    }

    #[test]
    fn kinds_match_schema_union() {
        assert_eq!(KINDS, ["person", "organisation", "location", "repository", "file", "symbol"]);
    }

    #[test]
    fn code_kinds_is_subset_of_kinds() {
        for k in CODE_KINDS {
            assert!(KINDS.contains(k));
        }
    }

    #[test]
    fn kind_table_accepts_known_kinds() {
        assert_eq!(kind_table("person").unwrap(), "person");
        assert_eq!(kind_table("symbol").unwrap(), "symbol");
    }

    #[test]
    fn kind_table_rejects_unknown_or_injected_kind() {
        assert!(kind_table("user").is_err());
        assert!(kind_table("person; DROP TABLE user").is_err());
        assert!(kind_table("").is_err());
    }

    #[test]
    fn parent_label_maps_file_to_repository() {
        assert_eq!(parent_label("file", "repository"), "part_of");
    }

    #[test]
    fn parent_label_maps_symbol_to_file() {
        assert_eq!(parent_label("symbol", "file"), "defined_in");
    }

    #[test]
    fn parent_label_falls_back_to_part_of_for_mismatched_pairing() {
        assert_eq!(parent_label("symbol", "repository"), "part_of");
        assert_eq!(parent_label("file", "symbol"), "part_of");
    }

    #[test]
    fn default_memory_type_is_world() {
        assert_eq!(default_memory_type(), "world");
    }

    #[test]
    fn only_entity_tables_are_entity_ids() {
        assert!(is_entity_id(&"person:a".parse().unwrap()));
        assert!(is_entity_id(&"symbol:a".parse().unwrap()));
        for other in ["memory:a", "vault_member:a", "vault:a", "user:a", "cache_record:a"] {
            assert!(!is_entity_id(&other.parse().unwrap()), "{other}");
        }
    }

    fn entity_in(vault: &str) -> EntityRow {
        EntityRow {
            id: "person:x".parse().unwrap(),
            owner: None,
            vault: vault.parse().unwrap(),
            name: String::new(),
            aliases: Vec::new(),
            summary: String::new(),
        }
    }

    #[test]
    fn relations_and_merges_need_one_vault() {
        assert!(same_vault(&entity_in("vault:a"), &entity_in("vault:a")).is_ok());
        assert!(same_vault(&entity_in("vault:a"), &entity_in("vault:b")).is_err());
    }
}
