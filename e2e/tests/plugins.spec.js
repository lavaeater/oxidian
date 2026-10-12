// @ts-check
// The plugin registry: list, enable, configure, disable. Bullet Journal is
// plugin #1 and the first user of the generic settings form — see
// docs/plugin-architecture.md.

const { test, expect } = require("@playwright/test");
const { seedConfig, mockGitHub } = require("./helpers");

const BUJO_DIR = ".oxidian/plugins/bujo";

async function openApp(page, files = { "Note.md": "# Note\n" }, cfg = {}) {
  await mockGitHub(page, files);
  await seedConfig(page, cfg);
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  return files;
}

async function openPlugins(page) {
  await page.locator(".sidebar-icon-btn[title='Plugins']").click();
  await expect(page.locator(".plugins-modal")).toBeVisible();
}

test("the panel lists Bullet Journal, off, and not yet configurable", async ({ page }) => {
  await openApp(page);
  await openPlugins(page);

  await expect(page.locator(".plugin-name")).toHaveText("Bullet Journal");
  await expect(page.locator(".plugin-path")).toHaveText(`${BUJO_DIR}/`);
  await expect(page.locator(".plugin-toggle")).toHaveText("Off");
  // Configuring something that isn't on has nothing to configure.
  await expect(page.locator(".plugin-configure")).toBeDisabled();
});

test("enabling scaffolds the plugin's folder in the vault", async ({ page }) => {
  const files = await openApp(page);
  await openPlugins(page);
  await page.locator(".plugin-toggle").click();
  await expect(page.locator(".plugin-toggle")).toHaveText("On");

  // The manifest records the flag, and settings + templates land in the
  // plugin's own folder rather than being scattered through the vault.
  await expect
    .poll(() => Object.keys(files).filter((p) => p.startsWith(".oxidian/")).sort())
    .toEqual([
      ".oxidian/plugins.json",
      `${BUJO_DIR}/settings.json`,
      `${BUJO_DIR}/templates/daily-log.md`,
      `${BUJO_DIR}/templates/monthly-log.md`,
      `${BUJO_DIR}/templates/weekly-log.md`,
    ]);
  expect(JSON.parse(files[".oxidian/plugins.json"])).toEqual({ bujo: { enabled: true } });
  expect(JSON.parse(files[`${BUJO_DIR}/settings.json`])).toEqual({
    daily_template: `${BUJO_DIR}/templates/daily-log.md`,
    monthly_template: `${BUJO_DIR}/templates/monthly-log.md`,
    weekly_template: `${BUJO_DIR}/templates/weekly-log.md`,
  });
});

test("enabling turns on the period switcher, disabling turns it off again", async ({ page }) => {
  const files = await openApp(page);
  await expect(page.locator(".period-switcher")).toHaveCount(0);

  await openPlugins(page);
  await page.locator(".plugin-toggle").click();
  await expect(page.locator(".plugin-toggle")).toHaveText("On");
  await page.locator(".move-picker-backdrop").click({ position: { x: 5, y: 5 } });
  await expect(page.locator(".period-switcher")).toBeVisible();

  await openPlugins(page);
  await page.locator(".plugin-toggle").click();
  await expect(page.locator(".plugin-toggle")).toHaveText("Off");
  await page.locator(".move-picker-backdrop").click({ position: { x: 5, y: 5 } });
  await expect(page.locator(".period-switcher")).toHaveCount(0);

  // Switching a plugin off is not uninstalling it: the user's templates and
  // settings are their files and must survive untouched.
  expect(files[`${BUJO_DIR}/templates/weekly-log.md`]).toBeTruthy();
  expect(JSON.parse(files[".oxidian/plugins.json"])).toEqual({ bujo: { enabled: false } });
});

test("the settings form is built from the plugin's declared fields", async ({ page }) => {
  const files = await openApp(page);
  await openPlugins(page);
  await page.locator(".plugin-toggle").click();
  await expect(page.locator(".plugin-configure")).toBeEnabled();
  await page.locator(".plugin-configure").click();

  const inputs = page.locator(".plugins-form .settings-input");
  await expect(inputs).toHaveCount(3);
  await expect(page.locator(".plugins-form")).toContainText("Weekly log template");
  await expect(inputs.nth(1)).toHaveValue(`${BUJO_DIR}/templates/weekly-log.md`);

  await inputs.nth(1).fill("journal/weeks/W.md");
  await page.getByRole("button", { name: "Save" }).click();

  // Saving returns to the list and persists into the vault.
  await expect(page.locator(".plugins-list")).toBeVisible();
  await expect
    .poll(() => JSON.parse(files[`${BUJO_DIR}/settings.json`]).weekly_template)
    .toBe("journal/weeks/W.md");
});

test("a pre-plugin vault keeps its logs without enabling anything", async ({ page }) => {
  // Vaults configured before the plugin existed carry their template paths in
  // the app config. Those must keep working, or an upgrade silently removes a
  // feature the user was already using.
  await openApp(page, { "Note.md": "# Note\n" }, {
    weekly_note_template: ".oxidian/templates/weekly.md",
  });
  await expect(page.locator(".period-switcher")).toBeVisible();
});

test("enabling a legacy vault seeds the paths it already had", async ({ page }) => {
  const files = await openApp(page, { "Note.md": "# Note\n" }, {
    weekly_note_template: ".oxidian/templates/weekly.md",
  });
  await openPlugins(page);
  await page.locator(".plugin-toggle").click();
  await expect(page.locator(".plugin-toggle")).toHaveText("On");

  await expect
    .poll(() => JSON.parse(files[`${BUJO_DIR}/settings.json`] || "{}").weekly_template)
    .toBe(".oxidian/templates/weekly.md");
  // The untouched daily default said nothing, so the plugin's own wins.
  expect(JSON.parse(files[`${BUJO_DIR}/settings.json`]).daily_template).toBe(
    `${BUJO_DIR}/templates/daily-log.md`,
  );
});
