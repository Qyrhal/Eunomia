import { test, expect } from "@playwright/test";
import { registerAndOnboard, uniqueEmail } from "./helpers";

const REPLY = [
  "## Plan",
  "",
  "Use **bold**, *italics* and `inline code`, see [the docs](https://example.com/docs).",
  "",
  "- first item",
  "- second item",
  "",
  "```rust",
  'let answer = "42";',
  "```",
  "",
  "| a | b |",
  "|---|---|",
  "| 1 | 2 |",
  "",
  "<script>window.__pwned = true</script>",
].join("\n");

test("assistant replies render as markdown; user text and raw HTML stay literal", async ({ page }) => {
  await registerAndOnboard(page, uniqueEmail("md"));
  const thread = { id: "chat_thread:md1", title: "Markdown", created_at: new Date().toISOString(), updated_at: new Date().toISOString() };
  await page.route("**/api/chat/threads", (route) =>
    route.request().method() === "GET" ? route.fulfill({ json: [thread] }) : route.continue()
  );
  await page.route("**/api/chat/threads/*/history", (route) =>
    route.fulfill({
      json: [
        { role: "user", content: "**not bold** please" },
        { role: "assistant", content: REPLY },
      ],
    })
  );

  await page.goto("/chat");
  const md = page.locator(".md");
  await expect(md.getByRole("heading", { name: "Plan" })).toBeVisible();
  await expect(md.locator("strong")).toHaveText("bold");
  await expect(md.locator("em")).toHaveText("italics");
  await expect(md.locator("p code")).toHaveText("inline code");
  await expect(md.getByRole("link", { name: "the docs" })).toHaveAttribute("href", "https://example.com/docs");
  await expect(md.getByRole("link", { name: "the docs" })).toHaveAttribute("target", "_blank");
  await expect(md.locator("li")).toHaveCount(2);
  await expect(md.locator("pre code")).toContainText('let answer = "42";');
  await expect(md.locator("table td").first()).toHaveText("1");

  // the user's own message is shown as typed, and a <script> in a reply is inert
  await expect(page.getByText("**not bold** please")).toBeVisible();
  expect(await page.evaluate(() => (window as unknown as { __pwned?: boolean }).__pwned)).toBeUndefined();
});
