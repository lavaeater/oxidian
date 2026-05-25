# Oxidian — Roadmap

This document sequences the work from `git-integration.md` and `user-stories.md` into milestones. Each milestone is shippable on its own — a user can pick up the app at any milestone and get real value from it.

---

## Current status (May 2026)

The app is live and working on **web** and **Android**. The core read/write loop, editor, graph, and multi-provider support are done. Below is a detailed status for every user story.

---

## Implemented user stories

| US | Feature | Notes |
|----|---------|-------|
| US 1.1 / 1.2 | **Bookmarks** — pin notes, dedicated sidebar pane | ✅ Done |
| US 3.1 | **Daily note** — one-click creation | ✅ Done |
| US 4.1 / 4.2 | **Graph view** — force-directed global graph, backlinks | ✅ Done |
| US 6.1 / 6.2 | **Outline pane** — live heading tree, click-to-scroll | ✅ Done |
| US 8.1 / 8.2 | **Properties view** — YAML frontmatter editor, collapsible | ✅ Done |
| US 9.1 | **Export as HTML** — standalone single-note export | ✅ Done |
| US 10.1 / 10.2 | **Quick Switcher** — fuzzy file search modal | ✅ Done |
| US 11.1 | **Global search** — full-text search across all notes | ✅ Done |
| US 12.1 | **Slash commands** — `/` menu for tables, callouts, etc. | ✅ Done |
| US 16.1 / 16.2 | **Word count** — live word count in editor status bar | ✅ Done |
| US 17.1 | **Formatting toolbar** — floating Bold/Italic/Heading/List toolbar | ✅ Done |
| — | **GitLab support** — second provider alongside GitHub | ✅ Done |
| — | **Auto-save** — saves every 2 s of inactivity | ✅ Done |
| — | **Saved/Unsaved status** — visual indicator in editor bar | ✅ Done |
| — | **File tree with folder collapse** — folders collapsed by default | ✅ Done |
| — | **WikiLink index + backlinks panel** | ✅ Done |
| — | **Android mobile app** — shared `app` crate, drawer nav, bottom bar | ✅ Done |

---

## Not yet implemented

Listed roughly by priority / dependency order.

### High priority — complete the M2/M3 layer

| US | Feature | Effort |
|----|---------|--------|
| US 11.2 | Search filters (`path:`, `tag:`, `file:` prefixes) | Small |
| US 14.1 / 14.2 | **Tags pane** — collects all `#tags`, click to search | Medium |
| US 2.1 / 2.2 | **Command Palette** (Ctrl+P) — fuzzy command search | Medium |
| US 15.1 / 15.2 | **Templates** — template folder setting, insert via slash/palette | Medium |
| US 3.2 / 3.3 | Weekly/Monthly notes; natural-language date parsing (`@today`, `@next friday`) | Medium |

### Medium priority — knowledge graph completeness

| US | Feature | Effort |
|----|---------|--------|
| US 7.1 | **Hover preview** — popover on `[[link]]` hover | Small |
| US 4.2 | Local graph (currently global-only; local subgraph view) | Medium |

### Medium priority — desktop power

| US | Feature | Effort |
|----|---------|--------|
| — | **Local git backend** (`git2-rs`) — offline-first desktop vault | Large |
| — | Directory picker (open any local folder) | Small (needs local backend) |
| — | Push/pull buttons + ahead/behind badge | Small (needs local backend) |

### Lower priority — advanced composition

| US | Feature | Effort |
|----|---------|--------|
| US 5.1 | **Extract to note** — selection → new note + `[[link]]` | Medium |
| US 5.2 | **Merge notes** — append + delete source + update links | Medium |
| US 18.1 / 18.2 | **Dataview** — SQL-like query blocks rendered inline | Large |
| US 19.1 / 19.2 | **Kanban** — headings as columns, drag to reorder | Large |

### Publishing & export

| US | Feature | Effort |
|----|---------|--------|
| US 9.2 | **Static site export** — compile vault to HTML site | Large |
| US 13.1 / 13.2 | **Presentation / slides mode** — `---` splits into slides | Medium |

### Multi-provider & mobile completeness

| US | Feature | Effort |
|----|---------|--------|
| — | **Gitea backend** — third provider | Medium |
| — | **OAuth Device Flow** — no-PAT auth for GitHub/GitLab | Medium |
| — | **iOS build** — Dioxus mobile iOS target | Medium |
| — | **PAT in platform keychain** (Keychain on iOS, Keystore on Android) | Medium |
| — | Swipe-right to file list, pull-to-refresh, haptic feedback | Small |

---

## Milestone map (what's been shipped vs. what's next)

```
M0  Vault foundation            ✅ Done
M1  Usable editor loop          ✅ Done
M2  Navigation & discovery      ✅ Search/Bookmarks/Backlinks/Outline
                                 ⬜ Tags pane, Command Palette (remaining)
M3  Editing productivity        ✅ Slash commands, Properties, Toolbar, Daily note
                                 ⬜ Templates, Weekly/Monthly notes (remaining)
M4  Local git (desktop)         ⬜ Not started
M5  Knowledge graph             ✅ Force-directed graph, WikiLink index, Backlinks
                                 ⬜ Hover preview (remaining)
M6  Note refactoring            ⬜ Not started
M7  Multi-provider & mobile     ✅ GitHub + GitLab, Android
                                 ⬜ Gitea, OAuth, iOS (remaining)
M8  Publish & export            ✅ Single-note HTML export
                                 ⬜ Static site, Slides (remaining)
```

---

## Suggested next sprint

1. **Tags pane** (US 14) — completes M2
2. **Command Palette** (US 2) — high UX value, ties together all commands
3. **Hover preview** (US 7) — small effort, completes M5
4. **Templates** (US 15) — unblocks M3 completion
5. **Search filters** (US 11.2) — small, makes search much more useful

---

## Dependency graph

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
