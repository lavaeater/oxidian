// @ts-check
// A [[wikilink]] whose note doesn't exist renders as missing and offers to
// create it. This is the vertical slice the Rust tests can't reach on their own:
// app::links resolution -> MarkdownArea's link rendering -> the `newnote:`
// action round trip -> create_file -> the link resolving on the next paint.

const { test, expect } = require("@playwright/test");
const { openFile, seedConfig, mockGitHub } = require("./helpers");

const VAULT = {
  "notes/Home.md": "# Home\n\nSee [[Existing]] and [[Missing]] and [[/rooted/Elsewhere]].\n",
  "notes/Existing.md": "# Existing\n",
};

async function openHome(page, files = VAULT) {
  await mockGitHub(page, { ...files });
  await seedConfig(page);
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  await expandNotes(page);
  await openFile(page, "Home");
  await expect(page.locator(".md-area")).toBeVisible();
}

/** Folders start collapsed; the notes all live in `notes/`. */
async function expandNotes(page) {
  const dir = page.locator(".file-tree-dir-label", { hasText: "notes" }).first();
  await dir.click();
  await expect(page.locator(".file-entry-name").first()).toBeVisible();
}

/** The wikilink span whose text contains `target`. */
function link(page, target) {
  return page.locator(".md-wikilink", { hasText: target }).first();
}

test("a link to a note that exists is linked; one that doesn't is marked missing", async ({
  page,
}) => {
  await openHome(page);

  await expect(link(page, "Existing")).toHaveClass(/md-wikilink--linked/);
  await expect(link(page, "Existing")).toHaveAttribute("data-navigate", "Existing");

  await expect(link(page, "Missing")).toHaveClass(/md-wikilink--missing/);
  // A missing link is not navigable — there is nothing to navigate to.
  await expect(link(page, "Missing")).not.toHaveAttribute("data-navigate", /.*/);
});

test("hovering a missing link reveals a Create note button", async ({ page }) => {
  await openHome(page);

  const missing = link(page, "Missing");
  const create = missing.locator(".md-wikilink-create");

  // Hidden until hover on a pointer device, so prose stays clean.
  await expect(create).toBeHidden();
  await missing.hover();
  await expect(create).toBeVisible();
  await expect(create).toHaveText("Create note");

  // A link that resolves never offers it.
  await link(page, "Existing").hover();
  await expect(link(page, "Existing").locator(".md-wikilink-create")).toHaveCount(0);
});

test("Create note writes the note beside the current one and opens it", async ({ page }) => {
  const writes = [];
  page.on("request", (r) => {
    if (r.method() === "PUT" && r.url().includes("/contents/")) {
      writes.push(decodeURIComponent(r.url().split("/contents/")[1].split("?")[0]));
    }
  });

  await openHome(page);

  const missing = link(page, "Missing");
  await missing.hover();
  await missing.locator(".md-wikilink-create").click();

  // A bare [[Missing]] written in notes/Home.md lands in notes/.
  await expect.poll(() => writes).toContain("notes/Missing.md");

  // ...and the new note opens, seeded with its own title.
  await expect(page.locator(".md-area")).toContainText("# Missing");
});

test("a leading slash creates the note from the vault root", async ({ page }) => {
  const writes = [];
  page.on("request", (r) => {
    if (r.method() === "PUT" && r.url().includes("/contents/")) {
      writes.push(decodeURIComponent(r.url().split("/contents/")[1].split("?")[0]));
    }
  });

  await openHome(page);

  const rooted = link(page, "rooted/Elsewhere");
  await rooted.hover();
  await rooted.locator(".md-wikilink-create").click();

  // Rooted, so NOT notes/rooted/Elsewhere.md.
  await expect.poll(() => writes).toContain("rooted/Elsewhere.md");
});

test("the link stops advertising itself as missing once the note exists", async ({ page }) => {
  await openHome(page);

  const missing = link(page, "Missing");
  await missing.hover();
  await missing.locator(".md-wikilink-create").click();
  await expect(page.locator(".md-area")).toContainText("# Missing");

  // Back in the note that holds the link: it now resolves.
  await openFile(page, "Home");
  await expect(page.locator(".md-area")).toContainText("See");
  await expect(link(page, "Missing")).toHaveClass(/md-wikilink--linked/);
});

test("clicking a resolved link opens that note", async ({ page }) => {
  await openHome(page);

  await link(page, "Existing").click();
  await expect(page.locator(".md-area")).toContainText("# Existing");
});
