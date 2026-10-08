-- ------------------------------
-- OPTION
-- ------------------------------

OPTION IMPORT;

-- ------------------------------
-- ANALYZERS
-- ------------------------------

DEFINE ANALYZER cache_text_analyzer TOKENIZERS BLANK,CLASS FILTERS LOWERCASE, SNOWBALL(ENGLISH);

-- ------------------------------
-- TABLE: _migration
-- ------------------------------

DEFINE TABLE _migration TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE applied_at ON _migration TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE checksum ON _migration TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE name ON _migration TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE version ON _migration TYPE int PERMISSIONS FULL;

DEFINE INDEX _migration_version_unique ON _migration FIELDS version UNIQUE;


-- ------------------------------
-- TABLE DATA: _migration
-- ------------------------------

INSERT [ { applied_at: d'2026-10-08T13:40:56.960642Z', checksum: '7227cfd0e1a0f672f6938c4bada0b9d7c720e42ff06ff7d8d93bfd11f231acd2', id: _migration:5ebdzcby45ga9z1vvdkg, name: 'jobs', version: 5 }, { applied_at: d'2026-10-08T13:40:56.919471Z', checksum: '31199597d8ffdb899e26dd741ec3886f6af03467f65710f4c7a1db2bfc3ce5a5', id: _migration:6gitpsh6rj6bjnrxj6ag, name: 'baseline', version: 1 }, { applied_at: d'2026-10-08T13:40:56.930358Z', checksum: 'a366323047deccfaf9eb2aae2588f96f41f15f5116c47d255792eab91c51e512', id: _migration:auenedvo30qpcxxente4, name: 'entity_name_unique', version: 2 }, { applied_at: d'2026-10-08T13:40:56.961895Z', checksum: '8d349adeeb3ab47c2f99d3a444ae931a40367af9db136e59ee8560d4d53c14ac', id: _migration:ge2ikizpg5wo3g4fjsfm, name: 'oauth_code_redeemed', version: 6 }, { applied_at: d'2026-10-08T13:40:56.951018Z', checksum: 'b45c4ac323fd801f9b6e57bb4001a761307370017ed4bebeae4adebc9c397189', id: _migration:j8jexhl0a3t3upkkl1hf, name: 'oauth', version: 4 }, { applied_at: d'2026-10-08T13:40:56.938577Z', checksum: '2942a994ffdaa819f2c5cab28424617242efb86bf9e7c1bb20490db3400a4818', id: _migration:nk4r5yi1o8m4vqw8h5sd, name: 'auth', version: 3 }, { applied_at: d'2026-10-08T13:40:56.966165Z', checksum: '8abcf40a7cfcd7eb31684ddf83ac430c7df41dc1916d0181be2d60575cfd336d', id: _migration:vwmf5hxowwn0adpbtrvu, name: 'capsules', version: 7 } ];

-- ------------------------------
-- TABLE: api_token
-- ------------------------------

DEFINE TABLE api_token TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE created_at ON api_token TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE expires_at ON api_token TYPE none | datetime PERMISSIONS FULL;
DEFINE FIELD OVERWRITE last_used_at ON api_token TYPE none | datetime PERMISSIONS FULL;
DEFINE FIELD OVERWRITE name ON api_token TYPE string DEFAULT 'API token' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner ON api_token TYPE record<user> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE scopes ON api_token TYPE array<string> DEFAULT ['memory:read', 'memory:write', 'vaults:admin', 'connectors'] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE scopes.* ON api_token TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE token_hash ON api_token TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE vault ON api_token TYPE none | record<vault> PERMISSIONS FULL;

DEFINE INDEX api_token_hash_unique ON api_token FIELDS token_hash UNIQUE;
DEFINE INDEX api_token_owner_idx ON api_token FIELDS owner;


-- ------------------------------
-- TABLE DATA: api_token
-- ------------------------------


-- ------------------------------
-- TABLE: app_settings
-- ------------------------------

