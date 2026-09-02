"""Shared OpenAI-compatible tool-calling loop, used by both the chat endpoint
and the task-description generator."""

import json

import httpx

from connectors.models import AppSettings

from .tools import TOOL_IMPLS, TOOL_SCHEMAS

MAX_TOOL_ROUNDS = 5


class ToolLoopError(Exception):
    def __init__(self, detail: str, status: int = 502):
        self.detail = detail
        self.status = status


def run_tool_loop(
    system_prompt: str,
    messages: list[dict],
    tool_schemas: list[dict] | None = None,
    tool_impls: dict | None = None,
) -> tuple[dict, list[dict]]:
    """Runs `messages` (sans system prompt) through the configured LLM, letting it
    call tools up to MAX_TOOL_ROUNDS times. Returns (final_assistant_message, tool_trace).

    `tool_schemas`/`tool_impls` default to the full tool set; pass a restricted
    subset to keep a caller (e.g. drafting a task description) read-only."""
    tool_schemas = TOOL_SCHEMAS if tool_schemas is None else tool_schemas
    tool_impls = TOOL_IMPLS if tool_impls is None else tool_impls

    settings_row = AppSettings.load()
    if not settings_row.llm_base_url or not settings_row.llm_api_key:
        raise ToolLoopError("Set an LLM endpoint and API key in Settings first.", status=400)

    full_messages = [{"role": "system", "content": system_prompt}] + messages

    client = httpx.Client(
        base_url=settings_row.llm_base_url.rstrip("/"),
        headers={"Authorization": f"Bearer {settings_row.llm_api_key}"},
        timeout=60,
    )

    tool_trace: list[dict] = []
    try:
        for _ in range(MAX_TOOL_ROUNDS):
            resp = client.post(
                "/chat/completions",
                json={
                    "model": settings_row.llm_model,
                    "messages": full_messages,
                    "tools": tool_schemas,
                },
            )
            resp.raise_for_status()
            choice = resp.json()["choices"][0]["message"]
            full_messages.append(choice)

            tool_calls = choice.get("tool_calls") or []
            if not tool_calls:
                return choice, tool_trace

            for call in tool_calls:
                name = call["function"]["name"]
                try:
                    args = json.loads(call["function"]["arguments"] or "{}")
                except json.JSONDecodeError:
                    args = {}
                impl = tool_impls.get(name)
                result = impl(**args) if impl else {"error": f"unknown tool {name}"}
                tool_trace.append({"tool": name, "args": args, "result": result})
                full_messages.append(
                    {
                        "role": "tool",
                        "tool_call_id": call["id"],
                        "content": json.dumps(result, default=str),
                    }
                )
        raise ToolLoopError("Too many tool-call rounds without a final answer.")
    except httpx.HTTPStatusError as exc:
        raise ToolLoopError(f"LLM endpoint error: {exc.response.status_code} {exc.response.text}") from exc
    except httpx.HTTPError as exc:
        raise ToolLoopError(f"Could not reach LLM endpoint: {exc}") from exc
    finally:
        client.close()


def stream_tool_loop(system_prompt, messages, tool_schemas=None, tool_impls=None):
    """Generator version of run_tool_loop: resolves tool-call rounds the same way,
    but streams the model's token deltas as they arrive on the final (no-tool-call)
    round. Yields dicts: {"type": "token", "content": str}, {"type": "tool", ...},
    {"type": "error", "detail": str}, or {"type": "done"}."""
    tool_schemas = TOOL_SCHEMAS if tool_schemas is None else tool_schemas
    tool_impls = TOOL_IMPLS if tool_impls is None else tool_impls

    settings_row = AppSettings.load()
    if not settings_row.llm_base_url or not settings_row.llm_api_key:
        yield {"type": "error", "detail": "Set an LLM endpoint and API key in Settings first."}
        return

    full_messages = [{"role": "system", "content": system_prompt}] + messages

    client = httpx.Client(
        base_url=settings_row.llm_base_url.rstrip("/"),
        headers={"Authorization": f"Bearer {settings_row.llm_api_key}"},
        timeout=60,
    )

    try:
        for _ in range(MAX_TOOL_ROUNDS):
            content = ""
            tool_calls: dict[int, dict] = {}
            with client.stream(
                "POST",
                "/chat/completions",
                json={
                    "model": settings_row.llm_model,
                    "messages": full_messages,
                    "tools": tool_schemas,
                    "stream": True,
                },
            ) as resp:
                resp.raise_for_status()
                for line in resp.iter_lines():
                    if not line.startswith("data: "):
                        continue
                    payload = line[len("data: "):]
                    if payload == "[DONE]":
                        break
                    delta = json.loads(payload)["choices"][0]["delta"]
                    if delta.get("content"):
                        content += delta["content"]
                        yield {"type": "token", "content": delta["content"]}
                    for tc in delta.get("tool_calls") or []:
                        slot = tool_calls.setdefault(
                            tc["index"], {"id": "", "function": {"name": "", "arguments": ""}}
                        )
                        if tc.get("id"):
                            slot["id"] = tc["id"]
                        fn = tc.get("function") or {}
                        if fn.get("name"):
                            slot["function"]["name"] += fn["name"]
                        if fn.get("arguments"):
                            slot["function"]["arguments"] += fn["arguments"]

            if not tool_calls:
                full_messages.append({"role": "assistant", "content": content})
                yield {"type": "done"}
                return

            ordered_calls = [tool_calls[i] for i in sorted(tool_calls)]
            full_messages.append(
                {"role": "assistant", "content": content or None, "tool_calls": ordered_calls}
            )
            for call in ordered_calls:
                name = call["function"]["name"]
                try:
                    args = json.loads(call["function"]["arguments"] or "{}")
                except json.JSONDecodeError:
                    args = {}
                impl = tool_impls.get(name)
                result = impl(**args) if impl else {"error": f"unknown tool {name}"}
                yield {"type": "tool", "tool": name, "args": args, "result": result}
                full_messages.append(
                    {
                        "role": "tool",
                        "tool_call_id": call["id"],
                        "content": json.dumps(result, default=str),
                    }
                )
        yield {"type": "error", "detail": "Too many tool-call rounds without a final answer."}
    except httpx.HTTPStatusError as exc:
        yield {"type": "error", "detail": f"LLM endpoint error: {exc.response.status_code} {exc.response.text}"}
    except httpx.HTTPError as exc:
        yield {"type": "error", "detail": f"Could not reach LLM endpoint: {exc}"}
    finally:
        client.close()
