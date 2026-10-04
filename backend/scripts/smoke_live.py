"""Smoke-check a running Eunomia over its REST surface, against a live
docker-compose stack.

    python scripts/smoke_live.py [--base http://127.0.0.1:8001]
                                  [--email you@example.com --password ...]

Registers a throwaway test user (or logs in with EUNOMIA_SMOKE_EMAIL /
EUNOMIA_SMOKE_PASSWORD / --email / --password, so the same account can be
reused across runs), then checks: the tool catalogue loads, `/api/auth/me`
resolves, `/api/tools/search` runs with a trivial query without 500ing,
`/api/sources` and `/api/entities/graph` return, and no response body
contains an obvious leaked secret (same regex approach as the old Django
`smoke.py` -- see `git show 1825e11:backend/smoke.py`). Exit 0 = ok.
"""

import argparse
import os
import re
import secrets
import sys

import httpx

SECRET_RE = re.compile(r"\b[\w.+-]+@[\w-]+\.\w+\b|\b\d{13,19}\b")


def _scan(label: str, body: object) -> int:
    """Return 1 and print to stderr if `body`'s JSON text looks like it
    contains a leaked secret (email address, card/account-number-shaped
    digit run); 0 otherwise."""
    text = body if isinstance(body, str) else repr(body)
    leak = SECRET_RE.search(text)
    if leak:
        print(f"LEAK: {leak.group(0)!r} in {label}", file=sys.stderr)
        return 1
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", default="http://127.0.0.1:8001")
    ap.add_argument("--email", default=os.environ.get("EUNOMIA_SMOKE_EMAIL", ""))
    ap.add_argument("--password", default=os.environ.get("EUNOMIA_SMOKE_PASSWORD", ""))
    a = ap.parse_args()

    email = a.email or f"smoke-{secrets.token_hex(6)}@example.com"
    password = a.password or secrets.token_urlsafe(16)

    c = httpx.Client(base_url=a.base, timeout=15)
    leaks = 0

    creds = {"email": email, "password": password}
    # NB: auth/register, auth/login and auth/me legitimately echo the
    # caller's own email back -- that's not a leak, so they're exempt from
    # the secret-leak scan below (which covers data endpoints where no
    # caller-identifying or credential info should ever appear).
    res = c.post("/api/auth/register", json=creds)
    if res.status_code == 409:
        # already exists (reused via env vars) -- log in instead
        res = c.post("/api/auth/login", json=creds)
    res.raise_for_status()
    print(f"ok: authenticated as {email}")

    me = c.get("/api/auth/me").raise_for_status().json()
    assert me.get("email") == email, me
    print("ok: auth/me")

    cat = c.get("/api/tools").raise_for_status().json()
    leaks += _scan("tools catalogue", cat)
    assert {"search", "get", "list", "links"} <= set(cat), cat
    print(f"ok: {len(cat)} tools")

    res = c.post("/api/tools/search", json={"query": "a", "limit": 5})
    res.raise_for_status()
    hits = res.json()
    leaks += _scan("tools/search", hits)
    print(f"ok: search -> {len(hits.get('results', []))} hits")

    sources = c.get("/api/sources").raise_for_status().json()
    leaks += _scan("sources", sources)
    print(f"ok: {len(sources)} sources")

    graph = c.get("/api/entities/graph").raise_for_status().json()
    leaks += _scan("entities/graph", graph)
    print("ok: entities/graph")

    if leaks:
        print(f"smoke FAILED: {leaks} leak(s) found", file=sys.stderr)
        return 1

    print("smoke passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
