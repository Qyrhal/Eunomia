// Runtime config for the generated client in `./gen` (see openapi-ts.config.ts).
// Mirrors `api.ts`: same base URL, cookies on every request, and a fresh
// W3C `traceparent` per request so the backend joins the browser's trace.
//
// Example (typed end to end, not yet used by any page):
//   import { me } from "@/lib/gen";
//   const { data, error } = await me(); // data: AuthOut, error: Problem
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
