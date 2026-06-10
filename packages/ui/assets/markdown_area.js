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

// Handles Enter inside the editor:
//   • list/task lines with content  → continue the list (insert next marker)
//   • empty list/task lines         → exit the list (delete the marker, no new item)
//   • any line                      → force a re-render of the line being left
//
// The re-render is the important part on mobile: it is what rebuilds one
// `.md-line` div per line (so block formatting like headings updates and the
// per-line list logic stays correct). Desktop gets this from `selectionchange`,
// but the Android WebView doesn't fire it reliably on Enter — so we trigger it
// here, off the keydown that we already know fires (list continuation works).
export function setup_keyboard(id) {
    const el = document.getElementById(id);
    if (!el || el.dataset.kbSetup) return;
    el.dataset.kbSetup = '1';

    // Ask the Rust side to re-tokenise + re-render the current content, which
    // restores one div per line. Deferred so the DOM has settled after Enter.
    function scheduleRerender() {
        requestAnimationFrame(function () {
            if (el.dataset.rendering) return;
            el.dataset.lineChange = '1';
            el.dispatchEvent(new Event('input', { bubbles: true }));
        });
    }

    el.addEventListener('keydown', function (e) {
        if (e.key !== 'Enter' || e.shiftKey || e.ctrlKey || e.metaKey) return;
        const sel = window.getSelection();
        if (!sel || !sel.rangeCount) return;
        // Walk up to the containing md-line div.
        let node = sel.anchorNode;
        if (node && node.nodeType !== 1) node = node.parentElement;
        while (node && node !== el) {
            if (node.classList && node.classList.contains('md-line')) break;
            node = node.parentElement;
        }
        if (!node || node === el) { scheduleRerender(); return; }
        // textContent includes hidden marker text regardless of font-size CSS.
        // NB: if lines have merged into one div (mobile), this is the *first*
        // line of the div — but a prior Enter's re-render keeps that from
        // happening for the line under the cursor in practice.
        const line = node.textContent;
        let prefix = null;
        let markerLen = 0;
        // Task item: `- [ ] ` / `- [x] ` (with optional indent).
        const taskM = line.match(/^(\s*[-*+] )\[[ xX]\] /);
        if (taskM) {
            markerLen = taskM[0].length;
            prefix = '\n' + taskM[1] + '[ ] ';
        } else {
            // Ordered list: `1. ` `2. ` …
            const olM = line.match(/^(\s*)(\d+)\. /);
            if (olM) {
                markerLen = olM[0].length;
                prefix = '\n' + olM[1] + (parseInt(olM[2]) + 1) + '. ';
            } else {
                // Unordered list: `- ` / `* ` / `+ `
                const ulM = line.match(/^(\s*)([-*+]) /);
                if (ulM) {
                    markerLen = ulM[0].length;
                    prefix = '\n' + ulM[1] + ulM[2] + ' ';
                }
            }
        }
        if (prefix) {
            if (line.slice(markerLen).trim() === '') {
                // Empty list/task item: exit the list. Delete the marker
                // (markerLen chars before the cursor, which sits right after it)
                // so the line becomes blank, and DON'T insert another item.
                e.preventDefault();
                for (let i = 0; i < markerLen; i++) document.execCommand('delete');
            } else {
                e.preventDefault();
                document.execCommand('insertText', false, prefix);
            }
        }
        // Plain lines fall through to the browser's default Enter.
        scheduleRerender();
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
