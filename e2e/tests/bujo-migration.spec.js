// @ts-check
// The migration review: closing a period by deciding about each unfinished
// entry. The load-bearing property is that migrating never *moves* a line — the
// original is re-marked where it stands, so the closing log keeps an honest
// record of what was on it. See docs/bujo-roadmap.md §5.

const { test, expect } = require("@playwright/test");
const { seedConfig, mockGitHub } = require("./helpers");

// No daily-note template exists in this vault, so a daily log resolves to
// `YYYY-MM-DD.md` at the vault root — the same fallback the "Today's note"
// button uses, and what tasks-move.spec.js relies on. That keeps this spec
// about migration rather than about template resolution, which phase 1 covers.
function ymd(d) {
  const p = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}
const TODAY = ymd(new Date());
const YESTERDAY = ymd(new Date(Date.now() - 86400000));

const YESTERDAY_LOG = [
  `# ${YESTERDAY}`,
  "",
  "- [ ] call the plumber",
  "- [ ] draft the proposal 📅 2026-09-01",
  "- [ ] learn the accordion",
  "- [x] pay rent ✅ " + YESTERDAY,
  "",
].join("\n");

const VAULT = {
  [`${YESTERDAY}.md`]: YESTERDAY_LOG,
  "Readme.md": "# Notes\n",
};

/** Records every write so the two-file result can be checked at the wire. */
function trackWrites(page) {
  const writes = [];
  page.on("request", (r) => {
    if (r.method() === "PUT" && r.url().includes("/contents/")) {
      try {
        writes.push({
          path: decodeURIComponent(r.url().split("/contents/")[1].split("?")[0]),
          body: Buffer.from(r.postDataJSON().content, "base64").toString("utf-8"),
        });
      } catch {
        /* not a content write */
      }
    }
  });
  return writes;
}

async function openReview(page, vault = VAULT) {
  // A copy: the mock writes back into the object it's given, and a shared
  // VAULT would carry one test's migrations into the next.
  await mockGitHub(page, { ...vault });
  // A weekly template makes the period switcher (and its Review button) appear;
  // the review itself runs on whichever scale is selected, Day by default.
  await seedConfig(page, { weekly_note_template: ".oxidian/templates/weekly.md" });
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Review" }).click();
  await expect(page.locator(".review-modal")).toBeVisible();
}

test("the review lists yesterday's unfinished entries, and only those", async ({ page }) => {
  await openReview(page);

  await expect(page.locator(".review-modal")).toContainText(`${YESTERDAY} → ${TODAY}`);
  const rows = page.locator(".review-row");
  await expect(rows).toHaveCount(3);
  await expect(page.locator(".review-modal")).toContainText("call the plumber");
  // A completed entry is not up for review — it was already dealt with.
  await expect(page.locator(".review-modal")).not.toContainText("pay rent");
  // A due date rides along so the decision is an informed one.
  await expect(page.locator(".review-row", { hasText: "draft the proposal" }))
    .toContainText("📅 2026-09-01");
});

test("nothing is written until Apply, and Apply needs a decision", async ({ page }) => {
  const writes = trackWrites(page);
  await openReview(page);

  // There is no "carry everything" default: the friction is the point.
  await expect(page.locator(".review-modal")).toContainText("0 of 3 decided");
  await expect(page.getByRole("button", { name: "Apply" })).toBeDisabled();

  await page.locator(".review-row", { hasText: "call the plumber" })
    .getByRole("button", { name: "Migrate" }).click();
  await expect(page.locator(".review-modal")).toContainText("1 of 3 decided");
  await expect(page.getByRole("button", { name: "Apply" })).toBeEnabled();
  expect(writes, "deciding must not write anything on its own").toHaveLength(0);

  // Clicking the same decision again undoes it, so a misclick isn't binding.
  await page.locator(".review-row", { hasText: "call the plumber" })
    .getByRole("button", { name: "Migrate" }).click();
  await expect(page.locator(".review-modal")).toContainText("0 of 3 decided");
});

