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
        const nav = e.target.closest('[data-navigate]');
        if (nav) {
            // Same reason as the action path below: letting the caret land here
            // fires a selectionchange, the editor re-renders the now-active line
            // as source, and the node is detached before `click` arrives — so
            // the navigation would be silently dropped. `preventDefault` on
            // mousedown does not cancel the click, so a real <a href> (an
            // external markdown link) still follows its href.
            e.preventDefault();
            el._navClick = nav.dataset.navigate;
            return;
        }
        // An action inside rendered output (a dataview task checkbox). The
        // payload is opaque here — the host decides what it means. Checked
        // before the render click below, so the checkbox wins over "edit the
        // source of the block I'm in".
        const action = e.target.closest('[data-action]');
        if (action) {
            // Keep the caret out of the block. Letting it move would fire a
            // selectionchange, which re-renders the editor and detaches this
            // node before the `click` event lands — the action would be lost.
            // `preventDefault` on mousedown does not cancel the click.
            e.preventDefault();
            el._actionClick = action.dataset.action;
            // Flip it now rather than waiting for the write to land: the host
            // repaints from real data afterwards, and this matches what the
            // in-source checkboxes already do.
            if (action.classList.contains('md-task-checkbox')) {
                const on = action.dataset.checked !== 'true';
                action.dataset.checked = on ? 'true' : 'false';
                // Edit the existing text node rather than assigning
                // `textContent`, which replaces it: swapping the node the
                // pointer is over between mousedown and mouseup makes the
                // browser skip the `click` event entirely, and the action
                // would never reach the host.
                if (action.firstChild) action.firstChild.nodeValue = on ? '[x]' : '[ ]';
            }
            return;
        }
        // `data-pos` is the source offset of the `[`; a checkbox without one
        // (a rendered dataview task) isn't editable through this path yet.
        const cb = e.target.closest('.md-task-checkbox');
        if (cb && cb.dataset.pos !== undefined) {
            el._taskClick = {
                pos: parseInt(cb.dataset.pos),
                checked: cb.dataset.checked === 'true'
            };
            return;
        }
        // Clicking a rendered block (a folded ```dataview) asks to edit its
        // source: the offset is where the caret should land once it unfolds.
        const render = e.target.closest('[data-md-render]');
        if (render && render.dataset.editOffset !== undefined) {
            // Same reason as above; the caret is placed deliberately once the
            // block has unfolded (`apply_html_and_restore_cursor`).
            e.preventDefault();
            el._renderClick = render.dataset.editOffset;
        }
    }, true);
}

// The text of a node as the *document model* sees it: rendered blocks
// (`[data-md-render]`, e.g. a dataview table) are output, not note content, so
// they contribute nothing. Every caret/offset calculation goes through this —
// plain `textContent` would count the rendered table's text as note text and
// desynchronise the model from the source on the very first keystroke.
// Works on elements, text nodes, and the DocumentFragment a Range clones into.
function visibleText(node) {
    if (!node) return '';
    if (node.nodeType === 3) return node.textContent || '';
    if (node.nodeType === 1 && node.dataset && node.dataset.mdRender !== undefined) return '';
    let t = '';
    for (let i = 0; i < node.childNodes.length; i++) t += visibleText(node.childNodes[i]);
    return t;
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

    // Space on a task line offers the task-metadata menu (due date, priority,
    // done). Obsidian triggers on space too, and although it fires often, it
    // fires *while you are still on the task you mean* — which is the whole
    // point. Enter used to arm it, and that was backwards: Enter also moves the
    // caret to the next line, so the menu appeared for a task you had already
    // left.
    //
    // The space itself is never intercepted: it types normally, and the menu is
    // purely additive. `updateTaskMenuArm` disarms the moment you type anything
    // else, so ignoring it costs one keystroke.
    el.addEventListener('keydown', function (e) {
        if (e.key === ' ' && !e.ctrlKey && !e.metaKey && !e.isComposing) {
            const [text, cursor] = lineTextAndCursor(el);
            if (cursor >= 0) {
                const ls = text.lastIndexOf('\n', cursor - 1) + 1;
                let le = text.indexOf('\n', cursor);
                if (le < 0) le = text.length;
                // Only at the end of a task line: mid-line, a space is just a
                // space between words, and popping a menu there would be noise.
                el._armTaskMenu = cursor === le && /^\s*[-*+] \[[ xX]\] /.test(text.slice(ls, le));
            }
            return;
        }
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
            cursor = text.length + visibleText(pre.cloneContents()).length;
        }
        text += visibleText(kid);
    }
    // Caret sitting directly on the editor element, between line nodes.
    if (range && cursor < 0 && range.startContainer === el) {
        let t = '';
        for (let i = 0; i < range.startOffset && i < kids.length; i++) {
            if (i > 0) t += '\n';
            t += visibleText(kids[i]);
        }
        cursor = t.length;
    }
    return [text, cursor];
}

