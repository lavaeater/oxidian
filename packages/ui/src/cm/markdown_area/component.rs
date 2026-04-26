use dioxus::prelude::*;

use super::tokenizer::{tokenize, Token, TokenKind};

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

// ── JS helpers (scoped to element ID) ────────────────────────────────────────

fn js_get_cursor(id: &str) -> String {
    format!(
        r#"(function() {{
    const el = document.getElementById({id:?});
    if (!el) return -1;
    const sel = window.getSelection();
    if (!sel || sel.rangeCount === 0) return -1;
    const range = sel.getRangeAt(0);
    if (!el.contains(range.startContainer)) return -1;
    let offset = 0;
    const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT, null);
    while (walker.nextNode()) {{
        if (walker.currentNode === range.startContainer) {{
            return offset + range.startOffset;
        }}
        offset += walker.currentNode.textContent.length;
    }}
    return -1;
}})()"#
    )
}

fn js_get_text(id: &str) -> String {
    format!(
        r#"(function() {{
    const el = document.getElementById({id:?});
    if (!el) return "";
    return el.innerText;
}})()"#
    )
}

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

// ── Offset-map helpers ────────────────────────────────────────────────────────

/// Build a display-offset → source-offset mapping.
/// Needed because formatted tokens hide their markers, so DOM character
/// positions differ from source positions.
fn build_offset_map(source: &str, tokens: &[Token], cursor_source_pos: Option<usize>) -> Vec<usize> {
    let mut map: Vec<usize> = Vec::new();
    let mut last_source_end = 0;

    for token in tokens {
        if token.range.start > last_source_end {
            let gap = &source[last_source_end..token.range.start];
            for (i, _) in gap.char_indices() {
                map.push(last_source_end + i);
            }
        }

        let is_active = cursor_source_pos
            .map(|cp| token.contains(cp))
            .unwrap_or(false);

        if is_active || token.kind == TokenKind::Plain {
            let raw = token.raw(source);
            for (i, _) in raw.char_indices() {
                map.push(token.range.start + i);
            }
        } else {
            let display = token.display(source);
            for (i, _) in display.char_indices() {
                map.push(token.content_range.start + i);
            }
        }

        last_source_end = token.range.end;
    }

    if last_source_end < source.len() {
        let tail = &source[last_source_end..];
        for (i, _) in tail.char_indices() {
            map.push(last_source_end + i);
        }
    }

    map.push(source.len());
    map
}

fn source_to_display_offset(map: &[usize], source_offset: usize) -> usize {
    map.iter()
        .position(|&s| s >= source_offset)
        .unwrap_or(map.len().saturating_sub(1))
}

