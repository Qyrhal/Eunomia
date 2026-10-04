import { test, expect } from "@playwright/test";
import { registerAndOnboard, uniqueEmail } from "./helpers";

test.describe("Connectors", () => {
  test("saving a connector's credentials marks it connected", async ({ page }) => {
    await registerAndOnboard(page, uniqueEmail("connectors"));
    await page.goto("/connectors");

    await expect(page.getByText("Not connected").first()).toBeVisible();

    const upBankCard = page.locator(".ledger", { hasText: "Up Bank" });
    await upBankCard.getByPlaceholder("up:yeah:…").fill("up:yeah:e2e-fake-token");
    await upBankCard.getByRole("button", { name: /Save/ }).click();

    await expect(upBankCard.getByText("Connected")).toBeVisible();

    // Reflected after a reload too, not just optimistic local state.
    await page.reload();
    await expect(page.locator(".ledger", { hasText: "Up Bank" }).getByText("Connected")).toBeVisible();
  });
});
