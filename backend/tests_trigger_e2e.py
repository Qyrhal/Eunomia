"""Live trigger-path e2e (#49): real HTTP, real HMAC verification.

DeliveryTests / CronTickTests / ScheduleTests mock triggers.delivery.httpx.post,
proving only half the path. The other half: the POST body is signed with
X-Webhook-Signature-V2 in a way a real Hermes webhook adapter (mock_hermes.py)
actually accepts, and the digest / schedule payload on the wire is well-formed.

A real HTTP server identical to mock_hermes.py's handler (same signature checks,
same 401-on-bad-signature) is run in-process, and Eunomia fires against it over
real sockets via httpx. No mocking anywhere on the delivery side.
"""

import hashlib
import hmac
import json
import socket
import threading
import time
from datetime import timedelta
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from django.test import TestCase
from django.utils import timezone

from connectors.models import AppSettings
from tasks.models import Project, Task
from triggers.models import Trigger
from triggers.scheduler import plan_schedules, tick_crons
from triggers.tools import enable_builtins


class MockHermes:
    """The same contract mock_hermes.py enforces: V2 HMAC + fresh timestamp,
    else 401. Records each verified POST."""

    def __init__(self, secret: str):
        self.secret = secret
        self.requests: list[tuple[str, dict]] = []

    def start(self) -> int:
        sink = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *a):
                pass

            def do_POST(self):
                body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
                ts = self.headers.get("X-Webhook-Timestamp", "")
                sig = self.headers.get("X-Webhook-Signature-V2", "")
                expected = hmac.new(
                    sink.secret.encode(), f"{ts}.{body.decode()}".encode(), hashlib.sha256
                ).hexdigest()
                fresh = abs(time.time() - int(ts or 0)) < 300
                if hmac.compare_digest(sig, expected) and fresh:
                    sink.requests.append((self.path, json.loads(body)))
                    self.send_response(200)
                    self.end_headers()
                    self.wfile.write(b'{"received": true}')
                else:
                    self.send_response(401)
                    self.end_headers()
                    self.wfile.write(b'{"error": "bad signature"}')

        srv = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.port = srv.server_address[1]
        threading.Thread(target=srv.serve_forever, daemon=True).start()
        self._srv = srv
        return self.port

    def stop(self):
        if self._srv:
            self._srv.shutdown()


class TriggerE2ETests(TestCase):
    def setUp(self):
        self.hermes = MockHermes("hermes-shared-secret")
        port = self.hermes.start()
        self.addCleanup(self.hermes.stop)
        s = AppSettings.load()
        s.embedding_backend = AppSettings.EMBED_STUB
        s.hermes_webhook_url = f"http://127.0.0.1:{port}/webhooks"
        s.hermes_webhook_secret = "hermes-shared-secret"
        s.save()
        self.proj = Project.objects.create(name="Home")

    def make_task(self, title, due_at=None, remind_at=None, **kw):
        return Task.objects.create(project=self.proj, title=title,
                                   due_at=due_at, remind_at=remind_at, **kw)

    def _deliveries(self, trigger_key):
        return [(p, e) for p, e in self.hermes.requests if e["trigger"] == trigger_key]

    def test_enable_builtins_then_real_http_digest_delivery(self):
        # the exact path from #49 ask 2: enable with a route, then a tick fires
        # the digest over real HTTP to the route the builtins were wired with
        out = enable_builtins()
        self.assertTrue(out["enabled"]["daily_digest"]["enabled"])
        self.assertEqual(out["enabled"]["daily_digest"]["webhook_route"], "eunomia")
        Trigger.objects.filter(pk="daily_digest").update(spec={
            "cron": "* * * * *", "digest": True})  # every-minute cron = due on the first tick
        self.make_task("overdue", due_at=timezone.now() - timedelta(hours=2))
        self.make_task("done", due_at=timezone.now() - timedelta(hours=1), completed=True)

        tick_crons()
        deliveries = self._deliveries("daily_digest")
        self.assertEqual(len(deliveries), 1)
        path, event = deliveries[0]
        self.assertEqual(path, "/webhooks/eunomia")
        self.assertEqual(event["payload"]["kind"], "digest")
        self.assertEqual(event["payload"]["overdue_count"], 1)
        self.assertNotIn("done", [t["title"] for t in event["payload"]["due_today"] or []])
        # dedupe: a second tick within the same minute does not fire again
        tick_crons()
        self.assertEqual(len(self._deliveries("daily_digest")), 1)

    def test_schedule_trigger_delivers_over_real_http(self):
        enable_builtins()
        # task_due_soon spec: anchor task.due_at, offset_s = -86400 (-24h).
        # fire_at = due_at - 24h must land in (now - 1h, now], the hourly window,
        # so due_at = now + 23h lands fire_at exactly 1h ago. Add a few minutes of
        # margin: the test's due_at is fixed shortly before plan_schedules computes its
        # own "now", which would otherwise shave fire_at to just left of window_start.
        self.make_task("task due soon", due_at=timezone.now() + timedelta(hours=23, minutes=5))
        plan_schedules()
        deliveries = self._deliveries("task_due_soon")
        self.assertEqual(len(deliveries), 1)
        path, event = deliveries[0]
        self.assertEqual(path, "/webhooks/eunomia")
        self.assertEqual(event["kind"], "schedule")
        self.assertEqual(event["entity"]["type"], "schedule")
        self.assertIn("task due soon", event["entity"]["title"])
        self.assertIn("anchor_at", event["payload"])
        self.assertIn("fire_at", event["payload"])

    def test_cron_tick_delivers_digest_over_real_http(self):
        enable_builtins()
        Trigger.objects.filter(pk="daily_digest").update(spec={
            "cron": "* * * * *", "digest": True, "payload": {"kind": "digest"}})
        self.make_task("open one", due_at=timezone.now() + timedelta(days=5))
        self.make_task("late one", due_at=timezone.now() - timedelta(days=2))

        tick_crons()
        deliveries = self._deliveries("daily_digest")
        self.assertEqual(len(deliveries), 1)
        event = deliveries[0][1]
        self.assertEqual(event["payload"]["kind"], "digest")
        self.assertEqual(event["payload"]["overdue_count"], 1)
        self.assertEqual(event["payload"]["open_total"], 2)
        self.assertEqual(event["payload"]["due_today"], [])  # due in 5 days: not today

    def test_bad_signature_is_rejected_401(self):
        # the counterpart: a wrong secret dead-letters rather than succeeding
        s = AppSettings.load()
        s.hermes_webhook_secret = "wrong-secret-on-purpose"
        s.save()
        enable_builtins()
        Trigger.objects.filter(pk="daily_digest").update(spec={
            "cron": "* * * * *", "digest": True})
        self.make_task("t", due_at=timezone.now() - timedelta(hours=1))
        tick_crons()
        self.assertEqual(self.hermes.requests, [])  # never accepted
        from triggers.models import DeliveryLog
        self.assertTrue(DeliveryLog.objects.filter(dead=True).exists())
