# Oxidian — Bullet Journal (US 20–22)

**Status:** phases 0–3 built — entries carry a real status (`[>] [<] [-] [o]`, not just done/not-done), weekly and monthly logs sit alongside the daily note behind a period switcher, **Review** closes a period by deciding about each unfinished entry, and the editor itself now renders and cycles signifiers instead of making you type brackets by hand. Next up: 4 (the running index).
**Source material:** [`bullet-journal.md`](bullet-journal.md) — the method as described by its authors, plus the notes that started this.
**Depends on:** `packages/index` (tasks, tags, dates), `packages/app/src/dates.rs`, `template.rs`, the tasks-move machinery in `views/vault.rs`.

> The ask was "BuJo as a plugin". §2 argues it should be core instead, and says what would have to exist first if you'd rather it were a plugin. Everything from §3 onward is the same work either way — the data model and the index queries don't care which vehicle carries them.

---

## 1. What the method actually needs from us

Stripping the method down to what software has to provide:

| BuJo concept | What it means | Nearest thing we have |
|---|---|---|
| **Rapid logging** | Short, atomic entries, one line each | Markdown list items ✅ |
| **Signifiers** | A glyph per entry *type and state*: task, done, migrated, scheduled, dropped, event, note | `- [ ]` / `- [x]` only — **binary**, the main gap |
| **Daily Log** | Today's entries under a date header | Daily note ✅ (`template.rs`, "Today's note") |
| **Weekly / Monthly Log** | Same, per week and month, plus a calendar column | ❌ — US 3.2/3.3, never built |
| **Future Log** | Things dated beyond this month | Partly: tasks carry `📅 due` ✅, nothing collects them |
| **Migration** | End-of-cycle review: each open task is moved, deferred, dropped, or done | **Half-built** — "Move to today" / "Move to…" already move open tasks between notes, transactionally |
| **Index** | A running table of contents: what exists, where it lives | `packages/index` holds every note, tag, link, heading, task ✅ — no page renders it |
| **Collections** | Thematic groupings outside the date structure | Tags + folders + DQL ✅ |

Two honest observations. First, **the index half of this is essentially done** — the `data-view` work built exactly the substrate a running index needs, which is why the idea occurred to you while looking at it. Second, **the entry model is the real work**: everything downstream (migration, review, statistics) depends on an entry knowing it was *migrated* rather than just *not done*, and today a task is a bool.

---

## 2. The plugin question

**Recommendation: build this in core.** Three reasons, all from the repo's own documents rather than taste.

**It has no vehicle.** [`plugin-architecture.md`](plugin-architecture.md) is a design sketch — there is no host, no `window.oxidian`, no manifest loader, no lifecycle, no bridge. The only plugin-shaped thing in the codebase is the **nav plugin registry** (`views/vault.rs:369`), which picks between the tree and flat-list file views at *compile* time; its `NavPlugin` struct is `#[allow(dead_code)]` and its own comment says a runtime registry "is left as future work". Building BuJo as a plugin therefore means building the entire plugin system first: the API shim, the bidirectional `document::eval` bridge, the loader, and eventually the manager UI. That is the larger of the two projects, and it would put every bit of BuJo value behind it.

**Its own dividing line puts BuJo in core.** That document says: *"if a feature requires tight UI integration, real-time reactivity, or access to internal state (signals, file SHA tracking), it belongs in core."* BuJo needs new bullet syntax parsed and rendered inside `MarkdownArea`, status write-back through the SHA-checked save path, and queries over the vault index. Its "What's not a plugin (yet)" table already lists **Daily notes** as core, for the weaker reason that they need templates and a sidebar button.

**The sketched API can't express it.** The proposed surface is `listFiles` / `readFile` / `writeFile` / `openFile` / `showNotice` / `registerCommand`. Rapid logging is an editor concern and migration is an index concern; neither reduces to reading and writing whole files without reimplementing the parser in JS and losing the SHA conflict checks.

**If you want it as a plugin anyway**, the honest sequencing is: build phases 0–1 below in core regardless (the parser and the periodic logs are shared infrastructure), then build the plugin host as its own project, then express only phases 3–4 (the migration ritual and the index page) as plugin code. You would be writing the interesting parts twice. The one real argument for it — that it proves the plugin system with a serious first customer — is a good argument, but it's an argument for building the plugin system, not for delaying BuJo behind it.