DEFINE TABLE app_settings TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE embedding_model ON app_settings TYPE string DEFAULT 'text-embedding-3-small' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE memory_skill ON app_settings TYPE string DEFAULT '' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE observations_mission ON app_settings TYPE string DEFAULT 'Observations are stable facts about people and relationships: preferences, skills, roles, recurring patterns, and how they change over time. Ignore ephemeral or one-off details.' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE openai_api_key_encrypted ON app_settings TYPE string DEFAULT '' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE openai_base_url ON app_settings TYPE string DEFAULT 'https://api.openai.com/v1' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner ON app_settings TYPE record<user> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE sync_intervals ON app_settings TYPE object FLEXIBLE DEFAULT {  } PERMISSIONS FULL;
DEFINE FIELD OVERWRITE theme ON app_settings TYPE object FLEXIBLE DEFAULT {  } PERMISSIONS FULL;
DEFINE FIELD OVERWRITE updated_at ON app_settings TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE INDEX app_settings_owner_unique ON app_settings FIELDS owner UNIQUE;


-- ------------------------------
-- TABLE DATA: app_settings
-- ------------------------------


-- ------------------------------
-- TABLE: audit_event
-- ------------------------------

DEFINE TABLE audit_event TYPE NORMAL SCHEMAFULL PERMISSIONS FOR select, create FULL, FOR update, delete NONE;

DEFINE FIELD OVERWRITE action ON audit_event TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE actor_id ON audit_event TYPE string DEFAULT '' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE actor_kind ON audit_event TYPE string ASSERT $value INSIDE ['user', 'token', 'oauth', 'anonymous'] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE created_at ON audit_event TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE detail ON audit_event TYPE string DEFAULT '' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE outcome ON audit_event TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE target ON audit_event TYPE string DEFAULT '' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE trace_id ON audit_event TYPE string DEFAULT '' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE user ON audit_event TYPE none | record<user> PERMISSIONS FULL;

DEFINE INDEX audit_event_trace_idx ON audit_event FIELDS trace_id;
DEFINE INDEX audit_event_user_idx ON audit_event FIELDS user, created_at;


-- ------------------------------
-- TABLE DATA: audit_event
-- ------------------------------


-- ------------------------------
-- TABLE: audit_log
-- ------------------------------

DEFINE TABLE audit_log TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE args_summary ON audit_log TYPE string DEFAULT '' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE created_at ON audit_log TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE outcome ON audit_log TYPE string ASSERT $value INSIDE ['ok', 'error'] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner ON audit_log TYPE record<user> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE tool_name ON audit_log TYPE string PERMISSIONS FULL;

DEFINE INDEX audit_log_owner_idx ON audit_log FIELDS owner, created_at;


-- ------------------------------
-- TABLE DATA: audit_log
-- ------------------------------


-- ------------------------------
-- TABLE: cache_record
-- ------------------------------

DEFINE TABLE cache_record TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE body_text ON cache_record TYPE string DEFAULT '' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE content_hash ON cache_record TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE deleted ON cache_record TYPE bool DEFAULT false PERMISSIONS FULL;
DEFINE FIELD OVERWRITE embedding ON cache_record TYPE none | array<float> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE embedding.* ON cache_record TYPE float PERMISSIONS FULL;
DEFINE FIELD OVERWRITE external_id ON cache_record TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE ingested_at ON cache_record TYPE datetime PERMISSIONS FULL;
DEFINE FIELD OVERWRITE occurred_at ON cache_record TYPE none | datetime PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner ON cache_record TYPE record<user> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE payload ON cache_record TYPE object FLEXIBLE DEFAULT {  } PERMISSIONS FULL;
DEFINE FIELD OVERWRITE source ON cache_record TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE title ON cache_record TYPE string DEFAULT '' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE type ON cache_record TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE updated_at ON cache_record TYPE datetime PERMISSIONS FULL;
DEFINE FIELD OVERWRITE url ON cache_record TYPE string DEFAULT '' PERMISSIONS FULL;

DEFINE INDEX cache_record_embedding_idx ON cache_record FIELDS embedding HNSW DIMENSION 1536 DIST COSINE TYPE F32 EFC 150 M 12 M0 24 LM 0.40242960438184466f;
DEFINE INDEX cache_record_fts_idx ON cache_record FIELDS title FULLTEXT ANALYZER cache_text_analyzer BM25(1.2,0.75) HIGHLIGHTS;
DEFINE INDEX cache_record_occurred ON cache_record FIELDS occurred_at;
DEFINE INDEX cache_record_source_type ON cache_record FIELDS source, type;


-- ------------------------------
-- TABLE DATA: cache_record
-- ------------------------------

