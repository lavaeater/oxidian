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
  // Both legs are polled: the DELETE is issued after the PUT resolves, so a bare
  // assertion here races the round trip whenever the machine is loaded.
  await expect
    .poll(() => requests.filter((r) => r === "PUT Archive/Inbox.md").length)
    .toBe(1);
  await expect.poll(() => requests).toContain("DELETE Inbox.md");

  // The note left the root; expanding Archive shows it in its new home.
  await expect(page.locator(".file-entry-name")).toHaveText([]);
  await folder.click();
  const archive = page.locator(".file-tree-dir", { hasText: "Archive" }).first();
  await expect(archive.locator(".file-entry-name")).toHaveText(["Inbox.md"]);
});

test("a folder move that partially fails still refreshes the tree to the real state", async ({ page }) => {
  // `move_dir` moves files one at a time and stops at the first error (e.g. a
  // name collision at the destination) — so a failed move can still be a
  // *partial* one: some files really did move server-side even though the
  // overall result is Err. The sidebar must reflect that, not the pre-move
  // snapshot (see views/vault.rs::handle_move).
  page.on("dialog", (d) => d.accept());
  await mockGitHub(page, {
    "Projects/Alpha.md": "# Alpha\n",
    "Projects/Beta.md": "# Beta\n",
    "Archive/.gitkeep": "",
  });
  // Fail only the Beta leg (Projects/Beta.md -> Archive/Projects/Beta.md); the
  // Alpha leg (which the sequential move attempts first) succeeds normally.
  await page.route(
    /api\.github\.com\/repos\/[^/]+\/[^/]+\/contents\/Archive\/Projects\/Beta\.md/,
    (route, request) => {
      if (request.method() === "PUT") {
        route.fulfill({ status: 422, json: { message: "File already exists" } });
      } else {
        route.continue();
      }
    }
  );
  await seedConfig(page);
  await page.goto("/");

  const projectsDir = page.locator(".file-tree-dir-label", { hasText: "Projects" }).first();
  const archiveDir = page.locator(".file-tree-dir-label", { hasText: "Archive" }).first();
  await projectsDir.dragTo(archiveDir);

  // Expand both top-level folders and check the real, partially-moved state:
  // Alpha.md under Archive/Projects, Beta.md still under the top-level Projects.
  await archiveDir.click();
  await expect(page.locator(".file-tree-dir-label", { hasText: "Projects" })).toHaveCount(2);
  // DOM order: Archive's own subtree (incl. its nested Projects) renders before
  // the sibling top-level Projects, so nth(0) is the nested one, nth(1) the
  // top-level one that still holds Beta.md.
  await page.locator(".file-tree-dir-label", { hasText: "Projects" }).nth(0).click();
  await page.locator(".file-tree-dir-label", { hasText: "Projects" }).nth(1).click();
  await expect(
    page.locator(".file-tree-dir", { hasText: "Archive" }).locator(".file-entry-name")
  ).toHaveText(["Alpha.md"]);
  // "Projects" also matches Archive's subtree once its nested Projects/Alpha.md
  // is expanded, so scope to the top-level dir whose *own* header label (not a
  // nested one) reads "Projects".
  const topLevelProjects = page.locator(".file-tree > .file-tree-dir").filter({
    has: page.locator(":scope > .file-tree-dir-name .file-tree-dir-label", { hasText: "Projects" }),
  });
  await expect(topLevelProjects.locator(".file-entry-name")).toHaveText(["Beta.md"]);
});
