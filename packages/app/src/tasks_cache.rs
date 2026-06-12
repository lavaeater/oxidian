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

    // Files whose content changed (SHA differs) or that we've never seen.
    let stale: Vec<String> = md
        .iter()
        .filter(|f| cache.files.get(&f.path).map(|e| e.sha != f.sha).unwrap_or(true))
        .map(|f| f.path.clone())
        .collect();

    let sha_by_path: HashMap<&str, &str> =
        md.iter().map(|f| (f.path.as_str(), f.sha.as_str())).collect();

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
        for (path, content) in futures::future::join_all(futs).await.into_iter().flatten() {
            let parsed = tasks::parse_file(&path, &content);
            let sha = sha_by_path.get(path.as_str()).map(|s| s.to_string()).unwrap_or_default();
            cache.files.insert(path, Entry { sha, tasks: parsed });
        }
    }

    // Forget files that no longer exist in the vault.
    let present: HashSet<&str> = md.iter().map(|f| f.path.as_str()).collect();
    cache.files.retain(|p, _| present.contains(p.as_str()));

    save(&cache);

    cache.files.values().flat_map(|e| e.tasks.iter().cloned()).collect()
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
