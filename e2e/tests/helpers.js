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

/** base64-encode a UTF-8 string the way the GitHub Contents API returns it. */
function b64(text) {
  return Buffer.from(text, "utf-8").toString("base64");
}

/**
 * Mock the GitHub REST API used by the vault backend.
 *   files:    { [path]: markdownString }  — the note vault contents
 * The git-trees endpoint is synthesized from the file paths; the contents
 * endpoint serves each file's base64 body. Any other api.github.com call gets a
 * benign empty 200 so a stray request can't hang the test.
 */
async function mockGitHub(page, files = {}) {
  const paths = Object.keys(files);

  // NOTE: Playwright evaluates route handlers in REVERSE registration order
  // (most-recently-added first). Register the broad catch-all FIRST so the
  // specific tree/contents routes added afterwards take precedence.

  // Catch-all: keep any other GitHub call from escaping to the network.
  await page.route(/api\.github\.com\//, (route) => {
    route.fulfill({ json: {} });
  });

  // File contents (read_file): GET /repos/:owner/:repo/contents/:path
  await page.route(/api\.github\.com\/repos\/[^/]+\/[^/]+\/contents\/(.+)/, (route, request) => {
    const url = new URL(request.url());
    const match = url.pathname.match(/\/contents\/(.+)$/);
    const path = match ? decodeURIComponent(match[1]) : "";
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
  await page.route(/api\.github\.com\/repos\/[^/]+\/[^/]+\/git\/trees\//, (route) => {
    route.fulfill({
      json: {
        sha: "tree-sha",
        tree: paths.map((p) => ({
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

module.exports = { seedConfig, mockGitHub, fakeConfig, b64, CONFIG_KEY };
