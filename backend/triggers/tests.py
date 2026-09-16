import json
from datetime import timedelta
from unittest.mock import patch

import httpx
from django.test import TestCase
from django.utils import timezone

from cache.models import CacheRecord
from connectors.models import AppSettings

from .delivery import _sign, fire
from .digest import build_digest
from .models import DeliveryLog, Trigger
from .rules import evaluate_record
from .scheduler import plan_schedules, tick_crons
from .tools import create_trigger, enable_builtins, ensure_builtins, test_trigger


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

    def test_plan_schedules_remind_at_anchor(self):
        from tasks.models import Project, Task

        proj = Project.objects.create(name="P")
        # remind in ~23h -> fire time ~1h ago -> inside the window
        Task.objects.create(project=proj, title="call mum", remind_at=timezone.now() + timedelta(hours=23, minutes=30))
        Trigger.objects.create(key="rem", kind="schedule", enabled=True,
                               spec={"anchor": "task.remind_at", "offset_s": -86400})
        with patch("triggers.delivery.httpx.post", return_value=_Resp(200)) as p:
            plan_schedules()
        p.assert_called_once()
        body = json.loads(p.call_args.kwargs["content"])
        self.assertEqual(body["entity"]["id"], f"task:{Task.objects.get(title='call mum').id}")

    def test_plan_schedules_unknown_anchor_ignored(self):
        Trigger.objects.create(key="bad", kind="schedule", enabled=True,
                               spec={"anchor": "task.nope", "offset_s": 0})
        with patch("triggers.delivery.httpx.post", return_value=_Resp(200)) as p:
            plan_schedules()
        p.assert_not_called()


class DigestTests(TestCase):
    def test_build_digest_shape_and_counts(self):
        from tasks.models import Project, Task

        home = Project.objects.create(name="Home")
        work = Project.objects.create(name="Work")
        # end of the local day: always inside today's window and always in the future
        local_day_end = timezone.localtime(timezone.now()).replace(
            hour=0, minute=0, second=0, microsecond=0) + timedelta(days=1)

        # overdue (open), day-anchored due/reminder pairs, a future due, and a completed one
        Task.objects.create(project=home, title="overdue", due_at=timezone.now() - timedelta(days=2))
        Task.objects.create(project=home, title="due a", due_at=local_day_end - timedelta(seconds=2), priority=2)
        Task.objects.create(project=home, title="due b", due_at=local_day_end - timedelta(seconds=1))
        Task.objects.create(project=home, title="future", due_at=local_day_end + timedelta(days=3))
        Task.objects.create(project=home, title="reminder", remind_at=local_day_end - timedelta(seconds=1), flagged=True)
        Task.objects.create(project=work, title="work overdue", due_at=timezone.now() - timedelta(days=2))
        Task.objects.create(project=home, title="done", due_at=timezone.now() - timedelta(hours=3), completed=True)

        d = build_digest()

        self.assertEqual(d["kind"], "digest")
        self.assertEqual(d["open_total"], 6)          # the completed one is excluded
        self.assertEqual(d["overdue_count"], 2)
        self.assertEqual([t["title"] for t in d["due_today"]], ["due a", "due b"])
        self.assertEqual([t["title"] for t in d["reminders_today"]], ["reminder"])
        self.assertEqual(d["due_today"][0]["priority"], 2)
        self.assertEqual(d["reminders_today"][0]["flagged"], True)
        by_proj = {row["project"]: row for row in d["by_project"]}
        self.assertEqual(by_proj["Home"], {"project": "Home", "open_count": 5, "overdue_count": 1})
        self.assertEqual(by_proj["Work"], {"project": "Work", "open_count": 1, "overdue_count": 1})
        self.assertNotIn("done", json.dumps(d))


class CronTickTests(TestCase):
    def setUp(self):
        s = AppSettings.load()
        s.hermes_webhook_url = "http://h/webhooks"
        s.save()

    def test_tick_fires_due_cron_with_digest_payload(self):
        from tasks.models import Project, Task

        proj = Project.objects.create(name="P")
        Task.objects.create(project=proj, title="t", due_at=timezone.now() - timedelta(hours=2))
        Trigger.objects.create(key="cron_t", kind="cron", enabled=True,
                               webhook_route="eunomia", spec={"cron": "* * * * *", "digest": True})
        with patch("triggers.delivery.httpx.post", return_value=_Resp(200)) as p:
            tick_crons()
        p.assert_called_once()
        body = json.loads(p.call_args.kwargs["content"])
        self.assertEqual(body["payload"]["kind"], "digest")
        self.assertEqual(body["payload"]["overdue_count"], 1)
        self.assertEqual(body["entity"]["type"], "cron")

    def test_tick_non_digest_cron_uses_spec_payload(self):
        Trigger.objects.create(key="cron_plain", kind="cron", enabled=True,
                               spec={"cron": "* * * * *", "payload": {"hello": 1}})
        with patch("triggers.delivery.httpx.post", return_value=_Resp(200)) as p:
            tick_crons()
        p.assert_called_once()
        self.assertEqual(json.loads(p.call_args.kwargs["content"])["payload"], {"hello": 1})

    def test_tick_skips_not_due_and_disabled_crons(self):
        Trigger.objects.create(key="yearly", kind="cron", enabled=True, spec={"cron": "0 0 1 1 *"})
        Trigger.objects.create(key="off", kind="cron", enabled=False, spec={"cron": "* * * * *"})
        with patch("triggers.delivery.httpx.post", return_value=_Resp(200)) as p:
            tick_crons()
        p.assert_not_called()

    def test_tick_no_double_fire_same_minute(self):
        Trigger.objects.create(key="cron_t", kind="cron", enabled=True, spec={"cron": "* * * * *"})
        with patch("triggers.delivery.httpx.post", return_value=_Resp(200)) as p:
            tick_crons()
            tick_crons()
        p.assert_called_once()


class BuiltinEnableTests(TestCase):
    def test_enable_builtins_creates_enables_and_sets_route(self):
        enable_builtins()
        due = Trigger.objects.get(pk="task_due_soon")
        dig = Trigger.objects.get(pk="daily_digest")
        self.assertTrue(due.enabled and dig.enabled)
        self.assertEqual(due.webhook_route, "eunomia")
        self.assertEqual(dig.webhook_route, "eunomia")

    def test_enable_builtins_idempotent_and_preserves_route(self):
        enable_builtins()
        Trigger.objects.filter(pk="daily_digest").update(webhook_route="custom")
        again = enable_builtins()
        self.assertEqual(Trigger.objects.get(pk="daily_digest").webhook_route, "custom")
        self.assertEqual(Trigger.objects.get(pk="task_due_soon").webhook_route, "eunomia")
        self.assertTrue(again["enabled"]["daily_digest"]["enabled"])

    def test_enable_builtins_custom_route(self):
        enable_builtins(webhook_route="agents/eunomia")
        self.assertEqual(Trigger.objects.get(pk="task_due_soon").webhook_route, "agents/eunomia")

    def test_ensure_builtins_backfills_missing_route(self):
        ensure_builtins()
        Trigger.objects.filter(pk="daily_digest").update(webhook_route="")
        ensure_builtins()
        self.assertEqual(Trigger.objects.get(pk="daily_digest").webhook_route, "eunomia")
        # still disabled: backfill must not silently turn triggers on
        self.assertFalse(Trigger.objects.get(pk="daily_digest").enabled)


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
