import json

from django.http import StreamingHttpResponse
from rest_framework.response import Response
from rest_framework.views import APIView

from connectors.models import AppSettings

from . import tools as t
from .engine import ToolLoopError, run_tool_loop, stream_tool_loop

RESEARCH_TOOL_NAMES = {"list_calendar_events", "list_recent_transactions", "search_emails"}
RESEARCH_TOOL_SCHEMAS = [s for s in t.TOOL_SCHEMAS if s["function"]["name"] in RESEARCH_TOOL_NAMES]
RESEARCH_TOOL_IMPLS = {k: v for k, v in t.TOOL_IMPLS.items() if k in RESEARCH_TOOL_NAMES}

GENERATE_DESCRIPTION_PROMPT = (
    "You draft short, useful notes for a single task on a personal to-do list. "
    "You'll be given the task's title, and sometimes a draft description the user has "
    "already started writing — use both as context for what the task is actually about. "
    "If a draft description is present, refine and extend it rather than ignoring it or "
    "starting over. You may call the research tools available to you (calendar, bank "
    "transactions, email search) to pull in real, specific context relevant to the task — "
    "a matching calendar event, a related email thread, or a relevant transaction. "
    "Only use context you actually found via a tool call; never invent details. "
    "If nothing relevant turns up, write a plain, brief note with no fabricated specifics. "
    "Respond with 2-4 sentences of plain text — no markdown, no preamble, no tool-call summary."
)


class ChatView(APIView):
    """POST {"messages": [{"role": "user", "content": "..."}]} -> runs the OpenAI-compatible
    chat-completions tool-calling loop and returns the final assistant message plus the
    full trace of any tool calls made along the way."""

    def post(self, request):
        settings_row = AppSettings.load()
        messages = request.data.get("messages", [])
        try:
            message, tool_trace = run_tool_loop(settings_row.system_prompt, messages)
        except ToolLoopError as exc:
            return Response({"detail": exc.detail}, status=exc.status)
        return Response({"message": message, "tool_trace": tool_trace})


class ChatStreamView(APIView):
    """POST {"messages": [...]} -> Server-Sent Events. Each event is `data: {json}\\n\\n`,
    one of {"type": "token", "content": str} for streamed answer text, {"type": "tool", ...}
    when a tool call resolves, {"type": "error", "detail": str}, or {"type": "done"}."""

    def post(self, request):
        settings_row = AppSettings.load()
        messages = request.data.get("messages", [])

        def events():
            for event in stream_tool_loop(settings_row.system_prompt, messages):
                yield f"data: {json.dumps(event, default=str)}\n\n"

        return StreamingHttpResponse(events(), content_type="text/event-stream")


class GenerateTaskDetailsView(APIView):
    """POST {"title": "...", "notes": "...", "project_name": "..."} -> drafts task notes,
    pulling in real context from calendar/email/bank via read-only tool calls when relevant.
    `notes`, if present, is whatever draft the user already typed — used as context to
    extend, not replaced wholesale."""

    def post(self, request):
        title = (request.data.get("title") or "").strip()
        if not title:
            return Response({"detail": "title is required"}, status=400)

        notes = (request.data.get("notes") or "").strip()
        project_name = request.data.get("project_name")
        user_message = f"Task title: \"{title}\""
        if project_name:
            user_message += f"\nProject: \"{project_name}\""
        if notes:
            user_message += f"\nDraft description so far: \"{notes}\""

        try:
            message, tool_trace = run_tool_loop(
                GENERATE_DESCRIPTION_PROMPT,
                [{"role": "user", "content": user_message}],
                tool_schemas=RESEARCH_TOOL_SCHEMAS,
                tool_impls=RESEARCH_TOOL_IMPLS,
            )
        except ToolLoopError as exc:
            return Response({"detail": exc.detail}, status=exc.status)

        return Response({"description": message.get("content") or "", "tool_trace": tool_trace})


SUGGEST_TASKS_PROMPT = (
    "You scan a personal register for things that should become tasks. Call the research "
    "tools available to you (calendar, recent bank transactions, email search) to look for: "
    "upcoming meetings that could use a prep task, bills or subscriptions that need a "
    "follow-up, or emails that clearly imply an action. Only suggest a task grounded in "
    "something you actually found via a tool call — never invent events, emails, or "
    "transactions, and don't suggest generic tasks with no real basis. "
    "Respond with ONLY a JSON array (no markdown, no prose, no code fences), at most 5 "
    'items, each shaped exactly like {"title": "...", "notes": "...", "due_at": "...", or null}. '
    "`due_at` is an ISO 8601 datetime if the source has a clear date (e.g. a meeting time), "
    "otherwise null. If nothing currently needs a task, respond with exactly []."
)


def _parse_json_array(text: str) -> list[dict]:
    text = text.strip()
    for candidate in (text, text[text.find("[") : text.rfind("]") + 1]):
        try:
            data = json.loads(candidate)
            if isinstance(data, list):
                return [item for item in data if isinstance(item, dict) and item.get("title")]
        except (json.JSONDecodeError, ValueError):
            continue
    return []


class SuggestTasksView(APIView):
    """POST -> scans calendar/email/bank activity (read-only tool calls) and proposes
    up to 5 candidate tasks. Nothing is created here — the caller decides what to add."""

    def post(self, request):
        try:
            message, tool_trace = run_tool_loop(
                SUGGEST_TASKS_PROMPT,
                [{"role": "user", "content": "Scan for anything that should become a task right now."}],
                tool_schemas=RESEARCH_TOOL_SCHEMAS,
                tool_impls=RESEARCH_TOOL_IMPLS,
            )
        except ToolLoopError as exc:
            return Response({"detail": exc.detail}, status=exc.status)

        suggestions = _parse_json_array(message.get("content") or "")
        return Response({"suggestions": suggestions, "tool_trace": tool_trace})
