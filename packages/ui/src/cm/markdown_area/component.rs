use std::fmt::Write as _;

use dioxus::prelude::*;
use dioxus_use_js::use_js;

use super::tokenizer::{Token, TokenKind, tokenize, tokenize_line};

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
    setup_scroll,
    read_state,
    read_click,
    apply_html_and_restore_cursor
});

// ── Rendered blocks ───────────────────────────────────────────────────────────

/// Turns a fenced block into HTML. Called with `(language, body)`; returning an
/// empty string means "not mine" and the block stays plain source.
///
/// This is a callback rather than something `ui` does itself because the
/// renderers live above us — `dataview` needs the vault index, which this crate
/// cannot depend on.
///
/// A plain `Rc<dyn Fn>` rather than a Dioxus `Callback` so that rendering stays
/// callable from pure functions, with no Dioxus runtime in scope.
type RenderFn = dyn Fn(&str, &str) -> String;

#[derive(Clone)]
pub struct BlockRenderer(std::rc::Rc<RenderFn>);

impl BlockRenderer {
    pub fn new(f: impl Fn(&str, &str) -> String + 'static) -> Self {
        Self(std::rc::Rc::new(f))
    }

    fn call(&self, lang: &str, body: &str) -> String {
        (self.0)(lang, body)
    }
}

impl PartialEq for BlockRenderer {
    /// Identity, so a renderer built once (`use_hook`) doesn't re-render the
    /// editor on every pass. Building a fresh one each render would.
    fn eq(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.0, &other.0)
    }
}

// ── Link resolution ───────────────────────────────────────────────────────────

/// Answers "does this wikilink target exist?" for the editor.
///
/// Like [`BlockRenderer`], this is a callback because the answer lives above
/// this crate: only the host knows the vault's file list and its path rules.
/// Without one, every wikilink renders as linked — the previous behaviour.
type ResolveFn = dyn Fn(&str) -> bool;

#[derive(Clone)]
pub struct LinkResolver(std::rc::Rc<ResolveFn>);

impl LinkResolver {
    pub fn new(f: impl Fn(&str) -> bool + 'static) -> Self {
        Self(std::rc::Rc::new(f))
    }

    fn exists(&self, target: &str) -> bool {
        (self.0)(target)
    }
}

impl PartialEq for LinkResolver {
    /// Identity, for the same reason as [`BlockRenderer`]: a resolver built once
    /// must not re-render the editor on every pass.
    fn eq(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.0, &other.0)
    }
}

/// A fenced block currently shown as rendered output instead of source.
struct RenderedBlock {
    /// Byte range of the whole block, opening fence line through closing fence.
    range: std::ops::Range<usize>,
    html: String,
}

/// Which fenced blocks render, given where the caret is.
///
/// A block containing the caret is left as source — that is the editor's whole
/// premise, applied to a block instead of a line: you see the output until you
/// go in to edit it.
fn rendered_blocks(
    source: &str,
    tokens: &[Token],
    cursor: Option<usize>,
    renderer: Option<&BlockRenderer>,
) -> Vec<RenderedBlock> {
    let Some(renderer) = renderer else {
        return Vec::new();
    };
    let mut blocks = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        let TokenKind::CodeFence { lang_range: Some(lang) } = &token.kind else {
            continue;
        };
        // The matching closing fence. An unterminated block is still being
        // typed, so it renders as nothing at all.
        let Some(close) = tokens[i + 1..]
            .iter()
            .find(|t| matches!(t.kind, TokenKind::CodeFence { .. }))
        else {
            continue;
        };
        let range = token.range.start..close.range.end;
        if cursor.is_some_and(|c| range.contains(&c) || c == range.end) {
            continue;
        }
        // Body: everything between the two fence lines, without the newline
        // that ends the opening fence.
        let body_start = (token.range.end + 1).min(close.range.start);
        let body = source[body_start..close.range.start].trim_end_matches('\n');
        let html = renderer.call(source[lang.clone()].trim(), body);
        if !html.is_empty() {
            blocks.push(RenderedBlock { range, html });
        }
    }
    blocks
}

/// The class for the `.md-line` starting at `pos`.
///
/// The source lines of a rendered block stay in the DOM — they are what the
/// document model reads back — but are hidden. The block's *first* line hosts
/// the rendered output, so only its tokens are hidden, not the line itself.
fn line_class(blocks: &[RenderedBlock], pos: usize) -> &'static str {
    for b in blocks {
        if pos == b.range.start {
            return " md-render-host";
        }
        if pos > b.range.start && pos < b.range.end {
            return " md-render-hidden";
        }
    }
    ""
}

