// @ts-check
// Deleting a note: the file-entry delete button asks for confirmation
// (js::confirm_dialog -> window.confirm) and only removes the note on accept.
// Confirming issues a DELETE and drops the note from the list; dismissing is a
// no-op — a destructive action must never fire without confirmation.

const { test, expect } = require("@playwright/test");
const { seedConfig, mockGitHub } = require("./helpers");

async function bootVault(page) {
  await mockGitHub(page, { "Alpha.md": "# Alpha\n", "Beta.md": "# Beta\n" });
  await seedConfig(page);
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  await expect(page.locator(".file-entry-name")).toHaveText(["Alpha.md", "Beta.md"]);
}

/** Click the delete button on the file entry whose name contains `label`. */
async function clickDelete(page, label) {
  const entry = page.locator(".file-entry", { hasText: label }).first();
  await entry.hover();
  await entry.locator(".file-entry-delete").click({ force: true });
}

test("confirming the dialog deletes the note and removes it from the list", async ({ page }) => {
  await bootVault(page);

  let dialogMessage = "";
  page.once("dialog", (dialog) => {
    dialogMessage = dialog.message();
    dialog.accept();
  });

  const deleteRequests = [];
  page.on("request", (r) => {
    if (r.method() === "DELETE" && r.url().includes("/contents/")) deleteRequests.push(r.url());
  });

  await clickDelete(page, "Alpha");

  // The note is gone; its neighbour remains.
  await expect(page.locator(".file-entry-name")).toHaveText(["Beta.md"]);
  expect(dialogMessage).toContain("Delete 'Alpha.md'?");
  expect(deleteRequests.length).toBe(1);
});

test("dismissing the confirmation leaves the note untouched", async ({ page }) => {
  await bootVault(page);

  page.once("dialog", (dialog) => dialog.dismiss());

  let deleteFired = false;
  page.on("request", (r) => {
    if (r.method() === "DELETE" && r.url().includes("/contents/")) deleteFired = true;
  });

  await clickDelete(page, "Alpha");

  // Nothing removed, no DELETE issued.
  await expect(page.locator(".file-entry-name")).toHaveText(["Alpha.md", "Beta.md"]);
  expect(deleteFired).toBe(false);
});
