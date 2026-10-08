-- Pre-step (Rust, migrate.rs): merges duplicate (vault, name) entities so these indexes can build.
DEFINE INDEX IF NOT EXISTS person_vault_name_unique ON person FIELDS vault, name UNIQUE;
DEFINE INDEX IF NOT EXISTS organisation_vault_name_unique ON organisation FIELDS vault, name UNIQUE;
DEFINE INDEX IF NOT EXISTS location_vault_name_unique ON location FIELDS vault, name UNIQUE;
DEFINE INDEX IF NOT EXISTS repository_vault_name_unique ON repository FIELDS vault, name UNIQUE;
DEFINE INDEX IF NOT EXISTS file_vault_name_unique ON file FIELDS vault, name UNIQUE;
DEFINE INDEX IF NOT EXISTS symbol_vault_name_unique ON symbol FIELDS vault, name UNIQUE;
