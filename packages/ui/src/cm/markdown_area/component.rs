use dioxus::prelude::*;

use super::tokenizer::{tokenize, tokenize_line, Token, TokenKind};

// ── Variant ───────────────────────────────────────────────────────────────────

#[derive(Copy, Clone, PartialEq, Default)]
#[non_exhaustive]
pub enum MarkdownAreaVariant {
    #[default]
    Default,
    Fade,
    Outline,
    Ghost,
}

impl MarkdownAreaVariant {
    pub fn class(&self) -> &'static str {
        match self {
            MarkdownAreaVariant::Default => "default",
            MarkdownAreaVariant::Fade => "fade",
            MarkdownAreaVariant::Outline => "outline",
            MarkdownAreaVariant::Ghost => "ghost",
        }
    }
}

// ── Per-instance ID ───────────────────────────────────────────────────────────

fn next_editor_id() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    format!("md-area-{}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

// ── JS helpers ────────────────────────────────────────────────────────────────

// Sets up mousedown capture for task-checkbox clicks and navigate clicks.
fn js_setup_tasks(id: &str) -> String {
    format!(
        r#"(function() {{
    const el = document.getElementById({id:?});
    if (!el || el.dataset.taskSetup) return;
    el.dataset.taskSetup = '1';
    el.addEventListener('mousedown', function(e) {{
        const cb = e.target.closest('.md-task-checkbox');
        if (cb) {{
            e.preventDefault();
            el._taskClick = {{
                pos: parseInt(cb.dataset.pos),
                checked: cb.dataset.checked === 'true'
            }};
            return;
        }}
        const nav = e.target.closest('[data-navigate]');
        if (nav) {{
            el._navClick = nav.dataset.navigate;
        }}
    }}, true);
}})()"#
    )
}

// Sets up a selectionchange listener that marks the active line div so CSS
// can show its markers. Simpler than per-token tracking.
fn js_setup_selection(id: &str) -> String {
    format!(
        r#"(function() {{
    const el = document.getElementById({id:?});
    if (!el || el.dataset.selSetup) return;
    el.dataset.selSetup = '1';
    document.addEventListener('selectionchange', function() {{
        const prev = el.querySelector('.md-line--active');
        const sel = window.getSelection();
        let next = null;
        if (sel && sel.rangeCount > 0 && el.contains(sel.anchorNode)) {{
            let cur = sel.anchorNode;
            if (cur.nodeType !== 1) cur = cur.parentElement;
            while (cur && cur !== el) {{
                if (cur.classList && cur.classList.contains('md-line')) {{
                    next = cur;
                    break;
                }}
                cur = cur.parentElement;
            }}
        }}
        if (prev !== next) {{
            if (prev) prev.classList.remove('md-line--active');
            if (next) next.classList.add('md-line--active');
        }}
    }});
}})()"#
    )
}

// Reads innerText and cursor offset together, then sends "cursor_offset\ntext".
// If a navigate or task-checkbox click was recorded, sends those first.
fn js_read_state(id: &str) -> String {
    format!(
        r#"(function() {{
    const el = document.getElementById({id:?});
    if (!el) {{ dioxus.send("-1\n"); return; }}
    if (el._navClick) {{
        const url = el._navClick;
        el._navClick = null;
        dioxus.send('nav:' + url);
        return;
    }}
    if (el._taskClick) {{
        const tc = el._taskClick;
        el._taskClick = null;
        dioxus.send('cb:' + tc.pos + ':' + (tc.checked ? '1' : '0'));
        return;
    }}
    const text = el.innerText;
    const sel = window.getSelection();
    let cursor = -1;
    if (sel && sel.rangeCount > 0) {{
        const range = sel.getRangeAt(0);
        if (el.contains(range.startContainer)) {{
            let offset = 0;
            const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT, null);
            while (walker.nextNode()) {{
                if (walker.currentNode === range.startContainer) {{
                    cursor = offset + range.startOffset;
                    break;
                }}
                offset += walker.currentNode.textContent.length;
            }}
        }}
    }}
    dioxus.send(cursor + "\n" + text);
}})()"#
    )
}

