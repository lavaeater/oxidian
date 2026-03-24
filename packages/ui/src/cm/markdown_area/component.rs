use dioxus::prelude::*;

use super::tokenizer::{tokenize, Token, TokenKind};

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

/// JavaScript helper: reads cursor offset within the contenteditable element.
/// Walks text nodes and computes a flat character offset.
const JS_GET_CURSOR: &str = r#"
(function() {
    const el = document.querySelector('[data-slot="markdown-area"]');
    if (!el) return -1;
    const sel = window.getSelection();
    if (!sel || sel.rangeCount === 0) return -1;
    const range = sel.getRangeAt(0);
    let offset = 0;
    const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT, null);
    while (walker.nextNode()) {
        if (walker.currentNode === range.startContainer) {
            return offset + range.startOffset;
        }
        offset += walker.currentNode.textContent.length;
    }
    return -1;
})()
"#;

/// JavaScript helper: sets cursor to a flat character offset inside the
/// contenteditable element.
const JS_SET_CURSOR: &str = r#"
(function(targetOffset) {
    const el = document.querySelector('[data-slot="markdown-area"]');
    if (!el) return;
    let offset = 0;
    const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT, null);
    while (walker.nextNode()) {
        const node = walker.currentNode;
        const nodeLen = node.textContent.length;
        if (offset + nodeLen >= targetOffset) {
            const sel = window.getSelection();
            const range = document.createRange();
            range.setStart(node, targetOffset - offset);
            range.collapse(true);
            sel.removeAllRanges();
            sel.addRange(range);
            return;
        }
        offset += nodeLen;
    }
})
"#;

/// JavaScript helper: reads the full text content from the contenteditable div.
const JS_GET_TEXT: &str = r#"
(function() {
    const el = document.querySelector('[data-slot="markdown-area"]');
    if (!el) return "";
    return el.innerText;
})()
"#;

/// Build a display-offset → source-offset mapping for a given token list and
/// source string. This is needed because formatted tokens hide their markers,
/// so the character positions in the DOM differ from positions in the source.
fn build_offset_map(source: &str, tokens: &[Token], cursor_source_pos: Option<usize>) -> Vec<usize> {
    let mut map: Vec<usize> = Vec::new();
    let mut last_source_end = 0;

    for token in tokens {
        // Gap between previous token end and this token start (newlines, etc.)
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
            // Show raw text — display positions map 1:1 to source positions
            let raw = token.raw(source);
            for (i, _) in raw.char_indices() {
                map.push(token.range.start + i);
            }
        } else {
            // Show formatted (display) text only
            let display = token.display(source);
            for (i, _) in display.char_indices() {
                map.push(token.content_range.start + i);
            }
        }

        last_source_end = token.range.end;
    }

    // Trailing text after last token
    if last_source_end < source.len() {
        let tail = &source[last_source_end..];
        for (i, _) in tail.char_indices() {
            map.push(last_source_end + i);
        }
    }

    // Sentinel for end-of-content cursor position
    map.push(source.len());
    map
}

/// Convert a source offset to a display offset using the offset map.
fn source_to_display_offset(map: &[usize], source_offset: usize) -> usize {
    map.iter()
        .position(|&s| s >= source_offset)
        .unwrap_or(map.len().saturating_sub(1))
}

/// Convert a display offset to a source offset using the offset map.
fn display_to_source_offset(map: &[usize], display_offset: usize) -> usize {
    if display_offset < map.len() {
        map[display_offset]
    } else {
        *map.last().unwrap_or(&0)
    }
}

#[component]
pub fn MarkdownArea(
    mut content: Signal<String>,
    #[props(default)] variant: MarkdownAreaVariant,
    onfocus: Option<EventHandler<FocusEvent>>,
    onblur: Option<EventHandler<FocusEvent>>,
) -> Element {
    let mut cursor_pos = use_signal(|| None::<usize>);
    let mut is_focused = use_signal(|| false);

    let source = content.read().clone();
    let tokens = use_memo(move || tokenize(&content.read()));

    let current_cursor = cursor_pos();
    let current_tokens = tokens();

    // Build offset map for cursor translation
    let offset_map = use_memo(move || {
        build_offset_map(&content.read(), &tokens.read(), cursor_pos())
    });

    // After every render, restore the cursor position if the editor is focused
    let restore_cursor = cursor_pos();
    use_effect(move || {
        if let Some(src_pos) = restore_cursor {
            let map = offset_map.read();
            let display_pos = source_to_display_offset(&map, src_pos);
            spawn(async move {
                let js = format!("{}({})", JS_SET_CURSOR, display_pos);
                _ = document::eval(&js).join::<()>().await;
            });
        }
    });

    // Read cursor position from the DOM and map to source offset
    let sync_cursor = move || {
        let map = offset_map.read().clone();
        spawn(async move {
            if let Ok(display_off) = document::eval(JS_GET_CURSOR).join::<i64>().await {
                if display_off >= 0 {
                    let d = display_off as usize;
                    let src_off = display_to_source_offset(&map, d);
                    cursor_pos.set(Some(src_off));
                }
            }
        });
    };

    // Handle text input: read new text + cursor from DOM
    let handle_input = move |_evt: Event<FormData>| {
        spawn(async move {
            // Read text from DOM
            if let Ok(new_text) = document::eval(JS_GET_TEXT).join::<String>().await {
                // Read cursor before updating content (which triggers re-render)
                let cursor_display = document::eval(JS_GET_CURSOR).join::<i64>().await.ok();

                // Build map for the *new* text with no active token to get raw mapping
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
            class: "md-area",
            "data-slot": "markdown-area",
            "data-style": variant.class(),
            contenteditable: "true",
            spellcheck: "false",
            oninput: handle_input,
            onclick: move |_: Event<MouseData>| sync_cursor(),
            onkeyup: move |_: Event<KeyboardData>| sync_cursor(),
            onfocus: move |e| {
                is_focused.set(true);
                if let Some(cb) = onfocus { cb(e); }
            },
            onblur: move |e| {
                is_focused.set(false);
                cursor_pos.set(None);
                if let Some(cb) = onblur { cb(e); }
            },
            // Render tokens
            {render_tokens(&source, &current_tokens, current_cursor)}
        }
    }
}

fn render_tokens(source: &str, tokens: &[Token], cursor_pos: Option<usize>) -> Element {
    let mut last_end = 0;
    let mut elements: Vec<Element> = Vec::new();

    for token in tokens {
        // Render any gap (newlines between lines)
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
                // Remove trailing br since split produces an empty after last \n
                elements.pop();
            } else if !gap.is_empty() {
                let g = gap.to_string();
                elements.push(rsx! { span { "{g}" } });
            }
        }

        let is_active = cursor_pos
            .map(|cp| token.contains(cp))
            .unwrap_or(false);

        elements.push(render_single_token(source, token, is_active));
        last_end = token.range.end;
    }

    // Trailing text
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
        for (_i, el) in elements.into_iter().enumerate() {
            {el}
        }
    }
}

fn render_single_token(source: &str, token: &Token, is_active: bool) -> Element {
    if is_active {
        // Show raw markdown text with a subtle marker highlight
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
            let class = format!("md-token md-heading md-h{}", level);
            rsx! {
                span { class: "{class}", "{display}" }
            }
        }
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
    }
}
