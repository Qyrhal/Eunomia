import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listSources, syncSource } from "@/lib/gen";
import type { SourceRow } from "@/lib/types";
import { call } from "./client";
import { toolKeys } from "./tools";

export const sourceKeys = {
  all: ["sources"] as const,
  list: () => [...sourceKeys.all, "list"] as const,
};

export const useSources = (opts?: { refetchInterval?: number }) =>
  useQuery({ queryKey: sourceKeys.list(), queryFn: () => call(listSources()) as Promise<SourceRow[]>, ...opts });

export function useSyncSource() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (key: string) => call(syncSource({ path: { key } })),
    // Record counts, sync status and the synced records themselves all change.
    onSettled: () => Promise.all([qc.invalidateQueries({ queryKey: sourceKeys.all }), qc.invalidateQueries({ queryKey: toolKeys.all })]),
  });
}
