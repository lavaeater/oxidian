// @ts-check
// The vault index lives in IndexedDB, one record per note — not in localStorage
// (a ~5 MB cap) and not as a single blob (which every save would rewrite whole,
// and the index carries every note's text for search). These tests pin the
// storage shape, since that shape is what bounds how large a vault can get.

const { test, expect } = require("@playwright/test");
const { openFile, seedConfig, mockGitHub } = require("./helpers");

const INDEX_KEY = "oxidian_index_v1";

/** Read from one of the app's IndexedDB stores, in the page. */
function readStore(page, store) {
  return page.evaluate(
    (s) =>
      new Promise((resolve) => {
        const req = indexedDB.open("oxidian", 2);
        req.onerror = () => resolve(null);
        req.onsuccess = () => {
          const db = req.result;
          if (!db.objectStoreNames.contains(s)) return resolve(null);
          const t = db.transaction(s, "readonly");
          const st = t.objectStore(s);
          const keys = st.getAllKeys();
          const vals = st.getAll();
          t.oncomplete = () =>
            resolve(keys.result.map((k, i) => [k, vals.result[i]]));
          t.onerror = () => resolve(null);
        };
      }),
    store
  );
}

/** The per-note index records, as an object of path -> parsed page. */
async function readPages(page) {
  const rows = (await readStore(page, "pages")) || [];
  return Object.fromEntries(rows.map(([k, v]) => [k, JSON.parse(v)]));
}

const VAULT = {
  "Alpha.md": "---\ntags: [a]\n---\n\n# Alpha",
  "Beta.md": "# Beta\n\n- [ ] something",
};

async function bootWithIndex(page, vault = VAULT) {
  await mockGitHub(page, vault);
  await seedConfig(page);
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  // The footer reports the index, so it doubles as the signal that the refresh
  // has finished writing.
  const count = Object.keys(vault).length;
  await expect(page.locator(".storage-footer")).toContainText(`${count} notes indexed`);
}

test("the index is stored one record per note, not as one blob", async ({ page }) => {
  await bootWithIndex(page);

  const pages = await readPages(page);
  expect(Object.keys(pages).sort()).toEqual(["Alpha.md", "Beta.md"]);
  // Each record is that note alone, with the text search needs.
  expect(pages["Alpha.md"].tags).toEqual(["a"]);
  expect(pages["Beta.md"].text).toContain("- [ ] something");
  expect(pages["Beta.md"]).not.toHaveProperty("pages");

  // Neither of the shapes it used to live in is left holding space.
  const blobs = Object.fromEntries((await readStore(page, "blobs")) || []);
  expect(blobs).not.toHaveProperty([INDEX_KEY]);
  const ls = await page.evaluate((k) => window.localStorage.getItem(k), INDEX_KEY);
  expect(ls, "index must not occupy the localStorage budget").toBeNull();
});

test("a reload reads the stored index instead of the vault", async ({ page }) => {
  // The point of persisting at all. This also guards the JS bridge: a
  // promise-returning helper that isn't declared `async` hands the Promise back
  // instead of its value, so every read silently returns empty and the index is
  // rebuilt from the network on every boot — invisible except right here.
  let reads = 0;
  await mockGitHub(page, VAULT);
  await page.route(/api\.github\.com\/repos\/[^/]+\/[^/]+\/contents\/(.+)/, (route, request) => {
    if (request.method() === "GET") reads++;
    route.fallback();
  });
  await seedConfig(page);

  await page.goto("/");
  await expect(page.locator(".storage-footer")).toContainText("2 notes indexed");
  expect(reads, "first boot reads the vault to build the index").toBeGreaterThan(0);

  const afterFirstBoot = reads;
  await page.reload();
  await expect(page.locator(".storage-footer")).toContainText("2 notes indexed");
  // Give any stray refresh a chance to happen before declaring victory.
  await page.waitForTimeout(500);
  expect(reads, "nothing changed, so nothing should be re-read").toBe(afterFirstBoot);
});

test("saving a note rewrites only that note's record", async ({ page }) => {
  // The point of the per-note split: a save must not scale with the vault.
  await bootWithIndex(page);
  const before = await readPages(page);

  await openFile(page, "Beta");
  await expect(page.locator(".md-area")).toContainText("Beta");
  await page.locator(".md-area").getByText("Beta").first().click();
  await page.keyboard.press("End");
  await page.keyboard.type(" edited");

  // Wait for the save to reach the index.
  await expect
    .poll(async () => (await readPages(page))["Beta.md"].text)
    .toContain("edited");

  const after = await readPages(page);
  // The untouched note's record is byte-identical — it was never rewritten.
  expect(after["Alpha.md"]).toEqual(before["Alpha.md"]);
});

test("an index left as a single blob by an older build is adopted, not rebuilt", async ({
  page,
}) => {
  await mockGitHub(page, VAULT);
  await seedConfig(page);

  // Count note reads so the assertion is the one that matters: adopting the
  // blob must cost *zero* reads, where starting cold re-downloads the vault.
  let reads = 0;
  await page.route(/api\.github\.com\/repos\/[^/]+\/[^/]+\/contents\/(.+)/, (route, request) => {
    if (request.method() === "GET") reads++;
    route.fallback();
  });

  await page.goto("/");
  await expect(page.locator(".storage-footer")).toContainText("2 notes indexed");

  // Rewind the store to the previous build's shape: the same index, as one
  // blob, with no per-note records. Built from what the app itself wrote, so
  // the payload is faithful rather than hand-rolled.
  await page.evaluate(
    (key) =>
      new Promise((resolve) => {
        const req = indexedDB.open("oxidian", 2);
        req.onsuccess = () => {
          const db = req.result;
          const read = db.transaction("pages", "readonly");
          const st = read.objectStore("pages");
          const keys = st.getAllKeys();
          const vals = st.getAll();
          read.oncomplete = () => {
            const pages = {};
            keys.result.forEach((k, i) => (pages[k] = JSON.parse(vals.result[i])));
            const t = db.transaction(["pages", "blobs"], "readwrite");
            t.objectStore("pages").clear();
            t.objectStore("blobs").put(JSON.stringify({ version: 2, pages }), key);
            t.oncomplete = () => { db.close(); resolve(); };
          };
        };
        req.onerror = () => resolve();
      }),
    INDEX_KEY
  );

  const readsBefore = reads;
  await page.reload();
  await expect(page.locator(".storage-footer")).toContainText("2 notes indexed");

  // Split back into per-note records...
  await expect
    .poll(async () => Object.keys(await readPages(page)).sort())
    .toEqual(["Alpha.md", "Beta.md"]);
  // ...with the blob gone rather than duplicated...
  await expect
    .poll(async () => Object.fromEntries((await readStore(page, "blobs")) || []))
    .not.toHaveProperty([INDEX_KEY]);
  // ...and no note was re-read to do it.
  expect(reads).toBe(readsBefore);
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
  await expect.poll(async () => Object.keys(await readPages(page))).toContain("Beta.md");
});
