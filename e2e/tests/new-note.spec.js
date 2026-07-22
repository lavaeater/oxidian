// @ts-check
// Creating a note: the "New note" dialog creates the file via the Git host and
// opens it in the editor. Uses the stateful mock so the create (PUT) is followed
// by a successful read (GET) of the same path.

const { test, expect } = require("@playwright/test");
const { seedConfig, mockGitHub } = require("./helpers");

test("creating a note opens it in the editor with the seeded heading", async ({ page }) => {
  await mockGitHub(page, { "Existing.md": "# Existing\n\nhi" });
  await seedConfig(page);

  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();

  // Open the New note dialog and create "Fresh".
  await page.locator('[title="New note"]').first().click();
  const dialog = page.getByText("New note", { exact: true });
  await expect(dialog).toBeVisible();
  await page.getByPlaceholder(/note-name/).fill("Fresh");
  await page.getByRole("button", { name: "Create" }).click();

  // The new note opens in the editor. New notes are seeded with a `# <title>`
  // heading (see NewFileModal), and the stateful mock serves that back on read.
  await expect(page.locator(".editor-filename")).toHaveText("Fresh.md");
  await expect(page.locator(".md-area .md-heading")).toContainText("Fresh");
});
