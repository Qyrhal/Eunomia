import { test, expect } from "@playwright/test";
import { registerAndOnboard, uniqueEmail } from "./helpers";

// Every page and settings tab renders without an uncaught error or a React
// error in the console.
test("every page and tab renders cleanly", async ({ page }) => {
  test.setTimeout(90_000);
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(`pageerror: ${e.message}`));
  page.on("console", (m) => {
    // a 4xx/5xx shows up as "Failed to load resource" -- some are expected (e.g. chat with no key)
    if (m.type() === "error" && !m.text().startsWith("Failed to load resource")) errors.push(m.text());
  });
  await registerAndOnboard(page, uniqueEmail("pages"));

  const pages: [string, RegExp | string][] = [
    ["/", "Connect an MCP client"],
    ["/chat", "Chat"],
    ["/entities", "Entities"],
    ["/code", "Code"],
    ["/vaults", "Who sees what"],
    ["/connectors", "Connectors"],
    ["/docs", "Quickstart"],
    ["/skill", "How your agents use memory"],
    ["/settings", "The instrument"],
  ];
  for (const [path, text] of pages) {
    await page.goto(path);
    await expect(page.getByText(text).first(), path).toBeVisible();
  }
  for (const tab of ["General", "Updates", "API tokens", "Sessions", "Your data"]) {
    await page.getByRole("tab", { name: tab }).click();
    await expect(page.getByRole("tab", { name: tab })).toHaveAttribute("aria-selected", "true");
  }
  await page.goto("/entities");
  await page.getByRole("tab", { name: "Vector cloud" }).click();
  await expect(page.getByRole("group", { name: "Vault layers" })).toBeVisible();
  expect(errors).toEqual([]);
});
