/// In-memory `WikiLink` graph built from vault content.
///
/// We fetch file content lazily: files are added as the user opens them
/// (content is already loaded), and a full index build can be triggered
/// explicitly (fetches all remaining files).
/// A single directed link: `source_path` → `target_title`.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub source: String,
    pub target: String,
}

/// The index: a list of all discovered links.
#[derive(Clone, Default, PartialEq)]
pub struct WikiLinkIndex {
    pub links: Vec<Link>,
    /// Paths whose content has been indexed.
    pub indexed: std::collections::HashSet<String>,
}

impl WikiLinkIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Index the links in a single file.  No-op if already indexed.
    pub fn index_file(&mut self, path: &str, content: &str) {
        if self.indexed.contains(path) {
            return;
        }
        self.indexed.insert(path.to_string());
        // Remove old links from this source (in case we re-index after edit)
        self.links.retain(|l| l.source != path);
        // Extract [[target]] and [[target|label]] patterns
        let mut rest = content;
        while let Some(start) = rest.find("[[") {
            rest = &rest[start + 2..];
            if let Some(end) = rest.find("]]") {
                let inner = &rest[..end];
                let target = inner.split('|').next().unwrap_or(inner).trim();
                if !target.is_empty() {
                    self.links.push(Link {
                        source: path.to_string(),
                        target: target.to_string(),
                    });
                }
                rest = &rest[end + 2..];
            } else {
                break;
            }
        }
    }

    /// Re-index a file (used after the user edits it).
    pub fn reindex_file(&mut self, path: &str, content: &str) {
        self.indexed.remove(path);
        self.index_file(path, content);
    }

    /// All files that link TO `target_title` (case-insensitive stem match).
    pub fn backlinks(&self, path: &str) -> Vec<&str> {
        let stem = stem(path);
        self.links
            .iter()
            .filter(|l| l.source != path && stems_match(&l.target, &stem))
            .map(|l| l.source.as_str())
            .collect()
    }

    /// All files that `path` links TO, resolved to actual paths from `all_files`.
    pub fn outlinks<'a>(&self, path: &str, all_files: &'a [String]) -> Vec<&'a str> {
        let targets: Vec<&str> = self.links
            .iter()
            .filter(|l| l.source == path)
            .map(|l| l.target.as_str())
            .collect();
        all_files
            .iter()
            .filter(|f| {
                let s = stem(f);
                targets.iter().any(|t| stems_match(t, &s))
            })
            .map(std::string::String::as_str)
            .collect()
    }

    /// Edges for graph rendering: (`source_path`, `target_path`) resolved pairs.
    pub fn edges(&self, all_files: &[String]) -> Vec<(String, String)> {
        self.links
            .iter()
            .filter_map(|l| {
                let target_stem = l.target.to_lowercase();
                let target_path = all_files.iter().find(|f| stem(f) == target_stem)?;
                Some((l.source.clone(), target_path.clone()))
            })
            .collect()
    }
}

/// Extract the note "stem" from a path: lowercase filename without extension.
fn stem(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.trim_end_matches(".md").to_lowercase()
}

fn stems_match(link_target: &str, file_stem: &str) -> bool {
    link_target.to_lowercase() == *file_stem
}

#[cfg(test)]
mod tests {
    use super::*;

    fn targets_of<'a>(idx: &'a WikiLinkIndex, source: &str) -> Vec<&'a str> {
        idx.links
            .iter()
            .filter(|l| l.source == source)
            .map(|l| l.target.as_str())
            .collect()
    }

    #[test]
    fn extracts_simple_and_labeled_links() {
        let mut idx = WikiLinkIndex::new();
        idx.index_file("a.md", "see [[Target]] and [[Other|a label]] here");
        assert_eq!(targets_of(&idx, "a.md"), vec!["Target", "Other"]);
    }

    #[test]
    fn trims_and_skips_empty_targets() {
        let mut idx = WikiLinkIndex::new();
        idx.index_file("a.md", "[[  Spaced  ]] then [[]] then [[   ]]");
        // Empty / whitespace-only targets are dropped; real one is trimmed.
        assert_eq!(targets_of(&idx, "a.md"), vec!["Spaced"]);
    }

    #[test]
    fn unclosed_bracket_stops_scanning() {
        let mut idx = WikiLinkIndex::new();
        idx.index_file("a.md", "[[Good]] then [[Unclosed and more text");
        assert_eq!(targets_of(&idx, "a.md"), vec!["Good"]);
    }

    #[test]
    fn index_file_is_noop_when_already_indexed() {
        let mut idx = WikiLinkIndex::new();
        idx.index_file("a.md", "[[First]]");
        // Second call with different content is ignored (already indexed).
        idx.index_file("a.md", "[[Second]]");
        assert_eq!(targets_of(&idx, "a.md"), vec!["First"]);
    }

    #[test]
    fn reindex_replaces_stale_links() {
        let mut idx = WikiLinkIndex::new();
        idx.index_file("a.md", "[[First]] [[Shared]]");
        idx.reindex_file("a.md", "[[Second]] [[Shared]]");
        assert_eq!(targets_of(&idx, "a.md"), vec!["Second", "Shared"]);
    }

    #[test]
    fn backlinks_are_case_insensitive_and_exclude_self() {
        let mut idx = WikiLinkIndex::new();
        idx.index_file("notes/a.md", "[[Target]]");
        idx.index_file("notes/b.md", "[[target]]"); // lowercase still matches
        idx.index_file("notes/Target.md", "[[Target]]"); // self-link excluded
        let mut back = idx.backlinks("notes/Target.md");
        back.sort_unstable();
        assert_eq!(back, vec!["notes/a.md", "notes/b.md"]);
    }

    #[test]
    fn outlinks_resolve_targets_to_paths() {
        let mut idx = WikiLinkIndex::new();
        idx.index_file("a.md", "[[Target]] [[Missing]]");
        let all = vec![
            "a.md".to_string(),
            "sub/Target.md".to_string(),
            "unrelated.md".to_string(),
        ];
        assert_eq!(idx.outlinks("a.md", &all), vec!["sub/Target.md"]);
    }

    #[test]
    fn edges_resolve_source_and_target_paths() {
        let mut idx = WikiLinkIndex::new();
        idx.index_file("notes/a.md", "[[Target]]");
        let all = vec!["notes/a.md".to_string(), "notes/Target.md".to_string()];
        assert_eq!(
            idx.edges(&all),
            vec![("notes/a.md".to_string(), "notes/Target.md".to_string())]
        );
    }

    #[test]
    fn edges_drops_unresolved_targets() {
        let mut idx = WikiLinkIndex::new();
        idx.index_file("a.md", "[[Nowhere]]");
        let all = vec!["a.md".to_string()];
        assert!(idx.edges(&all).is_empty());
    }

    #[test]
    fn stem_strips_dir_and_extension() {
        assert_eq!(stem("notes/sub/My Note.md"), "my note");
        assert_eq!(stem("Plain"), "plain");
    }
}
