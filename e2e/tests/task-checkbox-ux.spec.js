// @ts-check
// Task-checkbox ergonomics (ui-polish.md: "Jumpy UX" + "Caret Stuck in
// Checkbox Lists").
//
//  • Ticking a checkbox must not move the caret into the editor — on Android
//    that focuses the contenteditable, pops the soft keyboard, flips the line
//    to raw source and recentres the document.
//  • A caret sitting between the list marker and the `[` must still move
//    up/down with the arrow keys.

const { test, expect } = require("@playwright/test");
const { openFile, seedConfig, mockGitHub } = require("./helpers");

const LIST = "# Shopping\n\n- [ ] milk\n- [ ] eggs\n- [ ] bread\n- [ ] butter";

async function openList(page) {
  await mockGitHub(page, { "Shopping.md": LIST });
  await seedConfig(page);
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  await openFile(page, "Shopping");
  const editor = page.locator(".md-area");
  await expect(editor.locator(".md-task-checkbox")).toHaveCount(4);
  return editor;
}

/** Index of the `.md-line` holding the caret, or -1. */
function caretLine(page) {
  return page.evaluate(() => {
    const el = document.querySelector(".md-area");
    const sel = window.getSelection();
    if (!el || !sel || !sel.rangeCount || !el.contains(sel.anchorNode)) return -1;
    let n = sel.anchorNode;
    if (n.nodeType !== 1) n = n.parentElement;
    while (n && n.parentElement !== el) n = n.parentElement;
    return n ? Array.prototype.indexOf.call(el.children, n) : -1;
  });
}

function editorFocused(page) {
  return page.evaluate(() => {
    const el = document.querySelector(".md-area");
    return !!el && (document.activeElement === el || el.contains(document.activeElement));
  });
}

test.describe("touch", () => {
  test.use({ hasTouch: true });

  test("tapping a checkbox ticks it without focusing the editor", async ({ page }) => {
    const editor = await openList(page);
    const boxes = editor.locator(".md-task-checkbox");

    await boxes.nth(1).tap();

    await expect(editor.locator(".md-task-checkbox").nth(1)).toHaveAttribute("data-checked", "true");
    // No caret, no focus → no soft keyboard, no raw-source line, no recentre.
    expect(await editorFocused(page)).toBe(false);
    await expect(editor.locator(".md-line--active")).toHaveCount(0);

    // Ticking several in a row keeps working (the shopping-list case).
    await editor.locator(".md-task-checkbox").nth(2).tap();
    await expect(editor.locator(".md-task-checkbox").nth(2)).toHaveAttribute("data-checked", "true");
    expect(await editorFocused(page)).toBe(false);

    // And it is in the model, not just painted.
    await page.getByText("Oxidian", { exact: true }).click();
    await expect(editor).toContainText("[x] eggs");
    await expect(editor).toContainText("[x] bread");
    await expect(editor).toContainText("[ ] milk");
  });
});

test("clicking a checkbox while editing another line leaves the caret there", async ({ page }) => {
  const editor = await openList(page);

  // Put the caret at the end of "milk" (line index 2).
  await editor.getByText("milk").click();
  await page.keyboard.press("End");
  expect(await caretLine(page)).toBe(2);

  await editor.locator(".md-task-checkbox").nth(3).click();
  await expect(editor.locator(".md-task-checkbox").nth(3)).toHaveAttribute("data-checked", "true");
  expect(await caretLine(page)).toBe(2);

  // Typing still lands on the line being edited.
  await page.keyboard.type(" ok");
  await page.getByText("Oxidian", { exact: true }).click();
  await expect(editor).toContainText("[ ] milk ok");
  await expect(editor).toContainText("[x] butter");
});

for (const [key, from, to] of [["ArrowDown", 3, 4], ["ArrowUp", 3, 2]]) {
  test(`${key} moves the caret off a task line from between "- " and "["`, async ({ page }) => {
    const editor = await openList(page);

    // "eggs" is line index 3. Caret to line start, then past "- ".
    await editor.getByText("eggs").click();
    await page.keyboard.press("Home");
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowRight");
    expect(await caretLine(page)).toBe(from);
    const offset = await page.evaluate(() => window.getSelection()?.toString().length ?? -1);
    expect(offset).toBe(0);

    await page.keyboard.press(key);
    await expect.poll(() => caretLine(page)).toBe(to);
  });
}
