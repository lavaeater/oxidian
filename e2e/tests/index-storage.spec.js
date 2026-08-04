// @ts-check
// The vault index lives in IndexedDB, not localStorage. That distinction is the
// ceiling on vault size (localStorage caps out around 5 MB), so it's worth
// asserting where the bytes actually land — and that an index left in
// localStorage by an older build is moved rather than abandoned.

const { test, expect } = require("@playwright/test");
const { seedConfig, mockGitHub } = require("./helpers");

const INDEX_KEY = "oxidian_index_v1";

/** Read a key from the app's IndexedDB blob store, in the page. */
function readBlob(page, key) {
  return page.evaluate(
    (k) =>
      new Promise((resolve) => {
        const req = indexedDB.open("oxidian", 1);
        req.onerror = () => resolve(null);
        req.onsuccess = () => {
          const db = req.result;
          if (!db.objectStoreNames.contains("blobs")) return resolve(null);
          const get = db.transaction("blobs", "readonly").objectStore("blobs").get(k);
          get.onsuccess = () => resolve(get.result ?? null);
          get.onerror = () => resolve(null);
        };
      }),
    key
  );
}

const VAULT = {
  "Alpha.md": "---\ntags: [a]\n---\n\n# Alpha",
  "Beta.md": "# Beta\n\n- [ ] something",
};

test("the index is written to IndexedDB and not to localStorage", async ({ page }) => {
  await mockGitHub(page, VAULT);
  await seedConfig(page);
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();

  // The footer reports the index, so it doubles as the signal that the refresh
  // has finished writing.
  await expect(page.locator(".storage-footer")).toContainText("2 notes indexed");

  const blob = await readBlob(page, INDEX_KEY);
  expect(blob, "index should be in IndexedDB").toBeTruthy();
  expect(JSON.parse(blob).pages).toHaveProperty(["Alpha.md"]);

  const ls = await page.evaluate((k) => window.localStorage.getItem(k), INDEX_KEY);
  expect(ls, "index must not occupy the localStorage budget").toBeNull();
});

test("an index left in localStorage by an older build is migrated", async ({ page }) => {
  await mockGitHub(page, VAULT);
  await seedConfig(page);
  // A v1 index as the previous build would have written it, plus the cache that
  // preceded the index entirely.
  const legacy = JSON.stringify({
    version: 1,
    pages: {
      "Alpha.md": {
        path: "Alpha.md",
        sha: "sha-Alpha.md",
        fields: {},
        tags: ["a"],
        links: [],
        headings: [],
        tasks: [],
      },
    },
  });
  await page.addInitScript(
    ([k, v]) => {
      window.localStorage.setItem(k, v);
      window.localStorage.setItem("oxidian_tasks_cache", "[]");
    },
    [INDEX_KEY, legacy]
  );

  await page.goto("/");
  await expect(page.locator(".storage-footer")).toContainText("notes indexed");

  // Moved, not copied: nothing is left holding localStorage quota.
  await expect
    .poll(() => page.evaluate((k) => window.localStorage.getItem(k), INDEX_KEY))
    .toBeNull();
  expect(
    await page.evaluate(() => window.localStorage.getItem("oxidian_tasks_cache"))
  ).toBeNull();

  const blob = await readBlob(page, INDEX_KEY);
  expect(JSON.parse(blob).pages).toHaveProperty(["Alpha.md"]);
});

test("Rebuild discards the index and reads the vault again", async ({ page }) => {
  await mockGitHub(page, VAULT);
  await seedConfig(page);
  // Count note reads, so "it read the vault again" is asserted rather than
  // assumed — the note count alone would be identical before and after.
  let reads = 0;
  await page.route(/api\.github\.com\/repos\/[^/]+\/[^/]+\/contents\/(.+)/, (route, request) => {
    if (request.method() === "GET") reads++;
    route.fallback();
  });

  await page.goto("/");
  const footer = page.locator(".storage-footer");
  await expect(footer).toContainText("2 notes indexed");
  const before = reads;
  expect(before).toBeGreaterThan(0);

  await footer.getByRole("button", { name: /rebuild/i }).click();

  // It comes back on its own: the index is a cache, so clearing it costs a
  // refresh and nothing else.
  await expect.poll(() => reads).toBeGreaterThan(before);
  await expect(footer).toContainText("2 notes indexed");
  const blob = await readBlob(page, INDEX_KEY);
  expect(JSON.parse(blob).pages).toHaveProperty(["Beta.md"]);
});
