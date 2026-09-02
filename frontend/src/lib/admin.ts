import { api } from "./api";

// ---- data browser (generic tools) ----
export type Hit = {
  id: string;
  source: string;
  type: string;
  title: string;
  snippet: string;
  occurred_at: string | null;
  url: string | null;
};
export type FullRecord = Hit & {
  external_id: string;
  body_text: string;
  payload: Record<string, unknown>;
  links: { rel: string; direction: string; target_id: string }[];
};

export const search = (body: {
  query: string;
  sources?: string[];
  types?: string[];
  mode?: string;
  limit?: number;
}) => api.post<{ results: Hit[] }>("/api/tools/search", body);

export const getRecord = (id: string) => api.post<FullRecord | { error: string }>("/api/tools/get", { id });

// ---- sources ----
export type SourceRow = {
  key: string;
  label: string;
  provider: string;
  record_types: string[];
  enabled: boolean;
  last_run: string | null;
  last_error: string;
};
export const listSources = () => api.get<SourceRow[]>("/api/sources");
export const syncSource = (key: string) =>
  api.post<Record<string, unknown>>(`/api/sources/${key}/sync`, {});

// ---- vault ----
export type Secret = {
  token: string;
  kind: string;
  type: string;
  source: string;
  first_seen: string;
  last_used: string;
};
export type AuditRow = {
  kind: string;
  actor: string;
  tokens: string[];
  detail: string;
  at: string;
};
export const listSecrets = () => api.get<Secret[]>("/api/vault/secrets");
export const revealSecret = (token: string) =>
  api.post<{ token: string; value: string } | { error: string }>("/api/vault/reveal", { token });
export const auditLog = () => api.get<AuditRow[]>("/api/vault/audit");

// ---- triggers (via the tool registry) ----
export type Trigger = {
  key: string;
  kind: string;
  enabled: boolean;
  spec: Record<string, unknown>;
  webhook_route: string;
  dedupe_window_s: number;
  fire_count: number;
  last_fired_at: string | null;
};
export type Delivery = {
  trigger: string;
  entity: string;
  attempt: number;
  status: number | null;
  ok: boolean;
  dead: boolean;
  detail: string;
  at: string;
};
export const listTriggers = () =>
  api.post<{ triggers: Trigger[] }>("/api/tools/list_triggers", {});
export const createTrigger = (b: {
  key: string;
  kind: string;
  spec: unknown;
  webhook_route?: string;
  dedupe_window_s?: number;
}) => api.post<{ key: string; created: boolean }>("/api/tools/create_trigger", b);
export const updateTrigger = (b: { key: string; enabled?: boolean; spec?: unknown }) =>
  api.post<{ updated: boolean }>("/api/tools/update_trigger", b);
export const deleteTrigger = (key: string) =>
  api.post<{ deleted: boolean }>("/api/tools/delete_trigger", { key });
export const testTrigger = (key: string) =>
  api.post<{ delivered: boolean }>("/api/tools/test_trigger", { key });
export const deliveryLog = () =>
  api.post<{ deliveries: Delivery[] }>("/api/tools/delivery_log", { limit: 100 });
