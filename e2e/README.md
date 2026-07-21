# Oxidian E2E tests (Playwright)

End-to-end tests that drive the **real web build** in a headless Chromium.
Oxidian is a client-only Dioxus app, so there is no backend to stand up: tests
seed config into `localStorage` and mock the Git-host REST API at the network
layer, so nothing here touches real GitHub/GitLab or needs OAuth.

## Prerequisites

- The Dioxus CLI (`dx`) on `PATH` — `curl -sSL http://dioxus.dev/install.sh | sh`
- Node 18+
- One-time setup:
  ```sh
  cd e2e
  npm install
  npx playwright install chromium
  ```

## Running

```sh
cd e2e
npm test              # headless
npm run test:headed   # watch it in a real browser
npm run report        # open the last HTML report (after a CI run)
```

Playwright starts the app itself via the `webServer` block in
`playwright.config.js`:

```
dx run --package web --addr 127.0.0.1 --port 8080 --open false --hot-reload false
```

`dx run` (not `dx serve`) builds once and serves **without** file-watching or
hot-patching, so no "app is being rebuilt" overlay can appear mid-test. The first
run compiles the wasm bundle and can take a few minutes; later runs reuse the
warm `target/` dir. Override the port with `OXIDIAN_E2E_PORT`.

## How the harness works (`tests/helpers.js`)

- **`seedConfig(page, overrides?)`** — `addInitScript` writes a fake
  `GithubConfig` to `localStorage` under `oxidian_cfg` (the key from
  `packages/app/src/state.rs`) *before* the app boots, so `state::load_config()`
  finds it and renders `VaultBrowser` instead of `Settings`.
- **`mockGitHub(page, files)`** — routes `api.github.com` so `list_files`
  (git-trees) and `read_file` (contents) return canned data built from a
  `{ path: markdown }` map. A catch-all keeps stray GitHub calls off the network.
  > Playwright evaluates `page.route` handlers in **reverse** registration
  > order, so the broad catch-all is registered first and the specific routes
  > last — otherwise the catch-all would swallow everything.

## Current specs

| Spec | Covers |
| --- | --- |
| `onboarding.spec.js` | First-run (no config) shows the Settings screen with provider choices and the connection fields |
| `vault-browser.spec.js` | Seeded config boots into the vault browser, lists mocked notes, filters out `.gitkeep`, expands a folder to reveal a nested note, and shows the empty-vault status |

## Adding a spec

1. `seedConfig(page)` (+ `mockGitHub` if the flow reads/writes the repo).
2. `await page.goto("/")`.
3. Prefer role/text locators over CSS. If a target is hard to select, add a
   stable `data-testid`/class in the RSX rather than writing a brittle selector.

See `docs/testing.md` for where E2E fits in the overall test strategy.
