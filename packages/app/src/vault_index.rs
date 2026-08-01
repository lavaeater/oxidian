//! Persistence and refresh for the shared vault index.
//!
//! `index` is pure — it parses and diffs but never fetches or stores. This
//! module is the platform half: it loads the index from the local KV store
//! (`localStorage` on web, `native_store`'s JSON file on Android/desktop),
//! reads the files the index says are stale, and writes it back.
//!
//! Nothing here is authoritative. The repo is the source of truth and the index
//! is a cache of it, so a wiped or corrupt store costs one slow refresh and can
//! never cost data. See `docs/dataview.md` §6.
//!
//! ⚠️ The KV store is `localStorage` on web today, which caps out around 5 MB —
//! roughly a 3–5 k note vault. Moving the index to IndexedDB is phase 1.

use index::Index;
use vault::{FileMeta, GithubConfig};

const KEY: &str = "oxidian_index_v1";
/// Superseded by `KEY`; removed on first load so it stops occupying quota.
const LEGACY_TASKS_KEY: &str = "oxidian_tasks_cache";

/// How many files to read concurrently when refreshing.
const READ_CONCURRENCY: usize = 8;

pub async fn load() -> Index {
    let raw = crate::js::ls_get(KEY).await;
    if raw.is_empty() {
        // One-time cleanup of the pre-index tasks cache.
        crate::js::ls_remove(LEGACY_TASKS_KEY);
        return Index::new();
    }
    Index::from_json(&raw)
}

pub fn save(idx: &Index) {
    if let Some(json) = idx.to_json() {
        crate::js::ls_set(KEY, json);
    }
}

/// Bring the index up to date with `files` (the current vault listing) and
/// persist it.
///
/// Only notes whose blob SHA changed are re-read, so a vault that hasn't
/// changed since the last refresh performs **zero** file reads — the one tree
/// request the caller already made to produce `files` is the entire cost.
pub async fn refresh(cfg: &GithubConfig, files: &[FileMeta]) -> Index {
    let mut idx = load().await;
    let stale = idx.stale(files);

    let mut fresh = Vec::with_capacity(stale.len());
    for chunk in stale.chunks(READ_CONCURRENCY) {
        let futs = chunk.iter().map(|path| {
            let path = path.clone();
            async move {
                vault::dispatch::read_file(cfg, &path)
                    .await
                    .ok()
                    .map(|fc| (path, fc.content))
            }
        });
        fresh.extend(futures::future::join_all(futs).await.into_iter().flatten());
    }

    idx.apply(fresh, files);
    save(&idx);
    idx
}

/// Forget one note so the next refresh re-reads it. Call after writing a file:
/// the remote blob SHA has changed, but the listing we hold is the pre-write
/// one, so SHA comparison alone would consider the entry fresh.
pub async fn invalidate(path: &str) {
    let mut idx = load().await;
    if idx.invalidate(path) {
        save(&idx);
    }
}

/// Update one note in place from content we already have, without a round trip.
/// Used on save, so the index reflects the edit immediately. The SHA recorded is
/// the note's new blob SHA as reported by the write.
pub async fn update_page(path: &str, sha: &str, content: &str) {
    let mut idx = load().await;
    idx.upsert(path, sha, content);
    save(&idx);
}
