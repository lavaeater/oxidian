// @ts-check
// Search runs against the local index, not GitHub's code-search API — that
// endpoint sends no CORS headers, so every search from the browser failed with
// a network error. These tests assert both halves of that: results appear, and
// no search request leaves the page.

const { test, expect } = require("@playwright/test");
const { seedConfig, mockGitHub } = require("./helpers");

const VAULT = {
  "notes/Coffee.md": "---\ntags: [drink]\n---\n\n# Brewing\n\nGrind the beans fresh.",
  "notes/Tea.md": "# Tea\n\nGreen tea is nice.",
  "Journal.md": "Drank coffee today.",
};

async function openSearch(page) {
  await seedConfig(page);
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  // Wait for the index to be populated — search is only as good as it is.
  await expect(page.locator(".storage-footer")).toContainText("3 notes indexed");
  await page.locator(".panel-tab[title='Search']").click();
  await expect(page.locator(".search-input")).toBeVisible();
}

test("searching finds notes by body text, with context", async ({ page }) => {
  await mockGitHub(page, VAULT);
  await openSearch(page);

  await page.locator(".search-input").fill("beans");

  const items = page.locator(".search-item");
  await expect(items).toHaveCount(1);
  await expect(items.first()).toContainText("Coffee");
  await expect(items.first()).toContainText("Grind the beans fresh.");

  // Opening a result loads that note.
  await items.first().click();
  await expect(page.locator(".md-area")).toContainText("Grind the beans fresh.");
});

test("a title match is ranked above a body match", async ({ page }) => {
  await mockGitHub(page, VAULT);
  await openSearch(page);

  await page.locator(".search-input").fill("coffee");

  const items = page.locator(".search-item");
  await expect(items).toHaveCount(2);
  await expect(items.nth(0)).toContainText("Coffee");
  await expect(items.nth(0)).toContainText("title");
  await expect(items.nth(1)).toContainText("Journal");
  await expect(items.nth(1)).toContainText("text");
});

test("search never calls the GitHub search API", async ({ page }) => {
  await mockGitHub(page, VAULT);
  // Anything reaching /search/ would have been blocked by CORS in a real
  // browser session; fail loudly instead of silently returning nothing.
  let searchCalls = 0;
  await page.route(/api\.github\.com\/search\//, (route) => {
    searchCalls++;
    route.fulfill({ json: { items: [] } });
  });

  await openSearch(page);
  await page.locator(".search-input").fill("coffee");
  await expect(page.locator(".search-item")).toHaveCount(2);

  expect(searchCalls).toBe(0);
});

test("no results says so, and an empty query says what it would search", async ({ page }) => {
  await mockGitHub(page, VAULT);
  await openSearch(page);

  await page.locator(".search-input").fill("zzzznope");
  await expect(page.locator(".search-empty")).toContainText("No results");

  await page.locator(".search-input").fill("");
  await expect(page.locator(".search-panel")).toContainText("3 indexed notes");
});
