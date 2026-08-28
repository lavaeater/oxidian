// @ts-check
// Rapid logging in the editor: the signifiers render as glyphs rather than raw
// brackets, and Ctrl/⌘-Enter cycles an entry's state without typing any of the
// bracket characters. See docs/bujo-roadmap.md §3 (phase 2).

const { test, expect } = require("@playwright/test");
const { openFile, seedConfig, mockGitHub } = require("./helpers");

const LOG = [
  "# Monday",
  "",
  "- [ ] call the plumber",
  "- [x] pay rent",
  "- [>] draft the proposal",
  "- [<] book flights",
  "- [-] learn the accordion",
  "- [o] standup at 10:00",
  "- just a note",
  "- [q] not a task",
  "",
].join("\n");

async function openLog(page, content = LOG) {
  await mockGitHub(page, { "Log.md": content });
  await seedConfig(page);
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  await openFile(page, "Log");
  await expect(page.locator(".md-area")).toBeVisible();
}

test("every signifier is recognised and carries its status", async ({ page }) => {
  await openLog(page);

  const boxes = page.locator(".md-area .md-task-checkbox");
  await expect(boxes).toHaveCount(6);
  const statuses = await boxes.evaluateAll((els) =>
    els.map((e) => e.getAttribute("data-status")),
  );
  expect(statuses).toEqual([
    "open",
    "done",
    "migrated",
    "scheduled",
    "dropped",
    "event",
  ]);

  // Only a genuinely done entry is "checked" — migrated and dropped are not
  // done, and the editor must not flatten them into it.
  const checked = await boxes.evaluateAll((els) =>
    els.map((e) => e.getAttribute("data-checked")),
  );
  expect(checked).toEqual(["false", "true", "false", "false", "false", "false"]);
});

test("an unknown marker stays a plain list item", async ({ page }) => {
  await openLog(page);
  // `- [q] not a task` and `- just a note` render as text, not entries.
  await expect(page.locator(".md-area")).toContainText("[q] not a task");
  await expect(page.locator(".md-area .md-task-checkbox")).toHaveCount(6);
});

test("the raw brackets stay in the document even though they're hidden", async ({ page }) => {
  await openLog(page);
  // The glyph is drawn in CSS precisely so the source survives the round trip:
  // innerText is the document model, so losing the characters would lose data.
  const text = await page.locator(".md-area").innerText();
  for (const marker of ["[ ]", "[x]", "[>]", "[<]", "[-]", "[o]"]) {
    expect(text).toContain(marker);
  }
});

test("Ctrl+Enter cycles an entry's signifier without typing brackets", async ({ page }) => {
  await openLog(page, "# Log\n\n- [ ] call the plumber\n");

  const editor = page.locator(".md-area");
  await editor.getByText("call the plumber").click();
  await page.keyboard.press("End");

  // open → done → migrated → scheduled
  for (const expected of ["done", "migrated", "scheduled"]) {
    await page.keyboard.press("Control+Enter");
    await expect(editor.locator(".md-task-checkbox")).toHaveAttribute(
      "data-status",
      expected,
    );
  }
  // Shift reverses, so overshooting costs one keystroke rather than five.
  await page.keyboard.press("Control+Shift+Enter");
  await expect(editor.locator(".md-task-checkbox")).toHaveAttribute(
    "data-status",
    "migrated",
  );
  await expect(editor).toContainText("[>] call the plumber");
});

test("Ctrl+Enter on a plain bullet turns the note into a task", async ({ page }) => {
  await openLog(page, "# Log\n\n- had an idea\n");

  const editor = page.locator(".md-area");
  await editor.getByText("had an idea").click();
  await page.keyboard.press("End");
  await expect(editor.locator(".md-task-checkbox")).toHaveCount(0);

  await page.keyboard.press("Control+Enter");
  await expect(editor.locator(".md-task-checkbox")).toHaveAttribute("data-status", "open");
  await expect(editor).toContainText("[ ] had an idea");
});

test("Enter after any entry continues the list with an open one", async ({ page }) => {
  // Migrating or dropping something says nothing about the next line.
  await openLog(page, "# Log\n\n- [>] moved on\n");

  const editor = page.locator(".md-area");
  await editor.getByText("moved on").click();
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await page.keyboard.type("next thing");

  await expect(editor).toContainText("[ ] next thing");
  // ...and the entry above is untouched.
  await expect(editor).toContainText("[>] moved on");
});
