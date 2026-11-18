// @ts-check
// Editor layout (ui-polish.md: "Conserve vertical space" + "Modify Size of
// Sidebar"): bar visibility/position via the tab-strip layout menu, Super
// Collapse, remembered folding of properties and the outline.

const { test, expect } = require("@playwright/test");
const { openFile, seedConfig, mockGitHub } = require("./helpers");

const NOTE = "---\ntags: demo\n---\n\n# Heading one\n\nBody text.\n\n## Heading two";

async function openNote(page) {
  await mockGitHub(page, { "Note.md": NOTE });
  await seedConfig(page);
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  await openFile(page, "Note");
  await expect(page.locator(".md-area")).toBeVisible();
}

const layoutBtn = (page) => page.locator('.tab-strip-actions button[title^="Layout"]');
const row = (page, label) => page.locator(".layout-menu-row").filter({ hasText: label });

test("defaults: status bar at the bottom, properties folded, formatting on top", async ({ page }) => {
  await openNote(page);
  await expect(page.locator(".editor-bars--bottom .editor-titlebar")).toBeVisible();
  await expect(page.locator(".editor-bars--top .editor-titlebar")).toHaveCount(0);
  await expect(page.locator(".editor-bars--top .formatting-toolbar")).toBeVisible();
  await expect(page.locator(".props-panel")).toBeVisible();
  await expect(page.locator(".props-body")).toHaveCount(0);
});

test("the layout menu hides and moves bars, and it is remembered", async ({ page }) => {
  await openNote(page);
  await layoutBtn(page).click();
  await expect(page.locator(".layout-menu")).toBeVisible();

  await row(page, "Formatting toolbar").getByRole("checkbox").uncheck();
  await expect(page.locator(".formatting-toolbar")).toHaveCount(0);

  await row(page, "Status bar").getByRole("button", { name: "Top" }).click();
  await expect(page.locator(".editor-bars--top .editor-titlebar")).toBeVisible();
  await expect(page.locator(".editor-bars--bottom")).toHaveCount(0);

  // Click-away closes the menu.
  await page.locator(".layout-menu-scrim").click({ position: { x: 5, y: 300 } });
  await expect(page.locator(".layout-menu")).toHaveCount(0);

  // Survives a reload.
  await page.reload();
  await openFile(page, "Note");
  await expect(page.locator(".editor-bars--top .editor-titlebar")).toBeVisible();
  await expect(page.locator(".formatting-toolbar")).toHaveCount(0);
});

test("Super Collapse leaves only the tab strip", async ({ page }) => {
  await openNote(page);
  await layoutBtn(page).click();
  await page.locator(".layout-menu-super").getByRole("checkbox").check();

  await expect(page.locator(".editor-bars")).toHaveCount(0);
  await expect(page.locator(".tab-strip")).toBeVisible();
  await expect(layoutBtn(page)).toBeVisible();

  await page.locator(".layout-menu-super").getByRole("checkbox").uncheck();
  await expect(page.locator(".editor-titlebar")).toBeVisible();
});

test("properties and outline folding is remembered across notes", async ({ page }) => {
  await openNote(page);
  await page.locator(".props-header").click();
  await expect(page.locator(".props-body")).toBeVisible();

  const outline = page.locator(".outline-pane");
  await expect(outline.locator(".outline-item")).toHaveCount(2);
  await outline.locator(".outline-title").click();
  await expect(outline.locator(".outline-item")).toHaveCount(0);

  await page.reload();
  await openFile(page, "Note");
  await expect(page.locator(".props-body")).toBeVisible();
  await expect(page.locator(".outline-pane .outline-item")).toHaveCount(0);
});
