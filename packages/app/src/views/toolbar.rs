//! The formatting toolbar above the editor.
//!
//! Buttons don't touch the `content` signal: while the editor is focused it
//! deliberately doesn't repaint, so a direct write would be invisible and then
//! overwritten by the next keystroke. Instead each button reads the editor's
//! text + selection (`js::editor_range`), computes the edit here in pure Rust,
//! and hands it back through the editor's own edit path (`js::editor_apply`).
//! Offsets cross the JS boundary as UTF-16 code units and are converted to byte
//! offsets for the string work.

use dioxus::prelude::*;

use crate::js;

/// The result of a toolbar edit: new source and the caret (byte offset).
#[derive(Debug, PartialEq, Eq)]
pub struct Edit {
    pub text: String,
    pub caret: usize,
}

// ── Offsets ───────────────────────────────────────────────────────────────────

/// Byte index of the UTF-16 offset `u` in `s` (clamped to `s.len()`).
pub fn utf16_to_byte(s: &str, u: usize) -> usize {
    let mut acc = 0;
    for (i, ch) in s.char_indices() {
        if acc >= u {
            return i;
        }
        acc += ch.len_utf16();
    }
    s.len()
}

/// UTF-16 offset of byte index `b` in `s`.
pub fn byte_to_utf16(s: &str, b: usize) -> usize {
    s[..b.min(s.len())].chars().map(char::len_utf16).sum()
}

// ── Inline wrap ───────────────────────────────────────────────────────────────

/// Wraps `start..end` in `prefix`/`suffix`, or unwraps it if it is already
/// wrapped. An empty selection inserts the pair with the caret between them.
pub fn wrap(source: &str, start: usize, end: usize, prefix: &str, suffix: &str) -> Edit {
    let (start, end) = (start.min(end), start.max(end));
    // Already wrapped → unwrap. The marker char just outside the pair must not
    // repeat it, so italic (`*`) inside bold (`**x**`) isn't mistaken for it.
    let before = &source[..start];
    let after = &source[end..];
    let pre_char = prefix.chars().next();
    let suf_char = suffix.chars().last();
    if start != end
        && before.ends_with(prefix)
        && after.starts_with(suffix)
        && before[..before.len() - prefix.len()].chars().last() != pre_char
        && after[suffix.len()..].chars().next() != suf_char
    {
        let mut s = String::with_capacity(source.len());
        s.push_str(&before[..before.len() - prefix.len()]);
        s.push_str(&source[start..end]);
        s.push_str(&after[suffix.len()..]);
        return Edit { caret: end - prefix.len(), text: s };
    }

    let mut s = String::with_capacity(source.len() + prefix.len() + suffix.len());
    s.push_str(before);
    s.push_str(prefix);
    s.push_str(&source[start..end]);
    s.push_str(suffix);
    s.push_str(after);
    let caret = if start == end {
        start + prefix.len()
    } else {
        end + prefix.len() + suffix.len()
    };
    Edit { text: s, caret }
}

// ── Line prefixes ─────────────────────────────────────────────────────────────

/// The block prefix a line already carries, in the same family as the button.
/// `(indent, len, same)`: leading whitespace kept as-is, the prefix length after
/// it, and whether it is the *same* kind as the button (→ toggle off).
fn existing_prefix(line: &str, prefix: &str) -> (usize, usize, bool) {
    if prefix.starts_with('#') {
        let hashes = line.bytes().take_while(|&b| b == b'#').count();
        if (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') {
            return (0, hashes + 1, hashes + 1 == prefix.len());
        }
        return (0, 0, false);
    }
    if prefix.starts_with('>') {
        return if line.starts_with("> ") { (0, 2, true) } else { (0, 0, false) };
    }
    // Lists: bullet, task (any signifier), ordered. Indent is preserved.
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let rest = &line[indent..];
    let want_task = prefix.contains('[');
    let bytes = rest.as_bytes();
    if bytes.len() >= 2 && matches!(bytes[0], b'-' | b'*' | b'+') && bytes[1] == b' ' {
        let is_task = bytes.len() >= 6 && bytes[2] == b'[' && bytes[4] == b']' && bytes[5] == b' ';
        let len = if is_task { 6 } else { 2 };
        return (indent, len, is_task == want_task);
    }
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 && rest[digits..].starts_with(". ") {
        return (indent, digits + 2, false);
    }
    (indent, 0, false)
}

