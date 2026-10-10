-- Personal access tokens: scopes, optional vault restriction, optional expiry.
-- Existing tokens become full-scope and non-expiring, so no install breaks.
DEFINE FIELD OVERWRITE scopes ON api_token TYPE array<string> DEFAULT ["memory:read", "memory:write", "vaults:admin", "connectors"];
DEFINE FIELD OVERWRITE vault ON api_token TYPE option<record<vault>>;
DEFINE FIELD OVERWRITE expires_at ON api_token TYPE option<datetime>;
UPDATE api_token SET scopes = ["memory:read", "memory:write", "vaults:admin", "connectors"] WHERE scopes = NONE;

-- Sessions expire (sliding window, extended on use by the backend).
DEFINE FIELD OVERWRITE expires_at ON session TYPE option<datetime>;
UPDATE session SET expires_at = time::now() + 30d WHERE expires_at = NONE;

-- Append-only security ledger. The application only ever CREATEs and SELECTs
-- here; the PERMISSIONS clause is a second line of defence for record users
-- (the backend connects as a system user, which it does not restrict).
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
