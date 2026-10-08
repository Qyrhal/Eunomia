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
  test("renders in 3D, selects a node, clears selection on vault switch", async ({ page }) => {
    await registerAndOnboard(page, uniqueEmail("graph"));
    await seedTwoRelatedPeople(page);
    await tool(page, "vault_create", { name: "Other vault" });

    await page.goto("/entities");
    const graph = page.getByRole("img", { name: "Entity relationship graph" });
    await expect(graph.locator("canvas")).toBeVisible();
    // labels are drawn over the WebGL canvas for small graphs
    await expect(graph.locator(".scene3d-label", { hasText: "Ada Lovelace" })).toBeAttached();

    // every node is reachable without a mouse, and selecting opens the panel
    await page.getByRole("button", { name: "Ada Lovelace" }).press("Enter");
    await expect(page.getByRole("button", { name: "Edit" })).toBeVisible();

    // Switching vault drops the selection from the old vault.
    await page.getByLabel("Vault").selectOption({ label: "Other vault" });
    await expect(page.getByRole("button", { name: "Edit" })).toHaveCount(0);
  });

  test("vector cloud layers vaults in one 3D space, including a merge of them", async ({ page }) => {
    await registerAndOnboard(page, uniqueEmail("cloud"));
    await seedTwoRelatedPeople(page);
    const team = await tool(page, "vault_create", { name: "Team" });
    await tool(page, "memory_write", { subject_name: "Grace Hopper", subject_kind: "person", text: "Built the first compiler.", vault_id: team.id });
    const personal = (await tool(page, "vault_list", {})).results.find((v: { kind: string }) => v.kind === "personal");
    const merged = await tool(page, "vault_merge", { vault_id_a: personal.id, vault_id_b: team.id, name: "Everything" });
    expect(merged.entities).toBe(3);

    await page.goto("/entities");
    await page.getByRole("tab", { name: "Vector cloud" }).click();
    const cloud = page.getByRole("img", { name: "Vector cloud" });
    await expect(cloud.locator("canvas")).toBeVisible();
    await expect(cloud.getByText("PC1")).toBeAttached();

    const layers = page.getByRole("group", { name: "Vault layers" });
    await layers.getByRole("button", { name: /Team/ }).click();
    await layers.getByRole("button", { name: /Everything/ }).click();
    await expect(layers.getByRole("button", { name: /Everything/ })).toContainText("3"); // one point per memory
    await expect(layers.getByRole("button", { name: /^Team/ })).toContainText("1");

    await page.getByRole("button", { name: "Built the first compiler." }).first().press("Enter");
    await expect(page.getByText(/· world$/).first()).toBeVisible();
  });
});

test.describe("Command palette", () => {
  test("opens with the shortcut, searches pages and entities, and starts fresh each time", async ({ page }) => {
    await registerAndOnboard(page, uniqueEmail("palette"));
    await seedTwoRelatedPeople(page);
    await page.goto("/");
    await expect(page.getByText("Connected sources", { exact: true })).toBeVisible(); // hydrated

    const input = page.getByPlaceholder("Jump to a page…");
    const palette = page.getByRole("dialog", { name: "Command palette" });
    await page.keyboard.press("ControlOrMeta+k");
    await expect(input).toBeFocused();

    await input.fill("Ada");
    await expect(palette.getByText("Ada Lovelace")).toBeVisible();

    // Below the search threshold, data hits are hidden again.
    await input.fill("A");
    await expect(palette.getByText("Ada Lovelace")).toHaveCount(0);

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
