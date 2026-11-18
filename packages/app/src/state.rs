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

// ── UI layout preferences ─────────────────────────────────────────────────────
//
// Which editor bars show and where (ui-polish.md: "Conserve vertical space").
// One global setting, not per note or per vault. Missing fields fall back to
// their defaults, so older or partial JSON keeps loading as fields are added.

const UI_PREFS_KEY: &str = "oxidian_ui";

/// Where an editor bar sits relative to the document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BarPos {
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BarPref {
    pub visible: bool,
    pub pos: BarPos,
}

/// The bars under the tab strip that can be shown, hidden and moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bar {
    /// File path, palette, bookmark, word count, save state, move, export.
    Status,
    /// The frontmatter properties editor.
    Properties,
    /// Heading / bold / list buttons.
    Formatting,
}

impl Bar {
    pub const ALL: [Bar; 3] = [Bar::Status, Bar::Properties, Bar::Formatting];

    pub fn label(self) -> &'static str {
        match self {
            Bar::Status => "Status bar",
            Bar::Properties => "Properties",
            Bar::Formatting => "Formatting toolbar",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct UiPrefs {
    pub status: BarPref,
    pub properties: BarPref,
    pub formatting: BarPref,
    /// Hide every bar except the tab strip (which holds the layout button).
    pub super_collapse: bool,
    /// Whether the properties panel's body is folded.
    pub props_collapsed: bool,
    /// Whether the sidebar outline is folded to its title.
    pub outline_collapsed: bool,
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            // Status reads best out of the way at the bottom, like an IDE's.
            status: BarPref { visible: true, pos: BarPos::Bottom },
            properties: BarPref { visible: true, pos: BarPos::Top },
            formatting: BarPref { visible: true, pos: BarPos::Top },
            super_collapse: false,
            // Folded by default: a note's properties are reference, not the
            // first thing you want between the tabs and the text.
            props_collapsed: true,
            outline_collapsed: false,
        }
    }
}

impl UiPrefs {
    pub fn bar(&self, bar: Bar) -> BarPref {
        match bar {
            Bar::Status => self.status,
            Bar::Properties => self.properties,
            Bar::Formatting => self.formatting,
        }
    }

    pub fn bar_mut(&mut self, bar: Bar) -> &mut BarPref {
        match bar {
            Bar::Status => &mut self.status,
            Bar::Properties => &mut self.properties,
            Bar::Formatting => &mut self.formatting,
        }
    }

    /// The bars to draw at `pos`, top to bottom. At the bottom the order is
    /// mirrored, so the status bar keeps the outer edge in both positions.
    pub fn bars_at(&self, pos: BarPos) -> Vec<Bar> {
        if self.super_collapse {
            return Vec::new();
        }
        let mut v: Vec<Bar> = Bar::ALL
            .into_iter()
            .filter(|&b| self.bar(b).visible && self.bar(b).pos == pos)
            .collect();
        if pos == BarPos::Bottom {
            v.reverse();
        }
        v
    }

    pub fn from_json(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }
}

pub async fn load_ui_prefs() -> UiPrefs {
    UiPrefs::from_json(&js::ls_get(UI_PREFS_KEY).await)
}

pub fn save_ui_prefs(prefs: &UiPrefs) {
    if let Ok(json) = serde_json::to_string(prefs) {
        js::ls_set(UI_PREFS_KEY, json);
    }
}

#[cfg(test)]
mod ui_prefs_tests {
    use super::*;

    #[test]
    fn defaults_put_status_at_the_bottom_and_fold_properties() {
        let p = UiPrefs::default();
        assert_eq!(p.bars_at(BarPos::Top), vec![Bar::Properties, Bar::Formatting]);
        assert_eq!(p.bars_at(BarPos::Bottom), vec![Bar::Status]);
        assert!(p.props_collapsed);
    }

    #[test]
    fn bottom_order_is_mirrored_so_status_stays_on_the_edge() {
        let mut p = UiPrefs::default();
        p.formatting.pos = BarPos::Bottom;
        assert_eq!(p.bars_at(BarPos::Bottom), vec![Bar::Formatting, Bar::Status]);
        p.status.pos = BarPos::Top;
        assert_eq!(p.bars_at(BarPos::Top), vec![Bar::Status, Bar::Properties]);
    }

    #[test]
    fn hidden_bars_and_super_collapse_draw_nothing() {
        let mut p = UiPrefs::default();
        p.properties.visible = false;
        assert_eq!(p.bars_at(BarPos::Top), vec![Bar::Formatting]);
        p.super_collapse = true;
        assert!(p.bars_at(BarPos::Top).is_empty());
        assert!(p.bars_at(BarPos::Bottom).is_empty());
    }

    #[test]
    fn partial_or_garbage_json_falls_back_to_defaults() {
        let p = UiPrefs::from_json(r#"{"super_collapse":true}"#);
        assert!(p.super_collapse);
        assert_eq!(p.status, UiPrefs::default().status);
        assert_eq!(UiPrefs::from_json(""), UiPrefs::default());
        assert_eq!(UiPrefs::from_json("not json"), UiPrefs::default());
    }

    #[test]
    fn round_trips_through_json() {
        let mut p = UiPrefs::default();
        p.formatting = BarPref { visible: false, pos: BarPos::Bottom };
        p.outline_collapsed = true;
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(UiPrefs::from_json(&json), p);
    }
}
