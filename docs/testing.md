# Testing plan for Oxidian

This document is the plan for building a comprehensive, layered test suite for
Oxidian. It follows the approach in the official Dioxus 0.7/0.8 testing guide
(<https://dioxuslabs.com/learn/0.7/guides/testing/web>) and its three reference
examples in `packages/playwright-tests/{web,liveview,fullstack}` of the Dioxus
repo, adapted to Oxidian's architecture (a client-only Dioxus app talking
directly to a Git host — **no server / SSR / server functions**).

## Goals

1. **Test our own code** — the logic, components and hooks in `packages/app`,
   `packages/ui` and `packages/vault`, not the framework or the UI-primitive
   library.
2. **Regression safety** — when we add, remove or change a feature, the rest of
   the app must be provably still whole. This is the job of the top two layers
   (component + E2E) plus a golden/snapshot strategy.
3. **Runs on both priority targets** — logic and component tests run in native
   `cargo test`; E2E must be exercised on **web** and, where feasible, on
   **Android** (the two priority platforms), since they share the `app` crate.

## The testing pyramid for Oxidian

```
        ┌───────────────────────────────────────────┐
   E2E  │ Playwright vs `dx serve --package web`     │  few, slow, high-value
        │ real browser, real localStorage, real DOM  │  whole-flow regression
        ├───────────────────────────────────────────┤
 Comp.  │ dioxus-ssr render + pretty_assertions      │  render a component,
        │ + VirtualDom driving for interaction/hooks │  assert on the HTML
        ├───────────────────────────────────────────┤
 Unit   │ plain `#[test]` on pure functions          │  many, fast, exhaustive
        │ tasks / dates / tokenizer / wikilink / …   │  edge-case coverage
        └───────────────────────────────────────────┘
```

The guiding rule: **push logic down**. Every behaviour that can be expressed as
a pure function should be tested at the unit layer (cheap, exhaustive), so the
component and E2E layers only have to prove wiring, not business rules.

---

## Layer 1 — Unit tests (pure logic)

Plain `#[cfg(test)] mod tests` next to the code, run with `cargo test`. This is
the layer we already have and should saturate first — it's free of Dioxus and
runs on the native target instantly.

### Already covered (keep, extend)
- `packages/app/src/dates.rs` — weekday, ISO week, parse roundtrip.
- `packages/app/src/tasks.rs` — parse, toggle/stamp, locate.
- `packages/ui/src/cm/markdown_area/tokenizer.rs` — bold/italic/code/heading/
  wikilink/image tokenizing.
- `packages/app/src/tasks_cache.rs` — has tests.

### Gaps to fill (high priority — pure and currently untested)
| Module | Function(s) | What to assert |
| --- | --- | --- |
| `app/src/wikilink_index.rs` | `index_file`, `reindex_file`, `backlinks`, `outlinks`, `edges` | building an index from a set of notes yields the right forward/back links; reindex removes stale links; `edges` is symmetric-free/deduped; broken links resolve sanely |
| `app/src/template.rs` | `substitute_vars`, `strip_tabstops`, `parse_template`, `TemplateVars::from_json` | `{{title}}`/`{{date}}` substitution, tabstop stripping (`$0`, `${1:foo}`), frontmatter/meta parsing |
| `app/src/dates.rs` | `add_days`, `days_in_month`, `is_leap` | month/year rollover, leap years, Feb 28→29→Mar 1 |
| `app/src/export.rs` | export/serialization helpers | round-trip / format stability |
| `vault/src/lib.rs` | `FileMeta::name`/`dir`, config defaults, `WikiLink` parsing, `VaultError` mapping | path splitting edge cases (nested dirs, no extension), default branch/templates dir |
| `vault/src/{github,gitlab}.rs` | URL/path/base64 construction, header building, response→type mapping | **factor the pure request-building and response-parsing bits out of the `async fn`s** so they can be unit-tested without network (see note below) |