// What the last mousedown asked for, if anything. Read (and cleared) by the
// click handler only — deliberately separate from `read_state`, because both
// are destructive reads and a click and a selectionchange arrive together: when
// they shared one function, whichever handler ran first ate the other's payload
// and the editor silently skipped a re-render.
//   ""                              → nothing pending
//   "nav:<url>"                     → navigate click
//   "cb:<pos>:<0|1>"                → task-checkbox click
//   "act:<payload>"                 → action inside rendered output
//   "edit:<offset>"                 → rendered block clicked; show its source
export function read_click(id) {
    const el = document.getElementById(id);
    if (!el) return '';
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
    if (el._actionClick != null) {
        const a = el._actionClick;
        el._actionClick = null;
        return 'act:' + a;
    }
    if (el._renderClick != null) {
        const off = el._renderClick;
        el._renderClick = null;
        return 'edit:' + off;
    }
    return '';
}

// Reads text + cursor together and returns the tagged-string protocol the Rust
// side parses. Possible returns:
//   "-1\n"                          → element missing
//   "linechange\n<cursor>\n<text>"  → active line changed
//   "<cursor>\n<text>"              → normal keystroke
export function read_state(id) {
    const el = document.getElementById(id);
    if (!el) return "-1\n";
    // Enter handler computed the new text + caret in the model (see
    // setup_keyboard) — use it verbatim and force a re-render.
    if (el._pendingText != null) {
        const text = el._pendingText, cursor = el._pendingCursor;
        el._pendingText = null;
        el._pendingCursor = null;
        el.dataset.lineChange = '';
        updateTaskMenuArm(el, text, cursor);
        return 'linechange\n' + cursor + '\n' + text;
    }
    const [text, cursor] = lineTextAndCursor(el);
    updateTaskMenuArm(el, text, cursor);
    if (el.dataset.lineChange) {
        el.dataset.lineChange = '';
        return 'linechange\n' + cursor + '\n' + text;
    }
    return cursor + "\n" + text;
}

// Keeps `el._armTaskMenu` accurate as the user keeps typing: it's set true by
// space at the end of a task line (see `setup_keyboard`), and stays true only
// while the caret is still sitting right after that space. Typing any character
// disarms it — which is the "start typing and it goes away" behaviour — as does
// moving off the line. `app`'s `task_menu_armed()` (in `oxidian.js`) polls this
// flag directly off the DOM element to decide whether to show the menu.
function updateTaskMenuArm(el, text, cursor) {
    if (!el._armTaskMenu) return;
    if (cursor < 0) { el._armTaskMenu = false; return; }
    const lineStart = text.lastIndexOf('\n', cursor - 1) + 1;
    let lineEnd = text.indexOf('\n', cursor);
    if (lineEnd < 0) lineEnd = text.length;
    const line = text.slice(lineStart, lineEnd);
    const stillArmed = cursor === lineEnd
        && /^\s*[-*+] \[[ xX]\] /.test(line)
        && line.endsWith(' ');
    if (!stillArmed) el._armTaskMenu = false;
}

// ── Keep the writing line comfortable ────────────────────────────────────────
// Typing near the bottom of the viewport leaves the caret cramped against the
// edge. This keeps the *active line* inside a comfortable vertical band of the
// scroll container, smoothly nudging the document up once the user pauses. It
// reacts only to editing (`input`), so scrolling up to re-read is never yanked
// back down.

