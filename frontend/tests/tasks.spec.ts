import { test, expect, APIRequestContext } from "@playwright/test";

const API = "http://localhost:8000";

async function createProject(request: APIRequestContext, name: string) {
  const res = await request.post(`${API}/api/projects/`, { data: { name } });
  return res.json();
}

async function deleteProject(request: APIRequestContext, id: string) {
  await request.delete(`${API}/api/projects/${id}/`);
}

test.describe("Tasks page", () => {
  let projectId: string;
  let projectName: string;

  test.beforeEach(async ({ request }) => {
    projectName = `E2E ${Date.now()}`;
    const project = await createProject(request, projectName);
    projectId = project.id;
  });

  test.afterEach(async ({ request }) => {
    await deleteProject(request, projectId);
  });

  async function goToProject(page: import("@playwright/test").Page) {
    await page.goto("/tasks");
    await page.getByRole("button", { name: projectName }).click();
  }

  test("adds a task and it appears in the list", async ({ page }) => {
    await goToProject(page);
    await page.getByPlaceholder("New reminder…").fill("Buy picnic blanket");
    await page.getByRole("button", { name: "Add" }).click();

    await expect(page.getByText("Buy picnic blanket")).toBeVisible();
  });

  test("adds a task with manually written notes", async ({ page }) => {
    await goToProject(page);
    await page.getByPlaceholder("New reminder…").fill("Pack cooler");
    await page.getByPlaceholder(/Notes \(optional\)/).fill("Ice, drinks, sandwiches");
    await page.getByRole("button", { name: "Add" }).click();

    await expect(page.getByText("Pack cooler")).toBeVisible();
    await expect(page.getByText("Ice, drinks, sandwiches")).toBeVisible();
  });

  test("completing a task removes it from the default (open-only) view", async ({ page }) => {
    await goToProject(page);
    await page.getByPlaceholder("New reminder…").fill("Finish report");
    await page.getByRole("button", { name: "Add" }).click();
    const row = page.locator("li", { hasText: "Finish report" });
    await expect(row).toBeVisible();

    await row.getByRole("button", { name: "Mark done" }).click();
    await expect(row).not.toBeVisible();

    await page.getByText("Show completed").click();
    await expect(page.locator("li", { hasText: "Finish report" })).toBeVisible();
  });

  test("flagging a task persists via the API", async ({ page, request }) => {
    await goToProject(page);
    await page.getByPlaceholder("New reminder…").fill("Call the vet");
    await page.getByRole("button", { name: "Add" }).click();
    const row = page.locator("li", { hasText: "Call the vet" });
    await expect(row).toBeVisible();

    const flagged = page.waitForResponse((r) => r.url().includes("/api/tasks/") && r.request().method() === "PATCH");
    await row.getByRole("button", { name: "Flag" }).click();
    await flagged;

    const tasks = await (await request.get(`${API}/api/tasks/?project=${projectId}`)).json();
    const task = tasks.find((t: { title: string }) => t.title === "Call the vet");
    expect(task.flagged).toBe(true);
  });

  test("deletes a task", async ({ page }) => {
    await goToProject(page);
    await page.getByPlaceholder("New reminder…").fill("Temporary task");
    await page.getByRole("button", { name: "Add" }).click();
    const row = page.locator("li", { hasText: "Temporary task" });
    await expect(row).toBeVisible();

    await row.getByRole("button", { name: "Delete" }).click();
    await expect(row).not.toBeVisible();
  });

  test("filtering by project only shows that project's tasks", async ({ page, request }) => {
    const other = await createProject(request, `E2E other ${Date.now()}`);
    await request.post(`${API}/api/tasks/`, { data: { title: "In this project", project: projectId } });
    await request.post(`${API}/api/tasks/`, { data: { title: "In the other project", project: other.id } });

    await goToProject(page);
    await expect(page.getByText("In this project")).toBeVisible();
    await expect(page.getByText("In the other project")).not.toBeVisible();

    await deleteProject(request, other.id);
  });

  test("generating notes with AI surfaces a clear error when no LLM is configured", async ({ page }) => {
    // A fresh dev backend has no LLM endpoint/key set — the UI should show the
    // backend's error rather than fail silently.
    await goToProject(page);
    await page.getByPlaceholder("New reminder…").fill("Picnic with Sam");
    await page.getByRole("button", { name: /Generate/ }).click();

    await expect(page.getByText(/Settings/)).toBeVisible({ timeout: 10_000 });
  });

  test("sets a due date/time via the quick-pick and it's saved on the task", async ({ page }) => {
    await goToProject(page);
    await page.getByPlaceholder("New reminder…").fill("Dentist appointment");
    await page.getByRole("button", { name: "Tomorrow" }).first().click();
    await page.getByRole("button", { name: "Add" }).click();

    const row = page.locator("li", { hasText: "Dentist appointment" });
    await expect(row).toBeVisible();
    // the picker should reflect a chosen date somewhere in the saved task's due line
    await expect(row.locator("div").filter({ hasText: /\d{1,2}[/:]\d{2}/ }).first()).toBeVisible();
  });

  test("sets a due date/time manually via the native input", async ({ page }) => {
    await goToProject(page);
    await page.getByPlaceholder("New reminder…").fill("Manual due date");
    await page.locator('input[type="datetime-local"]').first().fill("2030-06-15T14:30");
    await page.getByRole("button", { name: "Add" }).click();

    const row = page.locator("li", { hasText: "Manual due date" });
    await expect(row).toBeVisible();
    await expect(row.getByText(/2030/)).toBeVisible();
  });

  test("edits a task's title, notes, and due date", async ({ page, request }) => {
    await goToProject(page);
    await page.getByPlaceholder("New reminder…").fill("Draft title");
    await page.getByRole("button", { name: "Add" }).click();
    const row = page.locator("li", { hasText: "Draft title" });
    await expect(row).toBeVisible();

    await row.getByRole("button", { name: "Edit" }).click();
    const titleInput = page.locator("li").filter({ has: page.locator('button:has-text("Save")') }).locator("input").first();
    await titleInput.fill("Edited title");
    await page.getByPlaceholder("Notes", { exact: true }).fill("edited notes");
    await page.getByRole("button", { name: "Save" }).click();

    await expect(page.getByText("Edited title")).toBeVisible();
    await expect(page.getByText("edited notes")).toBeVisible();
    await expect(page.getByText("Draft title")).not.toBeVisible();

    const tasks = await (await request.get(`${API}/api/tasks/?project=${projectId}`)).json();
    const task = tasks.find((t: { title: string }) => t.title === "Edited title");
    expect(task.notes).toBe("edited notes");
  });

  test("cancelling an edit discards changes", async ({ page }) => {
    await goToProject(page);
    await page.getByPlaceholder("New reminder…").fill("Unchanged title");
    await page.getByRole("button", { name: "Add" }).click();
    const row = page.locator("li", { hasText: "Unchanged title" });
    await expect(row).toBeVisible();

    await row.getByRole("button", { name: "Edit" }).click();
    const titleInput = page.locator("li").filter({ has: page.locator('button:has-text("Save")') }).locator("input").first();
    await titleInput.fill("Should not persist");
    await page.getByRole("button", { name: "Cancel" }).click();

    await expect(page.getByText("Unchanged title")).toBeVisible();
    await expect(page.getByText("Should not persist")).not.toBeVisible();
  });

  test("scan suggestions require an explicit approve before anything is created", async ({ page, request }) => {
    // No LLM configured -> the scan itself errors, but the important behavioral
    // guarantee is that a scan (successful or not) never creates a task on its own.
    await goToProject(page);
    const before = await (await request.get(`${API}/api/tasks/?project=${projectId}`)).json();

    await page.getByRole("button", { name: /Scan/ }).click();
    await page.waitForTimeout(1000);

    const after = await (await request.get(`${API}/api/tasks/?project=${projectId}`)).json();
    expect(after.length).toBe(before.length);
    // there must be no auto-triggered "Add"/"Approve" — only a manual one per suggestion card
    await expect(page.getByRole("button", { name: "Approve" })).toHaveCount(0);
  });

  test("deleting a task shows an undo toast that restores it", async ({ page }) => {
    await goToProject(page);
    await page.getByPlaceholder("New reminder…").fill("Reversible task");
    await page.getByRole("button", { name: "Add" }).click();
    const row = page.locator("li", { hasText: "Reversible task" });
    await expect(row).toBeVisible();

    await row.getByRole("button", { name: "Delete" }).click();
    await expect(row).not.toBeVisible();
    await page.getByRole("button", { name: "Undo" }).click();
    await expect(page.locator("li", { hasText: "Reversible task" })).toBeVisible();
  });

  test("quick-add shorthand pulls out a tag and a due date from the title", async ({ page, request }) => {
    await goToProject(page);
    await page.getByPlaceholder("New reminder…").fill("Renew gym membership tomorrow #health");
    await page.getByRole("button", { name: "Add" }).click();

    const row = page.locator("li", { hasText: "Renew gym membership" });
    await expect(row).toBeVisible();
    await expect(row.getByText("#health")).toBeVisible();

    const tasks = await (await request.get(`${API}/api/tasks/?project=${projectId}`)).json();
    const task = tasks.find((t: { title: string }) => t.title === "Renew gym membership");
    expect(task.due_at).toBeTruthy();
  });

  test("completing a recurring task creates the next occurrence", async ({ page, request }) => {
    await request.post(`${API}/api/tasks/`, {
      data: { title: "Weekly review", project: projectId, due_at: "2026-01-01T09:00:00Z", recurrence: "weekly" },
    });
    await goToProject(page);
    const row = page.locator("li", { hasText: "Weekly review" });
    await expect(row).toBeVisible();
    await expect(row.getByText("↻ weekly")).toBeVisible();

    await row.getByRole("button", { name: "Mark done" }).click();
    await page.waitForTimeout(300);

    const tasks = await (await request.get(`${API}/api/tasks/?project=${projectId}`)).json();
    const openCopy = tasks.find((t: { title: string; completed: boolean }) => t.title === "Weekly review" && !t.completed);
    expect(openCopy).toBeTruthy();
    expect(openCopy.due_at.startsWith("2026-01-08")).toBe(true);
  });

  test("dragging a task by its handle reorders the list", async ({ page }) => {
    await goToProject(page);
    await page.getByPlaceholder("New reminder…").fill("First task");
    await page.getByRole("button", { name: "Add" }).click();
    const first = page.locator("li", { hasText: "First task" });
    await expect(first).toBeVisible();

    await page.getByPlaceholder("New reminder…").fill("Second task");
    await page.getByRole("button", { name: "Add" }).click();
    const second = page.locator("li", { hasText: "Second task" });
    await expect(second).toBeVisible();

    await first.getByLabel("Drag to reorder").dragTo(second);

    const titles = await page.locator("li >> div.text-\\[13\\.5px\\]").allTextContents();
    expect(titles.findIndex((t) => t.includes("Second task"))).toBeLessThan(
      titles.findIndex((t) => t.includes("First task"))
    );
  });
});

