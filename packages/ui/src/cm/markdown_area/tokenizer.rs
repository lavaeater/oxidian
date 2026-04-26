use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // ── Inline ──────────────────────────────────────────────────────────────
    Plain,
    Bold,
    Italic,
    BoldItalic,
    Code,
    Strikethrough,
    /// `[text](url)` — `content_range` is the link text, `url_range` is the URL.
    Link { url_range: Range<usize> },
    /// `[[target]]` or `[[target|display]]` — `content_range` is the display
    /// text (falls back to target when no `|` is present).
    WikiLink {
        target_range: Range<usize>,
        display_range: Option<Range<usize>>,
    },
    /// `![alt](url)` — `content_range` is the alt text, `url_range` is the URL.
    Image { url_range: Range<usize> },
    // ── Block (one token per whole line) ────────────────────────────────────
    Heading(u8),
    /// `> content` — `content_range` starts after the `> ` prefix.
    Blockquote,
    /// `- item` / `* item` / `1. item` — `content_range` starts after the marker.
    ListItem { ordered: bool, depth: u8 },
    /// `- [ ] item` / `- [x] item` — `content_range` is the task text.
    /// `bracket_pos` is the source byte position of the `[` character.
    TaskItem { checked: bool, depth: u8, bracket_pos: usize },
    /// `---` / `***` / `___`
    HorizontalRule,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    /// Byte range covering the full token including markers.
    pub range: Range<usize>,
    /// Byte range of the display content (without markers).
    pub content_range: Range<usize>,
}

impl Token {
    pub fn raw<'a>(&self, source: &'a str) -> &'a str {
        &source[self.range.clone()]
    }

    pub fn display<'a>(&self, source: &'a str) -> &'a str {
        &source[self.content_range.clone()]
    }

    pub fn contains(&self, pos: usize) -> bool {
        pos >= self.range.start && pos <= self.range.end
    }
}

// ── Public entry point ───────────────────────────────────────────────────────

/// Tokenize the full source string into a flat list of tokens.
pub fn tokenize(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut pos = 0;

    while pos < source.len() {
        let line_end = source[pos..]
            .find('\n')
            .map(|i| pos + i)
            .unwrap_or(source.len());
        let line = &source[pos..line_end];

        // Block-level detections (whole-line tokens) ────────────────────────

        if is_horizontal_rule(line) {
            tokens.push(Token {
                kind: TokenKind::HorizontalRule,
                range: pos..line_end,
                content_range: pos..line_end,
            });
        } else if line.starts_with("> ") || line == ">" {
            let content_start = if line.starts_with("> ") {
                pos + 2
            } else {
                pos + 1
            };
            tokens.push(Token {
                kind: TokenKind::Blockquote,
                range: pos..line_end,
                content_range: content_start..line_end,
            });
        } else if let Some(token) = detect_task_item(line, pos, line_end) {
            tokens.push(token);
        } else if let Some((ordered, depth, content_start)) = detect_list_item(line, pos) {
            tokens.push(Token {
                kind: TokenKind::ListItem { ordered, depth },
                range: pos..line_end,
                content_range: content_start..line_end,
            });
        } else if let Some(token) = detect_heading(line, pos, line_end) {
            tokens.push(token);
        } else {
            // Inline tokens ─────────────────────────────────────────────────
            tokens.extend(tokenize_line(line, pos));
        }

        pos = if line_end < source.len() {
            line_end + 1
        } else {
            line_end
        };
    }

    tokens
}

// ── Block helpers ────────────────────────────────────────────────────────────

fn is_horizontal_rule(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() < 3 {
        return false;
    }
    let first = match trimmed.chars().next() {
        Some(c @ ('-' | '*' | '_')) => c,
        _ => return false,
    };
    trimmed.chars().all(|c| c == first || c == ' ')
        && trimmed.chars().filter(|&c| c == first).count() >= 3
}

