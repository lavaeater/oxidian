//! Typed bindings to the browser glue in `assets/oxidian.js`.
//!
//! Every function in that file is bound via `dioxus_use_js::use_js!`, replacing
//! the hand-built `document::eval` strings that used to be scattered across the
//! views. The macro embeds the JS at compile time and checks the call sites, so
//! there is no string formatting or manual escaping to get wrong.
//!
//! Value-returning helpers are `async`. Fire-and-forget helpers are plain `fn`s
//! that `spawn` the call: the generated bindings are ordinary `async fn`s, so —
//! unlike `document::eval`, which runs immediately — the JS only executes once
//! the future is polled.

use dioxus::prelude::*;

mod bindings {
    use dioxus::prelude::*;
    use dioxus_use_js::use_js;
    use_js!("assets/oxidian.js"::{
        ls_get, ls_set, ls_remove,
        blob_get, blob_set, blob_remove, storage_estimate,
        records_all, records_put, records_delete, records_clear,
        today, date_vars,
        confirm_dialog, copy_to_clipboard,
        focus_selector, scroll_active_into_view, start_sidebar_resize,
        download_file,
        get_selection,
        slash_query, apply_slash,
        task_menu_armed, dismiss_task_menu,
        build_signin_link, read_signin_link, request_persistent_storage,
        get_drag_data, set_drag_data, clear_drag_data,
    });
}

// ── localStorage ──────────────────────────────────────────────────────────────

// On native (desktop/mobile) these route to a filesystem-backed store instead
// of the WebView's localStorage, which doesn't reliably survive a cold restart
// on Android (the GitHub token was lost on every launch). Web keeps real
// localStorage. See `crate::native_store`.

/// Reads a `localStorage` key, returning `""` when absent.
#[allow(clippy::unused_async)] // native branch has no await; wasm32 branch does
pub async fn ls_get(key: &str) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        bindings::ls_get(key).await.unwrap_or_default()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::native_store::get(key)
    }
}

