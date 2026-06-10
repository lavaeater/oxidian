// DOM glue for the `MarkdownArea` inline-markdown editor.
//
// Each exported function is bound into Rust via `dioxus_use_js::use_js!`
// (see component.rs), becoming an async `fn` that returns `Result<_, JsError>`.
// Arguments arrive already deserialized from Rust, so — unlike the old
// `format!`-built eval strings — there is no manual escaping to get wrong.

// Sets up mousedown capture for task-checkbox clicks and navigate clicks.
export function setup_tasks(id) {
    const el = document.getElementById(id);
    if (!el || el.dataset.taskSetup) return;
    el.dataset.taskSetup = '1';
    el.addEventListener('mousedown', function (e) {
        const cb = e.target.closest('.md-task-checkbox');
        if (cb) {
            el._taskClick = {
                pos: parseInt(cb.dataset.pos),
                checked: cb.dataset.checked === 'true'
            };
            return;
        }
        const nav = e.target.closest('[data-navigate]');
        if (nav) {
            el._navClick = nav.dataset.navigate;
        }
    }, true);
}

// Sets up a selectionchange listener that marks the active line div so CSS
// can show its markers. Simpler than per-token tracking.
export function setup_selection(id) {
    const el = document.getElementById(id);
    if (!el || el.dataset.selSetup) return;
    el.dataset.selSetup = '1';
    document.addEventListener('selectionchange', function () {
        const prev = el.querySelector('.md-line--active');
        const sel = window.getSelection();
        let next = null;
        if (sel && sel.rangeCount > 0 && el.contains(sel.anchorNode)) {
            let cur = sel.anchorNode;
            if (cur.nodeType !== 1) cur = cur.parentElement;
            while (cur && cur !== el) {
                if (cur.classList && cur.classList.contains('md-line')) {
                    next = cur;
                    break;
                }
                cur = cur.parentElement;
            }
        }
        if (prev !== next) {
            if (prev) {
                // Sync data-checked from actual text before the line goes inactive.
                const cb = prev.querySelector('.md-task-checkbox');
                if (cb) {
                    const t = cb.textContent;
                    cb.dataset.checked = (t.startsWith('[x]') || t.startsWith('[X]')) ? 'true' : 'false';
                }
                prev.classList.remove('md-line--active');
                // Skip if we're mid-render (innerHTML was just set by us).
                if (!el.dataset.rendering) {
                    el.dataset.lineChange = '1';
                    el.dispatchEvent(new Event('input', { bubbles: true }));
                }
            }
            if (next) next.classList.add('md-line--active');
        }
    });
}

// Handles Enter inside the editor entirely in the text model:
//   • list/task line with content → continue the list (newline + next marker)
//   • empty list/task line         → exit the list (remove the marker, no new line)
//   • any other line               → plain newline
//
// We `preventDefault` and never use `execCommand`/the browser's default Enter,
// because those insert a `<br>` for the line break — and `<br>` is invisible to
// `textContent` (what `lineTextAndCursor` reads), so the newline would be lost
// and the next line would collapse onto the previous one. Instead we compute the
// new full text + caret offset ourselves and hand them to the Rust side via
// `_pendingText`/`_pendingCursor` (read by `read_state`), which re-tokenises and
// rebuilds one clean `.md-line` div per line with the caret restored. Keeping
// `<br>` out of the DOM is also what lets the caret/line math stay correct.
export function setup_keyboard(id) {
    const el = document.getElementById(id);
    if (!el || el.dataset.kbSetup) return;
    el.dataset.kbSetup = '1';

    el.addEventListener('keydown', function (e) {
        // ctrl/meta+Enter may be a shortcut elsewhere; IME Enter confirms a
        // composition rather than inserting a line.
        if (e.key !== 'Enter' || e.ctrlKey || e.metaKey || e.isComposing) return;
        const [text, cursor] = lineTextAndCursor(el);
        if (cursor < 0) return; // no caret in the editor — let the browser handle it

        const lineStart = text.lastIndexOf('\n', cursor - 1) + 1;
        let lineEnd = text.indexOf('\n', cursor);
        if (lineEnd < 0) lineEnd = text.length;
        const line = text.slice(lineStart, lineEnd);

        // Detect a list/task marker on the current line (Shift+Enter = plain
        // newline, so it never continues a list).
        let marker = null;   // marker to start the continued item with
        let markerLen = 0;   // length of this line's existing marker
        if (!e.shiftKey) {
            const taskM = line.match(/^(\s*[-*+] )\[[ xX]\] /);
            if (taskM) { markerLen = taskM[0].length; marker = taskM[1] + '[ ] '; }
            else {
                const olM = line.match(/^(\s*)(\d+)\. /);
                if (olM) { markerLen = olM[0].length; marker = olM[1] + (parseInt(olM[2]) + 1) + '. '; }
                else {
                    const ulM = line.match(/^(\s*)([-*+]) /);
                    if (ulM) { markerLen = ulM[0].length; marker = ulM[1] + ulM[2] + ' '; }
                }
            }
        }

        let newText, newCursor;
        if (marker && line.slice(markerLen).trim() === '') {
            // Empty item → exit the list: drop the marker, no new line.
            newText = text.slice(0, lineStart) + text.slice(lineStart + markerLen);
            newCursor = lineStart;
        } else if (marker) {
            // Continue the list: newline + next marker at the caret.
            const ins = '\n' + marker;
            newText = text.slice(0, cursor) + ins + text.slice(cursor);
            newCursor = cursor + ins.length;
        } else {
            // Plain newline (covers Shift+Enter and non-list lines).
            newText = text.slice(0, cursor) + '\n' + text.slice(cursor);
            newCursor = cursor + 1;
        }

        e.preventDefault();
        el._pendingText = newText;
        el._pendingCursor = newCursor;
        el.dataset.lineChange = '1';
        el.dispatchEvent(new Event('input', { bubbles: true }));
    });
}

