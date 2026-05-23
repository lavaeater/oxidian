# Oxidian — Roadmap

This document sequences the work from `git-integration.md` and `user-stories.md` into milestones. Each milestone is shippable on its own — a user can pick up the app at any milestone and get real value from it.

Dependencies drive the order: the vault backend must exist before any feature that reads or writes files; the file tree must exist before search, tags, or the graph; WikiLink resolution must work before hover previews and the graph view.

---

## Milestone 0 — Vault foundation

> *You can connect to a GitHub repo and open a note.*

This is pure infrastructure. Nothing shippable to end users yet, but everything else depends on it.

**Git integration (from `git-integration.md`)**
- Define the `VaultBackend` trait and supporting types (`FileMeta`, `CommitMeta`) in a new `packages/vault` crate
- Implement `GitHubApiBackend`: `list_files`, `read_file`
- PAT authentication: settings screen, token stored in localStorage (web) / keychain (native)
- Wire `VaultBackend` into a Dioxus context so any component can access it

**App shell**
- File tree sidebar (flat list first, folder grouping later)
- Click a file → opens in `MarkdownArea` (already built)
- Route structure: `/` = file tree, `/note/:path` = editor

**Done means:** a user pastes their GitHub PAT, picks a repo, and can browse and read their markdown notes.

---

## Milestone 1 — Usable editor loop

> *You can edit and save notes. The app is genuinely useful for daily use.*

**Git integration**
- `write_file` on `GitHubApiBackend` (Contents API PUT with blob SHA precondition)
- Auto-save after ~2 seconds of inactivity; explicit Ctrl+S also works
- "Saved · 3s ago" / "Unsaved changes" status indicator in the bottom bar
- 409 Conflict detection with simple "keep yours / keep theirs" resolution

**User stories covered**
- **US 16.1 / 16.2** — Word and character count in the status bar; updates to selection count on highlight
- **US 10.1 / 10.2** — Quick Switcher (Ctrl+O): fuzzy-search over the file tree, prioritises recently opened files

**Done means:** open a note, edit it, save it, see it appear as a commit in GitHub. Word count and quick file switching work.

---

## Milestone 2 — Navigation and discovery

> *You can find things across your vault without already knowing where they are.*

These features all read from the file tree that Milestone 0 built.

**User stories covered**
- **US 11.1 / 11.2** — Global full-text search pane; filter by `path:`, `tag:`, `file:` prefixes. Index built client-side from the file listing + content fetch on demand.
- **US 14.1 / 14.2** — Tags pane: collects all `#tag` occurrences across files, displays as a collapsible tree, clicking a tag runs a pre-filled search.
- **US 6.1 / 6.2** — Outline pane: parses headings from the active note, click to scroll.
- **US 1.1 / 1.2** — Bookmarks: pin a note or a note+heading anchor; dedicated sidebar pane; persisted in a `.oxidian/bookmarks.json` file in the repo.
- **US 2.1 / 2.2** — Command Palette (Ctrl+P): fuzzy search over all registered commands; opens other features (new note, switch branch, toggle sidebar, etc.).

**Done means:** you can find any note by content or tag, navigate within long notes, bookmark frequently used ones, and drive the whole app from the keyboard.

---

## Milestone 3 — Editing productivity

> *Writing is faster and the app guides you on formatting.*

**User stories covered**
- **US 17.1** — Editing toolbar: floating toolbar on text selection (Bold, Italic, Strike, H1–H3, bullet list, checkbox). Wraps text with the appropriate markdown markers.
- **US 12.1** — Slash commands (`/` in editor): contextual insert menu for tables, task lists, callouts, and template insertion.
- **US 8.1 / 8.2** — Properties view: renders YAML frontmatter at the top of a note as a key/value UI; collapsible; writes back to the `---` block on change.
- **US 15.1 / 15.2** — Templates: designate a folder as template source; insert via command palette or slash command.
- **US 3.1 / 3.2 / 3.3** — Periodic notes: one-click Daily/Weekly/Monthly note creation with template application; natural-language date parsing (`@today`, `@next friday`) converts inline to `[[YYYY-MM-DD]]` links.

**Done means:** a new user with no markdown knowledge can format notes comfortably; power users can move fast with slash commands and templates.

---

## Milestone 4 — Local backend and desktop power features

> *The desktop app works entirely offline and syncs on your terms.*

