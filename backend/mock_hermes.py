"""A stand-in for the Hermes gateway webhook adapter — for local testing.

    python mock_hermes.py [--port 8644] [--secret <shared secret>]

Accepts POST /webhooks/<route>, verifies the X-Webhook-Signature-V2 HMAC exactly
as real Hermes does, and prints each event. Point Eunomia's `hermes_webhook_url`
at http://127.0.0.1:8644/webhooks and set the same secret in Settings.
"""

import argparse
import hashlib
import hmac
import json
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

SECRET = ""
AGENT = None  # set to (eunomia_base, token) to actually reason about each event


def _run_agent(event: dict, base: str, token: str):
    """Hand a verified notification to a real LLM with Eunomia's tools — this is
    the bit that makes the mock behave like Hermes rather than just a sink."""
    try:
        import hermes_sim

        schemas, call = hermes_sim.rest_toolset(base, token)
        q = (
            f"You just received this notification from the user's data layer:\n"
            f"{json.dumps(event)}\n\n"
            f"Look into it with the tools and tell the user what happened in one line."
        )
        out = hermes_sim.run(q, schemas, call, max_rounds=5)
        print("  agent tools:", [n for n, _, _ in out["tool_calls"]])
        print("  agent says :", (out["answer"] or "").strip()[:300])
    except Exception as e:  # never let the agent break delivery ack
        print("  agent error:", e)


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a):  # quiet the default access log
        pass

    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        ts = self.headers.get("X-Webhook-Timestamp", "")
        sig = self.headers.get("X-Webhook-Signature-V2", "")
        expected = hmac.new(SECRET.encode(), f"{ts}.{body.decode()}".encode(), hashlib.sha256).hexdigest()
        ok = hmac.compare_digest(sig, expected)
        fresh = abs(time.time() - int(ts or 0)) < 300

        status = "OK" if (ok and fresh) else "REJECTED"
        print(f"\n[{status}] {self.path}   sig={'valid' if ok else 'BAD'} ts={'fresh' if fresh else 'stale'}")
        event = None
        try:
            event = json.loads(body)
            print(json.dumps(event, indent=2))
        except ValueError:
            print(body.decode(errors="replace"))

        if ok and fresh and AGENT and event:
            threading.Thread(target=_run_agent, args=(event, *AGENT), daemon=True).start()

        self.send_response(200 if (ok and fresh) else 401)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(b'{"received": true}' if ok else b'{"error": "bad signature"}')


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=8644)
    ap.add_argument("--secret", default="hermes-shared-secret")
    ap.add_argument("--agent", action="store_true",
                    help="reason about each event with a real LLM (needs SIM_LLM_* env)")
    ap.add_argument("--eunomia", default="http://127.0.0.1:8000")
    ap.add_argument("--token", default="", help="EUNOMIA_API_TOKEN, if set")
    args = ap.parse_args()
    SECRET = args.secret
    if args.agent:
        AGENT = (args.eunomia.rstrip("/"), args.token)
    print(f"mock Hermes gateway on http://127.0.0.1:{args.port}/webhooks  (secret: {args.secret!r})"
          f"{'  + agent' if args.agent else ''}")
    ThreadingHTTPServer(("127.0.0.1", args.port), Handler).serve_forever()
