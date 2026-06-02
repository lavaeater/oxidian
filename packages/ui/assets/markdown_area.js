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

// Intercepts Enter on list lines and inserts the correct continuation prefix.
export function setup_keyboard(id) {
    const el = document.getElementById(id);
    if (!el || el.dataset.kbSetup) return;
    el.dataset.kbSetup = '1';
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
        if (!node || node === el) return;
        // textContent includes hidden marker text regardless of font-size CSS.
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
        if (!prefix) return;
        // If the line has no content beyond the marker, exit the list instead.
        if (line.slice(markerLen).trim() === '') return;
        e.preventDefault();
        document.execCommand('insertText', false, prefix);
    });
}

// Reads innerText and cursor offset together and returns the tagged-string
// protocol the Rust side parses. If a navigate or task-checkbox click was
// recorded, those are returned first. Possible returns:
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
    const text = el.innerText;
    const sel = window.getSelection();
    let cursor = -1;
    if (sel && sel.rangeCount > 0) {
        const range = sel.getRangeAt(0);
        if (el.contains(range.startContainer)) {
            let offset = 0;
            const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT, null);
            while (walker.nextNode()) {
                if (walker.currentNode === range.startContainer) {
                    cursor = offset + range.startOffset;
                    break;
                }
                offset += walker.currentNode.textContent.length;
            }
        }
    }
    if (el.dataset.lineChange) {
        el.dataset.lineChange = '';
        return 'linechange\n' + cursor + '\n' + text;
    }
    return cursor + "\n" + text;
}

// Sets innerHTML directly (bypassing the Dioxus render cycle) and immediately
// restores the cursor — both synchronously, so they can't race each other.
// `html` arrives already serialized from Rust; no manual escaping required.
export function apply_html_and_restore_cursor(id, html, cursor) {
    const el = document.getElementById(id);
    if (!el) return;
    el.dataset.rendering = '1';
    el.innerHTML = html;
    if (cursor >= 0) {
        let remaining = cursor;
        const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT, null);
        while (walker.nextNode()) {
            const len = walker.currentNode.textContent.length;
            if (remaining <= len) {
                try {
                    const range = document.createRange();
                    range.setStart(walker.currentNode, remaining);
                    range.collapse(true);
                    window.getSelection().removeAllRanges();
                    window.getSelection().addRange(range);
                } catch (_) {}
                return;
            }
            remaining -= len;
        }
    }
    // Clear the flag after the selectionchange triggered by innerHTML has fired.
    setTimeout(function () { el.dataset.rendering = ''; }, 0);
}
