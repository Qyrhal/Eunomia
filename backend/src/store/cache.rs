//! Statements for the cache area. See the module docs in store/mod.rs.
//!
//! Not here, built at runtime with `store::dynamic` (the variant count
//! explodes, the SQL embeds a literal, or one statement is repeated per term or entity):
//! `cache.keyword_ids` (per term, filtered), `cache.semantic_ids` (KNN `<|k,ef|>` needs literal
//! integers), `cache.semantic_exact`, `cache.list_records`, `cache.graph_entities`,
//! `cache.graph_memories`, `cache.memory_text_terms`.

use super::Stmt;

pub const MEMORY_FOR_EMBED: Stmt = Stmt::new("cache.memory_for_embed", "SELECT id, text, type FROM memory WHERE vault = $vault LIMIT $limit");
pub const RECORDS_FOR_EMBED: Stmt = Stmt::new(
    "cache.records_for_embed",
    "SELECT id, title, body_text, embedding FROM cache_record WHERE owner = $owner AND deleted = false LIMIT $limit",
);

// newest first, limited in the database, so a broad range never loads the whole range
pub const CACHE_RECORDS_IN_RANGE: Stmt = Stmt::new(
    "cache.records_in_range",
    "SELECT id, occurred_at FROM cache_record WHERE owner = $owner AND deleted = false \
     AND occurred_at >= $since AND occurred_at <= $until ORDER BY occurred_at DESC LIMIT $limit",
);
pub const MEMORIES_IN_RANGE: Stmt = Stmt::new(
    "cache.memories_in_range",
    "SELECT id, created_at FROM memory WHERE vault = $vault \
     AND created_at >= $since AND created_at <= $until ORDER BY created_at DESC LIMIT $limit",
);

pub const DELETE_SYNC_LINKS: Stmt = Stmt::new("cache.delete_sync_links", "DELETE linked_to WHERE in = $id AND origin = 'sync'");
pub const RELATE_SYNC_LINK: Stmt = Stmt::new("cache.relate_sync_link", "RELATE $in->linked_to->$out SET rel = $rel, origin = 'sync'");
pub const TOUCH_INGESTED: Stmt = Stmt::new("cache.touch_ingested", "UPDATE $id SET ingested_at = time::now()");
pub const UPSERT_RECORD: Stmt = Stmt::new(
    "cache.upsert_record",
    "UPSERT $id SET owner = $owner, source = $source, type = $type, external_id = $external_id, \
     title = $title, body_text = $body_text, occurred_at = $occurred_at, url = $url, \
     payload = $payload, content_hash = $content_hash, ingested_at = $ingested_at, \
     updated_at = $updated_at, deleted = $deleted RETURN AFTER",
);
/// Whether a stored record already has its embedding (an unchanged replayed record is re-embedded only if not).
pub const HAS_EMBEDDING: Stmt = Stmt::new("cache.has_embedding", "SELECT VALUE embedding != NONE FROM ONLY $id");
pub const SET_EMBEDDING: Stmt = Stmt::new("cache.set_embedding", "UPDATE $id SET embedding = $embedding");

/// One live record of the owner by id (`$id` a `cache_record` record id), a tombstone reads as absent.
pub const RECORDS_BY_IDS: Stmt =
    Stmt::new("cache.records_by_ids", "SELECT * FROM $ids WHERE owner = $owner AND deleted = false");

/// A memory page for recall's hydrate: this vault's, and not a stale observation (its facts changed
/// since it was consolidated, so it is not a current fact; the raw facts are still recalled).
pub const MEMORIES_FOR_RECALL: Stmt = Stmt::new(
    "cache.memories_for_recall",
    "SELECT id, text, source, created_at, type FROM $ids WHERE vault = $vault \
     AND !(type = \"observation\" AND status = \"stale\")",
);

/// Each source's live linked records in either direction, one traversal for the whole batch (recall's
/// graph arm). Tombstoned sources and targets are skipped.
pub const LIVE_NEIGHBOURS: Stmt = Stmt::new(
    "cache.live_neighbours",
    "SELECT id, ->linked_to->(cache_record WHERE deleted = false) AS fwd, \
     <-linked_to<-(cache_record WHERE deleted = false) AS back FROM $ids WHERE deleted = false",
);

// A tombstoned endpoint hides the link (a not-yet-synced target, like a category placeholder, does not),
// and a tombstoned record has no links at all.
pub const LINKS_OUT: Stmt = Stmt::new(
    "cache.links_out",
    "SELECT rel, out FROM linked_to WHERE in = $id AND in.deleted != true AND out.deleted != true",
);
pub const LINKS_OUT_REL: Stmt = Stmt::new(
    "cache.links_out_rel",
    "SELECT rel, out FROM linked_to WHERE in = $id AND rel = $rel AND in.deleted != true AND out.deleted != true",
);
pub const LINKS_IN: Stmt = Stmt::new(
    "cache.links_in",
    "SELECT rel, in FROM linked_to WHERE out = $id AND in.deleted != true AND out.deleted != true",
);
pub const LINKS_IN_REL: Stmt = Stmt::new(
    "cache.links_in_rel",
    "SELECT rel, in FROM linked_to WHERE out = $id AND rel = $rel AND in.deleted != true AND out.deleted != true",
);

pub const EMBED_CACHE_GET: Stmt = Stmt::new("cache.embed_cache_get", "SELECT text_hmac, vector FROM embed_cache WHERE text_hmac IN $keys");
pub const EMBED_CACHE_PUT: Stmt = Stmt::new("cache.embed_cache_put", "UPSERT $id SET text_hmac = $hmac, vector = $vector");

pub const RECORD_AUDIT: Stmt = Stmt::new(
    "cache.record_audit",
    "CREATE audit_log SET owner = $owner, tool_name = $tool_name, \
     args_summary = $args_summary, outcome = $outcome",
);
pub const AUDIT_PAGE: Stmt = Stmt::new(
    "cache.audit_page",
    "SELECT * FROM audit_log WHERE owner = $owner ORDER BY created_at DESC LIMIT $limit START $offset",
);
pub const AUDIT_COUNT: Stmt = Stmt::new("cache.audit_count", "SELECT count() FROM audit_log WHERE owner = $owner GROUP ALL");

pub const ALL: &[&Stmt] = &[
    &MEMORY_FOR_EMBED,
    &RECORDS_FOR_EMBED,
    &CACHE_RECORDS_IN_RANGE,
    &MEMORIES_IN_RANGE,
    &DELETE_SYNC_LINKS,
    &RELATE_SYNC_LINK,
    &TOUCH_INGESTED,
    &UPSERT_RECORD,
    &HAS_EMBEDDING,
    &RECORDS_BY_IDS,
    &MEMORIES_FOR_RECALL,
    &LIVE_NEIGHBOURS,
    &SET_EMBEDDING,
    &LINKS_OUT,
    &LINKS_OUT_REL,
    &LINKS_IN,
    &LINKS_IN_REL,
    &EMBED_CACHE_GET,
    &EMBED_CACHE_PUT,
    &RECORD_AUDIT,
    &AUDIT_PAGE,
    &AUDIT_COUNT,
];