**Middle path, and what this plan assumes:** build in core, but keep the BuJo logic in its own module (`packages/index/src/bujo.rs` + `packages/app/src/views/bujo.rs`) with no reach into unrelated view state, so that if the plugin host later lands, the seam is already there. Phase 7 covers that.

---

## 3. Data model: signifiers that stay plain markdown

The vault is Markdown in a git repo and must stay readable by Obsidian — that is the product's core promise, so **no new syntax**. Fortunately the checkbox character is free-form, and Obsidian-Tasks already established conventions for exactly these states:

| BuJo | Glyph | Markdown | Obsidian-Tasks meaning |
|---|---|---|---|
| Task, open | • | `- [ ] Draft the proposal` | TODO |
| Task, complete | ✗ | `- [x] Draft the proposal` | DONE |
| Task, migrated | > | `- [>] Draft the proposal` | forwarded |
| Task, scheduled | < | `- [<] Draft the proposal` | scheduled / deferred |
| Task, dropped | ~~•~~ | `- [-] Draft the proposal` | CANCELLED |
| Event | ○ | `- [o] Standup at 10:00` | (custom) |
| Note | — | `- Prefers async reviews` | plain list item |

A note needs no marker at all — a bare list item is already "a note". That keeps the common case free of ceremony, which is the whole point of rapid logging.

### The change this forces

`index::tasks::Task` currently has `checked: bool`, and `parse_line` accepts only `[ ]`, `[x]`, `[X]` (`packages/index/src/tasks.rs:83-90`). This becomes:

```rust
pub enum Status { Open, Done, Migrated, Scheduled, Dropped, Event }
```

with `checked` kept as a derived helper (`status == Done`) so the Tasks panel, Kanban, and DQL `TASK` queries keep working unchanged. `Task` is `Serialize`/`Deserialize` into `PageData`, so this is a stored-shape change: **`FORMAT_VERSION` 2 → 3**, and an older index is discarded and rebuilt, exactly as the search change did.

Two parsing rules worth deciding now rather than discovering later:

- **An unknown marker is not a task.** `- [q] thing` should stay a plain list item, not a task with a mystery state — otherwise every stray bracket becomes an entry.
- **Migrated and dropped tasks are "closed" for rollover purposes but not "done" for statistics.** The distinction is the entire value of the review in §5; collapsing it loses the signal.

---

## 4. The logs

Daily notes exist. Weekly and monthly do not, and BuJo needs all three — so this plan **absorbs US 3.2 / 3.3** rather than duplicating it.

- **Paths and templates** follow the existing daily-note settings pattern: a folder plus a template per period (`journal/2026-08-13.md`, `journal/weekly/2026-W33.md`, `journal/monthly/2026-08.md`).
- **`dates.rs` needs to grow.** It is deliberately thin today — `today()`, `add_days`, and formatting. Weekly and monthly logs need ISO week numbers, week start/end, month length, and "the period containing date D". All of it is pure arithmetic, so it lands with unit tests and no renderer, like the rest of that module.
- **The monthly log's calendar column** (dates down the left, events beside them) is a rendered view over the index, not stored text — the entries live in the daily notes, and the monthly log shows them.

**Navigation matters more than it sounds.** The method's value comes from moving between scales — today → this week → this month → today. A period switcher that always knows where "up" and "next" are is most of the felt experience.

---

## 5. Migration: the ritual, and the part we nearly have

Migration is what separates BuJo from a to-do list: at the end of each cycle you touch every unfinished task and decide — do it, move it, schedule it, or drop it. The friction is deliberate; a task you've rewritten three times is telling you something.

**We already move tasks between notes transactionally.** `views/vault.rs` has "Move to today" and "Move to…", built on `tasks::extract_lines` / `append_lines`, and the e2e suite pins the ordering that matters: *"the destination is written before the source is emptied"*, so a failure between the two writes duplicates tasks rather than losing them. That is the hard half of migration, already built and tested.

What is missing is the **review surface**: one screen, one entry at a time, showing every open entry from the closing period with four actions.

| Action | Writes | Result |
|---|---|---|
| **Done** | `- [x]` + `✅ YYYY-MM-DD` in place | Completed, stays in its original log |
| **Migrate** | `- [>]` in place, appended as `- [ ]` in the next period's log | Moved forward |
| **Schedule** | `- [<]` in place, appended to the Future Log with `📅 date` | Deferred to a chosen date |
| **Drop** | `- [-]` in place | Consciously abandoned, still visible |

The original line is never deleted — it is re-marked. That is true to the paper method (you can see your own churn) and it is what makes the statistics in phase 6 possible.

