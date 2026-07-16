//! Local, disposable accelerator for the vault-wide Tasks scan.
//!
//! `vault::dispatch::list_files` already returns each file's git **blob SHA**
//! (a content hash) from a single recursive tree request we make anyway to build
//! the file tree. We key cached parsed tasks on that SHA, so re-opening the
//! Tasks view only re-reads files whose content actually changed — a steady
//! vault does zero file reads on the second open.
//!
//! The cache lives in the same KV store as everything else (`localStorage` on
//! web, `native_store` JSON on Android/desktop), never in the repo. It is purely
//! local and self-healing: a missing or stale entry just triggers a re-read, so
//! it can never produce a wrong answer and is safe to drop at any time.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use vault::{FileMeta, GithubConfig};

use crate::tasks::{self, Task};

const CACHE_KEY: &str = "oxidian_tasks_cache";

#[derive(Default, Serialize, Deserialize)]
struct Cache {
    /// path -> last-seen blob SHA + the tasks parsed from that content.
    files: HashMap<String, Entry>,
}

#[derive(Serialize, Deserialize)]
struct Entry {
    sha: String,
    tasks: Vec<Task>,
}

impl Cache {
    /// Paths that must be re-read: files we've never seen, or whose blob SHA
    /// has changed since they were last cached.
    fn stale(&self, md: &[&FileMeta]) -> Vec<String> {
        md.iter()
            .filter(|f| self.files.get(&f.path).map(|e| e.sha != f.sha).unwrap_or(true))
            .map(|f| f.path.clone())
            .collect()
    }

    /// Fold freshly-read `(path, content)` pairs into the cache (parsing tasks
    /// and recording each file's current SHA), then forget any file no longer
    /// present in the vault.
    fn apply(&mut self, fresh: Vec<(String, String)>, md: &[&FileMeta]) {
        let sha_by_path: HashMap<&str, &str> =
            md.iter().map(|f| (f.path.as_str(), f.sha.as_str())).collect();
        for (path, content) in fresh {
            let tasks = tasks::parse_file(&path, &content);
            let sha = sha_by_path.get(path.as_str()).map(|s| s.to_string()).unwrap_or_default();
            self.files.insert(path, Entry { sha, tasks });
        }
        let present: HashSet<&str> = md.iter().map(|f| f.path.as_str()).collect();
        self.files.retain(|p, _| present.contains(p.as_str()));
    }

    fn all_tasks(&self) -> Vec<Task> {
        self.files.values().flat_map(|e| e.tasks.iter().cloned()).collect()
    }
}

async fn load() -> Cache {
    let raw = crate::js::ls_get(CACHE_KEY).await;
    if raw.is_empty() {
        return Cache::default();
    }
    serde_json::from_str(&raw).unwrap_or_default()
}

fn save(cache: &Cache) {
    if let Ok(json) = serde_json::to_string(cache) {
        crate::js::ls_set(CACHE_KEY, json);
    }
}

/// Scan every markdown file for tasks, reusing cached results for files whose
/// blob SHA is unchanged since the last scan. Only changed/new files are read
/// from the host (up to 8 concurrently); deleted files are dropped from the
/// cache. The updated cache is persisted before returning.
pub async fn scan(cfg: &GithubConfig, files: &[FileMeta]) -> Vec<Task> {
    let md: Vec<&FileMeta> = files.iter().filter(|f| f.path.ends_with(".md")).collect();
    let mut cache = load().await;

    let stale = cache.stale(&md);
    let mut fresh = Vec::new();
    for chunk in stale.chunks(8) {
        let futs = chunk.iter().map(|p| {
            let p = p.clone();
            async move {
                vault::dispatch::read_file(cfg, &p)
                    .await
                    .ok()
                    .map(|fc| (p, fc.content))
            }
        });
        fresh.extend(futures::future::join_all(futs).await.into_iter().flatten());
    }

    cache.apply(fresh, &md);
    save(&cache);
    cache.all_tasks()
}

