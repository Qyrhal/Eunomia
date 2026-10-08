# Releasing

## The SurrealDB 3 bridge rule

"Update now" runs the updater script that is already loaded from the OLD release (`scripts/auto-update.sh` is read into memory before it checks out the new tag). A release that adds the SurrealDB 3 upgrade hook therefore cannot protect the install that applies it, only the next update.

So the move to SurrealDB 3 takes two releases, in order:

1. **Bridge release**: ships the new `scripts/auto-update.sh` and `scripts/upgrade-surreal-v3.sh` and still pins `surrealdb/surrealdb:v2.x`. Installs take it normally.
2. **3.x release**: pins `surrealdb/surrealdb:v3.x`. Tag it only after the bridge release has been out long enough for installs to have taken it. Installs that skip the bridge hit the crash loop described at the top of [upgrading-to-surrealdb-3.md](upgrading-to-surrealdb-3.md) and need the manual recovery command.

Version rule: the tag immediately before any 3.x-pinning tag (by `sort -V`) must already contain the hook. Never put the hook and the 3.x pin in the same tag.

## Enforcement

`scripts/ci/check-release-order.sh <tag>` exits 1 when `<tag>` pins 3.x and the previous `v*` tag lacks `scripts/upgrade-surreal-v3.sh` or the call to it in `scripts/auto-update.sh`. Run it before tagging, and as the first step of `.github/workflows/release.yml` (needs `fetch-depth: 0`):

```yaml
- uses: actions/checkout@v4
  with: { ref: "${{ env.TAG }}", fetch-depth: 0 }
- run: bash scripts/ci/check-release-order.sh "$TAG"
```

Test: `bash scripts/tests/check-release-order.test.sh`.
