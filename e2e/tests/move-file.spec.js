// @ts-check
// Drag-and-drop file move: dragging a note onto a folder confirms, then moves it
// (the Contents API has no native move, so this is create-at-new + delete-old)
// and the sidebar reflects the new location. Drag data rides on a window global
// (window.__oxidianDragData) set by ondragstart and read by ondrop, so
// Playwright's native drag emulation drives the real handlers.

const { test, expect } = require("@playwright/test");
const { seedConfig, mockGitHub } = require("./helpers");

test("dragging a note onto a folder moves it there after confirmation", async ({ page }) => {
  page.on("dialog", (d) => d.accept());

  const requests = [];
  page.on("request", (r) => {
    if (r.url().includes("/contents/")) {
      requests.push(`${r.method()} ${decodeURIComponent(r.url().split("/contents/")[1].split("?")[0])}`);
    }
  });

  await mockGitHub(page, { "Inbox.md": "# Inbox\n", "Archive/.gitkeep": "" });
  await seedConfig(page);
  await page.goto("/");

  // Starts at the vault root.
  await expect(page.locator(".file-entry-name")).toHaveText(["Inbox.md"]);

  // Drag the note onto the Archive folder.
  const note = page.locator(".file-entry", { hasText: "Inbox" }).first();
  const folder = page.locator(".file-tree-dir-label", { hasText: "Archive" }).first();
  await note.dragTo(folder);

  // The move is create-at-new + delete-old (no native move in the Contents API).
  await expect
    .poll(() => requests.filter((r) => r === "PUT Archive/Inbox.md").length)
    .toBe(1);
  expect(requests).toContain("DELETE Inbox.md");

  // The note left the root; expanding Archive shows it in its new home.
  await expect(page.locator(".file-entry-name")).toHaveText([]);
  await folder.click();
  const archive = page.locator(".file-tree-dir", { hasText: "Archive" }).first();
  await expect(archive.locator(".file-entry-name")).toHaveText(["Inbox.md"]);
});
