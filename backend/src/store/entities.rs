//! Statements for the entities area. See the module docs in store/mod.rs.

use super::{dynamic, Q, Stmt};
use crate::db::Db;

pub const EMAILS_FOR: Stmt = Stmt::new("entities.emails_for", "SELECT id, email FROM user WHERE id IN $ids");

pub const MERGE_ALIASES: Stmt = Stmt::new(
    "entities.merge_aliases",
    "UPDATE $id SET aliases = array::sort(array::union(aliases, $aliases)), updated_at = time::now() RETURN AFTER",
);

// One round trip, all-or-nothing; see add_memory in entities/service.rs.
pub const WRITE_OBSERVATION: Stmt = Stmt::new(
    "entities.write_observation",
    r#"BEGIN TRANSACTION;
        LET $cur = (SELECT VALUE id FROM $obs_id);
        LET $legacy = IF array::len($cur) > 0 { [] } ELSE {
            (SELECT VALUE id FROM memory WHERE subject = $subject AND type = "observation" LIMIT 1)
        };
        LET $target = array::concat($cur, $legacy);
        LET $row = IF array::len($target) > 0 {
            (UPDATE $target[0] SET text = $text, version = version + 1, status = "fresh", updated_at = time::now() RETURN AFTER)
        } ELSE {
            (CREATE $obs_id SET owner = $owner, vault = $vault, subject = $subject, text = $text, type = "observation", source = $source RETURN AFTER)
        };
        RETURN $row;
        COMMIT TRANSACTION;"#,
);

pub const WRITE_FACT: Stmt = Stmt::new(
    "entities.write_fact",
    r#"BEGIN TRANSACTION;
        CREATE memory SET owner = $owner, vault = $vault, subject = $subject, text = $text, type = $type, source = $source RETURN AFTER;
        LET $cur = (SELECT VALUE id FROM $obs_id);
        IF array::len($cur) > 0 {
            UPDATE $obs_id SET status = "stale" WHERE status != "stale"
        } ELSE {
            UPDATE memory SET status = "stale" WHERE subject = $subject AND type = "observation" AND status != "stale"
        };
        COMMIT TRANSACTION;"#,
);

pub const RELATE_RETURNING: Stmt = Stmt::new(
    "entities.relate_returning",
    "RELATE $in->relates_to->$out SET label = $label, owner = $owner, source = $source RETURN AFTER",
);

pub const RELATE: Stmt =
    Stmt::new("entities.relate", "RELATE $in->relates_to->$out SET label = $label, owner = $owner, source = $source");

pub const FIND_RELATION: Stmt = Stmt::new(
    "entities.find_relation",
    "SELECT * FROM relates_to WHERE in = $in AND out = $out AND label = $label LIMIT 1",
);

pub const DELETE_RECORD: Stmt = Stmt::new("entities.delete_record", "DELETE $id");

pub const UPDATE_MEMORY_TEXT: Stmt = Stmt::new(
    "entities.update_memory_text",
    "UPDATE $id SET version = version + 1, updated_at = time::now(), text = $text RETURN AFTER",
);
pub const UPDATE_MEMORY_TYPE: Stmt = Stmt::new(
    "entities.update_memory_type",
    "UPDATE $id SET version = version + 1, updated_at = time::now(), type = $type RETURN AFTER",
);
pub const UPDATE_MEMORY_TEXT_TYPE: Stmt = Stmt::new(
    "entities.update_memory_text_type",
    "UPDATE $id SET version = version + 1, updated_at = time::now(), text = $text, type = $type RETURN AFTER",
);

pub const STALE_OBSERVATIONS: Stmt = Stmt::new(
    "entities.stale_observations",
    r#"UPDATE memory SET status = "stale" WHERE subject = $subject AND type = "observation""#,
);

pub const DELETE_ENTITY: Stmt = Stmt::new(
    "entities.delete_entity",
    "BEGIN TRANSACTION; DELETE memory WHERE subject = $id; \
     DELETE relates_to WHERE in = $id OR out = $id; DELETE $id; COMMIT TRANSACTION;",
);

pub const REASSIGN_MEMORIES: Stmt =
    Stmt::new("entities.reassign_memories", "UPDATE memory SET subject = $winner WHERE subject = $loser");

pub const EDGES_OUT: Stmt = Stmt::new("entities.edges_out", "SELECT * FROM relates_to WHERE in = $id");
pub const EDGES_IN: Stmt = Stmt::new("entities.edges_in", "SELECT * FROM relates_to WHERE out = $id");
pub const DELETE_EDGES: Stmt = Stmt::new("entities.delete_edges", "DELETE relates_to WHERE in = $id OR out = $id");

