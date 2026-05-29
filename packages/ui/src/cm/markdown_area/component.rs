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
            if (prev) {{
                // Sync data-checked from actual text before the line goes
                // inactive — the user may have typed [x]/[ ] directly.
                const cb = prev.querySelector('.md-task-checkbox');
                if (cb) {{
                    const t = cb.textContent;
                    cb.dataset.checked = (t.startsWith('[x]') || t.startsWith('[X]')) ? 'true' : 'false';
                }}
                prev.classList.remove('md-line--active');
            }}
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

// ── HTML rendering ────────────────────────────────────────────────────────────

fn tokens_to_html(source: &str, tokens: &[Token]) -> String {
    let mut out = String::with_capacity(source.len() * 3);
    let mut last_end = 0;

    out.push_str("<div class=\"md-line\">");

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

    out.push_str("</div>");
    out
}

fn emit_gap_html(source: &str, start: usize, end: usize, out: &mut String) {
    for ch in source[start..end].chars() {
        if ch == '\n' {
            // Close the current line div and open a new one.
            // Block divs create implicit line breaks; no <br> needed.
            out.push_str("</div><div class=\"md-line\">");
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
                "<span class=\"md-task-checkbox\" \
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

        TokenKind::TableRow { cells, is_separator } => {
            if *is_separator {
                // Render as an invisible line divider; raw text appears as marker when active.
                out.push_str("<span class=\"md-token md-table-sep\">");
                marker(raw, out);
                out.push_str("</span>");
            } else {
                out.push_str("<span class=\"md-token md-table-row\">");
                let base = token.range.start;
                let mut consumed = 0; // offset into raw
                for cell in cells {
                    // Emit everything up to the cell (includes leading pipe + space)
                    let up_to = cell.start - base;
                    for ch in raw[consumed..up_to].chars() {
                        if ch == '|' { marker("|", out); } else { push_escaped_char(ch, out); }
                    }
                    out.push_str("<span class=\"md-table-cell\">");
                    push_inline_html(source, cell.clone(), out);
                    out.push_str("</span>");
                    consumed = cell.end - base;
                }
                // Trailing pipe(s) and whitespace
                for ch in raw[consumed..].chars() {
                    if ch == '|' { marker("|", out); } else { push_escaped_char(ch, out); }
                }
                out.push_str("</span>");
            }
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
    let id = use_memo(|| next_editor_id());
    let mut is_focused = use_signal(|| false);

    // rendered_html is a manually-managed signal rather than a reactive memo.
    // We only push updates when the editor is NOT focused, so that typing never
    // replaces dangerous_inner_html under the user's cursor.
    let mut rendered_html = use_signal(|| {
        let src = content.peek();
        let tokens = tokenize(&src);
        tokens_to_html(&src, &tokens)
    });

    use_effect(move || {
        let src = content();     // subscribe to content changes
        if !is_focused() {       // also subscribe to focus changes
            let tokens = tokenize(&src);
            rendered_html.set(tokens_to_html(&src, &tokens));
        }
    });

    use_effect(move || {
        document::eval(&js_setup_tasks(&id()));
        document::eval(&js_setup_selection(&id()));
    });

    let handle_input = move |_: Event<FormData>| {
        let editor_id = id();
        spawn(async move {
            if let Ok(payload) = document::eval(&js_read_state(&editor_id))
                .recv::<String>()
                .await
            {
                // Payload is "cursor\ntext"; we only need the text.
                let text = payload.split_once('\n')
                    .map(|(_, t)| t)
                    .unwrap_or(&payload)
                    .to_string();
                content.set(text);
            }
        });
    };

    let handle_click = move || {
        let editor_id = id();
        spawn(async move {
            let Ok(payload) = document::eval(&js_read_state(&editor_id))
                .recv::<String>()
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
                    if let Ok(hint_pos) = pos_str.parse::<usize>() {
                        let was_checked = was_checked_str == "1";
                        let new_bracket = if was_checked { "[ ]" } else { "[x]" };
                        let mut src = content.read().clone();
                        // Re-tokenize current content to find the actual bracket
                        // position — the hint from data-pos may be stale if the
                        // user edited above this line while focused.
                        let tokens = tokenize(&src);
                        let actual_pos = tokens.iter()
                            .filter_map(|t| match &t.kind {
                                TokenKind::TaskItem { checked, bracket_pos, .. }
                                    if *checked == was_checked => Some(*bracket_pos),
                                _ => None,
                            })
                            .min_by_key(|&p| p.abs_diff(hint_pos));
                        if let Some(pos) = actual_pos {
                            if pos + 3 <= src.len() {
                                src.replace_range(pos..pos + 3, new_bracket);
                                // Update rendered_html immediately so the toggle
                                // is visible without waiting for blur — the
                                // use_effect guard skips updates while focused.
                                let new_html = tokens_to_html(&src, &tokenize(&src));
                                rendered_html.set(new_html);
                                content.set(src);
                            }
                        }
                    }
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
            onfocus: move |e| {
                is_focused.set(true);
                if let Some(cb) = onfocus { cb(e); }
            },
            onblur: move |e| {
                is_focused.set(false);
                if let Some(cb) = onblur { cb(e); }
            },
        }
    }
}