const RECENTER_DELAY_MS = 400;
// Comfortable band + landing target, as fractions of the container height.
const BAND_TOP = 0.25, BAND_BOTTOM = 0.62, TARGET = 0.42;

export function setup_scroll(id) {
    const el = document.getElementById(id);
    if (!el || el.dataset.scrollSetup) return;
    el.dataset.scrollSetup = '1';
    let timer = null;
    el.addEventListener('input', function () {
        clearTimeout(timer);
        timer = setTimeout(function () { recenterActiveLine(el); }, RECENTER_DELAY_MS);
    });
}

// Force an immediate recenter (used by tests and any explicit caller).
export function recenter_caret(id) {
    const el = document.getElementById(id);
    if (el) recenterActiveLine(el);
}

// The scroll container: the editor element itself when it overflows (the
// `.md-area` is `overflow-y: auto`), otherwise the nearest scrollable ancestor.
function scrollContainer(el) {
    let p = el;
    while (p) {
        const oy = getComputedStyle(p).overflowY;
        if ((oy === 'auto' || oy === 'scroll') && p.scrollHeight > p.clientHeight + 1) return p;
        p = p.parentElement;
    }
    return null;
}

function recenterActiveLine(el) {
    const line = el.querySelector('.md-line--active') || activeLineFromSelection(el);
    if (!line) return;
    const cont = scrollContainer(el);
    if (!cont) return;
    const y = line.getBoundingClientRect().top - cont.getBoundingClientRect().top;
    const h = cont.clientHeight;
    if (y >= h * BAND_TOP && y <= h * BAND_BOTTOM) return; // already comfortable
    cont.scrollBy({ top: y - h * TARGET, behavior: 'smooth' });
}

// Fallback when no line carries the active class yet (e.g. right after a
// programmatic caret restore): derive the line from the current selection.
function activeLineFromSelection(el) {
    const sel = window.getSelection();
    if (!sel || !sel.rangeCount || !el.contains(sel.anchorNode)) return null;
    let cur = sel.anchorNode;
    if (cur.nodeType !== 1) cur = cur.parentElement;
    while (cur && cur !== el) {
        if (cur.classList && cur.classList.contains('md-line')) return cur;
        cur = cur.parentElement;
    }
    return null;
}

// Places a collapsed caret `offset` characters into a single `.md-line`. When
// the line has no text node (an empty line), the caret is set on the element
// itself so it still lands on that blank line.
function placeCaretInLine(line, offset) {
    // Skip text inside rendered blocks — the caret belongs in the source, and
    // `offset` was measured in the source's coordinate space (`visibleText`).
    const walker = document.createTreeWalker(line, NodeFilter.SHOW_TEXT, {
        acceptNode: function (n) {
            for (let p = n.parentElement; p && p !== line; p = p.parentElement) {
                if (p.dataset && p.dataset.mdRender !== undefined) return NodeFilter.FILTER_REJECT;
            }
            return NodeFilter.FILTER_ACCEPT;
        }
    });
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
        // A click on a rendered block can ask for the caret while the editor is
        // not yet focused; without this the selection would be set on a blurred
        // element and the first keystroke would go nowhere.
        if (document.activeElement !== el) el.focus();
        const lines = el.querySelectorAll(':scope > .md-line');
        if (lines.length) {
            let remaining = cursor;
            let placed = false;
            for (let li = 0; li < lines.length; li++) {
                const len = visibleText(lines[li]).length;
                if (remaining <= len) {
                    placeCaretInLine(lines[li], remaining);
                    placed = true;
                    break;
                }
                remaining -= len + 1; // +1 for the '\n' between lines
            }
            if (!placed) {
                const last = lines[lines.length - 1];
                placeCaretInLine(last, visibleText(last).length);
            }
        }
    }
    // Clear the flag after the selectionchange triggered by innerHTML has fired.
    setTimeout(function () { el.dataset.rendering = ''; }, 0);
}
