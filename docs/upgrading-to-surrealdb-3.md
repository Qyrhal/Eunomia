# Upgrading a self-hosted install to SurrealDB 3

Eunomia moves from SurrealDB 2.x to SurrealDB 3.3. Released installs run `surrealdb/surrealdb:v2.3` (v1.2.2 and v1.3.0), so that is the real starting point. SurrealDB 3 cannot read 2.x data files, and it refuses to open a database that a newer version has written to. So the data is copied across, never upgraded in place. The script `scripts/upgrade-surreal-v3.sh` does the copy and checks it. You normally do not run it yourself: clicking "Update now" in Settings runs it for you.

## What happens

1. The updater checks out the new release. If its `docker-compose.yml` pins a `surrealdb/surrealdb:v3.x` image and the running database is 2.x, it runs the upgrade script before it pulls or restarts anything else.
2. The backend stops. From here nothing writes to the database. The app is down until the script finishes.
3. An encrypted backup, named `pre-v3-<time>.surql.enc` in the `eunomia-backups` volume. Nothing has to exist beforehand: no released install has a `backup` service. The script copies the `surreal` binary out of the RUNNING server's image into a throwaway volume, pulls (or builds) the new release's backup image, and runs `docker compose run --rm --no-deps backup now pre-v3` with `SURREAL_BIN` pointing at that binary. So a 2.3 server is exported by the 2.3 CLI, never by the 3.x CLI inside the backup image. Nightly rotation never deletes it. Make sure `BACKUP_ENCRYPTION_KEY` is set in `.env` (the updater adds one if missing).
4. The old server stops and its volume is copied to a scratch volume. A 2.7 server (`surrealdb/surrealdb:v2.7.0`) is started on the COPY and exported with `surreal export --v3`. The old volume is never opened by a newer version, because [SurrealDB's upgrade docs](https://surrealdb.com/docs/manage/self-hosted/upgrades-and-patching) say some minor upgrades "can only be reverted" from an export, "including 2.6 to 2.7", so an in-place 2.3 to 2.7 bump would break rollback. The unencrypted export is held in a temporary directory and shredded when the script ends; the scratch container and volume are removed when it ends.
5. The export is adjusted for two index kinds 3.x removed: the `MTREE` embedding index becomes `HNSW` (same name, same dimension, cosine, F32), and the two-field full-text index on `cache_record (title, body_text)` becomes one index per field (`cache_record_fts_idx_title`, `cache_record_fts_idx_body_text`).
6. The 2.7 scratch server stops. SurrealDB 3.3 starts on a NEW volume, `eunomia-surreal-data-v3`, and the export is imported. HNSW indexes are built during the import.
7. The script compares the record count of every table, and the `_migration` ledger rows, between 2.x and 3.x. If they match it writes `SURREAL_DATA_VOLUME=eunomia-surreal-data-v3` into `.env` (so every later `docker compose up` keeps using the new volume) and the updater carries on with the normal pull and restart.
8. If anything fails or does not match, the new volume is deleted, the old 2.x image starts again on the old volume, the backend restarts, the checkout is put back on the previous release, and Settings shows the error. Nothing was changed. You can click "Update now" again after fixing the cause. The script log is `update-status/upgrade.log`.

Running it by hand (same result): `bash scripts/upgrade-surreal-v3.sh` from the repo root, on a checkout whose compose file already pins 3.x. It exits 0 immediately if the stack already runs 3.x, so it is safe to re-run.

## How long it takes and how much disk it needs

Time is roughly linear in data size: a small install (thousands of records) takes under a minute. The slow part is building the HNSW embedding index during import, so installs with hundreds of thousands of embeddings should plan for minutes to tens of minutes. The app is offline for the whole time.

Disk: plan for about 4 times the size of the current `eunomia-surreal-data` volume. That is the old volume (kept), the scratch copy (removed at the end), the new volume, and the temporary export (plus the encrypted backup, which is smaller than the export). Check free space with `docker system df` before you start.

## Rolling back

SurrealDB 3.3 refuses downgrades, so rollback means going back to the old volume, which is never modified or deleted:

1. If the script failed, it already did this for you.
2. After a successful upgrade, to go back anyway: set the compose image back to the old one, `surrealdb/surrealdb:v2.3` on released installs (check out the previous release tag), remove the `SURREAL_DATA_VOLUME=` line from `.env`, and run `docker compose up -d`. You lose anything written after the upgrade. To keep it, restore the post-upgrade state from a backup into 3.x instead.
3. If the old volume is already gone, restore `pre-v3-*.surql.enc` into a 2.x server: `docker compose exec backup eunomia-backup restore pre-v3-<time>.surql.enc`.

## Deleting the old volume

Keep `eunomia-surreal-data` for at least one release after the upgrade (rollback needs it). When you are happy with 3.x and have a recent backup:

```
docker compose stop
docker volume ls | grep surreal-data      # old: <project>_eunomia-surreal-data
docker volume rm <project>_eunomia-surreal-data
docker compose up -d
```

`<project>` is the compose project name (the checkout folder name, `eunomia` by default). Never remove the `-v3` volume: it holds the live data.

## Notes

- The hook lives in `scripts/auto-update.sh`, and the running copy of that script is the one from the OLD release (the updater reads it before it checks out the new one). So the release that adds the hook must ship, and be installed, before the release that pins SurrealDB 3.x. Otherwise run `scripts/upgrade-surreal-v3.sh` by hand once.
- What is proven and what is not (be honest about this before trusting a real install):
  - Proven by shell tests with mocks (no Docker): the updater only runs the upgrade before pulling, undoes the checkout on failure, and injects `ENCRYPTION_KEY`/`JWT_SECRET` only after it succeeds; the backup script refuses a half-written export and its nightly loop survives a failed run.
  - Needs Docker, written but NOT yet run: the whole of `scripts/upgrade-surreal-v3.sh`, including case B in `scripts/tests/upgrade-surreal-v3.test.sh` (starts from the real v1.2.2 compose file: `surrealdb:v2.3`, no `backup` service).
  - Unproven and not covered by the docs: that the 2.7 server opens a RocksDB volume written by 2.3 (the docs only describe 2.6 to 2.7 in a separate guide), and that `export --v3` works on data that came from 2.3. The official 2.x to 3.x guide states `--v3` exports exist "starting with 2.6.0" and says nothing about older servers. If 2.7 refuses the copy the script fails safe: it rolls back and the old volume is untouched. A 2.3 install that fails here must first be moved through an intermediate 2.x release by hand.
  - Unproven: a 2.3 `surreal` binary running inside the Debian backup image (it is a glibc binary, like the one the image already copies from the 3.x image).
- Test: `bash scripts/tests/upgrade-surreal-v3.test.sh` runs the whole flow against real containers (project `fw-upgrade`): a 2.7 install, a forced mismatch that must roll back, the real upgrade, and case B from the released v1.2.2 compose file. CI runs it on every pull request.
