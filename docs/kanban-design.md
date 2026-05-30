# Kanban Board — Design

## Model: board-as-document (folder-backed cards)

A **board is a markdown document** (e.g. `kanban/kanban.md`) that owns the board's
structure and ordering. The individual note files stay clean — their position is
tracked by the board doc, not by frontmatter in each note.

```markdown
---
kanban-plugin: board
---

## Todo

- [[redesign-homepage]]
- [[fix-login-bug]]

## Doing

- [[write-tests]]

## Done

- [[setup-ci]]
```

- Each `## heading` is a **column** (in document order).
- Each `- [[Title]]` item is a **card** (in document order).
- A card's note lives at `<board-dir>/<Column>/<Title>.md`, where `<board-dir>` is
  the folder containing the board doc.

So for the example above (board at `kanban/kanban.md`), the vault holds:

```
kanban/
  kanban.md            ← the board document
  Todo/
    redesign-homepage.md
    fix-login-bug.md
  Doing/
    write-tests.md
  Done/
    setup-ci.md
```

### Why a document, not just folders

Git does not track empty directories, so a pure "columns = subfolders" model can't
represent an empty column or column order. The board document is the source of
truth for **which columns exist** and **in what order**, and for **card ordering**
within each column. Folders still back the cards (US 19.3), giving the best of
both: clean notes, real version-controlled structure.

## Accessing the board

A **Kanban tab** (🗂) in the sidebar panel tabs takes a board name. The input is
resolved to a document path:

- `kanban`            → `kanban/kanban.md`
- `kanban/board.md`   → used as-is

The value is persisted in `localStorage`. The main editor pane is replaced by the
full-width `KanbanBoard` component (keyed on the board path, so switching boards
remounts cleanly).

### First open / bootstrapping

If the board document doesn't exist yet, it is **created automatically**. Any
existing subfolders of the board directory that already contain notes are imported
as columns (with their notes as cards), so pointing the board at an existing
folder structure "just works". Empty folders cannot be detected (git doesn't store
them) — use **+ New column** to add them.

## Operations

All mutating actions update the board document **and** the underlying files, then
commit:

| Action | File effect | Board-doc effect |
|--------|-------------|------------------|
| Drag card A→B | move `dir/A/Title.md` → `dir/B/Title.md` (read→create→delete) | remove from column A's list, append to column B's |
| Add card | create `dir/Column/Title.md` with `# Title` | append `- [[Title]]` under the column |
| New column | create `dir/Column/.gitkeep` | append `## Column` |

The board doc is re-serialised from the in-memory model (preamble/frontmatter
preserved verbatim) and written back with its tracked SHA.

## Drag and drop

Standard HTML5 drag API. On `dragstart` a card stores `"<column>\x1e<title>"` in
`window.__oxidianDragData`; the destination column's `ondrop` reads it back and
fires the move. Works on both WASM and the native WebView with no extra shim.

## Out of scope (for now)

- US 19.1: single-file Kanban (one document whose `## headings`/list items are the
  whole board, with no backing folders).
- Reordering cards *within* a column by dragging (order currently follows the doc;
  moving across columns appends to the end of the target).
- Renaming/deleting columns from the UI.

## Files

| File | Role |
|------|------|
| `packages/app/src/views/kanban.rs` | `KanbanBoard` + `KanbanColumn`; board parse/serialize |
| `packages/app/src/views/vault.rs` | `Panel::Kanban`, board-path input, board pane |
| `packages/web/assets/main.css` (+ desktop/mobile) | Kanban CSS |