test("migrating re-marks the original and opens a fresh copy in today's log", async ({ page }) => {
  const writes = trackWrites(page);
  await openReview(page);

  await page.locator(".review-row", { hasText: "call the plumber" })
    .getByRole("button", { name: "Migrate" }).click();
  await page.locator(".review-row", { hasText: "learn the accordion" })
    .getByRole("button", { name: "Drop" }).click();
  await page.getByRole("button", { name: "Apply" }).click();

  await expect.poll(() => writes.length).toBeGreaterThanOrEqual(2);

  // Destination first — a failure between the writes must duplicate, not lose.
  expect(writes[0].path).toBe(`${TODAY}.md`);
  expect(writes[0].body).toContain("- [ ] call the plumber");

  const source = writes.find((w) => w.path === `${YESTERDAY}.md`);
  expect(source, "the source log is rewritten too").toBeTruthy();
  // The entry is still there, marked as having moved on — not deleted.
  expect(source.body).toContain("- [>] call the plumber");
  expect(source.body).toContain("- [-] learn the accordion");
  // Untouched entries and the completed one are exactly as they were.
  expect(source.body).toContain("- [ ] draft the proposal 📅 2026-09-01");
  expect(source.body).toContain(`- [x] pay rent ✅ ${YESTERDAY}`);
});

test("completing an entry during the review stamps it where it stands", async ({ page }) => {
  const writes = trackWrites(page);
  await openReview(page);

  await page.locator(".review-row", { hasText: "call the plumber" })
    .getByRole("button", { name: "Done" }).click();
  await page.getByRole("button", { name: "Apply" }).click();

  await expect.poll(() => writes.length).toBeGreaterThan(0);
  const source = writes.find((w) => w.path === `${YESTERDAY}.md`);
  expect(source.body).toContain(`- [x] call the plumber ✅ ${TODAY}`);
  // Nothing was carried forward, so today's log is never touched.
  expect(writes.some((w) => w.path === `${TODAY}.md`)).toBe(false);
});

// A monthly template gives the review a month log to defer into — and the
// backlog waiting in it to pull from.
const THIS_MONTH = TODAY.slice(0, 7);
const MONTH_LOG = `monthly/${THIS_MONTH}.md`;
const MONTHLY_TEMPLATE = [
  "---",
  "oxid_template:",
  "  filepath: monthly/${OXID_DATE_YEAR}-${OXID_DATE_MONTH}.md",
  "---",
  "# ${OXID_DATE_YEAR}-${OXID_DATE_MONTH}",
  "",
].join("\n");

async function openReviewWithMonth(page, vault = VAULT) {
  await mockGitHub(page, { ...vault, ".oxidian/templates/monthly.md": MONTHLY_TEMPLATE });
  await seedConfig(page, {
    weekly_note_template: ".oxidian/templates/weekly.md",
    monthly_note_template: ".oxidian/templates/monthly.md",
  });
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Review" }).click();
  await expect(page.locator(".review-modal")).toBeVisible();
}

