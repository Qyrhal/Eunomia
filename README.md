# Eunomia

Personal work/uni/business dashboard. Reads an Obsidian vault off disk, buckets notes
by frontmatter tag / folder / inline hashtag, serves a dashboard.

## Run

```
python3 -m venv .venv
.venv/bin/pip install -e ".[dev]"
EUNOMIA_VAULT_PATH=/path/to/vault .venv/bin/uvicorn eunomia.api:app --reload
```

Then `POST /sync` to scan the vault, `GET /` for the dashboard, `GET /api/notes` for JSON.

## Test

```
.venv/bin/pytest
```

## Status

Step 1 of the plan: vault reader + rule classifier + SQLite + dashboard. No LLM fallback,
no Slack/Linear/GitHub/HeyPocket/Reminders integrations yet — those are next.