INSERT [ { body_text: 'weekly shop', content_hash: 'h', deleted: false, embedding: [1f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f], external_id: 'r1', id: cache_record:r1, ingested_at: d'2026-10-08T13:40:56.977379Z', owner: user:u, payload: {  }, source: 'demo', title: 'Woolworths groceries', type: 'note', updated_at: d'2026-10-08T13:40:56.977382Z', url: '' }, { body_text: 'monthly streaming', content_hash: 'h', deleted: false, embedding: [0f, 1f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f, 0f], external_id: 'r2', id: cache_record:r2, ingested_at: d'2026-10-08T13:40:56.984431Z', owner: user:u, payload: {  }, source: 'demo', title: 'Netflix subscription', type: 'note', updated_at: d'2026-10-08T13:40:56.984435Z', url: '' } ];

-- ------------------------------
-- TABLE: chat_message
-- ------------------------------

DEFINE TABLE chat_message TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE content ON chat_message TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE created_at ON chat_message TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner ON chat_message TYPE record<user> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE role ON chat_message TYPE string ASSERT $value INSIDE ['user', 'assistant', 'tool'] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE thread_id ON chat_message TYPE record<chat_thread> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE tool_call_id ON chat_message TYPE none | string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE tool_calls ON chat_message TYPE none | array<object> FLEXIBLE PERMISSIONS FULL;
DEFINE FIELD OVERWRITE tool_calls.* ON chat_message TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE INDEX chat_message_owner_idx ON chat_message FIELDS owner, created_at;
DEFINE INDEX chat_message_thread_idx ON chat_message FIELDS thread_id, created_at;


-- ------------------------------
-- TABLE DATA: chat_message
-- ------------------------------


-- ------------------------------
-- TABLE: chat_thread
-- ------------------------------

DEFINE TABLE chat_thread TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE created_at ON chat_thread TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner ON chat_thread TYPE record<user> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE title ON chat_thread TYPE string DEFAULT 'New chat' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE updated_at ON chat_thread TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE INDEX chat_thread_owner_idx ON chat_thread FIELDS owner, updated_at;


-- ------------------------------
-- TABLE DATA: chat_thread
-- ------------------------------


-- ------------------------------
-- TABLE: connector
-- ------------------------------

DEFINE TABLE connector TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE config ON connector TYPE object FLEXIBLE DEFAULT {  } PERMISSIONS FULL;
DEFINE FIELD OVERWRITE credentials_encrypted ON connector TYPE string DEFAULT '' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE enabled ON connector TYPE bool DEFAULT false PERMISSIONS FULL;
DEFINE FIELD OVERWRITE kind ON connector TYPE string ASSERT $value INSIDE ['up_bank', 'pocketai', 'open_connector', 'github', 'slack', 'notion', 'linear', 'gmail', 'google_calendar', 'discord', 'spotify', 'todoist', 'stripe', 'demo'] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner ON connector TYPE record<user> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE updated_at ON connector TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE INDEX connector_owner_kind_unique ON connector FIELDS owner, kind UNIQUE;


-- ------------------------------
-- TABLE DATA: connector
-- ------------------------------


-- ------------------------------
-- TABLE: embed_cache
-- ------------------------------

DEFINE TABLE embed_cache TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE created_at ON embed_cache TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE text_hmac ON embed_cache TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE vector ON embed_cache TYPE array<float> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE vector.* ON embed_cache TYPE float PERMISSIONS FULL;

DEFINE INDEX embed_cache_hmac_unique ON embed_cache FIELDS text_hmac UNIQUE;


-- ------------------------------
-- TABLE DATA: embed_cache
-- ------------------------------


-- ------------------------------
-- TABLE: failure_capsule
-- ------------------------------

DEFINE TABLE failure_capsule TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE args ON failure_capsule TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE code ON failure_capsule TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE created_at ON failure_capsule TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE kind ON failure_capsule TYPE string ASSERT $value INSIDE ['tool', 'route'] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE name ON failure_capsule TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE source ON failure_capsule TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE status ON failure_capsule TYPE int PERMISSIONS FULL;
DEFINE FIELD OVERWRITE trace_id ON failure_capsule TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE truncated ON failure_capsule TYPE bool DEFAULT false PERMISSIONS FULL;
DEFINE FIELD OVERWRITE user ON failure_capsule TYPE none | string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE version ON failure_capsule TYPE string PERMISSIONS FULL;