test("without a monthly log there is nowhere to defer to, so no To month", async ({ page }) => {
  await openReview(page);
  await expect(page.getByRole("button", { name: "To month" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Schedule" })).toHaveCount(0);
});

test("To month defers an entry up into the month log", async ({ page }) => {
  const writes = trackWrites(page);
  await openReviewWithMonth(page);

  await page.locator(".review-row", { hasText: "learn the accordion" })
    .getByRole("button", { name: "To month" }).click();
  await page.getByRole("button", { name: "Apply" }).click();

  await expect.poll(() => writes.length).toBeGreaterThanOrEqual(2);
  // Destination first, as with any carry.
  expect(writes[0].path).toBe(MONTH_LOG);
  expect(writes[0].body).toContain("- [ ] learn the accordion");
  // The original records that it went back up, not forward.
  const source = writes.find((w) => w.path === `${YESTERDAY}.md`);
  expect(source.body).toContain("- [<] learn the accordion");
  expect(writes.some((w) => w.path === `${TODAY}.md`)).toBe(false);
});

test("the month backlog is listed, and Pull in brings an entry down into today", async ({ page }) => {
  const writes = trackWrites(page);
  await openReviewWithMonth(page, {
    ...VAULT,
    [MONTH_LOG]: `# ${THIS_MONTH}\n\n- [ ] fix the bike\n- [ ] sort the attic\n- [x] book flights\n`,
  });

  const backlog = page.locator(".review-list--backlog .review-row");
  await expect(backlog).toHaveCount(2);
  await expect(page.locator(".review-list--backlog")).not.toContainText("book flights");

  await backlog.filter({ hasText: "fix the bike" })
    .getByRole("button", { name: "Pull in" }).click();
  await backlog.filter({ hasText: "sort the attic" })
    .getByRole("button", { name: "Done" }).click();
  // Pulling from the backlog alone is enough to apply.
  await expect(page.locator(".review-modal")).toContainText("0 of 3 decided · 2 from backlog");
  await page.getByRole("button", { name: "Apply" }).click();

  await expect.poll(() => writes.length).toBeGreaterThanOrEqual(2);
  expect(writes[0].path).toBe(`${TODAY}.md`);
  expect(writes[0].body).toContain("- [ ] fix the bike");
  const month = writes.find((w) => w.path === MONTH_LOG);
  expect(month.body).toContain("- [>] fix the bike");
  expect(month.body).toContain(`- [x] sort the attic ✅ ${TODAY}`);
});

test("the log is resolved through the template, not the root fallback", async ({ page }) => {
  // Regression: templates are read from the vault, so they arrive a beat after
  // the app does. Acting immediately used to snapshot an empty template list —
  // indistinguishable from "no template configured" — and resolve the log to
  // `YYYY-MM-DD.md` at the vault root, silently ignoring the journal folder.
  const template = [
    "---",
    "oxid_template:",
    "  filepath: journal/${OXID_DATE_YEAR}-${OXID_DATE_MONTH}-${OXID_DATE_DATE}.md",
    "---",
    "# ${OXID_DATE_YEAR}-${OXID_DATE_MONTH}-${OXID_DATE_DATE}",
    "",
  ].join("\n");

  const writes = trackWrites(page);
  await mockGitHub(page, {
    ".oxidian/templates/daily-note.md": template,
    [`journal/${YESTERDAY}.md`]: YESTERDAY_LOG,
    // A decoy at the root: if resolution falls back, the review finds this
    // instead and the test would pass for the wrong reason. It must not.
    [`${YESTERDAY}.md`]: `# ${YESTERDAY}\n\n- [ ] the wrong note\n`,
  });
  await seedConfig(page, { weekly_note_template: ".oxidian/templates/weekly.md" });
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  // No settling wait — acting at once is the whole point of the test.
  await page.getByRole("button", { name: "Review" }).click();
  await expect(page.locator(".review-modal")).toBeVisible();

  await expect(page.locator(".review-modal")).toContainText("call the plumber");
  await expect(page.locator(".review-modal")).not.toContainText("the wrong note");

  await page.locator(".review-row", { hasText: "call the plumber" })
    .getByRole("button", { name: "Migrate" }).click();
  await page.getByRole("button", { name: "Apply" }).click();

  await expect.poll(() => writes.length).toBeGreaterThanOrEqual(2);
  // Both sides land in the journal folder the template names.
  expect(writes[0].path).toBe(`journal/${TODAY}.md`);
  expect(writes.some((w) => w.path === `journal/${YESTERDAY}.md`)).toBe(true);
  expect(writes.some((w) => w.path === `${TODAY}.md`)).toBe(false);
});

test("a period with nothing open says so instead of offering busywork", async ({ page }) => {
  await openReview(page, {
    ...VAULT,
    [`${YESTERDAY}.md`]: `# ${YESTERDAY}\n\n- [x] all done ✅ ${YESTERDAY}\n`,
  });
  await expect(page.locator(".review-empty")).toContainText("Nothing left open");
  await expect(page.getByRole("button", { name: "Apply" })).toHaveCount(0);
});