// Fire-and-forget: places the cursor at `target` character offset.
fn js_set_cursor(id: &str, target: usize) -> String {
    format!(
        r#"(function() {{
    const el = document.getElementById({id:?});
    if (!el) return;
    let offset = 0;
    const target = {target};
    const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT, null);
    while (walker.nextNode()) {{
        const node = walker.currentNode;
        const len = node.textContent.length;
        if (offset + len >= target) {{
            const sel = window.getSelection();
            const range = document.createRange();
            range.setStart(node, target - offset);
            range.collapse(true);
            sel.removeAllRanges();
            sel.addRange(range);
            return;
        }}
        offset += len;
    }}
}})()"#
    )
}

// ── HTML rendering ────────────────────────────────────────────────────────────

fn tokens_to_html(source: &str, tokens: &[Token]) -> String {
    let mut out = String::with_capacity(source.len() * 3);
    let mut last_end = 0;

    for token in tokens {
        if token.range.start > last_end {
            emit_gap_html(source, last_end, token.range.start, &mut out);
        }
        push_token_html(source, token, &mut out);
        last_end = token.range.end;
    }

    if last_end < source.len() {
        emit_gap_html(source, last_end, source.len(), &mut out);
    }

    out
}

fn emit_gap_html(source: &str, start: usize, end: usize, out: &mut String) {
    for ch in source[start..end].chars() {
        if ch == '\n' {
            out.push_str("<br>");
        } else {
            push_escaped_char(ch, out);
        }
    }
}

// Tokenizes a block token's content range for inline formatting, then renders each.
fn push_inline_html(source: &str, content_range: std::ops::Range<usize>, out: &mut String) {
    let content = &source[content_range.clone()];
    let inline_tokens = tokenize_line(content, content_range.start);
    for token in &inline_tokens {
        push_token_html(source, token, out);
    }
}

