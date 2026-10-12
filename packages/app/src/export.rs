/// Export a markdown note to standalone HTML.
///
/// Renders via a clean HTML-only pass (no contenteditable, no marker spans,
/// no CSS classes that require the Oxidian stylesheet).
use std::fmt::Write as _;

use ui::tokenizer::{tokenize, TokenKind};

pub fn to_html(title: &str, markdown: &str) -> String {
    let tokens = tokenize(markdown);
    let body = render_tokens(markdown, &tokens);
    let title = escape_html(title);

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
  body {{ max-width: 720px; margin: 2rem auto; padding: 0 1.5rem;
         font-family: system-ui, sans-serif; line-height: 1.7;
         background: #fff; color: #1a1a1a; }}
  h1,h2,h3,h4,h5,h6 {{ margin-top: 1.5em; margin-bottom: .4em; font-weight: 700; }}
  h1 {{ font-size: 2em; }} h2 {{ font-size: 1.5em; }} h3 {{ font-size: 1.25em; }}
  a {{ color: #4493f8; text-decoration: underline; }}
  code {{ font-family: ui-monospace,monospace; font-size:.9em;
          background:#f3f4f6; padding:.1em .3em; border-radius:3px; }}
  pre  {{ background:#f3f4f6; padding:1em; border-radius:6px; overflow-x:auto; }}
  pre code {{ background:none; padding:0; }}
  blockquote {{ border-left:3px solid #d0d7de; margin:0; padding:.4em 1em; color:#57606a; }}
  table {{ border-collapse:collapse; width:100%; }}
  th,td {{ border:1px solid #d0d7de; padding:.4em .8em; text-align:left; }}
  th {{ background:#f6f8fa; font-weight:600; }}
  hr {{ border:none; border-top:1px solid #d0d7de; margin:1.5em 0; }}
  ul,ol {{ padding-left:1.5em; }}
  li {{ margin:.2em 0; }}
  input[type=checkbox] {{ margin-right:.4em; }}
  .wikilink {{ color: #6f42c1; }}
</style>
</head>
<body>
{body}
</body>
</html>
"#
    )
}

#[allow(clippy::too_many_lines)]
fn render_tokens(source: &str, tokens: &[ui::tokenizer::Token]) -> String {
    let mut out = String::new();
    let mut in_code_fence = false;
    let mut last_end = 0;

    for token in tokens {
        // Emit any gap (shouldn't be significant in normal markdown)
        let _ = &source[last_end..token.range.start];

        let raw = token.raw(source);
        let display = token.display(source);

        match &token.kind {
            TokenKind::Plain => {
                out.push_str(&escape_html(display));
            }
            TokenKind::Bold => {
                out.push_str("<strong>");
                out.push_str(&escape_html(display));
                out.push_str("</strong>");
            }
            TokenKind::Italic => {
                out.push_str("<em>");
                out.push_str(&escape_html(display));
                out.push_str("</em>");
            }
            TokenKind::BoldItalic => {
                out.push_str("<strong><em>");
                out.push_str(&escape_html(display));
                out.push_str("</em></strong>");
            }
            TokenKind::Code => {
                out.push_str("<code>");
                out.push_str(&escape_html(display));
                out.push_str("</code>");
            }
            TokenKind::Strikethrough => {
                out.push_str("<s>");
                out.push_str(&escape_html(display));
                out.push_str("</s>");
            }
            TokenKind::Heading(level) => {
                let _ = write!(out, "<h{level}>");
                out.push_str(&escape_html(display));
                let _ = write!(out, "</h{level}>");
            }
            TokenKind::Blockquote => {
                out.push_str("<blockquote>");
                out.push_str(&escape_html(display));
                out.push_str("</blockquote>");
            }
            TokenKind::ListItem { ordered, depth } => {
                let indent = "  ".repeat(*depth as usize);
                let tag = if *ordered { "ol" } else { "ul" };
                // Simple single-item list (no grouping for now)
                let _ = write!(out, "{indent}<{tag}><li>");
                out.push_str(&escape_html(display));
                let _ = write!(out, "</li></{tag}>");
            }
            TokenKind::TaskItem { marker, .. } => {
                let check = if matches!(marker, 'x' | 'X') { " checked" } else { "" };
                // An exported entry that was migrated or dropped is neither
                // ticked nor plain-unticked, so the signifier goes in beside
                // the box rather than being flattened away.
                let signifier = match marker {
                    '>' => " <span class=\"bujo\">›</span>",
                    '<' => " <span class=\"bujo\">‹</span>",
                    '-' => " <span class=\"bujo\">–</span>",
                    'o' | 'O' => " <span class=\"bujo\">○</span>",
                    _ => "",
                };
                let _ = write!(
                    out,
                    "<ul><li><input type=\"checkbox\" disabled{check}>{signifier} {}",
                    escape_html(display)
                );
                out.push_str("</li></ul>");
            }
            TokenKind::HorizontalRule => {
                out.push_str("<hr>");
            }
            TokenKind::Link { url_range } => {
                let url = escape_html(&source[url_range.clone()]);
                let _ = write!(out, "<a href=\"{url}\">");
                out.push_str(&escape_html(display));
                out.push_str("</a>");
            }
            TokenKind::WikiLink { target_range, .. } => {
                let target = escape_html(&source[target_range.clone()]);
                let _ = write!(out, "<span class=\"wikilink\" data-target=\"{target}\">");
                out.push_str(&escape_html(display));
                out.push_str("</span>");
            }
            TokenKind::Image { url_range } => {
                let url = escape_html(&source[url_range.clone()]);
                let _ = write!(
                    out,
                    "<img src=\"{url}\" alt=\"{}\" style=\"max-width:100%\">",
                    escape_html(display)
                );
            }
            TokenKind::CodeFence { lang_range } => {
                if in_code_fence {
                    out.push_str("</code></pre>");
                    in_code_fence = false;
                } else {
                    let lang = lang_range.as_ref().map_or("", |r| &source[r.clone()]);
                    let lang_attr = if lang.is_empty() { String::new() } else { format!(" class=\"language-{lang}\"") };
                    let _ = write!(out, "<pre><code{lang_attr}>");
                    in_code_fence = true;
                }
            }
            TokenKind::CodeBlock => {
                out.push_str(&escape_html(raw));
                out.push('\n');
            }
            TokenKind::TableRow { cells, is_separator } => {
                if *is_separator {
                    // skip — handled by wrapping the next row in tbody, but for
                    // simplicity we just skip separator lines in export
                } else {
                    out.push_str("<tr>");
                    for cell in cells {
                        out.push_str("<td>");
                        out.push_str(&escape_html(&source[cell.clone()]));
                        out.push_str("</td>");
                    }
                    out.push_str("</tr>");
                }
            }
        }

        last_end = token.range.end;
    }

    // Close any unclosed code fence
    if in_code_fence {
        out.push_str("</code></pre>");
    }

    // Wrap bare <tr> rows in a table element
    // (simple post-process: wrap consecutive <tr> blocks)
    wrap_tables(out)
}

fn wrap_tables(html: String) -> String {
    if !html.contains("<tr>") {
        return html;
    }
    let mut result = String::new();
    let mut in_table = false;
    for line in html.split('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with("<tr>") {
            if !in_table { result.push_str("<table>\n"); in_table = true; }
            result.push_str(line); result.push('\n');
        } else {
            if in_table { result.push_str("</table>\n"); in_table = false; }
            result.push_str(line); result.push('\n');
        }
    }
    if in_table { result.push_str("</table>\n"); }
    result
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
}

// The actual browser download is triggered by `js::download_file()`
// (see `assets/oxidian.js`), which takes the filename and rendered HTML
// directly — no manual string escaping.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_document_and_escapes_title() {
        let html = to_html("A & B <x>", "hello");
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<title>A &amp; B &lt;x&gt;</title>"));
        assert!(html.contains("hello"));
        assert!(html.trim_end().ends_with("</html>"));
    }

    #[test]
    fn renders_inline_formatting() {
        let html = to_html("t", "**bold** and *italic* and `code`");
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<em>italic</em>"));
        assert!(html.contains("<code>code</code>"));
    }

    #[test]
    fn renders_headings() {
        let html = to_html("t", "# Title\n## Sub");
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<h2>Sub</h2>"));
    }

    #[test]
    fn escapes_html_in_body_content() {
        let html = to_html("t", "a < b && c > d");
        assert!(html.contains("a &lt; b &amp;&amp; c &gt; d"));
        // The raw angle bracket from the note must not leak through.
        assert!(!html.contains("a < b"));
    }

    #[test]
    fn renders_wikilink_with_target_attribute() {
        let html = to_html("t", "see [[Other Note]]");
        assert!(html.contains(r#"<span class="wikilink" data-target="Other Note">"#));
    }

    #[test]
    fn renders_link_with_href() {
        let html = to_html("t", "[label](https://example.com)");
        assert!(html.contains(r#"<a href="https://example.com">"#));
        assert!(html.contains("label</a>"));
    }

    #[test]
    fn wraps_consecutive_table_rows_in_table() {
        let md = "| a | b |\n| --- | --- |\n| 1 | 2 |";
        let html = to_html("t", md);
        assert!(html.contains("<table>"));
        assert!(html.contains("</table>"));
        assert!(html.contains("<tr>"));
        // The separator row must not become a data row.
        assert!(!html.contains("<td>---</td>"));
    }
}
