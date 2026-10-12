//! The shared vault index.
//!
//! One parsed model of the whole vault, refreshed incrementally, that the Tasks
//! view, backlinks, the graph, a future tags pane, and Dataview all read from —
//! instead of each maintaining its own partial view. See `docs/dataview.md`.
//!
//! **This crate does no I/O.** It parses content into [`PageData`], stores it
//! keyed by path, and answers "which files must I re-read?" ([`Index::stale`]).
//! Fetching and persisting belong to the caller (`app::vault_index`), which is
//! what keeps every layer here a pure function that tests without a renderer,
//! a network, or a browser.
//!
//! The refresh loop, and why it is cheap:
//!
//! 1. `vault::dispatch::list_files` returns the whole tree in **one** request,
//!    including a git **blob SHA** per file.
//! 2. [`Index::stale`] diffs those SHAs against what is already indexed.
//! 3. The caller re-reads only the stale paths and hands them to [`Index::apply`].
//!
//! A blob SHA is a content hash, so an unchanged file is *provably* unchanged:
//! a stale entry is impossible, not merely unlikely. Opening a settled vault
//! therefore costs a single HTTP request.

pub mod dql;
pub mod extract;
pub mod frontmatter;
pub mod search;
pub mod tasks;
pub mod value;

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use vault::FileMeta;

pub use extract::PageData;
pub use value::{Date, Value};
pub use tasks::{Priority, Task};

/// Bumped whenever the on-disk shape changes; a mismatch discards the cache
/// rather than trying to migrate it. Safe, because the index is only ever a
/// cache of the repo — throwing it away costs one slow refresh, never data.
// 2 adds `PageData::text` (the note body) so search can run locally. An index
// written by an older build lacks it and is discarded rather than half-usable:
// `from_json` returns an empty index on a version mismatch and the next refresh
// rebuilds it.
// 3 replaces `Task::checked` (a bool) with `Task::status`, so an entry can be
// migrated or dropped rather than only done — the Bullet Journal signifiers,
// see `docs/bujo-roadmap.md` §3. Records written by an older build would
// deserialise into the wrong shape, so they are discarded like any other
// version bump.
pub const FORMAT_VERSION: u32 = 3;

/// What an [`Index::apply`] changed, so the caller can persist just that.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Changed {
    /// Notes written or rewritten.
    pub updated: Vec<String>,
    /// Notes that left the vault and must be dropped from the store too.
    pub removed: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Index {
    #[serde(default)]
    version: u32,
    /// path → extracted page.
    pages: BTreeMap<String, PageData>,
}

impl Index {
    pub fn new() -> Self {
        Self { version: FORMAT_VERSION, pages: BTreeMap::new() }
    }

    /// Restore from a whole-index JSON payload. An unreadable or outdated
    /// payload yields an empty index, which just means the next refresh reads
    /// everything.
    ///
    /// Only used to adopt an index written by a build that stored it as one
    /// blob; the store is per-note now (`from_records`).
    pub fn from_json(raw: &str) -> Self {
        match serde_json::from_str::<Index>(raw) {
            Ok(idx) if idx.version == FORMAT_VERSION => idx,
            _ => Self::new(),
        }
    }

