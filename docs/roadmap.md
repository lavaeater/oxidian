# Oxidian — Roadmap

This document sequences the work from `git-integration.md` and `user-stories.md` into milestones. Each milestone is shippable on its own — a user can pick up the app at any milestone and get real value from it.

---

## Current status (August 2026)

The app is live and working on **web** and **Android**. The core read/write loop, editor, graph, Kanban, and multi-provider support are done. The `data-view` branch added the shared vault index and, on top of it, inline Dataview blocks and local full-text search.

**Platform priority is now web + mobile (Android).** The author always has either a computer or an Android phone, and web also covers iPad. `packages/desktop` still builds and shares all the same code, but is no longer a focus — so the desktop-only **local git backend (M4)** and **iOS** are deprioritized. See `CLAUDE.md`.

Below is a detailed status for every user story.

---

## Implemented user stories

| US | Feature | Notes |
|----|---------|-------|
| US 1.1 / 1.2 | **Bookmarks** — pin notes, dedicated sidebar pane | ✅ Done |
| US 3.1 | **Daily note** — one-click creation (uses the template engine) | ✅ Done |
| US 4.1 | **Graph view** — force-directed global graph | ✅ Done |
| US 4.2 | **Backlinks** — backlinks panel | ✅ Done |
| US 6.1 / 6.2 | **Outline pane** — live heading tree, click-to-scroll | ✅ Done |
| US 8.1 / 8.2 | **Properties view** — YAML frontmatter editor, collapsible | ✅ Done |
| US 9.1 | **Export as HTML** — standalone single-note export | ✅ Done |
| US 2.1 / 2.2 | **Command Palette** (Ctrl/⌘-P) — fuzzy command search + reusable global keyboard-shortcut framework (`shortcuts.rs`) | ✅ Done |
| US 10.1 / 10.2 | **Quick Switcher** — fuzzy file search modal (Ctrl/⌘-O) | ✅ Done |
| US 11.1 | **Global search** — full-text search across all notes, served from the local index (`index::search`; see `docs/dataview.md` §12). Was GitHub code-search, which the browser blocks on CORS. | ✅ Done |
| US 11.2 | **Search filters** — `path:`, `file:`, `tag:` prefixes, ANDed with the free-text terms; filters alone are a valid query (`tag:daily`) | ✅ Done |
| US 12.1 | **Slash commands** — `/` menu for tables, callouts, etc. | ✅ Done |
| US 16.1 / 16.2 | **Word count** — live word count in editor status bar | ✅ Done |
| US 17.1 | **Formatting toolbar** — floating Bold/Italic/Heading/List toolbar | ✅ Done |
| US 19.1 / 19.2 | **Kanban** — headings as columns, drag to reorder | ✅ Done |
| US 18.1 / 18.2 | **Dataview** — `dataview` fences render inline: `TABLE` / `LIST` / `TASK`, `FROM` / `WHERE` / `SORT` / `LIMIT`, ~20 built-in functions. A block folds back to its source when the caret enters it or you click the output, and `TASK` results toggle through to the note. Phases 0–5 of [`dataview.md`](dataview.md); 6–8 remain, see below | ✅ Done |
| — | **Shared vault index** (`packages/index`) — one incrementally-refreshed model of the vault (frontmatter, inline `key:: value` fields, tags, headings, links, tasks, body text) behind Tasks, backlinks, graph, search and Dataview, stored one record per note in IndexedDB (web) / a file each (native), with a storage footer and Rebuild | ✅ Done |
| — | **Tasks view** — vault-wide checkbox aggregation grouped by file, Obsidian-Tasks metadata (📅 due / priority / ✅ done), toggle + jump (`tasks.rs`, `Panel::Tasks`) | ✅ Done |
| — | **GitLab support** — second provider alongside GitHub | ✅ Done |
| — | **GitHub OAuth device flow** — no-PAT sign-in (incl. mobile copy-code + token persistence) | ✅ Done |
| — | **Auto-save** — saves after ~5 s of inactivity, with countdown | ✅ Done |
| — | **Saved/Unsaved status** — visual indicator in editor bar | ✅ Done |
| — | **File tree with folder collapse** — folders collapsed by default | ✅ Done |
| — | **Delete file / folder** — with confirm dialog | ✅ Done |
| — | **Drag-and-drop move** — move files/folders in the tree | ✅ Done |
| — | **Tabs + two-pane split** — `EditorPane`, Obsidian-style preview tabs | ✅ Done |
| — | **Responsive sidebar drawer** — mobile drawer nav + bottom bar | ✅ Done |
| — | **WikiLink index + backlinks panel** | ✅ Done |
| — | **Android mobile app** — shared `app` crate; bridge/save/editor all working | ✅ Done |
| — | **Persistent native auth** — token/bookmarks/board stored on the filesystem (`native_store.rs`), survives cold restarts on Android/desktop instead of relying on WebView `localStorage` | ✅ Done |

---

## Not yet implemented

Listed roughly by priority / dependency order. Filtered to the web + Android focus.

### High priority — complete the M2/M3 layer

| US | Feature | Effort |
|----|---------|--------|
| US 15.1 / 15.2 | **Templates** — folder setting + insert via slash/palette (*engine + daily-note done; general insertion remaining*) | Small–Medium |
| US 14.1 / 14.2 | **Tags pane** — collects all `#tags`, click to search. Downgraded from Medium: the index already collects tags (`tag_counts` / `pages_with_tag`, nested tags kept whole) and search already understands `tag:`, so only the panel is left | Small |
| US 3.2 / 3.3 | Weekly/Monthly notes; natural-language date parsing (`@today`, `@next friday`) | Medium |

