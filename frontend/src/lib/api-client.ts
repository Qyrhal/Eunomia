// Runtime config for the generated client in `./gen` (see openapi-ts.config.ts).
// Mirrors `api.ts`: same base URL, cookies on every request, and a fresh
// W3C `traceparent` per request so the backend joins the browser's trace.
//
// Pages do not call `./gen` directly: they use the TanStack Query hooks in
// `./queries`, whose `call()` turns the error result into an `ApiError`.
import type { CreateClientConfig } from "./gen/client";
import { API_URL, traceparent } from "./api";

export const createClientConfig: CreateClientConfig = (config) => ({
  ...config,
  baseUrl: API_URL,
  credentials: "include",
  fetch: (input, init) => {
    const request = new Request(input, init);
    request.headers.set("traceparent", traceparent());
    return fetch(request);
  },
});