/// Drop one file from the cache so the next scan re-reads it from the host.
/// Called after a task toggle writes the file back: the remote blob SHA has
/// changed, but the locally-held `files` list may still carry the old SHA, so we
/// can't rely on SHA comparison to notice — invalidating forces a fresh read.
pub async fn invalidate(path: &str) {
    let mut cache = load().await;
    if cache.files.remove(path).is_some() {
        save(&cache);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(path: &str, sha: &str) -> FileMeta {
        FileMeta { path: path.into(), sha: sha.into(), size: 0 }
    }

    /// Build the borrowed `md` slice `stale`/`apply` expect from owned metas.
    fn refs(metas: &[FileMeta]) -> Vec<&FileMeta> {
        metas.iter().collect()
    }

    #[test]
    fn unseen_and_changed_files_are_stale() {
        let mut cache = Cache::default();
        let metas = vec![meta("a.md", "sha-a"), meta("b.md", "sha-b")];
        // Cold cache: everything is stale.
        assert_eq!(cache.stale(&refs(&metas)), vec!["a.md", "b.md"]);

        cache.apply(
            vec![("a.md".into(), "- [ ] x".into()), ("b.md".into(), "- [ ] y".into())],
            &refs(&metas),
        );

        // Unchanged SHAs: nothing stale.
        assert!(cache.stale(&refs(&metas)).is_empty());

        // Bump b's SHA (content changed) and add a new file: only those re-read.
        let metas2 = vec![meta("a.md", "sha-a"), meta("b.md", "sha-b2"), meta("c.md", "sha-c")];
        assert_eq!(cache.stale(&refs(&metas2)), vec!["b.md", "c.md"]);
    }

    #[test]
    fn apply_reuses_unread_files_and_updates_read_ones() {
        let mut cache = Cache::default();
        let metas = vec![meta("a.md", "sha-a"), meta("b.md", "sha-b")];
        cache.apply(
            vec![("a.md".into(), "- [ ] keep".into()), ("b.md".into(), "- [ ] old".into())],
            &refs(&metas),
        );

        // b changed; re-read only b. a is not in `fresh`, so its cached task survives.
        let metas2 = vec![meta("a.md", "sha-a"), meta("b.md", "sha-b2")];
        cache.apply(vec![("b.md".into(), "- [ ] new".into())], &refs(&metas2));

        let mut texts: Vec<String> = cache.all_tasks().into_iter().map(|t| t.text).collect();
        texts.sort();
        assert_eq!(texts, vec!["keep", "new"]);
        assert_eq!(cache.files["b.md"].sha, "sha-b2");
    }

    #[test]
    fn deleted_files_are_evicted() {
        let mut cache = Cache::default();
        let metas = vec![meta("a.md", "sha-a"), meta("b.md", "sha-b")];
        cache.apply(
            vec![("a.md".into(), "- [ ] a".into()), ("b.md".into(), "- [ ] b".into())],
            &refs(&metas),
        );

        // b.md is gone from the vault; an apply with no fresh reads drops it.
        let metas2 = vec![meta("a.md", "sha-a")];
        cache.apply(vec![], &refs(&metas2));

        assert!(cache.files.contains_key("a.md"));
        assert!(!cache.files.contains_key("b.md"));
        assert_eq!(cache.all_tasks().len(), 1);
    }

    #[test]
    fn serialization_round_trips() {
        let mut cache = Cache::default();
        let metas = vec![meta("a.md", "sha-a")];
        cache.apply(
            vec![("a.md".into(), "- [ ] Pay rent 📅 2026-06-15 ⏫".into())],
            &refs(&metas),
        );

        let json = serde_json::to_string(&cache).unwrap();
        let back: Cache = serde_json::from_str(&json).unwrap();
        let t = &back.files["a.md"].tasks[0];
        assert_eq!(t.text, "Pay rent");
        assert_eq!(t.due.as_deref(), Some("2026-06-15"));
        assert_eq!(t.priority, crate::tasks::Priority::High);
        assert_eq!(back.files["a.md"].sha, "sha-a");
    }
}
