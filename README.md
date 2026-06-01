# Oxidian

An Obsidian-style "second brain" web app backed by a Git repository. Your notes
are plain Markdown files living in a GitHub (or GitLab) repo — version-controlled,
no proprietary format, editable from anywhere.

Oxidian is built with [Dioxus 0.7](https://dioxuslabs.com/) as a fullstack
workspace and runs on **web** (WASM + SSR), **desktop**, and **mobile** from the
same shared codebase.

---

## Getting started

### 1. Install the Dioxus CLI

```sh
curl -sSL http://dioxus.dev/install.sh | sh
```

### 2. Run a platform

From the repo root:

```sh
dx serve --package web        # browser (WASM + server)
dx serve --package desktop    # native desktop window
dx serve --package mobile     # mobile
```

### 3. Connect your vault

On first launch you'll see the **Connect your vault** screen.

| Field | What it is |
|-------|-----------|
| **Provider** | GitHub or GitLab |
| **Token** | A Personal Access Token with `repo` scope (GitHub) or `api` scope (GitLab) |
| **Owner** | Your username or namespace (e.g. `octocat`) |
| **Repository** | The repo holding your notes (e.g. `my-notes`) |
| **Branch** | Defaults to `main` |
| **Templates folder** | Defaults to `.oxidian/templates` |
| **Daily note template** | Defaults to `.oxidian/templates/daily-note.md` |

- **GitHub on desktop/mobile** can use *Sign in with GitHub* (OAuth device flow) —
  click the link, the code is pre-filled, and your username is detected
  automatically.
- **In the browser**, GitHub's OAuth endpoints don't send CORS headers, so paste
  a [Personal Access Token](https://github.com/settings/tokens) instead.
- **GitLab** always uses a Personal Access Token.

Your configuration is stored in the browser's `localStorage`. Use the **⚙
Disconnect vault** button in the sidebar to clear it and log out.

---

## Features

### Note browsing

- **File tree** (📁 tab) — a collapsible folder tree of every Markdown file in
  the vault. Folders auto-expand to reveal the file you're viewing.
- **Current file & folder highlighting** — the open file is highlighted, and so is
  its containing folder (or the last folder you clicked).
- **Outline** — when a note is open, its headings appear below the tree for quick
  navigation.
- **Resizable sidebar** — drag the divider between the sidebar and the editor to
  set the tree width (160–600px); the width persists.
- **Quick Switcher** — fuzzy "go to file" jump list.

### Editing

Oxidian uses **hybrid WYSIWYG Markdown**: notes render as formatted Markdown, and
the line you click into reveals its raw Markdown for inline editing (Obsidian
style — not a split-pane preview).

- **Formatting toolbar** — Heading 1–3, Bold, Italic, Strikethrough, Inline code,
  Bullet list, Task item, Blockquote.
- **Task checkboxes** — click `- [ ]` / `- [x]` checkboxes directly in the
  rendered view to toggle them.
- **Smart list continuation** — pressing Enter inside a bullet, numbered, or task
  list continues the list with the correct prefix.
- **Auto-save** — edits save automatically after a short debounce (a 5-second
  countdown shown in the title bar), committing back to your repo. Switching files
  flushes pending changes first.
- **Word count** and a live **save status** indicator (Saving in Ns… / Saving… /
  Saved / Save failed).

### Properties (frontmatter)

YAML frontmatter (`---` block at the top of a note) is rendered as a collapsible
key → value editor. Editing a value writes it back into the note.

### Slash commands & templates

Type `/` in the editor to open the **slash menu**. Built-in snippets:

```
Heading 1/2/3 · Bold · Italic · Bullet · Numbered · Task · Quote
Code block · Table · Divider · WikiLink
```

Your own **templates** (Markdown files in the templates folder) also appear in the
slash menu. A template can declare frontmatter to control its behaviour:

```yaml
---
oxid_template:
  filepath: "journal/${OXID_DATE_YEAR}/${OXID_DATE_DATE}.md"
  description: "Daily note"
---
# ${OXID_DATE_DAY_NAME} ${OXID_DATE_DATE} ${OXID_DATE_MONTH_NAME} ${OXID_DATE_YEAR}
```

- A template **with** `filepath` creates (or opens) a file at that path.
- A template **without** `filepath` is inserted at the cursor.

Supported substitution variables (both `OXID_` and `FOAM_` prefixes work):

| Variable | Example |
|----------|---------|
| `${OXID_DATE_YEAR}` | `2026` |
| `${OXID_DATE_YEAR_SHORT}` | `26` |
| `${OXID_DATE_MONTH}` | `05` |
| `${OXID_DATE_MONTH_NAME}` | `May` |
| `${OXID_DATE_DATE}` | `30` |
| `${OXID_DATE_DAY_NAME}` | `Friday` |
| `${OXID_DATE_WEEK}` | `22` |
| `${OXID_TITLE}` / `${OXID_TITLE_SAFE}` | note title |
| `${OXID_CURRENT_DIR}` | folder of the active note |

### Daily notes

The **📅** button in the sidebar header creates/opens today's note using your
configured daily-note template (falling back to a simple `YYYY-MM-DD.md` note if
no template is set).

### Creating notes & folders

- **✏ New note** — create a note; supports `folder/note-name` paths.
- **📁+ New folder** — create a folder. The default location is the currently
  selected folder; the live preview shows where it will land. Rules:
  - plain name (`Todo`) → created inside the current folder
  - leading slash (`/Todo`) → created at the vault root
  - nested path (`a/b/c`) → all intermediate folders are created (`mkdir -p` style)

  Empty folders are kept with a `.gitkeep` placeholder (Git can't store empty
  directories); the placeholder is hidden from the tree.

### Wikilinks, backlinks & graph

- **`[[Wikilinks]]`** between notes are indexed as you open files.
- **Backlinks** panel (↩ tab) — every note that links to the current one.
- **Graph** panel (◉ tab) — a visual graph of linked notes; click a node to open
  it.

### Search

The **🔍 Search** panel runs a code search across the repo (GitHub/GitLab search
API) and lists matching notes with a content fragment.

### Bookmarks

Click the **🔖** in the editor title bar to bookmark the current note. Bookmarks
live in the 🔖 sidebar panel and persist in `localStorage`.

### Kanban board

The **🗂 Kanban** panel turns a folder into a visual board (folder-backed model):

- A **board** is a vault folder; each **subfolder** is a **column**; each `.md`
  file inside is a **card**.
- Enter the board's folder path (e.g. `Projects`) and press Enter; the path is
  remembered.
- **Drag a card** between columns and Oxidian moves the underlying file to the new
  column's folder and commits the change.
- **+ New column** creates a new subfolder (kept alive with a `.gitkeep`).
- Clicking a card opens it in the editor.

Example layout:

```
Projects/
  Todo/      → "Todo" column
    redesign-homepage.md
  Doing/     → "Doing" column
    write-tests.md
  Done/      → "Done" column
    setup-ci.md
```

See [`docs/kanban-design.md`](docs/kanban-design.md) for the full design.

### Export

The **↓** button in the editor title bar exports the current note as a standalone
HTML file.

---

## Development

This is a Dioxus 0.7 fullstack workspace with one crate per platform plus shared
crates.

```
oxidian/
├─ packages/
│  ├─ web/       # Web entry point (WASM + SSR)
│  ├─ desktop/   # Desktop entry point
│  ├─ mobile/    # Mobile entry point
│  ├─ app/       # Shared app logic: views, state, templates, wikilink index
│  ├─ ui/        # Shared Dioxus components (MarkdownArea, primitives, …)
│  ├─ api/       # Shared server functions (#[post]/#[get])
│  └─ vault/     # Git provider clients (GitHub/GitLab): list/read/write/search
└─ docs/         # Design docs, user stories, roadmap, templates
```

### Feature flags

Each platform crate splits server vs. client builds via Cargo features:

- `desktop` / `web` / `mobile` enable the respective Dioxus renderer.
- `server` enables `dioxus/server` and propagates `ui/server` → `api/server`.

`dx serve` selects the right features automatically.

### Common commands

```sh
cargo build                     # build the workspace
cargo clippy                    # lint
cargo test                      # run tests
cargo test -p <package-name>    # test a single package
```

### Architecture notes (Dioxus 0.7)

- No `cx`, `Scope`, or `use_state` — use `use_signal`, `use_memo`, `use_resource`.
- Never hold a `Signal::read()`/`write()` borrow across an `.await` (enforced via
  `clippy.toml`).
- Use `use_server_future` (not `use_resource`) for async data that must hydrate on
  SSR.
- Routes are a `#[derive(Routable)]` enum; layouts use `#[layout(Component)]`.
- Assets are referenced via the `asset!("/assets/...")` macro.

---

## Roadmap & docs

- [`docs/user-stories.md`](docs/user-stories.md) — feature backlog by epic
- [`docs/roadmap.md`](docs/roadmap.md) — roadmap
- [`docs/plugin-architecture.md`](docs/plugin-architecture.md) — JavaScript plugin
  system design
- [`docs/kanban-design.md`](docs/kanban-design.md) — Kanban board design
- [`docs/git-integration.md`](docs/git-integration.md) · [`docs/mobile-support.md`](docs/mobile-support.md)
