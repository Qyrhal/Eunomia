import { test, expect, type APIRequestContext } from "@playwright/test";
import { registerAndOnboard, uniqueEmail } from "./helpers";

// Exercises /mcp through the frontend's proxy, the URL the dashboard shows.
async function newToken(request: APIRequestContext): Promise<string> {
  const reg = await request.post("/api/auth/register", {
    data: { email: uniqueEmail("mcp"), password: "correct-horse-battery-staple" },
  });
  expect(reg.ok()).toBeTruthy();
  const res = await request.post("/api/auth/tokens", { data: { name: "MCP" } });
  return (await res.json()).token;
}

function rpc(request: APIRequestContext, token: string | null, body: unknown) {
  return request.post("/mcp", {
    headers: {
      Accept: "application/json, text/event-stream",
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
    },
    data: body,
  });
}

test.describe("MCP server", () => {
  test("the dashboard shows this app's /mcp URL and a ready-to-run Claude Code command", async ({ page }) => {
    await registerAndOnboard(page, uniqueEmail("mcpcard"));
    const url = `${new URL(page.url()).origin}/mcp`;
    await expect(page.getByText(url, { exact: true })).toBeVisible();

    await page.getByRole("button", { name: "Generate a token" }).click();
    await expect(page.getByText(`claude mcp add --transport http eunomia ${url} --header`)).toBeVisible();
  });

  test("initialize, list tools, and call one", async ({ request }) => {
    const token = await newToken(request);

    const init = await rpc(request, token, {
      jsonrpc: "2.0",
      id: 1,
      method: "initialize",
      params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "e2e", version: "1" } },
    });
    expect(init.status()).toBe(200);
    const initBody = await init.json();
    expect(initBody.result.protocolVersion).toBe("2025-06-18");
    expect(initBody.result.capabilities.tools).toBeTruthy();

    const notified = await rpc(request, token, { jsonrpc: "2.0", method: "notifications/initialized" });
    expect(notified.status()).toBe(202);

    const list = await (await rpc(request, token, { jsonrpc: "2.0", id: 2, method: "tools/list" })).json();
    const names = list.result.tools.map((t: { name: string }) => t.name);
    expect(names).toEqual(expect.arrayContaining(["recall", "memory_write", "vault_list", "search"]));
    for (const t of list.result.tools) expect(t.description.length).toBeGreaterThan(10);

    const call = await (
      await rpc(request, token, { jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "vault_list", arguments: {} } })
    ).json();
    expect(call.result.isError).toBe(false);
    expect(call.result.content[0].text).toContain("Personal");
  });

  test("a written memory is recalled through MCP", async ({ request }) => {
    const token = await newToken(request);
    const call = (id: number, name: string, args: object) =>
      rpc(request, token, { jsonrpc: "2.0", id, method: "tools/call", params: { name, arguments: args } }).then((r) => r.json());

    const written = await call(1, "memory_write", {
      subject_name: "Ada Lovelace",
      subject_kind: "person",
      text: "Wrote the first published algorithm for the Analytical Engine.",
    });
    expect(written.result.isError).toBe(false);

    const found = await call(2, "entities_search", { query: "Ada" });
    expect(found.result.content[0].text).toContain("Ada Lovelace");
  });

  test("protocol errors and auth", async ({ request }) => {
    const token = await newToken(request);

    expect((await rpc(request, null, { jsonrpc: "2.0", id: 1, method: "tools/list" })).status()).toBe(401);
    expect((await rpc(request, "not-a-real-token", { jsonrpc: "2.0", id: 1, method: "tools/list" })).status()).toBe(401);
    expect((await request.get("/mcp")).status()).toBe(405);

    const unknownTool = await (
      await rpc(request, token, { jsonrpc: "2.0", id: 1, method: "tools/call", params: { name: "nope", arguments: {} } })
    ).json();
    expect(unknownTool.error.code).toBe(-32602);

    const unknownMethod = await (await rpc(request, token, { jsonrpc: "2.0", id: 2, method: "resources/list" })).json();
    expect(unknownMethod.error.code).toBe(-32601);

    // A tool that fails is a result the model sees, not a protocol error.
    const badArgs = await (
      await rpc(request, token, { jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "get", arguments: {} } })
    ).json();
    expect(badArgs.result.isError).toBe(true);
  });
});
