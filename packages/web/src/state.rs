use dioxus::prelude::*;
use vault::GithubConfig;

const STORAGE_KEY: &str = "oxidian_cfg";

/// Load config from localStorage via JS eval. Returns None if nothing is stored.
pub async fn load_config() -> Option<GithubConfig> {
    let json = document::eval(&format!(
        "dioxus.send(localStorage.getItem({:?}) || '')",
        STORAGE_KEY
    ))
    .join::<String>()
    .await
    .unwrap_or_default();

    if json.is_empty() {
        None
    } else {
        serde_json::from_str(&json).ok()
    }
}

/// Persist config to localStorage.
pub fn save_config(cfg: &GithubConfig) {
    let json = serde_json::to_string(cfg).unwrap_or_default();
    // Escape for JS string literal — use JSON.parse so no escaping needed.
    document::eval(&format!(
        "localStorage.setItem({:?}, {})",
        STORAGE_KEY,
        serde_json::to_string(&json).unwrap_or_default()
    ));
}

/// Remove config from localStorage (logout).
pub fn clear_config() {
    document::eval(&format!("localStorage.removeItem({:?})", STORAGE_KEY));
}

const BOOKMARKS_KEY: &str = "oxidian_bookmarks";

pub async fn load_bookmarks() -> Vec<String> {
    let json = document::eval(&format!(
        "dioxus.send(localStorage.getItem({:?}) || '[]')",
        BOOKMARKS_KEY
    ))
    .join::<String>()
    .await
    .unwrap_or_default();
    serde_json::from_str(&json).unwrap_or_default()
}

pub fn save_bookmarks(bookmarks: &[String]) {
    let json = serde_json::to_string(bookmarks).unwrap_or_default();
    document::eval(&format!(
        "localStorage.setItem({:?}, {})",
        BOOKMARKS_KEY,
        serde_json::to_string(&json).unwrap_or_default()
    ));
}
