# Migrations

`tenant/NNNN_name.surql` files run once each, in order, at boot (`src/migrate.rs`). Each file and its `_migration` ledger row commit in one transaction. A file's sha256 is stored; editing an applied file stops the boot with a checksum error, so never edit one. Add a new file instead.

Rules for new migrations:

- Use `DEFINE ... OVERWRITE`, not `IF NOT EXISTS`, so a changed definition really applies. (0001 and 0002 keep `IF NOT EXISTS` because they must be safe over installs that predate the ledger.)
- Destructive changes go expand then contract: add the new shape and keep writing both in one release, drop the old shape in a later migration.
- Data backfills that can be slow become jobs later, not migration steps. Data fixes that must precede a constraint (see 0002) are a Rust pre-step in `migrate.rs`, keyed by version.
- Register the file in the `MIGRATIONS` list in `src/migrate.rs`.
