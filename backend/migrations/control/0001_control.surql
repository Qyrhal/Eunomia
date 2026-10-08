-- The control database: everything that exists before an org does (accounts, credentials), plus the
-- org registry and the routing table that says where each org's data lives. Per-org data lives in
-- `org_<uuid>` databases (migrations/tenant). Shapes match the single-database schema they were
-- moved out of (tenant 0001, 0003 to 0007), so a self-host install's rows copy over unchanged.

DEFINE TABLE OVERWRITE user SCHEMAFULL;
DEFINE FIELD OVERWRITE email ON user TYPE string;
DEFINE FIELD OVERWRITE password_hash ON user TYPE string;
DEFINE FIELD OVERWRITE api_token_hash ON user TYPE option<string>;
DEFINE FIELD OVERWRITE onboarded_at ON user TYPE option<datetime>;
DEFINE FIELD OVERWRITE created_at ON user TYPE datetime DEFAULT time::now();
DEFINE INDEX OVERWRITE user_email_unique ON user FIELDS email UNIQUE;

DEFINE TABLE OVERWRITE org SCHEMAFULL;
DEFINE FIELD OVERWRITE name ON org TYPE string;
DEFINE FIELD OVERWRITE created_at ON org TYPE datetime DEFAULT time::now();

-- A user's place in an org. A credential (session, token, grant) resolves to the user's oldest membership.
DEFINE TABLE OVERWRITE membership SCHEMAFULL;
DEFINE FIELD OVERWRITE user ON membership TYPE record<user>;
DEFINE FIELD OVERWRITE org ON membership TYPE record<org>;
DEFINE FIELD OVERWRITE role ON membership TYPE string DEFAULT "member" ASSERT $value IN ["owner", "member"];
DEFINE FIELD OVERWRITE created_at ON membership TYPE datetime DEFAULT time::now();
DEFINE INDEX OVERWRITE membership_unique ON membership FIELDS user, org UNIQUE;
DEFINE INDEX OVERWRITE membership_user_idx ON membership FIELDS user, created_at;
DEFINE INDEX OVERWRITE membership_org_idx ON membership FIELDS org;

-- The routing table. Record id = the org's key; `db` is the database holding its data, `db_user` the
-- database user the app signs in as, `db_pass_enc` that user's password (AES-GCM under ENCRYPTION_KEY).
DEFINE TABLE OVERWRITE tenant SCHEMAFULL;
DEFINE FIELD OVERWRITE org ON tenant TYPE record<org>;
DEFINE FIELD OVERWRITE db ON tenant TYPE string;
DEFINE FIELD OVERWRITE db_user ON tenant TYPE string DEFAULT "app";
DEFINE FIELD OVERWRITE db_pass_enc ON tenant TYPE string;
DEFINE FIELD OVERWRITE schema_version ON tenant TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE status ON tenant TYPE string DEFAULT "provisioning" ASSERT $value IN ["provisioning", "moving", "ready"];
DEFINE FIELD OVERWRITE created_at ON tenant TYPE datetime DEFAULT time::now();
DEFINE FIELD OVERWRITE updated_at ON tenant TYPE datetime DEFAULT time::now();
DEFINE INDEX OVERWRITE tenant_org_unique ON tenant FIELDS org UNIQUE;

DEFINE TABLE OVERWRITE api_token SCHEMAFULL;
DEFINE FIELD OVERWRITE owner ON api_token TYPE record<user>;
DEFINE FIELD OVERWRITE name ON api_token TYPE string DEFAULT "API token";
DEFINE FIELD OVERWRITE token_hash ON api_token TYPE string;
DEFINE FIELD OVERWRITE scopes ON api_token TYPE array<string> DEFAULT ["memory:read", "memory:write", "vaults:admin", "connectors"];
-- a vault lives in the org's database; this is only its id
DEFINE FIELD OVERWRITE vault ON api_token TYPE option<record<vault>>;
DEFINE FIELD OVERWRITE expires_at ON api_token TYPE option<datetime>;
DEFINE FIELD OVERWRITE created_at ON api_token TYPE datetime DEFAULT time::now();
DEFINE FIELD OVERWRITE last_used_at ON api_token TYPE option<datetime>;
DEFINE INDEX OVERWRITE api_token_hash_unique ON api_token FIELDS token_hash UNIQUE;
DEFINE INDEX OVERWRITE api_token_owner_idx ON api_token FIELDS owner;

