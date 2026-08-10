// @ts-check
// The Enter-triggered task-metadata menu: continuing a non-empty task line
// with Enter offers due-date/priority/done-today emoji, mirroring the
// Obsidian Tasks plugin. Drives the arm/disarm flag in markdown_area.js's
// Enter handler (the JS-side complement to no Rust unit test reaching it) and
// the app-level poll/render wiring in views/vault.rs + views/task_menu.rs.

const { test, expect } = require("@playwright/test");
const { openFile, seedConfig, mockGitHub } = require("./helpers");

async function openNote(page, files) {
  await mockGitHub(page, files);
  await seedConfig(page);
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  await openFile(page, "Tasks");
  await expect(page.locator(".md-area")).toBeVisible();
}

test("pressing Enter after a non-empty task line shows the task menu", async ({ page }) => {
  await openNote(page, { "Tasks.md": "- [ ] Buy milk" });

  const editor = page.locator(".md-area");
  await editor.getByText("Buy milk").click();
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");

  await expect(page.locator(".task-menu")).toBeVisible();
  await expect(page.locator(".task-menu")).toContainText("Due date");
  await expect(page.locator(".task-menu")).toContainText("Highest priority");
  await expect(page.locator(".task-menu")).toContainText("Done today");
});

test("picking a priority inserts its emoji on the new blank task line", async ({ page }) => {
  await openNote(page, { "Tasks.md": "- [ ] Buy milk" });

  const editor = page.locator(".md-area");
  await editor.getByText("Buy milk").click();
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await expect(page.locator(".task-menu")).toBeVisible();

  await page.locator(".task-menu-item", { hasText: "High priority" }).click();
  await expect(page.locator(".task-menu")).not.toBeVisible();
  await expect(editor).toContainText("⏫");

  // Still a live, editable task line — typing continues right after the emoji.
  await page.keyboard.type("Feed the cat");
  await expect(editor).toContainText("Feed the cat");
});

test("picking Due date opens a calendar and inserts the picked date", async ({ page }) => {
  await openNote(page, { "Tasks.md": "- [ ] Buy milk" });

  const editor = page.locator(".md-area");
  await editor.getByText("Buy milk").click();
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await expect(page.locator(".task-menu")).toBeVisible();

  await page.locator(".task-menu-item", { hasText: "Due date" }).click();
  const picker = page.locator(".task-date-picker");
  await expect(picker).toBeVisible();

  // Click whichever day is marked "today" — deterministic regardless of when
  // the test runs, and exercises the real dioxus-primitives Calendar.
  await picker.locator('button[data-today="true"]').click();

  await expect(page.locator(".task-menu")).not.toBeVisible();
  await expect(editor).toContainText("📅");
});

test("pressing Enter again without picking anything just continues as normal", async ({ page }) => {
  await openNote(page, { "Tasks.md": "- [ ] Buy milk" });

  const editor = page.locator(".md-area");
  await editor.getByText("Buy milk").click();
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await expect(page.locator(".task-menu")).toBeVisible();

  // Second Enter on the still-empty task line: the menu gets out of the way
  // and the editor's normal "empty item -> exit the list" behaviour fires —
  // the same as if the menu had never existed.
  await page.keyboard.press("Enter");
  await expect(page.locator(".task-menu")).not.toBeVisible();

  const taskItems = editor.locator(".md-task-item");
  await expect(taskItems).toHaveCount(1);
  await expect(editor).toContainText("Buy milk");
});
