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
        today, date_vars,
        confirm_dialog, copy_to_clipboard,
        focus_selector, scroll_active_into_view, start_sidebar_resize,
        download_file,
        get_selection,
        slash_query, apply_slash,
        get_drag_data, set_drag_data, clear_drag_data,
    });
}

// ── localStorage ──────────────────────────────────────────────────────────────

// On native (desktop/mobile) these route to a filesystem-backed store instead
// of the WebView's localStorage, which doesn't reliably survive a cold restart
// on Android (the GitHub token was lost on every launch). Web keeps real
// localStorage. See `crate::native_store`.

/// Reads a `localStorage` key, returning `""` when absent.
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
    if v[0] < 0 { (0, 0) } else { (v[0] as usize, v[1] as usize) }
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