    /// Rebuild from persisted per-note records, `(path, json)`.
    ///
    /// A record that won't parse is dropped rather than failing the load: the
    /// index is a cache, so one bad record costs one re-read of one note, and
    /// the alternative — discarding the whole vault's index — costs far more.
    pub fn from_records<I>(records: I) -> Self
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let mut idx = Self::new();
        for (path, json) in records {
            if let Ok(page) = serde_json::from_str::<PageData>(&json) {
                idx.pages.insert(path, page);
            }
        }
        idx
    }

    /// One note's persisted form.
    pub fn page_json(&self, path: &str) -> Option<String> {
        self.pages.get(path).and_then(|p| serde_json::to_string(p).ok())
    }

    pub fn len(&self) -> usize {
        self.pages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    pub fn get(&self, path: &str) -> Option<&PageData> {
        self.pages.get(path)
    }

    pub fn pages(&self) -> impl Iterator<Item = &PageData> {
        self.pages.values()
    }

    /// Paths that must be re-read: notes we have never seen, or whose blob SHA
    /// changed since they were indexed. Non-markdown entries are ignored.
    pub fn stale(&self, files: &[FileMeta]) -> Vec<String> {
        notes(files)
            .filter(|f| self.pages.get(&f.path).is_none_or(|p| p.sha != f.sha))
            .map(|f| f.path.clone())
            .collect()
    }

    /// Fold freshly-read `(path, content)` pairs in, then forget any note no
    /// longer present in `files`.
    ///
    /// Reports exactly what moved so the caller can persist that and nothing
    /// else — the difference between a write proportional to the change and one
    /// proportional to the vault.
    pub fn apply(&mut self, fresh: Vec<(String, String)>, files: &[FileMeta]) -> Changed {
        let sha_by_path: BTreeMap<&str, &str> =
            notes(files).map(|f| (f.path.as_str(), f.sha.as_str())).collect();
        let mut updated = Vec::with_capacity(fresh.len());
        for (path, content) in fresh {
            let sha = sha_by_path.get(path.as_str()).copied().unwrap_or_default();
            self.pages.insert(path.clone(), extract::extract(&path, sha, &content));
            updated.push(path);
        }
        let present: HashSet<&str> = sha_by_path.keys().copied().collect();
        let removed: Vec<String> = self
            .pages
            .keys()
            .filter(|p| !present.contains(p.as_str()))
            .cloned()
            .collect();
        self.pages.retain(|p, _| present.contains(p.as_str()));
        self.version = FORMAT_VERSION;
        Changed { updated, removed }
    }

    /// Insert or replace a single note from content the caller already holds,
    /// leaving the rest of the index alone. This is deliberately *not*
    /// `apply` with a one-element listing: `apply` also evicts everything
    /// absent from the listing it is given, which for one file would empty the
    /// index.
    pub fn upsert(&mut self, path: &str, sha: &str, content: &str) {
        self.pages.insert(path.to_string(), extract::extract(path, sha, content));
    }

    /// Drop one note so the next refresh re-reads it. Needed after *we* write a
    /// file: the remote blob SHA has changed, but the file listing we hold is
    /// still the pre-write one, so SHA comparison alone would call it fresh.
    pub fn invalidate(&mut self, path: &str) -> bool {
        self.pages.remove(path).is_some()
    }

    // ── Derived views ────────────────────────────────────────────────────────

    /// Every task in the vault, unsorted (`tasks::cmp` orders them).
    pub fn tasks(&self) -> Vec<Task> {
        self.pages.values().flat_map(|p| p.tasks.iter().cloned()).collect()
    }

    /// Every distinct tag with the number of notes carrying it, most used first
    /// then alphabetical. This is the Tags pane (US 14) in one call.
    pub fn tag_counts(&self) -> Vec<(String, usize)> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for page in self.pages.values() {
            for tag in &page.tags {
                *counts.entry(tag.to_lowercase()).or_default() += 1;
            }
        }
        let mut out: Vec<(String, usize)> = counts.into_iter().collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        out
    }

    /// Notes carrying `tag` (case-insensitive, `#` optional).
    pub fn pages_with_tag(&self, tag: &str) -> Vec<&PageData> {
        self.pages.values().filter(|p| p.has_tag(tag)).collect()
    }

    /// Notes directly under `folder` and its subfolders. `""` means the whole
    /// vault, which is what a bare `FROM ""` source asks for.
    pub fn pages_in_folder(&self, folder: &str) -> Vec<&PageData> {
        let folder = folder.trim_matches('/');
        if folder.is_empty() {
            return self.pages.values().collect();
        }
        let prefix = format!("{folder}/");
        self.pages.values().filter(|p| p.path.starts_with(&prefix)).collect()
    }

    /// Paths that link to `path`, matched on the note's stem the way wikilinks
    /// are written. Self-links are excluded.
    pub fn backlinks(&self, path: &str) -> Vec<&str> {
        let Some(target) = self.pages.get(path) else { return Vec::new() };
        let stem = target.stem().to_lowercase();
        self.pages
            .values()
            .filter(|p| p.path != path && p.links.iter().any(|l| l.to_lowercase() == stem))
            .map(|p| p.path.as_str())
            .collect()
    }

    /// Resolved `(source_path, target_path)` pairs for graph rendering.
    /// Links pointing at notes that don't exist are dropped.
    pub fn edges(&self) -> Vec<(String, String)> {
        let by_stem: BTreeMap<String, &str> = self
            .pages
            .values()
            .map(|p| (p.stem().to_lowercase(), p.path.as_str()))
            .collect();
        let mut edges = Vec::new();
        for page in self.pages.values() {
            for link in &page.links {
                if let Some(target) = by_stem.get(&link.to_lowercase()) {
                    edges.push((page.path.clone(), (*target).to_string()));
                }
            }
        }
        edges
    }
}