pub fn ls_set(key: impl Into<String>, value: impl Into<String>) {
    let (key, value) = (key.into(), value.into());
    #[cfg(target_arch = "wasm32")]
    {
        spawn(async move {
            let _: Result<(), _> = bindings::ls_set(key, value).await;
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::native_store::set(&key, &value);
    }
}

pub fn ls_remove(key: impl Into<String>) {
    let key = key.into();
    #[cfg(target_arch = "wasm32")]
    {
        spawn(async move {
            let _: Result<(), _> = bindings::ls_remove(key).await;
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::native_store::remove(&key);
    }
}

// ── Large blobs ───────────────────────────────────────────────────────────────

// Anything that scales with the size of the vault goes here instead of
// `localStorage`: IndexedDB on web, a file of its own on native. Small,
// fixed-size settings stay in `ls_*`. See `crate::vault_index` and `docs/dataview.md` §6.7.

/// Reads a blob, returning `""` when absent (as `ls_get` does).
#[allow(clippy::unused_async)] // native branch has no await; wasm32 branch does
pub async fn blob_get(key: &str) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        bindings::blob_get(key).await.unwrap_or_default()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::native_store::blob_get(key)
    }
}

/// Writes a blob. Awaited rather than fire-and-forget so a caller that writes
/// and immediately reads back (or navigates away) sees its own write.
#[allow(clippy::unused_async)] // native branch has no await; wasm32 branch does
pub async fn blob_set(key: &str, value: impl Into<String>) {
    let value = value.into();
    #[cfg(target_arch = "wasm32")]
    {
        let _: Result<bool, _> = bindings::blob_set(key, value).await;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::native_store::blob_set(key, &value);
    }
}

#[allow(clippy::unused_async)] // native branch has no await; wasm32 branch does
pub async fn blob_remove(key: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        let _: Result<bool, _> = bindings::blob_remove(key).await;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::native_store::blob_remove(key);
    }
}

// ── Per-note index records ────────────────────────────────────────────────────

// The index is stored one record per note (IndexedDB's `pages` store on web, a
// file each on native) so saving a note costs one small write rather than a
// rewrite of the whole index. Every call is a single batched transaction —
// seeding a vault is thousands of records.

/// Every record, as `(path, json)`.
#[allow(clippy::unused_async)] // native branch has no await; wasm32 branch does
pub async fn records_all() -> Vec<(String, String)> {
    #[cfg(target_arch = "wasm32")]
    {
        bindings::records_all().await.unwrap_or_default()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::native_store::records_all()
    }
}

#[allow(clippy::unused_async)] // native branch has no await; wasm32 branch does
pub async fn records_put(entries: Vec<(String, String)>) {
    if entries.is_empty() {
        return;
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _: Result<bool, _> = bindings::records_put(entries).await;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::native_store::records_put(&entries);
    }
}

#[allow(clippy::unused_async)] // native branch has no await; wasm32 branch does
pub async fn records_delete(keys: Vec<String>) {
    if keys.is_empty() {
        return;
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _: Result<bool, _> = bindings::records_delete(keys).await;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::native_store::records_delete(&keys);
    }
}

#[allow(clippy::unused_async)] // native branch has no await; wasm32 branch does
pub async fn records_clear() {
    #[cfg(target_arch = "wasm32")]
    {
        let _: Result<bool, _> = bindings::records_clear().await;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::native_store::records_clear();
    }
}

/// `(usage, quota, persisted)` in bytes. `-1` for a figure the platform won't
/// report — quota is always `-1` on native, where there isn't one.
#[allow(clippy::unused_async)] // native branch has no await; wasm32 branch does
pub async fn storage_estimate() -> (i64, i64, bool) {
    #[cfg(target_arch = "wasm32")]
    {
        let v: [i64; 3] = bindings::storage_estimate().await.unwrap_or([-1, -1, 0]);
        (v[0], v[1], v[2] == 1)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let (usage, quota) = crate::native_store::usage();
        // A file in the app's private directory is as persistent as it gets.
        (usage, quota, true)
    }
}

// ── Dates ─────────────────────────────────────────────────────────────────────

/// Today's date as `YYYY-MM-DD`.
pub async fn today() -> String {
    bindings::today().await.unwrap_or_default()
}

/// Date variables as a JSON string (parsed by `TemplateVars::from_json`).
pub async fn date_vars() -> String {
    bindings::date_vars().await.unwrap_or_default()
}

// ── Dialogs / clipboard ───────────────────────────────────────────────────────

pub async fn confirm_dialog(message: &str) -> bool {
    bindings::confirm_dialog(message).await.unwrap_or(false)
}

pub fn copy_to_clipboard(text: impl Into<String>) {
    let text = text.into();
    spawn(async move {
        let _: Result<(), _> = bindings::copy_to_clipboard(text).await;
    });
}

// ── Focus / scroll / resize ───────────────────────────────────────────────────

pub fn focus_selector(selector: impl Into<String>) {
    let selector = selector.into();
    spawn(async move {
        let _: Result<(), _> = bindings::focus_selector(selector).await;
    });
}

pub fn scroll_active_into_view() {
    spawn(async move {
        let _: Result<(), _> = bindings::scroll_active_into_view().await;
    });
}

pub fn start_sidebar_resize() {
    spawn(async move {
        let _: Result<(), _> = bindings::start_sidebar_resize().await;
    });
}

// ── HTML export ───────────────────────────────────────────────────────────────

/// Triggers a browser download of `content` as `filename`.
pub fn download_file(filename: impl Into<String>, content: impl Into<String>) {
    let (filename, content) = (filename.into(), content.into());
    spawn(async move {
        let _: Result<(), _> = bindings::download_file(filename, content).await;
    });
}

// ── Editor selection ──────────────────────────────────────────────────────────

/// `(start, end)` selection offsets in the active editor; `(0, 0)` when none.
pub async fn get_selection() -> (usize, usize) {
    let v: [i64; 2] = bindings::get_selection().await.unwrap_or([-1, -1]);
    if v[0] < 0 || v[1] < 0 {
        (0, 0)
    } else {
        (
            usize::try_from(v[0]).unwrap_or(0),
            usize::try_from(v[1]).unwrap_or(0),
        )
    }
}

// ── Slash commands ────────────────────────────────────────────────────────────

/// Sentinel meaning "cursor is not in a `/…` token". Distinct from `""`, which
/// means "cursor is directly after `/` with no query yet".
pub const NO_SLASH: &str = "\x00";

pub async fn slash_query() -> String {
    bindings::slash_query().await.unwrap_or_else(|_| NO_SLASH.to_string())
}

pub fn apply_slash(snippet: impl Into<String>, slash_len: usize) {
    let snippet = snippet.into();
    spawn(async move {
        let _: Result<(), _> = bindings::apply_slash(snippet, slash_len).await;
    });
}

// ── Task metadata menu ────────────────────────────────────────────────────────

/// Whether the caret sits on a blank task line just created by continuing a
/// non-empty task via Enter — the trigger for the task-metadata menu.
pub async fn task_menu_armed() -> bool {
    bindings::task_menu_armed().await.unwrap_or(false)
}

/// Dismisses the task-metadata menu without changing any text.
pub fn dismiss_task_menu() {
    spawn(async move {
        let _: Result<(), _> = bindings::dismiss_task_menu().await;
    });
}

// ── Sign-in link / persistent storage ─────────────────────────────────────────

/// Builds a bookmarkable sign-in link carrying `cfg_json` in the URL fragment.
/// Web only — returns `""` on native (no shareable URL).
#[allow(clippy::unused_async)] // native branch has no await; wasm32 branch does
pub async fn build_signin_link(cfg_json: impl Into<String>) -> String {
    let cfg_json = cfg_json.into();
    #[cfg(target_arch = "wasm32")]
    {
        bindings::build_signin_link(cfg_json).await.unwrap_or_default()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = cfg_json;
        String::new()
    }
}

/// Consumes a `#cfg=…` sign-in link if the current URL carries one: returns the
/// decoded config JSON and strips the fragment. `""` when there is none. Web only.
#[allow(clippy::unused_async)] // native branch has no await; wasm32 branch does
pub async fn read_signin_link() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        bindings::read_signin_link().await.unwrap_or_default()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        String::new()
    }
}

/// Best-effort request to make client storage persistent (resist eviction).
pub fn request_persistent_storage() {
    #[cfg(target_arch = "wasm32")]
    {
        spawn(async move {
            let _: Result<(), _> = bindings::request_persistent_storage().await;
        });
    }
}

// ── Kanban drag data ──────────────────────────────────────────────────────────

pub async fn get_drag_data() -> String {
    bindings::get_drag_data().await.unwrap_or_default()
}

pub fn set_drag_data(data: impl Into<String>) {
    let data = data.into();
    spawn(async move {
        let _: Result<(), _> = bindings::set_drag_data(data).await;
    });
}

pub fn clear_drag_data() {
    spawn(async move {
        let _: Result<(), _> = bindings::clear_drag_data().await;
    });
}
