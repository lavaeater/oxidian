//! One pass over a note's content, producing everything the index knows about
//! it. Pure: no I/O, no network, no platform. See `docs/dataview.md` §4.1.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::frontmatter;
use crate::tasks::{self, Task};
use crate::value::Value;

/// Everything extracted from a single note.
///
/// `sha` is the git blob SHA the data was extracted from — a content hash, so
/// an entry whose SHA still matches the vault listing is *provably* current.
/// That is what makes incremental refresh cheap and correct (`Index::stale`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PageData {
    pub path: String,
    pub sha: String,
    /// Typed fields: frontmatter keys merged with inline `key:: value` fields.
    /// Frontmatter wins on a collision, being the more deliberate declaration.
    pub fields: BTreeMap<String, Value>,
    /// Tags from `#inline` use and from a frontmatter `tags:` key, without `#`.
    pub tags: Vec<String>,
    /// `[[WikiLink]]` targets as written (no `|display` part, unresolved).
    pub links: Vec<String>,
    /// `(level, text)` for every ATX heading.
    pub headings: Vec<(u8, String)>,
    pub tasks: Vec<Task>,
    /// The note's body, verbatim and minus frontmatter — what full-text search
    /// runs over.
    ///
    /// Keeping it costs storage but *no requests*: a refresh already downloads
    /// every changed note in full to extract the rest of this struct. That is
    /// what makes search local, instant, and offline — and it is the only way
    /// to have search at all in the browser, where GitHub's code-search API is
    /// unreachable across origins.
    #[serde(default)]
    pub text: String,
}

impl PageData {
    /// Filename without directory or `.md` extension — the note's title as
    /// wikilinks refer to it.
    pub fn stem(&self) -> &str {
        let name = self.path.rsplit('/').next().unwrap_or(&self.path);
        name.strip_suffix(".md").unwrap_or(name)
    }

    /// Directory portion, empty for a root note.
    pub fn folder(&self) -> &str {
        match self.path.rfind('/') {
            Some(i) => &self.path[..i],
            None => "",
        }
    }

    /// A field by name, case-insensitively (note authors are inconsistent about
    /// `Rating` vs `rating`).
    pub fn field(&self, key: &str) -> Option<&Value> {
        self.fields.get(key).or_else(|| {
            self.fields
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v)
        })
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        let tag = tag.trim_start_matches('#');
        self.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
    }
}

/// Extract a page from its raw content.
pub fn extract(path: &str, sha: &str, content: &str) -> PageData {
    let (fm_pairs, body_offset) = match frontmatter::split_frontmatter(content) {
        Some((fm, body)) => (
            frontmatter::parse_pairs(fm),
            content.len() - body.len(),
        ),
        None => (Vec::new(), 0),
    };
    let body = &content[body_offset..];

    let mut tags: Vec<String> = fm_pairs
        .iter()
        .filter(|(k, _)| k == "tags" || k == "tag")
        .flat_map(|(_, v)| frontmatter::parse_tag_list(v))
        .collect();

    let mut links = Vec::new();
    let mut headings = Vec::new();
    let mut inline_fields: Vec<(String, String)> = Vec::new();
    let mut in_fence = false;

    for line in body.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if let Some(h) = heading(line) {
            headings.push(h);
        }
        collect_tags(line, &mut tags);
        collect_links(line, &mut links);
        collect_inline_fields(line, &mut inline_fields);
    }

    dedup_preserving_order(&mut tags);

    // Inline fields first so frontmatter overwrites them on a collision.
    let mut fields: BTreeMap<String, Value> = inline_fields
        .into_iter()
        .map(|(k, v)| (k, Value::infer(&v)))
        .collect();
    for (k, v) in fm_pairs {
        fields.insert(k, Value::infer(&v));
    }

    PageData {
        path: path.to_string(),
        sha: sha.to_string(),
        fields,
        text: body.to_string(),
        tags,
        links,
        headings,
        // Tasks are parsed from the whole content so line numbers stay absolute
        // — `tasks::toggled_content` writes back by line index.
        tasks: tasks::parse_file(path, content),
    }
}