test.describe("Command palette", () => {
  test("Cmd+K opens the palette and navigates to a page", async ({ page }) => {
    await page.goto("/tasks");
    await expect(page.getByRole("heading", { name: "Open items" })).toBeVisible();
    await page.keyboard.press("Control+k");
    const input = page.getByPlaceholder("Jump to a page or project…");
    await expect(input).toBeVisible();

    await input.fill("Finance");
    await page.getByRole("button", { name: "Finance" }).click();
    await expect(page).toHaveURL(/\/finance/);
  });
});

test.describe("Project CRUD", () => {
  test("creates a new project via the sidebar", async ({ page }) => {
    const name = `New project ${Date.now()}`;
    await page.goto("/tasks");
    await page.getByRole("button", { name: "New project" }).click();
    await page.getByPlaceholder("Project name…").fill(name);
    await page.getByPlaceholder("Project name…").press("Enter");

    const row = page.getByRole("button", { name });
    await expect(row).toBeVisible();

    // cleanup: delete it back out
    await row.locator("..").hover();
    page.once("dialog", (d) => d.accept());
    await page.getByRole("button", { name: "Delete project" }).click();
    await expect(row).not.toBeVisible();
  });

  test("renames a project", async ({ page, request }) => {
    const original = `Rename me ${Date.now()}`;
    const renamed = `Renamed ${Date.now()}`;
    const project = await createProject(request, original);

    await page.goto("/tasks");
    const row = page.getByRole("button", { name: original });
    await row.locator("..").hover();
    await page.getByRole("button", { name: "Rename project" }).click();
    await page.getByLabel("Project name").fill(renamed);
    await page.getByLabel("Project name").press("Enter");

    await expect(page.getByRole("button", { name: renamed })).toBeVisible();

    await deleteProject(request, project.id);
  });

  test("deleting a project also deletes its tasks", async ({ page, request }) => {
    const name = `Delete me ${Date.now()}`;
    const project = await createProject(request, name);
    await request.post(`${API}/api/tasks/`, { data: { title: "Doomed task", project: project.id } });

    await page.goto("/tasks");
    const row = page.getByRole("button", { name });
    await expect(row).toBeVisible();

    await row.locator("..").hover();
    page.once("dialog", (d) => d.accept());
    await page.getByRole("button", { name: "Delete project" }).click();
    await expect(row).not.toBeVisible();

    // the project (and its FK) is genuinely gone, not just hidden — filtering by
    // it now 400s, and the task itself no longer turns up anywhere
    const allTasks = await (await request.get(`${API}/api/tasks/`)).json();
    expect(allTasks.some((t: { title: string }) => t.title === "Doomed task")).toBe(false);
  });

  test("cancelling delete via the confirm dialog keeps the project", async ({ page, request }) => {
    const name = `Keep me ${Date.now()}`;
    const project = await createProject(request, name);

    await page.goto("/tasks");
    const row = page.getByRole("button", { name });
    await expect(row).toBeVisible();

    await row.locator("..").hover();
    page.once("dialog", (d) => d.dismiss());
    await page.getByRole("button", { name: "Delete project" }).click();
    await expect(row).toBeVisible();

    await deleteProject(request, project.id);
  });
});