**Vault network functions:** the `list_files`/`read_file`/`write_file`/… fns are
`async` and hit the network, so they can't be pure-unit-tested as-is. Two options
(do both over time):
1. Extract the pure parts — building the request URL/body, encoding content,
   parsing the JSON response into `FileMeta`/`FileContent`, mapping HTTP status
   to `VaultError` — into free functions and unit-test those exhaustively.
2. Cover the actual HTTP round-trip with a mock server (`wiremock`) as an
   integration test (Layer 1.5 below).

### Conventions
- Unit tests live in-file under `#[cfg(test)] mod tests`.
- Prefer table-driven cases for parsers/tokenizers.
- No `dioxus` import at this layer.

---

## Layer 1.5 — Vault integration tests (mocked HTTP)

`packages/vault` has no Dioxus dependency and owns *all* network I/O, which makes
it the natural seam for testing the Git-host protocol without a real repo.

- Add `wiremock` (or `httpmock`) + `tokio` as `[dev-dependencies]` in
  `packages/vault/Cargo.toml`.
- Stand up a mock GitHub/GitLab REST endpoint and assert that:
  - `list_files` parses a directory listing into `FileMeta`s.
  - `read_file` base64-decodes content and captures the blob SHA.
  - `write_file` sends the **SHA-checked** update and surfaces a 409 as the
    right `VaultError` (conflict) — this is the core "writes are SHA-checked for
    conflicts" guarantee from the product spec.
  - `create_file` / `delete_file` / `move_file` / `move_dir` issue the correct
    verb+path sequence.
  - `read_many` aggregates and preserves order.
- These run under `cargo test -p vault` on the native target — no browser.

This layer is where we lock down the backend contract so a refactor of
`github.rs`/`gitlab.rs` can't silently change wire behaviour.

---

## Layer 2 — Component & hook tests (dioxus-ssr + VirtualDom)

Native `cargo test` that renders real Dioxus components to a string and asserts
on the output — the guide's "component testing" approach. Because Oxidian is
client-only, we render with `dioxus-ssr` purely as a *test harness* (there is no
SSR in production).

### Setup
Add to the crate(s) under test (`app`, `ui`) as `[dev-dependencies]`:
```toml
[dev-dependencies]
dioxus-ssr      = "0.8.0-alpha.0"   # match the dioxus alpha in use
pretty_assertions = "1"
futures         = "0.3"
```
Component/hook tests go in `packages/<crate>/tests/` (integration test dir) or an
in-crate `#[cfg(test)]` module.

### 2a. Rendering a component (snapshot of HTML)
```rust
use dioxus::prelude::*;
use pretty_assertions::assert_eq;

#[test]
fn badge_renders_label() {
    // Render a single component to static HTML.
    let html = dioxus_ssr::render_element(rsx! {
        Badge { "Draft" }
    });
    assert!(html.contains("Draft"));
    assert!(html.contains("class=\"")); // spot-check structural class
}
```
For richer components, drive a `VirtualDom` so effects/memos resolve:
```rust
fn render(app: fn() -> Element) -> String {
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}
```

### 2b. Testing a hook by manually driving the VirtualDom
This is the guide's "hook testing" pattern — there's no dedicated hook-test
library, so we mount a tiny probe component that calls the hook, then step the
`VirtualDom` and assert on what the hook produced. Primitives:
`VirtualDom::new_with_props`, `rebuild_in_place`, `render_immediate`,
`mark_dirty` / waiting on the scheduler.

