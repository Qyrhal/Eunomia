import json
from types import SimpleNamespace

from app.config import settings
from entities.extract import extract_entities
from entities.service import get_entity, list_entities


class _FakeClient:
    def __init__(self, content: str):
        self._content = content
        self.chat = SimpleNamespace(completions=SimpleNamespace(create=self._create))

    async def _create(self, **kwargs):
        message = SimpleNamespace(content=self._content)
        return SimpleNamespace(choices=[SimpleNamespace(message=message)])


async def test_stub_mode_is_noop_and_never_calls_openai(monkeypatch, surreal_db, owner):
    monkeypatch.setattr(settings, "EMBEDDINGS_BACKEND", "stub")

    def _boom():
        raise AssertionError("OpenAI client must not be constructed in stub mode")

    monkeypatch.setattr("entities.extract._openai_client", _boom)

    await extract_entities(
        owner,
        {"id": "heypocket:transcript:t1", "title": "Call", "body_text": "Alex from Acme called about the project."},
    )

    assert await list_entities(owner) == []


async def test_short_body_is_noop(surreal_db, owner):
    # too short to be worth an LLM call -- short-circuits before touching
    # settings.EMBEDDINGS_BACKEND or the network at all.
    await extract_entities(owner, {"id": "heypocket:transcript:t2", "title": "hi", "body_text": "ok thanks"})
    assert await list_entities(owner) == []


async def test_llm_failure_is_swallowed(monkeypatch, surreal_db, owner):
    monkeypatch.setattr(settings, "EMBEDDINGS_BACKEND", "openai")

    def _raiser():
        raise RuntimeError("network down")

    monkeypatch.setattr("entities.extract._openai_client", _raiser)

    # must not raise
    await extract_entities(
        owner,
        {"id": "heypocket:transcript:t3", "title": "Call", "body_text": "Alex from Acme called about the project."},
    )
    assert await list_entities(owner) == []


async def test_happy_path_parses_response_and_upserts_through_service(monkeypatch, surreal_db, owner):
    monkeypatch.setattr(settings, "EMBEDDINGS_BACKEND", "openai")

    payload = {
        "people": [{"name": "Alex", "aliases": ["Al"], "facts": ["Works at Acme."]}],
        "organisations": [{"name": "Acme", "aliases": [], "facts": []}],
        "relations": [
            {"from": "Alex", "from_kind": "person", "to": "Acme", "to_kind": "organisation", "label": "works_at"}
        ],
    }
    fake = _FakeClient(json.dumps(payload))
    monkeypatch.setattr("entities.extract._openai_client", lambda: fake)

    await extract_entities(
        owner,
        {
            "id": "heypocket:transcript:t4",
            "title": "Call",
            "body_text": "Alex from Acme called about the project timeline.",
        },
    )

    people = await list_entities(owner, kind="person")
    orgs = await list_entities(owner, kind="organisation")
    assert len(people) == 1
    assert people[0]["name"] == "Alex"
    assert "Al" in people[0]["aliases"]
    assert len(orgs) == 1
    assert orgs[0]["name"] == "Acme"

    alex = await get_entity(owner, people[0]["id"])
    assert len(alex["memory"]) == 1
    assert alex["memory"][0]["text"] == "Works at Acme."
    assert len(alex["relations"]) == 1
    assert alex["relations"][0]["label"] == "works_at"
    assert alex["relations"][0]["direction"] == "out"
