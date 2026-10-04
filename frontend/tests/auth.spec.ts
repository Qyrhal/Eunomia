import { test, expect } from "@playwright/test";
import { registerAndOnboard, uniqueEmail } from "./helpers";

test.describe("Auth", () => {
  test("register -> onboarding -> dashboard", async ({ page }) => {
    await registerAndOnboard(page, uniqueEmail("auth"));
    await expect(page).toHaveURL("http://localhost:3000/");
    await expect(page.getByText("The register")).toBeVisible();
  });

  test("an unauthenticated visitor to a protected page is redirected to login or register", async ({ page }) => {
    await page.goto("/settings");
    await expect(page).toHaveURL(/\/(login|register)/);
  });

  test("logging back in with the right password returns to the dashboard", async ({ page, context }) => {
    const email = uniqueEmail("relogin");
    await registerAndOnboard(page, email);

    // Drop the session and confirm login re-establishes it.
    await context.clearCookies();
    await page.goto("/login");
    await page.getByLabel("Email").fill(email);
    await page.getByLabel("Password").fill("correct-horse-battery-staple");
    await page.getByRole("button", { name: "Sign in" }).click();

    await expect(page).toHaveURL("http://localhost:3000/");
  });
});
