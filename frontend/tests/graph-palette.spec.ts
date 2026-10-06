import { test, expect, type Page } from "@playwright/test";
import { registerAndOnboard, uniqueEmail } from "./helpers";

async function tool(page: Page, name: string, args: object) {
  const res = await page.request.post(`/api/tools/${name}`, { data: args });
  expect(res.ok()).toBeTruthy();
  return res.json();
}

async function seedTwoRelatedPeople(page: Page) {
  const ada = await tool(page, "memory_write", {
    subject_name: "Ada Lovelace",
    subject_kind: "person",
    text: "Wrote the first published algorithm.",
  });
  const charles = await tool(page, "memory_write", {
    subject_name: "Charles Babbage",
    subject_kind: "person",
    text: "Designed the Analytical Engine.",
  });
  await tool(page, "code_relate", { from_id: ada.entity.id, to_id: charles.entity.id, label: "collaborated_with" });
}

test.describe("Entity graph", () => {
  test("draws relations that follow dragged nodes, selects on click, clears selection on vault switch", async ({ page }) => {
    await registerAndOnboard(page, uniqueEmail("graph"));
    await seedTwoRelatedPeople(page);
    await tool(page, "vault_create", { name: "Other vault" });

    await page.goto("/entities");
    const svg = page.getByRole("img", { name: "Entity relationship graph" });
    const link = svg.locator("line");
    await expect(link).toHaveCount(1);

    const adaNode = svg.locator("g").filter({ has: page.locator("title", { hasText: /^Ada Lovelace/ }) }).last();
    await expect(adaNode).toBeVisible();

    // Dragging a node moves the end of its link with it.
    await page.waitForTimeout(1500); // let the force layout settle
    const before = [await link.getAttribute("x1"), await link.getAttribute("x2")];
    const box = (await adaNode.boundingBox())!;
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width / 2 + 120, box.y + box.height / 2 + 90, { steps: 8 });
    await page.mouse.up();
    await expect
      .poll(async () => [await link.getAttribute("x1"), await link.getAttribute("x2")])
      .not.toEqual(before);

    // A click (no drag) selects the node.
    await adaNode.click();
    await expect(page.getByRole("button", { name: "Edit" })).toBeVisible();

    // Switching vault drops the selection from the old vault.
    await page.getByLabel("Vault").selectOption({ label: "Other vault" });
    await expect(page.getByRole("button", { name: "Edit" })).toHaveCount(0);
  });
});

test.describe("Command palette", () => {
  test("opens with the shortcut, searches pages and entities, and starts fresh each time", async ({ page }) => {
    await registerAndOnboard(page, uniqueEmail("palette"));
    await seedTwoRelatedPeople(page);
    await page.goto("/");
    await expect(page.getByText("What's next", { exact: true })).toBeVisible(); // hydrated

    const input = page.getByPlaceholder("Jump to a page…");
    await page.keyboard.press("ControlOrMeta+k");
    await expect(input).toBeFocused();

    await input.fill("Ada");
    await expect(page.getByText("Ada Lovelace")).toBeVisible();

    // Below the search threshold, data hits are hidden again.
    await input.fill("A");
    await expect(page.getByText("Ada Lovelace")).toHaveCount(0);

    await input.fill("sett");
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(/\/settings$/);
    await expect(input).toHaveCount(0);

    await page.keyboard.press("ControlOrMeta+k");
    await expect(input).toHaveValue("");
    await page.keyboard.press("Escape");
    await expect(input).toHaveCount(0);
  });
});
