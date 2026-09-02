"""Deliver a trigger event to the Hermes gateway webhook adapter (#2/#9).

POST {hermes_webhook_url}/{route} with the generic V2 HMAC scheme:
  X-Webhook-Timestamp: <unix seconds>
  X-Webhook-Signature-V2: hex HMAC_SHA256(secret, f"{ts}.{body}")
"""

import hashlib
import hmac
import json
import time

import httpx
from django.utils import timezone

from .models import DeliveryLog, Trigger

# attempt 1 immediate, then short retries. Kept tight because rule-triggered
# deliveries run inline in the ingest path (#31 stage 7) — a bulk sync that
# matches many records must not stall for minutes when Hermes is briefly down.
_RETRY_BACKOFF = [0, 3, 10]


def _sign(secret: str, ts: str, body: str) -> str:
    return hmac.new(secret.encode(), f"{ts}.{body}".encode(), hashlib.sha256).hexdigest()


def _recent_fire(trigger: Trigger, entity_id: str) -> bool:
    if trigger.dedupe_window_s <= 0:
        return False
    cutoff = timezone.now() - timezone.timedelta(seconds=trigger.dedupe_window_s)
    return DeliveryLog.objects.filter(
        trigger_key=trigger.key, entity_id=entity_id, ok=True, created_at__gte=cutoff
    ).exists()


def fire(trigger: Trigger, entity: dict, payload: dict | None = None, *, sleep=time.sleep) -> bool:
    """Deliver one event. Returns True on a 2xx. Records every attempt."""
    from connectors.models import AppSettings

    entity_id = entity.get("id", "")
    if _recent_fire(trigger, entity_id):
        return False

    cfg = AppSettings.load()
    base = (cfg.hermes_webhook_url or "").rstrip("/")
    route = trigger.webhook_route.strip("/")
    secret = cfg.hermes_webhook_secret
    if not base:
        DeliveryLog.objects.create(trigger_key=trigger.key, entity_id=entity_id, ok=False,
                                   dead=True, detail="hermes_webhook_url not set")
        return False

    body = json.dumps({
        "trigger": trigger.key, "kind": trigger.kind, "entity": entity,
        "matched_at": timezone.now().isoformat(), "payload": payload or {},
    }, default=str)
    url = f"{base}/{route}" if route else base

    for attempt, wait in enumerate(_RETRY_BACKOFF, start=1):
        if wait:
            sleep(wait)
        ts = str(int(time.time()))
        try:
            r = httpx.post(url, content=body, timeout=15, headers={
                "Content-Type": "application/json",
                "X-Webhook-Timestamp": ts,
                "X-Webhook-Signature-V2": _sign(secret, ts, body),
            })
            ok = 200 <= r.status_code < 300
            DeliveryLog.objects.create(trigger_key=trigger.key, entity_id=entity_id,
                                       attempt=attempt, http_status=r.status_code, ok=ok)
            if ok:
                Trigger.objects.filter(pk=trigger.pk).update(
                    last_fired_at=timezone.now(), fire_count=trigger.fire_count + 1)
                return True
        except (httpx.ConnectError, httpx.ConnectTimeout) as e:
            # host isn't answering — retrying inline is pointless, dead-letter now
            DeliveryLog.objects.create(trigger_key=trigger.key, entity_id=entity_id,
                                       attempt=attempt, ok=False, detail=str(e)[:300])
            break
        except httpx.HTTPError as e:
            DeliveryLog.objects.create(trigger_key=trigger.key, entity_id=entity_id,
                                       attempt=attempt, ok=False, detail=str(e)[:300])

    DeliveryLog.objects.filter(trigger_key=trigger.key, entity_id=entity_id).order_by("-id").first()
    DeliveryLog.objects.create(trigger_key=trigger.key, entity_id=entity_id, ok=False,
                               dead=True, detail="gave up after retries")
    return False
