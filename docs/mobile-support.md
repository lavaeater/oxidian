# Mobile Support — Android

## Goal

A native Android app (APK) that gives the full Oxidian note-editing experience:
connect to a GitHub/GitLab repo, browse and edit markdown, auto-save, search,
bookmarks, WikiLink graph — all from a phone.

## Architecture decision: shared `app` crate

The web package contains all the real application logic (`state.rs`,
`wikilink_index.rs`, `export.rs`, `views/`). Both platforms need identical
logic, only the layout differs.

**Solution**: extract a `packages/app` library crate that holds everything
except the platform entry-point. Web and mobile are thin wrappers that supply
platform-specific CSS and call `dioxus::launch`.

```
packages/
  app/           ← shared: state, views, wikilink_index, export
  web/           ← entry point + desktop CSS
  mobile/        ← entry point + mobile CSS
  ui/            ← low-level Dioxus components (MarkdownArea, etc.)
  vault/         ← GitHub/GitLab API
```

## Mobile UX model

Desktop and mobile share the same component tree (`VaultBrowser`), driven by
responsive CSS:

| Screen ≥ 768 px (desktop/tablet) | Screen < 768 px (phone) |
|---|---|
| Fixed sidebar (260 px) + editor side-by-side | Sidebar hidden; bottom tab bar |
| Panel tabs in sidebar header | Tab bar: Files / Search / Graph / Bookmarks |
| Click file → opens in adjacent editor | Click file → slides editor into view (sidebar slides out) |
| Formatting toolbar always visible | Toolbar collapses into a single scrollable row |

A single CSS variable `--mobile: 1` injected via a `<meta>` tag absence drives
this; plain `@media (max-width: 768px)` handles the rest.

## Milestones

### M-Mob-1 — Shared `app` crate + mobile wired up
- Create `packages/app` (lib crate): move `state`, `wikilink_index`, `export`,
  `views/` from `packages/web`.
- Update `packages/web` to depend on `app`; remove the now-moved modules.
- Wire `packages/mobile` to the same `VaultBrowser` / `Settings` components.
- Add `vault` + `app` deps to mobile's `Cargo.toml`.
- Verify both `cargo check -p web` and `cargo check -p mobile` pass.

### M-Mob-2 — Mobile layout CSS
- `packages/mobile/assets/main.css`: dark theme matching web, plus:
  - Bottom tab bar (60 px, fixed, touch-sized 44 px icons)
  - Full-screen panels: sidebar expands to 100 % width, slides in/out
  - Editor pane takes 100 % width; back-button in titlebar navigates to file list
  - Touch-sized tap targets (min 44 px height on all interactive elements)
  - Larger base font (16 px body) and comfortable line-height
  - No `:hover` dependencies (replaced by `:active` feedback)
  - Soft-keyboard safe area: padding-bottom matches `env(keyboard-inset-height,0)`
- Inject `<meta name="viewport" content="width=device-width, initial-scale=1">`
  in mobile `main.rs`.

### M-Mob-3 — Android build configuration
- `packages/mobile/Dioxus.toml`: Android-specific settings (app name, bundle ID,
  min SDK 24, icon, permissions: INTERNET).
- `packages/mobile/assets/icons/`: placeholder launcher icons (192×192 px).
- `README` section: `dx build --platform android` instructions, how to install
  the APK with `adb install`.
- Gradle / NDK prerequisites listed.

### M-Mob-4 — Touch & UX polish (stretch)
- Swipe-right on the editor to return to the file list.
- Pull-to-refresh on the file list.
- Long-press a file entry for a context menu (rename, delete).
- Haptic feedback on save (via `navigator.vibrate(50)`).

## Build commands

```sh
# Check compilation
cargo check -p mobile

# Run in a simulator / device (requires Android NDK + dx CLI ≥ 0.7)
dx serve --package mobile --platform android

# Build APK
dx build --package mobile --platform android --release
```

## Known constraints

- `document::eval` and `localStorage` work identically in the Dioxus mobile
  WebView — no platform-specific storage code needed.
- The `vault` crate uses `reqwest` with `wasm32-unknown-unknown` targets on web
  and native TLS on mobile/desktop — the existing Cargo feature split handles this.
- Slash command polling uses `setTimeout` via `document::eval`; this works fine
  in the WebView.
