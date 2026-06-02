use vault::GithubConfig;

use crate::js;

const STORAGE_KEY: &str = "oxidian_cfg";

/// Load config from localStorage. Returns None if nothing is stored.
pub async fn load_config() -> Option<GithubConfig> {
    let json = js::ls_get(STORAGE_KEY).await;
    if json.is_empty() {
        None
    } else {
        serde_json::from_str(&json).ok()
    }
}

/// Persist config to localStorage.
pub fn save_config(cfg: &GithubConfig) {
    let json = serde_json::to_string(cfg).unwrap_or_default();
    js::ls_set(STORAGE_KEY, json);
}

/// Remove config from localStorage (logout).
pub fn clear_config() {
    js::ls_remove(STORAGE_KEY);
}

const BOOKMARKS_KEY: &str = "oxidian_bookmarks";

pub async fn load_bookmarks() -> Vec<String> {
    let json = js::ls_get(BOOKMARKS_KEY).await;
    if json.is_empty() {
        return Vec::new();
    }
    serde_json::from_str(&json).unwrap_or_default()
}

pub fn save_bookmarks(bookmarks: &[String]) {
    let json = serde_json::to_string(bookmarks).unwrap_or_default();
    js::ls_set(BOOKMARKS_KEY, json);
}