fn push_token_html(source: &str, token: &Token, out: &mut String) {
    let raw = token.raw(source);
    let display = token.display(source);

    match &token.kind {
        TokenKind::Plain => {
            out.push_str("<span class=\"md-token md-plain\">");
            push_escaped(display, out);
            out.push_str("</span>");
        }

        TokenKind::Bold => {
            out.push_str("<strong class=\"md-token md-bold\">");
            marker(&raw[..2], out);
            push_escaped(display, out);
            marker(&raw[raw.len() - 2..], out);
            out.push_str("</strong>");
        }

        TokenKind::Italic => {
            out.push_str("<em class=\"md-token md-italic\">");
            marker(&raw[..1], out);
            push_escaped(display, out);
            marker(&raw[raw.len() - 1..], out);
            out.push_str("</em>");
        }

        TokenKind::BoldItalic => {
            out.push_str("<strong class=\"md-token md-bold-italic\"><em>");
            marker(&raw[..3], out);
            push_escaped(display, out);
            marker(&raw[raw.len() - 3..], out);
            out.push_str("</em></strong>");
        }

        TokenKind::Code => {
            out.push_str("<code class=\"md-token md-code\">");
            marker("`", out);
            push_escaped(display, out);
            marker("`", out);
            out.push_str("</code>");
        }

        TokenKind::Strikethrough => {
            out.push_str("<s class=\"md-token md-strikethrough\">");
            marker("~~", out);
            push_escaped(display, out);
            marker("~~", out);
            out.push_str("</s>");
        }

        TokenKind::Heading(level) => {
            let prefix_len = raw.len() - display.len();
            let class = format!("md-token md-heading md-h{level}");
            out.push_str(&format!("<span class=\"{class}\">"));
            marker(&raw[..prefix_len], out);
            push_inline_html(source, token.content_range.clone(), out);
            out.push_str("</span>");
        }

        TokenKind::Blockquote => {
            let prefix_len = token.content_range.start - token.range.start;
            out.push_str("<span class=\"md-token md-blockquote\">");
            marker(&raw[..prefix_len], out);
            push_inline_html(source, token.content_range.clone(), out);
            out.push_str("</span>");
        }

        TokenKind::ListItem { ordered, depth } => {
            let prefix_len = token.content_range.start - token.range.start;
            let indent = format!("{}em", *depth as f32 * 1.5);
            out.push_str(&format!(
                "<span class=\"md-token md-list-item{}\" style=\"padding-left:{indent}\">",
                if *ordered { " md-list-ordered" } else { " md-list-unordered" }
            ));
            marker(&raw[..prefix_len], out);
            push_inline_html(source, token.content_range.clone(), out);
            out.push_str("</span>");
        }

        TokenKind::TaskItem { checked, depth, bracket_pos } => {
            let prefix_len = bracket_pos - token.range.start;
            let indent = format!("{}em", *depth as f32 * 1.5);
            let bracket_text = if *checked { "[x]" } else { "[ ]" };
            out.push_str(&format!(
                "<span class=\"md-token md-task-item\" style=\"padding-left:{indent}\">"
            ));
            marker(&raw[..prefix_len], out);
            out.push_str(&format!(
                "<span class=\"md-task-checkbox\" contenteditable=\"false\" \
                 data-pos=\"{}\" data-checked=\"{}\">{} </span>",
                bracket_pos,
                checked,
                bracket_text,
            ));
            push_inline_html(source, token.content_range.clone(), out);
            out.push_str("</span>");
        }

        TokenKind::HorizontalRule => {
            out.push_str("<span class=\"md-token md-hr\">");
            marker(raw, out);
            out.push_str("</span>");
        }

        TokenKind::Link { url_range } => {
            let url = &source[url_range.clone()];
            let url_escaped = escaped_attr(url);
            out.push_str(&format!(
                "<a class=\"md-token md-link\" href=\"{url_escaped}\" data-navigate=\"{url_escaped}\">"
            ));
            marker("[", out);
            push_escaped(display, out);
            out.push_str("<span class=\"md-marker\">](");
            push_escaped(url, out);
            out.push_str(")</span>");
            out.push_str("</a>");
        }

        TokenKind::WikiLink { target_range, display_range } => {
            let target = &source[target_range.clone()];
            let target_escaped = escaped_attr(target);
            out.push_str(&format!(
                "<span class=\"md-token md-wikilink md-wikilink--linked\" data-navigate=\"{target_escaped}\">"
            ));
            marker("[[", out);
            if display_range.is_some() {
                out.push_str("<span class=\"md-wikilink-target\">");
                push_escaped(target, out);
                out.push_str("</span>");
                marker("|", out);
                push_escaped(display, out);
            } else {
                push_escaped(display, out);
            }
            marker("]]", out);
            out.push_str("</span>");
        }

        TokenKind::Image { url_range } => {
            let url = &source[url_range.clone()];
            out.push_str("<span class=\"md-token md-image\">");
            marker("![", out);
            push_escaped(display, out);
            out.push_str("<span class=\"md-marker\">](");
            push_escaped(url, out);
            out.push_str(")</span></span>");
        }

        TokenKind::CodeFence { lang_range } => {
            out.push_str("<span class=\"md-token md-code-fence\">");
            marker("```", out);
            if let Some(lr) = lang_range {
                out.push_str("<span class=\"md-code-lang\">");
                push_escaped(&source[lr.clone()], out);
                out.push_str("</span>");
            }
            out.push_str("</span>");
        }

        TokenKind::CodeBlock => {
            out.push_str("<span class=\"md-token md-code-block\">");
            push_escaped(raw, out);
            out.push_str("</span>");
        }
    }
}

fn marker(text: &str, out: &mut String) {
    out.push_str("<span class=\"md-marker\">");
    push_escaped(text, out);
    out.push_str("</span>");
}

fn push_escaped(s: &str, out: &mut String) {
    for ch in s.chars() {
        push_escaped_char(ch, out);
    }
}

fn push_escaped_char(ch: char, out: &mut String) {
    match ch {
        '&' => out.push_str("&amp;"),
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        '"' => out.push_str("&quot;"),
        _ => out.push(ch),
    }
}

fn escaped_attr(s: &str) -> String {
    let mut out = String::new();
    push_escaped(s, &mut out);
    out
}

