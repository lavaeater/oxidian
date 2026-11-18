// @ts-check
// The formatting toolbar (ui-polish.md: "why aren't the buttons working?").
// Buttons must change the note while the editor is focused, land on the right
// line, and survive the next keystroke (which used to overwrite them).

const { test, expect } = require("@playwright/test");
const { openFile, seedConfig, mockGitHub } = require("./helpers");

async function openNote(page, md) {
  await mockGitHub(page, { "Note.md": md });
  await seedConfig(page);
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  await openFile(page, "Note");
  const editor = page.locator(".md-area");
  await expect(editor).toBeVisible();
  return editor;
}

const blur = (page) => page.getByText("Oxidian", { exact: true }).click();
const btn = (page, title) => page.locator(`.formatting-toolbar button[title="${title}"]`);

test("H1 turns the caret's line into a heading, and typing keeps it", async ({ page }) => {
  const editor = await openNote(page, "first\n\nsecond line\n\nthird");
  await editor.getByText("second line").click();
  await page.keyboard.press("End");

  await btn(page, "Heading 1").click();
  // The edit is async (JS → Rust → JS); a keystroke racing it drops the edit
  // rather than being overwritten, so wait for it like a person would.
  await expect(editor.locator(".md-heading.md-h1")).toHaveCount(1);
  await page.keyboard.type("!");
  await blur(page);

  await expect(editor.locator(".md-heading.md-h1")).toHaveText("# second line!");
  // Other lines untouched — the offset lands on the right line.
  await expect(editor.locator(".md-heading")).toHaveCount(1);
});

test("Bold wraps the selected word", async ({ page }) => {
  const editor = await openNote(page, "intro\n\nbuy some milk today");
  await editor.getByText("buy some milk today").dblclick({ position: { x: 80, y: 5 } });
  const selected = await page.evaluate(() => window.getSelection()?.toString());
  expect(selected?.trim().length).toBeGreaterThan(0);

  await btn(page, "Bold").click();
  await blur(page);

  await expect(editor.locator(".md-bold")).toHaveText(`**${selected?.trim()}**`);
});

test("Task turns a bullet into a task", async ({ page }) => {
  const editor = await openNote(page, "# List\n\n- milk\n- eggs");
  await editor.getByText("eggs").click();

  await btn(page, "Task item").click();
  await blur(page);

  await expect(editor.locator(".md-task-checkbox")).toHaveCount(1);
  await expect(editor).toContainText("[ ] eggs");
  await expect(editor).not.toContainText("[ ] milk");
});