/// `# Heading` → `(1, "Heading")`. A `#` run with no following space is a tag,
/// not a heading, and more than six is not a heading either.
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

/// Inline `#tags`. Follows Obsidian's rules closely enough to be unsurprising:
/// the `#` must not be preceded by a word character (so `C#` is not a tag), the
/// tag body is alphanumerics plus `_-/`, and an all-numeric body (`#1`) is not
/// a tag.
fn collect_tags(line: &str, out: &mut Vec<String>) {
    let bytes = line.as_bytes();
    let mut i = 0;
    while let Some(rel) = line[i..].find('#') {
        let start = i + rel;
        let prev_ok = start == 0 || {
            let p = bytes[start - 1];
            !(p.is_ascii_alphanumeric() || p == b'_' || p == b'#')
        };
        let body: String = line[start + 1..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == '/')
            .collect();
        i = start + 1 + body.len();
        if prev_ok && !body.is_empty() && !body.chars().all(|c| c.is_ascii_digit()) {
            out.push(body);
        }
    }
}

/// `[[target]]` and `[[target|display]]` → `target`.
fn collect_links(line: &str, out: &mut Vec<String>) {
    let mut rest = line;
    while let Some(start) = rest.find("[[") {
        rest = &rest[start + 2..];
        let Some(end) = rest.find("]]") else { break };
        let inner = &rest[..end];
        let target = inner.split('|').next().unwrap_or(inner).trim();
        if !target.is_empty() {
            out.push(target.to_string());
        }
        rest = &rest[end + 2..];
    }
}

/// Inline fields: `key:: value` on its own line (optionally behind a list
/// bullet or blockquote marker), and the bracketed forms `[key:: value]` /
/// `(key:: value)` embedded mid-sentence. Later occurrences of a key win.
fn collect_inline_fields(line: &str, out: &mut Vec<(String, String)>) {
    let mut consumed_brackets = false;
    for (open, close) in [('[', ']'), ('(', ')')] {
        let mut rest = line;
        while let Some(start) = rest.find(open) {
            rest = &rest[start + 1..];
            let Some(end) = rest.find(close) else { break };
            let inner = &rest[..end];
            rest = &rest[end + 1..];
            // `[[WikiLink]]` opens with a second bracket and is not a field.
            if inner.starts_with(open) {
                continue;
            }
            if let Some((k, v)) = split_field(inner) {
                out.push((k, v));
                consumed_brackets = true;
            }
        }
    }
    if consumed_brackets {
        return;
    }
    // Line form: strip a leading bullet / blockquote marker, then split.
    let bare = line
        .trim_start()
        .trim_start_matches(['>', '-', '*', '+'])
        .trim_start();
    if let Some((k, v)) = split_field(bare) {
        out.push((k, v));
    }
}

/// `key:: value` → `("key", "value")`, rejecting empty or bracket-bearing keys.
fn split_field(s: &str) -> Option<(String, String)> {
    let (key, value) = s.split_once("::")?;
    let key = key.trim();
    if key.is_empty() || key.contains(['[', ']', '(', ')', ':', '#']) {
        return None;
    }
    Some((key.to_string(), value.trim().to_string()))
}

