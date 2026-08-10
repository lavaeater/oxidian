//! Persistence and refresh for the shared vault index.
//!
//! `index` is pure — it parses and diffs but never fetches or stores. This
//! module is the platform half: it loads the index from local storage, reads
//! the files the index says are stale, and writes it back.
//!
//! Nothing here is authoritative. The repo is the source of truth and the index
//! is a cache of it, so a wiped or corrupt store costs one slow refresh and can
//! never cost data. See `docs/dataview.md` §6.
//!
//! The index is the one thing here that scales with the vault, so it lives in
//! the *blob* store (`js::blob_*`): `IndexedDB` on web, its own file on native.
//! `localStorage` was the original home and is the wrong shape for this — a ~5 MB
//! cap (about 3–5 k notes) and a synchronous write on the UI thread. An index
//! left there by an older build is migrated on first load.

use dioxus::prelude::*;
use index::Index;
use vault::{FileMeta, GithubConfig};

const KEY: &str = "oxidian_index_v1";
/// Superseded by `KEY`; removed on first load so it stops occupying quota.
const LEGACY_TASKS_KEY: &str = "oxidian_tasks_cache";

/// How many files to read concurrently when refreshing.
const READ_CONCURRENCY: usize = 8;

/// The live index, shared by every view that queries the vault.
///
/// Kept in memory as well as in the store because dataview blocks render
/// *synchronously*, inside the editor's paint — they cannot wait on a load.
/// Call once, at the vault root.
pub fn provide() -> Signal<Index> {
    use_context_provider(|| Signal::new(Index::new()))
}

/// The shared index provided by [`provide`].
pub fn use_index() -> Signal<Index> {
    use_context()
}

pub async fn load() -> Index {
    let raw = crate::js::blob_get(KEY).await;
    if !raw.is_empty() {
        return Index::from_json(&raw);
    }
    migrate_from_local_storage().await
}

/// Adopt an index written by a build that kept it in `localStorage`, then clear
/// it from there — it is dead weight against a 5 MB budget the settings still
/// share. Also drops the pre-index tasks cache. Returns an empty index when
/// there is nothing to migrate, which is the normal path.
async fn migrate_from_local_storage() -> Index {
    crate::js::ls_remove(LEGACY_TASKS_KEY);
    let legacy = crate::js::ls_get(KEY).await;
    if legacy.is_empty() {
        return Index::new();
    }
    let idx = Index::from_json(&legacy);
    crate::js::blob_set(KEY, legacy).await;
    crate::js::ls_remove(KEY);
    idx
}

pub async fn save(idx: &Index) {
    if let Some(json) = idx.to_json() {
        crate::js::blob_set(KEY, json).await;
    }
}

/// Drop the stored index. It is a cache, so this only costs one slow refresh —
/// the escape hatch when a user suspects it has gone stale or wants the space.
pub async fn clear() {
    crate::js::blob_remove(KEY).await;
    crate::js::ls_remove(KEY);
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
    save(&idx).await;
    idx
}

/// Forget one note so the next refresh re-reads it. Call after writing a file:
/// the remote blob SHA has changed, but the listing we hold is the pre-write
/// one, so SHA comparison alone would consider the entry fresh.
///
/// Returns the updated index so the caller can publish it to [`use_index`] —
/// these take no `Signal` themselves so they stay callable from anywhere.
pub async fn invalidate(path: &str) -> Index {
    let mut idx = load().await;
    if idx.invalidate(path) {
        save(&idx).await;
    }
    idx
}

/// Update one note in place from content we already have, without a round trip.
/// Used on save, so the index reflects the edit immediately. The SHA recorded is
/// the note's new blob SHA as reported by the write.
/// Returns the updated index, as [`invalidate`] does.
pub async fn update_page(path: &str, sha: &str, content: &str) -> Index {
    let mut idx = load().await;
    idx.upsert(path, sha, content);
    save(&idx).await;
    idx
}
