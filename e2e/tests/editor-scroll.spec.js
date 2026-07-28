// @ts-check
// Comfortable-scroll: when you write near the bottom of the viewport the caret
// gets cramped against the edge. The editor keeps the *active line* inside a
// comfortable vertical band of the scroll container, nudging the document up
// once you pause typing (markdown_area.js `setup_scroll`). This drives the real
// DOM glue — the native SSR tests have no layout/scroll to measure.

const { test, expect } = require("@playwright/test");
const { seedConfig, mockGitHub } = require("./helpers");

/** Boot into the vault and open `Note.md`. */
async function openNote(page, files) {
  await mockGitHub(page, files);
  await seedConfig(page);
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  await page.getByText("Note", { exact: false }).first().click();
  await expect(page.locator(".md-area")).toBeVisible();
}

/**
 * The active writing line's top, as a fraction of the scroll container's height
 * (0 = top edge, 1 = bottom edge). Null if there's no active line yet.
 */
function activeLineRatio(page) {
  return page.evaluate(() => {
    const el = document.querySelector(".md-area");
    const line = el && el.querySelector(".md-line--active");
    if (!line) return null;
    // The editor element itself scrolls (`.md-area` is overflow-y:auto); fall
    // back to a scrollable ancestor only if it doesn't overflow.
    let cont = el;
    while (cont) {
      const oy = getComputedStyle(cont).overflowY;
      if ((oy === "auto" || oy === "scroll") && cont.scrollHeight > cont.clientHeight + 1) break;
      cont = cont.parentElement;
    }
    if (!cont) return null;
    const y = line.getBoundingClientRect().top - cont.getBoundingClientRect().top;
    return y / cont.clientHeight;
  });
}

test("typing near the bottom recenters the writing line", async ({ page }) => {
  // A tall document so the editor column actually scrolls.
  const body = Array.from({ length: 80 }, (_, i) => `Line ${i + 1}`).join("\n");
  await openNote(page, { "Note.md": body });

  // Put the caret on the last line, then scroll it down near the bottom edge —
  // the cramped situation the feature is meant to relieve. (Scroll-past-end
  // padding under the last line is what makes recentering it even possible.)
  await page.getByText("Line 80", { exact: true }).first().click();
  await page.evaluate(() => {
    const el = document.querySelector(".md-area");
    const line = el.querySelector(".md-line--active");
    const y = line.getBoundingClientRect().top - el.getBoundingClientRect().top;
    el.scrollTop += y - el.clientHeight * 0.88; // active line at ~88% down
  });
  await expect.poll(() => activeLineRatio(page)).toBeGreaterThan(0.7);

  // Type: this is "editing", which schedules the debounced recenter.
  await page.keyboard.type(" more");

  // The writing line is nudged up into the comfortable band (~0.42) and is no
  // longer pinned to the bottom edge.
  await expect.poll(() => activeLineRatio(page), { timeout: 4000 }).toBeLessThan(0.62);
  expect(await activeLineRatio(page)).toBeGreaterThan(0.25);
});

test("editing a short document does not scroll (nothing to recenter)", async ({ page }) => {
  // A document that fits the viewport has no scrollable ancestor, so recentering
  // is a no-op and typing must never jump the view.
  await openNote(page, { "Note.md": "# Title\n\nShort note." });

  await page.getByText("Short note.", { exact: false }).click();
  await page.keyboard.press("End");
  await page.keyboard.type(" edited");
  await expect(page.locator(".md-area")).toContainText("Short note. edited");

  // No scrollable ancestor overflowed → ratio math finds no container.
  expect(await activeLineRatio(page)).toBeNull();
});