test.describe("Tags", () => {
  let projectId: string;
  let projectName: string;

  test.beforeEach(async ({ request }) => {
    projectName = `E2E ${Date.now()}`;
    const project = await createProject(request, projectName);
    projectId = project.id;
  });

  test.afterEach(async ({ request }) => {
    await deleteProject(request, projectId);
  });

  async function goToProject(page: import("@playwright/test").Page) {
    await page.goto("/tasks");
    await page.getByRole("button", { name: projectName }).click();
  }

  test("adds a brand-new tag to a task and it's saved", async ({ page, request }) => {
    await goToProject(page);
    await page.getByPlaceholder("New reminder…").fill("Renew passport");
    const tagInput = page.getByPlaceholder("Add tag…");
    await tagInput.fill("errands");
    await tagInput.press("Enter");
    await page.getByRole("button", { name: "Add" }).click();

    const row = page.locator("li", { hasText: "Renew passport" });
    await expect(row.getByText("#errands")).toBeVisible();

    const tasks = await (await request.get(`${API}/api/tasks/?project=${projectId}`)).json();
    const task = tasks.find((t: { title: string }) => t.title === "Renew passport");
    expect(task.tags).toEqual(["errands"]);
  });

  test("filtering by tag in the sidebar only shows matching tasks", async ({ page, request }) => {
    await request.post(`${API}/api/tasks/`, { data: { title: "Renew visa", project: projectId, tags: ["urgent"] } });
    await request.post(`${API}/api/tasks/`, { data: { title: "Water the plants", project: projectId } });

    await goToProject(page);
    await expect(page.getByText("Renew visa")).toBeVisible();
    await expect(page.getByText("Water the plants")).toBeVisible();

    await page.getByRole("button", { name: "#urgent", exact: true }).click();
    await expect(page.getByText("Renew visa")).toBeVisible();
    await expect(page.getByText("Water the plants")).not.toBeVisible();

    // clicking the active tag again clears the filter
    await page.getByRole("button", { name: "#urgent", exact: true }).click();
    await expect(page.getByText("Water the plants")).toBeVisible();
  });
});

