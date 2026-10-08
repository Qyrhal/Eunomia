import { expect, test } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { registerAndOnboard, uniqueEmail } from "./helpers";

// Settings → HTTPS. The backend only writes https.json into its
// UPDATE_STATUS_DIR; the updater (scripts/auto-update.sh) applies it and
// writes https-status.json. Here the test plays the updater, so it needs the
// backend's UPDATE_STATUS_DIR as E2E_UPDATE_STATUS_DIR.
const dir = process.env.E2E_UPDATE_STATUS_DIR;
test.skip(!dir, "set E2E_UPDATE_STATUS_DIR to the backend's UPDATE_STATUS_DIR");

const requestFile = () => path.join(dir!, "https.json");
const statusFile = () => path.join(dir!, "https-status.json");
const writeStatus = (s: object) => fs.writeFileSync(statusFile(), JSON.stringify(s, null, 2));

test.beforeEach(async ({ page }) => {
  fs.rmSync(requestFile(), { force: true });
  fs.rmSync(statusFile(), { force: true });
  await registerAndOnboard(page, uniqueEmail("https"));
  await page.goto("/settings?tab=https");
  await expect(page.getByRole("status")).toHaveText("Off");
});

test("an invalid domain is rejected and nothing is requested", async ({ page }) => {
  await page.getByLabel("Domain").fill("http://evil.example.com;reboot");
  await page.getByLabel("Email for Let's Encrypt").fill("me@example.com");
  await page.getByRole("button", { name: "Enable HTTPS" }).click();
  await expect(page.getByText(/Enter a domain name like/)).toBeVisible();
  expect(fs.existsSync(requestFile())).toBe(false);
  await expect(page.getByRole("status")).toHaveText("Off");
});

test("an invalid email is rejected", async ({ page }) => {
  await page.getByLabel("Domain").fill("eunomia.example.com");
  await page.getByLabel("Email for Let's Encrypt").fill("not-an-email");
  await page.getByRole("button", { name: "Enable HTTPS" }).click();
  await expect(page.getByText(/valid email address/)).toBeVisible();
  expect(fs.existsSync(requestFile())).toBe(false);
});

test("enable: request written, pending, then active; disable", async ({ page }) => {
  await page.getByLabel("Domain").fill("Eunomia.Example.com");
  await page.getByLabel("Email for Let's Encrypt").fill("me@example.com");
  await page.getByRole("button", { name: "Enable HTTPS" }).click();
  await expect(page.getByRole("status")).toContainText("Pending — getting a certificate for eunomia.example.com");
  const req = JSON.parse(fs.readFileSync(requestFile(), "utf8"));
  expect(req).toEqual({ enabled: true, domain: "eunomia.example.com", email: "me@example.com" });

  // the updater picks it up: caddy started, no certificate yet
  fs.rmSync(requestFile());
  writeStatus({ state: "pending", domain: "eunomia.example.com", message: "no certificate yet. Check that eunomia.example.com points at this machine", checked_at: new Date().toISOString() });
  await expect(page.getByText(/points at this machine/)).toBeVisible({ timeout: 10_000 });

  // ...and the certificate answers
  writeStatus({ state: "active", domain: "eunomia.example.com", message: null, checked_at: new Date().toISOString() });
  await expect(page.getByRole("status")).toContainText("Active", { timeout: 10_000 });
  await expect(page.getByRole("link", { name: "https://eunomia.example.com" })).toBeVisible();

  await page.getByRole("button", { name: "Disable" }).click();
  await expect(page.getByRole("status")).toHaveText("Off");
  expect(JSON.parse(fs.readFileSync(requestFile(), "utf8")).enabled).toBe(false);
});

test("an updater error is shown", async ({ page }) => {
  writeStatus({ state: "error", domain: "eunomia.example.com", message: "could not start caddy: port 443 is already allocated", checked_at: new Date().toISOString() });
  await page.reload();
  await expect(page.getByRole("status")).toHaveText("Error");
  await expect(page.getByText("could not start caddy: port 443 is already allocated")).toBeVisible();
});
