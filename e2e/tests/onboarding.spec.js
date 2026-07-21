// @ts-check
// First-run onboarding: with no stored config, the app must show the Settings
// screen (not the vault browser). Needs no network mocking — nothing is fetched
// until a vault is connected.

const { test, expect } = require("@playwright/test");

test.beforeEach(async ({ page }) => {
  // Ensure a truly fresh first-run: no leftover config from a previous test.
  await page.addInitScript(() => window.localStorage.clear());
});

test("first run shows the Settings screen", async ({ page }) => {
  await page.goto("/");

  await expect(page.getByText("Connect your vault")).toBeVisible();
  await expect(page.getByRole("button", { name: "Sign in with GitHub" })).toBeVisible();
  // The vault sidebar must NOT be present before a vault is connected.
  await expect(page.getByText("Oxidian", { exact: true })).toHaveCount(0);
});

test("settings form exposes the vault connection fields", async ({ page }) => {
  await page.goto("/");

  // Placeholders come straight from the Settings view (packages/app/.../settings.rs).
  await expect(page.getByPlaceholder("octocat")).toBeVisible();      // owner
  await expect(page.getByPlaceholder("my-notes")).toBeVisible();     // repo
  await expect(page.getByRole("button", { name: "Connect vault" })).toBeVisible();
});
