# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Product

Oxidian is an Obsidian-style "second brain" app backed by a Git host repository:

- **Auth:** GitHub OAuth device flow (`request_device_code` → `poll_device_token` in `packages/vault`), or a personal access token. GitLab is also supported. The resulting token + repo/branch config is stored client-side in `localStorage` and used to read/write the repo directly via the host's REST API — there is no Oxidian backend.
- **Storage:** Markdown files in a GitHub/GitLab repo act as the note vault (version-controlled, no proprietary format). Writes are SHA-checked for conflicts.
- **Editor:** Hybrid WYSIWYG markdown — notes render as formatted markdown; clicking/focusing a line reveals and edits the raw markdown for that line only (Obsidian-style inline editing, not a split-pane preview). The `MarkdownArea` component in `packages/ui/src/cm/` is the editor.
- **Platform priority:** **`packages/web` and `packages/mobile` (Android only) are the priority targets** — the author always has either a computer or an Android phone, and web also covers iPad. `packages/desktop` still builds and shares all the same code, but isn't a focus. All platforms are thin shells over the shared `app` crate, so they stay in sync automatically; because web and mobile share that code, **changes to shared UI/editor logic must be validated on both web and Android** (iOS is out of scope for now).

## Commands

Install the Dioxus CLI if needed:
```sh
curl -sSL http://dioxus.dev/install.sh | sh
```

Serve a platform (run from the repo root, specifying the package):
```sh
dx serve --package desktop
dx serve --package web
dx serve --package mobile
```

Build:
```sh
cargo build
```

Lint:
```sh
cargo clippy
```

Test:
```sh
cargo test
cargo test -p <package-name>        # single package
cargo test <test_name>              # single test
```

## Architecture

This is a Dioxus 0.8 (alpha) workspace. Almost all logic lives in two shared crates; the per-platform crates are thin entry points:

- `packages/app` — **the application.** Owns the real logic and UI: `state` (localStorage config/bookmarks), `views` (the `VaultBrowser` and `Settings` entry components, plus `graph`, `kanban`, `properties`, `slash`, `toolbar`), `export`, `icons`, `template`, `wikilink_index`, and `js` (typed bindings to `assets/oxidian.js` via `dioxus-use-js`). Also owns the shared `MAIN_CSS` asset (see below).
- `packages/vault` — Git host backends (`github`, `gitlab`) and shared types (`FileMeta`, `FileContent`, `VaultError`, `WikiLink`, `GithubConfig`). All network I/O lives here; it has no Dioxus dependency.
- `packages/ui` — lower-level shared components: `Hero`, `Navbar`, `MarkdownArea` (the inline markdown editor, in `src/cm/`), and a large set of dioxus-primitives UI components under `src/components/`.
- `packages/web`, `packages/desktop`, `packages/mobile` — thin shells. Each `main.rs` is essentially the same: `dioxus::launch(App)`, where `App` loads config from storage and renders `app::VaultBrowser` (configured) or `app::Settings` (first run). They link `app::MAIN_CSS` and own only genuinely platform-specific bits (their own `favicon`, the viewport meta tag, mobile/desktop bundle config in `Dioxus.toml`).

There is **no server / SSR / `api` crate** — the client talks to the Git host API directly, so there are no server functions.

### Shared assets

`main.css` is owned by `app` (`packages/app/assets/main.css`) and exported as `pub const MAIN_CSS: Asset` from `app/src/lib.rs`, so all three platforms link the exact same stylesheet and can't drift. `app` depends on `manganis` directly because the `asset!` macro expands to a bare `manganis::` path. Add new shared assets the same way (in `app/assets`, exported from `lib.rs`); keep only platform-specific assets (e.g. `favicon.ico`) in the platform crates.

### Feature flags

Each platform crate has a single renderer feature — `web` / `desktop` / `mobile` — that enables the matching `dioxus/<renderer>`. `dx serve --package <crate>` selects it automatically.

### Key Dioxus 0.8 patterns

- No `cx`, `Scope`, or `use_state` — use `use_signal`, `use_memo`, `use_resource`.
- Never hold a `Signal::read()` / `write()` borrow across an `.await` point (enforced by `clippy.toml`).
- Assets referenced via the `asset!("/assets/...")` macro (path relative to crate root). For anything shared across platforms, export it from `app` rather than duplicating the file.
- Browser glue (localStorage, clipboard, selection, drag data, etc.) lives in `packages/app/assets/oxidian.js` and is bound with typed wrappers in `app/src/js.rs` via `dioxus-use-js`'s `use_js!` — prefer adding to that file over scattering `document::eval` strings.
- Native (desktop/mobile) builds need `libxdo` at link time on Linux (`xdotool` package on Arch).
