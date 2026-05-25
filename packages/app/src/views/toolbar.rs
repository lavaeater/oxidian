use dioxus::prelude::*;

const JS_GET_SEL: &str = r#"
(function() {
    const el = document.querySelector('.md-area[contenteditable="true"]');
    if (!el) { dioxus.send('[-1,-1]'); return; }
    const sel = window.getSelection();
    if (!sel || !sel.rangeCount || !el.contains(sel.anchorNode)) {
        dioxus.send('[-1,-1]'); return;
    }
    const range = sel.getRangeAt(0);
    let start = -1, end = -1, off = 0;
    const walk = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
    while (walk.nextNode()) {
        const n = walk.currentNode, len = n.textContent.length;
        if (start < 0 && n === range.startContainer) start = off + range.startOffset;
        if (end   < 0 && n === range.endContainer)   end   = off + range.endOffset;
        off += len;
    }
    if (start < 0) start = off;
    if (end   < 0) end   = off;
    dioxus.send(JSON.stringify([start, end]));
})();
"#;

pub async fn get_sel() -> (usize, usize) {
    let json = document::eval(JS_GET_SEL)
        .join::<String>()
        .await
        .unwrap_or_default();
    let v: [i64; 2] = serde_json::from_str(&json).unwrap_or([-1, -1]);
    if v[0] < 0 { (0, 0) } else { (v[0] as usize, v[1] as usize) }
}

pub fn wrap(source: &str, start: usize, end: usize, prefix: &str, suffix: &str) -> String {
    let mut s = source.to_string();
    let selected = source[start..end].to_string();
    s.replace_range(start..end, &format!("{prefix}{selected}{suffix}"));
    s
}

pub fn toggle_line_prefix(source: &str, cursor: usize, prefix: &str) -> String {
    let ls = source[..cursor].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line = &source[ls..];
    let mut s = source.to_string();
    if line.starts_with(prefix) {
        s.replace_range(ls..ls + prefix.len(), "");
    } else {
        s.insert_str(ls, prefix);
    }
    s
}

// ── Component ─────────────────────────────────────────────────────────────────

#[component]
pub fn FormattingToolbar(mut content: Signal<String>) -> Element {
    rsx! {
        div { class: "formatting-toolbar",
            TbPrefix { content, label: "H1", title: "Heading 1", prefix: "# " }
            TbPrefix { content, label: "H2", title: "Heading 2", prefix: "## " }
            TbPrefix { content, label: "H3", title: "Heading 3", prefix: "### " }
            div { class: "tb-sep" }
            TbInline { content, label: "B",      title: "Bold",          prefix: "**", suffix: "**" }
            TbInline { content, label: "I",      title: "Italic",        prefix: "*",  suffix: "*"  }
            TbInline { content, label: "S",      title: "Strikethrough", prefix: "~~", suffix: "~~" }
            TbInline { content, label: "`C`",    title: "Inline code",   prefix: "`",  suffix: "`"  }
            div { class: "tb-sep" }
            TbPrefix { content, label: "• List", title: "Bullet list", prefix: "- "      }
            TbPrefix { content, label: "☐ Task", title: "Task item",   prefix: "- [ ] "  }
            TbPrefix { content, label: "> Quote",title: "Blockquote",  prefix: "> "      }
        }
    }
}

#[component]
fn TbInline(
    mut content: Signal<String>,
    label: &'static str,
    title: &'static str,
    prefix: &'static str,
    suffix: &'static str,
) -> Element {
    rsx! {
        button {
            class: "tb-btn",
            title: "{title}",
            onmousedown: move |e| e.prevent_default(),
            onclick: move |_| {
                spawn(async move {
                    let (s, e) = get_sel().await;
                    content.with_mut(|c| *c = wrap(c, s, e, prefix, suffix));
                });
            },
            "{label}"
        }
    }
}

#[component]
fn TbPrefix(
    mut content: Signal<String>,
    label: &'static str,
    title: &'static str,
    prefix: &'static str,
) -> Element {
    rsx! {
        button {
            class: "tb-btn",
            title: "{title}",
            onmousedown: move |e| e.prevent_default(),
            onclick: move |_| {
                spawn(async move {
                    let (s, _) = get_sel().await;
                    content.with_mut(|c| *c = toggle_line_prefix(c, s, prefix));
                });
            },
            "{label}"
        }
    }
}
