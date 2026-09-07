"""Google Drive source (#11) — file metadata only. Body on demand (#41)."""

from django.utils import timezone

from sources._google import google_client, google_push_webhook, persist_refreshed_token
from sources.base import Source, SyncResult, ToolSpec


class GoogleDriveSource(Source):
    key = "google_drive"
    provider = "google"
    label = "Google Drive"
    record_types = ["gdrive.file"]
    auth_kind = "oauth"

    def sync(self, mode, cursor=None) -> SyncResult:
        client = google_client(self)
        q = "trashed = false"
        if cursor:
            q += f" and modifiedTime > '{cursor}'"
        files = client.drive_files(q, page_size=200)
        persist_refreshed_token(self, client)
        newest = max((f.get("modifiedTime") or "" for f in files), default=cursor or "")
        return SyncResult(records=files, cursor=newest or cursor or timezone.now().isoformat())

    def map(self, raw: dict) -> dict | None:
        if not raw.get("id"):
            return None
        owners = [o.get("emailAddress") for o in raw.get("owners", []) if o.get("emailAddress")]
        return {
            "id": f"google_drive:gdrive.file:{raw['id']}",
            "source": "google_drive",
            "type": "gdrive.file",
            "external_id": raw["id"],
            "title": raw.get("name", ""),
            "body_text": raw.get("name", ""),  # metadata only; get_document (#41) fetches the body
            "occurred_at": raw.get("modifiedTime"),
            "url": raw.get("webViewLink", ""),
            "payload": {
                "mime_type": raw.get("mimeType", ""),
                "owners": owners,
                "size": raw.get("size"),
                "created_time": raw.get("createdTime"),
            },
            "links": [],
            "deleted": False,
        }

    def tools(self) -> list[ToolSpec]:
        return [
            ToolSpec(
                name="get_document",
                schema={"type": "object", "properties": {"record_id": {"type": "string"}},
                        "required": ["record_id"]},
                impl=self._get_document,
            )
        ]

    def _get_document(self, record_id: str) -> dict:
        from cache.search import get

        rec = get(record_id)
        if not rec or rec.source != "google_drive":
            return {"error": "not a drive file in the cache"}
        client = google_client(self)
        try:
            text = client.drive_file_text(rec.external_id, rec.payload.get("mime_type", ""))
        except Exception as e:
            return {"error": f"fetch failed: {e}"}
        persist_refreshed_token(self, client)
        return {"id": record_id, "title": rec.title, "text": text[:20000]}

    def webhook(self, request):
        return google_push_webhook(self, request)
