import { useQuery } from "@tanstack/react-query";
import { listAudit } from "@/lib/gen";
import { call } from "./client";

export const auditKeys = {
  all: ["audit"] as const,
  list: (limit: number, offset: number) => [...auditKeys.all, "list", limit, offset] as const,
};

export const useAudit = (limit = 50, offset = 0) =>
  useQuery({ queryKey: auditKeys.list(limit, offset), queryFn: () => call(listAudit({ query: { limit, offset } })) });
