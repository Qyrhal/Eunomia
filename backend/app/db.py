"""SurrealDB connection + schema bootstrap.

Uses a single pooled async connection for the lifetime of the app (opened in
``app.main``'s lifespan), reused by the ``get_db`` FastAPI dependency.
"""

from collections.abc import AsyncIterator
from typing import Any

from surrealdb import AsyncSurreal

from app.config import settings

SCHEMA_STATEMENTS = [
    # accounts
    "DEFINE TABLE IF NOT EXISTS user SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS email ON user TYPE string;",
    "DEFINE FIELD IF NOT EXISTS password_hash ON user TYPE string;",
    "DEFINE FIELD IF NOT EXISTS api_token_hash ON user TYPE option<string>;",
    "DEFINE FIELD IF NOT EXISTS onboarded_at ON user TYPE option<datetime>;",
    "DEFINE FIELD IF NOT EXISTS created_at ON user TYPE datetime DEFAULT time::now();",
    "DEFINE INDEX IF NOT EXISTS user_email_unique ON user FIELDS email UNIQUE;",
    # per-user app settings (id = app_settings:⟨user_id⟩, one row per user)
    "DEFINE TABLE IF NOT EXISTS app_settings SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS owner ON app_settings TYPE record<user>;",
    'DEFINE FIELD IF NOT EXISTS embedding_model ON app_settings TYPE string DEFAULT "text-embedding-3-small";',
    "DEFINE FIELD IF NOT EXISTS sync_intervals ON app_settings FLEXIBLE TYPE object DEFAULT {};",
    "DEFINE FIELD IF NOT EXISTS theme ON app_settings FLEXIBLE TYPE object DEFAULT {};",
    "DEFINE FIELD IF NOT EXISTS updated_at ON app_settings TYPE datetime DEFAULT time::now();",
    "DEFINE INDEX IF NOT EXISTS app_settings_owner_unique ON app_settings FIELDS owner UNIQUE;",
    # connector credentials (owned, not global)
    "DEFINE TABLE IF NOT EXISTS connector SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS owner ON connector TYPE record<user>;",
    "DEFINE FIELD IF NOT EXISTS kind ON connector TYPE string "
    'ASSERT $value IN ["up_bank","pocketai","open_connector","demo"];',
    "DEFINE FIELD IF NOT EXISTS enabled ON connector TYPE bool DEFAULT false;",
    "DEFINE FIELD IF NOT EXISTS config ON connector FLEXIBLE TYPE object DEFAULT {};",
    'DEFINE FIELD IF NOT EXISTS credentials_encrypted ON connector TYPE string DEFAULT "";',
    "DEFINE FIELD IF NOT EXISTS updated_at ON connector TYPE datetime DEFAULT time::now();",
    "DEFINE INDEX IF NOT EXISTS connector_owner_kind_unique ON connector FIELDS owner, kind UNIQUE;",
    # sync health -- the "what's next / what's not" data source (owned)
    "DEFINE TABLE IF NOT EXISTS sync_status SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS owner ON sync_status TYPE record<user>;",
    'DEFINE FIELD IF NOT EXISTS cursor ON sync_status TYPE string DEFAULT "";',
    "DEFINE FIELD IF NOT EXISTS last_run ON sync_status TYPE option<datetime>;",
    "DEFINE FIELD IF NOT EXISTS last_ok ON sync_status TYPE option<datetime>;",
    'DEFINE FIELD IF NOT EXISTS last_error ON sync_status TYPE string DEFAULT "";',
    "DEFINE FIELD IF NOT EXISTS consecutive_failures ON sync_status TYPE int DEFAULT 0;",
    "DEFINE FIELD IF NOT EXISTS last_report ON sync_status FLEXIBLE TYPE object DEFAULT {};",
    # canonical record store (owned)
    "DEFINE TABLE IF NOT EXISTS cache_record SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS owner ON cache_record TYPE record<user>;",
    "DEFINE FIELD IF NOT EXISTS source ON cache_record TYPE string;",
    "DEFINE FIELD IF NOT EXISTS type ON cache_record TYPE string;",
    "DEFINE FIELD IF NOT EXISTS external_id ON cache_record TYPE string;",
    'DEFINE FIELD IF NOT EXISTS title ON cache_record TYPE string DEFAULT "";',
    'DEFINE FIELD IF NOT EXISTS body_text ON cache_record TYPE string DEFAULT "";',
    "DEFINE FIELD IF NOT EXISTS occurred_at ON cache_record TYPE option<datetime>;",
    'DEFINE FIELD IF NOT EXISTS url ON cache_record TYPE string DEFAULT "";',
    "DEFINE FIELD IF NOT EXISTS payload ON cache_record FLEXIBLE TYPE object DEFAULT {};",
    "DEFINE FIELD IF NOT EXISTS content_hash ON cache_record TYPE string;",
    "DEFINE FIELD IF NOT EXISTS ingested_at ON cache_record TYPE datetime;",
    "DEFINE FIELD IF NOT EXISTS updated_at ON cache_record TYPE datetime;",
    "DEFINE FIELD IF NOT EXISTS deleted ON cache_record TYPE bool DEFAULT false;",
    "DEFINE FIELD IF NOT EXISTS embedding ON cache_record TYPE option<array<float>>;",
    "DEFINE INDEX IF NOT EXISTS cache_record_source_type ON cache_record FIELDS source, type;",
    "DEFINE INDEX IF NOT EXISTS cache_record_occurred ON cache_record FIELDS occurred_at;",
    "DEFINE INDEX IF NOT EXISTS cache_record_embedding_idx ON cache_record FIELDS embedding "
    "MTREE DIMENSION 1536 DIST COSINE TYPE F32;",
    "DEFINE ANALYZER IF NOT EXISTS cache_text_analyzer TOKENIZERS blank,class FILTERS lowercase, snowball(english);",
    "DEFINE INDEX IF NOT EXISTS cache_record_fts_idx ON cache_record FIELDS title, body_text "
    "SEARCH ANALYZER cache_text_analyzer BM25 HIGHLIGHTS;",
    # typed graph edge, replaces CacheLink's string source_id/target_id pair
    "DEFINE TABLE IF NOT EXISTS linked_to SCHEMAFULL TYPE RELATION FROM cache_record TO cache_record;",
    "DEFINE FIELD IF NOT EXISTS rel ON linked_to TYPE string;",
    'DEFINE FIELD IF NOT EXISTS origin ON linked_to TYPE string ASSERT $value IN ["sync","agent"];',
    "DEFINE FIELD IF NOT EXISTS created_at ON linked_to TYPE datetime DEFAULT time::now();",
    "DEFINE INDEX IF NOT EXISTS linked_to_unique ON linked_to FIELDS in, out, rel UNIQUE;",
    # embedding memo cache
    "DEFINE TABLE IF NOT EXISTS embed_cache SCHEMAFULL;",
    "DEFINE FIELD IF NOT EXISTS text_hmac ON embed_cache TYPE string;",
    "DEFINE FIELD IF NOT EXISTS vector ON embed_cache TYPE array<float>;",
    "DEFINE FIELD IF NOT EXISTS created_at ON embed_cache TYPE datetime DEFAULT time::now();",
    "DEFINE INDEX IF NOT EXISTS embed_cache_hmac_unique ON embed_cache FIELDS text_hmac UNIQUE;",
]