```rust
use dioxus::prelude::*;
use std::sync::mpsc;

#[test]
fn use_global_shortcuts_fires_callback() {
    // A probe that installs the hook and reports what it saw.
    fn probe() -> Element {
        let tx = use_context::<mpsc::Sender<String>>();
        use_global_shortcuts(Callback::new(move |combo: String| {
            let _ = tx.send(combo);
        }));
        rsx! { div {} }
    }
    // ... provide the channel via context, build the vdom, simulate the
    // event source the hook listens to, then step the dom and assert the
    // callback delivered the expected shortcut string.
}
```
Hooks worth covering this way:
- `app/src/shortcuts.rs::use_global_shortcuts` — **caveat: this hook is
  JS-driven.** The chord-matching logic lives in `INSTALL_JS` (a `document::eval`
  string), not in Rust, and there is no JS runtime in the native harness, so the
  callback never fires under `cargo test`. There is no pure Rust logic to unit
  test here — this hook belongs to the **E2E layer** (press Ctrl+P in a real
  browser, assert the palette opens). The `VirtualDom`-driving pattern is still
  the right tool for any *future* hook whose logic lives in Rust.
- `ui/.../sidebar::use_sidebar` / `use_is_mobile` — context wiring and the
  mobile breakpoint signal.
- Any future `use_*` we add (the guide's whole point is that these get a test
  the moment they exist).

### 2c. Which components to test
Focus on **our** components (the ones with logic), not the vendored
dioxus-primitives wrappers in `ui/src/components/*` (those are upstream's to
test). Priorities:
- `ui/src/cm/markdown_area/component.rs` — the hybrid WYSIWYG editor. Assert:
  a rendered line shows formatted markdown; focusing a line reveals raw
  markdown; **guard the invariant from memory** that `dangerous_inner_html` is
  never updated while the contenteditable is focused (`is_focused` gating). This
  is a prime regression target.
- `app/src/views/toolbar.rs`, `slash.rs`, `properties.rs`, `kanban.rs`,
  `graph.rs` — render with representative props, assert the important
  structural output and that callbacks fire.
- `app/src/views/vault.rs` (`VaultBrowser`) and `settings.rs` — smoke-render in
  both states (configured vs first-run) with a fake/injected config.

> **Note on interaction depth:** dioxus-ssr renders one pass; it does not run a
> browser event loop. Use it for *render correctness* and hook-output checks.
> True click-through user flows belong in Layer 3.

---

## Layer 3 — End-to-end tests (Playwright)

Mirrors `packages/playwright-tests/web` from the Dioxus repo: a Playwright
project that boots the app with `dx serve` and drives a real Chromium browser.
This is the layer that proves *"change one feature, everything else stays
whole."*

### Structure
Create `packages/e2e/` (or top-level `e2e/`):
```
e2e/
  package.json           # @playwright/test devDependency
  playwright.config.js    # webServer: dx serve --package web, port 8080
  tests/
    onboarding.spec.js
    notes-crud.spec.js
    editor.spec.js
    wikilinks.spec.js
    tasks.spec.js
    ...
```
`playwright.config.js` (modeled on the Dioxus example):
```js
module.exports = defineConfig({
  testDir: "./tests",
  webServer: {
    command: "dx serve --package web --addr 127.0.0.1 --port 8080",
    port: 8080,
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
  use: { baseURL: "http://127.0.0.1:8080", trace: "retain-on-failure" },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
});
```
Locate elements by adding stable `class`/`id`/`data-testid` hooks in the RSX
(the Dioxus example locates by `button.increment-button`, `#main`, etc.).

### The auth problem (critical design decision)
Oxidian's real flow needs GitHub/GitLab OAuth + a live repo. E2E must **not**
depend on the real network. Options, pick per-suite:
1. **Seed localStorage before load** — inject a fake `GithubConfig` +
   token/bookmarks into `localStorage` in a Playwright `page.addInitScript`, so
   the app boots straight into `VaultBrowser` in the "configured" state.
2. **Intercept the Git host API** — `page.route('**/api.github.com/**', …)` to
   serve canned repo contents, so reads/writes hit a mock, deterministically.
   This is the E2E analogue of Layer 1.5 and lets us test real CRUD flows.
3. A dedicated throwaway test repo + a CI secret token — highest fidelity,
   slowest, use sparingly (maybe one "smoke against real GitHub" job).

Recommendation: build suites on **(1)+(2)** for everyday CI; keep **(3)** as an
optional nightly.

