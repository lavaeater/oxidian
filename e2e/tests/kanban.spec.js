// @ts-check
// Kanban board: a board *is* a markdown document — `## headings` are columns and
// `- [[Title]]` items are cards, each card's note living at
// `<board-dir>/<Column>/<Title>.md`. These specs cover the board rendering and a
// card drag between columns (which rewrites the board doc AND moves the note
// file between column folders). Card drag rides the same window-global drag data
// as the file tree, so Playwright's native drag emulation drives it.

const { test, expect } = require("@playwright/test");
const { seedConfig, seedBoard, mockGitHub } = require("./helpers");

const BOARD = {
  "kanban/kanban.md": "## To Do\n\n- [[Task A]]\n\n## Doing\n\n- [[Task B]]\n\n## Done\n",
  "kanban/To Do/Task A.md": "# Task A\n",
  "kanban/Doing/Task B.md": "# Task B\n",
};

/** Return "Column -> [card, …]" summaries for every rendered column. */
function columnSummaries(page) {
  return page.locator(".kanban-col").evaluateAll((cols) =>
    cols.map((c) => {
      const name = c.querySelector(".kanban-col-header")?.textContent.replace(/\s+/g, " ").trim() ?? "?";
      const cards = [...c.querySelectorAll(".kanban-card")].map((x) => x.textContent.trim());
      return `${name} -> [${cards.join(",")}]`;
    })
  );
}

async function openBoard(page, files = BOARD) {
  await mockGitHub(page, files);
  await seedConfig(page);
  await seedBoard(page, "kanban");
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  await page.locator('button[title="Kanban"]').click();
  await expect(page.locator(".kanban-board")).toBeVisible();
}

test("renders columns and cards from the board document", async ({ page }) => {
  await openBoard(page);

  // Three columns in document order, cards under the right headings.
  await expect(page.locator(".kanban-col-header")).toContainText(["To Do", "Doing", "Done"]);
  const toDo = page.locator(".kanban-col", { hasText: "To Do" });
  await expect(toDo.locator(".kanban-card")).toHaveText(["Task A"]);
  const done = page.locator(".kanban-col", { hasText: "Done" });
  await expect(done.locator(".kanban-card")).toHaveCount(0);
});

test("dragging a card to another column moves it and rewrites the board", async ({ page }) => {
  const requests = [];
  page.on("request", (r) => {
    if (r.url().includes("/contents/")) {
      requests.push(`${r.method()} ${decodeURIComponent(r.url().split("/contents/")[1].split("?")[0])}`);
    }
  });

  await openBoard(page);
  expect(await columnSummaries(page)).toEqual(
    expect.arrayContaining(["To Do1 -> [Task A]", "Done0 -> []"])
  );

  // Drag "Task A" from To Do into the Done column body.
  const card = page.locator(".kanban-card", { hasText: "Task A" }).first();
  const doneBody = page.locator(".kanban-col", { hasText: "Done" }).locator(".kanban-col-body").first();
  await card.dragTo(doneBody);

  // The card now lives under Done, and To Do is empty.
  await expect(page.locator(".kanban-col", { hasText: "Done" }).locator(".kanban-card")).toHaveText(["Task A"]);
  await expect(page.locator(".kanban-col", { hasText: "To Do" }).locator(".kanban-card")).toHaveCount(0);

  // The note file moved between column folders and the board doc was rewritten.
  await expect.poll(() => requests).toContain("PUT kanban/Done/Task A.md");
  expect(requests).toContain("DELETE kanban/To Do/Task A.md");
  expect(requests).toContain("PUT kanban/kanban.md");
});