// ── HTML rendering ────────────────────────────────────────────────────────────

/// The editor's full HTML for `source`, with fenced blocks rendered through
/// `renderer` unless the caret (`cursor`) is inside them.
fn render_html(
    source: &str,
    cursor: Option<usize>,
    renderer: Option<&BlockRenderer>,
    links: Option<&LinkResolver>,
) -> String {
    let tokens = tokenize(source);
    let blocks = rendered_blocks(source, &tokens, cursor, renderer);
    tokens_to_html(source, &tokens, &blocks, links)
}

fn tokens_to_html(
    source: &str,
    tokens: &[Token],
    blocks: &[RenderedBlock],
    links: Option<&LinkResolver>,
) -> String {
    let mut out = String::with_capacity(source.len() * 3);
    let mut last_end = 0;

    let _ = write!(out, "<div class=\"md-line{}\">", line_class(blocks, 0));

    for token in tokens {
        if token.range.start > last_end {
            emit_gap_html(source, last_end, token.range.start, blocks, &mut out);
        }
        push_token_html(source, token, links, &mut out);
        // The rendered output lives inside the block's first line, after the
        // (hidden) opening fence, so it sits exactly where the block does.
        if let Some(b) = blocks.iter().find(|b| b.range.start == token.range.start) {
            let _ = write!(
                out,
                "<div class=\"md-render\" data-md-render data-edit-offset=\"{}\">{}</div>",
                b.range.start, b.html
            );
        }
        last_end = token.range.end;
    }

    if last_end < source.len() {
        emit_gap_html(source, last_end, source.len(), blocks, &mut out);
    }

    out.push_str("</div>");
    out
}

fn emit_gap_html(
    source: &str,
    start: usize,
    end: usize,
    blocks: &[RenderedBlock],
    out: &mut String,
) {
    for (i, ch) in source[start..end].char_indices() {
        if ch == '\n' {
            // Close the current line div and open a new one.
            // Block divs create implicit line breaks; no <br> needed.
            out.push_str("</div><div class=\"md-line");
            out.push_str(line_class(blocks, start + i + 1));
            out.push_str("\">");
        } else {
            push_escaped_char(ch, out);
        }
    }
}

// Tokenizes a block token's content range for inline formatting, then renders each.
fn push_inline_html(
    source: &str,
    content_range: std::ops::Range<usize>,
    links: Option<&LinkResolver>,
    out: &mut String,
) {
    let content = &source[content_range.clone()];
    let inline_tokens = tokenize_line(content, content_range.start);
    for token in &inline_tokens {
        push_token_html(source, token, links, out);
    }
}