DEFINE INDEX failure_capsule_created_idx ON failure_capsule FIELDS created_at;


-- ------------------------------
-- TABLE DATA: failure_capsule
-- ------------------------------


-- ------------------------------
-- TABLE: file
-- ------------------------------

DEFINE TABLE file TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE aliases ON file TYPE array<string> DEFAULT [] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE aliases.* ON file TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE created_at ON file TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE name ON file TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner ON file TYPE record<user> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE summary ON file TYPE string DEFAULT '' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE updated_at ON file TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE vault ON file TYPE record<vault> PERMISSIONS FULL;

DEFINE INDEX file_vault_name_idx ON file FIELDS vault, name;
DEFINE INDEX file_vault_name_unique ON file FIELDS vault, name UNIQUE;


-- ------------------------------
-- TABLE DATA: file
-- ------------------------------


-- ------------------------------
-- TABLE: job
-- ------------------------------

DEFINE TABLE job TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE attempts ON job TYPE int DEFAULT 0 PERMISSIONS FULL;
DEFINE FIELD OVERWRITE created_at ON job TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE idempotency_key ON job TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE kind ON job TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE last_error_code ON job TYPE none | string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE locked_by ON job TYPE none | string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE locked_until ON job TYPE none | datetime PERMISSIONS FULL;
DEFINE FIELD OVERWRITE max_attempts ON job TYPE int DEFAULT 5 PERMISSIONS FULL;
DEFINE FIELD OVERWRITE org ON job TYPE none | string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner ON job TYPE record<user> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE payload ON job TYPE object FLEXIBLE DEFAULT {  } PERMISSIONS FULL;
DEFINE FIELD OVERWRITE run_at ON job TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE status ON job TYPE string DEFAULT 'ready' ASSERT $value INSIDE ['ready', 'running', 'done', 'dead'] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE traceparent ON job TYPE none | string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE updated_at ON job TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE INDEX job_idempotency_unique ON job FIELDS idempotency_key UNIQUE;
DEFINE INDEX job_lease_idx ON job FIELDS status, locked_until;
DEFINE INDEX job_ready_idx ON job FIELDS status, run_at;


-- ------------------------------
-- TABLE DATA: job
-- ------------------------------


-- ------------------------------
-- TABLE: job_leader
-- ------------------------------

DEFINE TABLE job_leader TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE holder ON job_leader TYPE none | string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE until ON job_leader TYPE datetime PERMISSIONS FULL;



-- ------------------------------
-- TABLE DATA: job_leader
-- ------------------------------

INSERT [ { id: job_leader:scheduler, until: d'1970-01-01T00:00:00Z' } ];

-- ------------------------------
-- TABLE: linked_to
-- ------------------------------

DEFINE TABLE linked_to TYPE RELATION IN cache_record OUT cache_record SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE created_at ON linked_to TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE in ON linked_to TYPE record<cache_record> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE origin ON linked_to TYPE string ASSERT $value INSIDE ['sync', 'agent'] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE out ON linked_to TYPE record<cache_record> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE rel ON linked_to TYPE string PERMISSIONS FULL;

DEFINE INDEX linked_to_unique ON linked_to FIELDS in, out, rel UNIQUE;


-- ------------------------------
-- TABLE DATA: linked_to
-- ------------------------------


-- ------------------------------
-- TABLE: location
-- ------------------------------

DEFINE TABLE location TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE aliases ON location TYPE array<string> DEFAULT [] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE aliases.* ON location TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE created_at ON location TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE name ON location TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner ON location TYPE record<user> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE summary ON location TYPE string DEFAULT '' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE updated_at ON location TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE vault ON location TYPE record<vault> PERMISSIONS FULL;

DEFINE INDEX location_vault_name_idx ON location FIELDS vault, name;
DEFINE INDEX location_vault_name_unique ON location FIELDS vault, name UNIQUE;


-- ------------------------------
-- TABLE DATA: location
-- ------------------------------


-- ------------------------------
-- TABLE: memory
-- ------------------------------

