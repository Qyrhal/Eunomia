//! SurrealDB connection + schema bootstrap. Ported statement-for-statement
//! from the Python `app/db.py` -- schema must stay identical across both
//! implementations during the migration.

use surrealdb::engine::remote::ws::{Client, Ws};
use surrealdb::opt::auth::Root;
use surrealdb::Surreal;

use crate::config::Settings;

pub type Db = Surreal<Client>;

pub const SCHEMA_STATEMENTS: &[&str] = &[
    "DEFINE TABLE IF NOT EXISTS user SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS email ON user TYPE string;",
    "DEFINE FIELD IF NOT EXISTS password_hash ON user TYPE string;",
    "DEFINE FIELD IF NOT EXISTS api_token_hash ON user TYPE option<string>;",
    "DEFINE FIELD IF NOT EXISTS onboarded_at ON user TYPE option<datetime>;",
    "DEFINE FIELD IF NOT EXISTS created_at ON user TYPE datetime DEFAULT time::now();",
    "DEFINE INDEX IF NOT EXISTS user_email_unique ON user FIELDS email UNIQUE;",
    "DEFINE TABLE IF NOT EXISTS api_token SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS owner ON api_token TYPE record<user>;",
    "DEFINE FIELD IF NOT EXISTS name ON api_token TYPE string DEFAULT \"API token\";",
    "DEFINE FIELD IF NOT EXISTS token_hash ON api_token TYPE string;",
    "DEFINE FIELD IF NOT EXISTS created_at ON api_token TYPE datetime DEFAULT time::now();",
    "DEFINE FIELD IF NOT EXISTS last_used_at ON api_token TYPE option<datetime>;",
    "DEFINE INDEX IF NOT EXISTS api_token_hash_unique ON api_token FIELDS token_hash UNIQUE;",
    "DEFINE INDEX IF NOT EXISTS api_token_owner_idx ON api_token FIELDS owner;",
    "DEFINE TABLE IF NOT EXISTS session SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS owner ON session TYPE record<user>;",
    "DEFINE FIELD IF NOT EXISTS sid ON session TYPE string;",
    "DEFINE FIELD IF NOT EXISTS user_agent ON session TYPE string DEFAULT \"\";",
    "DEFINE FIELD IF NOT EXISTS created_at ON session TYPE datetime DEFAULT time::now();",
    "DEFINE FIELD IF NOT EXISTS last_seen_at ON session TYPE datetime DEFAULT time::now();",
    "DEFINE FIELD IF NOT EXISTS revoked ON session TYPE bool DEFAULT false;",
    "DEFINE INDEX IF NOT EXISTS session_sid_unique ON session FIELDS sid UNIQUE;",
    "DEFINE INDEX IF NOT EXISTS session_owner_idx ON session FIELDS owner;",
    "DEFINE TABLE IF NOT EXISTS app_settings SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS owner ON app_settings TYPE record<user>;",
    "DEFINE FIELD IF NOT EXISTS embedding_model ON app_settings TYPE string DEFAULT \"text-embedding-3-small\";",
    "DEFINE FIELD IF NOT EXISTS chat_model ON app_settings TYPE string DEFAULT \"\";",
    "DEFINE FIELD IF NOT EXISTS sync_intervals ON app_settings FLEXIBLE TYPE object DEFAULT {};",
    "DEFINE FIELD IF NOT EXISTS theme ON app_settings FLEXIBLE TYPE object DEFAULT {};",
    "DEFINE FIELD IF NOT EXISTS openai_api_key_encrypted ON app_settings TYPE string DEFAULT \"\";",
    "DEFINE FIELD IF NOT EXISTS openai_base_url ON app_settings TYPE string DEFAULT \"https://api.openai.com/v1\";",
    "DEFINE FIELD IF NOT EXISTS observations_mission ON app_settings TYPE string DEFAULT \"Observations are stable facts about people and relationships: preferences, skills, roles, recurring patterns, and how they change over time. Ignore ephemeral or one-off details.\";",
    "DEFINE FIELD IF NOT EXISTS memory_skill ON app_settings TYPE string DEFAULT \"\";",
    "DEFINE FIELD IF NOT EXISTS updated_at ON app_settings TYPE datetime DEFAULT time::now();",
    "DEFINE INDEX IF NOT EXISTS app_settings_owner_unique ON app_settings FIELDS owner UNIQUE;",
    "DEFINE TABLE IF NOT EXISTS vault SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS name ON vault TYPE string;",
    "DEFINE FIELD IF NOT EXISTS kind ON vault TYPE string DEFAULT \"personal\" ASSERT $value IN [\"personal\",\"org\"];",
    "DEFINE FIELD IF NOT EXISTS created_at ON vault TYPE datetime DEFAULT time::now();",
    "DEFINE TABLE IF NOT EXISTS vault_member SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS vault ON vault_member TYPE record<vault>;",
    "DEFINE FIELD IF NOT EXISTS user ON vault_member TYPE record<user>;",
    "DEFINE FIELD IF NOT EXISTS role ON vault_member TYPE string DEFAULT \"member\" ASSERT $value IN [\"owner\",\"member\"];",
    "DEFINE FIELD IF NOT EXISTS status ON vault_member TYPE string DEFAULT \"active\" ASSERT $value IN [\"pending\",\"active\"];",
    "DEFINE FIELD IF NOT EXISTS created_at ON vault_member TYPE datetime DEFAULT time::now();",
    "DEFINE INDEX IF NOT EXISTS vault_member_unique ON vault_member FIELDS vault, user UNIQUE;",
    "DEFINE INDEX IF NOT EXISTS vault_member_user_idx ON vault_member FIELDS user;",
    "DEFINE TABLE IF NOT EXISTS connector SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS owner ON connector TYPE record<user>;",
    "DEFINE FIELD IF NOT EXISTS kind ON connector TYPE string ASSERT $value IN [\"up_bank\",\"pocketai\",\"open_connector\",\"github\",\"slack\",\"notion\",\"linear\",\"gmail\",\"google_calendar\",\"discord\",\"spotify\",\"todoist\",\"stripe\",\"demo\"];",
    "DEFINE FIELD IF NOT EXISTS enabled ON connector TYPE bool DEFAULT false;",
    "DEFINE FIELD IF NOT EXISTS config ON connector FLEXIBLE TYPE object DEFAULT {};",
    "DEFINE FIELD IF NOT EXISTS credentials_encrypted ON connector TYPE string DEFAULT \"\";",
    "DEFINE FIELD IF NOT EXISTS updated_at ON connector TYPE datetime DEFAULT time::now();",
    "DEFINE INDEX IF NOT EXISTS connector_owner_kind_unique ON connector FIELDS owner, kind UNIQUE;",
    "DEFINE TABLE IF NOT EXISTS sync_status SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS owner ON sync_status TYPE record<user>;",
    "DEFINE FIELD IF NOT EXISTS cursor ON sync_status TYPE string DEFAULT \"\";",
    "DEFINE FIELD IF NOT EXISTS last_run ON sync_status TYPE option<datetime>;",
    "DEFINE FIELD IF NOT EXISTS last_ok ON sync_status TYPE option<datetime>;",
    "DEFINE FIELD IF NOT EXISTS last_error ON sync_status TYPE string DEFAULT \"\";",
    "DEFINE FIELD IF NOT EXISTS consecutive_failures ON sync_status TYPE int DEFAULT 0;",
    "DEFINE FIELD IF NOT EXISTS last_report ON sync_status FLEXIBLE TYPE object DEFAULT {};",
    "DEFINE TABLE IF NOT EXISTS cache_record SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS owner ON cache_record TYPE record<user>;",
    "DEFINE FIELD IF NOT EXISTS source ON cache_record TYPE string;",
    "DEFINE FIELD IF NOT EXISTS type ON cache_record TYPE string;",
    "DEFINE FIELD IF NOT EXISTS external_id ON cache_record TYPE string;",
    "DEFINE FIELD IF NOT EXISTS title ON cache_record TYPE string DEFAULT \"\";",
    "DEFINE FIELD IF NOT EXISTS body_text ON cache_record TYPE string DEFAULT \"\";",
    "DEFINE FIELD IF NOT EXISTS occurred_at ON cache_record TYPE option<datetime>;",
    "DEFINE FIELD IF NOT EXISTS url ON cache_record TYPE string DEFAULT \"\";",
    "DEFINE FIELD IF NOT EXISTS payload ON cache_record FLEXIBLE TYPE object DEFAULT {};",
    "DEFINE FIELD IF NOT EXISTS content_hash ON cache_record TYPE string;",
    "DEFINE FIELD IF NOT EXISTS ingested_at ON cache_record TYPE datetime;",
    "DEFINE FIELD IF NOT EXISTS updated_at ON cache_record TYPE datetime;",
    "DEFINE FIELD IF NOT EXISTS deleted ON cache_record TYPE bool DEFAULT false;",
    "DEFINE FIELD IF NOT EXISTS embedding ON cache_record TYPE option<array<float>>;",
    "DEFINE INDEX IF NOT EXISTS cache_record_source_type ON cache_record FIELDS source, type;",
    "DEFINE INDEX IF NOT EXISTS cache_record_occurred ON cache_record FIELDS occurred_at;",
    "DEFINE INDEX IF NOT EXISTS cache_record_embedding_idx ON cache_record FIELDS embedding MTREE DIMENSION 1536 DIST COSINE TYPE F32;",
    "DEFINE ANALYZER IF NOT EXISTS cache_text_analyzer TOKENIZERS blank,class FILTERS lowercase, snowball(english);",
    "DEFINE INDEX IF NOT EXISTS cache_record_fts_idx ON cache_record FIELDS title, body_text SEARCH ANALYZER cache_text_analyzer BM25 HIGHLIGHTS;",
    // `@N@` only resolves against a composite search index's FIRST field
    // (title above), so body_text gets its own index.
    "DEFINE INDEX IF NOT EXISTS cache_record_body_fts_idx ON cache_record FIELDS body_text SEARCH ANALYZER cache_text_analyzer BM25;",
    // Everything Pocket returns for a recording, verbatim (`raw`), plus the
    // full speaker-labelled transcript -- the cache holds it chunked.
    "DEFINE TABLE IF NOT EXISTS pocket_recording SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS owner ON pocket_recording TYPE record<user>;",
    "DEFINE FIELD IF NOT EXISTS recording_id ON pocket_recording TYPE string;",
    "DEFINE FIELD IF NOT EXISTS title ON pocket_recording TYPE string DEFAULT \"\";",
    "DEFINE FIELD IF NOT EXISTS recorded_at ON pocket_recording TYPE option<datetime>;",
    "DEFINE FIELD IF NOT EXISTS duration_seconds ON pocket_recording TYPE number DEFAULT 0;",
    "DEFINE FIELD IF NOT EXISTS tags ON pocket_recording TYPE array<string> DEFAULT [];",
    "DEFINE FIELD IF NOT EXISTS speakers ON pocket_recording TYPE array<string> DEFAULT [];",
    "DEFINE FIELD IF NOT EXISTS summary ON pocket_recording TYPE string DEFAULT \"\";",
    "DEFINE FIELD IF NOT EXISTS action_items ON pocket_recording TYPE array<string> DEFAULT [];",
    "DEFINE FIELD IF NOT EXISTS transcript ON pocket_recording TYPE string DEFAULT \"\";",
    "DEFINE FIELD IF NOT EXISTS raw ON pocket_recording FLEXIBLE TYPE object DEFAULT {};",
    "DEFINE FIELD IF NOT EXISTS synced_at ON pocket_recording TYPE datetime DEFAULT time::now();",
    "DEFINE INDEX IF NOT EXISTS pocket_recording_owner_idx ON pocket_recording FIELDS owner;",
    // Pocket syncs before pocket_recording existed stored no transcript or
    // summary: restart those cursors so the next sync refetches them. A
    // no-op once the owner has any stored recording.
    "UPDATE sync_status SET cursor = \"\" WHERE cursor != \"\" AND string::ends_with(record::id(id), \":heypocket\") AND count((SELECT id FROM pocket_recording WHERE owner = $parent.owner LIMIT 1)) = 0;",
    "DEFINE TABLE IF NOT EXISTS linked_to SCHEMAFULL TYPE RELATION FROM cache_record TO cache_record;",
    "DEFINE FIELD IF NOT EXISTS rel ON linked_to TYPE string;",
    "DEFINE FIELD IF NOT EXISTS origin ON linked_to TYPE string ASSERT $value IN [\"sync\",\"agent\"];",
    "DEFINE FIELD IF NOT EXISTS created_at ON linked_to TYPE datetime DEFAULT time::now();",
    "DEFINE INDEX IF NOT EXISTS linked_to_unique ON linked_to FIELDS in, out, rel UNIQUE;",
    "DEFINE TABLE IF NOT EXISTS person SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS owner ON person TYPE record<user>;",
    "DEFINE FIELD IF NOT EXISTS vault ON person TYPE record<vault>;",
    "DEFINE FIELD IF NOT EXISTS name ON person TYPE string;",
    "DEFINE FIELD IF NOT EXISTS aliases ON person TYPE array<string> DEFAULT [];",
    "DEFINE FIELD IF NOT EXISTS summary ON person TYPE string DEFAULT \"\";",
    "DEFINE FIELD IF NOT EXISTS created_at ON person TYPE datetime DEFAULT time::now();",
    "DEFINE FIELD IF NOT EXISTS updated_at ON person TYPE datetime DEFAULT time::now();",
    "DEFINE INDEX IF NOT EXISTS person_vault_name_idx ON person FIELDS vault, name;",
    "DEFINE FIELD IF NOT EXISTS name_key ON person VALUE string::lowercase(name);",
    "DEFINE FIELD IF NOT EXISTS alias_keys ON person VALUE (aliases ?? []).map(|$a| string::lowercase($a));",
    "DEFINE INDEX IF NOT EXISTS person_alias_keys_idx ON person FIELDS alias_keys;",
    "DEFINE TABLE IF NOT EXISTS organisation SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS owner ON organisation TYPE record<user>;",
    "DEFINE FIELD IF NOT EXISTS vault ON organisation TYPE record<vault>;",
    "DEFINE FIELD IF NOT EXISTS name ON organisation TYPE string;",
    "DEFINE FIELD IF NOT EXISTS aliases ON organisation TYPE array<string> DEFAULT [];",
    "DEFINE FIELD IF NOT EXISTS summary ON organisation TYPE string DEFAULT \"\";",
    "DEFINE FIELD IF NOT EXISTS created_at ON organisation TYPE datetime DEFAULT time::now();",
    "DEFINE FIELD IF NOT EXISTS updated_at ON organisation TYPE datetime DEFAULT time::now();",
    "DEFINE INDEX IF NOT EXISTS organisation_vault_name_idx ON organisation FIELDS vault, name;",
    "DEFINE FIELD IF NOT EXISTS name_key ON organisation VALUE string::lowercase(name);",
    "DEFINE FIELD IF NOT EXISTS alias_keys ON organisation VALUE (aliases ?? []).map(|$a| string::lowercase($a));",
    "DEFINE INDEX IF NOT EXISTS organisation_alias_keys_idx ON organisation FIELDS alias_keys;",
    "DEFINE TABLE IF NOT EXISTS location SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS owner ON location TYPE record<user>;",
    "DEFINE FIELD IF NOT EXISTS vault ON location TYPE record<vault>;",
    "DEFINE FIELD IF NOT EXISTS name ON location TYPE string;",
    "DEFINE FIELD IF NOT EXISTS aliases ON location TYPE array<string> DEFAULT [];",
    "DEFINE FIELD IF NOT EXISTS summary ON location TYPE string DEFAULT \"\";",
    "DEFINE FIELD IF NOT EXISTS created_at ON location TYPE datetime DEFAULT time::now();",
    "DEFINE FIELD IF NOT EXISTS updated_at ON location TYPE datetime DEFAULT time::now();",
    "DEFINE INDEX IF NOT EXISTS location_vault_name_idx ON location FIELDS vault, name;",
    "DEFINE FIELD IF NOT EXISTS name_key ON location VALUE string::lowercase(name);",
    "DEFINE FIELD IF NOT EXISTS alias_keys ON location VALUE (aliases ?? []).map(|$a| string::lowercase($a));",
    "DEFINE INDEX IF NOT EXISTS location_alias_keys_idx ON location FIELDS alias_keys;",
    "DEFINE TABLE IF NOT EXISTS repository SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS owner ON repository TYPE record<user>;",
    "DEFINE FIELD IF NOT EXISTS vault ON repository TYPE record<vault>;",
    "DEFINE FIELD IF NOT EXISTS name ON repository TYPE string;",
    "DEFINE FIELD IF NOT EXISTS aliases ON repository TYPE array<string> DEFAULT [];",
    "DEFINE FIELD IF NOT EXISTS summary ON repository TYPE string DEFAULT \"\";",
    "DEFINE FIELD IF NOT EXISTS created_at ON repository TYPE datetime DEFAULT time::now();",
    "DEFINE FIELD IF NOT EXISTS updated_at ON repository TYPE datetime DEFAULT time::now();",
    "DEFINE INDEX IF NOT EXISTS repository_vault_name_idx ON repository FIELDS vault, name;",
    "DEFINE FIELD IF NOT EXISTS name_key ON repository VALUE string::lowercase(name);",
    "DEFINE FIELD IF NOT EXISTS alias_keys ON repository VALUE (aliases ?? []).map(|$a| string::lowercase($a));",
    "DEFINE INDEX IF NOT EXISTS repository_alias_keys_idx ON repository FIELDS alias_keys;",
    "DEFINE TABLE IF NOT EXISTS file SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS owner ON file TYPE record<user>;",
    "DEFINE FIELD IF NOT EXISTS vault ON file TYPE record<vault>;",
    "DEFINE FIELD IF NOT EXISTS name ON file TYPE string;",
    "DEFINE FIELD IF NOT EXISTS aliases ON file TYPE array<string> DEFAULT [];",
    "DEFINE FIELD IF NOT EXISTS summary ON file TYPE string DEFAULT \"\";",
    "DEFINE FIELD IF NOT EXISTS created_at ON file TYPE datetime DEFAULT time::now();",
    "DEFINE FIELD IF NOT EXISTS updated_at ON file TYPE datetime DEFAULT time::now();",
    "DEFINE INDEX IF NOT EXISTS file_vault_name_idx ON file FIELDS vault, name;",
    "DEFINE FIELD IF NOT EXISTS name_key ON file VALUE string::lowercase(name);",
    "DEFINE FIELD IF NOT EXISTS alias_keys ON file VALUE (aliases ?? []).map(|$a| string::lowercase($a));",
    "DEFINE INDEX IF NOT EXISTS file_alias_keys_idx ON file FIELDS alias_keys;",
    "DEFINE TABLE IF NOT EXISTS symbol SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS owner ON symbol TYPE record<user>;",
    "DEFINE FIELD IF NOT EXISTS vault ON symbol TYPE record<vault>;",
    "DEFINE FIELD IF NOT EXISTS name ON symbol TYPE string;",
    "DEFINE FIELD IF NOT EXISTS aliases ON symbol TYPE array<string> DEFAULT [];",
    "DEFINE FIELD IF NOT EXISTS summary ON symbol TYPE string DEFAULT \"\";",
    "DEFINE FIELD IF NOT EXISTS created_at ON symbol TYPE datetime DEFAULT time::now();",
    "DEFINE FIELD IF NOT EXISTS updated_at ON symbol TYPE datetime DEFAULT time::now();",
    "DEFINE INDEX IF NOT EXISTS symbol_vault_name_idx ON symbol FIELDS vault, name;",
    "DEFINE FIELD IF NOT EXISTS name_key ON symbol VALUE string::lowercase(name);",
    "DEFINE FIELD IF NOT EXISTS alias_keys ON symbol VALUE (aliases ?? []).map(|$a| string::lowercase($a));",
    "DEFINE INDEX IF NOT EXISTS symbol_alias_keys_idx ON symbol FIELDS alias_keys;",
    "DEFINE TABLE IF NOT EXISTS memory SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS owner ON memory TYPE record<user>;",
    "DEFINE FIELD IF NOT EXISTS vault ON memory TYPE record<vault>;",
    "DEFINE FIELD IF NOT EXISTS subject ON memory TYPE record<person | organisation | location | repository | file | symbol>;",
    "DEFINE FIELD IF NOT EXISTS text ON memory TYPE string;",
    "DEFINE FIELD IF NOT EXISTS source ON memory TYPE option<record<cache_record>>;",
    "DEFINE FIELD IF NOT EXISTS created_at ON memory TYPE datetime DEFAULT time::now();",
    "DEFINE FIELD IF NOT EXISTS type ON memory TYPE string DEFAULT \"world\" ASSERT $value IN [\"world\",\"experience\",\"observation\"];",
    "DEFINE FIELD IF NOT EXISTS proof_count ON memory TYPE int DEFAULT 1;",
    // fresh/stale: an observation; superseded: a raw fact a later one contradicted.
    "DEFINE FIELD OVERWRITE status ON memory TYPE option<string> ASSERT $value IN [\"fresh\",\"stale\",\"superseded\"];",
    "DEFINE FIELD IF NOT EXISTS source_memories ON memory TYPE option<array<record<memory>>>;",
    "DEFINE FIELD IF NOT EXISTS updated_at ON memory TYPE datetime DEFAULT time::now();",
    "DEFINE FIELD IF NOT EXISTS version ON memory TYPE int DEFAULT 1;",
    // set only on observations, so the UNIQUE index (NONE values are not
    // indexed) allows exactly one observation per subject
    "DEFINE FIELD IF NOT EXISTS obs_subject ON memory VALUE IF type = \"observation\" THEN subject ELSE NONE END;",
    "DEFINE INDEX IF NOT EXISTS memory_subject_idx ON memory FIELDS subject;",
    "DEFINE INDEX IF NOT EXISTS memory_vault_idx ON memory FIELDS vault;",
    "DEFINE INDEX IF NOT EXISTS memory_text_fts_idx ON memory FIELDS text SEARCH ANALYZER cache_text_analyzer BM25;",
    "DEFINE TABLE IF NOT EXISTS relates_to SCHEMAFULL TYPE RELATION FROM person | organisation | location | repository | file | symbol TO person | organisation | location | repository | file | symbol;",
    "DEFINE FIELD IF NOT EXISTS label ON relates_to TYPE string DEFAULT \"\";",
    "DEFINE FIELD IF NOT EXISTS source ON relates_to TYPE option<record<cache_record>>;",
    "DEFINE FIELD IF NOT EXISTS owner ON relates_to TYPE option<record<user>>;",
    "DEFINE FIELD IF NOT EXISTS created_at ON relates_to TYPE datetime DEFAULT time::now();",
    "DEFINE INDEX IF NOT EXISTS relates_to_unique ON relates_to FIELDS in, out, label UNIQUE;",
    "DEFINE TABLE IF NOT EXISTS chat_thread SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS owner ON chat_thread TYPE record<user>;",
    "DEFINE FIELD IF NOT EXISTS title ON chat_thread TYPE string DEFAULT \"New chat\";",
    "DEFINE FIELD IF NOT EXISTS created_at ON chat_thread TYPE datetime DEFAULT time::now();",
    "DEFINE FIELD IF NOT EXISTS updated_at ON chat_thread TYPE datetime DEFAULT time::now();",
    "DEFINE INDEX IF NOT EXISTS chat_thread_owner_idx ON chat_thread FIELDS owner, updated_at;",
    "DEFINE TABLE IF NOT EXISTS chat_message SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS owner ON chat_message TYPE record<user>;",
    "DEFINE FIELD IF NOT EXISTS thread_id ON chat_message TYPE record<chat_thread>;",
    "DEFINE FIELD IF NOT EXISTS role ON chat_message TYPE string ASSERT $value IN [\"user\",\"assistant\",\"tool\"];",
    "DEFINE FIELD IF NOT EXISTS content ON chat_message TYPE string;",
    "DEFINE FIELD IF NOT EXISTS tool_calls ON chat_message FLEXIBLE TYPE option<array<object>>;",
    "DEFINE FIELD IF NOT EXISTS tool_call_id ON chat_message TYPE option<string>;",
    "DEFINE FIELD IF NOT EXISTS created_at ON chat_message TYPE datetime DEFAULT time::now();",
    "DEFINE INDEX IF NOT EXISTS chat_message_owner_idx ON chat_message FIELDS owner, created_at;",
    "DEFINE INDEX IF NOT EXISTS chat_message_thread_idx ON chat_message FIELDS thread_id, created_at;",
    "DEFINE TABLE IF NOT EXISTS audit_log SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS owner ON audit_log TYPE record<user>;",
    "DEFINE FIELD IF NOT EXISTS tool_name ON audit_log TYPE string;",
    "DEFINE FIELD IF NOT EXISTS args_summary ON audit_log TYPE string DEFAULT \"\";",
    "DEFINE FIELD IF NOT EXISTS outcome ON audit_log TYPE string ASSERT $value IN [\"ok\",\"error\"];",
    "DEFINE FIELD IF NOT EXISTS created_at ON audit_log TYPE datetime DEFAULT time::now();",
    "DEFINE INDEX IF NOT EXISTS audit_log_owner_idx ON audit_log FIELDS owner, created_at;",
    "DEFINE TABLE IF NOT EXISTS embed_cache SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS text_hmac ON embed_cache TYPE string;",
    "DEFINE FIELD IF NOT EXISTS vector ON embed_cache TYPE array<float>;",
    "DEFINE FIELD IF NOT EXISTS created_at ON embed_cache TYPE datetime DEFAULT time::now();",
    "DEFINE INDEX IF NOT EXISTS embed_cache_hmac_unique ON embed_cache FIELDS text_hmac UNIQUE;",
];

