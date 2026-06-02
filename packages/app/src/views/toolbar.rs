use dioxus::prelude::*;

use crate::js;

/// `(start, end)` selection offsets in the active editor. Thin wrapper around
/// `js::get_selection()` (DOM glue lives in `assets/oxidian.js`).
pub async fn get_sel() -> (usize, usize) {
    js::get_selection().await
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