#[allow(clippy::too_many_lines)]
fn push_token_html(
    source: &str,
    token: &Token,
    links: Option<&LinkResolver>,
    out: &mut String,
) {
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
            let _ = write!(out, "<span class=\"{class}\">");
            marker(&raw[..prefix_len], out);
            push_inline_html(source, token.content_range.clone(), links, out);
            out.push_str("</span>");
        }

        TokenKind::Blockquote => {
            let prefix_len = token.content_range.start - token.range.start;
            out.push_str("<span class=\"md-token md-blockquote\">");
            marker(&raw[..prefix_len], out);
            push_inline_html(source, token.content_range.clone(), links, out);
            out.push_str("</span>");
        }

        TokenKind::ListItem { ordered, depth } => {
            let prefix_len = token.content_range.start - token.range.start;
            let indent = format!("{}em", f32::from(*depth) * 1.5);
            let _ = write!(
                out,
                "<span class=\"md-token md-list-item{}\" style=\"padding-left:{indent}\">",
                if *ordered {
                    " md-list-ordered"
                } else {
                    " md-list-unordered"
                }
            );
            marker(&raw[..prefix_len], out);
            push_inline_html(source, token.content_range.clone(), links, out);
            out.push_str("</span>");
        }

        TokenKind::TaskItem {
            checked,
            depth,
            bracket_pos,
        } => {
            let prefix_len = bracket_pos - token.range.start;
            let indent = format!("{}em", f32::from(*depth) * 1.5);
            let bracket_text = if *checked { "[x]" } else { "[ ]" };
            let _ = write!(
                out,
                "<span class=\"md-token md-task-item\" style=\"padding-left:{indent}\">"
            );
            marker(&raw[..prefix_len], out);
            let _ = write!(
                out,
                "<span class=\"md-task-checkbox\" \
                 data-pos=\"{bracket_pos}\" data-checked=\"{checked}\">{bracket_text} </span>",
            );
            push_inline_html(source, token.content_range.clone(), links, out);
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
            let _ = write!(
                out,
                "<a class=\"md-token md-link\" href=\"{url_escaped}\" data-navigate=\"{url_escaped}\">"
            );
            marker("[", out);
            push_escaped(display, out);
            out.push_str("<span class=\"md-marker\">](");
            push_escaped(url, out);
            out.push_str(")</span>");
            out.push_str("</a>");
        }

        TokenKind::WikiLink {
            target_range,
            display_range,
        } => {
            let target = &source[target_range.clone()];
            let target_escaped = escaped_attr(target);
            // Unresolved links carry no `data-navigate`: there is nothing to
            // navigate to, and leaving it off means a click falls through to the
            // editor's normal "show me the source" handling. The offer to create
            // the note is an explicit button instead, so a stray click on the
            // link text never writes to the vault.
            let exists = links.is_none_or(|r| r.exists(target));
            if exists {
                let _ = write!(
                    out,
                    "<span class=\"md-token md-wikilink md-wikilink--linked\" data-navigate=\"{target_escaped}\">"
                );
            } else {
                out.push_str("<span class=\"md-token md-wikilink md-wikilink--missing\">");
            }
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
            if !exists {
                // `data-action`, the same channel the dataview blocks use: the
                // editor doesn't know what creating a note means, the host does.
                let _ = write!(
                    out,
                    "<span class=\"md-wikilink-create\" data-action=\"newnote:{target_escaped}\" \
                     title=\"Create note\" contenteditable=\"false\">Create note</span>"
                );
            }
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

        TokenKind::TableRow {
            cells,
            is_separator,
        } => {
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
                        if ch == '|' {
                            marker("|", out);
                        } else {
                            push_escaped_char(ch, out);
                        }
                    }
                    out.push_str("<span class=\"md-table-cell\">");
                    push_inline_html(source, cell.clone(), links, out);
                    out.push_str("</span>");
                    consumed = cell.end - base;
                }
                // Trailing pipe(s) and whitespace
                for ch in raw[consumed..].chars() {
                    if ch == '|' {
                        marker("|", out);
                    } else {
                        push_escaped_char(ch, out);
                    }
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
    /// Called with the target note/URL when a `WikiLink` or Link is clicked.
    on_navigate: Option<EventHandler<String>>,
    /// Called when something inside rendered output is clicked, with that
    /// element's `data-action` payload. The editor doesn't interpret it: the
    /// same crate that rendered the block decides what its actions mean.
    on_block_action: Option<EventHandler<String>>,
    /// Renders fenced code blocks (` ```dataview `, …) as output. See
    /// [`BlockRenderer`]; without one, every fence stays plain source.
    /// (`ReadSignal` only so it stays `Copy` — the editor's event handlers
    /// are `FnMut` closures that each need it.)
    render_block: ReadSignal<Option<BlockRenderer>>,
    /// Decides which `[[wikilinks]]` point at notes that exist. Unresolved ones
    /// render as missing and offer to create the note. See [`LinkResolver`];
    /// without one every link renders as linked.
    #[props(default)]
    resolve_link: ReadSignal<Option<LinkResolver>>,
    onfocus: Option<EventHandler<FocusEvent>>,
    onblur: Option<EventHandler<FocusEvent>>,
) -> Element {
    let id = use_memo(next_editor_id);
    let mut is_focused = use_signal(|| false);

    // rendered_html is a manually-managed signal rather than a reactive memo.
    // We only push updates when the editor is NOT focused, so that typing never
    // replaces dangerous_inner_html under the user's cursor.
    let mut rendered_html =
        use_signal(|| render_html(&content.peek(), None, render_block.read().as_ref(), resolve_link.read().as_ref()));

    use_effect(move || {
        let src = content(); // subscribe to content changes
        // Subscribing to the renderer too: a caller that swaps it in (because
        // the data behind it changed) means the same source renders differently
        // and the block has to be repainted.
        let renderer = render_block();
        // Same for the link resolver: a new one means the vault's file list
        // changed, so a link that was missing may now resolve (and stop
        // offering to create the note it just created).
        let resolver = resolve_link();
        if !is_focused() {
            // also subscribe to focus changes
            // No caret in the editor, so every renderable block renders.
            rendered_html.set(render_html(&src, None, renderer.as_ref(), resolver.as_ref()));
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
            let _: Result<(), _> = setup_scroll(&editor_id).await;
        });
    });

    let handle_input = move |_: Event<FormData>| {
        let editor_id = id();
        spawn(async move {
            let payload: Result<String, _> = read_state(&editor_id).await;
            let payload = match payload {
                Ok(p) => p,
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
                // `text` now comes from `lineTextAndCursor` (one '\n' per line
                // boundary), so a trailing '\n' is a real empty last line the
                // caret may sit on — don't strip it, or that line (and its
                // caret target) disappears on re-render.
                let text = text.to_string();
                content.set(text.clone());
                // cursor = -1 only when there is no caret in the editor; with
                // line-deterministic offsets even empty/blank lines get a real
                // offset, so leaving a block now re-renders on mobile too.
                if let Ok(cursor) = usize::try_from(cursor) {
                    let new_html =
                        render_html(&text, Some(cursor), render_block.read().as_ref(), resolve_link.read().as_ref());
                    let _: Result<(), _> =
                        apply_html_and_restore_cursor(&editor_id, &new_html, cursor).await;
                }
            } else {
                // Normal keystroke: update content only; rendered_html stays
                // untouched while focused to avoid resetting the cursor.
                let text = payload
                    .split_once('\n')
                    .map_or(payload.as_str(), |(_, t)| t)
                    .to_string();
                content.set(text);
            }
        });
    };

    let handle_click = move || {
        let editor_id = id();
        spawn(async move {
            // `read_click`, not `read_state`: both are destructive reads and a
            // click arrives alongside a selectionchange, so sharing one would
            // let this handler eat the input handler's re-render.
            let payload: Result<String, _> = read_click(&editor_id).await;
            let Ok(payload) = payload else {
                return;
            };

            // A wikilink click is terminal; anything else falls through to the
            // checkbox handler below. Do NOT fold this into a let-chain with
            // `on_navigate` — a nav click with no handler must still stop here.
            if let Some(url) = payload.strip_prefix("nav:") {
                if let Some(cb) = on_navigate {
                    cb(url.to_string());
                }
                return;
            }

            // An action inside rendered output — a dataview task checkbox,
            // say. The editor has already flipped it optimistically; making it
            // true is the host's job.
            if let Some(action) = payload.strip_prefix("act:") {
                if let Some(cb) = on_block_action {
                    cb(action.to_string());
                }
                return;
            }

            // Clicking rendered output asks to edit the source behind it. The
            // caret offset is unaffected by folding — rendered output is not
            // part of the document model — so we can hand it straight back.
            if let Some(offset) = payload.strip_prefix("edit:") {
                if let Ok(offset) = offset.parse::<usize>() {
                    let src = content.read().clone();
                    let html = render_html(&src, Some(offset), render_block.read().as_ref(), resolve_link.read().as_ref());
                    let _: Result<(), _> =
                        apply_html_and_restore_cursor(&editor_id, &html, offset).await;
                }
                return;
            }

            if let Some(rest) = payload.strip_prefix("cb:")
                && let Some((pos_str, was_checked_str)) = rest.split_once(':')
                && let Ok(hint_pos) = pos_str.parse::<usize>()
            {
                let was_checked = was_checked_str == "1";
                let new_bracket = if was_checked { "[ ]" } else { "[x]" };
                let mut src = content.read().clone();
                // Re-tokenize current content to find the actual bracket
                // position — the hint from data-pos may be stale if the
                // user edited above this line while focused.
                let tokens = tokenize(&src);
                let actual_pos = tokens
                    .iter()
                    .filter_map(|t| match &t.kind {
                        TokenKind::TaskItem {
                            checked,
                            bracket_pos,
                            ..
                        } if *checked == was_checked => Some(*bracket_pos),
                        _ => None,
                    })
                    .min_by_key(|&p| p.abs_diff(hint_pos));
                if let Some(pos) = actual_pos
                    && pos + 3 <= src.len()
                {
                    src.replace_range(pos..pos + 3, new_bracket);
                    // Update rendered_html immediately so the toggle
                    // is visible without waiting for blur — the
                    // use_effect guard skips updates while focused.
                    rendered_html.set(render_html(&src, None, render_block.read().as_ref(), resolve_link.read().as_ref()));
                    content.set(src);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact HTML the editor paints for a given markdown source — this is
    /// the rendering contract, so any drift here is a visible editor regression.
    fn html(src: &str) -> String {
        render_html(src, None, None, None)
    }

    /// A renderer that claims `demo` fences and shouts the body back.
    fn demo_renderer() -> BlockRenderer {
        BlockRenderer::new(|lang, body| {
            if lang == "demo" {
                format!("<b>{}</b>", body.to_uppercase())
            } else {
                String::new()
            }
        })
    }

    #[test]
    fn wraps_output_in_a_single_md_line() {
        let out = html("hello");
        assert!(out.starts_with("<div class=\"md-line\">"));
        assert!(out.ends_with("</div>"));
        assert!(out.contains("md-plain"));
        assert!(out.contains("hello"));
    }

    #[test]
    fn newlines_open_new_md_line_divs() {
        // Two source lines become two md-line divs (block divs, no <br>).
        let out = html("a\nb");
        assert_eq!(out.matches("<div class=\"md-line\">").count(), 2);
        assert!(!out.contains("<br"));
    }

    #[test]
    fn bold_keeps_visible_markers_and_class() {
        let out = html("**hi**");
        assert!(out.contains("<strong class=\"md-token md-bold\">"));
        // The `**` markers are preserved (Obsidian-style raw-on-focus editing).
        assert_eq!(out.matches("md-marker").count(), 2);
        assert!(out.contains("hi"));
    }

    #[test]
    fn escapes_html_special_chars() {
        let out = html("a < b & \"c\"");
        assert!(out.contains("&lt;"));
        assert!(out.contains("&amp;"));
        assert!(out.contains("&quot;"));
        // No raw angle bracket from the note leaks into the DOM string.
        assert!(!out.contains("a < b"));
    }

    #[test]
    fn heading_carries_level_class() {
        let out = html("## Title");
        assert!(out.contains("md-heading"));
        assert!(out.contains("md-h2"));
        assert!(out.contains("Title"));
    }

    #[test]
    fn wikilink_exposes_navigate_target() {
        let out = html("[[My Note]]");
        assert!(out.contains("md-wikilink"));
        assert!(out.contains("data-navigate=\"My Note\""));
    }

    #[test]
    fn labeled_wikilink_splits_target_and_display() {
        let out = html("[[Target|shown]]");
        assert!(out.contains("data-navigate=\"Target\""));
        assert!(out.contains("md-wikilink-target"));
        assert!(out.contains("shown"));
    }

    /// Resolves only the targets it is given, so a test can say exactly which
    /// links exist.
    fn resolver(known: &'static [&'static str]) -> LinkResolver {
        LinkResolver::new(move |t| known.contains(&t))
    }

    #[test]
    fn without_a_resolver_every_wikilink_stays_linked() {
        // The pre-existing behaviour: no host opinion means no missing links.
        let out = html("[[Anything At All]]");
        assert!(out.contains("md-wikilink--linked"));
        assert!(!out.contains("md-wikilink--missing"));
        assert!(!out.contains("newnote:"));
    }

    #[test]
    fn a_resolved_wikilink_navigates_and_offers_nothing() {
        let out = render_html("[[Here]]", None, None, Some(&resolver(&["Here"])));
        assert!(out.contains("md-wikilink--linked"));
        assert!(out.contains("data-navigate=\"Here\""));
        assert!(!out.contains("md-wikilink-create"));
    }

    #[test]
    fn an_unresolved_wikilink_offers_to_create_the_note() {
        let out = render_html("[[Nowhere]]", None, None, Some(&resolver(&["Here"])));
        assert!(out.contains("md-wikilink--missing"));
        assert!(out.contains("data-action=\"newnote:Nowhere\""));
        // No navigate target: a click falls through to editing the source.
        assert!(!out.contains("data-navigate"));
    }

    #[test]
    fn an_unresolved_labeled_wikilink_creates_the_target_not_the_label() {
        let out = render_html("[[Nowhere|see this]]", None, None, Some(&resolver(&[])));
        assert!(out.contains("data-action=\"newnote:Nowhere\""));
        assert!(out.contains("see this"));
    }

    #[test]
    fn the_create_action_target_is_escaped() {
        let out = render_html("[[a\"b<c]]", None, None, Some(&resolver(&[])));
        assert!(out.contains("newnote:a&quot;b&lt;c"));
        assert!(!out.contains("newnote:a\"b<c"));
    }

    #[test]
    fn resolution_sees_the_raw_target_including_a_heading() {
        // The host decides what `#Section` means; the editor passes it through.
        let out = render_html("[[Note#Section]]", None, None, Some(&resolver(&["Note#Section"])));
        assert!(out.contains("md-wikilink--linked"));
    }

    #[test]
    fn link_carries_href_and_navigate() {
        let out = html("[label](https://example.com)");
        assert!(out.contains("md-link"));
        assert!(out.contains("href=\"https://example.com\""));
        assert!(out.contains("data-navigate=\"https://example.com\""));
    }

    #[test]
    fn task_item_exposes_checkbox_state() {
        let checked = html("- [x] done");
        assert!(checked.contains("md-task-checkbox"));
        assert!(checked.contains("data-checked=\"true\""));
        let unchecked = html("- [ ] todo");
        assert!(unchecked.contains("data-checked=\"false\""));
    }

    // ── Rendered blocks ──────────────────────────────────────────────────────

    const DEMO: &str = "before\n```demo\nquery\n```\nafter";

    #[test]
    fn a_claimed_fence_renders_and_its_source_is_hidden_but_present() {
        let out = render_html(DEMO, None, Some(&demo_renderer()), None);
        assert!(out.contains("<b>QUERY</b>"), "got: {out}");
        // The source must still be in the DOM: it is what the document model
        // reads back, and losing it would lose the note's text.
        assert!(out.contains("query"));
        assert!(out.contains("md-render-host"), "the fence line hosts the output");
        assert_eq!(out.matches("md-render-hidden").count(), 2, "body + closing fence");
        // Lines outside the block are untouched.
        assert!(out.contains("<div class=\"md-line\">"));
    }

    #[test]
    fn the_caret_inside_a_block_shows_its_source_instead() {
        // Offset 12 is inside "query" (line 3 of DEMO).
        let out = render_html(DEMO, Some(12), Some(&demo_renderer()), None);
        assert!(!out.contains("<b>QUERY</b>"), "editing shows source, not output");
        assert!(!out.contains("md-render"));
        // Leaving the block again brings the output back.
        assert!(render_html(DEMO, Some(0), Some(&demo_renderer()), None).contains("<b>QUERY</b>"));
    }

    #[test]
    fn line_count_is_the_same_folded_or_not() {
        // The caret offsets the JS side computes are line-based, so folding a
        // block must never change how many `.md-line` divs exist.
        let folded = render_html(DEMO, None, Some(&demo_renderer()), None);
        let source = render_html(DEMO, Some(12), Some(&demo_renderer()), None);
        assert_eq!(
            folded.matches("class=\"md-line").count(),
            source.matches("class=\"md-line").count(),
        );
        assert_eq!(folded.matches("class=\"md-line").count(), 5);
    }

    #[test]
    fn an_unclaimed_or_unterminated_fence_stays_source() {
        // A language the renderer doesn't claim.
        let rust = render_html("```rust\nfn f() {}\n```", None, Some(&demo_renderer()), None);
        assert!(!rust.contains("md-render"));
        // Still being typed: no closing fence yet.
        let half = render_html("```demo\nquer", None, Some(&demo_renderer()), None);
        assert!(!half.contains("md-render"));
        // And with no renderer at all, nothing changes.
        assert!(!render_html(DEMO, None, None, None).contains("md-render"));
    }

    #[test]
    fn the_output_carries_the_offset_that_reopens_the_source() {
        let out = render_html(DEMO, None, Some(&demo_renderer()), None);
        // 7 = start of the ```demo line ("before\n" is 7 bytes).
        assert!(out.contains("data-edit-offset=\"7\""), "got: {out}");
        assert!(out.contains("data-md-render"), "excluded from the document model");
    }

    #[test]
    fn wikilink_target_attribute_is_escaped() {
        // A quote in the target must not break out of the attribute.
        let out = html("[[a\"b]]");
        assert!(out.contains("data-navigate=\"a&quot;b\""));
        assert!(!out.contains("data-navigate=\"a\"b\""));
    }
}
