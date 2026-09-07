from unittest.mock import patch

from django.test import TestCase

from cache.models import CacheRecord
from cache.search import upsert
from connectors.models import AppSettings, Connector
from sources import registry
from sources.google_calendar.source import GoogleCalendarSource
from sources.google_drive.source import GoogleDriveSource
from sources.google_gmail.source import GmailSource

GMAIL_MSG = {
    "id": "m1", "threadId": "t1", "subject": "Picnic Saturday", "from": "sam@example.com",
    "to": "me@example.com", "snippet": "bring a blanket", "labelIds": ["INBOX", "UNREAD"],
    "internalDate": "1767225600000",
}
CAL_EVT = {
    "id": "e1", "summary": "Standup", "location": "Zoom", "description": "daily",
    "htmlLink": "https://cal/e1", "status": "confirmed",
    "start": {"dateTime": "2026-02-01T09:00:00Z"},
    "attendees": [{"email": "a@x.com"}, {"email": "b@x.com"}],
    "organizer": {"email": "a@x.com"},
}
DRIVE_FILE = {
    "id": "f1", "name": "Q2 Plan", "mimeType": "application/vnd.google-apps.document",
    "modifiedTime": "2026-01-20T12:00:00Z", "createdTime": "2026-01-01T00:00:00Z",
    "webViewLink": "https://docs/f1", "owners": [{"emailAddress": "me@x.com"}], "size": "2048",
}


class GoogleMapTests(TestCase):
    def test_gmail_map(self):
        env = GmailSource().map(GMAIL_MSG)
        self.assertEqual(env["id"], "google_gmail:gmail.message:m1")
        self.assertEqual(env["title"], "Picnic Saturday")
        self.assertTrue(env["payload"]["unread"])
        self.assertEqual(env["payload"]["from"], "sam@example.com")

    def test_calendar_map(self):
        env = GoogleCalendarSource().map(CAL_EVT)
        self.assertEqual(env["type"], "gcal.event")
        self.assertEqual(env["occurred_at"], "2026-02-01T09:00:00Z")
        self.assertEqual(env["payload"]["attendees"], ["a@x.com", "b@x.com"])

    def test_calendar_cancelled_is_deleted(self):
        self.assertTrue(GoogleCalendarSource().map(dict(CAL_EVT, status="cancelled"))["deleted"])

    def test_drive_map_is_metadata_only(self):
        env = GoogleDriveSource().map(DRIVE_FILE)
        self.assertEqual(env["type"], "gdrive.file")
        self.assertEqual(env["body_text"], "Q2 Plan")  # no body pulled
        self.assertEqual(env["payload"]["mime_type"], "application/vnd.google-apps.document")


class GoogleSyncTests(TestCase):
    def setUp(self):
        s = AppSettings.load(); s.embedding_backend = AppSettings.EMBED_STUB; s.save()
        c = Connector.objects.create(kind="google", enabled=True)
        c.credentials = {"token": "TOK", "refresh_token": "RTOK", "client_id": "cid", "client_secret": "csec"}
        c.save()
        for src in (GmailSource(), GoogleCalendarSource(), GoogleDriveSource()):
            registry.register(src)

    def test_three_google_sources_share_one_connector(self):
        keys = {s.key for s in registry.enabled()}
        self.assertTrue({"google_gmail", "google_calendar", "google_drive"} <= keys)

    def test_gmail_sync(self):
        with patch("connectors.clients.GoogleClient.gmail_list_detailed", return_value=[GMAIL_MSG]), \
             patch("sources._google.persist_refreshed_token"):
            report, _ = registry.run_sync("google_gmail")
        self.assertEqual(report.written, 1)
        self.assertTrue(CacheRecord.objects.filter(pk="google_gmail:gmail.message:m1").exists())

    def test_calendar_sync(self):
        with patch("connectors.clients.GoogleClient.calendar_events", return_value=[CAL_EVT]), \
             patch("sources._google.persist_refreshed_token"):
            report, _ = registry.run_sync("google_calendar")
        self.assertEqual(report.written, 1)

    def test_drive_sync(self):
        with patch("connectors.clients.GoogleClient.drive_files", return_value=[DRIVE_FILE]), \
             patch("sources._google.persist_refreshed_token"):
            report, cursor = registry.run_sync("google_drive")
        self.assertEqual(report.written, 1)
        self.assertEqual(cursor, "2026-01-20T12:00:00Z")



class DriveGetDocumentTests(TestCase):
    def setUp(self):
        c = Connector.objects.create(kind="google", enabled=True)
        c.credentials = {"token": "TOK", "refresh_token": "R", "client_id": "c", "client_secret": "s"}
        c.save()
        registry.register(GoogleDriveSource())
        upsert(GoogleDriveSource().map(DRIVE_FILE))

    def test_get_document_fetches(self):
        src = registry.get("google_drive")
        with patch("connectors.clients.GoogleClient.drive_file_text", return_value="secret plan, email ceo@corp.com"), \
             patch("sources._google.persist_refreshed_token"):
            out = src._get_document("google_drive:gdrive.file:f1")
        self.assertIn("secret plan", out["text"])

    def test_get_document_rejects_non_drive_id(self):
        self.assertIn("error", registry.get("google_drive")._get_document("up_bank:up.transaction:1"))