### Medium priority — knowledge graph completeness

| US | Feature | Effort |
|----|---------|--------|
| US 7.1 | **Hover preview** — popover on `[[link]]` hover (only CSS `:hover` today) | Small |
| US 4.2 | Local graph (currently global-only; local subgraph view) | Medium |

### Lower priority — advanced composition

| US | Feature | Effort |
|----|---------|--------|
| US 5.1 | **Extract to note** — selection → new note + `[[link]]` | Medium |
| US 5.2 | **Merge notes** — append + delete source + update links | Medium |
| US 18.1 / 18.2 | **Dataview — remaining phases** — inline `` `= expr` `` queries (phase 6); bulk archive seed for a full offline vault (phase 7, partly overtaken — see below); `dataviewjs` (phase 8, maybe never). Plus the DQL clauses deliberately deferred in phase 3: `GROUP BY`, `FLATTEN`, `CALENDAR`, each of which parses to a "not supported yet" message rather than a syntax error | Medium |

### Publishing & export

| US | Feature | Effort |
|----|---------|--------|
| US 9.2 | **Static site export** — compile vault to HTML site | Large |
| US 13.1 / 13.2 | **Presentation / slides mode** — `---` splits into slides | Medium |

### Mobile completeness (Android)

| US | Feature | Effort |
|----|---------|--------|
| — | **Encrypt token in Android Keystore** — hardening only; the token already persists across restarts in app-private storage (`native_store.rs`, `getFilesDir()`), just in plaintext | Medium |
| — | Swipe-right to file list, pull-to-refresh, haptic feedback | Small |

### Deprioritized (desktop-only / out of scope for now)

| US | Feature | Why parked |
|----|---------|------------|
| — | **Local git backend** (`git2-rs`), directory picker, push/pull badge (M4) | Desktop-only; desktop is no longer a focus |
| — | **iOS build** + iOS Keychain | Out of scope (no Apple device in the loop) |
| — | **Gitea backend** — third provider | Optional; GitHub + GitLab cover current needs |

---

## Milestone map (what's been shipped vs. what's next)

```
M0  Vault foundation            ✅ Done
M1  Usable editor loop          ✅ Done (+ tabs/split, delete, drag-move)
M2  Navigation & discovery      ✅ Search/Bookmarks/Backlinks/Outline/Quick Switcher/Command Palette
                                 ⬜ Tags pane (remaining)
M3  Editing productivity        ✅ Slash, Properties, Toolbar, Daily note, template engine
                                 ⬜ Template insertion, Weekly/Monthly notes (remaining)
M4  Local git (desktop)         ⬜ Deprioritized (desktop not a focus)
M5  Knowledge graph             ✅ Force-directed graph, WikiLink index, Backlinks
                                 ⬜ Hover preview, local graph (remaining)
M6  Note refactoring            ⬜ Not started (Extract, Merge)
                                 ✅ Dataview phases 0–5 — blocks render, fold to source, TASK write-back
                                 ⬜ Dataview phases 6–8 — inline `= expr`, offline mirror, dataviewjs
M7  Multi-provider & mobile     ✅ GitHub + GitLab + OAuth device flow, Android (fully working)
                                 ⬜ Android Keystore, mobile gestures; Gitea/iOS parked
M8  Publish & export            ✅ Single-note HTML export
                                 ⬜ Static site, Slides (remaining)
```

---

## Suggested next sprint

Ordered for the web + Android focus (no desktop/local-git dependencies):

1. **Templates — general insertion** (US 15) — *in progress*: reuse the existing engine; expose "Insert template" via the Command Palette (and slash menu).
2. **Tags pane** (US 14) — completes M2, and is now the cheapest item on the board: the index and the `tag:` search filter are both already there, so this is a panel and a click handler.
3. **Hover preview** (US 7) — small effort, completes M5.
4. **Weekly/Monthly notes + natural-language dates** (US 3.2/3.3) — completes M3.

*(Done: Command Palette + global keyboard-shortcut framework — US 2.)*

**Cross-cutting, landed on `data-view`:** the shared vault index (`packages/index`) generalises
`tasks_cache`'s blob-SHA diffing so Tasks, backlinks, graph, search, and Dataview all read one
incrementally-refreshed index instead of several partial ones. Two knock-on effects worth knowing
when picking the next thing up:

- The **Tags pane (US 14)** is now mostly a UI job — `Index::tag_counts` and `pages_with_tag`
  already exist and are tested.
- **Phase 7 (full offline vault) is half-done by accident.** Search needed every note's body, so
  the index already stores it — the content mirror exists in substance. What is missing is the
  bulk *seed*: one tarball request instead of thousands of `read_file` calls, plus the two open
  questions in [`dataview.md`](dataview.md) §7 (does `codeload.github.com` send CORS headers for an
  authenticated request, and where to decompress gzip in wasm).

---

## Dependency graph

```
M0 (vault backend + file tree)
  └── M1 (edit + save + tabs/split)
        └── M2 (search, tags, outline, bookmarks, command palette)
              ├── M3 (toolbar, slash commands, templates, periodic notes)
              ├── M4 (local git backend — desktop only, parked)
              └── M5 (graph view, hover preview, WikiLink index)
                    ├── M6 (note composer, dataview, kanban ✅)
                    └── M7 (multi-provider, mobile)
                          └── M8 (publish + export)
```

## Most-wanted (author)

- ~~Delete file / folder~~ ✅ Done
- ~~File and folder sync~~ ✅ Done (read/write loop)
- Keyboard shortcuts (arrives with the Command Palette)
