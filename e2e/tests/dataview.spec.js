// @ts-check
// A ```dataview fence renders as a query result inside the editor, and clicking
// the result puts you back in its source. This is the full vertical slice:
// vault_index refresh -> index::dql parse/execute/render -> MarkdownArea's
// fold/unfold, none of which the Rust unit tests can exercise together.

const { test, expect } = require("@playwright/test");
const { openFile, seedConfig, mockGitHub } = require("./helpers");

const VAULT = {
  "games/Pong.md": "---\nrating: 4\n---\n\nAn early one.",
  "games/Deus Ex.md": "---\nrating: 9\n---\n\nThe best one.",
  "notes/Diary.md": "---\nrating: 1\n---\n\nNot a game.",
  "Queries.md":
    "# Games\n\n```dataview\ntable rating\nfrom \"games\"\nsort rating desc\n```\n\nEnd.",
};

async function openQueries(page) {
  await mockGitHub(page, VAULT);
  await seedConfig(page);
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  await openFile(page, "Queries");
  await expect(page.locator(".md-area")).toBeVisible();
}

test("a dataview block renders its query result", async ({ page }) => {
  await openQueries(page);

  const table = page.locator(".dataview-table");
  await expect(table).toBeVisible();

  // FROM "games" selects only the two notes in that folder...
  const rows = table.locator("tbody tr");
  await expect(rows).toHaveCount(2);
  // ...and SORT rating DESC puts the 9 first.
  await expect(rows.nth(0)).toContainText("Deus Ex");
  await expect(rows.nth(0)).toContainText("9");
  await expect(rows.nth(1)).toContainText("Pong");
  await expect(page.locator(".dataview")).not.toContainText("Diary");

  // The query source is hidden, but the surrounding note is untouched.
  await expect(page.locator(".md-area")).toContainText("End.");
  await expect(page.locator(".md-render-host")).toBeAttached();
});

test("clicking the result reveals the query source, and leaving re-renders it", async ({
  page,
}) => {
  await openQueries(page);
  const editor = page.locator(".md-area");

  await page.locator(".dataview-table").click();

  // The block is now source: the table is gone and the query is editable text.
  await expect(page.locator(".dataview-table")).toHaveCount(0);
  await expect(editor).toContainText("sort rating desc");

  // Click a line outside the block; the query renders again.
  await editor.getByText("End.").click();
  await expect(page.locator(".dataview-table")).toBeVisible();
});

test("the block's source survives a round trip through the editor", async ({ page }) => {
  await openQueries(page);
  const editor = page.locator(".md-area");

  // Type on the last line. This drives read_state, which rebuilds the note
  // text from the DOM — if the rendered table counted as note content it would
  // be written into the file. It must not be.
  await editor.getByText("End.").click();
  await page.keyboard.press("End");
  await page.keyboard.type("!");

  await expect(editor).toContainText("End.!");
  // The query source is still exactly what it was, once.
  await expect(editor).toContainText("table rating");
  const text = (await editor.textContent()) || "";
  expect(text.match(/table rating/g) || []).toHaveLength(1);
});

test("a broken query shows a message instead of breaking the note", async ({ page }) => {
  await mockGitHub(page, {
    "Broken.md": "```dataview\ntabl rating\n```\n\nStill here.",
  });
  await seedConfig(page);
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  await openFile(page, "Broken");

  await expect(page.locator(".dataview-error")).toBeVisible();
  await expect(page.locator(".dataview-error")).toContainText("not a query type");
  await expect(page.locator(".md-area")).toContainText("Still here.");
});

test("a TASK result toggles the checkbox in the note it came from", async ({ page }) => {
  await mockGitHub(page, {
    "work/Todo.md": "- [ ] ship dataview\n- [x] write parser\n",
    "Board.md": "```dataview\ntask from \"work\"\n```\n\nEnd.",
  });
  await seedConfig(page);
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  await openFile(page, "Board");

  const items = page.locator(".dataview-task");
  await expect(items).toHaveCount(2);
  const ship = items.filter({ hasText: "ship dataview" });
  const box = ship.locator(".md-task-checkbox");
  await expect(box).toHaveAttribute("data-checked", "false");

  await box.click();
  // Flips immediately, without waiting for the write to land.
  await expect(box).toHaveAttribute("data-checked", "true");

  // ...and the write reached the file it came from, not the note we're in.
  await page.locator(".file-tree-dir-label", { hasText: "work" }).first().click();
  await openFile(page, "Todo");
  await expect(page.locator(".md-area")).toContainText("[x] ship dataview");
  await expect(page.locator(".md-area")).toContainText("[x] write parser");
});
