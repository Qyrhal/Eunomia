import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { invokeTool } from "@/lib/gen";
import type { EntitySummary, ToolHit, ToolRecord } from "@/lib/types";
import { ApiError } from "@/lib/api";
import { call } from "./client";

export const toolKeys = {
  all: ["tools"] as const,
  list: (source: string) => [...toolKeys.all, "list", source] as const,
  search: (source: string, query: string) => [...toolKeys.all, "search", source, query] as const,
  record: (id: string) => [...toolKeys.all, "record", id] as const,
  palette: (query: string) => [...toolKeys.all, "palette", query] as const,
  docs: (topic?: string) => [...toolKeys.all, "docs", topic ?? "index"] as const,
};

// Tools answer HTTP 200 with `{ error }` for a bad call, so a failure is a value here, not a throw.
type Hits = { results: ToolHit[] } | { error: string };
export const invoke = <T>(name: string, body: Record<string, unknown> = {}) => call(invokeTool({ path: { name }, body })) as Promise<T>;
const hits = (res: Hits) => ("results" in res ? res.results : []);

export const useSourceRecords = (source: string) =>
  useQuery({
    queryKey: toolKeys.list(source),
    queryFn: async () => hits(await invoke<Hits>("list", { filters: { source }, sort: "-occurred_at", limit: 50 })),
  });

// `query` is "" for no search; the previous results stay on screen while a new search loads.
export const useSourceSearch = (source: string, query: string) =>
  useQuery({
    queryKey: toolKeys.search(source, query),
    queryFn: async () => hits(await invoke<Hits>("search", { query, sources: [source], limit: 50 })),
    enabled: query !== "",
    placeholderData: keepPreviousData,
  });

export const useRecord = (id: string, enabled: boolean) =>
  useQuery({
    queryKey: toolKeys.record(id),
    queryFn: async () => {
      const res = await invoke<ToolRecord | { error: string }>("get", { id });
      // A tool-level failure is a 200 with `{ error }`; surface it as a non-retried error.
      if ("error" in res) throw new ApiError(200, "tool.error", res.error);
      return res;
    },
    enabled,
  });

/** Command palette: entity and record matches for a debounced query. Failures read as no matches. */
export const usePaletteSearch = (query: string, limit: number, enabled: boolean) => {
  const entities = useQuery({
    queryKey: [...toolKeys.palette(query), "entities"],
    queryFn: async () => {
      const res = await invoke<{ results: EntitySummary[] } | { error: string }>("entities_search", { query, limit });
      return "results" in res ? res.results : [];
    },
    enabled,
    placeholderData: keepPreviousData,
  });
  const records = useQuery({
    queryKey: [...toolKeys.palette(query), "records"],
    queryFn: async () => hits(await invoke<Hits>("search", { query, limit })),
    enabled,
    placeholderData: keepPreviousData,
  });
  return { entities: entities.data ?? [], records: records.data ?? [] };
};

export const useDocs = () =>
  useQuery({ queryKey: toolKeys.docs(), queryFn: () => invoke<{ docs: { topic: string; title: string }[] }>("docs") });

export const useDoc = (topic: string) =>
  useQuery({
    queryKey: toolKeys.docs(topic),
    queryFn: () => invoke<{ topic: string; title: string; markdown: string }>("docs", { topic }),
    placeholderData: keepPreviousData,
  });
