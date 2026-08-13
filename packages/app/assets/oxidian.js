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

// NOTE: every promise-returning export here MUST be declared `async`.
// `use_js!` only emits an `await` for functions marked async, so a plain
// `function` returning a Promise hands the Promise itself back across the
// bridge, where it fails to serialise and the Rust side silently sees a default
// value — a write that appears to work and a read that always comes back empty.
//
// ── Large blobs (IndexedDB) ──────────────────────────────────────────────────
// `localStorage` caps out around 5 MB and is synchronous, which makes it the
// wrong place for the vault index — a few thousand notes exceed it and the
// write blocks the UI thread. IndexedDB has a quota in the hundreds of MB (and
// no hard cap when storage is persisted), so anything that scales with vault
// size lives here. Small, fixed-size settings stay in localStorage.
//
// Deliberately hand-rolled rather than pulling in a wrapper library: one object
// store, three operations, no schema evolution to speak of.

const BLOB_DB = 'oxidian';
const BLOB_DB_VERSION = 2;
const BLOB_STORE = 'blobs';
// One record per note, keyed by path. See `records_*` below.
const PAGE_STORE = 'pages';

function blobDb() {
    return new Promise((resolve, reject) => {
        const req = indexedDB.open(BLOB_DB, BLOB_DB_VERSION);
        req.onupgradeneeded = () => {
            const db = req.result;
            if (!db.objectStoreNames.contains(BLOB_STORE)) db.createObjectStore(BLOB_STORE);
            if (!db.objectStoreNames.contains(PAGE_STORE)) db.createObjectStore(PAGE_STORE);
        };
        req.onsuccess = () => resolve(req.result);
        req.onerror = () => reject(req.error);
    });
}

function tx(store, mode, run) {
    return blobDb().then(db => new Promise((resolve, reject) => {
        const t = db.transaction(store, mode);
        const req = run(t.objectStore(store));
        t.oncomplete = () => { db.close(); resolve(req ? req.result : undefined); };
        t.onerror = () => { db.close(); reject(t.error); };
    }));
}

// Returns '' when the key is absent, matching `ls_get`, so callers treat a
// missing store and an empty store the same way.
export async function blob_get(key) {
    return tx(BLOB_STORE, 'readonly', st => st.get(key)).then(v => v || '').catch(() => '');
}

export async function blob_set(key, value) {
    // Swallows quota errors: the index is a cache, and failing to save it must
    // never break the app — the next refresh simply rebuilds it.
    return tx(BLOB_STORE, 'readwrite', st => st.put(value, key)).then(() => true).catch(() => false);
}

export async function blob_remove(key) {
    return tx(BLOB_STORE, 'readwrite', st => st.delete(key)).then(() => true).catch(() => false);
}

// ── Per-note index records ───────────────────────────────────────────────────
// The index is stored one record per note rather than as a single blob. Saving
// a note then costs one small write instead of re-serialising and rewriting the
// entire index — which, now that the index carries every note's text for search,
// is roughly the size of the vault.
//
// Every operation is batched into a single transaction: seeding a vault is
// thousands of puts, and one transaction per put would be pathologically slow.

// All records as `[[path, json], ...]`.
export async function records_all() {
    return blobDb().then(db => new Promise((resolve, reject) => {
        const t = db.transaction(PAGE_STORE, 'readonly');
        const st = t.objectStore(PAGE_STORE);
        // getAllKeys + getAll rather than a cursor: two requests instead of one
        // round trip per record.
        const keys = st.getAllKeys();
        const vals = st.getAll();
        t.oncomplete = () => {
            db.close();
            resolve(keys.result.map((k, i) => [k, vals.result[i]]));
        };
        t.onerror = () => { db.close(); reject(t.error); };
    })).catch(() => []);
}

export async function records_put(entries) {
    return tx(PAGE_STORE, 'readwrite', st => {
        for (const [key, value] of entries) st.put(value, key);
    }).then(() => true).catch(() => false);
}

export async function records_delete(keys) {
    return tx(PAGE_STORE, 'readwrite', st => {
        for (const key of keys) st.delete(key);
    }).then(() => true).catch(() => false);
}