DEFINE TABLE OVERWRITE session SCHEMAFULL;
DEFINE FIELD OVERWRITE owner ON session TYPE record<user>;
DEFINE FIELD OVERWRITE sid ON session TYPE string;
DEFINE FIELD OVERWRITE user_agent ON session TYPE string DEFAULT "";
DEFINE FIELD OVERWRITE expires_at ON session TYPE option<datetime>;
DEFINE FIELD OVERWRITE created_at ON session TYPE datetime DEFAULT time::now();
DEFINE FIELD OVERWRITE last_seen_at ON session TYPE datetime DEFAULT time::now();
DEFINE FIELD OVERWRITE revoked ON session TYPE bool DEFAULT false;
DEFINE INDEX OVERWRITE session_sid_unique ON session FIELDS sid UNIQUE;
DEFINE INDEX OVERWRITE session_owner_idx ON session FIELDS owner;

-- OAuth 2.1 authorization server for MCP clients. Secrets are stored as sha256 hashes only.
DEFINE TABLE OVERWRITE oauth_client SCHEMAFULL;
DEFINE FIELD OVERWRITE client_id ON oauth_client TYPE string;
DEFINE FIELD OVERWRITE name ON oauth_client TYPE string;
DEFINE FIELD OVERWRITE logo_uri ON oauth_client TYPE option<string>;
DEFINE FIELD OVERWRITE client_uri ON oauth_client TYPE option<string>;
DEFINE FIELD OVERWRITE redirect_uris ON oauth_client TYPE array<string>;
DEFINE FIELD OVERWRITE kind ON oauth_client TYPE string ASSERT $value IN ["dcr","cimd"];
DEFINE FIELD OVERWRITE fetched_at ON oauth_client TYPE datetime DEFAULT time::now();
DEFINE FIELD OVERWRITE expires_at ON oauth_client TYPE option<datetime>;
DEFINE INDEX OVERWRITE oauth_client_id_unique ON oauth_client FIELDS client_id UNIQUE;

DEFINE TABLE OVERWRITE oauth_code SCHEMAFULL;
DEFINE FIELD OVERWRITE code_hash ON oauth_code TYPE string;
DEFINE FIELD OVERWRITE owner ON oauth_code TYPE record<user>;
DEFINE FIELD OVERWRITE client_id ON oauth_code TYPE string;
DEFINE FIELD OVERWRITE redirect_uri ON oauth_code TYPE string;
DEFINE FIELD OVERWRITE code_challenge ON oauth_code TYPE string;
DEFINE FIELD OVERWRITE scope ON oauth_code TYPE array<string>;
DEFINE FIELD OVERWRITE resource ON oauth_code TYPE string;
DEFINE FIELD OVERWRITE expires_at ON oauth_code TYPE datetime;
DEFINE FIELD OVERWRITE redeemed_at ON oauth_code TYPE option<datetime>;
DEFINE FIELD OVERWRITE grant_id ON oauth_code TYPE option<record<oauth_grant>>;
DEFINE INDEX OVERWRITE oauth_code_hash_unique ON oauth_code FIELDS code_hash UNIQUE;

DEFINE TABLE OVERWRITE oauth_grant SCHEMAFULL;
DEFINE FIELD OVERWRITE owner ON oauth_grant TYPE record<user>;
DEFINE FIELD OVERWRITE client_id ON oauth_grant TYPE string;
DEFINE FIELD OVERWRITE client_name ON oauth_grant TYPE string;
DEFINE FIELD OVERWRITE client_logo ON oauth_grant TYPE option<string>;
DEFINE FIELD OVERWRITE scope ON oauth_grant TYPE array<string>;
DEFINE FIELD OVERWRITE resource ON oauth_grant TYPE string;
DEFINE FIELD OVERWRITE created_at ON oauth_grant TYPE datetime DEFAULT time::now();
DEFINE FIELD OVERWRITE last_used_at ON oauth_grant TYPE option<datetime>;
DEFINE INDEX OVERWRITE oauth_grant_owner_idx ON oauth_grant FIELDS owner;

DEFINE TABLE OVERWRITE oauth_token SCHEMAFULL;
DEFINE FIELD OVERWRITE kind ON oauth_token TYPE string ASSERT $value IN ["access","refresh"];
DEFINE FIELD OVERWRITE token_hash ON oauth_token TYPE string;
DEFINE FIELD OVERWRITE family ON oauth_token TYPE record<oauth_grant>;
DEFINE FIELD OVERWRITE expires_at ON oauth_token TYPE datetime;
DEFINE FIELD OVERWRITE used_at ON oauth_token TYPE option<datetime>;
DEFINE INDEX OVERWRITE oauth_token_hash_unique ON oauth_token FIELDS token_hash UNIQUE;
DEFINE INDEX OVERWRITE oauth_token_family_idx ON oauth_token FIELDS family;

