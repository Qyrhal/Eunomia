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
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

SECRET = ""


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
        try:
            print(json.dumps(json.loads(body), indent=2))
        except ValueError:
            print(body.decode(errors="replace"))

        self.send_response(200 if (ok and fresh) else 401)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(b'{"received": true}' if ok else b'{"error": "bad signature"}')


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=8644)
    ap.add_argument("--secret", default="hermes-shared-secret")
    args = ap.parse_args()
    SECRET = args.secret
    print(f"mock Hermes gateway on http://127.0.0.1:{args.port}/webhooks  (secret: {args.secret!r})")
    ThreadingHTTPServer(("127.0.0.1", args.port), Handler).serve_forever()
