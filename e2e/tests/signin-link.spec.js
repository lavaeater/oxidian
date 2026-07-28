// @ts-check
// Sign-in link: the vault config can travel in the URL fragment (`#cfg=…`) so it
// can be bookmarked / stored in a password manager and restore the vault in one
// click — the fix for browsers that wipe localStorage between sessions (e.g. a
// managed work profile). Opening such a link imports + persists the config and
// strips the fragment; the sidebar can also produce a link for the current vault.

const { test, expect } = require("@playwright/test");
const { mockGitHub, seedConfig, fakeConfig } = require("./helpers");

const FILES = { "Welcome.md": "# Welcome\n\nHello." };

/** Encode a config object into a `#cfg=…` fragment (URL-safe base64, no pad). */
function signinFragment(cfg = fakeConfig()) {
  return "#cfg=" + Buffer.from(JSON.stringify(cfg), "utf-8").toString("base64url");
}

test("opening a sign-in link boots into the vault and strips the fragment", async ({ page }) => {
  // Note: no seedConfig — the config arrives entirely via the URL fragment.
  await mockGitHub(page, FILES);
  await page.goto("/" + signinFragment());

  // Booted straight into the vault (config imported), not the Settings screen.
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  await expect(page.getByText("Welcome", { exact: false }).first()).toBeVisible();
  await expect(page.getByText("Connect your vault")).toHaveCount(0);

  // The token no longer lingers in the address bar…
  await expect.poll(() => page.evaluate(() => location.hash)).toBe("");
  // …but it was persisted to localStorage for the rest of the session.
  const stored = await page.evaluate(() => localStorage.getItem("oxidian_cfg"));
  expect(stored).toContain("fake-token");
});

test("a malformed sign-in link falls back to the Settings screen", async ({ page }) => {
  await mockGitHub(page, FILES);
  await page.goto("/#cfg=not-valid-base64-json!!!");

  // Garbage fragment must not boot a broken vault — onboarding is shown instead.
  await expect(page.getByText("Connect your vault")).toBeVisible();
});

test("the sidebar copies a sign-in link for the current vault", async ({ page, context }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await mockGitHub(page, FILES);
  await seedConfig(page, { owner: "octocat", repo: "notes" });
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();

  await page.locator('button[title="Copy sign-in link"]').click();
  await expect(page.locator('button[title="Sign-in link copied!"]')).toBeVisible();

  // The clipboard holds a link whose fragment decodes back to this vault config.
  const link = await page.evaluate(() => navigator.clipboard.readText());
  expect(link).toContain("#cfg=");
  const frag = link.split("#cfg=")[1];
  const json = Buffer.from(frag, "base64url").toString("utf-8");
  const cfg = JSON.parse(json);
  expect(cfg.owner).toBe("octocat");
  expect(cfg.repo).toBe("notes");
});
