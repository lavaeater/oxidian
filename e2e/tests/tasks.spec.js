// @ts-check
// Toggling a task checkbox inside the editor flips the markdown checkbox state.
// This drives the `cb:` click path in markdown_area.js -> tasks::toggled_content,
// the interactive complement to the tasks.rs unit tests.

const { test, expect } = require("@playwright/test");
const { openFile, seedConfig, mockGitHub } = require("./helpers");

test("clicking a task checkbox toggles it from unchecked to checked", async ({ page }) => {
  await mockGitHub(page, { "Tasks.md": "# Tasks\n\n- [ ] first thing\n- [x] done thing" });
  await seedConfig(page);

  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  await openFile(page, "Tasks");

  const editor = page.locator(".md-area");
  await expect(editor).toBeVisible();

  const boxes = editor.locator(".md-task-checkbox");
  // First task is unchecked, second is checked (from the source markdown).
  await expect(boxes.nth(0)).toHaveAttribute("data-checked", "false");
  await expect(boxes.nth(1)).toHaveAttribute("data-checked", "true");

  // Click the unchecked box: it flips to checked and the rendered text updates.
  await boxes.nth(0).click();
  await expect(editor.locator(".md-task-checkbox").nth(0)).toHaveAttribute("data-checked", "true");
  await expect(editor).toContainText("[x] first thing");
});