### Regression-critical flows to cover
- **Onboarding:** first-run shows `Settings`; completing config lands in
  `VaultBrowser`; config persists across reload (localStorage).
- **Note CRUD:** create → appears in browser; edit → save is SHA-checked; a
  simulated 409 surfaces a conflict message; delete removes it.
- **Editor (the crown jewel):** click a line → raw markdown revealed; edit →
  blur → re-renders formatted; multi-line, wikilinks, tasks; **the focused-
  contenteditable invariant** end-to-end.
- **Wikilinks & graph:** `[[Note]]` navigates; backlinks panel is correct; graph
  view reflects edges.
- **Tasks / kanban:** toggling a task checkbox writes the done-stamp back;
  kanban drag moves reflect in the markdown.
- **Slash menu, toolbar, templates, export.**

### Android
The web and mobile shells share the `app` crate, so a passing web E2E suite
covers the shared logic. For Android-specific validation:
- Primary: manual/scripted smoke on device/emulator for touch, the on-screen
  keyboard's effect on the contenteditable editor, and the reqwest/rustls launch
  path (see the Android rustls memory).
- Playwright can't drive a Dioxus native Android app; if we want automated
  device tests later, evaluate Appium/Espresso as a separate track. For now,
  **web E2E is the regression gate; Android gets a documented manual smoke
  checklist** (add `docs/android-smoke-checklist.md`).

---

## Cross-cutting: what "stays whole" means mechanically

To make regressions *fail a test* rather than get noticed in review:

1. **Snapshot the tokenizer & template output** — golden files under
   `tests/snapshots/` (consider `insta`) so any change to markdown parsing or
   template substitution shows a reviewable diff.
2. **A component render snapshot per view** — SSR each view with fixed props and
   snapshot the HTML; unintended structural changes then surface as diffs.
3. **The E2E suite is the integration contract** — every user-facing feature
   gets at least one spec; removing/altering a feature must consciously update
   its spec, which is the audit trail.
4. **Vault wire-contract tests (Layer 1.5)** — lock the API request/response
   shape so backend refactors can't silently break reads/writes.

---

## Tooling & CI

### Local commands
```sh
cargo test                     # unit + component + hook + vault-integration (native)
cargo test -p vault            # vault only
cargo test -p app              # app logic + components
cargo clippy                   # lint (already in the workflow)

cd e2e && npx playwright test  # E2E (auto-starts `dx serve --package web`)
```

### New dev-dependencies to add
- `packages/app`, `packages/ui`: `dioxus-ssr`, `pretty_assertions`, `futures`.
- `packages/vault`: `wiremock` (or `httpmock`), `tokio` (multi-thread, macros).
- `e2e/`: `@playwright/test`.

### CI (GitHub Actions) — suggested jobs
1. `cargo test --workspace` + `cargo clippy --workspace` (fast, always).
2. `playwright` job: install the `dx` CLI, `cargo build`, run `npx playwright
   test` with the localStorage-seed + route-mock strategy; upload the Playwright
   HTML report + traces on failure.
3. *(optional, nightly)* real-repo smoke using a CI token secret.

---

## Rollout order (recommended)

1. **Fill the pure-logic gaps** (Layer 1): `wikilink_index`, `template`,
   `dates::add_days`, `export`, `vault` path/parse helpers. Cheapest wins, and
   it forces us to extract pure helpers out of the async vault fns.
2. **Vault mocked-HTTP tests** (Layer 1.5) — lock the backend contract.
3. **Component render + hook harness** (Layer 2) — start with `MarkdownArea`
   and `use_global_shortcuts`; add the SSR snapshot helper.
4. **Playwright scaffold + onboarding/CRUD/editor specs** (Layer 3) with the
   localStorage-seed + route-mock strategy.
5. **Snapshots + CI wiring**, then broaden E2E coverage feature-by-feature.
6. **Android smoke checklist** documented and run before releases.

