# Migrations

`tenant/NNNN_name.surql` files run once each, in order, at boot (`src/migrate.rs`). Each file and its `_migration` ledger row commit in one transaction. A file's sha256 is stored; editing an applied file stops the boot with a checksum error, so never edit one. Add a new file instead.

Rules for new migrations:

- Use `DEFINE ... OVERWRITE`, not `IF NOT EXISTS`, so a changed definition really applies. (0001 and 0002 keep `IF NOT EXISTS` because they must be safe over installs that predate the ledger.)
- Destructive changes go expand then contract: add the new shape and keep writing both in one release, drop the old shape in a later migration.
- Data backfills that can be slow become jobs later, not migration steps. Data fixes that must precede a constraint (see 0002) are a Rust pre-step in `migrate.rs`, keyed by version.
- Register the file in the `MIGRATIONS` list in `src/migrate.rs`.

## SurrealDB 3.x

- The files are 3.x syntax. 0001 and 0005 were rewritten from 2.x syntax before release of the 3.x build; the sha256 the 2.x files recorded are accepted in `LEGACY_CHECKSUMS` (`src/migrate.rs`) because an install upgraded with `surreal v2 export --v3` carries its old ledger. Do not add to that list for new files.
- 0001 has no vector or full-text index. `0008_v3_indexes.surql` removes whatever the export converter produced and defines `HNSW DIMENSION 1536 DIST COSINE TYPE F32` and one `FULLTEXT` index per text field (title, body_text, memory text), all `CONCURRENTLY`, plus an owner index for the exact-scan fallback. `STRICT` databases are left to the org-tenancy work: provisioning runs `DEFINE DATABASE ... STRICT` per org, and the single self-host database stays non-strict so a fresh boot keeps working without a separate create step.
- `tests/fixtures/v2_export_converted.surql` is the upgrade fixture; `tests/migrations.rs` documents how it is regenerated.
