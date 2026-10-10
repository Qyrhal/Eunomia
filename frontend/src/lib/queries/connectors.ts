import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { deleteConnectorData, listConnectors, testConnector, updateConnector } from "@/lib/gen";
import type { Connector, ConnectorKind, ConnectorUpdate } from "@/lib/types";
import { call } from "./client";
import { sourceKeys } from "./sources";

export const connectorKeys = {
  all: ["connectors"] as const,
  list: () => [...connectorKeys.all, "list"] as const,
};

export const useConnectors = () =>
  useQuery({ queryKey: connectorKeys.list(), queryFn: () => call(listConnectors()) as Promise<Connector[]> });

export function useUpdateConnector() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ kind, body }: { kind: ConnectorKind; body: ConnectorUpdate }) =>
      call(updateConnector({ path: { kind }, body })) as Promise<Connector>,
    // Enabling or disabling a connector changes which sources are connected.
    onSuccess: () => Promise.all([qc.invalidateQueries({ queryKey: connectorKeys.all }), qc.invalidateQueries({ queryKey: sourceKeys.all })]),
  });
}

export const useTestConnector = () => useMutation({ mutationFn: (kind: ConnectorKind) => call(testConnector({ path: { kind } })) });

/** Deletes everything synced from one connector. Records, search, the graph and counts all change, so refetch them all. */
export function useDeleteConnectorData() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (kind: ConnectorKind) => call(deleteConnectorData({ path: { kind } })),
    onSuccess: () => qc.invalidateQueries(),
  });
}
