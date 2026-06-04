// Centralized browser glue for the Oxidian app, bound into Rust via
// `dioxus_use_js::use_js!` (see src/js.rs). Each exported function becomes an
// async Rust fn and arguments arrive already deserialized — so, unlike the old
// `format!`-built `document::eval` strings, there is no manual escaping to get
// wrong.

// ── localStorage ─────────────────────────────────────────────────────────────

export function ls_get(key) {
    return localStorage.getItem(key) || '';
}

export function ls_set(key, value) {
    localStorage.setItem(key, value);
}

export function ls_remove(key) {
    localStorage.removeItem(key);
}

// ── Dates ────────────────────────────────────────────────────────────────────

// Today's date as YYYY-MM-DD.
export function today() {
    return new Date().toISOString().split('T')[0];
}

// Rich set of date variables for template substitution. Returns a JSON string
// (parsed by `TemplateVars::from_json` on the Rust side).
export function date_vars() {
    const d = new Date();
    const months = ['January', 'February', 'March', 'April', 'May', 'June',
        'July', 'August', 'September', 'October', 'November', 'December'];
    const days = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
    const pad = n => String(n).padStart(2, '0');
    const jan4 = new Date(d.getFullYear(), 0, 4);
    const dow = jan4.getDay() || 7;
    const weekStart = new Date(jan4);
    weekStart.setDate(jan4.getDate() - dow + 1);
    const week = Math.max(1, Math.floor((d - weekStart) / 604800000) + 1);
    return JSON.stringify({
        year: String(d.getFullYear()),
        yearShort: String(d.getFullYear()).slice(-2),
        month: pad(d.getMonth() + 1),
        monthName: months[d.getMonth()],
        date: pad(d.getDate()),
        dayName: days[d.getDay()],
        week: pad(week)
    });
}

// ── Dialogs / clipboard ──────────────────────────────────────────────────────

export function confirm_dialog(message) {
    return !!window.confirm(message);
}

export function copy_to_clipboard(text) {
    if (navigator.clipboard) navigator.clipboard.writeText(text).catch(() => { });
}

// ── Focus / scroll / resize ──────────────────────────────────────────────────

export function focus_selector(selector) {
    requestAnimationFrame(function () {
        const el = document.querySelector(selector);
        if (el) el.focus();
    });
}

export function scroll_active_into_view() {
    setTimeout(function () {
        const el = document.querySelector('.file-entry--active');
        if (el) el.scrollIntoView({ block: 'nearest' });
    }, 50);
}

// Begins an interactive sidebar drag: the listeners detach themselves on pointerup.
export function start_sidebar_resize() {
    const root = document.documentElement;
    function onMove(e) {
        const w = Math.max(160, Math.min(600, e.clientX));
        root.style.setProperty('--sidebar-w', w + 'px');
    }
    function onUp() {
        window.removeEventListener('pointermove', onMove);
        window.removeEventListener('pointerup', onUp);
        document.body.style.cursor = '';
        document.body.style.userSelect = '';
    }
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
}

// ── HTML export download ──────────────────────────────────────────────────────

