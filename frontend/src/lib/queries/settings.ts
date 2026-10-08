import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { completeOnboarding, getSettings, getUpdateStatus, updateSettings } from "@/lib/gen";
import type { AppSettings, SettingsUpdate, UpdateStatus } from "@/lib/api";
import { call } from "./client";
import { authKeys } from "./auth";

export const settingsKeys = {
  all: ["settings"] as const,
  get: () => [...settingsKeys.all, "get"] as const,
  update: () => [...settingsKeys.all, "update"] as const,
};

// The Settings page writes through api.ts, so a cached copy must not outlive the page that changed it.
export const useSettings = (enabled = true) =>
  useQuery({ queryKey: settingsKeys.get(), queryFn: () => call(getSettings()) as Promise<AppSettings>, staleTime: 0, enabled });

export function useUpdateSettings() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: SettingsUpdate) => call(updateSettings({ body })) as Promise<AppSettings>,
    onSuccess: (s) => qc.setQueryData(settingsKeys.get(), s),
  });
}

export function useCompleteOnboarding() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => call(completeOnboarding()),
    onSuccess: () => qc.invalidateQueries({ queryKey: authKeys.me() }),
  });
}

export const useUpdateStatus = () =>
  useQuery({ queryKey: [...settingsKeys.all, "update-status"], queryFn: () => call(getUpdateStatus()) as Promise<UpdateStatus> });