**Git integration**
- New `LocalGitBackend` in `packages/vault` behind the `desktop` feature flag, using `git2-rs`
- Open any local folder as a vault (directory picker)
- Stage + commit on every save (same auto-save trigger as M1)
- Push / pull buttons; ahead/behind commit count badge in the status bar
- SSH and HTTPS credential handling via `git2` credential helpers

**Done means:** on desktop, the app works with zero network; sync is explicit and the user sees their local Git history.

---

## Milestone 5 — Knowledge graph and WikiLink intelligence

> *Your vault is a connected graph, not just a list of files.*

These features require resolving `[[WikiLink]]` targets to actual file paths — the full file tree must be in memory and WikiLink targets must be matched by title.

**User stories covered**
- **US 4.1** — Global graph view: force-directed graph of all notes and their `[[link]]` connections; click a node to open the note.
- **US 4.2** — Local graph: subgraph centred on the active note, 1–2 hops out.
- **US 7.1** — Hover preview: hovering an internal `[[link]]` shows a popover with the first 200 characters of the target note and its title.

**Infrastructure**
- WikiLink index: built from the full file tree on vault load, updated incrementally on save.
- Backlinks panel: shows all notes that link to the active note.

**Done means:** the vault feels like a network, not a folder. You can explore connections without knowing them in advance.

---

## Milestone 6 — Note refactoring and advanced composition

> *The vault is easy to restructure as it grows.*

**User stories covered**
- **US 5.1** — Extract to note: select text → "Extract to new note" command → creates the note and replaces the selection with a `[[link]]`.
- **US 5.2** — Merge notes: merge the active note into a chosen existing note (appends content, deletes the source, updates all inbound links).
- **US 18.1 / 18.2** — Dataview: a special ` ```dataview ` code block that executes a SQL-like query over the file index and renders results as a list, table, or task list inline in the note.
- **US 19.1 / 19.2** — Kanban: a "Kanban view" toggle on notes whose top-level headings represent columns; drag-and-drop reorders the underlying markdown list items.

---

## Milestone 7 — Multi-provider and mobile

> *The app works on any device, with any Git host.*

**Git integration**
- `GitLabApiBackend` and `GiteaApiBackend` (same trait, different base URLs and auth headers)
- Settings UI: choose provider, enter base URL, authenticate
- OAuth Device Flow for GitHub and GitLab (no server required)

**Mobile**
- iOS and Android builds via Dioxus mobile
- PAT stored in platform keychain
- API-only backend (same as web); no local git on first cut
- Touch-optimised toolbar (larger tap targets, swipe to switch files)

**Done means:** the app works on iPhone and Android; users on GitLab or self-hosted Gitea can connect their vault.

---

## Milestone 8 — Publishing and export

> *Your vault can be shared with people who don't use Oxidian.*

**User stories covered**
- **US 9.1** — Export single note as standalone HTML (styles inlined, no external deps).
- **US 9.2** — Compile vault or folder to a static HTML site (Oxidian-branded, WikiLinks become relative HTML links, full-text search index baked in).
- **US 13.1 / 13.2** — Slides / presentation mode: `---` horizontal rules divide the note into slides; a presentation view renders them full-screen with keyboard navigation.

---

## Dependency graph (summary)

```
M0 (vault backend + file tree)
  └── M1 (edit + save)
        └── M2 (search, tags, outline, bookmarks, command palette)
              ├── M3 (toolbar, slash commands, templates, periodic notes)
              ├── M4 (local git backend — desktop only)
              └── M5 (graph view, hover preview, WikiLink index)
                    ├── M6 (note composer, dataview, kanban)
                    └── M7 (multi-provider, mobile)
                          └── M8 (publish + export)
```

M3, M4, and M5 can be developed in parallel once M2 is done. M6 depends on M5 (needs the WikiLink index). M8 can start as soon as M1 is done (export is mostly renderer work).

---

## What is already done

| Area | Status |
| --- | --- |
| Hybrid WYSIWYG markdown editor (`MarkdownArea`) | ✅ Done |
| Tokenizer with inline + block tokens | ✅ Done |
| Task checkboxes | ✅ Done |
| Fenced code blocks | ✅ Done |
| Tables | ✅ Done |
| Obsidian-style line-level marker reveal | ✅ Done |
| `VaultBackend` trait (design) | 📄 Designed, not implemented |
| Everything else | 🔲 Not started |
