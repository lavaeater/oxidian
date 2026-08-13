// @ts-check
// Bullet Journal signifiers end-to-end: a vault using `[>]`, `[<]`, `[-]` and
// `[o]` is parsed, indexed, and shown as those states rather than being
// flattened back into done/not-done. See docs/bujo-roadmap.md §3.

const { test, expect } = require("@playwright/test");
const { seedConfig, mockGitHub } = require("./helpers");

const VAULT = {
  "Log.md": [
    "# Monday",
    "",
    "- [ ] draft the proposal",
    "- [x] pay rent",
    "- [>] call the plumber",
    "- [<] book flights",
    "- [-] learn the accordion",
    "- [o] standup at 10:00",
    "- [q] not a task at all",
    "- just a note",
    "",
  ].join("\n"),
};

async function openTasks(page, files = VAULT) {
  await mockGitHub(page, files);
  await seedConfig(page);
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  await page.locator(".panel-tab[title='Tasks']").click();
  await expect(page.locator(".tasks-panel")).toBeVisible();
}

test("only genuinely open entries are counted as open", async ({ page }) => {
  await openTasks(page);

  // Six parseable entries, but exactly one of them is still awaiting a
  // decision. Migrated, scheduled, dropped, done and events are all resolved,
  // so a count of "1 open" is the whole point of the status model.
  await expect(page.locator(".tasks-header")).toContainText("1 open");
  // "Hide done" is on by default, so the done entry is the only one hidden —
  // migrated and dropped stay visible, which is deliberate.
  await expect(page.locator(".task-row")).toHaveCount(5);

  await page.getByText("Hide done").click();
  await expect(page.locator(".task-row")).toHaveCount(6);
});

test("an unknown marker is not an entry", async ({ page }) => {
  await openTasks(page);
  // `- [q]` and a bare bullet must stay plain list items, or every stray
  // bracket in a note becomes a task in a mystery state.
  await expect(page.locator(".tasks-panel")).not.toContainText("not a task at all");
  await expect(page.locator(".tasks-panel")).not.toContainText("just a note");
});

test("resolved-but-not-done entries are shown as themselves", async ({ page }) => {
  await openTasks(page);

  const migrated = page.locator(".task-row", { hasText: "call the plumber" });
  // Dimmed, but not struck through and not counted as done — you are meant to
  // be able to read your own churn back.
  await expect(migrated).toHaveClass(/task-row--closed/);
  await expect(migrated.locator(".task-check")).toHaveAttribute("title", "migrated");

  const dropped = page.locator(".task-row", { hasText: "learn the accordion" });
  await expect(dropped.locator(".task-check")).toHaveAttribute("title", "dropped");

  await page.getByText("Hide done").click();
  const done = page.locator(".task-row", { hasText: "pay rent" });
  await expect(done).toHaveClass(/task-row--done/);
});

test("only open entries offer to be moved", async ({ page }) => {
  await openTasks(page);
  // One open entry, so the group still offers a move...
  await expect(
    page.locator(".tasks-group").getByRole("button", { name: "Move to today" }),
  ).toBeVisible();

  // ...but a note whose only entries are resolved offers nothing, because a
  // migrated task has already been dealt with.
  await openTasks(page, { "Done.md": "# Done\n\n- [>] moved on\n- [-] dropped\n" });
  await expect(
    page.locator(".tasks-group").getByRole("button", { name: "Move to today" }),
  ).toHaveCount(0);
});

test("clicking an unfinished entry of any state completes it", async ({ page }) => {
  // Observe writes rather than routing them: `mockGitHub` owns the routes, and
  // Playwright runs route handlers in reverse registration order.
  const writes = [];
  page.on("request", (r) => {
    if (r.method() === "PUT" && r.url().includes("/contents/")) {
      try {
        writes.push(Buffer.from(r.postDataJSON().content, "base64").toString("utf-8"));
      } catch {
        /* not a content write */
      }
    }
  });

  await openTasks(page);
  // A migrated entry, clicked, becomes done — with a completion stamp.
  await page
    .locator(".task-row", { hasText: "call the plumber" })
    .locator(".task-check")
    .click();

  await expect.poll(() => writes.length).toBeGreaterThan(0);
  expect(writes[0]).toContain("- [x] call the plumber ✅");
  // The other entries are untouched.
  expect(writes[0]).toContain("- [<] book flights");
  expect(writes[0]).toContain("- [-] learn the accordion");
});