**Migration count.** A task migrated repeatedly should surface as such. Since the origin line survives, "how many times has this text been migrated" is answerable by following the chain; the simplest honest version is a counter written into the migrated line, and the alternative — inferring it by matching text across logs — is fragile. Decide in phase 3 (§8, open question 2).

---

## 6. The running index

This is the piece you were most drawn to, and it is the cheapest of the lot, because `packages/index` already holds everything it needs.

An **Index page** is a generated view, not a file you maintain:

- **Collections** — every tag with a count, every folder with notes, ordered by recent activity. `Index::tag_counts` and `pages_with_tag` exist and are tested.
- **First seen / last touched** — when a topic first appeared and when it was last written to. *Note:* the index does not store timestamps today; it stores content and blob SHAs. Either derive dates from the log a topic first appears in (free, and very BuJo — the daily log *is* the timestamp), or add a `first_seen` field. The first is recommended: it needs no new state and it is what a paper index does.
- **Threads** — the "little project that shows up in an index page" case: a tag or folder that has gained entries recently, with its most recent entries inline.

Rendered two ways, cheaply: as a **panel** (live, sortable, always current), and as a **`dataview` block** you can paste into a note (`TABLE FROM #project SORT ...`), which already works today. The panel is the product; the block is the escape hatch.

---

## 7. Phasing

Each phase is independently shippable, and the early ones pay for themselves even if BuJo stops half-built.

| Phase | Deliverable | Depends on |
|---|---|---|
| **0** ✅ | `Status` enum replacing `checked: bool`; parse and write `[>] [<] [-] [o]`; `FORMAT_VERSION` → 3; Tasks panel and DQL `TASK` show the new states. Pure `packages/index` work, fully unit-testable. `set_status_content` is the single write path, which phase 3 reuses. | — |
| **1** ✅ | Weekly and monthly logs (absorbs the weekly/monthly half of US 3.2/3.3): `weekly_note_template` / `monthly_note_template` settings, `dates::Period` arithmetic, palette commands, and the period switcher. Useful on its own to anyone who wants weekly notes. | 0 |
| **2** ✅ | Rapid logging in the editor: a keyboard shortcut and slash commands to cycle an entry's signifier, and glyph rendering in `MarkdownArea`. Makes the syntax pleasant instead of merely possible. | 0 |
| **3** ✅ | **The migration ritual.** Review over the closing period with Done / Migrate / Schedule / Drop (`MigrationReview` + `migrate_tasks`), destination written first. Not built on the existing move: the original line is re-marked, never cut. **The first recognisably-BuJo release.** | 1 |
| **4** | **The running Index** — collections, threads, first-seen/last-touched, as a panel. | 0 |
| **5** | Future Log: collect `📅`-dated and `[<]` entries beyond this month; migrate them into a month when it opens. | 3 |
| **6** | Reflection: per-period statistics — completed vs migrated vs dropped, and repeat-migration nagging. The "review what got done" half of the original ask. | 3 |
| **7** | *(Optional)* Expose the module through the plugin host, once one exists (§2). | plugin host |

Phases 0–1 are worth doing regardless of whether BuJo ever ships: richer task states improve the Tasks panel and Dataview immediately, and weekly/monthly notes are an outstanding user story in their own right.

---

## 8. Open questions

1. **Events as `- [o]` or as a plain bullet with a time?** `[o]` makes events indexable by the same parser and queryable in DQL; a plain bullet is more portable. Recommendation: `[o]`, decided in phase 0 because it changes the parser.
2. **How to count migrations** — a counter in the line, or inferred by matching text across logs? Affects phase 3, and the answer determines whether phase 6's nagging is trustworthy.
3. **Does the Index page get written to the vault?** A real BuJo index is a page in the notebook. Generating it live is easier and always correct; writing it means it survives outside Oxidian. Recommendation: live panel first, "export index to a note" later.
4. **Where do weekly/monthly logs live** — one `journal/` folder with a naming convention, or a folder per period type? Affects settings shape in phase 1.

---

## 9. User stories

Continuing the numbering in [`user-stories.md`](user-stories.md), which ends at Epic 19.

### **Epic 20: Rapid Logging**

*Capturing entries as fast as thinking about them.*

