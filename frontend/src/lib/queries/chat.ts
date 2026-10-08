import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createThread, deleteThread, getThreadHistory, listThreads } from "@/lib/gen";
import type { ChatMessage, ChatThread } from "@/lib/types";
import { call } from "./client";

export const chatKeys = {
  all: ["chat"] as const,
  threads: () => [...chatKeys.all, "threads"] as const,
  history: (threadId: string) => [...chatKeys.all, "history", threadId] as const,
};

export const threadsQuery = () => ({ queryKey: chatKeys.threads(), queryFn: () => call(listThreads()) as Promise<ChatThread[]> });
export const useThreads = () => useQuery(threadsQuery());

// Never reused past a send: the streamed reply is saved server side, so each open reads the stored thread.
export const historyQuery = (threadId: string) => ({
  queryKey: chatKeys.history(threadId),
  queryFn: () => call(getThreadHistory({ path: { thread_id: threadId } })) as Promise<ChatMessage[]>,
  staleTime: 0,
});

export function useCreateThread() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (title?: string) => call(createThread({ body: { title } })) as Promise<ChatThread>,
    onSuccess: () => qc.invalidateQueries({ queryKey: chatKeys.threads() }),
  });
}

export function useDeleteThread() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => call(deleteThread({ path: { thread_id: id } })),
    onSuccess: (_, id) => {
      qc.removeQueries({ queryKey: chatKeys.history(id) });
      return qc.invalidateQueries({ queryKey: chatKeys.threads() });
    },
  });
}
