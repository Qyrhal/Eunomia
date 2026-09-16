"""User-timezone handling for agent-facing task time I/O.

Timezone comes from AppSettings.user_timezone (an IANA name), set once at
deployment by the installing agent (the operator's zone, NOT the server's —
homelab boxes usually run UTC). Storage is always UTC (Django USE_TZ); this
module only governs how naive agent inputs are INTERPRETED and how datetimes
are PRESENTED back to the agent, so the agent never hand-converts.
"""

from django.utils import timezone as dj_tz


from datetime import timezone as dt_tz


def user_tz():
    """Resolve the configured user timezone as a tzinfo (UTC fallback)."""
    from connectors.models import AppSettings

    name = AppSettings.load().user_timezone
    if not name:
        return dt_tz.utc
    try:
        from zoneinfo import ZoneInfo

        return ZoneInfo(name)
    except Exception:
        return dj_tz.utc


def parse_user_datetime(value):
    """Coerce an agent-supplied date/datetime string into an aware datetime.

    Aware inputs (explicit offset or trailing Z) are honoured verbatim. Naive
    datetimes and bare dates are interpreted IN THE USER'S TIMEZONE, so
    "2026-09-08T17:00" means 5pm for the operator wherever the box lives.
    """
    if not value or not isinstance(value, str):
        return value or None
    from datetime import datetime, time

    from django.utils.dateparse import parse_date, parse_datetime

    dt = parse_datetime(value)
    if dt is None:
        d = parse_date(value)
        dt = datetime.combine(d, time()) if d else None
    if dt is None:
        return None
    if dj_tz.is_naive(dt):
        return dt.replace(tzinfo=user_tz())
    return dt


def local_fields(dt):
    """Render an aware datetime for agent consumption in the user's timezone.

    Returns {iso, local_iso, tz}: `iso` stays canonical UTC, `local_iso` is
    what the operator means, `tz` names the zone so the agent can reason
    about "today" without any conversion. None-safe for missing due dates.
    """
    if not dt:
        return None
    tz = user_tz()
    local = dt.astimezone(tz)
    return {
        "iso": dt.isoformat(),
        "local_iso": local.isoformat(),
        "tz": str(tz),
    }
