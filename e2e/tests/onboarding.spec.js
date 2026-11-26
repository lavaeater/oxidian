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

  await expect(page.getByRole("heading", { name: "Connect your vault" })).toBeVisible();
  // Both git-host providers are offered.
  await expect(page.getByRole("button", { name: "GitHub", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "GitLab", exact: true })).toBeVisible();
  // The primary action to finish setup.
  await expect(page.getByRole("button", { name: "Connect vault" })).toBeVisible();
});

test("settings form exposes the vault connection fields", async ({ page }) => {
  await page.goto("/");

  // Placeholders come straight from the Settings view (packages/app/.../settings.rs).
  await expect(page.getByPlaceholder("octocat")).toBeVisible();      // owner
  await expect(page.getByPlaceholder("my-notes")).toBeVisible();     // repo
  await expect(page.getByPlaceholder("main")).toBeVisible();         // branch
});

// ── Phone: the connect screen must stay usable with the soft keyboard open ──
// Android resizes the WebView for the keyboard (`adjustResize`), so a 800px
// phone becomes ~480px tall while typing. The card is taller than that; the
// whole form must still be reachable, Enter must connect, and the token must
// be pasteable.
test.describe("phone with keyboard open", () => {
  test.use({ viewport: { width: 390, height: 480 }, hasTouch: true });

  const { mockGitHub } = require("./helpers");

  async function fill(page) {
    await page.locator('input[placeholder^="ghp_"]').fill("ghp_test");
    await page.getByPlaceholder("octocat").fill("octocat");
    await page.getByPlaceholder("my-notes").fill("notes");
  }

  // Scroll the way a user does (wheel/swipe), not programmatically:
  // `scrollIntoView` scrolls even an `overflow: hidden` box, a finger can't.
  async function userScroll(page, dy) {
    await page.mouse.move(195, 240);
    for (let i = 0; i < 10; i++) await page.mouse.wheel(0, dy / 10);
    await page.waitForTimeout(200);
  }

  test("the Connect button can be scrolled to and connects", async ({ page }) => {
    await mockGitHub(page, { "Hello.md": "# Hi" });
    await page.goto("/");
    await fill(page);
    await userScroll(page, 3000);
    const connect = page.getByRole("button", { name: "Connect vault" });
    await expect(connect).toBeInViewport();
    await connect.tap();
    await expect(page.getByText("Oxidian", { exact: true })).toBeAttached();
  });

  test("the title is not clipped off the top", async ({ page }) => {
    await page.goto("/");
    await userScroll(page, -3000);
    await expect(page.getByRole("heading", { name: "Connect your vault" })).toBeInViewport();
  });

  test("pressing Enter in a field connects", async ({ page }) => {
    await mockGitHub(page, { "Hello.md": "# Hi" });
    await page.goto("/");
    await fill(page);
    await page.getByPlaceholder("my-notes").press("Enter");
    await expect(page.getByText("Oxidian", { exact: true })).toBeAttached();
  });

  test("a token can be pasted, and the field is a plain text field", async ({ page, context }) => {
    // A password-type field makes Gboard hide its clipboard suggestion; the
    // token is masked with CSS instead.
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    await page.goto("/");
    const token = page.locator('input[placeholder^="ghp_"]');
    await expect(token).toHaveAttribute("type", "text");
    await page.evaluate(() => navigator.clipboard.writeText("ghp_pasted"));
    await token.focus();
    await page.keyboard.press("ControlOrMeta+V");
    await expect(token).toHaveValue("ghp_pasted");
  });
});