_db: Any = None


async def connect_db() -> Any:
    """Open the single pooled connection used for the app's lifetime."""
    global _db
    db = AsyncSurreal(settings.SURREAL_URL)
    await db.connect()
    await db.signin({"username": settings.SURREAL_USER, "password": settings.SURREAL_PASS})
    await db.use(settings.SURREAL_NS, settings.SURREAL_DB)
    _db = db
    return db


async def close_db() -> None:
    global _db
    if _db is not None:
        await _db.close()
        _db = None


async def ensure_schema(db: AsyncSurreal) -> None:
    """Run every DEFINE statement from the schema. Idempotent (IF NOT EXISTS)."""
    for statement in SCHEMA_STATEMENTS:
        await db.query(statement)


async def get_db() -> AsyncIterator[AsyncSurreal]:
    """FastAPI dependency yielding the pooled SurrealDB connection."""
    if _db is None:
        raise RuntimeError("SurrealDB connection not initialized; check app lifespan.")
    yield _db


def db() -> AsyncSurreal:
    """Direct accessor for the pooled connection, for non-request call sites
    (``cache``/``embeddings`` services) that aren't FastAPI route handlers and
    so can't use the ``get_db`` dependency."""
    if _db is None:
        raise RuntimeError("SurrealDB connection not initialized; check app lifespan.")
    return _db