DEFINE TABLE memory TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE created_at ON memory TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner ON memory TYPE record<user> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE proof_count ON memory TYPE int DEFAULT 1 PERMISSIONS FULL;
DEFINE FIELD OVERWRITE source ON memory TYPE none | record<cache_record> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE source_memories ON memory TYPE none | array<record<memory>> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE source_memories.* ON memory TYPE record<memory> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE status ON memory TYPE none | string ASSERT $value INSIDE ['fresh', 'stale'] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE subject ON memory TYPE record<person | organisation | location | repository | file | symbol> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE text ON memory TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE type ON memory TYPE string DEFAULT 'world' ASSERT $value INSIDE ['world', 'experience', 'observation'] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE updated_at ON memory TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE vault ON memory TYPE record<vault> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE version ON memory TYPE int DEFAULT 1 PERMISSIONS FULL;

DEFINE INDEX memory_subject_idx ON memory FIELDS subject;
DEFINE INDEX memory_text_fts_idx ON memory FIELDS text FULLTEXT ANALYZER cache_text_analyzer BM25(1.2,0.75);
DEFINE INDEX memory_type_status_idx ON memory FIELDS type, status;
DEFINE INDEX memory_vault_idx ON memory FIELDS vault;


-- ------------------------------
-- TABLE DATA: memory
-- ------------------------------

INSERT [ { created_at: d'2026-10-08T13:40:56.975136Z', id: memory:m1, owner: user:u, proof_count: 1, subject: person:ann, text: 'Ann likes green tea', type: 'world', updated_at: d'2026-10-08T13:40:56.975177Z', vault: vault:v, version: 1 }, { created_at: d'2026-10-08T13:40:56.976159Z', id: memory:m2, owner: user:u, proof_count: 1, subject: person:ann, text: 'Ann works remotely', type: 'world', updated_at: d'2026-10-08T13:40:56.976204Z', vault: vault:v, version: 1 } ];

-- ------------------------------
-- TABLE: oauth_client
-- ------------------------------

DEFINE TABLE oauth_client TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE client_id ON oauth_client TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE client_uri ON oauth_client TYPE none | string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE expires_at ON oauth_client TYPE none | datetime PERMISSIONS FULL;
DEFINE FIELD OVERWRITE fetched_at ON oauth_client TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE kind ON oauth_client TYPE string ASSERT $value INSIDE ['dcr', 'cimd'] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE logo_uri ON oauth_client TYPE none | string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE name ON oauth_client TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE redirect_uris ON oauth_client TYPE array<string> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE redirect_uris.* ON oauth_client TYPE string PERMISSIONS FULL;

DEFINE INDEX oauth_client_id_unique ON oauth_client FIELDS client_id UNIQUE;


-- ------------------------------
-- TABLE DATA: oauth_client
-- ------------------------------


-- ------------------------------
-- TABLE: oauth_code
-- ------------------------------

DEFINE TABLE oauth_code TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE client_id ON oauth_code TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE code_challenge ON oauth_code TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE code_hash ON oauth_code TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE expires_at ON oauth_code TYPE datetime PERMISSIONS FULL;
DEFINE FIELD OVERWRITE grant_id ON oauth_code TYPE none | record<oauth_grant> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner ON oauth_code TYPE record<user> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE redeemed_at ON oauth_code TYPE none | datetime PERMISSIONS FULL;
DEFINE FIELD OVERWRITE redirect_uri ON oauth_code TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE resource ON oauth_code TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE scope ON oauth_code TYPE array<string> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE scope.* ON oauth_code TYPE string PERMISSIONS FULL;

DEFINE INDEX oauth_code_hash_unique ON oauth_code FIELDS code_hash UNIQUE;


-- ------------------------------
-- TABLE DATA: oauth_code
-- ------------------------------


-- ------------------------------
-- TABLE: oauth_grant
-- ------------------------------

DEFINE TABLE oauth_grant TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE client_id ON oauth_grant TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE client_logo ON oauth_grant TYPE none | string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE client_name ON oauth_grant TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE created_at ON oauth_grant TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE last_used_at ON oauth_grant TYPE none | datetime PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner ON oauth_grant TYPE record<user> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE resource ON oauth_grant TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE scope ON oauth_grant TYPE array<string> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE scope.* ON oauth_grant TYPE string PERMISSIONS FULL;

DEFINE INDEX oauth_grant_owner_idx ON oauth_grant FIELDS owner;


-- ------------------------------
-- TABLE DATA: oauth_grant
-- ------------------------------


-- ------------------------------
-- TABLE: oauth_token
-- ------------------------------

