# Hybrid Render — Design Plan

## The goal

The user's mental model, stated precisely:

> The markdown source lives in Rust as the source of truth.  
> We render it to editable HTML.  
> When the cursor enters a markdown "subnode" — the span between `**` markers, the text after `# ` on a heading line, the content of a `[[wikilink]]` — that subnode's raw markdown becomes visible and editable in place.  
> Everywhere else the markdown is invisible; only the formatted output is shown.

This is how Obsidian, Typora, and iA Writer work. It is **per-token, cursor-driven** mode switching.

---

## What we do now

We use a **styled-raw** approach:

- All markdown markers (`**`, `*`, `##`, `[[`, etc.) are always present in the DOM as `<span class="md-marker">` elements styled at ~35% opacity.
- The entire source is always visible; formatting is purely cosmetic (bold text is bold, headings are large, but the `**` and `##` are just faded).
- `dangerous_inner_html` replaces the child DOM on every content change, bypassing Dioxus's VDOM reconciliation.
- `el.innerText` always equals the raw markdown source, so reading it back on `oninput` is safe and lossless.

**What works well:** content integrity (source is never corrupted by hidden markers), simplicity (no cursor-position-dependent rendering), reliable editing.

**What is missing:** the Obsidian experience. The markers are always visible; the user asked for them to vanish except when the cursor is inside that token.

---

## Why the obvious fix is hard

The natural approach — render tokens as clean HTML (hiding markers), and switch a token to raw when the cursor enters it — has two compounding problems.

### Problem 1 — Content corruption on input

When a token shows only its *display* text (e.g. `<strong>bold</strong>` with the `**` stripped from the DOM), `el.innerText` returns `bold`, not `**bold**`. If we then update the source from `innerText`, the markdown markers are silently deleted. This happens on every keystroke.

The fix is to **never derive the full source from `el.innerText`**. Instead, only the *active token* (showing raw text) can be read back from the DOM; all other tokens must be kept in Rust and not touched.

### Problem 2 — VDOM reconciliation vs. browser editing

When the cursor moves and we flip a token from formatted (`<strong>bold</strong>`) to raw (`<span>**bold**</span>`), Dioxus replaces those DOM nodes. The browser cursor is destroyed. We must restore it with a JS call after every such transition.

More critically: between a cursor event firing and the JS restore running, there is an async gap. Dioxus's re-render happens synchronously but the JS cursor restore is a microtask. During that gap the user may have already typed another character. The sequence

```
cursor moves → re-render (cursor lost) → type → oninput reads wrong DOM → JS restore fires
```

produces subtle but real corruption.

### Problem 3 — Source ↔ DOM offset mapping

When a token is rendered as formatted HTML (markers hidden), cursor offset in the DOM does not equal cursor offset in the source. A cursor at display position 4 in `bold` maps to source position 6 in `**bold**`. Every cursor read from the DOM must be translated through an offset map. That map must be rebuilt on every render, and getting it wrong means the cursor teleports.

---

## Proposed approach — Line-level mode switching

The insight that makes this tractable: **do the mode switch at line granularity, not token granularity**.

- Each source line is one rendering unit.
- The line the cursor is currently on is rendered as **raw text** — a plain `<span>` or `<div>` containing exactly the source characters, no HTML markup, no hidden markers.
- Every other line is rendered as **formatted HTML** — markers hidden, styling applied.

### Why this is easier than token-level switching

1. **No offset map needed for the active line.** The active line shows raw text, so `cursor_dom_offset = cursor_source_offset` within that line. Reading it from the DOM is safe.

2. **Line boundaries are stable.** When the cursor moves from line A to line B, we freeze line A (save its raw text back to the source, render it as formatted) and thaw line B (render it as raw). There is no async gap between "cursor moves" and "DOM switches" because the switch only happens *after* we read the cursor position.

3. **`innerText` is only read for the active line.** We never read the full `el.innerText`. We query only the active line's element for its text content. This avoids the content-corruption problem entirely.

4. **Formatted lines are read-only from Dioxus's perspective.** The user can only type in the raw (active) line. Formatted lines have `contenteditable="false"` on their containers, so the browser cursor cannot enter them at all — it can only pass through them via arrow keys, which triggers a line switch.

---

## Data model

```
source: Signal<String>          // full markdown, the source of truth
active_line: Signal<Option<usize>>  // index into the source's newline-split lines
```

On every cursor event (click, arrow key, `selectionchange`):
1. Read the active DOM line element's text → save it back into `source` at the right line index.
2. Compute the new active line from the cursor's position in the DOM.
3. If the active line changed, update `active_line` signal → triggers re-render.

On re-render:
- Split `source` into lines.
- For each line, emit either raw-line HTML or formatted-line HTML depending on `active_line`.
- Use `dangerous_inner_html` on the outer editor div (avoids VDOM reconciliation).
- After render, restore cursor to the correct character offset within the raw active line (simple, because source offset = DOM offset there).

---

## DOM structure

