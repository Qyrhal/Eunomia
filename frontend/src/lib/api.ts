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

export type ApiToken = { id: string; name: string; created_at: string | null; last_used_at: string | null };
export type Session = { id: string; user_agent: string; created_at: string | null; last_seen_at: string | null };

export const auth = {
  register: (email: string, password: string) => api.post<Me>("/api/auth/register", { email, password }),
  login: (email: string, password: string) => api.post<Me>("/api/auth/login", { email, password }),
  logout: () => api.post<{ ok: boolean }>("/api/auth/logout"),
  me: () => api.get<Me>("/api/auth/me"),
  bootstrap: () => api.get<{ has_users: boolean }>("/api/auth/bootstrap"),
  tokens: {
    list: () => api.get<ApiToken[]>("/api/auth/tokens"),
    create: (name: string) => api.post<{ id: string; name: string; token: string }>("/api/auth/tokens", { name }),
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
  completeOnboarding: () => api.post<{ ok: boolean }>("/api/settings/complete-onboarding"),
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
// connectors
// ---------------------------------------------------------------------------

export type ConnectorKind =
  | "up_bank"
  | "pocketai"
  | "open_connector"
  | "github"
  | "slack"
  | "notion"
  | "linear"
  | "gmail"
  | "google_calendar"
  | "discord"
  | "spotify"
  | "todoist"
  | "stripe";

export type Connector = {
  kind: ConnectorKind;
  enabled: boolean;
  config: Record<string, unknown>;
  credentials_set: boolean;
  updated_at: string | null;
};

export type ConnectorUpdate = Partial<{
  enabled: boolean;
  config: Record<string, unknown>;
  credentials: Record<string, string>;
}>;

export type Snapshot = {
  up_bank: { transaction_count: number; spent: number } | null;
  pocketai: { recordings_count: number } | null;
};

export const connectors = {
  list: () => api.get<Connector[]>("/api/connectors"),
  get: (kind: ConnectorKind) => api.get<Connector>(`/api/connectors/${kind}`),
  update: (kind: ConnectorKind, body: ConnectorUpdate) => api.put<Connector>(`/api/connectors/${kind}`, body),
  test: (kind: ConnectorKind) => api.post<{ ok: boolean; error?: string }>(`/api/connectors/${kind}/test`),
  snapshot: () => api.get<Snapshot>("/api/snapshot"),
};

// ---------------------------------------------------------------------------
// sources
// ---------------------------------------------------------------------------

export type SyncStatus = {
  cursor: string;
  last_run: string | null;
  last_ok: string | null;
  last_error: string;
  consecutive_failures: number;
};

export type SourceRow = {
  key: string;
  label: string;
  provider: string;
  record_types: string[];
  connected: boolean;
  sync_status: SyncStatus;
  record_count: number;
};

export const sources = {
  list: () => api.get<SourceRow[]>("/api/sources"),
  status: () => api.get<Record<string, SyncStatus>>("/api/sources/status"),
  sync: (key: string) => api.post<Record<string, unknown>>(`/api/sources/${key}/sync`),
};

// ---------------------------------------------------------------------------
// tools
// ---------------------------------------------------------------------------

// A record as the generic `search`/`list` tools summarise it (no payload).
export type ToolHit = {
  id: string;
  source: string;
  type: string;
  title: string;
  snippet: string;
  occurred_at: string | null;
  url: string | null;
};

// A record as the generic `get` tool returns it in full.
export type ToolRecord = ToolHit & {
  external_id: string;
  body_text: string;
  payload: Record<string, unknown>;
};

export const tools = {
  catalogue: () => api.get<Record<string, unknown>>("/api/tools"),
  call: (name: string, body?: Record<string, unknown>) => api.post<Record<string, unknown>>(`/api/tools/${name}`, body ?? {}),
  search: (body: { query: string; sources?: string[]; limit?: number }) =>
    api.post<{ results: ToolHit[] } | { error: string }>("/api/tools/search", body),
  list: (body: { filters?: Record<string, unknown>; sort?: string; limit?: number }) =>
    api.post<{ results: ToolHit[] } | { error: string }>("/api/tools/list", body),
  get: (id: string) => api.post<ToolRecord | { error: string }>("/api/tools/get", { id }),
};

// ---------------------------------------------------------------------------
// entities
// ---------------------------------------------------------------------------

export type EntityKind = "person" | "organisation" | "location" | "repository" | "file" | "symbol";

export type EntitySummary = {
  id: string;
  kind: EntityKind;
  name: string;
  aliases: string[];
  summary: string;
};

export type EntityMemory = { id: string; text: string; created_at?: string; source?: string; owner_email: string | null };
export type EntityRelation = {
  id: string;
  in: string;
  out: string;
  label: string;
  direction: "in" | "out";
  owner_email: string | null;
};

export type EntityDetail = EntitySummary & {
  owner_email: string | null;
  memory: EntityMemory[];
  relations: EntityRelation[];
};

export type EntityGraphNode = { id: string; kind: EntityKind; name: string; owner_email: string | null };
export type EntityGraphEdge = { source: string; target: string; label: string; owner_email: string | null };
export type EntityGraph = { nodes: EntityGraphNode[]; edges: EntityGraphEdge[] };

export const entities = {
  list: (kind?: EntityKind, vaultId?: string) => {
    const params = new URLSearchParams();
    if (kind) params.set("kind", kind);
    if (vaultId) params.set("vault_id", vaultId);
    const qs = params.toString();
    return api.get<{ results: EntitySummary[]; total: number; has_more: boolean }>(
      `/api/entities${qs ? `?${qs}` : ""}`
    );
  },
  get: (id: string) => api.get<EntityDetail>(`/api/entities/${encodeURIComponent(id)}`),
  graph: (opts?: { kinds?: EntityKind[]; vaultId?: string }) => {
    const params = new URLSearchParams();
    (opts?.kinds ?? []).forEach((k) => params.append("kinds", k));
    if (opts?.vaultId) params.set("vault_id", opts.vaultId);
    const qs = params.toString();
    return api.get<EntityGraph>(`/api/entities/graph${qs ? `?${qs}` : ""}`);
  },
  create: (body: { kind: EntityKind; name: string; aliases?: string[]; vault_id?: string }) =>
    api.post<EntitySummary>("/api/entities", body),
  update: (id: string, body: Partial<{ name: string; aliases: string[]; summary: string }>) =>
    api.patch<EntitySummary>(`/api/entities/${encodeURIComponent(id)}`, body),
  delete: (id: string) => api.del<{ deleted: boolean }>(`/api/entities/${encodeURIComponent(id)}`),
  addMemory: (id: string, body: { text: string; type?: string }) =>
    api.post<EntityMemory>(`/api/entities/${encodeURIComponent(id)}/memory`, body),
  deleteMemory: (memoryId: string) =>
    api.del<{ deleted: boolean }>(`/api/entities/memory/${encodeURIComponent(memoryId)}`),
  addRelation: (id: string, body: { to_id: string; label: string }) =>
    api.post<EntityRelation>(`/api/entities/${encodeURIComponent(id)}/relations`, body),
  merge: (winnerId: string, loserId: string) =>
    api.post<EntitySummary>(`/api/entities/${encodeURIComponent(winnerId)}/merge`, { loser_id: loserId }),
};

// ---------------------------------------------------------------------------
// vaults
// ---------------------------------------------------------------------------

export type VaultRole = "owner" | "member";
export type VaultKind = "personal" | "org";

export type Vault = { id: string; name: string; kind: VaultKind; created_at: string | null; role: VaultRole };
export type VaultMember = { email: string; role: VaultRole };
export type VaultInvitation = {
  vault_id: string;
  vault_name: string;
  vault_kind: VaultKind;
  role: VaultRole;
  created_at: string | null;
};

export const vaults = {
  list: () => api.get<{ results: Vault[] }>("/api/vaults"),
  create: (name: string, kind: VaultKind = "org") => api.post<Vault>("/api/vaults", { name, kind }),
  rename: (id: string, name: string) => api.patch<Vault>(`/api/vaults/${encodeURIComponent(id)}`, { name }),
  delete: (id: string) => api.del<{ deleted: boolean }>(`/api/vaults/${encodeURIComponent(id)}`),
  members: (id: string) => api.get<{ results: VaultMember[] }>(`/api/vaults/${encodeURIComponent(id)}/members`),
  invite: (id: string, email: string, role: VaultRole = "member") =>
    api.post<{ vault_id: string; user_email: string; role: VaultRole }>(
      `/api/vaults/${encodeURIComponent(id)}/members`,
      { email, role }
    ),
  removeMember: (id: string, email: string) =>
    api.del<{ removed: boolean }>(`/api/vaults/${encodeURIComponent(id)}/members/${encodeURIComponent(email)}`),
  leave: (id: string) => api.post<{ left: boolean }>(`/api/vaults/${encodeURIComponent(id)}/leave`),
  invitations: () => api.get<{ results: VaultInvitation[] }>("/api/vaults/invitations"),
  acceptInvitation: (id: string) => api.post<Vault>(`/api/vaults/${encodeURIComponent(id)}/invitations/accept`),
  declineInvitation: (id: string) =>
    api.post<{ declined: boolean }>(`/api/vaults/${encodeURIComponent(id)}/invitations/decline`),
  clone: (id: string, name?: string, kind: VaultKind = "org") =>
    api.post<Vault & { entities_copied: number }>(`/api/vaults/${encodeURIComponent(id)}/clone`, { name, kind }),
  // Copies both into a NEW vault; the originals are untouched.
  merge: (a: string, b: string, name?: string) =>
    api.post<Vault & { entities: number }>("/api/vaults/merge", { vault_ids: [a, b], name: name || undefined }),
};

// ---------------------------------------------------------------------------
// vector cloud + docs
// ---------------------------------------------------------------------------

export type CloudPoint = { id: string; vault: string; kind: string; label: string; x: number; y: number; z: number };
export type Cloud = { space: "semantic" | "lexical"; points: CloudPoint[] };
export const cloud = (vaultIds: string[]) =>
  api.get<Cloud>(`/api/entities/cloud?vault_ids=${vaultIds.map(encodeURIComponent).join(",")}`);

// Served by the `docs` tool -- the same docs/ folder agents read over MCP.
export const docs = {
  list: () => api.post<{ docs: { topic: string; title: string }[] }>("/api/tools/docs", {}),
  get: (topic: string) => api.post<{ topic: string; title: string; markdown: string }>("/api/tools/docs", { topic }),
};

// ---------------------------------------------------------------------------
// chat
// ---------------------------------------------------------------------------

export type ChatToolCall = { id: string; type: "function"; function: { name: string; arguments: string } };

export type ChatMessage = {
  role: "user" | "assistant" | "tool";
  content: string;
  tool_calls?: ChatToolCall[] | null;
  created_at?: string;
};

export type ChatThread = { id: string; title: string; created_at: string | null; updated_at: string | null };

// One parsed Server-Sent Event from `POST /api/chat/threads/{id}` -- see
// `chat/service.py`'s module docstring for the exact event shapes.
export type ChatStreamEvent =
  | { type: "text"; delta: string }
  | { type: "tool_call"; name: string }
  | { type: "tool_result"; name: string }
  | { type: "done"; reply: string; tool_calls_made: string[] }
  | { type: "error"; message: string };

export const chat = {
  threads: {
    list: () => api.get<ChatThread[]>("/api/chat/threads"),
    create: (title?: string) => api.post<ChatThread>("/api/chat/threads", { title }),
    delete: (id: string) => api.del<{ ok: boolean }>(`/api/chat/threads/${encodeURIComponent(id)}`),
  },
  history: (threadId: string) => api.get<ChatMessage[]>(`/api/chat/threads/${encodeURIComponent(threadId)}/history`),
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