-- Append-only security ledger: the application only CREATEs rows here.
DEFINE TABLE OVERWRITE audit_event SCHEMAFULL PERMISSIONS FOR select, create FULL, FOR update, delete NONE;
DEFINE FIELD OVERWRITE user ON audit_event TYPE option<record<user>>;
DEFINE FIELD OVERWRITE actor_kind ON audit_event TYPE string ASSERT $value IN ["user", "token", "oauth", "anonymous"];
DEFINE FIELD OVERWRITE actor_id ON audit_event TYPE string DEFAULT "";
DEFINE FIELD OVERWRITE action ON audit_event TYPE string;
DEFINE FIELD OVERWRITE target ON audit_event TYPE string DEFAULT "";
DEFINE FIELD OVERWRITE outcome ON audit_event TYPE string;
DEFINE FIELD OVERWRITE trace_id ON audit_event TYPE string DEFAULT "";
DEFINE FIELD OVERWRITE detail ON audit_event TYPE string DEFAULT "";
DEFINE FIELD OVERWRITE created_at ON audit_event TYPE datetime DEFAULT time::now();
DEFINE INDEX OVERWRITE audit_event_user_idx ON audit_event FIELDS user, created_at;
DEFINE INDEX OVERWRITE audit_event_trace_idx ON audit_event FIELDS trace_id;

-- Background jobs (docs/architecture/jobs.md). `org` says whose database the handler works in.
DEFINE TABLE OVERWRITE job SCHEMAFULL;
DEFINE FIELD OVERWRITE org ON job TYPE option<string>;
DEFINE FIELD OVERWRITE kind ON job TYPE string;
DEFINE FIELD OVERWRITE owner ON job TYPE record<user>;
DEFINE FIELD OVERWRITE payload ON job TYPE object FLEXIBLE DEFAULT {};
DEFINE FIELD OVERWRITE idempotency_key ON job TYPE string;
DEFINE FIELD OVERWRITE run_at ON job TYPE datetime DEFAULT time::now();
DEFINE FIELD OVERWRITE attempts ON job TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE max_attempts ON job TYPE int DEFAULT 5;
DEFINE FIELD OVERWRITE locked_by ON job TYPE option<string>;
DEFINE FIELD OVERWRITE locked_until ON job TYPE option<datetime>;
DEFINE FIELD OVERWRITE status ON job TYPE string DEFAULT "ready" ASSERT $value IN ["ready", "running", "done", "dead"];
DEFINE FIELD OVERWRITE last_error_code ON job TYPE option<string>;
DEFINE FIELD OVERWRITE traceparent ON job TYPE option<string>;
DEFINE FIELD OVERWRITE created_at ON job TYPE datetime DEFAULT time::now();
DEFINE FIELD OVERWRITE updated_at ON job TYPE datetime DEFAULT time::now();
DEFINE INDEX OVERWRITE job_idempotency_unique ON job FIELDS idempotency_key UNIQUE;
DEFINE INDEX OVERWRITE job_ready_idx ON job FIELDS status, run_at;
DEFINE INDEX OVERWRITE job_lease_idx ON job FIELDS status, locked_until;

-- One row, id `scheduler`: whoever holds an unexpired lease runs the periodic reconcilers.
DEFINE TABLE OVERWRITE job_leader SCHEMAFULL;
DEFINE FIELD OVERWRITE holder ON job_leader TYPE option<string>;
DEFINE FIELD OVERWRITE until ON job_leader TYPE datetime;
UPSERT job_leader:scheduler SET holder = NONE, until = d"1970-01-01T00:00:00Z";

-- Redacted failure capsules (docs/debugging.md), pruned by the `prune_capsules` job.
DEFINE TABLE OVERWRITE failure_capsule SCHEMAFULL;
DEFINE FIELD OVERWRITE org ON failure_capsule TYPE option<string>;
DEFINE FIELD OVERWRITE trace_id ON failure_capsule TYPE string;
DEFINE FIELD OVERWRITE kind ON failure_capsule TYPE string ASSERT $value IN ["tool", "route"];
DEFINE FIELD OVERWRITE name ON failure_capsule TYPE string;
DEFINE FIELD OVERWRITE user ON failure_capsule TYPE option<string>;
DEFINE FIELD OVERWRITE args ON failure_capsule TYPE string;
DEFINE FIELD OVERWRITE code ON failure_capsule TYPE string;
DEFINE FIELD OVERWRITE status ON failure_capsule TYPE int;
DEFINE FIELD OVERWRITE source ON failure_capsule TYPE string;
DEFINE FIELD OVERWRITE version ON failure_capsule TYPE string;
DEFINE FIELD OVERWRITE truncated ON failure_capsule TYPE bool DEFAULT false;
DEFINE FIELD OVERWRITE created_at ON failure_capsule TYPE datetime DEFAULT time::now();
DEFINE INDEX OVERWRITE failure_capsule_created_idx ON failure_capsule FIELDS created_at;
