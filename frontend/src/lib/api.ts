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

export type Project = {
  id: string;
  name: string;
  color: string;
  icon: string;
  order: number;
  open_count: number;
};

export type Task = {
  id: string;
  project: string;
  parent: string | null;
  title: string;
  notes: string;
  url: string;
  due_at: string | null;
  remind_at: string | null;
  allocated_minutes: number | null;
  priority: 0 | 1 | 2 | 3;
  recurrence: "none" | "daily" | "weekly" | "monthly" | "yearly";
  flagged: boolean;
  completed: boolean;
  completed_at: string | null;
  tags: string[];
  created_by_ai: boolean;
  order: number;
  subtask_count: number;
};

export type AppSettings = {
  embedding_backend: "api" | "local" | "stub";
  embedding_model: string;
  llm_base_url: string;
  llm_api_key_set: boolean;
  sync_intervals: Record<string, number>;
  theme: { mode?: "light" | "dark" | "system"; accent?: string };
};

export type Connector = {
  kind: "up_bank" | "pocketai" | "open_connector";
  enabled: boolean;
  config: Record<string, unknown>;
  credentials_set: boolean;
  updated_at: string;
};

export type ChatMessage = { role: "user" | "assistant" | "system" | "tool"; content: string };

export type Overview = {
  open: number;
  completed: number;
  overdue: number;
  flagged: number;
  due_today: number;
};

export type Completion = { day: string; count: number };
export type ProjectBreakdown = { project__name: string; project__color: string; count: number };
export type PriorityBreakdown = { priority: 0 | 1 | 2 | 3; label: string; count: number };
export type AiContribution = { ai_created: number; human_created: number };
export type WeekOverWeek = { this_week: number; last_week: number; delta_pct: number | null };
export type UpcomingLoad = { day: string; count: number; minutes: number | null };

export type Snapshot = {
  up_bank: { transaction_count: number; spent: number; error?: string } | null;
  pocketai: { recordings_count: number; error?: string } | null;
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

export type TaskContext = {
  transactions: { description?: string; amount?: string; occurred_at?: string | null }[];
  recordings: { title?: string; occurred_at?: string | null }[];
};