fn detect_heading(line: &str, pos: usize, line_end: usize) -> Option<Token> {
    let bytes = line.as_bytes();
    if bytes.first() != Some(&b'#') {
        return None;
    }
    let mut level = 0u8;
    let mut i = 0;
    while i < bytes.len() && bytes[i] == b'#' && level < 6 {
        level += 1;
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b' ' {
        Some(Token {
            kind: TokenKind::Heading(level),
            range: pos..line_end,
            content_range: pos + i + 1..line_end,
        })
    } else {
        None
    }
}

/// Returns `(ordered, depth, source_offset_of_content_start)` when the line
/// is a list item, or `None` otherwise.
fn detect_list_item(line: &str, pos: usize) -> Option<(bool, u8, usize)> {
    let bytes = line.as_bytes();
    let mut i = 0;

    // Count leading spaces; every 2 spaces = 1 depth level.
    while i < bytes.len() && bytes[i] == b' ' {
        i += 1;
    }
    let depth = (i / 2) as u8;

    // Unordered: `- ` / `* ` / `+ `
    if i < bytes.len()
        && matches!(bytes[i], b'-' | b'*' | b'+')
        && i + 1 < bytes.len()
        && bytes[i + 1] == b' '
    {
        return Some((false, depth, pos + i + 2));
    }

    // Ordered: `1. ` etc.
    let digit_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i > digit_start
        && i + 1 < bytes.len()
        && bytes[i] == b'.'
        && bytes[i + 1] == b' '
    {
        return Some((true, depth, pos + i + 2));
    }

    None
}

// ── Inline tokenizer ─────────────────────────────────────────────────────────

/// Detects `- [ ] text` / `- [x] text` lines (unordered list items only).
/// Must be called before `detect_list_item` since it's more specific.
fn detect_task_item(line: &str, pos: usize, line_end: usize) -> Option<Token> {
    let (ordered, depth, content_start) = detect_list_item(line, pos)?;
    if ordered {
        return None;
    }

    let content_offset = content_start - pos;
    let content = &line[content_offset..];

    let (checked, text_start) = if content.starts_with("[ ] ") {
        (false, content_start + 4)
    } else if content.starts_with("[x] ") || content.starts_with("[X] ") {
        (true, content_start + 4)
    } else if content == "[ ]" {
        (false, content_start + 3)
    } else if content == "[x]" || content == "[X]" {
        (true, content_start + 3)
    } else {
        return None;
    };

    Some(Token {
        kind: TokenKind::TaskItem {
            checked,
            depth,
            bracket_pos: content_start,
        },
        range: pos..line_end,
        content_range: text_start..line_end,
    })
}

pub fn tokenize_line(line: &str, line_offset: usize) -> Vec<Token> {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut tokens = Vec::new();
    let mut pos = 0;
    let mut plain_start = 0;

    macro_rules! try_match {
        ($cond:expr, $parse:expr) => {
            if $cond {
                if let Some((token, end)) = $parse {
                    flush_plain(line, line_offset, plain_start, pos, &mut tokens);
                    tokens.push(token);
                    pos = end;
                    plain_start = pos;
                    continue;
                }
            }
        };
    }

    while pos < len {
        // Image must come before Link (both start with `[` but Image has `!`).
        try_match!(
            pos + 1 < len && bytes[pos] == b'!' && bytes[pos + 1] == b'[',
            parse_image(bytes, line_offset, pos)
        );

        // WikiLink must come before Link (both start with `[`).
        try_match!(
            pos + 1 < len && bytes[pos] == b'[' && bytes[pos + 1] == b'[',
            parse_wikilink(bytes, line, line_offset, pos)
        );

        try_match!(
            bytes[pos] == b'[',
            parse_link(bytes, line_offset, pos)
        );

        // Inline code: `...`
        if bytes[pos] == b'`' {
            if let Some(end) = find_closing(bytes, pos + 1, b'`') {
                flush_plain(line, line_offset, plain_start, pos, &mut tokens);
                tokens.push(Token {
                    kind: TokenKind::Code,
                    range: line_offset + pos..line_offset + end + 1,
                    content_range: line_offset + pos + 1..line_offset + end,
                });
                pos = end + 1;
                plain_start = pos;
                continue;
            }
        }

        // Strikethrough: ~~...~~
        if pos + 1 < len && bytes[pos] == b'~' && bytes[pos + 1] == b'~' {
            if let Some(end) = find_double_closing(bytes, pos + 2, b'~') {
                flush_plain(line, line_offset, plain_start, pos, &mut tokens);
                tokens.push(Token {
                    kind: TokenKind::Strikethrough,
                    range: line_offset + pos..line_offset + end + 2,
                    content_range: line_offset + pos + 2..line_offset + end,
                });
                pos = end + 2;
                plain_start = pos;
                continue;
            }
        }

        // Bold+Italic: ***...*** or ___...___  (must precede Bold and Italic checks)
        if pos + 2 < len
            && ((bytes[pos] == b'*' && bytes[pos + 1] == b'*' && bytes[pos + 2] == b'*')
                || (bytes[pos] == b'_' && bytes[pos + 1] == b'_' && bytes[pos + 2] == b'_'))
        {
            let marker = bytes[pos];
            if let Some(end) = find_triple_closing(bytes, pos + 3, marker) {
                flush_plain(line, line_offset, plain_start, pos, &mut tokens);
                tokens.push(Token {
                    kind: TokenKind::BoldItalic,
                    range: line_offset + pos..line_offset + end + 3,
                    content_range: line_offset + pos + 3..line_offset + end,
                });
                pos = end + 3;
                plain_start = pos;
                continue;
            }
        }

        // Bold: **...** or __...__
        if pos + 1 < len
            && ((bytes[pos] == b'*' && bytes[pos + 1] == b'*')
                || (bytes[pos] == b'_' && bytes[pos + 1] == b'_'))
        {
            let marker = bytes[pos];
            if let Some(end) = find_double_closing(bytes, pos + 2, marker) {
                flush_plain(line, line_offset, plain_start, pos, &mut tokens);
                tokens.push(Token {
                    kind: TokenKind::Bold,
                    range: line_offset + pos..line_offset + end + 2,
                    content_range: line_offset + pos + 2..line_offset + end,
                });
                pos = end + 2;
                plain_start = pos;
                continue;
            }
        }

        // Italic: *...* or _..._
        if bytes[pos] == b'*' || bytes[pos] == b'_' {
            let marker = bytes[pos];
            if let Some(end) = find_closing(bytes, pos + 1, marker) {
                flush_plain(line, line_offset, plain_start, pos, &mut tokens);
                tokens.push(Token {
                    kind: TokenKind::Italic,
                    range: line_offset + pos..line_offset + end + 1,
                    content_range: line_offset + pos + 1..line_offset + end,
                });
                pos = end + 1;
                plain_start = pos;
                continue;
            }
        }

        pos += 1;
    }

    flush_plain(line, line_offset, plain_start, pos, &mut tokens);
    tokens
}

// ── Inline parsers ────────────────────────────────────────────────────────────

fn parse_link(bytes: &[u8], line_offset: usize, pos: usize) -> Option<(Token, usize)> {
    debug_assert_eq!(bytes[pos], b'[');
    let text_end = find_bracket_close(bytes, pos + 1)?;
    if bytes.get(text_end + 1) != Some(&b'(') {
        return None;
    }
    let url_end = find_paren_close(bytes, text_end + 2)?;
    Some((
        Token {
            kind: TokenKind::Link {
                url_range: line_offset + text_end + 2..line_offset + url_end,
            },
            range: line_offset + pos..line_offset + url_end + 1,
            content_range: line_offset + pos + 1..line_offset + text_end,
        },
        url_end + 1,
    ))
}

fn parse_wikilink(
    bytes: &[u8],
    line: &str,
    line_offset: usize,
    pos: usize,
) -> Option<(Token, usize)> {
    debug_assert_eq!(bytes[pos], b'[');
    debug_assert_eq!(bytes[pos + 1], b'[');
    let mut i = pos + 2;
    while i + 1 < bytes.len() {
        if bytes[i] == b']' && bytes[i + 1] == b']' {
            let inner = &line[pos + 2..i];
            let (target_range, display_range, content_range) =
                if let Some(pipe) = inner.find('|') {
                    let ts = line_offset + pos + 2;
                    let te = ts + pipe;
                    let ds = te + 1;
                    let de = line_offset + i;
                    (ts..te, Some(ds..de), ds..de)
                } else {
                    let ts = line_offset + pos + 2;
                    let te = line_offset + i;
                    (ts..te, None, ts..te)
                };
            return Some((
                Token {
                    kind: TokenKind::WikiLink {
                        target_range,
                        display_range,
                    },
                    range: line_offset + pos..line_offset + i + 2,
                    content_range,
                },
                i + 2,
            ));
        }
        i += 1;
    }
    None
}

fn parse_image(bytes: &[u8], line_offset: usize, pos: usize) -> Option<(Token, usize)> {
    debug_assert_eq!(bytes[pos], b'!');
    debug_assert_eq!(bytes[pos + 1], b'[');
    let alt_end = find_bracket_close(bytes, pos + 2)?;
    if bytes.get(alt_end + 1) != Some(&b'(') {
        return None;
    }
    let url_end = find_paren_close(bytes, alt_end + 2)?;
    Some((
        Token {
            kind: TokenKind::Image {
                url_range: line_offset + alt_end + 2..line_offset + url_end,
            },
            range: line_offset + pos..line_offset + url_end + 1,
            content_range: line_offset + pos + 2..line_offset + alt_end,
        },
        url_end + 1,
    ))
}

// ── Low-level scan helpers ────────────────────────────────────────────────────

/// Find a single closing `marker` byte, requiring at least one char between
/// opener and closer (no empty spans).
fn find_closing(bytes: &[u8], start: usize, marker: u8) -> Option<usize> {
    for i in start..bytes.len() {
        if bytes[i] == marker && i > start {
            return Some(i);
        }
    }
    None
}

fn find_double_closing(bytes: &[u8], start: usize, marker: u8) -> Option<usize> {
    let mut i = start;
    while i + 1 < bytes.len() {
        if bytes[i] == marker && bytes[i + 1] == marker && i > start {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_triple_closing(bytes: &[u8], start: usize, marker: u8) -> Option<usize> {
    let mut i = start;
    while i + 2 < bytes.len() {
        if bytes[i] == marker
            && bytes[i + 1] == marker
            && bytes[i + 2] == marker
            && i > start
        {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Find `]` without crossing a `[` or newline (no nested brackets).
fn find_bracket_close(bytes: &[u8], start: usize) -> Option<usize> {
    for i in start..bytes.len() {
        match bytes[i] {
            b']' => return Some(i),
            b'[' | b'\n' => return None,
            _ => {}
        }
    }
    None
}

/// Find `)` without crossing a newline.
fn find_paren_close(bytes: &[u8], start: usize) -> Option<usize> {
    for i in start..bytes.len() {
        match bytes[i] {
            b')' => return Some(i),
            b'\n' => return None,
            _ => {}
        }
    }
    None
}

fn flush_plain(
    line: &str,
    line_offset: usize,
    start: usize,
    end: usize,
    tokens: &mut Vec<Token>,
) {
    if start < end && start < line.len() {
        tokens.push(Token {
            kind: TokenKind::Plain,
            range: line_offset + start..line_offset + end,
            content_range: line_offset + start..line_offset + end,
        });
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text() {
        let tokens = tokenize("hello world");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Plain);
        assert_eq!(tokens[0].display("hello world"), "hello world");
    }

    #[test]
    fn bold() {
        let src = "before **bold** after";
        let tokens = tokenize(src);
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[1].kind, TokenKind::Bold);
        assert_eq!(tokens[1].display(src), "bold");
        assert_eq!(tokens[1].raw(src), "**bold**");
    }

    #[test]
    fn italic() {
        let src = "*italic*";
        let tokens = tokenize(src);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Italic);
        assert_eq!(tokens[0].display(src), "italic");
    }

    #[test]
    fn code() {
        let src = "a `code` b";
        let tokens = tokenize(src);
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[1].kind, TokenKind::Code);
        assert_eq!(tokens[1].display(src), "code");
    }

    #[test]
    fn heading() {
        let src = "## Title";
        let tokens = tokenize(src);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Heading(2));
        assert_eq!(tokens[0].display(src), "Title");
    }

    #[test]
    fn strikethrough() {
        let src = "~~struck~~";
        let tokens = tokenize(src);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Strikethrough);
        assert_eq!(tokens[0].display(src), "struck");
    }

    #[test]
    fn multiline() {
        let src = "hello\n**bold**";
        let tokens = tokenize(src);
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].kind, TokenKind::Plain);
        assert_eq!(tokens[0].display(src), "hello");
        assert_eq!(tokens[1].kind, TokenKind::Bold);
        assert_eq!(tokens[1].display(src), "bold");
    }

    #[test]
    fn wikilink_simple() {
        let src = "see [[Note Name]] for details";
        let tokens = tokenize(src);
        assert_eq!(tokens.len(), 3);
        let TokenKind::WikiLink { ref target_range, display_range: None } = tokens[1].kind else {
            panic!("expected WikiLink");
        };
        assert_eq!(&src[target_range.clone()], "Note Name");
        assert_eq!(tokens[1].display(src), "Note Name");
        assert_eq!(tokens[1].raw(src), "[[Note Name]]");
    }

    #[test]
    fn wikilink_with_display() {
        let src = "[[target|My Label]]";
        let tokens = tokenize(src);
        assert_eq!(tokens.len(), 1);
        let TokenKind::WikiLink { ref target_range, display_range: Some(ref dr) } = tokens[0].kind else {
            panic!("expected WikiLink with display");
        };
        assert_eq!(&src[target_range.clone()], "target");
        assert_eq!(&src[dr.clone()], "My Label");
        assert_eq!(tokens[0].display(src), "My Label");
    }

    #[test]
    fn link() {
        let src = "[Dioxus](https://dioxuslabs.com)";
        let tokens = tokenize(src);
        assert_eq!(tokens.len(), 1);
        let TokenKind::Link { ref url_range } = tokens[0].kind else {
            panic!("expected Link");
        };
        assert_eq!(&src[url_range.clone()], "https://dioxuslabs.com");
        assert_eq!(tokens[0].display(src), "Dioxus");
    }

    #[test]
    fn image() {
        let src = "![alt text](https://example.com/img.png)";
        let tokens = tokenize(src);
        assert_eq!(tokens.len(), 1);
        let TokenKind::Image { ref url_range } = tokens[0].kind else {
            panic!("expected Image");
        };
        assert_eq!(&src[url_range.clone()], "https://example.com/img.png");
        assert_eq!(tokens[0].display(src), "alt text");
    }

    #[test]
    fn blockquote() {
        let src = "> This is a quote";
        let tokens = tokenize(src);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Blockquote);
        assert_eq!(tokens[0].display(src), "This is a quote");
    }

    #[test]
    fn unordered_list() {
        let src = "- item one\n- item two";
        let tokens = tokenize(src);
        assert_eq!(tokens.len(), 2);
        assert!(matches!(tokens[0].kind, TokenKind::ListItem { ordered: false, depth: 0 }));
        assert_eq!(tokens[0].display(src), "item one");
        assert!(matches!(tokens[1].kind, TokenKind::ListItem { ordered: false, depth: 0 }));
        assert_eq!(tokens[1].display(src), "item two");
    }

    #[test]
    #[test]
    fn task_item_unchecked() {
        let src = "- [ ] buy milk";
        let tokens = tokenize(src);
        assert_eq!(tokens.len(), 1);
        let TokenKind::TaskItem { checked: false, depth: 0, bracket_pos } = tokens[0].kind else {
            panic!("expected unchecked TaskItem");
        };
        assert_eq!(&src[bracket_pos..bracket_pos + 3], "[ ]");
        assert_eq!(tokens[0].display(src), "buy milk");
        assert_eq!(tokens[0].raw(src), "- [ ] buy milk");
    }

    #[test]
    fn task_item_checked() {
        let src = "- [x] done";
        let tokens = tokenize(src);
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0].kind, TokenKind::TaskItem { checked: true, .. }));
        assert_eq!(tokens[0].display(src), "done");
    }

    #[test]
    fn task_item_nested() {
        let src = "  - [ ] nested";
        let tokens = tokenize(src);
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0].kind, TokenKind::TaskItem { checked: false, depth: 1, .. }));
    }

    #[test]
    fn task_item_does_not_steal_plain_list() {
        // A plain list item starting with something other than [ ] stays ListItem
        let src = "- regular item";
        let tokens = tokenize(src);
        assert!(matches!(tokens[0].kind, TokenKind::ListItem { .. }));
    }

    fn ordered_list() {
        let src = "1. first\n2. second";
        let tokens = tokenize(src);
        assert_eq!(tokens.len(), 2);
        assert!(matches!(tokens[0].kind, TokenKind::ListItem { ordered: true, depth: 0 }));
        assert_eq!(tokens[0].display(src), "first");
    }

    #[test]
    fn horizontal_rule() {
        for src in ["---", "***", "___", "- - -"] {
            let tokens = tokenize(src);
            assert_eq!(tokens.len(), 1, "failed for {src:?}");
            assert_eq!(tokens[0].kind, TokenKind::HorizontalRule);
        }
    }

    #[test]
    fn unmatched_marker_stays_plain() {
        // A lone `*` with no closing should not eat adjacent plain text.
        let src = "a * b";
        let tokens = tokenize(src);
        // All three parts should be plain; none should be empty.
        for t in &tokens {
            assert_eq!(t.kind, TokenKind::Plain);
            assert!(!t.display(src).is_empty());
        }
        // Reconstruct: concatenated raw tokens must equal source.
        let reconstructed: String = tokens.iter().map(|t| t.raw(src)).collect();
        assert_eq!(reconstructed, src);
    }
}
