//! Statements for the cache area. See the module docs in store/mod.rs.
//!
//! Not here, built at runtime with `store::dynamic` (the variant count
//! explodes or the SQL embeds a literal): `cache.search_filtered`,
//! `cache.semantic_ids` (KNN `<|k,ef|>` needs literal integers),
//! `cache.list_records`, `cache.count_records`, `cache.generic_search`,
//! `cache.generic_list`, `cache.generic_count`.

use super::Stmt;

pub const MEMORY_FOR_EMBED: Stmt = Stmt::new("cache.memory_for_embed", "SELECT id, text, type FROM memory WHERE vault = $vault LIMIT $limit");
pub const RECORDS_FOR_EMBED: Stmt = Stmt::new(
    "cache.records_for_embed",
    "SELECT id, title, body_text, embedding FROM cache_record WHERE owner = $owner AND deleted = false LIMIT $limit",
);

macro_rules! entity_names {
    ($($ident:ident => $table:literal),* $(,)?) => {
        $(pub const $ident: Stmt = Stmt::new(
            concat!("cache.entity_names_", $table),
            concat!("SELECT id, name, aliases FROM ", $table, " WHERE vault = $vault"),
        );)*
        /// One statement per entity table, in the order recall scans them.
        pub const ENTITY_NAMES: &[&Stmt] = &[$(&$ident),*];
    };
}
entity_names! {
    ENTITY_NAMES_PERSON => "person",
    ENTITY_NAMES_ORGANISATION => "organisation",
    ENTITY_NAMES_LOCATION => "location",
    ENTITY_NAMES_REPOSITORY => "repository",
    ENTITY_NAMES_FILE => "file",
    ENTITY_NAMES_SYMBOL => "symbol",
}

pub const MEMORIES_BY_SUBJECT: Stmt = Stmt::new("cache.memories_by_subject", "SELECT * FROM memory WHERE subject = $id ORDER BY created_at DESC");
pub const MEMORY_BM25: Stmt = Stmt::new(
    "cache.memory_bm25",
    "SELECT id, search::score(1) AS score FROM memory \
     WHERE vault = $vault AND text @1@ $t ORDER BY score DESC LIMIT $limit",
);
pub const CACHE_RECORDS_IN_RANGE: Stmt = Stmt::new(
    "cache.records_in_range",
    "SELECT id, occurred_at FROM cache_record WHERE owner = $owner AND deleted = false \
     AND occurred_at >= <datetime>$since AND occurred_at <= <datetime>$until",
);
pub const MEMORIES_IN_RANGE: Stmt = Stmt::new(
    "cache.memories_in_range",
    "SELECT id, created_at FROM memory WHERE vault = $vault \
     AND created_at >= <datetime>$since AND created_at <= <datetime>$until",
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
pub const KEYWORD_IDS: Stmt = Stmt::new(
    "cache.keyword_ids",
    "SELECT id, search::score(1) AS score FROM cache_record \
     WHERE owner = $owner AND title @1@ $t AND deleted = false ORDER BY score DESC LIMIT $limit; \
     SELECT id, 0.0 AS score FROM cache_record WHERE owner = $owner AND deleted = false \
     AND body_text @1@ $t LIMIT $limit",
);

/// Exact cosine ranking of one owner's embedded records; the fallback when KNN comes up short.
pub const SEMANTIC_EXACT: Stmt = Stmt::new(
    "cache.semantic_exact",
    "SELECT id, vector::similarity::cosine(embedding, $vec) AS sim FROM cache_record \
     WHERE owner = $owner AND deleted = false AND embedding != NONE ORDER BY sim DESC LIMIT $limit",
);

pub const LINKS_OUT: Stmt = Stmt::new("cache.links_out", "SELECT rel, out FROM linked_to WHERE in = $id");
pub const LINKS_OUT_REL: Stmt = Stmt::new("cache.links_out_rel", "SELECT rel, out FROM linked_to WHERE in = $id AND rel = $rel");
pub const LINKS_IN: Stmt = Stmt::new("cache.links_in", "SELECT rel, in FROM linked_to WHERE out = $id");
pub const LINKS_IN_REL: Stmt = Stmt::new("cache.links_in_rel", "SELECT rel, in FROM linked_to WHERE out = $id AND rel = $rel");

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
    &ENTITY_NAMES_PERSON,
    &ENTITY_NAMES_ORGANISATION,
    &ENTITY_NAMES_LOCATION,
    &ENTITY_NAMES_REPOSITORY,
    &ENTITY_NAMES_FILE,
    &ENTITY_NAMES_SYMBOL,
    &MEMORIES_BY_SUBJECT,
    &MEMORY_BM25,
    &CACHE_RECORDS_IN_RANGE,
    &MEMORIES_IN_RANGE,
    &DELETE_SYNC_LINKS,
    &RELATE_SYNC_LINK,
    &TOUCH_INGESTED,
    &UPSERT_RECORD,
    &HAS_EMBEDDING,
    &SET_EMBEDDING,
    &KEYWORD_IDS,
    &SEMANTIC_EXACT,
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
