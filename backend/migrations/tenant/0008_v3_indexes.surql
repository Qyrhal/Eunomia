-- SurrealDB 3.x indexes. 0001 is 3.x-valid but no longer defines the vector and full-text
-- indexes (2.x MTREE and SEARCH ANALYZER are gone). This defines them explicitly instead of
-- trusting the `surreal v2 export --v3` conversion: an upgraded install arrives with whatever the
-- converter produced, so drop it by name first (a no-op on a fresh database).
REMOVE INDEX IF EXISTS cache_record_embedding_idx ON cache_record;
REMOVE INDEX IF EXISTS cache_record_fts_idx ON cache_record;
-- scripts/upgrade-surreal-v3.sh splits the 2.x two-field index into these two.
REMOVE INDEX IF EXISTS cache_record_fts_idx_title ON cache_record;
REMOVE INDEX IF EXISTS cache_record_fts_idx_body_text ON cache_record;
REMOVE INDEX IF EXISTS memory_text_fts_idx ON memory;
-- HNSW for semantic recall, queried with `<|K,EF|>`.
DEFINE INDEX OVERWRITE cache_record_embedding_idx ON cache_record FIELDS embedding HNSW DIMENSION 1536 DIST COSINE TYPE F32 CONCURRENTLY;
-- One FULLTEXT index per field: a two-field index only resolves the first field.
DEFINE INDEX OVERWRITE cache_record_title_fts ON cache_record FIELDS title FULLTEXT ANALYZER cache_text_analyzer BM25 HIGHLIGHTS CONCURRENTLY;
DEFINE INDEX OVERWRITE cache_record_body_fts ON cache_record FIELDS body_text FULLTEXT ANALYZER cache_text_analyzer BM25 HIGHLIGHTS CONCURRENTLY;
DEFINE INDEX OVERWRITE memory_text_fts_idx ON memory FIELDS text FULLTEXT ANALYZER cache_text_analyzer BM25 CONCURRENTLY;
-- The exact-scan fallback for semantic search filters cache_record by owner.
DEFINE INDEX OVERWRITE cache_record_owner_idx ON cache_record FIELDS owner;
