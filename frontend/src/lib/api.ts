const API_URL = process.env.NEXT_PUBLIC_API_URL || "http://localhost:8000";
// Matches backend EUNOMIA_API_TOKEN. Only needed when the backend sets it.
const API_TOKEN = process.env.NEXT_PUBLIC_API_TOKEN || "";

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${API_URL}${path}`, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      ...(API_TOKEN ? { Authorization: `Bearer ${API_TOKEN}` } : {}),
      ...init?.headers,
    },
  });
  if (!res.ok) {
    const body = await res.text();
    throw new Error(`${res.status} ${path}: ${body}`);
  }
  if (res.status === 204) return undefined as T;
  return res.json();
}

export const api = {
  get: <T>(path: string) => request<T>(path),
  post: <T>(path: string, data?: unknown) =>
    request<T>(path, { method: "POST", body: data ? JSON.stringify(data) : undefined }),
  patch: <T>(path: string, data: unknown) =>
    request<T>(path, { method: "PATCH", body: JSON.stringify(data) }),
  del: <T = void>(path: string) => request<T>(path, { method: "DELETE" }),
};

export const API_BASE = API_URL;

export type AppSettings = {
  embedding_backend: "api" | "local" | "stub";
  embedding_model: string;
  llm_base_url: string;
  hermes_webhook_url: string;
  hermes_webhook_secret_set: boolean;
  pii_allowlist: string[];
  pii_disabled_sources: string[];
  pii_min_confidence: number;
  vip_senders: string[];
  sync_intervals: Record<string, number>;
  theme: { mode?: "light" | "dark" | "system"; accent?: string };
};

export type Connector = {
  kind: "up_bank" | "pocketai" | "twenty";
  enabled: boolean;
  config: Record<string, unknown>;
  credentials_set: boolean;
  updated_at: string;
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