DEFINE TABLE oauth_token TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE expires_at ON oauth_token TYPE datetime PERMISSIONS FULL;
DEFINE FIELD OVERWRITE family ON oauth_token TYPE record<oauth_grant> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE kind ON oauth_token TYPE string ASSERT $value INSIDE ['access', 'refresh'] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE token_hash ON oauth_token TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE used_at ON oauth_token TYPE none | datetime PERMISSIONS FULL;

DEFINE INDEX oauth_token_family_idx ON oauth_token FIELDS family;
DEFINE INDEX oauth_token_hash_unique ON oauth_token FIELDS token_hash UNIQUE;


-- ------------------------------
-- TABLE DATA: oauth_token
-- ------------------------------


-- ------------------------------
-- TABLE: organisation
-- ------------------------------

DEFINE TABLE organisation TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE aliases ON organisation TYPE array<string> DEFAULT [] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE aliases.* ON organisation TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE created_at ON organisation TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE name ON organisation TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner ON organisation TYPE record<user> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE summary ON organisation TYPE string DEFAULT '' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE updated_at ON organisation TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE vault ON organisation TYPE record<vault> PERMISSIONS FULL;

DEFINE INDEX organisation_vault_name_idx ON organisation FIELDS vault, name;
DEFINE INDEX organisation_vault_name_unique ON organisation FIELDS vault, name UNIQUE;


-- ------------------------------
-- TABLE DATA: organisation
-- ------------------------------

INSERT [ { aliases: [], created_at: d'2026-10-08T13:40:56.973830Z', id: organisation:acme, name: 'Acme', owner: user:u, summary: '', updated_at: d'2026-10-08T13:40:56.973851Z', vault: vault:v } ];

-- ------------------------------
-- TABLE: person
-- ------------------------------

DEFINE TABLE person TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE aliases ON person TYPE array<string> DEFAULT [] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE aliases.* ON person TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE created_at ON person TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE name ON person TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner ON person TYPE record<user> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE summary ON person TYPE string DEFAULT '' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE updated_at ON person TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE vault ON person TYPE record<vault> PERMISSIONS FULL;

DEFINE INDEX person_vault_name_idx ON person FIELDS vault, name;
DEFINE INDEX person_vault_name_unique ON person FIELDS vault, name UNIQUE;


-- ------------------------------
-- TABLE DATA: person
-- ------------------------------

INSERT [ { aliases: [], created_at: d'2026-10-08T13:40:56.973336Z', id: person:ann, name: 'Ann', owner: user:u, summary: '', updated_at: d'2026-10-08T13:40:56.973359Z', vault: vault:v } ];

-- ------------------------------
-- TABLE: relates_to
-- ------------------------------

DEFINE TABLE relates_to TYPE RELATION IN person | organisation | location | repository | file | symbol OUT person | organisation | location | repository | file | symbol SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE created_at ON relates_to TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE in ON relates_to TYPE record<person | organisation | location | repository | file | symbol> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE label ON relates_to TYPE string DEFAULT '' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE out ON relates_to TYPE record<person | organisation | location | repository | file | symbol> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner ON relates_to TYPE none | record<user> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE source ON relates_to TYPE none | record<cache_record> PERMISSIONS FULL;

DEFINE INDEX relates_to_unique ON relates_to FIELDS in, out, label UNIQUE;


-- ------------------------------
-- TABLE DATA: relates_to
-- ------------------------------

INSERT RELATION [ { created_at: d'2026-10-08T13:40:56.974411Z', id: relates_to:79u3sstuvqmfs3vpsj69, in: person:ann, label: 'works_at', out: organisation:acme } ];

-- ------------------------------
-- TABLE: repository
-- ------------------------------

DEFINE TABLE repository TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE aliases ON repository TYPE array<string> DEFAULT [] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE aliases.* ON repository TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE created_at ON repository TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE name ON repository TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner ON repository TYPE record<user> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE summary ON repository TYPE string DEFAULT '' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE updated_at ON repository TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE vault ON repository TYPE record<vault> PERMISSIONS FULL;

DEFINE INDEX repository_vault_name_idx ON repository FIELDS vault, name;
DEFINE INDEX repository_vault_name_unique ON repository FIELDS vault, name UNIQUE;


-- ------------------------------
-- TABLE DATA: repository
-- ------------------------------


-- ------------------------------
-- TABLE: session
-- ------------------------------

