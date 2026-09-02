import json
from datetime import timedelta
from unittest.mock import patch

import httpx
from django.test import TestCase
from django.utils import timezone

from cache.models import CacheRecord
from connectors.models import AppSettings

from .delivery import _sign, fire
from .models import DeliveryLog, Trigger
from .rules import evaluate_record
from .scheduler import plan_schedules
from .tools import create_trigger, ensure_builtins, test_trigger


def _rec(**kw):
    d = dict(id="up_bank:up.transaction:1", source="up_bank", type="up.transaction",
             external_id="1", title="Big buy", body_text="", payload={"amount_cents": -25000},
             content_hash="h", ingested_at=timezone.now(), updated_at=timezone.now())
    d.update(kw)
    return CacheRecord.objects.create(**d)


class _Resp:
    def __init__(self, status): self.status_code = status


class DeliveryTests(TestCase):
    def setUp(self):
        s = AppSettings.load()
        s.hermes_webhook_url = "http://hermes.local/webhooks"
        s.hermes_webhook_secret = "shh"
        s.save()
        self.trg = Trigger.objects.create(key="t1", kind="record_rule", spec={}, webhook_route="eunomia")

    def test_fire_signs_and_posts_and_logs(self):
        with patch("triggers.delivery.httpx.post", return_value=_Resp(200)) as p:
            ok = fire(self.trg, {"id": "e1", "type": "x"}, sleep=lambda s: None)
        self.assertTrue(ok)
        args, kw = p.call_args
        self.assertEqual(args[0], "http://hermes.local/webhooks/eunomia")
        body = kw["content"]
        ts = kw["headers"]["X-Webhook-Timestamp"]
        self.assertEqual(kw["headers"]["X-Webhook-Signature-V2"], _sign("shh", ts, body))
        self.assertTrue(DeliveryLog.objects.filter(trigger_key="t1", ok=True).exists())

    def test_retries_then_dead_letters(self):
        with patch("triggers.delivery.httpx.post", return_value=_Resp(502)):
            ok = fire(self.trg, {"id": "e2"}, sleep=lambda s: None)
        self.assertFalse(ok)
        self.assertTrue(DeliveryLog.objects.filter(trigger_key="t1", dead=True).exists())
        self.assertGreaterEqual(DeliveryLog.objects.filter(trigger_key="t1", attempt__gte=1).count(), 4)

    def test_dedupe_window_suppresses_repeat(self):
        with patch("triggers.delivery.httpx.post", return_value=_Resp(200)):
            self.assertTrue(fire(self.trg, {"id": "e3"}, sleep=lambda s: None))
            self.assertFalse(fire(self.trg, {"id": "e3"}, sleep=lambda s: None))

    def test_no_webhook_url_dead_letters(self):
        s = AppSettings.load(); s.hermes_webhook_url = ""; s.save()
        self.assertFalse(fire(self.trg, {"id": "e4"}, sleep=lambda s: None))
        self.assertTrue(DeliveryLog.objects.filter(dead=True, detail__icontains="not set").exists())


class RuleTests(TestCase):
    def setUp(self):
        s = AppSettings.load()
        s.hermes_webhook_url = "http://h/webhooks"
        s.save()

    def test_record_rule_matches_and_fires(self):
        Trigger.objects.create(key="big", kind="record_rule", enabled=True,
                               spec={"types": ["up.transaction"], "match": [["payload.amount_cents", "lt", -20000]]})
        with patch("triggers.delivery.httpx.post", return_value=_Resp(200)) as p:
            fired = evaluate_record(_rec())
        self.assertEqual(fired, ["big"])
        p.assert_called_once()

    def test_record_rule_no_match(self):
        Trigger.objects.create(key="big", kind="record_rule", enabled=True,
                               spec={"match": [["payload.amount_cents", "lt", -100000]]})
        with patch("triggers.delivery.httpx.post", return_value=_Resp(200)):
            self.assertEqual(evaluate_record(_rec()), [])

    def test_disabled_rule_skipped(self):
        Trigger.objects.create(key="big", kind="record_rule", enabled=False,
                               spec={"match": [["payload.amount_cents", "lt", 0]]})
        self.assertEqual(evaluate_record(_rec()), [])


class ScheduleTests(TestCase):
    def setUp(self):
        s = AppSettings.load()
        s.hermes_webhook_url = "http://h/webhooks"
        s.save()

    def test_plan_schedules_fires_for_task_in_window(self):
        from tasks.models import Project, Task

        proj = Project.objects.create(name="P")
        # due in ~23h -> with offset -86400 the fire time is ~1h ago -> inside the window
        Task.objects.create(project=proj, title="pay rent", due_at=timezone.now() + timedelta(hours=23, minutes=30))
        Trigger.objects.create(key="due", kind="schedule", enabled=True,
                               spec={"anchor": "task.due_at", "offset_s": -86400})
        with patch("triggers.delivery.httpx.post", return_value=_Resp(200)) as p:
            plan_schedules()
        p.assert_called_once()

    def test_plan_schedules_no_double_fire(self):
        from tasks.models import Project, Task

        proj = Project.objects.create(name="P")
        Task.objects.create(project=proj, title="x", due_at=timezone.now() + timedelta(hours=23, minutes=30))
        Trigger.objects.create(key="due", kind="schedule", enabled=True,
                               spec={"anchor": "task.due_at", "offset_s": -86400})
        with patch("triggers.delivery.httpx.post", return_value=_Resp(200)) as p:
            plan_schedules()
            plan_schedules()
        p.assert_called_once()


class ToolTests(TestCase):
    def test_create_list_test_and_builtins(self):
        AppSettings.load().save()
        create_trigger("mine", "record_rule", {"match": []})
        self.assertTrue(Trigger.objects.filter(pk="mine").exists())

        ensure_builtins()
        keys = set(Trigger.objects.values_list("key", flat=True))
        self.assertTrue({"big_transaction", "vip_email", "task_due_soon", "daily_digest"} <= keys)
        self.assertFalse(Trigger.objects.get(pk="daily_digest").enabled)

    def test_tools_registered(self):
        from tools.registry import all_tools

        for name in ("create_trigger", "list_triggers", "delete_trigger", "test_trigger"):
            self.assertIn(name, all_tools())
