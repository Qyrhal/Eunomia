"""LLM entity-extraction pass, run as `cache/ingest.py`'s 4th stage.

Pulls person/organisation/location mentions, facts, and relations out of a
cache record's text via an OpenAI chat completion (JSON mode), then upserts
them through `entities.service`. Best-effort, enrichment only: this must
never raise -- a failure here (bad LLM output, network error, OpenAI outage)
is logged and swallowed, same spirit as "embedding failure is non-fatal" in
`cache/ingest.py`'s docstring.

Stub mode: there is no separate `EUNOMIA_EXTRACTION_BACKEND` env var.
`settings.EMBEDDINGS_BACKEND == "stub"` (see `embeddings/service.py`) is
reused as the one signal for "no real network calls in tests" -- when set,
`extract_entities` is a no-op (skips straight through, creates nothing).
Tests exercise the real parse-and-upsert path separately by mocking the
OpenAI client directly (see `tests/unit/test_entities_extract.py`), so the
happy path is still covered without inventing a second fake backend.
"""

import json
import logging

from app.config import settings

log = logging.getLogger("eunomia.entities.extract")

# body_text shorter than this has nothing worth extracting -- skip the LLM
# call entirely rather than spend tokens on "ok", "thanks", etc.
_MIN_BODY_LEN = 40

_KIND_MAP = {"people": "person", "organisations": "organisation", "locations": "location"}

_PROMPT = """Extract entities and relations mentioned in the text below. \
Return strict JSON, no prose, with this exact shape:

{{
  "people": [{{"name": str, "aliases": [str], "facts": [str]}}],
  "organisations": [{{"name": str, "aliases": [str], "facts": [str]}}],
  "locations": [{{"name": str, "aliases": [str], "facts": [str]}}],
  "relations": [{{"from": str, "from_kind": "person|organisation|location", \
"to": str, "to_kind": "person|organisation|location", "label": str}}]
}}

Only include entities actually mentioned in the text. Omit a section/field \
entirely if nothing was found for it, rather than inventing filler.

Text:
{text}
"""

_client = None


def _openai_client():
    global _client
    if _client is None:
        from openai import AsyncOpenAI

        _client = AsyncOpenAI(api_key=settings.OPENAI_API_KEY)
    return _client


async def _call_llm(text: str) -> dict:
    client = _openai_client()
    resp = await client.chat.completions.create(
        model="gpt-4o-mini",
        response_format={"type": "json_object"},
        messages=[{"role": "user", "content": _PROMPT.format(text=text)}],
    )
    return json.loads(resp.choices[0].message.content)


async def _apply(owner, source_record_id: str, data: dict) -> None:
    from entities.service import add_memory, add_relation, upsert_entity

    entity_ids: dict[str, dict] = {}  # name.lower() -> {"id": RecordID, "kind": str}

    async def _ensure(name: str, kind: str, aliases: list[str] | None = None) -> dict:
        key = name.lower()
        if key in entity_ids:
            return entity_ids[key]
        entity = await upsert_entity(owner, kind, name, aliases or [])
        entry = {"id": entity["id"], "kind": kind}
        entity_ids[key] = entry
        return entry

    for section, kind in _KIND_MAP.items():
        for item in data.get(section) or []:
            name = (item.get("name") or "").strip()
            if not name:
                continue
            entry = await _ensure(name, kind, item.get("aliases"))
            for fact in item.get("facts") or []:
                if fact:
                    await add_memory(owner, entry["id"], fact, source_record_id)

    for rel in data.get("relations") or []:
        from_name = (rel.get("from") or "").strip()
        to_name = (rel.get("to") or "").strip()
        from_kind = rel.get("from_kind")
        to_kind = rel.get("to_kind")
        if not from_name or not to_name:
            continue
        if from_kind not in _KIND_MAP.values() or to_kind not in _KIND_MAP.values():
            continue
        from_entry = await _ensure(from_name, from_kind)
        to_entry = await _ensure(to_name, to_kind)
        await add_relation(owner, from_entry["id"], to_entry["id"], rel.get("label") or "", source_record_id)


async def extract_entities(owner, record: dict) -> None:
    """Best-effort entity extraction + upsert for one ingested cache record
    (`record` has `id`, `title`, `body_text`). Never raises."""
    body = (record.get("body_text") or "").strip()
    if len(body) < _MIN_BODY_LEN:
        return
    if settings.EMBEDDINGS_BACKEND == "stub":
        return

    title = (record.get("title") or "").strip()
    text = f"{title}\n{body}".strip()

    try:
        data = await _call_llm(text)
    except Exception as e:
        log.warning("entity extraction LLM call failed for %s: %s", record.get("id"), e)
        return

    try:
        await _apply(owner, record["id"], data)
    except Exception as e:
        log.warning("entity extraction upsert failed for %s: %s", record.get("id"), e)
