-- OAuth 2.1 authorization server for MCP clients. Secrets (codes, tokens) are stored as sha256 hashes only.
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
DEFINE INDEX OVERWRITE oauth_code_hash_unique ON oauth_code FIELDS code_hash UNIQUE;

-- One row per user consent: the "token family". Deleting it revokes every token in it.
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
