"""Smoke-check a running Eunomia over its REST surface (#45).

    python smoke.py [--base http://127.0.0.1:8000] [--token $EUNOMIA_API_TOKEN]

Checks the tool catalogue loads, search/list run, and no obvious secret pattern
appears in a search response. Exit 0 = ok. This is the live-instance companion to
tests_smoke.py (which covers the full ingest -> trigger -> webhook path in-process).
"""

import argparse
import re
import sys

import httpx

SECRET_RE = re.compile(r"\b[\w.+-]+@[\w-]+\.\w+\b|\b\d{13,19}\b")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", default="http://127.0.0.1:8000")
    ap.add_argument("--token", default="")
    a = ap.parse_args()
    h = {"Authorization": f"Bearer {a.token}"} if a.token else {}
    c = httpx.Client(base_url=a.base, headers=h, timeout=15)

    cat = c.get("/api/tools").raise_for_status().json()
    names = {t["name"] for t in cat}
    assert {"search", "get", "list", "links"} <= names, names
    print(f"ok: {len(cat)} tools")

    res = c.post("/api/tools/search", json={"query": "the", "limit": 5}).raise_for_status().json()
    hits = res.get("results", [])
    print(f"ok: search -> {len(hits)} hits")
    for hcell in hits:
        leak = SECRET_RE.search(f"{hcell.get('title', '')} {hcell.get('snippet', '')}")
        if leak:
            print(f"LEAK: {leak.group(0)!r} in {hcell['id']}", file=sys.stderr)
            return 1

    c.post("/api/tools/list", json={"type": "up.transaction", "limit": 3}).raise_for_status()
    print("ok: list")
    print("smoke passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
