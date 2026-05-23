/// In-memory WikiLink graph built from vault content.
///
/// We fetch file content lazily: files are added as the user opens them
/// (content is already loaded), and a full index build can be triggered
/// explicitly (fetches all remaining files).

/// A single directed link: source_path → target_title.
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
            .map(|f| f.as_str())
            .collect()
    }

    /// Edges for graph rendering: (source_path, target_path) resolved pairs.
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
