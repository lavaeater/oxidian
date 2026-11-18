// @ts-check
// The right rail (ui-polish.md: "Modify Size of Sidebar"): outline + backlinks
// in their own column on wide screens, so the left sidebar is all file list.
// On phones the rail is hidden and the outline stays in the Files panel.

const { test, expect } = require("@playwright/test");
const { openFile, seedConfig, mockGitHub } = require("./helpers");

const BODY = Array.from({ length: 60 }, (_, i) => `filler ${i + 1}`).join("\n");
const FILES = {
  "Note.md": `# Top\n\n${BODY}\n\n## Deep section\n\nend`,
  "Ref.md": "See [[Note]] for details.",
};

async function openNote(page) {
  await mockGitHub(page, FILES);
  await seedConfig(page);
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  // The backlink index learns a note's links when it is opened, so open the
  // linking note first (existing behaviour of the Backlinks panel too).
  await openFile(page, "Ref");
  await expect(page.locator(".editor-filename")).toHaveText("Ref.md");
  await openFile(page, "Note");
  await expect(page.locator(".editor-filename")).toHaveText("Note.md");
}

async function enableRail(page) {
  await page.locator('.tab-strip-actions button[title^="Layout"]').click();
  await page.locator(".layout-menu-rail").getByRole("checkbox").check();
  await page.locator(".layout-menu-scrim").click({ position: { x: 5, y: 300 } });
}

test("the rail takes the outline out of the sidebar and shows backlinks", async ({ page }) => {
  await openNote(page);
  await expect(page.locator(".right-rail")).toHaveCount(0);
  await expect(page.locator(".sidebar .outline-pane")).toBeVisible();

  await enableRail(page);
  const rail = page.locator(".right-rail");
  await expect(rail.locator(".outline-item")).toHaveText(["Top", "Deep section"]);
  await expect(rail.locator(".backlinks-panel")).toContainText("Ref.md");
  // The sidebar copy is hidden on wide screens.
  await expect(page.locator(".sidebar .outline-pane")).toBeHidden();

  // Remembered across reloads; the close button turns it off again.
  await page.reload();
  await openFile(page, "Note");
  await expect(page.locator(".editor-filename")).toHaveText("Note.md");
  await expect(page.locator(".right-rail")).toBeVisible();
  await page.locator(".rail-close").click();
  await expect(page.locator(".right-rail")).toHaveCount(0);
  await expect(page.locator(".sidebar .outline-pane")).toBeVisible();
});

test("clicking an outline entry scrolls the editor to that heading", async ({ page }) => {
  await openNote(page);
  await enableRail(page);

  const heading = page.locator(".md-area .md-heading.md-h2");
  await expect(heading).not.toBeInViewport();
  await page.locator(".right-rail .outline-item", { hasText: "Deep section" }).click();
  await expect(heading).toBeInViewport();
});

test("clicking a backlink in the rail opens that note", async ({ page }) => {
  await openNote(page);
  await enableRail(page);
  await page.locator(".right-rail .backlinks-panel .file-entry", { hasText: "Ref.md" }).click();
  await expect(page.locator(".editor-filename")).toHaveText("Ref.md");
});

test.describe("phone", () => {
  test.use({ viewport: { width: 390, height: 800 } });

  test("the rail stays hidden and the outline stays in the Files panel", async ({ page }) => {
    await mockGitHub(page, FILES);
    // Rail switched on (e.g. synced prefs) — a phone must still cope.
    await page.addInitScript(() => localStorage.setItem("oxidian_ui", JSON.stringify({ right_rail: true })));
    await seedConfig(page);
    await page.goto("/");
    await openFile(page, "Note");
    await expect(page.locator(".md-area")).toBeVisible();
    await expect(page.locator(".right-rail")).toBeHidden();

    await page.locator(".bottom-nav-btn", { hasText: "Files" }).click();
    await expect(page.locator(".sidebar .outline-pane .outline-item").first()).toBeVisible();
  });
});
