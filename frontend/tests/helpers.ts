import type { Page } from "@playwright/test";

export function uniqueEmail(prefix: string): string {
  return `${prefix}-${Date.now()}-${Math.random().toString(36).slice(2)}@example.com`;
}

/** Registers a fresh user through the real UI (not an API shortcut, so the
 * session cookie lands in the browser context the same way a real visitor's
 * would) and clicks through the onboarding wizard taking the "skip" path at
 * every optional step, landing on the dashboard. */
export async function registerAndOnboard(page: Page, email: string, password = "correct-horse-battery-staple") {
  await page.goto("/register");
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Create account" }).click();

  await page.waitForURL(/\/onboarding/);
  await page.getByRole("button", { name: "Let's go" }).click();
  await page.getByRole("button", { name: "Skip for now" }).click();

  const skipFinish = page.getByRole("button", { name: "Skip & finish" });
  if (await skipFinish.isVisible({ timeout: 2000 }).catch(() => false)) {
    await skipFinish.click();
  }

  await page.waitForURL("http://localhost:3000/");
}
