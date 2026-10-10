import { test, expect } from "@playwright/test";
import { registerAndOnboard, uniqueEmail } from "./helpers";

test.describe("Dashboard", () => {
  test("sync-status sections and the entity graph render for a fresh user", async ({ page }) => {
    await registerAndOnboard(page, uniqueEmail("dashboard"));

    await expect(page.getByText("Connected sources", { exact: true })).toBeVisible();
    await expect(page.getByText("Add a source", { exact: true })).toBeVisible();

    // A brand-new user has no entities yet (nothing has synced/extracted) --
    // the graph must render its empty state rather than erroring.
    await page.goto("/entities");
    await expect(page.getByText(/No entities yet/)).toBeVisible();
  });
});
