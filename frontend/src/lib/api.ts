const API_URL = process.env.NEXT_PUBLIC_API_URL || "http://localhost:8001";
export const API_BASE = API_URL;

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

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${API_URL}${path}`, {
    ...init,
    credentials: "include",
    headers: {
      "Content-Type": "application/json",
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
    throw new Error(errorMessage(res.status, path, body));
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

export const auth = {
  register: (email: string, password: string) => api.post<Me>("/api/auth/register", { email, password }),
  login: (email: string, password: string) => api.post<Me>("/api/auth/login", { email, password }),
  logout: () => api.post<{ ok: boolean }>("/api/auth/logout"),
  me: () => api.get<Me>("/api/auth/me"),
  token: () => api.post<{ token: string }>("/api/auth/token"),
  bootstrap: () => api.get<{ has_users: boolean }>("/api/auth/bootstrap"),
};

// ---------------------------------------------------------------------------
// settings
// ---------------------------------------------------------------------------

export type AppSettings = {
  embedding_model: string;
  sync_intervals: Record<string, number>;
  theme: { mode?: "light" | "dark" | "system"; accent?: string };
  openai_api_key_set: boolean;
};

export type SettingsUpdate = Partial<{
  embedding_model: string;
  sync_intervals: Record<string, number>;
  theme: AppSettings["theme"];
  openai_api_key: string;
}>;

export const settings = {
  get: () => api.get<AppSettings>("/api/settings"),
  update: (body: SettingsUpdate) => api.patch<AppSettings>("/api/settings", body),
  completeOnboarding: () => api.post<{ ok: boolean }>("/api/settings/complete-onboarding"),
};

// ---------------------------------------------------------------------------
// connectors
// ---------------------------------------------------------------------------

export type ConnectorKind = "up_bank" | "pocketai" | "open_connector";

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

export type FinanceSummary = {
  balance: number;
  accounts: { name: string; balance: string }[];
  spend_by_category: { category: string; amount: number }[];
  spend_by_day: { day: string; amount: number }[];
  recent_transactions: { description: string; amount: string; created_at: string }[];
};

export type PocketSummary = {
  recordings_count: number;
  total_duration_minutes: number;
  tag_breakdown: { tag: string; count: number }[];
  recent_recordings: { title: string; duration_minutes: number; recorded_at: string; tags: string[] }[];
};

export const connectors = {
  list: () => api.get<Connector[]>("/api/connectors"),
  get: (kind: ConnectorKind) => api.get<Connector>(`/api/connectors/${kind}`),
  update: (kind: ConnectorKind, body: ConnectorUpdate) => api.put<Connector>(`/api/connectors/${kind}`, body),
  test: (kind: ConnectorKind) => api.post<{ ok: boolean; error?: string }>(`/api/connectors/${kind}/test`),
  snapshot: () => api.get<Snapshot>("/api/snapshot"),
  upBankFinanceSummary: (days = 30) => api.get<FinanceSummary>(`/api/connectors/up_bank/finance-summary?days=${days}`),
  pocketaiSummary: (days = 30) => api.get<PocketSummary>(`/api/connectors/pocketai/summary?days=${days}`),
  pocketaiAll: (limit = 50) => api.get<{ data: Record<string, unknown>[] }>(`/api/connectors/pocketai/all?limit=${limit}`),
  pocketaiSearch: (query: string) => api.get<{ data: Record<string, unknown>[] }>(`/api/connectors/pocketai/search?query=${encodeURIComponent(query)}`),
  pocketaiDetail: (recordingId: string) => api.get<Record<string, unknown>>(`/api/connectors/pocketai/detail/${recordingId}`),
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
};

export const sources = {
  list: () => api.get<SourceRow[]>("/api/sources"),
  status: () => api.get<Record<string, SyncStatus>>("/api/sources/status"),
  sync: (key: string) => api.post<Record<string, unknown>>(`/api/sources/${key}/sync`),
};

// ---------------------------------------------------------------------------
// tools
// ---------------------------------------------------------------------------

export const tools = {
  catalogue: () => api.get<Record<string, unknown>>("/api/tools"),
  call: (name: string, body?: Record<string, unknown>) => api.post<Record<string, unknown>>(`/api/tools/${name}`, body ?? {}),
};

// ---------------------------------------------------------------------------
// entities
// ---------------------------------------------------------------------------

export type EntityKind = "person" | "organisation" | "location";

export type EntitySummary = {
  id: string;
  kind: EntityKind;
  name: string;
  aliases: string[];
  summary: string;
};

export type EntityMemory = { id: string; text: string; created_at?: string; source?: string };
export type EntityRelation = { id: string; in: string; out: string; label: string; direction: "in" | "out" };

export type EntityDetail = EntitySummary & {
  memory: EntityMemory[];
  relations: EntityRelation[];
};

export type EntityGraphNode = { id: string; kind: EntityKind; name: string };
export type EntityGraphEdge = { source: string; target: string; label: string };
export type EntityGraph = { nodes: EntityGraphNode[]; edges: EntityGraphEdge[] };

export const entities = {
  list: (kind?: EntityKind) => api.get<EntitySummary[]>(`/api/entities${kind ? `?kind=${kind}` : ""}`),
  get: (id: string) => api.get<EntityDetail>(`/api/entities/${encodeURIComponent(id)}`),
  graph: () => api.get<EntityGraph>("/api/entities/graph"),
};
