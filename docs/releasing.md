# Releasing

## The SurrealDB 3 bridge rule

"Update now" runs the updater script that is already loaded from the OLD release (`scripts/auto-update.sh` is read into memory before it checks out the new tag). A release that adds the SurrealDB 3 upgrade hook therefore cannot protect the install that applies it, only the next update.

So the move to SurrealDB 3 takes two releases, in order:

1. **Bridge release**: ships the new `scripts/auto-update.sh` and `scripts/upgrade-surreal-v3.sh` and still pins `surrealdb/surrealdb:v2.x`. Installs take it normally. It must be cut from the last 2.x-compatible line (see below), never from `foundation`.
2. **3.x release**: pins `surrealdb/surrealdb:v3.x`. Tag it only after the bridge release has been out long enough for installs to have taken it. Installs that skip the bridge hit the crash loop described at the top of [upgrading-to-surrealdb-3.md](upgrading-to-surrealdb-3.md) and need the manual recovery command.

Version rule: the tag immediately before any 3.x-pinning tag (by `sort -V`) must already contain the hook. Never put the hook and the 3.x pin in the same tag.

## Cutting the bridge release

Versions: this `foundation` line is `2.0.0` (SurrealDB 3 and a database per org, a breaking self-host upgrade). The bridge is cut from `v1.4.1` (the last 1.x release) and is versioned `1.4.2`: bump `backend/Cargo.toml`, `backend/Cargo.lock` and `frontend/package.json` on the bridge branch, so the updater sees 1.4.1 < 1.4.2 < 2.0.0.

`foundation` cannot be the bridge: its backend uses SurrealDB SDK 3.x (`backend/Cargo.toml`), which refuses any 2.x server, so a tag from `foundation` with the compose pin flipped back to 2.x crash-loops every install. The bridge is the latest v1.x tag plus only the files below, taken from `foundation`:

| File | Why |
|---|---|
| `scripts/upgrade-surreal-v3.sh` | the hook (new file) |
| `scripts/auto-update.sh` | do not copy the whole file: keep the v1 one and add the hook block (the `new_major`/`old_major` check that calls `upgrade-surreal-v3.sh`), the `BACKUP_ENCRYPTION_KEY` generation, and `docker compose pull backend frontend backup`. Its `ENCRYPTION_KEY_LEGACY_EMPTY` and `JWT_SECRET` blocks need the 3.x backend, leave them out |
| `backend/scripts/backup/Dockerfile`, `backend/scripts/backup/eunomia-backup.sh` | the hook takes its pre-upgrade backup with the NEW compose file's `backup` image and `SURREAL_BIN`; v1.x has no such image. On the bridge the Dockerfile takes the CLI from `surrealdb/surrealdb:v2.3` (its nightly backups run against the 2.x server, which a 3.x CLI cannot talk to), and its restore lets the field definitions a 2.x export repeats overwrite, so a 2.x dump re-imports |
| `docker-compose.yml` | only add the `backup` service and the `eunomia-backups` volume (copy those two blocks); keep the `surrealdb/surrealdb:v2.x` pin and everything else as in the v1 tag |
| `.github/workflows/release.yml` | build and publish the `backup` image (matrix `image: [backend, frontend, backup]`, context `./backend/scripts/backup`) |
| `scripts/tests/auto-update.test.sh`, `scripts/tests/backup.test.sh`, `scripts/tests/upgrade-surreal-v3*.test.sh` | optional, keeps the bridge testable |

```sh
git switch -c release/bridge "$(git tag -l 'v1.*' | sort -V | tail -1)"
git checkout foundation -- scripts/upgrade-surreal-v3.sh backend/scripts/backup
git checkout foundation -- .github/workflows/release.yml   # then diff it against the v1 one
# hand-merge: scripts/auto-update.sh and docker-compose.yml as described in the table
bash scripts/tests/auto-update.test.sh && bash scripts/tests/backup.test.sh
bash scripts/ci/check-release-order.sh <bridge tag>        # "does not pin SurrealDB 3.x" is expected
git tag vX.Y.Z && git push origin release/bridge vX.Y.Z
```

The 3.x release is then cut from `foundation`, whose previous tag (the bridge) carries the hook.

## Enforcement

`scripts/ci/check-release-order.sh <tag>` exits 1 when `<tag>` pins 3.x and the previous `v*` tag lacks `scripts/upgrade-surreal-v3.sh` or the call to it in `scripts/auto-update.sh`. It also exits 1 when the compose pin and the `surrealdb` SDK major in `backend/Cargo.toml` at that tag disagree (a 2.x pin over a 3.x backend, or the reverse), which is what a bridge cut from `foundation` would be. Run it before tagging, and as the first step of `.github/workflows/release.yml` (needs `fetch-depth: 0`):

```yaml
- uses: actions/checkout@v4
  with: { ref: "${{ env.TAG }}", fetch-depth: 0 }
- run: bash scripts/ci/check-release-order.sh "$TAG"
```

Test: `bash scripts/tests/check-release-order.test.sh`.