* **US 20.1:** As a user, I want to mark a list entry as a task, event, or note using standard markdown so that my journal stays readable in Obsidian and in a plain text editor.
* **US 20.2:** As a user, I want a keyboard shortcut that cycles the entry on the current line through its states (open → done → migrated → scheduled → dropped) so that I can log without leaving the keyboard.
* **US 20.3:** As a user, I want entry signifiers rendered as their BuJo glyphs in the editor so that I can scan a log's shape at a glance rather than reading brackets.
* **US 20.4:** As a user, I want a slash command for each entry type so that I don't have to remember the bracket syntax.

### **Epic 21: Logs and Migration**

*The daily/weekly/monthly rhythm, and the review that holds it together.*

* **US 21.1:** As a user, I want weekly and monthly log notes created from their own templates so that I can plan at a scale larger than a day.
* **US 21.2:** As a user, I want to move between today, this week, and this month in one click so that changing scale is effortless enough to actually do.
* **US 21.3:** As a user, I want my monthly log to show a calendar column with the events from that month's daily logs so that I can see the shape of the month without re-entering anything.
* **US 21.4:** As a user, I want to be prompted to review the previous period's unfinished entries when I open a new one so that migration becomes a habit rather than a chore I forget.
* **US 21.5:** As a user, I want to mark each unfinished entry as done, migrated, scheduled, or dropped in one pass so that the review is quick and deliberate.
* **US 21.6:** As a user, I want a migrated entry to remain visible in its original log, marked as migrated, so that I can see my own churn instead of a falsified history.
* **US 21.7:** As a user, I want entries scheduled beyond this month collected into a Future Log so that far-off commitments are not lost between periods.
* **US 21.8:** As a user, I want to be told when I have migrated the same task several times so that I can decide whether it actually matters to me.

### **Epic 22: Index and Collections**

*The running table of contents.*

* **US 22.1:** As a user, I want an Index page listing every collection in my vault with how much is in it so that I can see everything I have going on in one place.
* **US 22.2:** As a user, I want to see when a topic first appeared and when I last wrote about it so that I can tell a live thread from a dormant one.
* **US 22.3:** As a user, I want a new project I start mentioning to show up in the Index by itself so that I never have to maintain the table of contents by hand.
* **US 22.4:** As a user, I want to click a collection in the Index to see its entries so that the Index is a way in, not just a summary.
* **US 22.5:** As a user, I want a per-period summary of what I completed, migrated, and dropped so that reflection is based on what actually happened.

---

## 10. Phase 2: what actually shipped

`index::tasks::Status` was already the one true signifier table (§3); phase 2's job was making the editor speak it instead of only reading `[ ]`/`[x]`. `packages/ui` took a dependency on `packages/index` for this — safe, since `index` has no Dioxus dependency, and it means the editor can never drift from what the parser and the migration review agree a signifier means.

- **Tokenizer** (`ui::cm::markdown_area::tokenizer`): `TaskItem` now carries a `Status` instead of a `checked: bool`, recognizing all six markers (`Status::from_marker`). An unrecognized bracket char (`- [q] …`) still falls through to a plain `ListItem`, per the open-question-1 decision in §3.
- **Rendering** (`component.rs`): the checkbox span carries `data-status` and `data-glyph`; CSS draws the existing box+check for open/done and the raw glyph (`› ‹ – ○`) as text for the other four, since there is no "checked" shading to apply to a signifier that isn't a boolean.
- **Click-to-toggle stays Open⇄Done only.** Clicking a migrated/scheduled/dropped/event checkbox is a no-op — those don't have an obvious single click action, and reusing the old boolean toggle on them would have silently downgraded a migrated task back to a plain open one. The handler resolves the *actual* status of the nearest task token rather than trusting the click's cached checked-flag, so a stray click can't accidentally flip an unrelated task elsewhere in the note (see the comment at the `cb:` handler).
- **Cycling** is `Ctrl`/`Cmd`+`Enter` on the current line: open → done → migrated → scheduled → dropped → open. Plain `Enter` was already spoken for (new line / continue list); this reuses the modifier-Enter slot the code already reserved. Event (`[o]`) isn't part of the cycle — it's a distinct entry type, reached via slash command, not a state a task passes through.
- **Slash commands**: `Event`, `Migrated`, `Scheduled`, `Dropped` alongside the existing `Task`, inserting the matching bracket.
- **Not changed:** the task-metadata menu's arm-on-space regex and the Enter-continuation regex both now accept any of the eight bracket chars (so migrated/dropped lines still continue as a new `[ ]` line on Enter), but no new UI was added for due dates/priority on non-open signifiers — that machinery was already signifier-agnostic.
