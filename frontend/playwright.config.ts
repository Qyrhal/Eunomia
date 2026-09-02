import { defineConfig } from "@playwright/test";

// Uses the system-installed Chrome (no browser download needed).
// The Django backend must already be running on :8000 — these tests
// exercise the real API, not a mock.
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
