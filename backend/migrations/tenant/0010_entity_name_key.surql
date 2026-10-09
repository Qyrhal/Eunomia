-- Case-insensitive entity identity. `name_key` and `alias_keys` are lowercase copies the database keeps
-- itself (computed on every write, so code at schema 9 keeps them right too). One entity per (vault, name_key)
-- makes concurrent "Ada" and "ada" converge; `alias_keys` makes alias lookup an index read.
-- Pre-step (Rust, migrate.rs): folds entities that differ only by case into the oldest, then defines the two fields
-- and backfills existing rows OUTSIDE this transaction: an index built in the same transaction as the backfill
-- does not see it and would silently enforce nothing for existing rows.
-- The existing case-sensitive (vault, name) unique index stays; a later contract migration may drop it.
DEFINE FIELD OVERWRITE name_key ON person TYPE string VALUE string::lowercase(name);
DEFINE FIELD OVERWRITE alias_keys ON person TYPE array<string> VALUE (aliases ?? []).map(|$a| string::lowercase($a));
DEFINE INDEX OVERWRITE person_vault_name_key_unique ON person FIELDS vault, name_key UNIQUE;
DEFINE INDEX OVERWRITE person_alias_keys_idx ON person FIELDS alias_keys;
DEFINE FIELD OVERWRITE name_key ON organisation TYPE string VALUE string::lowercase(name);
DEFINE FIELD OVERWRITE alias_keys ON organisation TYPE array<string> VALUE (aliases ?? []).map(|$a| string::lowercase($a));
DEFINE INDEX OVERWRITE organisation_vault_name_key_unique ON organisation FIELDS vault, name_key UNIQUE;
DEFINE INDEX OVERWRITE organisation_alias_keys_idx ON organisation FIELDS alias_keys;
DEFINE FIELD OVERWRITE name_key ON location TYPE string VALUE string::lowercase(name);
DEFINE FIELD OVERWRITE alias_keys ON location TYPE array<string> VALUE (aliases ?? []).map(|$a| string::lowercase($a));
DEFINE INDEX OVERWRITE location_vault_name_key_unique ON location FIELDS vault, name_key UNIQUE;
DEFINE INDEX OVERWRITE location_alias_keys_idx ON location FIELDS alias_keys;
DEFINE FIELD OVERWRITE name_key ON repository TYPE string VALUE string::lowercase(name);
DEFINE FIELD OVERWRITE alias_keys ON repository TYPE array<string> VALUE (aliases ?? []).map(|$a| string::lowercase($a));
DEFINE INDEX OVERWRITE repository_vault_name_key_unique ON repository FIELDS vault, name_key UNIQUE;
DEFINE INDEX OVERWRITE repository_alias_keys_idx ON repository FIELDS alias_keys;
DEFINE FIELD OVERWRITE name_key ON file TYPE string VALUE string::lowercase(name);
DEFINE FIELD OVERWRITE alias_keys ON file TYPE array<string> VALUE (aliases ?? []).map(|$a| string::lowercase($a));
DEFINE INDEX OVERWRITE file_vault_name_key_unique ON file FIELDS vault, name_key UNIQUE;
DEFINE INDEX OVERWRITE file_alias_keys_idx ON file FIELDS alias_keys;
DEFINE FIELD OVERWRITE name_key ON symbol TYPE string VALUE string::lowercase(name);
DEFINE FIELD OVERWRITE alias_keys ON symbol TYPE array<string> VALUE (aliases ?? []).map(|$a| string::lowercase($a));
DEFINE INDEX OVERWRITE symbol_vault_name_key_unique ON symbol FIELDS vault, name_key UNIQUE;
DEFINE INDEX OVERWRITE symbol_alias_keys_idx ON symbol FIELDS alias_keys;