/// Markdown notes only — the listing also carries `.gitkeep` placeholders so
/// empty folders survive in the file tree.
fn notes(files: &[FileMeta]) -> impl Iterator<Item = &FileMeta> {
    files.iter().filter(|f| {
        std::path::Path::new(&f.path)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(path: &str, sha: &str) -> FileMeta {
        FileMeta { path: path.into(), sha: sha.into(), size: 0 }
    }

    fn seeded() -> (Index, Vec<FileMeta>) {
        let files = vec![
            meta("games/Deus Ex.md", "sha-d"),
            meta("games/Thief.md", "sha-t"),
            meta("notes/Alice.md", "sha-a"),
            meta("empty/.gitkeep", "sha-k"),
        ];
        let mut idx = Index::new();
        idx.apply(
            vec![
                (
                    "games/Deus Ex.md".into(),
                    "---\ntags: [games]\n---\nPlayed with [[Alice]]\n- [ ] replay\n".into(),
                ),
                ("games/Thief.md".into(), "#games and [[Deus Ex]]\n".into()),
                ("notes/Alice.md".into(), "A person. [[Deus Ex]] was hers.\n".into()),
            ],
            &files,
        );
        (idx, files)
    }

    #[test]
    fn cold_index_is_entirely_stale_then_settles() {
        let (idx, files) = seeded();
        // Everything read; nothing left to do on the next open.
        assert!(idx.stale(&files).is_empty());
        // .gitkeep is not a note and never enters the index.
        assert_eq!(idx.len(), 3);
        assert!(idx.get("empty/.gitkeep").is_none());
    }

    #[test]
    fn only_changed_and_new_files_are_stale() {
        let (idx, _) = seeded();
        let next = vec![
            meta("games/Deus Ex.md", "sha-d"),   // unchanged
            meta("games/Thief.md", "sha-t2"),    // edited
            meta("notes/Alice.md", "sha-a"),     // unchanged
            meta("games/System Shock.md", "sha-s"), // new
        ];
        // Order follows the vault listing, so reads happen in tree order.
        assert_eq!(idx.stale(&next), vec!["games/Thief.md", "games/System Shock.md"]);
    }

    #[test]
    fn apply_keeps_unread_pages_and_evicts_deleted_ones() {
        let (mut idx, _) = seeded();
        let next = vec![meta("games/Deus Ex.md", "sha-d"), meta("games/Thief.md", "sha-t2")];
        idx.apply(vec![("games/Thief.md".into(), "rewritten #stealth\n".into())], &next);

        // Alice is gone from the vault → gone from the index.
        assert!(idx.get("notes/Alice.md").is_none());
        // Deus Ex was not re-read but survives intact.
        assert_eq!(idx.get("games/Deus Ex.md").unwrap().links, vec!["Alice"]);
        // Thief picked up its new content and SHA.
        let thief = idx.get("games/Thief.md").unwrap();
        assert_eq!(thief.tags, vec!["stealth"]);
        assert_eq!(thief.sha, "sha-t2");
    }

    #[test]
    fn upsert_touches_one_page_and_keeps_the_rest() {
        let (mut idx, _) = seeded();
        idx.upsert("games/Thief.md", "sha-t2", "#stealth\n");
        assert_eq!(idx.len(), 3, "no other page may be evicted");
        let thief = idx.get("games/Thief.md").unwrap();
        assert_eq!(thief.tags, vec!["stealth"]);
        assert_eq!(thief.sha, "sha-t2");
    }

    #[test]
    fn invalidate_forces_a_reread_of_one_path() {
        let (mut idx, files) = seeded();
        assert!(idx.invalidate("games/Thief.md"));
        assert!(!idx.invalidate("games/Thief.md"), "already gone");
        assert_eq!(idx.stale(&files), vec!["games/Thief.md"]);
    }

    #[test]
    fn a_whole_index_payload_is_still_readable_for_migration() {
        // The store is per-note now; this path exists only to adopt an index
        // left behind by a build that wrote it as one blob.
        let (idx, files) = seeded();
        let whole = serde_json::to_string(&idx).unwrap();
        let back = Index::from_json(&whole);
        assert_eq!(back.len(), 3);
        assert!(back.stale(&files).is_empty());

        // Garbage, or a payload from an older format, degrades to a cold index
        // rather than to wrong answers.
        assert!(Index::from_json("not json").is_empty());
        assert!(Index::from_json(r#"{"version":0,"pages":{}}"#).is_empty());
    }

    #[test]
    fn records_round_trip_one_note_at_a_time() {
        let (idx, files) = seeded();
        let records: Vec<(String, String)> = idx
            .pages()
            .map(|p| (p.path.clone(), idx.page_json(&p.path).unwrap()))
            .collect();

        let back = Index::from_records(records);
        assert_eq!(back.len(), 3);
        // Restored well enough that nothing needs re-reading.
        assert!(back.stale(&files).is_empty());
        assert_eq!(back.tasks().len(), idx.tasks().len());
    }

    #[test]
    fn one_unreadable_record_costs_only_that_note() {
        // Dropping the whole vault's index over a single bad record would turn a
        // one-note re-read into a full re-download.
        let (idx, _) = seeded();
        let good = idx.pages().next().unwrap();
        let back = Index::from_records(vec![
            (good.path.clone(), idx.page_json(&good.path).unwrap()),
            ("broken.md".to_string(), "{{{".to_string()),
        ]);
        assert_eq!(back.len(), 1);
        assert!(back.get(&good.path).is_some());
        assert!(back.get("broken.md").is_none());
    }

    #[test]
    fn apply_reports_exactly_what_it_changed() {
        let (mut idx, files) = seeded();
        let paths: Vec<String> = files.iter().map(|f| f.path.clone()).collect();

        // Re-reading one note reports one update and nothing removed.
        let changed = idx.apply(vec![(paths[0].clone(), "# New".into())], &files);
        assert_eq!(changed.updated, vec![paths[0].clone()]);
        assert!(changed.removed.is_empty());

        // A note that left the listing is reported as removed, so the caller
        // can delete its record rather than leaking it forever.
        let fewer: Vec<FileMeta> = files.iter().filter(|f| f.path != paths[1]).cloned().collect();
        let changed = idx.apply(vec![], &fewer);
        assert!(changed.updated.is_empty());
        assert_eq!(changed.removed, vec![paths[1].clone()]);
        assert!(idx.get(&paths[1]).is_none());
    }

    #[test]
    fn tasks_are_aggregated_across_the_vault() {
        let (idx, _) = seeded();
        let tasks = idx.tasks();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].path, "games/Deus Ex.md");
    }

    #[test]
    fn tag_counts_rank_by_use_then_name() {
        let (idx, _) = seeded();
        assert_eq!(idx.tag_counts(), vec![("games".to_string(), 2)]);
        assert_eq!(idx.pages_with_tag("#GAMES").len(), 2);
    }

    #[test]
    fn folder_source_includes_subfolders_and_empty_means_all() {
        let (idx, _) = seeded();
        let paths: Vec<&str> = idx.pages_in_folder("games").iter().map(|p| p.path.as_str()).collect();
        assert_eq!(paths, vec!["games/Deus Ex.md", "games/Thief.md"]);
        assert_eq!(idx.pages_in_folder("").len(), 3);
        assert_eq!(idx.pages_in_folder("/games/").len(), 2, "slashes are trimmed");
    }

    #[test]
    fn backlinks_and_edges_resolve_through_note_stems() {
        let (idx, _) = seeded();
        let mut back = idx.backlinks("games/Deus Ex.md");
        back.sort_unstable();
        assert_eq!(back, vec!["games/Thief.md", "notes/Alice.md"]);

        let mut edges = idx.edges();
        edges.sort();
        assert_eq!(
            edges,
            vec![
                ("games/Deus Ex.md".to_string(), "notes/Alice.md".to_string()),
                ("games/Thief.md".to_string(), "games/Deus Ex.md".to_string()),
                ("notes/Alice.md".to_string(), "games/Deus Ex.md".to_string()),
            ]
        );
    }
}