/// Toggles a block prefix (`# `, `- `, `- [ ] `, `> `) on every line the
/// selection touches. If all of them already carry it, it's removed; otherwise
/// each line's existing prefix of that family (another heading level, a bullet
/// vs a task) is replaced by it.
pub fn toggle_line_prefix(source: &str, start: usize, end: usize, prefix: &str) -> Edit {
    let (start, end) = (start.min(end), start.max(end));
    let first = source[..start].rfind('\n').map_or(0, |i| i + 1);
    // A selection ending right at a line start doesn't include that line.
    let last_pos = if end > start && end > first && source[..end].ends_with('\n') { end - 1 } else { end };
    let last_end = source[last_pos..].find('\n').map_or(source.len(), |i| last_pos + i);

    let block = &source[first..last_end];
    let lines: Vec<&str> = block.split('\n').collect();
    let all_same = lines.iter().all(|l| existing_prefix(l, prefix).2);

    let mut out = String::with_capacity(source.len() + lines.len() * prefix.len());
    out.push_str(&source[..first]);
    let mut caret = start;
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let (indent, old_len, _) = existing_prefix(line, prefix);
        let new = if all_same { "" } else { prefix };
        if i == 0 {
            // The caret sits on the first line; keep it on the same text.
            let in_line = start - first;
            let old_end = indent + old_len;
            caret = if in_line >= old_end {
                first + in_line - old_len + new.len()
            } else {
                first + indent + new.len()
            };
        }
        out.push_str(&line[..indent]);
        out.push_str(new);
        out.push_str(&line[indent + old_len..]);
    }
    out.push_str(&source[last_end..]);
    Edit { text: out, caret }
}

// ── Applying ──────────────────────────────────────────────────────────────────

/// Reads the active editor, applies `f` (byte offsets), writes the result back.
/// Does nothing when no editor has a caret.
async fn apply(f: impl FnOnce(&str, usize, usize) -> Edit) {
    let Some((text, s16, e16)) = js::editor_range().await else {
        return;
    };
    let (s, e) = (utf16_to_byte(&text, s16), utf16_to_byte(&text, e16));
    let edit = f(&text, s, e);
    let caret = byte_to_utf16(&edit.text, edit.caret);
    js::editor_apply(text, edit.text, caret);
}

// ── Component ─────────────────────────────────────────────────────────────────

#[component]
pub fn FormattingToolbar() -> Element {
    rsx! {
        div { class: "formatting-toolbar",
            TbPrefix { label: "H1", title: "Heading 1", prefix: "# " }
            TbPrefix { label: "H2", title: "Heading 2", prefix: "## " }
            TbPrefix { label: "H3", title: "Heading 3", prefix: "### " }
            div { class: "tb-sep" }
            TbInline { label: "B",      title: "Bold",          prefix: "**", suffix: "**" }
            TbInline { label: "I",      title: "Italic",        prefix: "*",  suffix: "*"  }
            TbInline { label: "S",      title: "Strikethrough", prefix: "~~", suffix: "~~" }
            TbInline { label: "`C`",    title: "Inline code",   prefix: "`",  suffix: "`"  }
            div { class: "tb-sep" }
            TbPrefix { label: "• List", title: "Bullet list", prefix: "- "      }
            TbPrefix { label: "☐ Task", title: "Task item",   prefix: "- [ ] "  }
            TbPrefix { label: "> Quote",title: "Blockquote",  prefix: "> "      }
        }
    }
}

#[component]
fn TbInline(label: &'static str, title: &'static str, prefix: &'static str, suffix: &'static str) -> Element {
    rsx! {
        button {
            class: "tb-btn",
            title: "{title}",
            // Keep focus and selection in the editor.
            onmousedown: move |e| e.prevent_default(),
            onclick: move |_| {
                spawn(apply(move |t, s, e| wrap(t, s, e, prefix, suffix)));
            },
            "{label}"
        }
    }
}

