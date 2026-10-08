import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createToken, getBootstrap, getMe, listTokens, login, logout, register } from "@/lib/gen";
import type { Me } from "@/lib/types";
import type { ApiToken } from "@/lib/api";
import { call } from "./client";

export const authKeys = {
  all: ["auth"] as const,
  me: () => [...authKeys.all, "me"] as const,
  bootstrap: () => [...authKeys.all, "bootstrap"] as const,
  tokens: () => [...authKeys.all, "tokens"] as const,
};

// Always refetch on mount: the session can end or change in another tab.
export const meQuery = () => ({ queryKey: authKeys.me(), queryFn: () => call(getMe()) as Promise<Me>, staleTime: 0 });
export const bootstrapQuery = () => ({ queryKey: authKeys.bootstrap(), queryFn: () => call(getBootstrap()), staleTime: 0 });

export const useMe = (enabled = true) => useQuery({ ...meQuery(), enabled });

// Settings still mints and lists tokens through api.ts, so never serve these from cache.
export const useTokens = () =>
  useQuery({ queryKey: authKeys.tokens(), queryFn: () => call(listTokens()) as Promise<ApiToken[]>, staleTime: 0, refetchInterval: 60_000 });

export function useCreateToken() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (name: string) => call(createToken({ body: { name } })),
    onSuccess: () => qc.invalidateQueries({ queryKey: authKeys.tokens() }),
  });
}

// A new session means a different user: nothing cached under the old one may survive.
const useSessionMutation = <V>(fn: (v: V) => Promise<Me>) => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSuccess: (me) => {
      qc.clear();
      qc.setQueryData(authKeys.me(), me);
    },
  });
};
export const useLogin = () =>
  useSessionMutation(({ email, password }: { email: string; password: string }) => call(login({ body: { email, password } })));
export const useRegister = () =>
  useSessionMutation(({ email, password }: { email: string; password: string }) => call(register({ body: { email, password } })));

export function useLogout() {
  const qc = useQueryClient();
  return useMutation({ mutationFn: () => call(logout()), onSettled: () => qc.clear() });
}
