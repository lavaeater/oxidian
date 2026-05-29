# Kanban Board — Design

## Model (US 19.3: folder-based)

A **board** is a vault folder. Each direct subfolder is a **column**. Each `.md`
file inside a column folder is a **card**. Example:

```
Projects/
  Todo/
    redesign-homepage.md
    fix-login-bug.md
  Doing/
    write-tests.md
  Done/
    setup-ci.md
```

Opening `Projects/` as a board renders three columns: Todo, Doing, Done.

Cards are sorted alphabetically (prototype). Future: front-matter `order` field
or numbered prefixes (e.g. `01-task.md`).

## Accessing the board

A **Kanban tab** (🗂) is added to the sidebar panel tabs in `VaultBrowser`. When
selected, a small text input lets the user type the board root folder path (e.g.
`Projects`). The input is persisted in `localStorage` so it survives page
reloads.

The main editor pane is replaced by the full-width `KanbanBoard` component when
the Kanban panel is active and a board path is set.

## Card display

Each card shows the filename without the `.md` extension. Clicking a card opens
it in the normal editor (sets `active_path`).

## Drag and drop — cross-column move

HTML5 drag API (`draggable`, `ondragstart`, `ondragover`, `ondrop`).

- `dragstart` records `(src_column, filename)` in JS `dataTransfer`.
- `dragover` on a column header/body accepts the drop.
- `drop` triggers a Rust callback that:
  1. Reads the file content + SHA from the vault.
  2. Creates the file at `<board>/<dst_column>/<filename>` (same content).
  3. Deletes the original at `<board>/<src_column>/<filename>`.
  4. Refreshes the file list signal.

This approach works on both WASM and WebView since it only uses standard DOM
events. No extra JS shim needed.

## Out of scope (prototype)

- US 19.1: single-file Kanban (headings → columns, list items → cards).
- Card ordering within a column.
- Creating/renaming columns.
- Card creation directly from the board (use the normal New Note button and place
  the file in the correct column folder).
- Optimistic UI during move (card briefly disappears; reload shows it in the new
  column).

## Files changed

| File | Change |
|------|--------|
| `packages/app/src/views/kanban.rs` | New `KanbanBoard` component |
| `packages/app/src/views/mod.rs` | Expose `kanban` module |
| `packages/app/src/views/vault.rs` | Add `Panel::Kanban`, wire board pane |
| `packages/web/assets/main.css` | Kanban board CSS |
