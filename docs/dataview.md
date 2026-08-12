# Oxidian — Dataview (US 18.1 / 18.2)

**Status:** phases 0–5 implemented — a `dataview` block renders in the editor, folds back to source when you click it, and `TASK` results are clickable. The index lives in IndexedDB (web) / its own file (native). Phase 6 (inline expressions) is next; see §9 for the phase table.
**Branch:** `data-view`.
**Code:** `packages/index` (`extract`, `value`, `frontmatter`, `tasks`, `Index`, `dql/{parse,eval,render}`), `packages/app/src/vault_index.rs`.

**Prior art:** [obsidian-dataview](https://github.com/blacksmithgu/obsidian-dataview) (DQL + DataviewJS), and its successor experiment [datacore](https://github.com/blacksmithgu/datacore), which exists largely because Dataview's re-query-everything-on-every-change model does not scale.

> Sections 2–8 are the original analysis, kept as written so the reasoning behind the design stays legible. Where a gap has since been closed, §9 and §10 say so — the analysis text is not retro-edited.

---

## 1. What we are building

A fenced code block whose *content is a query* and whose *rendered output is markdown/HTML*:

````markdown
```dataview
table time-played, length, rating
from "games"
sort rating desc
```
````

renders as a table of every note under `games/`, one row per note, with columns pulled from each note's frontmatter.

Two user stories:

- **US 18.1** — write a SQL-like query in a `dataview` block, filtering on tags, folders, and YAML properties.
- **US 18.2** — render results as **list**, **table**, or **task list** inline in the editor.

The hard part is not the query language. It is that every query is a query over *the whole vault*, and today Oxidian's vault lives behind the GitHub/GitLab REST API, one HTTP request per file. Section 6 onwards is about fixing that.

---

## 2. What Dataview actually is (feature inventory)

Dataview is three separable products. We should be explicit about which we are signing up for.

### 2.1 The index (the real product)

An in-memory model of the vault where every note is a **page object**:

| Field | Source | Have it today? |
|---|---|---|
| `file.path`, `file.name`, `file.folder`, `file.ext` | path | ✅ `FileMeta::name()` / `dir()` |
| `file.size` | tree listing | ✅ `FileMeta.size` |
| `file.link` | path | ✅ trivial |
| `file.tags`, `file.etags` | `#tag` in body + `tags:` frontmatter | ❌ no tag index (roadmap US 14) |
| `file.outlinks` | `[[wikilinks]]` | ✅ `wikilink_index::outlinks` |
| `file.inlinks` | reverse of the above | ✅ `wikilink_index::backlinks` |
| `file.tasks` | `- [ ]` items | ✅ `tasks::parse_file` (already parses 📅/✅/priority) |
| `file.lists` | all list items | ⚠️ tokenizer has `ListItem`, not aggregated |
| `file.ctime`, `file.mtime`, `file.cday`, `file.mday` | filesystem mtime | ❌ **no cheap source** — see §2.5 |
| `file.day` | date parsed out of the filename | ✅ trivial (`dates.rs`) |
| *arbitrary frontmatter keys* | YAML | ⚠️ `properties::parse_pairs` is scalar-only |
| *inline fields* (`key:: value`) | body text | ❌ not implemented |

### 2.2 DQL — the query language

```
<TABLE|LIST|TASK|CALENDAR> [WITHOUT ID] [<expr> [AS <name>], …]
FROM <source>
[WHERE <expr>] [SORT <expr> [ASC|DESC]] [GROUP BY <expr>] [FLATTEN <expr>] [LIMIT <n>]
```

- **Sources:** `"folder"`, `#tag`, `[[link]]` (pages linking to it), `outgoing([[link]])`, combined with `and` / `or` / `-`.
- **Expressions:** arithmetic, comparison, `contains()`, `length()`, `sort()`, `map()`, `filter()`, `regexmatch()`, `dateformat()`, `choice()`, `default()`, …
- **Value types:** string, number, boolean, **date**, **duration**, **link**, list, object, null. Dates and durations with real arithmetic (`file.day - dur(7 days)`) are a surprisingly large fraction of the total effort.
- **Query types:** `TABLE` (a row per page), `LIST` (bullets), `TASK` (checkboxes, individually toggleable and written back to source), `CALENDAR`.

### 2.3 DataviewJS

`dataviewjs` blocks execute arbitrary JavaScript against the index. **Out of scope for v1** — but note it maps exactly onto `docs/plugin-architecture.md`: the same `window.oxidian` host object could expose `oxidian.dv.pages(...)`. If we ever ship it, it must be opt-in per vault (arbitrary JS from a synced repo is a code-execution vector) and it can never work in a hardened enterprise CSP, which is one of our target environments.

### 2.4 Inline queries

`` `= this.file.name` `` and `` `$= dv.current().rating` `` — single-expression interpolation inside a paragraph. Cheap to add once the expression evaluator exists; worth doing, since it is what makes frontmatter feel alive.

### 2.5 The `ctime`/`mtime` problem

Dataview leans on filesystem timestamps. **We do not have them.** A git host knows *commit* dates, not file mtimes, and getting the last commit touching a path costs one `GET /repos/{o}/{r}/commits?path=…&per_page=1` **per file** — unaffordable at vault scale.

Options, in preference order:

1. **Don't ship `file.mtime` in v1.** Prefer `file.day` (date parsed from the filename, which is how daily notes work anyway) and explicit frontmatter (`created:`, `updated:`).
2. Have Oxidian **stamp `updated:` into frontmatter on save**. We control the write path; this is a few lines and makes the field exact and queryable.
3. On a full local mirror (§7), use the mirror file's own mtime — which is really "when we synced it", so it is honest only as a *local* notion.

This is a genuine semantic difference from Obsidian and belongs in user-facing docs, not just here.

---

## 3. What we already have to build on

The pieces are more assembled than the roadmap's "Large" estimate suggests:

| Piece | Where | Reuse |
|---|---|---|
| Fenced-block detection | `packages/ui/src/cm/markdown_area/tokenizer.rs` — `TokenKind::CodeFence { lang_range }` + `CodeBlock` | The render hook already exists: match `lang_range == "dataview"` |
| Frontmatter split/parse | `packages/app/src/views/properties.rs` — `split_frontmatter`, `parse_pairs` | Needs upgrading to lists/nested (§4.1) |
| Task parsing | `packages/app/src/tasks.rs` — `Task { path, line, checked, text, due, done, priority }` | Becomes `file.tasks`; `TASK` queries get write-back for free |
| Link graph | `packages/app/src/wikilink_index.rs` | `file.inlinks` / `file.outlinks` |
| **SHA-keyed incremental cache** | `packages/app/src/tasks_cache.rs` | **This is the blueprint for the whole index** — see §6.2 |
| Native KV store | `packages/app/src/native_store.rs` | Already routes `ls_get`/`ls_set` to a JSON file on Android/desktop |
| Persistent-storage request | `packages/app/src/js.rs` — `request_persistent_storage()` | Already wired; needed before we rely on browser storage |
| Block-rendered-as-view precedent | `packages/app/src/views/kanban.rs` | Kanban already turns a markdown document into an interactive view and writes edits back |

The gaps are: a **tag index**, an **inline-field parser**, a **real YAML value model**, the **query language**, and the **storage/perf layer**.

---

## 4. Proposed architecture

Five layers, each independently testable. This maps onto `docs/testing.md`'s pyramid — layers 1–4 are pure functions with no I/O and no Dioxus, so they are unit-testable and belong in the coverage numbers.

```
┌─ 5. Render      DataviewBlock component → table / list / task list
├─ 4. Execute     Query + Index → Vec<Row>          (pure)
├─ 3. Parse       DQL text → Query AST              (pure)
├─ 2. Extract     file content → PageData           (pure)
└─ 1. Store       vault content ⇄ local persistence (the hard part, §6/§7)
```

### 4.1 Layer 2 — extraction (`packages/app/src/index/extract.rs`)

One pure function, `extract(path, content, sha) -> PageData`, producing everything §2.1 needs in a single pass: frontmatter values, inline fields, tags, links, headings, tasks, list items.

This forces a **value model** we don't currently have:

```rust
pub enum Value {
    Null, Str(String), Num(f64), Bool(bool),
    Date(NaiveDate), Duration(Days),
    Link(String), List(Vec<Value>), Object(BTreeMap<String, Value>),
}
```

`properties::parse_pairs` returns `Vec<(String, String)>` and cannot represent `tags: [a, b]`. Two choices:

- **Pull in a real YAML parser** (`serde_yaml` / `saphyr`) and map into `Value`. Correct, costs wasm binary size.
- **Extend the hand-rolled parser** to flow lists + `- ` block lists + scalars, and stop there.

Recommendation: hand-rolled, extended. Obsidian frontmatter in practice is scalars, flat lists, and the odd nested map; a full YAML implementation is a lot of wasm for the tail. Revisit if users hit it. Either way `PropertiesPanel` should end up sharing the same parser rather than keeping a second one.

### 4.2 Layer 3 — the parser (`packages/app/src/dataview/parse.rs`)

Hand-written recursive descent, no parser-generator dependency. DQL is small and line-oriented; the only real subtlety is the expression grammar (precedence climbing). Parse errors must be *values*, not panics — a malformed query renders an inline error block, exactly like Dataview does. That is a UX requirement, since the query is being typed live in the editor.

### 4.3 Layer 4 — the evaluator (`packages/app/src/dataview/eval.rs`)

`fn execute(q: &Query, idx: &Index) -> Result<Rows, QueryError>`

Pipeline: resolve `FROM` to a candidate page set (index lookups, never a full scan when a tag/folder/link source is given) → filter `WHERE` → `FLATTEN` → `GROUP BY` → `SORT` → `LIMIT` → project columns.

### 4.4 Layer 5 — rendering

A `DataviewBlock { source: String }` component. The editor's tokenizer already identifies the fence and its language; `MarkdownArea` renders a fenced block today as plain code lines. The hook is: when `lang_range == "dataview"`, render the component instead of the code lines while the block is **not focused**, and fall back to raw text when the cursor enters it — which is precisely the hybrid-WYSIWYG contract the editor already implements for every other construct.

⚠️ Two known constraints from the editor's design (see the contenteditable memory and `component.rs`):
- The editor paints itself via `dangerous_inner_html` and must **not** be re-rendered while focused. A dataview block that re-renders as the index updates will fight this. Simplest correct approach: render dataview output into the surrounding document only on blur / initial load, or host the block in its own child element outside the contenteditable region.
- `TASK` queries need per-checkbox click → write back to *another* file. `tasks.rs` + `tasks_cache::invalidate` already do exactly this for the Tasks panel; reuse that path.

---

## 5. Why performance is the actual design problem

Dataview's model is: the whole vault is in RAM, and every query re-runs on every change. That is affordable when reading a file is a `read()` on a local SSD. Our reads are HTTPS round-trips to GitHub.

**Today's cost of "query the vault" with no index:**

| Vault size | Requests | Wall clock (8 concurrent, ~150 ms each) |
|---|---:|---:|
| 100 notes | 101 | ~2 s |
| 1 000 notes | 1 001 | ~20 s |
| 5 000 notes | 5 001 | ~95 s, and **at GitHub's 5 000 req/hr authenticated limit** |

And that is *per query block*, on *every* page open, unless we cache. Rate limits that matter:

- GitHub REST, authenticated: **~5 000 requests/hour**; **search/code: ~30/minute** (so search can't be the query backend either).
- GitHub Contents API returns base64 and **caps at ~1 MB** per file (bigger needs the blobs API) — fine for notes, but our `read_file` has no fallback today.
- `git/trees?recursive=1` returns the whole tree in **one request** with a **blob SHA per file** — this is the single most valuable thing the API gives us, and `list_files` already calls it. Caveat: it can come back `truncated: true` (~100 k entries / 7 MB) and **`tree_to_files` currently ignores that flag** — a silent correctness bug at large vault sizes that dataview would make visible.
- GitLab: `/projects/:id/repository/tree` (paginated) and a comparable per-user rate limit.

**Conclusion: queries must never touch the network.** They run against a local index. The network is only used to *refresh* that index, and refreshing must cost O(changed files), not O(vault).

---

## 6. Storage model

### 6.1 Two things worth persisting, and they are different

1. **The derived index** — parsed `PageData` per note. Small, regenerable, and the only thing queries actually read.
2. **The note content mirror** — the actual markdown. Large. Needed for full-text search, offline editing, and the Obsidian-like model.

They have different size profiles and different urgency. **The index alone unblocks dataview**; the mirror is the bigger "Obsidian model" prize (§7). Ship them in that order.

Index size estimate: a typical note yields ~0.5–2 KB of JSON `PageData` (frontmatter + tags + links + tasks + headings).

| Vault | Index (JSON) | Index (compact/binary) | Full content mirror |
|---|---:|---:|---:|
| 1 000 notes | ~1–2 MB | ~0.5 MB | ~5 MB |
| 10 000 notes | ~10–20 MB | ~5 MB | ~50 MB |

That table decides the storage backend on its own: **`localStorage` is out.**

### 6.2 The refresh algorithm (already prototyped in `tasks_cache.rs`)

`tasks_cache` is the correct design, applied to one facet. Generalise it:

1. `list_files` → one tree request → `(path, blob_sha)` for the whole vault.
2. Diff against the stored index: **stale = new paths + paths whose blob SHA changed**; evict paths that vanished.
3. Read only stale files (bounded concurrency, currently chunks of 8).
4. `extract()` each, store, bump an index generation counter.

Steady-state cost of opening a vault: **1 request**. Cost after editing 3 notes elsewhere: **4 requests**. This is the whole ballgame, and it works because git blob SHAs are content hashes — an unchanged file is *provably* unchanged, so a stale index entry is impossible rather than merely unlikely.

Recommendation: **promote `tasks_cache.rs` into `packages/app/src/index/`**, with tasks as one facet of `PageData`, so the Tasks panel, the graph, backlinks, a future tags pane, and dataview all share one index and one refresh. Right now the Tasks view, the wikilink index, and the file tree each maintain their own partial view of the vault.

### 6.3 Web — what's actually available

| Mechanism | Capacity | Fit |
|---|---|---|
| `localStorage` | ~5 MB, synchronous, strings | **Config and small keys only** — which is now all it holds. |
| **IndexedDB** | Quota-based, typically 60 % of free disk on Chrome/Firefox | **The default choice** for both index and mirror. Async, structured data, indexable keys. |
| **OPFS** (Origin Private File System) | Same quota, file-shaped | Best fit for a *content mirror* — real files, sync access handles inside a worker, cheap streaming writes. |
| **Cache Storage** | Same quota | Interesting for content: our keys are **blob SHAs**, i.e. content-addressed and immutable, which is exactly what a `Cache` is good at. |
| Storage Buckets | Same quota, per-bucket eviction/persistence policy | Chrome-only today. Nice-to-have, not a foundation. |

**Eviction is the risk, not capacity.** Browser storage is "best-effort" by default and may be cleared under disk pressure; Safari caps script-writable storage for sites without user engagement at ~7 days. Mitigations:

- Call **`navigator.storage.persist()`** — `js::request_persistent_storage()` already exists; make sure it runs before we depend on the index.
- Check `navigator.storage.estimate()` and surface remaining quota in Settings.
- **Treat all local state as a cache, never as the source of truth.** The repo is the truth. A wiped index is a slow first load, never data loss. This property is non-negotiable and it is what makes the whole scheme safe.

Enterprise note: IndexedDB/OPFS are ordinary web platform APIs, not extensions or file-system permissions — the web build keeps working in a locked-down enterprise browser. (Group policy *can* clear site data on exit; see the point above about degrading to a slow load.)

### 6.4 Android

We already write real files: `native_store.rs` gets `Context.getFilesDir()` over JNI and stores JSON there, precisely because the WebView's `localStorage` does not survive cold restarts. That mechanism generalises directly:

- **App-private internal storage** (`getFilesDir()`) — no runtime permission needed, survives until uninstall/clear-data. This is where the mirror and index belong. Scoped storage does not restrict it.
- Don't put the vault on shared external storage unless the user explicitly asks: that needs SAF (`ACTION_OPEN_DOCUMENT_TREE`) and a content-URI-based I/O path, which is a whole separate backend.
- Storage on a phone is finite: a mirror should be opt-in with a size estimate, and evictable per-folder.
- The token is still stored in plaintext there (roadmap has Keystore encryption as an open item) — a full content mirror raises the stakes on that, since we'd then be storing the entire vault at rest unencrypted.

### 6.5 Desktop

`dirs::data_dir()/oxidian` via the same `native_store` path, or — the roadmap's parked **M4 local-git backend** (`git2-rs`). Dataview is a strong argument for un-parking M4 *eventually*: a real clone gives content, history, mtimes, and offline for free, and turns "sync" into `git pull`. But desktop is explicitly not a focus, and **dataview must not depend on it**, since web and Android are the priority targets.

### 6.6 Recommended split

| Platform | Index | Content mirror |
|---|---|---|
| Web | IndexedDB | OPFS (phase 2, opt-in) |
| Android | JSON/binary file in `getFilesDir()` | Same directory, one file per note (phase 2) |
| Desktop | Same as Android via `dirs::data_dir()` | Local git clone if/when M4 lands |

One trait, three impls, e.g. `trait BlobStore { get(sha), put(sha, bytes), … }` behind `#[cfg]` — matching how `js::ls_get` already forks between web and native.

---

### 6.7 Where the bytes actually go

Two stores, split by whether the data scales with the vault:

| | Web | Native |
|---|---|---|
| Config, bookmarks, board (`js::ls_*`) | `localStorage` | `native_store`'s JSON map |
| Vault index (`js::blob_*`) | IndexedDB (`oxidian`/`blobs`) | its own file in the app data dir |

The index gets a file of its own on native rather than a key in the settings
map for the same reason it gets IndexedDB on web: it is rewritten on every save,
and folding it into the settings blob would re-encode and rewrite the token and
every setting each time — with JSON-inside-JSON escaping roughly doubling it.

Failure is always tolerable. `blob_set` swallows quota errors, `load` falls back
to an empty index, and a wiped store costs exactly one slow refresh, because the
repo is the source of truth. That is what makes **Rebuild** safe to offer as a
one-click action.

What the footer shows is worth having because both numbers are real limits:
`navigator.storage.estimate()` reports the quota the browser is actually giving
this origin, and `persisted()` says whether the browser may evict it under
pressure. Native reports its own footprint and no quota.

## 7. Getting to the Obsidian model (full local vault)

The end state you described — "vault fully downloaded to the device" — needs a **bulk seed**, because 5 000 sequential `read_file` calls is not a viable first-run experience.

- **GitHub:** `GET /repos/{owner}/{repo}/tarball/{ref}` (or `/zipball/`) — the entire vault in **one request**.
- **GitLab:** `GET /projects/:id/repository/archive.tar.gz?sha={ref}` — likewise.

Then incremental refresh via the tree-SHA diff of §6.2, so the archive is downloaded once, ever.

Two things to verify before committing to this (both are genuine unknowns, not formalities):

1. **CORS from the browser.** The tarball endpoint 302-redirects to `codeload.github.com`; whether that host serves CORS headers for an authenticated XHR needs testing. If it doesn't, the web build falls back to progressive per-file seeding (which is fine — web is also where a mirror matters least, since the index alone is enough for queries), while Android/desktop use the archive freely.
2. **Decompression in wasm.** `flate2` + `tar` compile to wasm; the browser also has a native `DecompressionStream('gzip')` we can reach through `oxidian.js`. Prefer the native one on web to keep the bundle small.

Sync then becomes: seed once → tree-diff on open → read changed blobs → write-through on save (we already SHA-check writes for conflicts). This is a "git client without git", which is roughly what the mobile Obsidian-git workflow is anyway.

---

## 8. Keeping queries fast once the data is local

- **Memoize per block** on `(query_text, index_generation)`. Re-running a query on every keystroke in an unrelated note is Dataview's original sin and datacore's reason for existing.
- **Index by what `FROM` selects on**: `tag → Vec<page_id>`, `folder → Vec<page_id>`, `link → Vec<page_id>`. Then a `FROM #games` query touches the pages it needs, not all 10 000.
- **Push refresh, don't poll.** One `Signal<u64>` generation counter; blocks subscribe. A note that saves re-extracts exactly one page and bumps the counter.
- **Keep extraction off the UI thread on first build.** Extracting 10 000 notes will jank the frame. Either chunk with yields between batches, or move it to a Web Worker on web (a second wasm instance is real work — chunking first, worker only if measured as necessary).
- **Measure before optimising.** A `WHERE` over 10 000 in-memory pages is single-digit milliseconds in Rust; the cost is almost entirely in extraction and I/O, not evaluation. Rust gives us a lot of headroom that Dataview (JS) never had — that is our structural advantage over the original, and worth not squandering on an over-engineered query planner.

---

### 8.1 Folding without corrupting the document

The editor's text model is the DOM: `read_state` rebuilds the note by walking
the editor's line divs. Rendered output lives *in* that DOM, so it has to be
invisible to the model or the first keystroke after a query block would write
the rendered table into the note.

Three rules make that hold:

- **The source never leaves the DOM.** A folded block still emits one
  `.md-line` per source line, with the same byte offsets; only CSS hides them
  (`.md-render-hidden`, and `.md-render-host > .md-token` for the line that
  carries the output). Caret offsets are therefore identical folded or not,
  which is why unfolding can hand the same offset straight back.
- **Output is marked `[data-md-render]`**, and every text/caret calculation in
  `markdown_area.js` goes through `visibleText`, which skips those subtrees.
  Nothing else may use `textContent`.
- **Clicking output is a request, not an edit.** It records an offset that the
  Rust side re-renders around, placing the caret in the source.

Fold state is decided in Rust from the caret offset (`rendered_blocks`), not in
CSS, so it is unit-testable and doesn't depend on `:has()` support.

Clicking *inside* output is its own path again: an element with `data-action`
sends its payload to the host and never moves the caret. Two things that look
like details are load-bearing, both found by the e2e test:

- The mousedown must `preventDefault`, or the caret moves into the block and the
  resulting re-render detaches the node mid-click.
- The optimistic flip must edit the checkbox's existing text node, not assign
  `textContent`. Replacing the node under the pointer between mousedown and
  mouseup makes the browser skip the `click` event altogether, so the action
  silently never fires.

One related bug fixed on the way: `read_state` was a single destructive read
shared by the click and input handlers, and a click arrives alongside a
selectionchange — whichever handler ran first ate the other's payload. Clicks
now read `read_click`, a separate channel.

## 9. Proposed sequencing

Each phase is independently useful and shippable.

| Phase | Deliverable | Depends on |
|---|---|---|
| **0** ✅ | `tasks_cache` → the `packages/index` crate: `PageData`, extraction, SHA-diff refresh; Tasks panel migrated (`app::vault_index`). **No user-visible change**, no dataview yet. | — |
| **1** ✅ | Index off `localStorage`: `js::blob_*` — IndexedDB on web, its own file under `native_store` on native — with a one-time migration of an index left behind by an older build. A storage footer in the sidebar reports notes indexed, bytes used against quota, whether storage is evictable, and offers **Rebuild**. See §6.7. | 0 |
| **2** ✅ | Extraction completeness: typed `Value` model with real dates and durations (`value.rs`), inline `key:: value` fields in line and bracketed forms, `#tag` index (`Index::tag_counts` / `pages_with_tag` — the **Tags pane**, US 14, is now mostly a UI job), headings, links. | 0 |
| **3** ✅ | DQL lexer, parser, and evaluator (`dql/`), pure and unit-tested. `TABLE [WITHOUT ID]` / `LIST` / `TASK`, `FROM` (folder, tag, link, `outgoing()`, `and`/`or`/`-`), `WHERE`, `SORT`, `LIMIT`, ~20 built-in functions. Errors are values. **Deferred:** `GROUP BY`, `FLATTEN`, `CALENDAR` — each parses to a clear "not supported yet" message rather than a confusing syntax error. | 2 |
| **4** ✅ | Rendering. `dql::render` turns a result into escaped HTML (table / list / task list, error block, empty state) and `dql::run` is the one call the UI makes. In the editor, `MarkdownArea` takes a `BlockRenderer` callback (the `ui` crate can't depend on `index`, so `app` supplies it) and shows a claimed fence as output, swapping back to source when the caret enters it — or when you click the output. **The first shippable dataview.** See §8.1 for how folding keeps the document model intact. | 3 |
| **5** ✅ | `TASK` queries with click-to-toggle write-back. Rendered checkboxes carry a `data-action`; the editor forwards it to the host via `on_block_action`, which resolves the task **from the index** (never from DOM text) and writes through the same `write_task_toggle` the Tasks panel uses. | 4 |
| **6** | Inline `` `= expr` `` queries. | 3 |
| **7** | Full content mirror + archive seeding (§7); offline vault; full-text search stops using the search API. | 1 |
| **8** | *(Maybe, maybe never)* `dataviewjs` via the plugin host API — opt-in per vault only. | plugin architecture |

Phases 0–2 are the ones that pay for themselves regardless of whether dataview ever ships: they make the Tasks view, graph, backlinks, and the tags pane share one coherent, incrementally-refreshed index instead of three partial ones.

---

## 10. Decisions

1. **Syntax:** match DQL closely for `TABLE` / `LIST` / `TASK`. Skip `CALENDAR` and DataviewJS. Compatibility means existing vaults just work and gives us a free spec plus a free test corpus.
2. **`file.mtime`:** ✅ **implemented** — Oxidian stamps `updated: YYYY-MM-DD` into frontmatter on save (`views/vault.rs::stamped_for_save`, `index::frontmatter::set_key`). At most one rewrite per note per day, and the stamped text is only adopted back into the editor if the user hasn't typed since the snapshot was taken. Note the side effect: **editing a note that has no frontmatter now gives it one.** If that turns out to be unwelcome, the natural knob is a `stamp_updated: bool` in `GithubConfig` defaulting to on.
3. **Crate layout:** ✅ **done** — `packages/index`, a standalone crate depending only on `vault` + serde. No Dioxus, no I/O, so every layer is testable without a renderer or a network.
4. **Mirror opt-in or default:** deferred to phase 7 (§11 has the elaboration and the encryption analysis).
5. **`truncated: true`:** ✅ **fixed** — `github::list_files` now detects the flag and falls back to a breadth-first per-directory walk, so a large vault gets a complete listing instead of a silently short one.

---

## 11. The mirror question, and encryption at rest

**What "opt-in or default" actually means.** Phase 7 stores a copy of every note on the device. Downsides of doing that unconditionally: a 10 k-note vault is ~50 MB on a phone that may be short on space; the first sync is a large download on possibly-metered mobile data; and the entire vault then sits at rest on the device, whereas today only the token and a derived index do. Default-on is the Obsidian feel (open the app on a plane, everything is there). Default-off is the conservative option and costs you offline access. This does not need deciding until phase 7 — phases 0–6 store only the derived index.

**Can we just encrypt it?** Yes, and it is not much work — but it is worth being precise about what it buys, because encryption at rest is only as good as the key storage underneath it.

| Platform | Where a key can live | What that actually protects against |
|---|---|---|
| **Android** | **Keystore** — hardware-backed (TEE/StrongBox), key material never leaves secure hardware | Offline extraction: a stolen/rooted device dump, ADB backup, malware reading app storage. Not: malware running *as our app*. |
| **Desktop** | OS keyring (`libsecret` / Keychain / DPAPI) | Another user account on the same machine, backups, sync-folder leakage. |
| **Web** | ⚠️ **nowhere good.** A non-extractable `CryptoKey` can be stored *in* IndexedDB via WebCrypto, so JS never sees the raw bytes — but any script in our origin can still *use* it, and it is deleted with site data. | Casual disk inspection and other origins (which same-origin policy already blocks). **Not** XSS, and not a determined local attacker. |

So: on **Android and desktop, encrypting the mirror is genuinely worth it** — the Keystore/keyring makes it real, and it composes with the roadmap's existing "encrypt token in Android Keystore" item. Do both with one key. On **web it is close to security theatre**, and the honest mitigation there is different: keep the mirror opt-in on web, or skip it entirely (the derived index is enough for queries, which is exactly the §6.1 split).

Recommended shape when phase 7 lands:

- Content encrypted with AES-256-GCM; per-vault key.
- Key in Android Keystore / OS keyring; on web, a non-extractable WebCrypto key in IndexedDB, clearly documented as weaker.
- **Encrypt the mirror and the index together** — the index leaks a great deal on its own (every note title, every tag, every task, the whole link graph). Encrypting the content but not the index would be a false sense of security.
- Keep it a *cache*: losing the key is never data loss, because the repo is the truth. That is what makes the whole scheme low-risk — a key that can be thrown away and regenerated is a very different engineering problem from one that must never be lost.

Corollary worth stating plainly: **the token is the crown jewel, not the notes.** It grants write access to the whole repo, and it is stored in plaintext today (`native_store.rs`). If encryption effort is limited, spend it there first.

## 12. Search, which the index turned out to also solve

Search was a separate feature built on GitHub's code-search API. It could not
work in the browser: `api.github.com/search/code` sends no
`Access-Control-Allow-Origin` header, so the request is blocked before it is
sent, and the web build is the priority target. It was also rate-limited to 30
requests a minute, needed a round trip per keystroke, had no GitLab equivalent,
and returned nothing offline.

The index already holds everything search needs except the prose, and the
refresh that builds it **already downloads every changed note in full** — so
keeping the body (`PageData::text`) costs storage but not a single extra
request. `index::search` is a pure pass over that:

- All whitespace-separated terms must match (AND), each a case-insensitive
  substring. Substring, not whole-word, so results narrow while you type.
- Terms may match across title, headings, tags, or body.
- Ranked by where the *first* term hit — title, then heading, then tag, then
  body — then by how many body lines matched, then by path so ties are stable.
- Frontmatter is not body text, or every note with a `tags:` key would match
  "tags".

No debounce, no spinner, no error state: it is a synchronous pass over data
already in memory, so there is nothing to wait for and nothing to fail. The one
honest caveat is that search covers what the index holds — a note it hasn't
picked up yet won't appear. The storage footer's note count is the user's read
on that, and Rebuild is the fix.

**Cost.** The index now scales with the size of the vault's prose rather than
its metadata: roughly vault-sized, so a 5 000-note vault lands in the low tens
of MB. That is why it is stored per note rather than as one blob — see §13.

`FORMAT_VERSION` went to 2 for this. An index written by an older build has no
text and is discarded rather than left half-usable; the next refresh rebuilds it.


## 13. Per-note records

Storing the index as a single JSON blob made every save O(vault): read the whole
blob, parse it, change one note, re-serialise, write it back. Tolerable when the
index held only metadata; not once it carries every note's text for search.

So the store is one record per note — IndexedDB's `pages` object store on web, a
file each under `pages/` on native — and the live index is a `Signal` that the
whole app shares rather than something reloaded per operation. A save now reads
nothing and writes one record.

`Index::apply` reports what it changed (`Changed { updated, removed }`) so a
refresh persists exactly the notes that moved and deletes the records of notes
that left the vault, instead of rewriting everything.

Details worth keeping:

- **Every store call is one batched transaction.** Seeding a vault is thousands
  of records; a transaction per record is pathologically slow.
- **Native filenames are percent-encoded, not sanitised.** Mapping awkward
  characters to `_` would put `a/b.md` and `a_b.md` in the same file, and one
  note would silently overwrite the other's record.
- **A record that won't parse is dropped, not fatal.** One bad record costs one
  note re-read; discarding the whole index costs a full re-download.
- **An absent version stamp is not a version mismatch.** It means a build that
  predates the stamp, whose blob we want to *adopt* — splitting it into records
  costs zero note reads, where starting cold re-downloads the vault.

### The bridge bug this uncovered

`use_js!` emits an `await` only for JS functions declared `async`. A plain
`function` that returns a Promise hands the *Promise* across the bridge, where
it fails to deserialise and the Rust side quietly gets a default value.

Every IndexedDB helper added in phase 1 was written that way. Writes appeared to
work (the JS still ran), but **every read returned empty**, so the stored index
was never actually loaded and each boot silently rebuilt it from the network.
Nothing failed; it was just slow, and the phase-1 tests missed it because they
inspected IndexedDB directly and a refresh repopulates it either way.

The regression test is the one that pins the actual promise: **a reload must
perform zero note reads.** Any future read that silently returns nothing fails
there. `oxidian.js` carries a warning at the IndexedDB section, and the rule is
simply: a promise-returning export must be `async`.
