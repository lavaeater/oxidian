use vault::GithubConfig;

use crate::js;

const STORAGE_KEY: &str = "oxidian_cfg";

/// Load config. A `#cfg=…` sign-in link in the URL takes precedence: it is
/// imported, persisted locally, and stripped from the URL — this restores a
/// vault in one click on browsers that wipe localStorage between sessions (e.g.
/// a managed work profile that clears site data on exit). Otherwise falls back
/// to the config stored in localStorage. Returns None if neither is present.
pub async fn load_config() -> Option<GithubConfig> {
    // Best-effort: ask the browser not to evict our storage under pressure.
    js::request_persistent_storage();

    // A sign-in link overrides any stored config.
    let imported = js::read_signin_link().await;
    if !imported.is_empty() {
        match serde_json::from_str::<GithubConfig>(&imported) {
            Ok(cfg) => {
                save_config(&cfg);
                return Some(cfg);
            }
            Err(e) => crate::console_log(&format!("[oxidian] sign-in link: parse error: {e}")),
        }
    }

    let json = js::ls_get(STORAGE_KEY).await;
    if json.is_empty() {
        None
    } else {
        match serde_json::from_str(&json) {
            Ok(cfg) => Some(cfg),
            Err(e) => {
                crate::console_log(&format!("[oxidian] load_config: parse error: {e}"));
                None
            }
        }
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

/// Build a bookmarkable sign-in link that carries `cfg` in the URL fragment, so
/// it can be stored in a password manager and used to restore the vault in one
/// click (see [`load_config`]). Empty string on native or on serialization error.
pub async fn signin_link(cfg: &GithubConfig) -> String {
    match serde_json::to_string(cfg) {
        Ok(json) => js::build_signin_link(json).await,
        Err(_) => String::new(),
    }
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
