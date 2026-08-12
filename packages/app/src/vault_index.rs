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
//! **Stored one record per note** (`js::records_*`: `IndexedDB`'s `pages` store on
//! web, a file each on native). The index carries every note's text so search
//! can run locally, which makes it roughly vault-sized — and the previous
//! whole-index blob meant every save deserialised it, changed one note, and
//! re-serialised the lot. Now a save reads nothing and writes one record.
//!
//! The live index is a `Signal` (see [`provide`]) rather than something reloaded
//! per operation, so these functions take it and mutate in place.

use dioxus::prelude::*;
use index::Index;
use vault::{FileMeta, GithubConfig};

/// Marks which format the stored records are in. A mismatch discards them
/// rather than half-reading them; it costs one refresh and can't cost data.
const VERSION_KEY: &str = "oxidian_index_version";
/// Where the whole index lived before it was split per note. Read once, to
/// adopt what's there instead of re-downloading the vault, then removed.
const LEGACY_BLOB_KEY: &str = "oxidian_index_v1";
/// Superseded by the index; removed on first load so it stops occupying quota.
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

/// Read the whole index from the store. Called once at startup.
pub async fn load() -> Index {
    crate::js::ls_remove(LEGACY_TASKS_KEY);

    let stored = crate::js::blob_get(VERSION_KEY).await;
    let current = index::FORMAT_VERSION.to_string();
    // An *absent* version is not a mismatch: it means a build that predates the
    // stamp, whose data we still want to adopt below. Only a version that is
    // present and different says the stored shape is one we can't read.
    if !stored.is_empty() && stored != current {
        crate::js::records_clear().await;
        crate::js::blob_remove(LEGACY_BLOB_KEY).await;
        crate::js::blob_set(VERSION_KEY, current).await;
        return Index::new();
    }
    crate::js::blob_set(VERSION_KEY, current).await;

    let records = crate::js::records_all().await;
    if !records.is_empty() {
        return Index::from_records(records);
    }
    migrate_whole_index().await
}

/// Adopt an index written as a single blob by an earlier build, splitting it
/// into records. Worth doing rather than starting cold: the blob is a complete,
/// current index, and rebuilding it means re-reading every note in the vault.
///
/// Checks `localStorage` too, for an install that skipped the build in between.
async fn migrate_whole_index() -> Index {
    let mut whole = crate::js::blob_get(LEGACY_BLOB_KEY).await;
    if whole.is_empty() {
        whole = crate::js::ls_get(LEGACY_BLOB_KEY).await;
    }
    if whole.is_empty() {
        return Index::new();
    }
    let idx = Index::from_json(&whole);
    if idx.is_empty() {
        // Unreadable or from a format we no longer understand; drop it.
        crate::js::blob_remove(LEGACY_BLOB_KEY).await;
        crate::js::ls_remove(LEGACY_BLOB_KEY);
        return idx;
    }
    persist_all(&idx).await;
    crate::js::blob_remove(LEGACY_BLOB_KEY).await;
    crate::js::ls_remove(LEGACY_BLOB_KEY);
    idx
}

/// Write every note's record. Only for seeding and migration — the normal path
/// writes just what changed.
async fn persist_all(idx: &Index) {
    let entries: Vec<(String, String)> = idx
        .pages()
        .filter_map(|p| Some((p.path.clone(), idx.page_json(&p.path)?)))
        .collect();
    crate::js::records_put(entries).await;
}

/// Drop the stored index. It is a cache, so this only costs one slow refresh —
/// the escape hatch when a user suspects it has gone stale or wants the space.
pub async fn clear() {
    crate::js::records_clear().await;
    crate::js::blob_remove(LEGACY_BLOB_KEY).await;
    crate::js::ls_remove(LEGACY_BLOB_KEY);
}

/// Bring the live index up to date with `files` (the current vault listing),
/// persisting only what changed.
///
/// Only notes whose blob SHA changed are re-read, so a vault that hasn't
/// changed since the last refresh performs **zero** file reads — the one tree
/// request the caller already made to produce `files` is the entire cost.
pub async fn refresh(mut idx: Signal<Index>, cfg: &GithubConfig, files: &[FileMeta]) {
    let stale = idx.peek().stale(files);

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

    // The borrow ends before the awaits below — never hold a signal borrow
    // across one (enforced by `clippy.toml`).
    let changed = idx.with_mut(|i| i.apply(fresh, files));
    if changed.updated.is_empty() && changed.removed.is_empty() {
        return;
    }
    let entries: Vec<(String, String)> = {
        let guard = idx.peek();
        changed
            .updated
            .iter()
            .filter_map(|p| Some((p.clone(), guard.page_json(p)?)))
            .collect()
    };
    crate::js::records_put(entries).await;
    crate::js::records_delete(changed.removed).await;
}

/// Forget one note so the next refresh re-reads it. Call after writing a file:
/// the remote blob SHA has changed, but the listing we hold is the pre-write
/// one, so SHA comparison alone would consider the entry fresh.
pub async fn invalidate(mut idx: Signal<Index>, path: &str) {
    if idx.with_mut(|i| i.invalidate(path)) {
        crate::js::records_delete(vec![path.to_string()]).await;
    }
}

/// Update one note in place from content we already have, without a round trip.
/// Used on save, so the index reflects the edit immediately. The SHA recorded is
/// the note's new blob SHA as reported by the write.
///
/// One record written, nothing read — this is the hot path, and it must not
/// scale with the size of the vault.
pub async fn update_page(mut idx: Signal<Index>, path: &str, sha: &str, content: &str) {
    idx.with_mut(|i| i.upsert(path, sha, content));
    let json = idx.peek().page_json(path);
    if let Some(json) = json {
        crate::js::records_put(vec![(path.to_string(), json)]).await;
    }
}
