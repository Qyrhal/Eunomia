from datetime import datetime, timezone as dt_tz

from django.test import TestCase

_FIXED = datetime(2026, 1, 2, 3, 4, 5, tzinfo=dt_tz.utc)

from connectors.models import AppSettings
from embeddings.service import embed

from .models import CacheLink, CacheRecord
from .search import get, links, list_records, search, set_embedding, upsert


def env(id, title, body, **extra):
    return {
        "id": id,
        "source": extra.get("source", "up_bank"),
        "type": extra.get("type", "up.transaction"),
        "external_id": id.split(":")[-1],
        "title": title,
        "body_text": body,
        "occurred_at": extra.get("occurred_at", _FIXED),
        "url": extra.get("url", ""),
        "payload": extra.get("payload", {}),
        "links": extra.get("links", []),
        "deleted": extra.get("deleted", False),
    }


class UpsertTests(TestCase):
    def test_insert_then_idempotent(self):
        r1, changed1 = upsert(env("up_bank:up.transaction:1", "Coffee", "flat white at Ona"))
        r2, changed2 = upsert(env("up_bank:up.transaction:1", "Coffee", "flat white at Ona"))
        self.assertTrue(changed1)
        self.assertFalse(changed2)
        self.assertEqual(CacheRecord.objects.count(), 1)

    def test_change_updates_and_rewrites_fts(self):
        upsert(env("up_bank:up.transaction:2", "Old", "old body"))
        _, changed = upsert(env("up_bank:up.transaction:2", "New", "new body groceries"))
        self.assertTrue(changed)
        hits = search("groceries", mode="keyword")
        self.assertEqual([h.id for h in hits], ["up_bank:up.transaction:2"])
        self.assertEqual(search("old", mode="keyword"), [])

    def test_links_reconciled(self):
        upsert(env("a:x.n:1", "A", "a", type="x.n", source="a",
                   links=[{"rel": "about", "target": "b:y.m:9"}]))
        self.assertEqual(CacheLink.objects.filter(source_id="a:x.n:1", rel="about").count(), 1)
        # re-sync with no links clears the sync-origin edge
        upsert(env("a:x.n:1", "A", "a2", type="x.n", source="a", links=[]))
        self.assertEqual(CacheLink.objects.filter(source_id="a:x.n:1", origin="sync").count(), 0)

    def test_agent_links_survive_resync(self):
        upsert(env("a:x.n:1", "A", "a", type="x.n", source="a"))
        CacheLink.objects.create(source_id="a:x.n:1", rel="about", target_id="task:t1", origin="agent")
        upsert(env("a:x.n:1", "A", "a3", type="x.n", source="a"))
        self.assertEqual(CacheLink.objects.filter(origin="agent").count(), 1)


class SearchTests(TestCase):
    def setUp(self):
        s = AppSettings.load()
        s.embedding_backend = AppSettings.EMBED_STUB
        s.save()
        for i, (t, b) in enumerate([
            ("Rent payment", "monthly rent transfer to landlord"),
            ("Grocery shop", "woolworths weekly groceries and veg"),
            ("Salary", "fortnightly salary deposit from employer"),
        ]):
            rid = f"up_bank:up.transaction:{i}"
            upsert(env(rid, t, b))
            set_embedding(rid, embed([t + "\n" + b])[0])

    def test_keyword(self):
        hits = search("groceries", mode="keyword")
        self.assertIn("up_bank:up.transaction:1", [h.id for h in hits])

    def test_semantic_returns_something(self):
        hits = search("income", mode="semantic", limit=3)
        self.assertTrue(hits)

    def test_hybrid_merges(self):
        hits = search("rent", mode="hybrid", limit=3)
        self.assertIn("up_bank:up.transaction:0", [h.id for h in hits])

    def test_type_and_source_filter(self):
        self.assertEqual(search("rent", types=["gcal.event"]), [])
        self.assertTrue(search("rent", sources=["up_bank"]))

    def test_deleted_excluded(self):
        upsert(env("up_bank:up.transaction:0", "Rent payment", "monthly rent transfer to landlord", deleted=True))
        self.assertEqual(search("rent", mode="keyword"), [])


class GetListLinksTests(TestCase):
    def test_get_and_list(self):
        upsert(env("up_bank:up.transaction:1", "Coffee", "x"))
        upsert(env("g:gcal.event:5", "Standup", "y", source="g", type="gcal.event"))
        self.assertEqual(get("up_bank:up.transaction:1").title, "Coffee")
        self.assertIsNone(get("nope"))
        self.assertEqual([r.id for r in list_records(type="gcal.event")], ["g:gcal.event:5"])

    def test_links_both_directions(self):
        upsert(env("a:x.n:1", "A", "a", type="x.n", source="a",
                   links=[{"rel": "about", "target": "b:x.n:2"}]))
        upsert(env("b:x.n:2", "B", "b", type="x.n", source="b"))
        self.assertEqual(links("a:x.n:1"), [{"rel": "about", "direction": "out", "target_id": "b:x.n:2"}])
        self.assertEqual(links("b:x.n:2"), [{"rel": "about", "direction": "in", "target_id": "a:x.n:1"}])
