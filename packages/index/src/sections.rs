//! A note's `##` sections: finding them, reading them, and replacing their
//! bodies — what the migration review needs to file a day's notes away.
//!
//! A section is a level-2 heading plus everything up to the next heading of
//! level 1 or 2; deeper headings travel with it. Headings inside fenced code
//! and inside frontmatter are not headings. Line numbers are absolute (counted
//! over the whole content, frontmatter included), like [`crate::tasks`].

/// One `## Heading` and the lines under it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    /// The heading text, without the `## `.
    pub heading: String,
    /// 0-based line index of the heading itself.
    pub start: usize,
    /// One past the section's last line.
    pub end: usize,
}

/// Every `##` section in `content`, in document order.
pub fn sections(content: &str) -> Vec<Section> {
    let lines: Vec<&str> = content.lines().collect();
    let first = frontmatter_lines(content);
    let mut out: Vec<Section> = Vec::new();
    let mut in_fence = false;
    for (i, line) in lines.iter().enumerate().skip(first) {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let Some((level, text)) = heading(line) else { continue };
        if level > 2 {
            continue;
        }
        if let Some(open) = out.last_mut().filter(|s| s.end == usize::MAX) {
            open.end = i;
        }
        if level == 2 {
            out.push(Section { heading: text, start: i, end: usize::MAX });
        }
    }
    if let Some(open) = out.last_mut().filter(|s| s.end == usize::MAX) {
        open.end = lines.len();
    }
    out
}

/// The `nth` (0-based) section headed `heading`, re-found in `content` —
/// for locating a section again after the file may have changed around it.
pub fn find(content: &str, heading: &str, nth: usize) -> Option<Section> {
    sections(content).into_iter().filter(|s| s.heading == heading).nth(nth)
}

/// The section's body: the lines under its heading, without the blank lines
/// at either end.
pub fn body(content: &str, section: &Section) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let inner = lines.get(section.start + 1..section.end).unwrap_or_default();
    let first = inner.iter().position(|l| !l.trim().is_empty());
    let last = inner.iter().rposition(|l| !l.trim().is_empty());
    match (first, last) {
        (Some(a), Some(b)) => inner[a..=b].join("\n"),
        _ => String::new(),
    }
}

/// `content` with the section's body replaced by `new_body`, keeping its
/// heading and a blank line before whatever follows.
pub fn replace_body(content: &str, section: &Section, new_body: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    out.extend_from_slice(&lines[..=section.start]);
    out.push("");
    out.extend(new_body.lines());
    if section.end < lines.len() {
        out.push("");
        out.extend_from_slice(&lines[section.end..]);
    }
    let mut s = out.join("\n");
    s.push('\n');
    s
}

/// How many lines the leading `---` frontmatter block occupies, fences
/// included; 0 when there is none.
fn frontmatter_lines(content: &str) -> usize {
    let mut lines = content.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return 0;
    }
    lines
        .position(|l| l.trim_end() == "---")
        .map_or(0, |i| i + 2)
}

/// `## Heading` → `(2, "Heading")`, by the same rules as the index's heading
/// extraction: a `#` run of 1–6 followed by a space.
fn heading(line: &str) -> Option<(u8, String)> {
    let level = line.bytes().take_while(|&b| b == b'#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let text = line[level..].strip_prefix(' ')?;
    // Bounded 1..=6 by the check above.
    #[allow(clippy::cast_possible_truncation)]
    let level = level as u8;
    Some((level, text.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "---\ntags: [log]\n## not a heading\n---\n# Monday\n\n## Log\n\n- [ ] Call Bo\n\n## Meeting\n\nTalked about X.\n### Detail\nMore.\n\n## Ideas\n```\n## fenced\n```\nEnd.\n";

    #[test]
    fn splits_at_level_two_and_keeps_deeper_headings_inside() {
        let s = sections(LOG);
        let names: Vec<&str> = s.iter().map(|s| s.heading.as_str()).collect();
        assert_eq!(names, vec!["Log", "Meeting", "Ideas"]);
        assert_eq!(body(LOG, &s[1]), "Talked about X.\n### Detail\nMore.");
    }

    #[test]
    fn frontmatter_and_fenced_code_hold_no_headings() {
        let s = sections(LOG);
        assert!(!s.iter().any(|s| s.heading.contains("not a heading") || s.heading == "fenced"));
        assert_eq!(body(LOG, &s[2]), "```\n## fenced\n```\nEnd.");
    }

    #[test]
    fn a_level_one_heading_ends_a_section() {
        let c = "## A\none\n# Next\n## B\ntwo\n";
        let s = sections(c);
        assert_eq!(body(c, &s[0]), "one");
        assert_eq!(body(c, &s[1]), "two");
    }

    #[test]
    fn replacing_a_body_keeps_the_heading_and_what_follows() {
        let s = find(LOG, "Meeting", 0).unwrap();
        let out = replace_body(LOG, &s, "→ [[Website#Meeting]]");
        assert!(out.contains("## Meeting\n\n→ [[Website#Meeting]]\n\n## Ideas\n"), "{out}");
        assert!(!out.contains("Talked about X."));
        // The other sections are untouched.
        assert_eq!(body(&out, &find(&out, "Log", 0).unwrap()), "- [ ] Call Bo");
    }

    #[test]
    fn replacing_the_last_section_ends_with_a_newline() {
        let c = "## Only\nold\n";
        let out = replace_body(c, &sections(c)[0], "new");
        assert_eq!(out, "## Only\n\nnew\n");
    }

    #[test]
    fn find_counts_repeated_headings() {
        let c = "## Notes\na\n## Notes\nb\n";
        assert_eq!(body(c, &find(c, "Notes", 1).unwrap()), "b");
        assert!(find(c, "Notes", 2).is_none());
    }
}
