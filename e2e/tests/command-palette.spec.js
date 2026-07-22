// @ts-check
// Keyboard shortcuts + command palette. This is the ONLY layer that can cover
// `use_global_shortcuts` (packages/app/src/shortcuts.rs): its chord-matching
// logic lives in a document::eval JS string, so there is no JS runtime under
// `cargo test` — it must be exercised in a real browser.

const { test, expect } = require("@playwright/test");
const { seedConfig, mockGitHub } = require("./helpers");

/** Boot into the vault and wait until it's interactive (file list rendered). */
async function bootVault(page) {
  await mockGitHub(page, { "Alpha.md": "# Alpha\n", "Beta.md": "# Beta\n" });
  await seedConfig(page);
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  // The shortcut listener installs in an effect after mount; waiting for a file
  // entry ensures the vault is interactive before we fire the chord.
  await expect(page.locator(".file-entry-name").first()).toBeVisible();
}

/** Press a chord until it opens the expected modal (robust to listener timing). */
async function pressUntilVisible(page, chord, locator) {
  await expect(async () => {
    await page.keyboard.press(chord);
    await expect(locator).toBeVisible({ timeout: 1000 });
  }).toPass({ timeout: 8000 });
}

test("Ctrl+P opens the command palette, filters, and Escape closes it", async ({ page }) => {
  await bootVault(page);

  const paletteInput = page.getByPlaceholder("Run a command…");
  await pressUntilVisible(page, "Control+p", paletteInput);

  // Typing filters the command list.
  await paletteInput.fill("new note");
  await expect(page.locator(".qs-item-name")).toContainText("New note");

  // Escape dismisses it.
  await page.keyboard.press("Escape");
  await expect(paletteInput).toBeHidden();
});

test("Ctrl+O opens the quick switcher", async ({ page }) => {
  await bootVault(page);

  const switcherInput = page.getByPlaceholder("Go to file…");
  await pressUntilVisible(page, "Control+o", switcherInput);

  await page.keyboard.press("Escape");
  await expect(switcherInput).toBeHidden();
});
