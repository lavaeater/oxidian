// @ts-check
// With a seeded config and a mocked Git host, the app boots into the vault
// browser and lists the repo's markdown notes. This is the regression contract
// for "configured state + read path" without any real network or OAuth.

const { test, expect } = require("@playwright/test");
const { seedConfig, mockGitHub } = require("./helpers");

test("seeded config boots into the vault browser and lists notes", async ({ page }) => {
  await mockGitHub(page, {
    "Welcome.md": "# Welcome\n\nThis is your vault.",
    "projects/Ideas.md": "# Ideas\n\n- [[Welcome]]",
    "empty/.gitkeep": "",
  });
  await seedConfig(page);

  await page.goto("/");

  // The vault sidebar renders (title is "Oxidian"), not the Settings screen.
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  await expect(page.getByText("Connect your vault")).toHaveCount(0);

  // The root note and the folder from the mocked tree are listed; the
  // .gitkeep placeholder is filtered out of the note list by the vault backend.
  await expect(page.getByText("Welcome", { exact: false }).first()).toBeVisible();
  await expect(page.getByText("projects", { exact: false }).first()).toBeVisible();

  // Expanding the folder reveals the nested note (tree interaction).
  await page.getByText("projects", { exact: false }).first().click();
  await expect(page.getByText("Ideas", { exact: false }).first()).toBeVisible();
});

test("empty vault shows the no-files status", async ({ page }) => {
  await mockGitHub(page, {}); // repo with no notes
  await seedConfig(page);

  await page.goto("/");

  await expect(page.getByText("No markdown files found.")).toBeVisible();
});
