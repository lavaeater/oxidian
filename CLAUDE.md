# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Product

Oxidian is an Obsidian-style "second brain" web app backed by a GitHub repository:

- **Auth:** GitHub OAuth — the logged-in user's GitHub identity is used to read/write their chosen repo via the GitHub API.
- **Storage:** Markdown files in a GitHub repo act as the note vault (version-controlled, no proprietary format).
- **Editor:** Hybrid WYSIWYG markdown — notes render as formatted markdown; clicking/focusing a line reveals and edits the raw markdown for that line only (Obsidian-style inline editing, not a split-pane preview). The existing `MarkdownArea` component in `packages/ui/src/cm/` is the starting point for this.
- **Platform priority:** `packages/web` first, then desktop/mobile.

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

This is a Dioxus 0.7 fullstack workspace with one crate per platform plus shared crates:

- `packages/ui` — shared Dioxus components (Hero, Navbar, Echo, MarkdownArea) and a large set of dioxus-primitives UI components under `src/components/`. Used by all platform crates.
- `packages/api` — shared server functions (annotated with `#[post]`/`#[get]`). Compiled into the server binary; on the client, calls are turned into HTTP requests automatically.
- `packages/desktop` — desktop entry point. Defines the `Route` enum and a `DesktopNavbar` layout wrapper around the shared `Navbar`.
- `packages/mobile` — same structure as desktop but for mobile.
- `packages/web` — same structure, targets WASM + server-side rendering.

### Feature flags

Each platform crate uses Cargo features to split server vs. client builds:
- `desktop`/`web`/`mobile` feature enables the respective Dioxus renderer.
- `server` feature enables `dioxus/server` and propagates `ui/server` → `api/server`.

`dx serve` handles selecting the right features automatically.

### Key Dioxus 0.7 patterns

- No `cx`, `Scope`, or `use_state` — use `use_signal`, `use_memo`, `use_resource`.
- Never hold a `Signal::read()` / `write()` borrow across an `.await` point (enforced by `clippy.toml`).
- Use `use_server_future` (not `use_resource`) for async data that must hydrate correctly on SSR.
- Routes are defined as a `#[derive(Routable)]` enum; layouts use `#[layout(Component)]`.
- Assets referenced via `asset!("/assets/...")` macro (path relative to crate root).
- Platform-specific layouts (e.g. `DesktopNavbar`) wrap the shared `ui::Navbar` so they can refer to the platform's own `Route` enum.