DEFINE TABLE session TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE created_at ON session TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE expires_at ON session TYPE none | datetime PERMISSIONS FULL;
DEFINE FIELD OVERWRITE last_seen_at ON session TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner ON session TYPE record<user> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE revoked ON session TYPE bool DEFAULT false PERMISSIONS FULL;
DEFINE FIELD OVERWRITE sid ON session TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE user_agent ON session TYPE string DEFAULT '' PERMISSIONS FULL;

DEFINE INDEX session_owner_idx ON session FIELDS owner;
DEFINE INDEX session_sid_unique ON session FIELDS sid UNIQUE;


-- ------------------------------
-- TABLE DATA: session
-- ------------------------------


-- ------------------------------
-- TABLE: symbol
-- ------------------------------

DEFINE TABLE symbol TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE aliases ON symbol TYPE array<string> DEFAULT [] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE aliases.* ON symbol TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE created_at ON symbol TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE name ON symbol TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner ON symbol TYPE record<user> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE summary ON symbol TYPE string DEFAULT '' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE updated_at ON symbol TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE vault ON symbol TYPE record<vault> PERMISSIONS FULL;

DEFINE INDEX symbol_vault_name_idx ON symbol FIELDS vault, name;
DEFINE INDEX symbol_vault_name_unique ON symbol FIELDS vault, name UNIQUE;


-- ------------------------------
-- TABLE DATA: symbol
-- ------------------------------


-- ------------------------------
-- TABLE: sync_status
-- ------------------------------

DEFINE TABLE sync_status TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE consecutive_failures ON sync_status TYPE int DEFAULT 0 PERMISSIONS FULL;
DEFINE FIELD OVERWRITE cursor ON sync_status TYPE string DEFAULT '' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE last_error ON sync_status TYPE string DEFAULT '' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE last_ok ON sync_status TYPE none | datetime PERMISSIONS FULL;
DEFINE FIELD OVERWRITE last_report ON sync_status TYPE object FLEXIBLE DEFAULT {  } PERMISSIONS FULL;
DEFINE FIELD OVERWRITE last_run ON sync_status TYPE none | datetime PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner ON sync_status TYPE record<user> PERMISSIONS FULL;



-- ------------------------------
-- TABLE DATA: sync_status
-- ------------------------------


-- ------------------------------
-- TABLE: user
-- ------------------------------

DEFINE TABLE user TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE api_token_hash ON user TYPE none | string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE created_at ON user TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE email ON user TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE onboarded_at ON user TYPE none | datetime PERMISSIONS FULL;
DEFINE FIELD OVERWRITE password_hash ON user TYPE string PERMISSIONS FULL;

DEFINE INDEX user_email_unique ON user FIELDS email UNIQUE;


-- ------------------------------
-- TABLE DATA: user
-- ------------------------------

INSERT [ { created_at: d'2026-10-08T13:40:56.972021Z', email: 'a@b.c', id: user:u, password_hash: 'x' } ];

-- ------------------------------
-- TABLE: vault
-- ------------------------------

DEFINE TABLE vault TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE created_at ON vault TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE kind ON vault TYPE string DEFAULT 'personal' ASSERT $value INSIDE ['personal', 'org'] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE name ON vault TYPE string PERMISSIONS FULL;



-- ------------------------------
-- TABLE DATA: vault
-- ------------------------------

INSERT [ { created_at: d'2026-10-08T13:40:56.972419Z', id: vault:v, kind: 'personal', name: 'Personal' } ];

-- ------------------------------
-- TABLE: vault_member
-- ------------------------------

DEFINE TABLE vault_member TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE created_at ON vault_member TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE role ON vault_member TYPE string DEFAULT 'member' ASSERT $value INSIDE ['owner', 'member'] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE status ON vault_member TYPE string DEFAULT 'active' ASSERT $value INSIDE ['pending', 'active'] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE user ON vault_member TYPE record<user> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE vault ON vault_member TYPE record<vault> PERMISSIONS FULL;

DEFINE INDEX vault_member_unique ON vault_member FIELDS vault, user UNIQUE;
DEFINE INDEX vault_member_user_idx ON vault_member FIELDS user;


-- ------------------------------
-- TABLE DATA: vault_member
-- ------------------------------

INSERT [ { created_at: d'2026-10-08T13:40:56.972740Z', id: vault_member:vm, role: 'owner', status: 'active', user: user:u, vault: vault:v } ];

