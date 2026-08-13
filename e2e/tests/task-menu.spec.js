// @ts-check
// The space-triggered task-metadata menu: a space at the end of a task line
// offers due-date/priority/done-today emoji, mirroring the Obsidian Tasks
// plugin. Drives the arm/disarm flag in markdown_area.js (the JS-side
// complement to no Rust unit test reaching it) and the app-level poll/render
// wiring in views/vault.rs + views/task_menu.rs.
//
// It used to arm on Enter, which was the wrong key: Enter also moves the caret
// to the next line, so the menu appeared for a task the user had already left.

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

/** Put the caret at the end of the task line and press space. */
async function armAtEndOfTask(page, text = "Buy milk") {
  await page.locator(".md-area").getByText(text).click();
  await page.keyboard.press("End");
  await page.keyboard.press("Space");
}

test("a space at the end of a task line shows the task menu", async ({ page }) => {
  await openNote(page, { "Tasks.md": "- [ ] Buy milk" });
  await armAtEndOfTask(page);

  const menu = page.locator(".task-menu");
  await expect(menu).toBeVisible();
  await expect(menu).toContainText("Due date");
  await expect(menu).toContainText("Highest priority");
  await expect(menu).toContainText("Done today");
});

test("the menu annotates the task you are on, not the next one", async ({ page }) => {
  // The whole reason for moving off Enter. Two tasks: arming on the first must
  // offer to annotate the first.
  await openNote(page, { "Tasks.md": "- [ ] Buy milk\n- [ ] Feed cat" });
  await armAtEndOfTask(page, "Buy milk");
  await expect(page.locator(".task-menu")).toBeVisible();

  await page.locator(".task-menu-item", { hasText: "High priority" }).click();

  const lines = page.locator(".md-area .md-task-item");
  await expect(lines.nth(0)).toContainText("Buy milk");
  await expect(lines.nth(0)).toContainText("⏫");
  await expect(lines.nth(1)).not.toContainText("⏫");
});

test("typing after the space dismisses the menu", async ({ page }) => {
  await openNote(page, { "Tasks.md": "- [ ] Buy milk" });
  await armAtEndOfTask(page);
  await expect(page.locator(".task-menu")).toBeVisible();

  // "start typing some text and it goes away" — and the text lands normally.
  await page.keyboard.type("today");
  await expect(page.locator(".task-menu")).not.toBeVisible();
  await expect(page.locator(".md-area")).toContainText("Buy milk today");
});

test("a space in the middle of a task line does not arm the menu", async ({ page }) => {
  // Otherwise the menu would flash on every word break while writing a task.
  await openNote(page, { "Tasks.md": "- [ ] Buy milk" });
  const editor = page.locator(".md-area");
  await editor.getByText("Buy milk").click();
  await page.keyboard.press("Home");
  await page.keyboard.press("Space");

  await expect(page.locator(".task-menu")).not.toBeVisible();
});

test("a space on a non-task line does not arm the menu", async ({ page }) => {
  await openNote(page, { "Tasks.md": "- [ ] Buy milk\n\nJust a paragraph" });
  const editor = page.locator(".md-area");
  await editor.getByText("Just a paragraph").click();
  await page.keyboard.press("End");
  await page.keyboard.press("Space");

  await expect(page.locator(".task-menu")).not.toBeVisible();
});

test("picking Due date opens a calendar and inserts the picked date", async ({ page }) => {
  await openNote(page, { "Tasks.md": "- [ ] Buy milk" });
  await armAtEndOfTask(page);
  await expect(page.locator(".task-menu")).toBeVisible();

  await page.locator(".task-menu-item", { hasText: "Due date" }).click();
  const picker = page.locator(".task-date-picker");
  await expect(picker).toBeVisible();

  // Click whichever day is marked "today" — deterministic regardless of when
  // the test runs, and exercises the real dioxus-primitives Calendar.
  await picker.locator('button[data-today="true"]').click();

  await expect(page.locator(".task-menu")).not.toBeVisible();
  await expect(page.locator(".md-area")).toContainText("📅");
});

test("Enter still just continues the list, with no menu", async ({ page }) => {
  await openNote(page, { "Tasks.md": "- [ ] Buy milk" });
  const editor = page.locator(".md-area");
  await editor.getByText("Buy milk").click();
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");

  await expect(page.locator(".task-menu")).not.toBeVisible();
  await expect(editor.locator(".md-task-item")).toHaveCount(2);
});
