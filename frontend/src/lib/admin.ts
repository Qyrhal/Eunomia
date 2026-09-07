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
