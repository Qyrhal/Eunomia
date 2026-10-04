import json

import respx
from httpx import Response

from app.config import settings
from connectors.crypto import encrypt
from sources.heypocket.source import HeyPocketSource

RECORDING = {
    "id": "rec-1",
    "title": "Weekly standup",
    "summary": "Talked about the roadmap.",
    "transcript": "We discussed...",
    "notes": "follow up with design",
    "duration": 1800,
    "recording_at": "2026-01-05T09:00:00+11:00",
    "created_at": "2026-01-04T09:00:00+11:00",
    "tags": [{"name": "work"}, {"name": "internal"}],
    "url": "https://heypocketai.com/r/1",
}


def test_map_recording():
    env = HeyPocketSource().map(RECORDING)
    assert env["id"] == "heypocket:heypocket.recording:rec-1"
    assert env["source"] == "heypocket"
    assert env["type"] == "heypocket.recording"
    assert env["title"] == "Weekly standup"
    assert "roadmap" in env["body_text"]
    assert env["payload"]["duration_seconds"] == 1800
    assert env["payload"]["tags"] == ["work", "internal"]
    assert env["url"] == "https://heypocketai.com/r/1"


def test_map_missing_id_skipped():
    assert HeyPocketSource().map({"title": "no id"}) is None


@respx.mock
async def test_sync_pulls_recordings(surreal_db, monkeypatch):
    monkeypatch.setattr(settings, "EMBEDDINGS_BACKEND", "stub")

    conn = surreal_db
    await conn.query(
        "CREATE connector SET kind = 'pocketai', enabled = true, credentials_encrypted = $enc",
        {"enc": encrypt(json.dumps({"api_key": "secret-key"}))},
    )

    respx.get("https://public.heypocketai.com/api/v1/public/recordings").mock(
        return_value=Response(200, json={"data": [RECORDING]})
    )

    src = HeyPocketSource()
    result = await src.sync("poll")

    assert len(result.records) == 1
    assert result.records[0]["id"] == "rec-1"
    assert result.cursor == "2026-01-05T09:00:00+11:00"
