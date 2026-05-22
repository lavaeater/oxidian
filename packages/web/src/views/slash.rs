use dioxus::prelude::*;

// JS: returns the text typed after the most recent `/` on the current line,
// or "" if the cursor is not right after a `/…` token.
pub const JS_SLASH_QUERY: &str = r#"
(function() {
    const el = document.querySelector('.md-area[contenteditable="true"]');
    if (!el) { dioxus.send(''); return; }
    const sel = window.getSelection();
    if (!sel || !sel.rangeCount || !el.contains(sel.anchorNode)) {
        dioxus.send(''); return;
    }
    const range = sel.getRangeAt(0);
    let offset = range.startOffset;
    let node = range.startContainer;
    let collected = '';
    // Walk backwards through text nodes
    outer: while (true) {
        const text = (node.textContent || '').slice(0, offset);
        for (let i = text.length - 1; i >= 0; i--) {
            const ch = text[i];
            if (ch === '/') { dioxus.send(collected); return; }
            if (/[\s\n]/.test(ch)) { dioxus.send(''); return; }
            collected = ch + collected;
        }
        // Previous text node
        const walk = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
        let prev = null, cur = walk.nextNode();
        while (cur && cur !== node) { prev = cur; cur = walk.nextNode(); }
        if (!prev) { dioxus.send(''); return; }
        node = prev; offset = prev.textContent.length;
    }
})();
"#;

// Replaces the `/query` token at the cursor with `snippet`.
// `slash_len` = 1 (the `/`) + query.len()
pub fn js_apply_slash(snippet: &str, slash_len: usize) -> String {
    let escaped = snippet.replace('`', "\\`").replace("${", "\\${");
    format!(r#"
(function() {{
    const el = document.querySelector('.md-area[contenteditable="true"]');
    if (!el) return;
    const sel = window.getSelection();
    if (!sel || !sel.rangeCount) return;
    const range = sel.getRangeAt(0);
    let remaining = {slash_len}, cur = range.startContainer, off = range.startOffset;
    while (remaining > 0 && cur) {{
        const take = Math.min(off, remaining);
        cur.textContent = cur.textContent.slice(0, off - take) + cur.textContent.slice(off);
        off -= take; remaining -= take;
        if (remaining > 0) {{
            const w = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
            let prev = null, c = w.nextNode();
            while (c && c !== cur) {{ prev = c; c = w.nextNode(); }}
            if (!prev) break;
            cur = prev; off = prev.textContent.length;
        }}
    }}
    const snippet = `{escaped}`;
    cur.textContent = cur.textContent.slice(0, off) + snippet + cur.textContent.slice(off);
    // Cursor placement: between markers for [[]], ****, or after snippet
    let cursor = off + snippet.length;
    if (snippet === '[[]]') cursor = off + 2;
    else if (snippet === '****') cursor = off + 2;
    else if (snippet === '**') cursor = off + 1;
    const r2 = document.createRange();
    r2.setStart(cur, Math.min(cursor, cur.textContent.length));
    r2.collapse(true);
    sel.removeAllRanges(); sel.addRange(r2);
    el.dispatchEvent(new Event('input', {{ bubbles: true }}));
}})();
"#)
}

// ── Slash commands catalogue ──────────────────────────────────────────────────

const COMMANDS: &[(&str, &str, &str)] = &[
    // (name, description, insert-text)
    ("Heading 1",  "# Large heading",     "# "),
    ("Heading 2",  "## Medium heading",   "## "),
    ("Heading 3",  "### Small heading",   "### "),
    ("Bold",       "**bold** text",       "****"),
    ("Italic",     "*italic* text",       "**"),
    ("Bullet",     "- Bullet list",       "- "),
    ("Numbered",   "1. Numbered list",    "1. "),
    ("Task",       "- [ ] Task item",     "- [ ] "),
    ("Quote",      "> Blockquote",        "> "),
    ("Code block", "``` code fence",      "```\n\n```"),
    ("Table",      "Markdown table",      "| Col 1 | Col 2 |\n| --- | --- |\n| | |"),
    ("Divider",    "Horizontal rule",     "\n---\n"),
    ("WikiLink",   "[[link to note]]",    "[[]]"),
];

// ── Component ─────────────────────────────────────────────────────────────────

/// `on_select(insert_text)` is called when the user picks a command.
#[component]
pub fn SlashMenu(
    query: String,
    on_select: EventHandler<String>,
    on_close: EventHandler<()>,
) -> Element {
    let q = query.to_lowercase();
    let filtered: Vec<(&str, &str, &str)> = COMMANDS.iter()
        .filter(|(name, _, _)| q.is_empty() || name.to_lowercase().contains(&q))
        .map(|(n, d, i)| (*n, *d, *i))
        .take(8)
        .collect();

    if filtered.is_empty() { return rsx! { div {} }; }

    let items: Vec<(String, String, String)> = filtered.iter()
        .map(|(n, d, i)| (n.to_string(), d.to_string(), i.to_string()))
        .collect();

    rsx! {
        div {
            class: "slash-overlay",
            onclick: move |_| on_close(()),
            div {
                class: "slash-menu",
                onclick: move |e| e.stop_propagation(),
                for (name, desc, insert) in items {
                    div {
                        class: "slash-item",
                        onclick: move |_| on_select(insert.clone()),
                        span { class: "slash-name", "{name}" }
                        span { class: "slash-desc", "{desc}" }
                    }
                }
            }
        }
    }
}
