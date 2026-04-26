---
name: Oxidian project vision
description: What the app is supposed to be — Obsidian-like second brain backed by a GitHub repo
type: project
---

Oxidian is a web-based "second brain" / note-taking app, similar to Obsidian and the foam-brain VS Code plugin.

**Core concept:** authenticate via GitHub OAuth, then use a chosen GitHub repository as the note vault (read/write files via the GitHub API or git).

**Key features planned:**
- GitHub OAuth login (repo used as storage backend)
- File browser / graph view of notes in the repo (like Obsidian vault)
- Hybrid WYSIWYG markdown editor: notes render as formatted markdown, but clicking/focusing a line switches that line to raw editable markdown (Obsidian-style inline editing — not a split-pane preview)

**Platform priority:** web first (`packages/web`), then potentially desktop/mobile.

**Why:** Markdown files live in a git repo so they are version-controlled, shareable, and portable — no proprietary format.

How to apply: keep the editor experience as the north star when making architecture decisions. Storage is GitHub API/git. Auth is GitHub OAuth. Start with the web package.
