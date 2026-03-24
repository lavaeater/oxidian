use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Plain,
    Bold,
    Italic,
    BoldItalic,
    Code,
    Strikethrough,
    Heading(u8),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    /// Byte range in the source covering the full token including markers
    pub range: Range<usize>,
    /// Byte range of just the inner content (without markers)
    pub content_range: Range<usize>,
}

impl Token {
    /// Returns the raw source text (with markers)
    pub fn raw<'a>(&self, source: &'a str) -> &'a str {
        &source[self.range.clone()]
    }

    /// Returns the display text (without markers)
    pub fn display<'a>(&self, source: &'a str) -> &'a str {
        &source[self.content_range.clone()]
    }

    /// Whether `pos` falls inside the full token range
    pub fn contains(&self, pos: usize) -> bool {
        pos >= self.range.start && pos <= self.range.end
    }
}

/// Tokenize a single line of markdown into inline tokens.
pub fn tokenize_line(line: &str, line_offset: usize) -> Vec<Token> {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut tokens = Vec::new();
    let mut pos = 0;
    let mut plain_start = pos;

    // Check for heading prefix
    if bytes.first() == Some(&b'#') {
        let mut level = 0u8;
        while pos < len && bytes[pos] == b'#' && level < 6 {
            level += 1;
            pos += 1;
        }
        if pos < len && bytes[pos] == b' ' {
            let content_start = pos + 1;
            tokens.push(Token {
                kind: TokenKind::Heading(level),
                range: line_offset..line_offset + len,
                content_range: line_offset + content_start..line_offset + len,
            });
            return tokens;
        }
        // Not a valid heading, reset
        pos = 0;
    }

    while pos < len {
        // Inline code: `...`
        if bytes[pos] == b'`' {
            flush_plain(line, line_offset, plain_start, pos, &mut tokens);
            if let Some(end) = find_closing(bytes, pos + 1, b'`') {
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
            flush_plain(line, line_offset, plain_start, pos, &mut tokens);
            if let Some(end) = find_double_closing(bytes, pos + 2, b'~') {
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

        // Bold+Italic: ***...*** or ___...___
        if pos + 2 < len
            && ((bytes[pos] == b'*' && bytes[pos + 1] == b'*' && bytes[pos + 2] == b'*')
                || (bytes[pos] == b'_' && bytes[pos + 1] == b'_' && bytes[pos + 2] == b'_'))
        {
            let marker = bytes[pos];
            flush_plain(line, line_offset, plain_start, pos, &mut tokens);
            if let Some(end) = find_triple_closing(bytes, pos + 3, marker) {
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
            flush_plain(line, line_offset, plain_start, pos, &mut tokens);
            if let Some(end) = find_double_closing(bytes, pos + 2, marker) {
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
            flush_plain(line, line_offset, plain_start, pos, &mut tokens);
            if let Some(end) = find_closing(bytes, pos + 1, marker) {
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

/// Tokenize the full source text, line by line.
pub fn tokenize(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut offset = 0;
    for line in source.split('\n') {
        let line_tokens = tokenize_line(line, offset);
        tokens.extend(line_tokens);
        offset += line.len() + 1; // +1 for the newline
    }
    tokens
}

fn flush_plain(line: &str, line_offset: usize, start: usize, end: usize, tokens: &mut Vec<Token>) {
    if start < end && start < line.len() {
        tokens.push(Token {
            kind: TokenKind::Plain,
            range: line_offset + start..line_offset + end,
            content_range: line_offset + start..line_offset + end,
        });
    }
}

fn find_closing(bytes: &[u8], start: usize, marker: u8) -> Option<usize> {
    for i in start..bytes.len() {
        if bytes[i] == marker {
            if i > start {
                return Some(i);
            }
        }
    }
    None
}

fn find_double_closing(bytes: &[u8], start: usize, marker: u8) -> Option<usize> {
    let mut i = start;
    while i + 1 < bytes.len() {
        if bytes[i] == marker && bytes[i + 1] == marker {
            if i > start {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn find_triple_closing(bytes: &[u8], start: usize, marker: u8) -> Option<usize> {
    let mut i = start;
    while i + 2 < bytes.len() {
        if bytes[i] == marker && bytes[i + 1] == marker && bytes[i + 2] == marker {
            if i > start {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

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
}
