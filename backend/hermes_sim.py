"""A minimal real agent that stands in for Hermes.

It does what Hermes does with Eunomia: discover the tool catalogue, run an
OpenAI-compatible tool-calling loop against a real LLM, and answer a question by
calling Eunomia's tools. Used by tests_hermes_sim.py.

Config (env, from backend/.env — gitignored):
    SIM_LLM_BASE_URL   OpenAI-compatible base, e.g. https://opencode.ai/zen/go/v1
    SIM_LLM_API_KEY
    SIM_LLM_MODEL      e.g. kimi-k2.7-code

Tools can come from Eunomia's REST catalogue (default, needs the API running) or
be passed in directly (the tests inject the in-process registry to stay fast).
"""

import json
import os

import httpx

SYSTEM = (
    "You are a personal assistant with live access to the user's data through tools "
    "(mail, calendar, bank transactions, files, tasks, and watches). Answer from tool "
    "results only — never invent figures or events. Values like [eunomia:email:3] are "
    "masked handles; treat them as opaque and pass them back verbatim when a tool needs "
    "one. Be concise."
)


def _cfg():
    return (
        os.environ["SIM_LLM_BASE_URL"].rstrip("/"),
        os.environ["SIM_LLM_API_KEY"],
        os.environ.get("SIM_LLM_MODEL", "kimi-k2.7-code"),
    )


def rest_toolset(base_url: str, token: str = "") -> tuple[list[dict], callable]:
    """Pull the tool catalogue + a caller from a running Eunomia over REST."""
    h = {"Authorization": f"Bearer {token}"} if token else {}
    cat = httpx.get(f"{base_url}/api/tools", headers=h, timeout=20).json()
    schemas = [
        {"type": "function", "function": {"name": t["name"], "description": t["name"],
                                          "parameters": t["schema"]}}
        for t in cat
    ]

    def call(name, args):
        try:
            r = httpx.post(f"{base_url}/api/tools/{name}", headers=h, json=args, timeout=60)
            if r.status_code >= 400:
                return {"error": f"tool {name} returned HTTP {r.status_code}"}
            return r.json()
        except (httpx.HTTPError, ValueError) as e:
            return {"error": f"tool {name} call failed: {e}"}

    return schemas, call


def registry_toolset() -> tuple[list[dict], callable]:
    """In-process: use tools.registry directly (fast path for tests)."""
    from tools.registry import all_tools, call as _call

    reg = all_tools()
    schemas = [
        {"type": "function", "function": {"name": n, "description": n, "parameters": s["schema"]}}
        for n, s in reg.items()
    ]
    return schemas, lambda name, args: _call(name, args)


def run(question: str, schemas: list[dict], call, *, max_rounds: int = 6) -> dict:
    """Run the tool-calling loop. Returns {answer, tool_calls: [(name, args, result)], transcript}."""
    base_url, api_key, model = _cfg()
    client = httpx.Client(base_url=base_url, headers={"Authorization": f"Bearer {api_key}"}, timeout=90)
    messages = [{"role": "system", "content": SYSTEM}, {"role": "user", "content": question}]
    trace = []

    try:
        for _ in range(max_rounds):
            resp = client.post("/chat/completions", json={
                "model": model, "messages": messages, "tools": schemas,
            })
            if resp.status_code >= 400:
                raise RuntimeError(f"LLM {resp.status_code}: {resp.text[:400]}")
            body = resp.json()
            if "choices" not in body:
                return {"answer": f"(LLM error: {body})", "tool_calls": trace, "transcript": messages}
            msg = body["choices"][0]["message"]
            messages.append(msg)
            calls = msg.get("tool_calls") or []
            if not calls:
                return {"answer": msg.get("content") or "", "tool_calls": trace, "transcript": messages}
            for c in calls:
                name = c["function"]["name"]
                try:
                    args = json.loads(c["function"]["arguments"] or "{}")
                except json.JSONDecodeError:
                    args = {}
                result = call(name, args)
                trace.append((name, args, result))
                messages.append({"role": "tool", "tool_call_id": c["id"],
                                 "content": json.dumps(result, default=str)[:8000]})
        return {"answer": "(gave up: too many tool rounds)", "tool_calls": trace, "transcript": messages}
    finally:
        client.close()


if __name__ == "__main__":
    import sys

    base = os.environ.get("EUNOMIA_BASE", "http://127.0.0.1:8000")
    schemas, call = rest_toolset(base, os.environ.get("EUNOMIA_API_TOKEN", ""))
    q = " ".join(sys.argv[1:]) or "How are the financials looking?"
    out = run(q, schemas, call)
    print("Q:", q, "\n")
    for name, args, _ in out["tool_calls"]:
        print(f"  → {name}({json.dumps(args)})")
    print("\nA:", out["answer"])
