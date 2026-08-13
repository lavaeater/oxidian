// @ts-check
// The Tasks panel can move a note's open tasks somewhere else: to today's
// periodic note, or to a note picked from a list. Covers the full slice —
// index::tasks::extract_lines/append_lines, the destination-first write order,
// and the panel refolding the index afterwards.

const { test, expect } = require("@playwright/test");
const { seedConfig, mockGitHub } = require("./helpers");

const WORK = "# Work\n\n- [ ] ship it 📅 2026-06-15\n- [x] already done\n- [ ] second thing\n";

const VAULT = {
  "Work.md": WORK,
  "Archive.md": "# Archive\n",
};

/** Today as YYYY-MM-DD, matching `dates::today()` in local time. */
function today() {
  const d = new Date();
  const p = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

/**
 * Boot into the Tasks panel. No daily-note template is present in the vault, so
 * "today's note" falls back to `YYYY-MM-DD.md` at the root — the same fallback
 * the "Today's note" button uses.
 */
async function openTasks(page, files) {
  await mockGitHub(page, files);
  await seedConfig(page);
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  await page.locator(".panel-tab[title='Tasks']").click();
  await expect(page.locator(".tasks-panel")).toBeVisible();
}

/** Records every write so a move can be checked at the network layer. */
function trackWrites(page) {
  const writes = [];
  page.on("request", (r) => {
    if (r.method() === "PUT" && r.url().includes("/contents/")) {
      let body = "";
      try {
        body = Buffer.from(r.postDataJSON().content, "base64").toString("utf-8");
      } catch {
        /* not a content write */
      }
      writes.push({
        path: decodeURIComponent(r.url().split("/contents/")[1].split("?")[0]),
        body,
      });
    }
  });
  return writes;
}

test("the group header offers both move buttons for a note with open tasks", async ({ page }) => {
  await openTasks(page, { ...VAULT });

  const group = page.locator(".tasks-group", { hasText: "Work" });
  await expect(group.getByRole("button", { name: "Move to today" })).toBeVisible();
  await expect(group.getByRole("button", { name: "Move to…" })).toBeVisible();
});

test("Move to today creates today's note and moves the open tasks into it", async ({ page }) => {
  const files = { ...VAULT };
  const writes = trackWrites(page);
  await openTasks(page, files);

  await page
    .locator(".tasks-group", { hasText: "Work" })
    .getByRole("button", { name: "Move to today" })
    .click();

  const dest = `${today()}.md`;
  await expect.poll(() => writes.map((w) => w.path)).toContain(dest);

  // The open tasks — with their metadata — landed in today's note...
  const written = writes.find((w) => w.path === dest);
  expect(written.body).toContain("- [ ] ship it 📅 2026-06-15");
  expect(written.body).toContain("- [ ] second thing");
  // ...and the completed one stayed behind as a record of what happened there.
  expect(written.body).not.toContain("already done");

  // The source note lost exactly those two lines.
  await expect.poll(() => files["Work.md"]).not.toContain("ship it");
  expect(files["Work.md"]).toContain("- [x] already done");
  expect(files["Work.md"]).toContain("# Work");
});

test("the destination is written before the source is emptied", async ({ page }) => {
  const writes = trackWrites(page);
  await openTasks(page, { ...VAULT });

  await page
    .locator(".tasks-group", { hasText: "Work" })
    .getByRole("button", { name: "Move to today" })
    .click();

  await expect.poll(() => writes.map((w) => w.path)).toContain("Work.md");
  // A failure between the two writes must duplicate the tasks, never lose them.
  const paths = writes.map((w) => w.path);
  expect(paths.indexOf(`${today()}.md`)).toBeLessThan(paths.indexOf("Work.md"));
});

test("Move to… moves the tasks into the note picked from the list", async ({ page }) => {
  const files = { ...VAULT };
  const writes = trackWrites(page);
  await openTasks(page, files);

  await page
    .locator(".tasks-group", { hasText: "Work" })
    .getByRole("button", { name: "Move to…" })
    .click();

  const picker = page.locator(".move-picker");
  await expect(picker).toBeVisible();
  await expect(picker).toContainText("Move 2 task(s) to…");
  // The note the tasks are leaving is not a destination.
  await expect(picker.locator(".move-picker-row", { hasText: "Work" })).toHaveCount(0);

  await picker.locator(".move-picker-row", { hasText: "Archive" }).click();

  await expect.poll(() => files["Archive.md"]).toContain("- [ ] ship it 📅 2026-06-15");
  expect(files["Archive.md"]).toContain("# Archive");
  await expect.poll(() => files["Work.md"]).not.toContain("second thing");
  expect(writes.some((w) => w.path === "Archive.md")).toBe(true);
});

test("the picker filters notes and Cancel moves nothing", async ({ page }) => {
  const files = { ...VAULT, "Ideas.md": "# Ideas\n" };
  const writes = trackWrites(page);
  await openTasks(page, files);

  await page
    .locator(".tasks-group", { hasText: "Work" })
    .getByRole("button", { name: "Move to…" })
    .click();

  const picker = page.locator(".move-picker");
  await picker.locator(".move-picker-input").fill("Ide");
  await expect(picker.locator(".move-picker-row")).toHaveCount(1);
  await expect(picker.locator(".move-picker-row")).toContainText("Ideas");

  await picker.getByRole("button", { name: "Cancel" }).click();
  await expect(picker).toBeHidden();
  expect(writes).toHaveLength(0);
  expect(files["Work.md"]).toBe(WORK);
});

test("a note whose tasks are all done offers no move buttons", async ({ page }) => {
  await openTasks(page, { "Done.md": "# Done\n\n- [x] finished\n" });

  // "Hide done" is on by default, so the group isn't even listed.
  await expect(page.locator(".tasks-panel")).toContainText("No tasks found.");
  await page.getByText("Hide done").click();

  const group = page.locator(".tasks-group", { hasText: "Done" });
  await expect(group).toBeVisible();
  await expect(group.getByRole("button", { name: "Move to today" })).toHaveCount(0);
});
