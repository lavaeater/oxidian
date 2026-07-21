// @ts-check
// The SHA-conflict guarantee at the app level: when a save collides with a
// remote change (HTTP 409), the editor must surface a conflict — never silently
// drop or overwrite the edit. This is the end-to-end complement to the vault
// unit test that maps 409 -> VaultError::Conflict.

const { test, expect } = require("@playwright/test");
const { seedConfig, mockGitHub } = require("./helpers");

test("a 409 on auto-save surfaces a conflict, not a silent overwrite", async ({ page }) => {
  // Every write returns 409.
  await mockGitHub(page, { "Note.md": "# Title\n\nbody text." }, { conflictOnWrite: true });
  await seedConfig(page);

  await page.goto("/");
  // Wait for the vault sidebar to finish loading before opening the note.
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  await page.getByText("Note", { exact: false }).first().click();
  const editor = page.locator(".md-area");
  await expect(editor).toBeVisible();
  await expect(page.locator(".editor-filename")).toHaveText("Note.md");

  // Make an edit to trigger the debounced auto-save.
  await editor.getByText("body text.", { exact: false }).click();
  await page.keyboard.press("End");
  await page.keyboard.type(" edited");

  // The auto-save is debounced (~5s) then fails on the mocked 409. The status
  // must show the failure, and the tooltip must carry the conflict message from
  // VaultError::Conflict — proving the edit wasn't silently accepted.
  // `.save-status--error` is unique to the error state (other `.save-status`
  // spans exist for loading / index counts), so assert against it directly.
  const status = page.locator(".save-status--error");
  await expect(status).toBeVisible({ timeout: 25000 });
  await expect(status).toContainText("Save failed");
  await expect(status).toHaveAttribute("title", /changed remotely/i);
});
