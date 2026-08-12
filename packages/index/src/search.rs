//! Full-text search over the index.
//!
//! Search used to go through GitHub's code-search API, which cannot work here:
//! `api.github.com/search/code` sends no CORS headers, so the browser blocks it
//! outright — and the browser is the priority target. It was also rate-limited
//! to 30 requests a minute, needed a round trip per keystroke, and had no GitLab
//! equivalent.
//!
//! Searching the index instead is local, instant, offline, and host-agnostic.
//! The index already holds every note's body (`PageData::text`), downloaded by
//! the refresh that built it, so this costs no requests at all.
//!
//! Pure — no I/O, no Dioxus. See `docs/dataview.md` §12.

use crate::{Index, PageData};

/// Where a note matched. Ordered by how strong a signal it is: a hit in the
/// title is what the user is most often looking for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchKind {
    Title,
    Heading,
    Tag,
    Body,
}

impl MatchKind {
    pub fn label(self) -> &'static str {
        match self {
            MatchKind::Title => "title",
            MatchKind::Heading => "heading",
            MatchKind::Tag => "tag",
            MatchKind::Body => "text",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchHit {
    pub path: String,
    /// Filename without directory or `.md`.
    pub title: String,
    pub kind: MatchKind,
    /// A line of context around the match, trimmed and length-capped.
    pub fragment: String,
    /// How many lines of the body matched. 0 when the hit is a title or tag.
    pub body_matches: usize,
}

/// Longest fragment we'll show in the results list.
const FRAGMENT_MAX: usize = 160;

/// Search every indexed note.
///
/// All whitespace-separated terms must match somewhere in the note (AND), each
/// as a case-insensitive substring. Substring rather than whole-word because
/// notes are prose and half-typed queries are the common case: results should
/// narrow as you type, not appear only once a word is finished.
///
/// Results are ranked by match kind, then by how many body lines matched, then
/// by path so the order is stable.
pub fn search(index: &Index, query: &str) -> Vec<SearchHit> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    if terms.is_empty() {
        return Vec::new();
    }

    let mut hits: Vec<SearchHit> = index
        .pages()
        .filter_map(|page| hit_for(page, &terms))
        .collect();

    hits.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then(b.body_matches.cmp(&a.body_matches))
            .then(a.path.cmp(&b.path))
    });
    hits
}

/// The best hit for one page, or `None` if it doesn't match every term.
fn hit_for(page: &PageData, terms: &[String]) -> Option<SearchHit> {
    let title = page.stem().to_string();
    let title_lc = title.to_lowercase();
    let text_lc = page.text.to_lowercase();
    let tags_lc: Vec<String> = page.tags.iter().map(|t| t.to_lowercase()).collect();
    let headings_lc: Vec<String> = page.headings.iter().map(|(_, h)| h.to_lowercase()).collect();

    // Every term has to appear *somewhere* in the note, in any of these.
    let matches_term = |term: &String| {
        title_lc.contains(term.as_str())
            || text_lc.contains(term.as_str())
            || tags_lc.iter().any(|t| t.contains(term.as_str()))
            || headings_lc.iter().any(|h| h.contains(term.as_str()))
    };
    if !terms.iter().all(matches_term) {
        return None;
    }

    // The first term decides which kind of hit this is and what context to
    // show — it's the one the user typed first and most likely means.
    let first = &terms[0];
    let body_lines: Vec<&str> = page
        .text
        .lines()
        .filter(|l| l.to_lowercase().contains(first.as_str()))
        .collect();

    let (kind, fragment) = if title_lc.contains(first.as_str()) {
        // A title hit still shows body context when there is any, since the
        // title is already the result's own heading.
        (MatchKind::Title, body_lines.first().map_or_else(String::new, |l| fragment(l)))
    } else if let Some(h) = page.headings.iter().find(|(_, h)| h.to_lowercase().contains(first.as_str())) {
        (MatchKind::Heading, fragment(&h.1))
    } else if let Some(line) = body_lines.first() {
        (MatchKind::Body, fragment(line))
    } else if let Some(t) = page.tags.iter().find(|t| t.to_lowercase().contains(first.as_str())) {
        (MatchKind::Tag, format!("#{t}"))
    } else {
        // Every term matched, but not the first one on its own — only possible
        // when the first term matched a field or a spread across sources.
        (MatchKind::Body, String::new())
    };

    Some(SearchHit {
        path: page.path.clone(),
        title,
        kind,
        fragment,
        body_matches: body_lines.len(),
    })
}