#[component]
fn TbPrefix(label: &'static str, title: &'static str, prefix: &'static str) -> Element {
    rsx! {
        button {
            class: "tb-btn",
            title: "{title}",
            onmousedown: move |e| e.prevent_default(),
            onclick: move |_| {
                spawn(apply(move |t, s, e| toggle_line_prefix(t, s, e, prefix)));
            },
            "{label}"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ed(text: &str, caret: usize) -> Edit {
        Edit { text: text.to_string(), caret }
    }

    #[test]
    fn utf16_round_trips_through_non_ascii() {
        let s = "å😀b";
        // å = 1 unit / 2 bytes, 😀 = 2 units / 4 bytes.
        assert_eq!(utf16_to_byte(s, 0), 0);
        assert_eq!(utf16_to_byte(s, 1), 2);
        assert_eq!(utf16_to_byte(s, 3), 6);
        assert_eq!(utf16_to_byte(s, 4), 7);
        assert_eq!(utf16_to_byte(s, 99), 7);
        assert_eq!(byte_to_utf16(s, 6), 3);
    }

    #[test]
    fn wrap_selection_puts_caret_after_suffix() {
        assert_eq!(wrap("a word b", 2, 6, "**", "**"), ed("a **word** b", 10));
    }

    #[test]
    fn wrap_empty_selection_puts_caret_between_markers() {
        assert_eq!(wrap("ab", 1, 1, "**", "**"), ed("a****b", 3));
    }

    #[test]
    fn wrap_unwraps_an_already_wrapped_selection() {
        assert_eq!(wrap("a **word** b", 4, 8, "**", "**"), ed("a word b", 6));
    }

    #[test]
    fn italic_inside_bold_wraps_rather_than_eating_a_bold_star() {
        assert_eq!(wrap("**word**", 2, 6, "*", "*").text, "***word***");
    }

    #[test]
    fn heading_toggles_on_and_off() {
        assert_eq!(toggle_line_prefix("title", 2, 2, "# "), ed("# title", 4));
        assert_eq!(toggle_line_prefix("# title", 4, 4, "# "), ed("title", 2));
    }

    #[test]
    fn heading_level_is_replaced_not_stacked() {
        assert_eq!(toggle_line_prefix("## title", 5, 5, "# ").text, "# title");
        assert_eq!(toggle_line_prefix("# title", 3, 3, "### ").text, "### title");
    }

    #[test]
    fn only_the_caret_line_changes() {
        let src = "one\ntwo\nthree";
        assert_eq!(toggle_line_prefix(src, 5, 5, "- "), ed("one\n- two\nthree", 7));
    }

    #[test]
    fn bullet_becomes_task_and_task_toggles_off() {
        assert_eq!(toggle_line_prefix("- milk", 3, 3, "- [ ] ").text, "- [ ] milk");
        assert_eq!(toggle_line_prefix("- [x] milk", 7, 7, "- [ ] ").text, "milk");
        assert_eq!(toggle_line_prefix("- [ ] milk", 7, 7, "- ").text, "- milk");
    }

    #[test]
    fn list_prefix_keeps_indentation() {
        assert_eq!(toggle_line_prefix("  - milk", 5, 5, "- [ ] ").text, "  - [ ] milk");
    }

    #[test]
    fn multi_line_selection_toggles_every_line() {
        let src = "a\nb\nc";
        assert_eq!(toggle_line_prefix(src, 0, 5, "- ").text, "- a\n- b\n- c");
        assert_eq!(toggle_line_prefix("- a\n- b", 0, 7, "- ").text, "a\nb");
        // Mixed lines: add to all rather than remove from some.
        assert_eq!(toggle_line_prefix("- a\nb", 0, 5, "- ").text, "- a\n- b");
    }

    #[test]
    fn selection_ending_at_a_line_start_excludes_that_line() {
        assert_eq!(toggle_line_prefix("a\nb", 0, 2, "> ").text, "> a\nb");
    }

    #[test]
    fn caret_inside_old_prefix_lands_after_new_one() {
        // Caret between "#" and " " of "## x".
        assert_eq!(toggle_line_prefix("## x", 1, 1, "# "), ed("# x", 2));
    }
}