pub async fn connect(settings: &Settings) -> surrealdb::Result<Db> {
    let db: Db = Surreal::new::<Ws>(
        settings
            .surreal_url
            .trim_start_matches("ws://")
            .trim_start_matches("wss://"),
    )
    .await?;
    db.signin(Root { username: &settings.surreal_user, password: &settings.surreal_pass })
        .await?;
    db.use_ns(&settings.surreal_ns).use_db(&settings.surreal_db).await?;
    Ok(db)
}

/// UNIQUE indexes that existing data may violate: [`ensure_schema`] folds
/// duplicates together first (`entities::service::dedupe`), since `DEFINE
/// INDEX ... UNIQUE` fails while any remain. One entity per (vault,
/// case-insensitive name) per kind; one observation per subject.
pub const UNIQUE_STATEMENTS: &[(&str, &str)] = &[
    ("person", "DEFINE INDEX IF NOT EXISTS person_vault_name_key_unique ON person FIELDS vault, name_key UNIQUE;"),
    ("organisation", "DEFINE INDEX IF NOT EXISTS organisation_vault_name_key_unique ON organisation FIELDS vault, name_key UNIQUE;"),
    ("location", "DEFINE INDEX IF NOT EXISTS location_vault_name_key_unique ON location FIELDS vault, name_key UNIQUE;"),
    ("repository", "DEFINE INDEX IF NOT EXISTS repository_vault_name_key_unique ON repository FIELDS vault, name_key UNIQUE;"),
    ("file", "DEFINE INDEX IF NOT EXISTS file_vault_name_key_unique ON file FIELDS vault, name_key UNIQUE;"),
    ("symbol", "DEFINE INDEX IF NOT EXISTS symbol_vault_name_key_unique ON symbol FIELDS vault, name_key UNIQUE;"),
    ("memory", "DEFINE INDEX IF NOT EXISTS memory_observation_unique ON memory FIELDS obs_subject UNIQUE;"),
];

pub async fn ensure_schema(db: &Db, settings: &Settings) -> surrealdb::Result<()> {
    for statement in SCHEMA_STATEMENTS {
        db.query(*statement).await?;
    }
    for (table, statement) in UNIQUE_STATEMENTS {
        // a failed DEFINE leaves no index behind, so a later start retries it
        if let Err(e) = crate::entities::service::dedupe(db, table).await {
            tracing::warn!("schema: could not fold duplicate {table} rows: {}", e.message);
        }
        // still duplicates (e.g. written mid-fold): stay up without the
        // index, retried on the next start
        if let Err(e) = db.query(*statement).await?.check() {
            tracing::warn!("schema: {table} unique index not defined yet: {e}");
        }
    }
    // Re-defining a field is idempotent in SurrealDB, so this just swaps the
    // DEFAULT baked into SCHEMA_STATEMENTS for the configured OPENAI_BASE_URL
    // (self-hosted/OpenAI-compatible endpoints) without duplicating the field
    // definition above.
    db.query(format!(
        "DEFINE FIELD IF NOT EXISTS openai_base_url ON app_settings TYPE string DEFAULT \"{}\";",
        settings.openai_base_url.replace('"', "\\\"")
    ))
    .await?;
    Ok(())
}
