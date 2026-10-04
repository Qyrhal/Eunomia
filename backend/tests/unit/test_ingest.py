from app.config import settings
from cache.ingest import ingest
from cache.search import get


async def test_ingest_happy_path(surreal_db, monkeypatch):
    monkeypatch.setattr(settings, "EMBEDDINGS_BACKEND", "stub")

    raw_records = [
        {"id": "1", "title": "First", "body": "First body"},
        {"id": "2", "title": "Second", "body": "Second body"},
    ]

    def map_fn(raw):
        return {
            "id": f"src:kind:{raw['id']}",
            "type": "kind",
            "external_id": raw["id"],
            "title": raw["title"],
            "body_text": raw["body"],
        }

    report = await ingest("src", raw_records, map_fn)

    assert report.source == "src"
    assert report.written == 2
    assert report.skipped == 0
    assert report.failed == 0
    assert report.errors == []

    rec = await get("src:kind:1")
    assert rec is not None
    assert rec.embedding is not None  # embedded during ingest


async def test_ingest_skips_unchanged_on_rerun(surreal_db, monkeypatch):
    monkeypatch.setattr(settings, "EMBEDDINGS_BACKEND", "stub")

    raw_records = [{"id": "1", "title": "First", "body": "First body"}]

    def map_fn(raw):
        return {
            "id": f"src:kind:{raw['id']}",
            "type": "kind",
            "external_id": raw["id"],
            "title": raw["title"],
            "body_text": raw["body"],
        }

    first = await ingest("src", raw_records, map_fn)
    assert first.written == 1

    second = await ingest("src", raw_records, map_fn)
    assert second.written == 0
    assert second.skipped == 1


async def test_ingest_mapper_none_is_skipped(surreal_db, monkeypatch):
    monkeypatch.setattr(settings, "EMBEDDINGS_BACKEND", "stub")

    raw_records = [{"id": "1", "drop": True}, {"id": "2", "drop": False}]

    def map_fn(raw):
        if raw["drop"]:
            return None
        return {"id": "src:kind:2", "type": "kind", "external_id": "2", "title": "t", "body_text": "b"}

    report = await ingest("src", raw_records, map_fn)
    assert report.skipped == 1
    assert report.written == 1


async def test_ingest_isolates_per_record_failure(surreal_db, monkeypatch):
    monkeypatch.setattr(settings, "EMBEDDINGS_BACKEND", "stub")

    raw_records = [{"id": "bad"}, {"id": "good"}]

    def map_fn(raw):
        if raw["id"] == "bad":
            raise RuntimeError("boom")
        return {"id": "src:kind:good", "type": "kind", "external_id": "good", "title": "t", "body_text": "b"}

    report = await ingest("src", raw_records, map_fn)

    assert report.failed == 1
    assert report.written == 1
    assert len(report.errors) == 1
    assert "boom" in report.errors[0]

    # the good record still made it through despite the bad one raising
    assert await get("src:kind:good") is not None