// Reads the editor text and caret offset together, in a *line-deterministic*
// space: each top-level child of the editor is one line, and lines are joined
// with exactly one '\n'. This is the crucial difference from `innerText`, whose
// trailing/empty-line newlines are unreliable in the Android WebView: it lets
// the caret offset distinguish "end of line N" from "start of empty line N+1"
// (they differ by the line-break char), so empty/blank lines get a real offset
// instead of -1 — which is what makes leaving a block re-render on mobile.
//
// Returns [text, cursor]; cursor is -1 only when there is no caret in the editor.
function lineTextAndCursor(el) {
    const sel = window.getSelection();
    const range = (sel && sel.rangeCount > 0 && el.contains(sel.anchorNode))
        ? sel.getRangeAt(0) : null;
    let text = '';
    let cursor = -1;
    const kids = el.childNodes;
    for (let i = 0; i < kids.length; i++) {
        if (i > 0) text += '\n';
        const kid = kids[i];
        if (range && cursor < 0 &&
            (kid === range.startContainer ||
                (kid.nodeType === 1 && kid.contains(range.startContainer)))) {
            const pre = range.cloneRange();
            pre.selectNodeContents(kid);
            try { pre.setEnd(range.startContainer, range.startOffset); } catch (_) { }
            cursor = text.length + pre.toString().length;
        }
        text += (kid.textContent || '');
    }
    // Caret sitting directly on the editor element, between line nodes.
    if (range && cursor < 0 && range.startContainer === el) {
        let t = '';
        for (let i = 0; i < range.startOffset && i < kids.length; i++) {
            if (i > 0) t += '\n';
            t += (kids[i].textContent || '');
        }
        cursor = t.length;
    }
    return [text, cursor];
}

// Reads text + cursor together and returns the tagged-string protocol the Rust
// side parses. If a navigate or task-checkbox click was recorded, those are
// returned first. Possible returns:
//   "-1\n"                          → element missing
//   "nav:<url>"                     → navigate click
//   "cb:<pos>:<0|1>"                → task-checkbox click
//   "linechange\n<cursor>\n<text>"  → active line changed
//   "<cursor>\n<text>"              → normal keystroke
export function read_state(id) {
    const el = document.getElementById(id);
    if (!el) return "-1\n";
    if (el._navClick) {
        const url = el._navClick;
        el._navClick = null;
        return 'nav:' + url;
    }
    if (el._taskClick) {
        const tc = el._taskClick;
        el._taskClick = null;
        return 'cb:' + tc.pos + ':' + (tc.checked ? '1' : '0');
    }
    // Enter handler computed the new text + caret in the model (see
    // setup_keyboard) — use it verbatim and force a re-render.
    if (el._pendingText != null) {
        const text = el._pendingText, cursor = el._pendingCursor;
        el._pendingText = null;
        el._pendingCursor = null;
        el.dataset.lineChange = '';
        return 'linechange\n' + cursor + '\n' + text;
    }
    const [text, cursor] = lineTextAndCursor(el);
    if (el.dataset.lineChange) {
        el.dataset.lineChange = '';
        return 'linechange\n' + cursor + '\n' + text;
    }
    return cursor + "\n" + text;
}

// Places a collapsed caret `offset` characters into a single `.md-line`. When
// the line has no text node (an empty line), the caret is set on the element
// itself so it still lands on that blank line.
function placeCaretInLine(line, offset) {
    const walker = document.createTreeWalker(line, NodeFilter.SHOW_TEXT, null);
    let acc = 0, node = null, nodeOff = 0;
    while (walker.nextNode()) {
        const n = walker.currentNode, len = n.textContent.length;
        if (offset <= acc + len) { node = n; nodeOff = offset - acc; break; }
        acc += len;
    }
    try {
        const range = document.createRange();
        if (node) range.setStart(node, nodeOff);
        else range.setStart(line, 0);
        range.collapse(true);
        const sel = window.getSelection();
        sel.removeAllRanges();
        sel.addRange(range);
    } catch (_) { }
}

// Sets innerHTML directly (bypassing the Dioxus render cycle) and immediately
// restores the caret — both synchronously, so they can't race each other. The
// caret offset is in the same line-deterministic space as `lineTextAndCursor`
// (one '\n' per line boundary), so we walk the rebuilt `.md-line` divs counting
// each line's text length plus one for the break between lines.
// `html` arrives already serialized from Rust; no manual escaping required.
export function apply_html_and_restore_cursor(id, html, cursor) {
    const el = document.getElementById(id);
    if (!el) return;
    el.dataset.rendering = '1';
    el.innerHTML = html;
    if (cursor >= 0) {
        const lines = el.querySelectorAll(':scope > .md-line');
        if (lines.length) {
            let remaining = cursor;
            let placed = false;
            for (let li = 0; li < lines.length; li++) {
                const len = lines[li].textContent.length;
                if (remaining <= len) {
                    placeCaretInLine(lines[li], remaining);
                    placed = true;
                    break;
                }
                remaining -= len + 1; // +1 for the '\n' between lines
            }
            if (!placed) {
                const last = lines[lines.length - 1];
                placeCaretInLine(last, last.textContent.length);
            }
        }
    }
    // Clear the flag after the selectionchange triggered by innerHTML has fired.
    setTimeout(function () { el.dataset.rendering = ''; }, 0);
}
