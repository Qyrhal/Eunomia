import { defineConfig } from "@playwright/test";

// Uses the system-installed Chrome (no browser download needed).
// These tests exercise the real API, not a mock: the FastAPI backend
// (and SurrealDB) must already be running on :8001 before `bun run test` --
// `docker-compose up` the backend+surrealdb first. Playwright's `webServer`
// only starts the frontend dev server below; starting the whole compose
// stack from here would make failures much harder to diagnose (slow first
// boot, DB not ready yet) than just running it yourself once beforehand.
export default defineConfig({
  testDir: "./tests",
  fullyParallel: false,
  retries: 0,
  reporter: "list",
  use: {
    baseURL: "http://localhost:3000",
    channel: "chrome",
  },
  webServer: {
    command: "bun run dev",
    url: "http://localhost:3000",
    reuseExistingServer: true,
    timeout: 30_000,
  },
});