/// One line of context: trimmed, and cut at a char boundary so multi-byte text
/// (which is most of this author's vault) can't panic the slice.
fn fragment(line: &str) -> String {
    let line = line.trim();
    if line.chars().count() <= FRAGMENT_MAX {
        return line.to_string();
    }
    let cut: String = line.chars().take(FRAGMENT_MAX).collect();
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use vault::FileMeta;

    fn vault(files: &[(&str, &str)]) -> Index {
        let metas: Vec<FileMeta> = files
            .iter()
            .map(|(p, _)| FileMeta { path: (*p).to_string(), sha: "s".into(), size: 0 })
            .collect();
        let contents: Vec<(String, String)> = files
            .iter()
            .map(|(p, c)| ((*p).to_string(), (*c).to_string()))
            .collect();
        let mut idx = Index::new();
        idx.apply(contents, &metas);
        idx
    }

    fn sample() -> Index {
        vault(&[
            ("notes/Coffee.md", "---\ntags: [drink]\n---\n\n# Brewing\n\nGrind beans fresh."),
            ("notes/Tea.md", "# Tea\n\nGreen tea is nice.\nBlack tea is also nice.\n"),
            ("Journal.md", "---\ntags: [daily]\n---\n\nDrank coffee today. Coffee again."),
        ])
    }

    #[test]
    fn finds_notes_by_body_text() {
        let hits = search(&sample(), "beans");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "notes/Coffee.md");
        assert_eq!(hits[0].kind, MatchKind::Body);
        assert_eq!(hits[0].fragment, "Grind beans fresh.");
    }

    #[test]
    fn a_title_match_outranks_a_body_match() {
        // "coffee" is the title of one note and body text in another.
        let hits = search(&sample(), "coffee");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].path, "notes/Coffee.md", "title hit comes first");
        assert_eq!(hits[0].kind, MatchKind::Title);
        assert_eq!(hits[1].path, "Journal.md");
        assert_eq!(hits[1].kind, MatchKind::Body);
    }

    #[test]
    fn search_is_case_insensitive_and_matches_partial_words() {
        // Half-typed queries have to work, or results can't narrow as you type.
        for q in ["GREEN", "gree", "Green Tea"] {
            let hits = search(&sample(), q);
            assert_eq!(hits.len(), 1, "query {q:?}");
            assert_eq!(hits[0].path, "notes/Tea.md", "query {q:?}");
        }
    }

    #[test]
    fn every_term_must_match_somewhere_in_the_note() {
        // "tea" and "coffee" both exist in the vault, but not in one note.
        assert!(search(&sample(), "tea coffee").is_empty());
        // Terms may match different parts: title + body.
        let hits = search(&sample(), "coffee brewing");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "notes/Coffee.md");
    }

    #[test]
    fn headings_and_tags_are_searchable() {
        let heading = search(&sample(), "brewing");
        assert_eq!(heading[0].kind, MatchKind::Heading);
        assert_eq!(heading[0].fragment, "Brewing");

        let tag = search(&sample(), "daily");
        assert_eq!(tag[0].path, "Journal.md");
        assert_eq!(tag[0].kind, MatchKind::Tag);
        assert_eq!(tag[0].fragment, "#daily");
    }

    #[test]
    fn frontmatter_is_not_body_text() {
        // Otherwise every note with a `tags:` key matches "tags".
        assert!(search(&sample(), "tags").is_empty());
    }

    #[test]
    fn ties_break_on_match_count_then_path_so_the_order_is_stable() {
        let idx = vault(&[
            ("a.md", "x\nx\nx"),
            ("b.md", "x\nx\nx\nx"),
            ("c.md", "x"),
        ]);
        let hits = search(&idx, "x");
        let paths: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
        assert_eq!(paths, ["b.md", "a.md", "c.md"]);
    }

    #[test]
    fn an_empty_or_blank_query_matches_nothing() {
        assert!(search(&sample(), "").is_empty());
        assert!(search(&sample(), "   ").is_empty());
    }

    #[test]
    fn long_lines_are_cut_at_a_char_boundary() {
        // A naive byte slice would panic here: 'ö' spans two bytes, and the cut
        // lands mid-character.
        let body = "ö".repeat(400);
        let idx = vault(&[("Swedish.md", &format!("hit {body}"))]);
        let hits = search(&idx, "hit");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].fragment.ends_with('…'));
        assert_eq!(hits[0].fragment.chars().count(), FRAGMENT_MAX + 1);
    }
}