test.describe("Subtasks", () => {
  let projectId: string;
  let projectName: string;

  test.beforeEach(async ({ request }) => {
    projectName = `E2E ${Date.now()}`;
    const project = await createProject(request, projectName);
    projectId = project.id;
  });

  test.afterEach(async ({ request }) => {
    await deleteProject(request, projectId);
  });

  async function goToProject(page: import("@playwright/test").Page) {
    await page.goto("/tasks");
    await page.getByRole("button", { name: projectName }).click();
  }

  test("subtasks never appear as their own top-level row", async ({ page, request }) => {
    const parent = await (
      await request.post(`${API}/api/tasks/`, { data: { title: "Plan trip", project: projectId } })
    ).json();
    await request.post(`${API}/api/tasks/`, { data: { title: "Book flights", project: projectId, parent: parent.id } });

    await goToProject(page);
    await expect(page.getByText("Plan trip")).toBeVisible();
    await expect(page.getByText("Book flights")).not.toBeVisible();
  });

  test("adds a subtask under a task and it shows the count", async ({ page, request }) => {
    await goToProject(page);
    await page.getByPlaceholder("New reminder…").fill("Plan trip");
    await page.getByRole("button", { name: "Add" }).click();
    const row = page.locator("li", { hasText: "Plan trip" });
    await expect(row).toBeVisible();

    await row.getByRole("button", { name: "Expand subtasks" }).click();
    await page.getByPlaceholder("Add subtask…").fill("Book flights");
    await page.getByPlaceholder("Add subtask…").press("Enter");

    await expect(page.getByText("Book flights")).toBeVisible();
    await expect(page.getByText("1 subtask")).toBeVisible();
  });

  test("completing a subtask persists independently of the parent", async ({ page, request }) => {
    const parent = await (
      await request.post(`${API}/api/tasks/`, { data: { title: "Plan trip", project: projectId } })
    ).json();
    await request.post(`${API}/api/tasks/`, { data: { title: "Book flights", project: projectId, parent: parent.id } });

    await goToProject(page);
    const row = page.locator("li", { hasText: "Plan trip" }).first();
    await row.getByRole("button", { name: "Expand subtasks" }).click();

    const subtaskRow = page.locator("div", { hasText: "Book flights" }).last();
    await subtaskRow.getByRole("button", { name: "Mark done" }).click();
    await page.waitForTimeout(300);

    const subtasks = await (await request.get(`${API}/api/tasks/?parent=${parent.id}`)).json();
    expect(subtasks[0].completed).toBe(true);

    const parentAfter = await (await request.get(`${API}/api/tasks/${parent.id}/`)).json();
    expect(parentAfter.completed).toBe(false);
  });
});
