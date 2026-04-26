# Oxidian — Implementation Plan

Web-first. Each phase is independently shippable.

---

## Phase 1 — GitHub OAuth

**Goal:** user can log in with GitHub and the server holds an access token.

### Server side (`packages/api`)

1. Add dependencies: `axum`, `reqwest`, `tower-sessions` (or `axum-sessions`), `serde`, `dotenvy`.
2. Add a `github_oauth` module with three server functions / raw Axum handlers:
   - `GET /auth/login` — redirects to `https://github.com/login/oauth/authorize?client_id=...&scope=repo`
   - `GET /auth/callback?code=...` — exchanges the code for an access token via GitHub's token endpoint, stores the token in a server-side session, redirects to `/`.
   - `POST /auth/logout` — clears the session.
3. Store `GITHUB_CLIENT_ID` and `GITHUB_CLIENT_SECRET` in `.env` (never committed). Read with `dotenvy`.
4. Expose a `#[get("/api/me")]` server function that returns the authenticated user's GitHub login/avatar (calls `GET https://api.github.com/user` with the token from session), or `Unauthenticated`.

### Client side (`packages/web`, `packages/ui`)

5. Add an `AuthContext` (a `Signal<Option<GitHubUser>>`) provided at the app root via `use_context_provider`.
6. On app boot, call `/api/me` with `use_server_future` to populate `AuthContext`.
7. Add a `LoginButton` component: if unauthenticated, renders an `<a href="/auth/login">` button; if authenticated, shows avatar + logout button.
8. Gate all other routes: redirect to a `/login` splash page if `AuthContext` is `None`.

### Required env vars

```
GITHUB_CLIENT_ID=
GITHUB_CLIENT_SECRET=
SESSION_SECRET=   # random 64-byte hex, for signing session cookies
```

---

## Phase 2 — Repo browser

**Goal:** authenticated user can pick a repo and browse its Markdown file tree.

### API (`packages/api`)

1. `#[get("/api/repos")]` — calls `GET https://api.github.com/user/repos` and returns a list of `{name, full_name, default_branch}`.
2. `#[get("/api/tree/:owner/:repo")]` — calls `GET /repos/{owner}/{repo}/git/trees/HEAD?recursive=1`, filters to `.md` files, returns a nested tree structure.
3. `#[get("/api/file/:owner/:repo/*path")]` — fetches file content via `GET /repos/{owner}/{repo}/contents/{path}` (base64-decoded), returns raw markdown string + the file's `sha` (needed for writes).

### UI (`packages/ui`, `packages/web`)

4. A `/vault` route: left sidebar shows the file tree (`FileTree` component, collapsible folders using the existing `collapsible` primitive). Clicking a file opens it in the editor pane.
5. Store active `(owner, repo, path)` in a `VaultContext` signal.
6. A repo-picker modal shown the first time (or from settings): renders the repos list, saves selection to `localStorage` so it persists across sessions.

---

## Phase 3 — Hybrid markdown editor

**Goal:** notes render as formatted markdown; clicking a line switches it to raw editable text.

This is the core feature. The existing `packages/ui/src/cm/` CodeMirror integration is the foundation.

### Approach

Use CodeMirror 6 with its **`@codemirror/lang-markdown`** language and the **decoration API** to implement "source mode with render decorations":

- Each line is rendered as formatted HTML (bold, italic, headings, links) when the cursor is **not** on that line.
- When the cursor enters a line, decorations are removed for that line and the raw markdown characters become visible and editable.
- This is the same technique Obsidian uses and is supported natively by `@codemirror/lang-markdown` with `hideMarkup` / inline decorations.

### Steps

1. In `packages/ui/src/cm/`, wire up a CodeMirror 6 instance via `web-sys` / JS interop (`wasm-bindgen`). The editor state is managed in JS; Dioxus holds a `NodeRef` to the container `div`.
2. Enable the markdown language with the `Obsidian-style` decoration extension. A reference implementation exists in the CodeMirror community as `codemirror-obsidian` or can be assembled from `@codemirror/lang-markdown`'s `markdownHighlighting` + a custom `ViewPlugin` that hides syntax markers on non-cursor lines.
3. Expose a `MarkdownEditor` Dioxus component:
   ```
   MarkdownEditor {
       content: Signal<String>,   // two-way bound
       on_change: EventHandler<String>,
   }
   ```
4. Sync: on CodeMirror `update` transactions, call a JS→Rust callback (via `Closure`) that writes the new doc string into the Dioxus signal.
5. On initial mount / file switch, push the loaded file content into the CodeMirror state via a JS function called from a Dioxus `use_effect`.

### JS interop strategy

Keep all CodeMirror setup in a small `editor.js` file bundled as an asset. Expose two functions:
- `window.oxidian.createEditor(container, initialContent, onChange)` — mounts the editor.
- `window.oxidian.setContent(view, content)` — replaces content without losing undo history (use CM's `replaceAll` transaction).

Call these from Rust via `web_sys::window().unwrap().get("oxidian")...`.

---

## Phase 4 — Save / commit

**Goal:** edits are saved back to GitHub.

### API

1. `#[post("/api/file/:owner/:repo/*path")]` with body `{content: String, sha: String, message: String}` — calls `PUT /repos/{owner}/{repo}/contents/{path}` (GitHub's update-file endpoint). Requires the previous `sha`.
2. Auto-save: debounce writes client-side (e.g. 2 s after last keystroke), then call the save server function. Show a subtle "saved" / "saving" / "error" indicator.

### UI

3. Track `dirty: Signal<bool>` in the editor; show unsaved-changes warning on navigation.
4. Optionally: a "commit message" prompt for intentional saves vs. silent auto-saves.

---

## Phase 5 — Graph view (stretch)

Render the wikilink graph (`[[Note Title]]` links between files) using a force-directed layout. Candidates: `d3-force` via JS interop, or a pure-Rust layout algo rendering to `<canvas>` / SVG.

---

## Dependency additions summary

| Crate / package | Where | Purpose |
|---|---|---|
| `axum` | `api` | Raw HTTP handlers for OAuth redirects |
| `reqwest` | `api` | GitHub API calls |
| `tower-sessions` + adapter | `api` | Server-side session storage |
| `dotenvy` | `api` | `.env` loading |
| `serde` / `serde_json` | `api`, `ui` | JSON types |
| `base64` | `api` | Decode GitHub file content |
| `@codemirror/*` (JS) | `ui` assets | Editor engine |

---

## File layout targets (web)

```
packages/web/src/
  main.rs          # routes: /, /login, /vault, /vault/:owner/:repo/*path
  views/
    login.rs
    vault.rs       # layout: FileTree sidebar + editor pane

packages/ui/src/
  auth.rs          # AuthContext, LoginButton
  vault_context.rs # VaultContext signal
  file_tree.rs     # FileTree component
  cm/
    mod.rs
    markdown_editor.rs  # MarkdownEditor Dioxus component
    editor.js           # CodeMirror bootstrap

packages/api/src/
  lib.rs
  auth.rs          # OAuth handlers + /api/me
  github.rs        # repo/tree/file server functions
  save.rs          # file write server function
```