pub const SET_ALIASES: Stmt = Stmt::new(
    "entities.set_aliases",
    "UPDATE $id SET aliases = $aliases, updated_at = time::now() RETURN AFTER",
);

pub const MEMORIES_OF: Stmt =
    Stmt::new("entities.memories_of", "SELECT * FROM memory WHERE subject = $id ORDER BY created_at DESC");

pub const EDGES_AMONG: Stmt = Stmt::new(
    "entities.edges_among",
    "SELECT * FROM relates_to WHERE $ids CONTAINS in AND $ids CONTAINS out",
);

pub const SET_SUMMARY: Stmt = Stmt::new(
    "entities.set_summary",
    "UPDATE $id SET summary = $summary, updated_at = time::now() RETURN AFTER",
);

// consolidate.rs
pub const RAW_MEMORIES: Stmt = Stmt::new(
    "entities.raw_memories",
    r#"SELECT * FROM memory WHERE subject = $id AND type IN ["world","experience"] ORDER BY created_at"#,
);
pub const OBSERVATION_OF: Stmt = Stmt::new(
    "entities.observation_of",
    r#"SELECT * FROM memory WHERE subject = $id AND type = "observation" LIMIT 1"#,
);
pub const CONSOLIDATE_CREATE: Stmt = Stmt::new(
    "entities.consolidate_create",
    r#"BEGIN TRANSACTION;
            LET $cur = (SELECT VALUE id FROM memory WHERE subject = $subject AND type = "observation" LIMIT 1);
            LET $row = IF array::len($cur) > 0 { [] } ELSE {
                (CREATE $obs_id SET owner = $owner, vault = $vault, subject = $subject, text = $text,
                    type = "observation", version = 1, proof_count = $proof_count, status = "fresh",
                    source_memories = $source_memories, updated_at = time::now() RETURN AFTER)
            };
            RETURN $row;
            COMMIT TRANSACTION;"#,
);
pub const CONSOLIDATE_UPDATE: Stmt = Stmt::new(
    "entities.consolidate_update",
    "UPDATE $id SET text = $text, version = version + 1, proof_count = $proof_count, \
             status = \"fresh\", source_memories = $source_memories, updated_at = time::now() \
             WHERE version = $expected_version RETURN AFTER",
);

// extract.rs
pub const UPSERT_APP_SETTINGS: Stmt =
    Stmt::new("entities.upsert_app_settings", "UPSERT $id SET owner = $owner RETURN AFTER");

pub const ALL: &[&Stmt] = &[
    &EMAILS_FOR,
    &MERGE_ALIASES,
    &WRITE_OBSERVATION,
    &WRITE_FACT,
    &RELATE_RETURNING,
    &RELATE,
    &FIND_RELATION,
    &DELETE_RECORD,
    &UPDATE_MEMORY_TEXT,
    &UPDATE_MEMORY_TYPE,
    &UPDATE_MEMORY_TEXT_TYPE,
    &STALE_OBSERVATIONS,
    &DELETE_ENTITY,
    &REASSIGN_MEMORIES,
    &EDGES_OUT,
    &EDGES_IN,
    &DELETE_EDGES,
    &SET_ALIASES,
    &MEMORIES_OF,
    &EDGES_AMONG,
    &SET_SUMMARY,
    &RAW_MEMORIES,
    &OBSERVATION_OF,
    &CONSOLIDATE_CREATE,
    &CONSOLIDATE_UPDATE,
    &UPSERT_APP_SETTINGS,
];

// The helpers below build SQL at runtime, so every_query_executes cannot
// cover them. `table` is always a kind_table()-validated entity table name
// (table names cannot be bound parameters); `set` is a join of fixed
// "field = $field" fragments chosen by the caller.

/// `SELECT * FROM {table} WHERE vault = $vault`, optionally ordered by name.
pub fn select_by_vault<'a>(db: &'a Db, table: &str, ordered: bool) -> Q<'a> {
    if ordered {
        dynamic(db, "entities.list_by_vault", format!("SELECT * FROM {table} WHERE vault = $vault ORDER BY name"))
    } else {
        dynamic(db, "entities.select_by_vault", format!("SELECT * FROM {table} WHERE vault = $vault"))
    }
}

pub fn create_entity<'a>(db: &'a Db, table: &str) -> Q<'a> {
    dynamic(
        db,
        "entities.create_entity",
        format!("CREATE {table} SET owner = $owner, vault = $vault, name = $name, aliases = $aliases RETURN AFTER"),
    )
}

/// Entity edit where only the passed fields change (7 combinations).
pub fn update_entity<'a>(db: &'a Db, set: &str) -> Q<'a> {
    dynamic(db, "entities.update_entity", format!("UPDATE $id SET {set} RETURN AFTER"))
}