## Implementation status

- **Layer 1 (pure units) — done for all already-pure code.** Added: `app`
  `wikilink_index` (10), `template` (12), `dates::add_days` (5), `export` (7);
  `vault` `lib` helpers (6). Pre-existing: `dates`, `tasks`, `tasks_cache`,
  `tokenizer`. Still open: extract + test pure request/response helpers from
  `github.rs`/`gitlab.rs`.
- **Layer 2 (component/hook harness) — bootstrapped.** `dioxus-ssr` +
  `pretty_assertions` added as dev-deps to `app` and `ui`. The reusable harness
  is `let mut dom = VirtualDom::new(app); dom.rebuild_in_place();
  dioxus_ssr::render(&dom)` (see `app/tests/ssr_smoke.rs`). Added: `MarkdownArea`
  `tokens_to_html` rendering contract (10 in-file unit tests) and a
  component-level render test (`ui/tests/markdown_area_render.rs`, 3) proving the
  contenteditable surface shows formatted markdown on initial unfocused mount.
  A single `rebuild_in_place` pass renders initial state only — effects/JS don't
  run, so interaction/cursor behaviour stays in Layer 3.
- **Layer 1.5 (vault backend contract) — done via pure extraction, not
  wiremock.** The base URL is a hardcoded `const`, so redirecting requests to a
  per-test localhost mock would force a production change (and env-var injection
  races under parallel tests). Instead — the plan's *first* listed option — the
  pure request-building and response-parsing were extracted from the async fns
  into free functions and tested exhaustively (16 tests): URL builders, tree
  filtering (`.md`/`.gitkeep`, blob-id→sha), base64+CRLF decode, GitHub search
  result mapping, device-flow poll classification, and `status_error` — which
  locks the **409 → `Conflict`** SHA-guard for GitLab and the 401/404 mapping for
  both. A live-HTTP round-trip (wiremock, or a throwaway repo) is still worth
  adding later as a smoke test, but the interesting logic is now covered without
  a network.
- **Layer 3 (Playwright E2E) — scaffolded and green.** `e2e/` holds a working
  Playwright project (`npm test`, 4 tests passing). The `webServer` runs
  **`dx run --package web`** (not `dx serve` — `run` builds once without
  watching/hot-patching, so no rebuild overlay races the tests). `tests/helpers.js`
  implements both strategies from the design below: `seedConfig` writes a fake
  `GithubConfig` to `localStorage` (`oxidian_cfg`) via `addInitScript` so the app
  boots into `VaultBrowser`, and `mockGitHub` routes `api.github.com` to serve
  canned tree/contents data (catch-all registered first — Playwright evaluates
  routes in reverse order). Specs: `onboarding` (first-run Settings screen) and
  `vault-browser` (seeded boot, note listing, `.gitkeep` filtered, folder
  expansion, empty-vault status) and `editor` (opening a note renders formatted
  markdown; typing round-trips through the model and survives the re-render on
  blur — the focused-contenteditable invariant). 6 specs passing.

  > **Finding surfaced by the editor spec:** the `VaultBrowser` mounts
  > `MarkdownArea` **without** an `on_navigate` handler
  > (`packages/app/src/views/vault.rs`), so clicking a `[[wikilink]]` *inside the
  > editor* is currently a no-op — the link renders but doesn't navigate. The
  > planned navigation spec is therefore deferred until that's wired up; the test
  > was not written to pass against absent behaviour.

  Next: note CRUD with a simulated 409 conflict, tasks/kanban, wikilink
  navigation (once wired), and the `Ctrl+P`/`Ctrl+O` shortcut hook that can't be
  tested natively. CI wiring (install `dx`, `npx playwright test`) still to do.

## Non-goals
- Testing the vendored dioxus-primitives components in `ui/src/components/*`
  (upstream's responsibility) — we only test *our* usage of them via views.
- iOS (out of scope per the platform priority).
- Testing the Dioxus framework itself.
