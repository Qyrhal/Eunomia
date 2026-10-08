import { QueryClient } from "@tanstack/react-query";
import { ApiError } from "@/lib/api";

// What every generated operation resolves to (responseStyle "fields", throwOnError off).
type Result<D> = Promise<{ data?: D; error?: unknown; response?: Response }>;

/** Turns the generated client's error result into an `ApiError`, so `ErrorLine` keeps its code and trace id. */
function toApiError(error: unknown, response: Response): ApiError {
  const b = error && typeof error === "object" ? (error as { code?: unknown; detail?: unknown; trace_id?: unknown }) : {};
  const code = typeof b.code === "string" && b.code ? b.code : `http.${response.status}`;
  const traceId = (typeof b.trace_id === "string" && b.trace_id) || response.headers.get("x-trace-id") || undefined;
  const path = response.url ? new URL(response.url).pathname + new URL(response.url).search : "";
  const detail = typeof b.detail === "string" && b.detail ? b.detail : `${response.status} ${path}`;
  return new ApiError(response.status, code, detail, traceId);
}

/** Awaits a generated operation and returns its data, or throws an `ApiError` (HTTP failures) or the raw error (network, parse). */
export async function call<D>(request: Result<D>): Promise<D> {
  const { data, error, response } = await request;
  if (error === undefined) return data as D;
  if (error instanceof Error || !response || response.ok) throw error;
  throw toApiError(error, response);
}

// Retry only what can succeed on a second try: network failures, 5xx and a DB write conflict. Never 4xx.
const retryable = (e: unknown) => !(e instanceof ApiError) || e.status >= 500 || e.code === "db.conflict";

export const makeQueryClient = () =>
  new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 30_000,
        refetchOnWindowFocus: false,
        retry: (count, e) => count < 2 && retryable(e),
      },
      mutations: { retry: (count, e) => count < 2 && e instanceof ApiError && e.code === "db.conflict" },
    },
  });