fn display_to_source_offset(map: &[usize], display_offset: usize) -> usize {
    if display_offset < map.len() {
        map[display_offset]
    } else {
        *map.last().unwrap_or(&0)
    }
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
    let mut cursor_pos = use_signal(|| None::<usize>);
    let id = use_memo(|| next_editor_id());

    let source = content.read().clone();
    let tokens = use_memo(move || tokenize(&content.read()));

    let current_cursor = cursor_pos();
    let current_tokens = tokens();

    let offset_map = use_memo(move || {
        build_offset_map(&content.read(), &tokens.read(), cursor_pos())
    });

    // Restore cursor after re-render.
    let restore_cursor = cursor_pos();
    use_effect(move || {
        if let Some(src_pos) = restore_cursor {
            let display_pos = source_to_display_offset(&offset_map.read(), src_pos);
            let js = js_set_cursor(&id(), display_pos);
            spawn(async move {
                _ = document::eval(&js).join::<()>().await;
            });
        }
    });

    let sync_cursor = move || {
        let map = offset_map.read().clone();
        let editor_id = id();
        spawn(async move {
            if let Ok(d) = document::eval(&js_get_cursor(&editor_id)).join::<i64>().await {
                if d >= 0 {
                    let src = display_to_source_offset(&map, d as usize);
                    cursor_pos.set(Some(src));
                }
            }
        });
    };

    let handle_input = move |_: Event<FormData>| {
        let editor_id = id();
        spawn(async move {
            if let Ok(new_text) = document::eval(&js_get_text(&editor_id)).join::<String>().await {
                let cursor_display = document::eval(&js_get_cursor(&editor_id))
                    .join::<i64>()
                    .await
                    .ok();
                let new_tokens = tokenize(&new_text);
                let new_map = build_offset_map(&new_text, &new_tokens, None);
                let new_cursor = cursor_display
                    .filter(|&d| d >= 0)
                    .map(|d| display_to_source_offset(&new_map, d as usize));
                *content.write() = new_text;
                cursor_pos.set(new_cursor);
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
            oninput: handle_input,
            onclick: move |_: Event<MouseData>| sync_cursor(),
            onkeyup: move |_: Event<KeyboardData>| sync_cursor(),
            onfocus: move |e| {
                if let Some(cb) = onfocus { cb(e); }
            },
            onblur: move |e| {
                cursor_pos.set(None);
                if let Some(cb) = onblur { cb(e); }
            },
            {render_tokens(&source, &current_tokens, current_cursor, on_navigate)}
        }
    }
}

// ── Rendering ─────────────────────────────────────────────────────────────────

fn render_tokens(
    source: &str,
    tokens: &[Token],
    cursor_pos: Option<usize>,
    on_navigate: Option<EventHandler<String>>,
) -> Element {
    let mut last_end = 0;
    let mut elements: Vec<Element> = Vec::new();

    for token in tokens {
        // Gaps between tokens (newlines become <br>)
        if token.range.start > last_end {
            let gap = &source[last_end..token.range.start];
            if gap.contains('\n') {
                for part in gap.split('\n') {
                    if !part.is_empty() {
                        let p = part.to_string();
                        elements.push(rsx! { span { "{p}" } });
                    }
                    elements.push(rsx! { br {} });
                }
                elements.pop(); // split('\n') produces one extra trailing entry
            } else if !gap.is_empty() {
                let g = gap.to_string();
                elements.push(rsx! { span { "{g}" } });
            }
        }

        let is_active = cursor_pos.map(|cp| token.contains(cp)).unwrap_or(false);
        elements.push(render_single_token(source, token, is_active, on_navigate));
        last_end = token.range.end;
    }

    // Trailing text after the last token
    if last_end < source.len() {
        let tail = &source[last_end..];
        if tail.contains('\n') {
            for part in tail.split('\n') {
                if !part.is_empty() {
                    let p = part.to_string();
                    elements.push(rsx! { span { "{p}" } });
                }
                elements.push(rsx! { br {} });
            }
            elements.pop();
        } else if !tail.is_empty() {
            let t = tail.to_string();
            elements.push(rsx! { span { "{t}" } });
        }
    }

    rsx! {
        for el in elements {
            {el}
        }
    }
}

fn render_single_token(
    source: &str,
    token: &Token,
    is_active: bool,
    on_navigate: Option<EventHandler<String>>,
) -> Element {
    if is_active {
        let raw = token.raw(source).to_string();
        let class = format!("md-token md-active md-{}", kind_class(&token.kind));
        return rsx! {
            span { class: "{class}", "{raw}" }
        };
    }

    let display = token.display(source).to_string();
    match &token.kind {
        TokenKind::Plain => rsx! {
            span { class: "md-token md-plain", "{display}" }
        },
        TokenKind::Bold => rsx! {
            strong { class: "md-token md-bold", "{display}" }
        },
        TokenKind::Italic => rsx! {
            em { class: "md-token md-italic", "{display}" }
        },
        TokenKind::BoldItalic => rsx! {
            strong { class: "md-token md-bold-italic",
                em { "{display}" }
            }
        },
        TokenKind::Code => rsx! {
            code { class: "md-token md-code", "{display}" }
        },
        TokenKind::Strikethrough => rsx! {
            s { class: "md-token md-strikethrough", "{display}" }
        },
        TokenKind::Heading(level) => {
            let class = format!("md-token md-heading md-h{level}");
            rsx! { span { class: "{class}", "{display}" } }
        }

        // ── New inline types ──────────────────────────────────────────────

        TokenKind::Link { url_range } => {
            let url = source[url_range.clone()].to_string();
            if let Some(nav) = on_navigate {
                rsx! {
                    a {
                        class: "md-token md-link",
                        href: "{url}",
                        onclick: move |e: Event<MouseData>| {
                            e.prevent_default();
                            nav.call(url.clone());
                        },
                        "{display}"
                    }
                }
            } else {
                rsx! { a { class: "md-token md-link", href: "{url}", "{display}" } }
            }
        }

        TokenKind::WikiLink { target_range, .. } => {
            let target = source[target_range.clone()].to_string();
            if let Some(nav) = on_navigate {
                rsx! {
                    span {
                        class: "md-token md-wikilink md-wikilink--linked",
                        onclick: move |_| nav.call(target.clone()),
                        "{display}"
                    }
                }
            } else {
                rsx! { span { class: "md-token md-wikilink", "{display}" } }
            }
        }

        TokenKind::Image { url_range } => {
            let url = source[url_range.clone()].to_string();
            rsx! {
                span { class: "md-token md-image", contenteditable: "false",
                    img { src: "{url}", alt: "{display}", draggable: "false", class: "md-image-img" }
                }
            }
        }

        // ── Block types ───────────────────────────────────────────────────

        TokenKind::Blockquote => rsx! {
            span { class: "md-token md-blockquote",
                span { class: "md-bq-bar", aria_hidden: "true" }
                "{display}"
            }
        },

        TokenKind::ListItem { ordered, depth } => {
            let indent = format!("{}em", *depth as f32 * 1.5);
            let marker = if *ordered { "1.".to_string() } else { "•".to_string() };
            rsx! {
                span { class: "md-token md-list-item", style: "padding-left: {indent}",
                    span { class: "md-li-marker", aria_hidden: "true", "{marker} " }
                    "{display}"
                }
            }
        }

        TokenKind::HorizontalRule => rsx! {
            span { class: "md-token md-hr", role: "separator", aria_hidden: "true" }
        },
    }
}

fn kind_class(kind: &TokenKind) -> &'static str {
    match kind {
        TokenKind::Plain => "plain",
        TokenKind::Bold => "bold",
        TokenKind::Italic => "italic",
        TokenKind::BoldItalic => "bold-italic",
        TokenKind::Code => "code",
        TokenKind::Strikethrough => "strikethrough",
        TokenKind::Heading(_) => "heading",
        TokenKind::Link { .. } => "link",
        TokenKind::WikiLink { .. } => "wikilink",
        TokenKind::Image { .. } => "image",
        TokenKind::Blockquote => "blockquote",
        TokenKind::ListItem { .. } => "list-item",
        TokenKind::HorizontalRule => "hr",
    }
}