// ── Component ─────────────────────────────────────────────────────────────────

#[component]
pub fn MarkdownArea(
    mut content: Signal<String>,
    #[props(default)] variant: MarkdownAreaVariant,
    #[props(default)] placeholder: String,
    /// Called with the target note/URL when a WikiLink or Link is clicked.
    on_navigate: Option<EventHandler<String>>,
    onfocus: Option<EventHandler<FocusEvent>>,
    onblur: Option<EventHandler<FocusEvent>>,
) -> Element {
    let mut cursor_pos: Signal<Option<usize>> = use_signal(|| None);
    let id = use_memo(|| next_editor_id());

    let rendered_html = use_memo(move || {
        let src = content.read();
        let tokens = tokenize(&src);
        tokens_to_html(&src, &tokens)
    });

    use_effect(move || {
        document::eval(&js_setup_tasks(&id()));
        document::eval(&js_setup_selection(&id()));
    });

    let saved_cursor = cursor_pos();
    use_effect(move || {
        if let Some(pos) = saved_cursor {
            document::eval(&js_set_cursor(&id(), pos));
        }
    });

    let handle_input = move |_: Event<FormData>| {
        let editor_id = id();
        spawn(async move {
            if let Ok(payload) = document::eval(&js_read_state(&editor_id))
                .join::<String>()
                .await
            {
                if let Some((cursor_str, text)) = payload.split_once('\n') {
                    let cursor = cursor_str.parse::<i64>().ok()
                        .filter(|&c| c >= 0)
                        .map(|c| c as usize);
                    cursor_pos.set(cursor);
                    content.set(text.to_string());
                }
            }
        });
    };

    let handle_click = move || {
        let editor_id = id();
        spawn(async move {
            let Ok(payload) = document::eval(&js_read_state(&editor_id))
                .join::<String>()
                .await
            else {
                return;
            };

            if let Some(url) = payload.strip_prefix("nav:") {
                if let Some(cb) = on_navigate {
                    cb(url.to_string());
                }
                return;
            }

            if let Some(rest) = payload.strip_prefix("cb:") {
                if let Some((pos_str, was_checked_str)) = rest.split_once(':') {
                    if let Ok(bracket_pos) = pos_str.parse::<usize>() {
                        let was_checked = was_checked_str == "1";
                        let new_bracket = if was_checked { "[ ]" } else { "[x]" };
                        let mut src = content.read().clone();
                        if bracket_pos + 3 <= src.len() {
                            src.replace_range(bracket_pos..bracket_pos + 3, new_bracket);
                            content.set(src);
                        }
                    }
                }
            } else if let Some((cursor_str, _)) = payload.split_once('\n') {
                let cursor = cursor_str.parse::<i64>().ok()
                    .filter(|&c| c >= 0)
                    .map(|c| c as usize);
                cursor_pos.set(cursor);
            }
        });
    };

    let sync_cursor = move || {
        let editor_id = id();
        spawn(async move {
            if let Ok(payload) = document::eval(&js_read_state(&editor_id))
                .join::<String>()
                .await
            {
                if let Some((cursor_str, _)) = payload.split_once('\n') {
                    let cursor = cursor_str.parse::<i64>().ok()
                        .filter(|&c| c >= 0)
                        .map(|c| c as usize);
                    cursor_pos.set(cursor);
                }
            }
        });
    };

    rsx! {
        document::Link { rel: "stylesheet", href: asset!("./style.css") }
        div {
            id: "{id}",
            class: "md-area",
            "data-style": variant.class(),
            "data-placeholder": "{placeholder}",
            contenteditable: "true",
            spellcheck: "false",
            dangerous_inner_html: "{rendered_html}",
            oninput: handle_input,
            onclick: move |_: Event<MouseData>| handle_click(),
            onkeyup: move |_: Event<KeyboardData>| sync_cursor(),
            onfocus: move |e| { if let Some(cb) = onfocus { cb(e); } },
            onblur: move |e| {
                cursor_pos.set(None);
                if let Some(cb) = onblur { cb(e); }
            },
        }
    }
}
