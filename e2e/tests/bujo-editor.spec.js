// @ts-check
// Rapid logging in the editor itself (docs/bujo-roadmap.md §7 phase 2):
// signifiers beyond open/done render their glyph, and Ctrl/Cmd+Enter cycles
// a task line through them. This drives markdown_area.js directly — the
// unit tests in packages/ui cover the tokenizer/render logic without a JS
// runtime, this covers the DOM glue that wires it to real keystrokes.

const { test, expect } = require("@playwright/test");
const { openFile, seedConfig, mockGitHub } = require("./helpers");

async function openTasks(page, body) {
  await mockGitHub(page, { "Tasks.md": body });
  await seedConfig(page);
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  await openFile(page, "Tasks");
  const editor = page.locator(".md-area");
  await expect(editor).toBeVisible();
  return editor;
}

test("bujo signifiers beyond open/done render their glyph, not a checkbox", async ({ page }) => {
  const editor = await openTasks(
    page,
    "# Tasks\n\n- [>] call the plumber\n- [o] standup at 10:00\n- [-] learn the accordion",
  );

  const boxes = editor.locator(".md-task-checkbox");
  await expect(boxes.nth(0)).toHaveAttribute("data-status", "migrated");
  await expect(boxes.nth(0)).toHaveAttribute("data-checked", "false");
  await expect(boxes.nth(1)).toHaveAttribute("data-status", "event");
  await expect(boxes.nth(2)).toHaveAttribute("data-status", "dropped");
});

test("ctrl+enter cycles the current line's signifier", async ({ page }) => {
  const editor = await openTasks(page, "# Tasks\n\n- [ ] draft the proposal");

  const line = editor.locator(".md-line").filter({ hasText: "draft the proposal" });
  await line.click();
  await page.keyboard.press("End");

  await page.keyboard.press("Control+Enter");
  await expect(editor).toContainText("[x] draft the proposal");

  await page.keyboard.press("Control+Enter");
  await expect(editor).toContainText("[>] draft the proposal");

  await page.keyboard.press("Control+Enter");
  await expect(editor).toContainText("[<] draft the proposal");

  await page.keyboard.press("Control+Enter");
  await expect(editor).toContainText("[-] draft the proposal");

  // Cycle wraps back to open.
  await page.keyboard.press("Control+Enter");
  await expect(editor).toContainText("[ ] draft the proposal");
});

test("clicking a migrated checkbox does not turn it into a plain done task", async ({ page }) => {
  const editor = await openTasks(page, "# Tasks\n\n- [>] call the plumber");

  const box = editor.locator(".md-task-checkbox").first();
  await expect(box).toHaveAttribute("data-status", "migrated");
  await box.click();

  // Migrated tasks aren't click-toggled — only the keyboard shortcut cycles
  // them (see the `cb:` handler in component.rs).
  await expect(box).toHaveAttribute("data-status", "migrated");
  await expect(editor).toContainText("[>] call the plumber");
});