export function download_file(filename, content) {
    const blob = new Blob([content], { type: 'text/html' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = filename;
    a.click();
    URL.revokeObjectURL(a.href);
}

// ── Editor selection (toolbar) ────────────────────────────────────────────────

// Returns the editable `.md-area` the user is currently working in. With a
// single editor this is just that editor; with a split (two editors in the DOM)
// it returns the one containing the selection, else the focused one, else the
// first. This keeps selection/slash/apply operations targeting the right pane.
function activeMdArea() {
    const areas = document.querySelectorAll('.md-area[contenteditable="true"]');
    if (areas.length <= 1) return areas[0] || null;
    const sel = window.getSelection();
    if (sel && sel.rangeCount && sel.anchorNode) {
        for (const el of areas) if (el.contains(sel.anchorNode)) return el;
    }
    const af = document.activeElement;
    if (af) {
        for (const el of areas) if (el === af || el.contains(af)) return el;
    }
    return areas[0];
}

// Returns [start, end] character offsets of the selection within the active
// editor, or [-1, -1] when there is none.
export function get_selection() {
    const el = activeMdArea();
    if (!el) return [-1, -1];
    const sel = window.getSelection();
    if (!sel || !sel.rangeCount || !el.contains(sel.anchorNode)) return [-1, -1];
    const range = sel.getRangeAt(0);
    let start = -1, end = -1, off = 0;
    const walk = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
    while (walk.nextNode()) {
        const n = walk.currentNode, len = n.textContent.length;
        if (start < 0 && n === range.startContainer) start = off + range.startOffset;
        if (end < 0 && n === range.endContainer) end = off + range.endOffset;
        off += len;
    }
    if (start < 0) start = off;
    if (end < 0) end = off;
    return [start, end];
}

// ── Slash commands ────────────────────────────────────────────────────────────

// Returns the text typed after the most recent `/` on the current line, "" if
// the cursor sits right after a bare `/`, or "\x00" (NO_SLASH) when the cursor
// is not in a slash token.
export function slash_query() {
    const NO_SLASH = '\x00';
    const el = activeMdArea();
    if (!el) return NO_SLASH;
    const sel = window.getSelection();
    if (!sel || !sel.rangeCount || !el.contains(sel.anchorNode)) return NO_SLASH;
    const range = sel.getRangeAt(0);
    let offset = range.startOffset;
    let node = range.startContainer;
    let collected = '';
    // Walk backwards through text nodes.
    while (true) {
        const text = (node.textContent || '').slice(0, offset);
        for (let i = text.length - 1; i >= 0; i--) {
            const ch = text[i];
            if (ch === '/') return collected;
            if (/[\s\n]/.test(ch)) return NO_SLASH;
            collected = ch + collected;
        }
        const walk = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
        let prev = null, cur = walk.nextNode();
        while (cur && cur !== node) { prev = cur; cur = walk.nextNode(); }
        if (!prev) return NO_SLASH;
        node = prev; offset = prev.textContent.length;
    }
}

// Replaces the `/query` token at the cursor with `snippet`. `slashLen` = 1 (the
// `/`) + query length. `snippet` arrives already deserialized — no escaping.
export function apply_slash(snippet, slashLen) {
    const el = activeMdArea();
    if (!el) return;
    const sel = window.getSelection();
    if (!sel || !sel.rangeCount) return;
    const range = sel.getRangeAt(0);
    let remaining = slashLen, cur = range.startContainer, off = range.startOffset;
    while (remaining > 0 && cur) {
        const take = Math.min(off, remaining);
        cur.textContent = cur.textContent.slice(0, off - take) + cur.textContent.slice(off);
        off -= take; remaining -= take;
        if (remaining > 0) {
            const w = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
            let prev = null, c = w.nextNode();
            while (c && c !== cur) { prev = c; c = w.nextNode(); }
            if (!prev) break;
            cur = prev; off = prev.textContent.length;
        }
    }
    cur.textContent = cur.textContent.slice(0, off) + snippet + cur.textContent.slice(off);
    // Cursor placement: between markers for [[]], ****, or after the snippet.
    let cursor = off + snippet.length;
    if (snippet === '[[]]') cursor = off + 2;
    else if (snippet === '****') cursor = off + 2;
    else if (snippet === '**') cursor = off + 1;
    const r2 = document.createRange();
    r2.setStart(cur, Math.min(cursor, cur.textContent.length));
    r2.collapse(true);
    sel.removeAllRanges(); sel.addRange(r2);
    el.dispatchEvent(new Event('input', { bubbles: true }));
}

// ── Kanban drag data ──────────────────────────────────────────────────────────

export function get_drag_data() {
    return window.__oxidianDragData || '';
}

export function set_drag_data(data) {
    window.__oxidianDragData = data;
}

export function clear_drag_data() {
    window.__oxidianDragData = '';
}
