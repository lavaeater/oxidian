use dioxus::prelude::*;
use dioxus_use_js::use_js;

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

// ── JS bindings ───────────────────────────────────────────────────────────────

// The editor's DOM glue lives in `assets/markdown_area.js`. `use_js!` turns each
// exported function into an async Rust fn (returning `Result<_, JsError>`) and
// serializes arguments across the boundary — replacing the old `format!`-built
// eval strings and the hand-rolled `js_str` escaper.
use_js!("assets/markdown_area.js"::{
    setup_tasks,
    setup_selection,
    setup_keyboard,
    read_state,
    apply_html_and_restore_cursor
});

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
        let editor_id = id();
        spawn(async move {
            // Register the editor's DOM listeners. Pure-JS bindings return a
            // generic `T: DeserializeOwned`, so the `Result<(), _>` annotation
            // pins it to the unit type.
            let _: Result<(), _> = setup_tasks(&editor_id).await;
            let _: Result<(), _> = setup_selection(&editor_id).await;
            let _: Result<(), _> = setup_keyboard(&editor_id).await;
        });
    });

    let handle_input = move |_: Event<FormData>| {
        let editor_id = id();
        spawn(async move {
            let payload: Result<String, _> = read_state(&editor_id).await;
            let payload = match payload {
                Ok(p) => {
                    log::info!("[oxidian] read_state(input): {} bytes", p.len());
                    p
                }
                Err(e) => {
                    log::info!("[oxidian] read_state(input) ERROR: {e:?}");
                    return;
                }
            };

            if let Some(rest) = payload.strip_prefix("linechange\n") {
                // Active line changed: re-render so block tokens (headings,
                // lists, …) reformat immediately.
                // We set innerHTML directly + restore cursor in one synchronous
                // JS call to avoid a race with Dioxus's own render cycle.
                // rendered_html is intentionally left alone — the use_effect
                // will sync it on the next blur.
                let (cursor_str, text) = rest.split_once('\n').unwrap_or(("-1", rest));
                let cursor: i64 = cursor_str.parse().unwrap_or(-1);
                // Chrome appends a trailing \n to innerText of contenteditable
                // divs; strip it to avoid accumulating blank lines.
                let text = text.trim_end_matches('\n').to_string();
                content.set(text.clone());
                // cursor = -1 means the selection is in an element with no
                // text nodes (e.g. a freshly Enter-created empty line).
                // Skip the re-render in that case — the DOM is untouched so
                // the cursor stays put, and formatting syncs on blur.
                if cursor >= 0 {
                    let tokens = tokenize(&text);
                    let new_html = tokens_to_html(&text, &tokens);
                    let _: Result<(), _> =
                        apply_html_and_restore_cursor(&editor_id, &new_html, cursor).await;
                }
            } else {
                // Normal keystroke: update content only; rendered_html stays
                // untouched while focused to avoid resetting the cursor.
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
            let payload: Result<String, _> = read_state(&editor_id).await;
            let Ok(payload) = payload else {
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
