// @ts-check
// Editor flows: opening a note renders it as formatted markdown in the hybrid
// WYSIWYG editor, and editing round-trips through the model. These exercise the
// real markdown_area.js DOM glue that the native SSR component tests can't reach
// (no JS runtime under `cargo test`).
//
// NOTE: wikilink navigation from *inside the editor* is intentionally not
// covered here — the VaultBrowser mounts `MarkdownArea` without an `on_navigate`
// handler (packages/app/src/views/vault.rs), so clicking a `[[link]]` in the
// editor is currently a no-op. See docs/testing.md. When that is wired up, add a
// navigation spec here.

const { test, expect } = require("@playwright/test");
const { openFile, seedConfig, mockGitHub } = require("./helpers");

const FILES = {
  "Note.md": "# Title\n\nSome **bold** text and a [[Other]] link.",
  "Other.md": "# Other\n\nThe link target.",
};

/** Boot into the vault and open `Note.md` in the editor. */
async function openNote(page, files = FILES) {
  await mockGitHub(page, files);
  await seedConfig(page);
  await page.goto("/");
  await expect(page.getByText("Oxidian", { exact: true })).toBeVisible();
  await openFile(page, "Note");
  await expect(page.locator(".md-area")).toBeVisible();
  await expect(page.locator(".editor-filename")).toHaveText("Note.md");
}

test("opening a note renders formatted markdown, not raw source", async ({ page }) => {
  await openNote(page);

  const editor = page.locator(".md-area");
  // Block + inline tokens are rendered as formatted spans (see tokens_to_html).
  await expect(editor.locator(".md-heading.md-h1")).toContainText("Title");
  await expect(editor.locator(".md-bold")).toContainText("bold");
  // The wikilink is rendered with its navigate target (rendering, not nav).
  await expect(editor.locator('[data-navigate="Other"]')).toBeVisible();
});

test("typing into a note round-trips through the model and survives re-render", async ({ page }) => {
  await openNote(page, { "Note.md": "# Title\n\nSome text here." });

  const editor = page.locator(".md-area");
  // Place the caret at the end of the body line and type.
  await editor.getByText("Some text here.", { exact: false }).click();
  await page.keyboard.press("End");
  await page.keyboard.type(" ADDED");

  // While the editor is focused the typed text is visible and NOT wiped by a
  // re-render under the cursor (the focused-contenteditable invariant).
  await expect(editor).toContainText("ADDED");

  // Blur the editor: the content model re-renders and still contains the edit,
  // proving the DOM edit was captured into the model (read_state -> content).
  await page.getByText("Oxidian", { exact: true }).click();
  await expect(editor).toContainText("Some text here. ADDED");
});