export async function records_clear() {
    return tx(PAGE_STORE, 'readwrite', st => st.clear()).then(() => true).catch(() => false);
}

// `[usage, quota, persisted]` in bytes, for the storage readout in Settings.
// -1 for usage/quota when the browser won't say (Safari, mostly).
export async function storage_estimate() {
    const persisted = () => (navigator.storage && navigator.storage.persisted)
        ? navigator.storage.persisted() : Promise.resolve(false);
    if (!navigator.storage || !navigator.storage.estimate) {
        return persisted().then(p => [-1, -1, p ? 1 : 0]);
    }
    return Promise.all([navigator.storage.estimate(), persisted()])
        .then(([est, p]) => [est.usage ?? -1, est.quota ?? -1, p ? 1 : 0])
        .catch(() => [-1, -1, 0]);
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
    // navigator.clipboard is unavailable in the Android WebView (it requires a
    // secure context / user-permission that the embedded view doesn't grant),
    // so fall back to the legacy execCommand path via a temporary textarea.
    if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(text).catch(() => legacyCopy(text));
    } else {
        legacyCopy(text);
    }
}

function legacyCopy(text) {
    try {
        const ta = document.createElement('textarea');
        ta.value = text;
        ta.setAttribute('readonly', '');
        ta.style.position = 'fixed';
        ta.style.top = '-1000px';
        ta.style.opacity = '0';
        document.body.appendChild(ta);
        ta.focus();
        ta.select();
        ta.setSelectionRange(0, text.length);
        document.execCommand('copy');
        document.body.removeChild(ta);
    } catch (_) { }
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

// ── Task metadata menu ────────────────────────────────────────────────────────
// Armed by `markdown_area.js`'s Enter handler the moment it continues a
// non-empty *task* line (`- [ ] ` → new blank task line); kept accurate by
// `read_state` as the user keeps typing. This just polls that flag off the
// currently-focused editor's DOM element — the "insert the picked emoji" side
// reuses `apply_slash(snippet, 0)` (no token to delete, just insert at caret).
export function task_menu_armed() {
    const el = activeMdArea();
    return !!(el && el._armTaskMenu);
}

// Explicit dismissal (e.g. clicking outside the menu) with no text change, so
// `apply_slash`'s own re-render can't re-validate the flag for us.
export function dismiss_task_menu() {
    const el = activeMdArea();
    if (el) el._armTaskMenu = false;
}

// ── Sign-in link (portable config) ────────────────────────────────────────────
// A "sign-in link" carries the vault config in the URL *fragment* so it can be
// bookmarked / stored in a password manager and restored in one click — handy
// where the browser wipes localStorage between sessions (e.g. a managed work
// profile that clears site data on exit). The token rides in the `#fragment`,
// which is never sent to any server; we strip it from the address bar the moment
// it's read so it doesn't linger in history or a copy-pasted URL.

// Build a shareable sign-in link for the given config JSON.
export function build_signin_link(cfgJson) {
    return location.origin + location.pathname + '#cfg=' + base64UrlEncode(cfgJson);
}

// If the current URL carries a `#cfg=…` sign-in link, return the decoded config
// JSON and strip the fragment. Returns '' when there is none.
export function read_signin_link() {
    const m = (location.hash || '').match(/[#&]cfg=([^&]+)/);
    if (!m) return '';
    let json;
    try { json = base64UrlDecode(m[1]); } catch (_) { return ''; }
    try {
        history.replaceState(null, '', location.origin + location.pathname + location.search);
    } catch (_) { }
    return json;
}

// Ask the browser to keep our storage persistent (resists eviction under storage
// pressure). Best-effort and silent — not available in every browser, and it
// does NOT override an enterprise "clear on exit" policy.
export function request_persistent_storage() {
    try {
        if (navigator.storage && navigator.storage.persist) navigator.storage.persist();
    } catch (_) { }
}

// UTF-8-safe base64 in the URL-safe alphabet, no padding.
function base64UrlEncode(str) {
    return btoa(unescape(encodeURIComponent(str)))
        .replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function base64UrlDecode(s) {
    s = s.replace(/-/g, '+').replace(/_/g, '/');
    while (s.length % 4) s += '=';
    return decodeURIComponent(escape(atob(s)));
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