fn dedup_preserving_order(v: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    v.retain(|t| seen.insert(t.to_lowercase()));
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "---\ntitle: Deus Ex\ntags: [games, rpg]\nrating: 9\n---\n\
# Deus Ex\n\n\
Played with [[Alice]] and [[Bob|my brother]]. Loved it #immersive-sim.\n\n\
## Notes\n\n\
- [ ] replay on #hardcore 📅 2026-09-01\n\
- [x] finish it\n\n\
```\n\
not a #tag and not [[a link]] and not a # heading\n\
```\n";

    fn page() -> PageData {
        extract("games/Deus Ex.md", "sha1", NOTE)
    }

    #[test]
    fn reads_frontmatter_fields_with_inferred_types() {
        let p = page();
        assert_eq!(p.field("title"), Some(&Value::Str("Deus Ex".into())));
        assert_eq!(p.field("rating"), Some(&Value::Num(9.0)), "9 is a number, not \"9\"");
        assert_eq!(p.field("RATING"), Some(&Value::Num(9.0)), "lookup is case-insensitive");
        assert_eq!(p.field("nope"), None);
    }

    #[test]
    fn reads_inline_fields_in_both_forms() {
        let p = extract(
            "a.md",
            "s",
            "length:: 20 hours\nRated (rating:: 9) by me\n- status:: playing\n",
        );
        assert_eq!(p.field("length"), Some(&Value::Str("20 hours".into())));
        assert_eq!(p.field("rating"), Some(&Value::Num(9.0)));
        assert_eq!(p.field("status"), Some(&Value::Str("playing".into())), "bullet is stripped");
    }

    #[test]
    fn frontmatter_beats_an_inline_field_of_the_same_name() {
        let p = extract("a.md", "s", "---\nrating: 9\n---\nrating:: 3\n");
        assert_eq!(p.field("rating"), Some(&Value::Num(9.0)));
    }

    #[test]
    fn wikilinks_are_not_mistaken_for_inline_fields() {
        let p = extract("a.md", "s", "See [[Some::Note]] and [[Plain]]\n");
        assert!(p.fields.is_empty(), "got {:?}", p.fields);
        assert_eq!(p.links, vec!["Some::Note", "Plain"]);
    }

    #[test]
    fn merges_frontmatter_and_inline_tags() {
        let p = page();
        assert_eq!(p.tags, vec!["games", "rpg", "immersive-sim", "hardcore"]);
        assert!(p.has_tag("games"));
        assert!(p.has_tag("#RPG"), "tag matching is case-insensitive and #-insensitive");
    }

    #[test]
    fn collects_links_without_display_text() {
        assert_eq!(page().links, vec!["Alice", "Bob"]);
    }

    #[test]
    fn collects_headings_with_levels() {
        assert_eq!(
            page().headings,
            vec![(1, "Deus Ex".to_string()), (2, "Notes".to_string())]
        );
    }

    #[test]
    fn parses_tasks_with_absolute_line_numbers() {
        let p = page();
        assert_eq!(p.tasks.len(), 2);
        assert_eq!(p.tasks[0].text, "replay on #hardcore");
        assert_eq!(p.tasks[0].due.as_deref(), Some("2026-09-01"));
        assert!(p.tasks[1].checked);
        // Line index must address the raw file, including the frontmatter, so
        // that `tasks::toggled_content` can write the toggle back.
        let raw_line = NOTE.lines().nth(p.tasks[0].line).unwrap();
        assert!(raw_line.contains("replay on"));
    }

    #[test]
    fn fenced_code_contributes_nothing() {
        let p = page();
        assert!(!p.tags.iter().any(|t| t == "tag"));
        assert!(!p.links.iter().any(|l| l == "a link"));
        assert!(!p.headings.iter().any(|(_, t)| t == "heading"));
    }

    #[test]
    fn headings_are_not_confused_with_tags() {
        let p = extract("a.md", "s", "# Real heading\n#realtag\n####### too deep\n");
        assert_eq!(p.headings, vec![(1, "Real heading".to_string())]);
        assert_eq!(p.tags, vec!["realtag"]);
    }

    #[test]
    fn hash_inside_a_word_is_not_a_tag() {
        let p = extract("a.md", "s", "I write C# and Rust, issue #42, but #real counts\n");
        assert_eq!(p.tags, vec!["real"], "C# and #42 must not become tags");
    }

    #[test]
    fn stem_and_folder_derive_from_the_path() {
        let p = page();
        assert_eq!(p.stem(), "Deus Ex");
        assert_eq!(p.folder(), "games");
        let root = extract("Root.md", "s", "");
        assert_eq!(root.folder(), "");
    }
}
