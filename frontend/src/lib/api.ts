// Same-origin by default: next.config.ts proxies /api/* to the backend, so
// the browser never needs to know the backend's host (works from any device
// or hostname, no CORS). NEXT_PUBLIC_API_URL only overrides that.
export const API_URL = process.env.NEXT_PUBLIC_API_URL || "";
export const apiOrigin = () => API_URL || window.location.origin;

// Session is an httpOnly JWT cookie set by /api/auth/{register,login} --
// every request needs `credentials: "include"` to send/receive it.
// FastAPI's default HTTPException body is `{"detail": "..."}` (or, for a
// 422 validation error, `{"detail": [{"msg": ..., ...}, ...]}`).
function errorMessage(status: number, path: string, body: unknown): string {
  if (body && typeof body === "object" && "detail" in body) {
    const detail = (body as { detail: unknown }).detail;
    if (typeof detail === "string") return detail;
    if (Array.isArray(detail)) {
      return detail.map((d) => (d && typeof d === "object" && "msg" in d ? String((d as { msg: unknown }).msg) : JSON.stringify(d))).join("; ");
    }
    if (detail != null) return JSON.stringify(detail);
  }
  return `${status} ${path}`;
}

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

// Old bodies are `{ detail }`; new ones are RFC 9457 problem+json with `code` and `trace_id`.
function apiError(res: Response, path: string, body: unknown): ApiError {
  const b = body && typeof body === "object" ? (body as { code?: unknown; trace_id?: unknown }) : {};
  const code = typeof b.code === "string" && b.code ? b.code : `http.${res.status}`;
  const traceId = (typeof b.trace_id === "string" && b.trace_id) || res.headers.get("x-trace-id") || undefined;
  return new ApiError(res.status, code, errorMessage(res.status, path, body), traceId);
}

const hex = (n: number) =>
  Array.from(crypto.getRandomValues(new Uint8Array(n)), (b) => b.toString(16).padStart(2, "0")).join("");

// W3C traceparent (version 00, sampled) so the backend joins the browser's trace.
export const traceparent = () => `00-${hex(16)}-${hex(8)}-01`;

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${API_URL}${path}`, {
    ...init,
    credentials: "include",
    headers: {
      "Content-Type": "application/json",
      traceparent: traceparent(),
      ...init?.headers,
    },
  });
  if (!res.ok) {
    let body: unknown = undefined;
    try {
      body = await res.json();
    } catch {
      // non-JSON error body, fall through with no `detail`
    }
    throw apiError(res, path, body);
  }
  if (res.status === 204) return undefined as T;
  const text = await res.text();
  return text ? JSON.parse(text) : (undefined as T);
}

export const api = {
  get: <T>(path: string) => request<T>(path),
  post: <T>(path: string, data?: unknown) =>
    request<T>(path, { method: "POST", body: data !== undefined ? JSON.stringify(data) : undefined }),
  patch: <T>(path: string, data: unknown) => request<T>(path, { method: "PATCH", body: JSON.stringify(data) }),
  put: <T>(path: string, data: unknown) => request<T>(path, { method: "PUT", body: JSON.stringify(data) }),
  del: <T = void>(path: string) => request<T>(path, { method: "DELETE" }),
};

// ---------------------------------------------------------------------------
// auth
// ---------------------------------------------------------------------------

export type Me = { id: string; email: string; onboarded: boolean };

export type Scope = "memory:read" | "memory:write" | "vaults:admin" | "connectors";
export type ApiToken = {
  id: string;
  name: string;
  created_at: string | null;
  last_used_at: string | null;
  scopes: Scope[];
  /** Set when the token is restricted to one vault. */
  vault_id: string | null;
  /** Null for a token that never expires. */
  expires_at: string | null;
};
export type NewToken = { scopes?: Scope[]; vault_id?: string | null; expires_at?: string | null };
export type Session = { id: string; user_agent: string; created_at: string | null; last_seen_at: string | null };

export const auth = {
  tokens: {
    list: () => api.get<ApiToken[]>("/api/auth/tokens"),
    create: (name: string, opts: NewToken = {}) =>
      api.post<{ id: string; name: string; token: string }>("/api/auth/tokens", { name, ...opts }),
    revoke: (id: string) => api.del<{ ok: boolean }>(`/api/auth/tokens/${encodeURIComponent(id)}`),
  },
  sessions: {
    list: () => api.get<Session[]>("/api/auth/sessions"),
    revoke: (id: string) => api.del<{ ok: boolean }>(`/api/auth/sessions/${encodeURIComponent(id)}`),
  },
};

// ---------------------------------------------------------------------------
// oauth (MCP clients connecting without a pasted token)
// ---------------------------------------------------------------------------

export type OAuthGrant = {
  id: string;
  client_id: string;
  client_name: string;
  client_logo: string | null;
  scope: string[];
  created_at: string;
  last_used_at: string | null;
};

export type ConsentInfo = {
  client: { name: string; logo_uri: string | null; client_uri: string | null };
  redirect_host: string;
  loopback: boolean;
  scopes: { scope: string; description: string }[];
  user_email: string;
};

export const oauth = {
  consent: (query: string) => api.get<ConsentInfo>(`/api/oauth/consent?${query}`),
  decide: (params: Record<string, string>, approve: boolean) =>
    api.post<{ redirect_to: string }>("/api/oauth/consent", { ...params, approve }),
  grants: {
    list: () => api.get<OAuthGrant[]>("/api/oauth/grants"),
    revoke: (id: string) => api.del<{ deleted: boolean }>(`/api/oauth/grants/${encodeURIComponent(id)}`),
  },
};

// ---------------------------------------------------------------------------
// settings
// ---------------------------------------------------------------------------

export type AppSettings = {
  embedding_model: string;
  sync_intervals: Record<string, number>;
  theme: { mode?: "light" | "dark" | "system"; accent?: string };
  openai_api_key_set: boolean;
  openai_base_url: string;
  observations_mission: string;
  memory_skill: string;
  memory_skill_custom: boolean;
};

export type SettingsUpdate = Partial<{
  memory_skill: string;
  embedding_model: string;
  sync_intervals: Record<string, number>;
  theme: AppSettings["theme"];
  openai_api_key: string;
  openai_base_url: string;
  observations_mission: string;
}>;

export type OpenAiModels = { models: string[]; error: string | null };

export const settings = {
  get: () => api.get<AppSettings>("/api/settings"),
  update: (body: SettingsUpdate) => api.patch<AppSettings>("/api/settings", body),
  openaiModels: () => api.get<OpenAiModels>("/api/settings/openai-models"),
};

// ---------------------------------------------------------------------------
// self-update
// ---------------------------------------------------------------------------

export type UpdateStatus =
  | { configured: false }
  | {
      configured: true;
      current_version: string;
      latest_version: string;
      update_available: boolean;
      checked_at: string;
      applying: boolean;
      error: string | null;
    };

export const update = {
  status: () => api.get<UpdateStatus>("/api/update/status"),
  request: () => api.post<{ configured: boolean; requested?: boolean }>("/api/update/request"),
  check: () => api.post<{ configured: boolean; requested?: boolean }>("/api/update/check"),
};

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
  // event as it arrives. Not built on `api.post`/EventSource: EventSource
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
