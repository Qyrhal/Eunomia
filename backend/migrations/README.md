# Migrations

Two sets, one runner (`src/migrate.rs`): `control/NNNN_name.surql` run once at boot in the `control` database, and `tenant/NNNN_name.surql` run in each `org_<uuid>` database when it is provisioned and, after an upgrade, by a `migrate_tenant` job per org (see `docs/architecture/tenancy.md`). Each file and its `_migration` ledger row commit in one transaction. A file's sha256 is stored; editing an applied file stops the boot with a checksum error, so never edit one. Add a new file instead.

Rules for new migrations:

- Use `DEFINE ... OVERWRITE`, not `IF NOT EXISTS`, so a changed definition really applies. (0001 and 0002 keep `IF NOT EXISTS` because they must be safe over installs that predate the ledger.)
- Destructive changes go expand then contract: add the new shape and keep writing both in one release, drop the old shape in a later migration.
- Data backfills that can be slow become jobs later, not migration steps. Data fixes that must precede a constraint (see 0002) are a Rust pre-step in `migrate.rs`, keyed by version.
- Register the file in the `MIGRATIONS` list (tenant) or `CONTROL_MIGRATIONS` list (control) in `src/migrate.rs`. A new table also goes in `store::TENANT_TABLES` or `store::CONTROL_TABLES`.
- Org databases are created `STRICT`: a migration must define every table, field and index it relies on.
- Code supports tenant schema N and N-1: do not ship a migration whose absence breaks the previous release (expand then contract), because orgs are migrated one at a time after the code is deployed.

## SurrealDB 3.x

- The files are 3.x syntax. 0001 and 0005 were rewritten from 2.x syntax before release of the 3.x build; the sha256 the 2.x files recorded are accepted in `LEGACY_CHECKSUMS` (`src/migrate.rs`) because an install upgraded with `surreal v2 export --v3` carries its old ledger. Do not add to that list for new files.
- 0001 has no vector or full-text index. `0008_v3_indexes.surql` removes whatever the export converter produced and defines `HNSW DIMENSION 1536 DIST COSINE TYPE F32` and one `FULLTEXT` index per text field (title, body_text, memory text), all `CONCURRENTLY`, plus an owner index for the exact-scan fallback. Org databases are `STRICT` (provisioning runs `DEFINE DATABASE ... STRICT`); the old single self-host database was not, and is only read by the one-time move.
- `tenant/0009_drop_control_tables.surql` removes the tables that moved to `control` (accounts, credentials, OAuth, jobs, capsules). Never apply it to the legacy single database: the move reads those tables (`LEGACY_TENANT_VERSION` is 8).
- `tests/fixtures/v2_export_converted.surql` is the upgrade fixture; `tests/migrations.rs` documents how it is regenerated.
