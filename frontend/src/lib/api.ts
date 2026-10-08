// Same-origin by default: next.config.ts proxies /api/* to the backend, so
// the browser never needs to know the backend's host (works from any device
// or hostname, no CORS). NEXT_PUBLIC_API_URL only overrides that.
export const API_URL = process.env.NEXT_PUBLIC_API_URL || "";
export const apiOrigin = () => API_URL || window.location.origin;

// Session is an httpOnly JWT cookie: every request needs `credentials: "include"`.
export class ApiError extends Error {
  status: number;
  code: string;
  detail: string;
  traceId?: string;
  constructor(status: number, code: string, detail: string, traceId?: string) {
    super(detail);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.detail = detail;
    this.traceId = traceId;
  }
}

const hex = (n: number) =>
  Array.from(crypto.getRandomValues(new Uint8Array(n)), (b) => b.toString(16).padStart(2, "0")).join("");

// W3C traceparent (version 00, sampled) so the backend joins the browser's trace.
export const traceparent = () => `00-${hex(16)}-${hex(8)}-01`;

// ---------------------------------------------------------------------------
// export
// ---------------------------------------------------------------------------

export async function downloadExport(): Promise<void> {
  const res = await fetch(`${API_URL}/api/export`, { credentials: "include" });
  if (!res.ok) throw new Error(`${res.status} /api/export`);
  const blob = await res.blob();
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = "eunomia-export.json";
  a.click();
  URL.revokeObjectURL(url);
}

// ---------------------------------------------------------------------------
// chat
// ---------------------------------------------------------------------------

// One parsed Server-Sent Event from `POST /api/chat/threads/{id}` -- see
// `chat/service.py`'s module docstring for the exact event shapes.
export type ChatStreamEvent =
  | { type: "text"; delta: string }
  | { type: "tool_call"; name: string }
  | { type: "tool_result"; name: string }
  | { type: "done"; reply: string; tool_calls_made: string[] }
  | { type: "error"; message: string };

export const chat = {
  // Streams a reply for `threadId`, calling `onEvent` for every parsed SSE
  // event as it arrives. Not built on the generated client/EventSource: EventSource
  // can't send a POST body, and this needs to parse a `ReadableStream`
  // chunk-by-chunk rather than wait for the whole response.
  send: async (
    threadId: string,
    message: string,
    onEvent: (event: ChatStreamEvent) => void,
    signal?: AbortSignal
  ): Promise<void> => {
    const res = await fetch(`${API_URL}/api/chat/threads/${encodeURIComponent(threadId)}`, {
      method: "POST",
      signal,
      credentials: "include",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ message }),
    });
    if (!res.ok || !res.body) {
      let detail = `${res.status} /api/chat/threads/${threadId}`;
      try {
        const body = await res.json();
        if (body && typeof body.detail === "string") detail = body.detail;
      } catch {
        // non-JSON error body, fall back to the status line above
      }
      throw new Error(detail);
    }

    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split("\n\n");
      buffer = lines.pop() ?? "";
      for (const line of lines) {
        if (!line.startsWith("data: ")) continue;
        onEvent(JSON.parse(line.slice("data: ".length)) as ChatStreamEvent);
      }
    }
  },
};
