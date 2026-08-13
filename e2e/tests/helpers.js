// @ts-check
// Shared E2E helpers: seed client-side config and mock the Git-host REST API so
// tests never touch real GitHub/GitLab or require OAuth.
//
// Storage keys mirror `packages/app/src/state.rs`:
//   oxidian_cfg       -> serialized vault::GithubConfig (JSON)
//   oxidian_bookmarks -> string[]

const CONFIG_KEY = "oxidian_cfg";

/** A minimal valid GithubConfig that boots straight into the VaultBrowser. */
function fakeConfig(overrides = {}) {
  return {
    token: "fake-token",
    owner: "octocat",
    repo: "notes",
    branch: "main",
    provider: "GitHub",
    templates_dir: ".oxidian/templates",
    daily_note_template: ".oxidian/templates/daily-note.md",
    ...overrides,
  };
}

/**
 * Seed localStorage BEFORE the app boots so `state::load_config()` finds a
 * config and renders VaultBrowser instead of Settings. Runs on every navigation.
 */
async function seedConfig(page, overrides = {}) {
  const cfg = JSON.stringify(fakeConfig(overrides));
  await page.addInitScript(
    ([key, value]) => window.localStorage.setItem(key, value),
    [CONFIG_KEY, cfg]
  );
}

/**
 * Seed the Kanban board root (localStorage `oxidian_board`, see
 * packages/app/src/views/vault.rs) so the Kanban panel opens a board. A bare
 * folder name like "kanban" resolves to the document "kanban/kanban.md".
 */
async function seedBoard(page, root = "kanban") {
  await page.addInitScript(
    (value) => window.localStorage.setItem("oxidian_board", value),
    root
  );
}

/**
 * Click a note in the sidebar file tree.
 *
 * Scoped to `.file-entry-name` on purpose: a bare `getByText("Note")` matches
 * anything on the page, so unrelated sidebar copy (the storage footer's
 * "N notes indexed", say) can win the race and the click goes nowhere.
 */
async function openFile(page, name) {
  await page.locator(".file-entry-name", { hasText: name }).first().click();
}

/** base64-encode a UTF-8 string the way the GitHub Contents API returns it. */
function b64(text) {
  return Buffer.from(text, "utf-8").toString("base64");
}

/**
 * Mock the GitHub REST API used by the vault backend.
 *   files:    { [path]: markdownString }  — the note vault contents
 *   options:
 *     conflictOnWrite: boolean — make every PUT (save/create) return HTTP 409,
 *       exercising the SHA-conflict guard (must surface as a conflict, never a
 *       silent overwrite).
 * The git-trees endpoint is synthesized from the CURRENT file paths (so a
 * create/move/delete is reflected on the next list); the contents endpoint
 * serves each file's base64 body on GET, records writes on PUT, and removes on
 * DELETE. Any other api.github.com call gets a benign empty 200 so a stray
 * request can't hang the test.
 */
async function mockGitHub(page, files = {}, options = {}) {
  // NOTE: Playwright evaluates route handlers in REVERSE registration order
  // (most-recently-added first). Register the broad catch-all FIRST so the
  // specific tree/contents routes added afterwards take precedence.

  // Catch-all: keep any other GitHub call from escaping to the network.
  await page.route(/api\.github\.com\//, (route) => {
    route.fulfill({ json: {} });
  });

  // File contents: GET = read_file, PUT = write/create, DELETE = delete.
  await page.route(/api\.github\.com\/repos\/[^/]+\/[^/]+\/contents\/(.+)/, (route, request) => {
    const method = request.method();
    const url = new URL(request.url());
    const match = url.pathname.match(/\/contents\/(.+)$/);
    const path = match ? decodeURIComponent(match[1]) : "";

    if (method === "PUT") {
      if (options.conflictOnWrite) {
        route.fulfill({ status: 409, json: { message: "Conflict" } });
        return;
      }
      // Stateful fake: record the written body so a later GET reflects it
      // (enables create-then-open and edit-then-reload flows). The write/create
      // body carries base64 `content`.
      try {
        const payload = request.postDataJSON();
        if (payload && typeof payload.content === "string") {
          files[path] = Buffer.from(payload.content, "base64").toString("utf-8");
        }
      } catch {
        /* body wasn't JSON — ignore */
      }
      // GitHub returns the new blob sha under `content.sha`.
      route.fulfill({ json: { content: { sha: `sha-${path}-v2` } } });
      return;
    }
    if (method === "DELETE") {
      delete files[path];
      route.fulfill({ json: {} });
      return;
    }
    // GET
    const body = files[path];
    if (body === undefined) {
      route.fulfill({ status: 404, json: { message: "Not Found" } });
      return;
    }
    route.fulfill({
      json: { path, sha: `sha-${path}`, content: b64(body), encoding: "base64" },
    });
  });

  // Git tree (list_files): GET /repos/:owner/:repo/git/trees/:branch?recursive=1
  // Recomputed from the current `files` map so create/move/delete are reflected.
  await page.route(/api\.github\.com\/repos\/[^/]+\/[^/]+\/git\/trees\//, (route) => {
    route.fulfill({
      json: {
        sha: "tree-sha",
        tree: Object.keys(files).map((p) => ({
          path: p,
          type: "blob",
          sha: `sha-${p}`,
          size: files[p].length,
        })),
        truncated: false,
      },
    });
  });
}

module.exports = {
  openFile, seedConfig, seedBoard, mockGitHub, fakeConfig, b64, CONFIG_KEY };