```html
<div id="md-area-0" class="md-area" contenteditable="true">
  <!-- formatted line (contenteditable="false" child) -->
  <div class="md-line md-line--rendered" contenteditable="false">
    <strong class="md-bold">bold</strong> and plain text
  </div>
  <!-- active line (plain text, editable) -->
  <div class="md-line md-line--raw">
    **bold** and plain text
  </div>
  <!-- another formatted line -->
  <div class="md-line md-line--rendered" contenteditable="false">
    ...
  </div>
</div>
```

Key choices:
- One `<div>` per source line. This gives the browser a clean model for Enter (new div) and Backspace-at-start (merge divs).
- Formatted lines are `contenteditable="false"` so the cursor skips over them directly from one raw line to the next — the browser treats them as single characters. Clicking a formatted line activates it.
- The outer div is `contenteditable="true"`. Dioxus only ever sets `dangerous_inner_html` on it; it never reconciles children.

---

## Cursor tracking

Replace the current async `js_read_state` polling with a `selectionchange` document listener set up once on mount:

```js
document.addEventListener('selectionchange', function() {
    const el = document.getElementById("md-area-0");
    if (!el || !el.contains(document.getSelection().anchorNode)) return;
    // Find which .md-line the cursor is in, and its char offset
    // Send via dioxus.send() to a use_coroutine
});
```

`selectionchange` fires synchronously after every cursor move (click, keyboard, drag). Using it instead of `onclick` + `onkeyup` eliminates the polling gap that currently causes missed transitions.

In Rust, a `use_coroutine` receives `(line_index: usize, char_offset: usize)` messages and updates `active_line`.

---

## Edit flow

```
User types 'x' on the active line
  ↓
browser inserts 'x' into the raw-line div (DOM mutation)
  ↓
oninput fires
  ↓
JS reads the active line div's innerText (raw markdown, safe)
  ↓
Rust: replace line[active_line] in source with the new text
  ↓
active_line unchanged → only re-render this one line (optional optimisation)
  ↓
cursor_pos unchanged → no cursor restoration needed
```

Because we only read the *active* line's innerText, and that line is always raw, content is never corrupted.

### Enter key (line split)

`onkeydown` intercepts Enter:
1. `preventDefault()` (stop browser from inserting a `<div>`)
2. JS reads cursor offset in active line
3. Rust splits `source[active_line]` at that offset into two lines
4. `active_line` advances by one
5. Re-render → cursor lands at start of new active line

### Backspace at line start (line merge)

`onkeydown` intercepts Backspace when cursor is at offset 0:
1. `preventDefault()`
2. Rust merges `source[active_line - 1]` and `source[active_line]`
3. `active_line` decreases by one; cursor offset = old length of merged-into line
4. Re-render → cursor restored

---

## Implementation steps

| # | What | Where |
|---|------|--------|
| 1 | Add `active_line: Signal<Option<usize>>` to component | `component.rs` |
| 2 | Split rendering into `render_line_raw(line)` and `render_line_formatted(line, tokens)` | `component.rs` |
| 3 | Build `lines_to_html(source, active_line) -> String` and wire to `dangerous_inner_html` | `component.rs` |
| 4 | Set up `selectionchange` listener in `use_effect` on mount; pipe events into a `use_coroutine` | `component.rs` |
| 5 | On active-line change: read old active line's text from DOM, update `source`, set `active_line` | `component.rs` |
| 6 | Handle Enter / Backspace-at-0 in `onkeydown` to split/merge lines | `component.rs` |
| 7 | Add CSS for `.md-line--raw` (monospace, slight background tint) and `.md-line--rendered` | `style.css` |
| 8 | Remove the styled-raw marker approach (no longer needed) | tokenizer + component |
| 9 | Update task-checkbox toggle to work with the new line-based rendering | `component.rs` |

Steps 1–4 can be done without removing the current styled-raw rendering (feature-flag the switch) so there is always a working fallback.

---

## What stays the same

- The `tokenizer.rs` is unchanged — it still produces the same token tree from a source line.
- `dangerous_inner_html` stays; the line-div approach just generates different HTML.
- The task-checkbox toggle mechanism (mousedown capture → `cb:` payload) is unchanged.
- The `Token` / `TokenKind` types are unchanged.

---

## Open questions

- **`selectionchange` + Dioxus coroutine**: Dioxus 0.7's `use_coroutine` accepts messages sent from Rust code via a channel handle. Bridging JS → Rust requires either the `dioxus.send()` eval mechanism (one-shot, not a persistent listener) or `wasm-bindgen` closures. The cleanest solution is a `wasm-bindgen` `Closure` stored in a `use_signal` so it lives as long as the component. This requires adding `wasm-bindgen` and `web-sys` to `packages/ui`'s dependencies.

- **`contenteditable="false"` and arrow-key cursor skip**: browsers treat `contenteditable="false"` inline elements as single characters for cursor movement. Block-level (`display: block`) `contenteditable="false"` divs may behave differently across browsers — needs testing in Chrome, Firefox, and Safari.

- **Paste**: multi-line paste needs its own `onpaste` handler that reads `event.clipboardData.getData('text/plain')`, splits by newline, inserts into source, and prevents default.
