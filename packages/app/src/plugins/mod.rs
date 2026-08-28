//! The plugin registry.
//!
//! A "plugin" here is a *registry entry*, not necessarily a JavaScript file.
//! Oxidian's own features (Bullet Journal, so far) are compiled-in Rust, and
//! `docs/plugin-architecture.md` describes a future external JS surface — but
//! both need the same three things from the app: a place in a list, an on/off
//! switch, and a settings form. So the description of a plugin is *data*
//! ([`PluginDef`]), and one generic form renders it. Getting the built-ins
//! through that pipe first is what proves it can carry external plugins later.
//!
//! Everything a plugin owns lives in the vault under `.oxidian/plugins/<id>/`,
//! so it is version-controlled next to the notes and syncs to every device the
//! same way they do. Enablement lives in one small `.oxidian/plugins.json`
//! rather than being implied by the folder's existence, because "installed but
//! switched off" has to be expressible without deleting the user's templates.

pub mod bujo;
pub mod store;

pub use store::PluginStore;

/// Where the enabled/disabled flags live.
pub const MANIFEST_PATH: &str = ".oxidian/plugins.json";

/// The folder a plugin owns, relative to the vault root.
#[must_use]
pub fn plugin_dir(id: &str) -> String {
    format!(".oxidian/plugins/{id}")
}

/// Where a plugin's settings live.
#[must_use]
pub fn settings_path(id: &str) -> String {
    format!("{}/settings.json", plugin_dir(id))
}

/// Does this vault path belong to some plugin's template folder?
///
/// Plugins ship their templates inside their own folder rather than the user's
/// `templates_dir`, so the template scan has to look there too — otherwise a
/// plugin's default template would be written to the vault and then never
/// found by the resolver that needs it.
#[must_use]
pub fn is_plugin_template(path: &str) -> bool {
    path.starts_with(".oxidian/plugins/")
        && path.contains("/templates/")
        && std::path::Path::new(path)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
}

/// A file a plugin seeds into its folder the first time it is enabled.
pub struct Asset {
    /// Path relative to the plugin's folder, e.g. `templates/weekly-log.md`.
    pub path: &'static str,
    pub contents: &'static str,
}

/// What kind of vault path a [`Field::Path`] points at — decides only the
/// placeholder and help text today, but is the hook for a future path picker.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    File,
    Dir,
}

/// One configurable setting, rendered generically by `views::plugins`.
///
/// The set is deliberately small. A plugin whose configuration doesn't fit
/// these shapes ships its own panel instead; this is the "trivial to configure"
/// tier, and keeping it narrow is what lets one form serve every plugin.
pub enum Field {
    Text {
        key: &'static str,
        label: &'static str,
        help: &'static str,
        default: &'static str,
    },
    Path {
        key: &'static str,
        label: &'static str,
        help: &'static str,
        kind: PathKind,
        default: &'static str,
    },
    Bool {
        key: &'static str,
        label: &'static str,
        help: &'static str,
        default: bool,
    },
    Select {
        key: &'static str,
        label: &'static str,
        help: &'static str,
        options: &'static [(&'static str, &'static str)],
        default: &'static str,
    },
}

impl Field {
    #[must_use]
    pub fn key(&self) -> &'static str {
        match self {
            Field::Text { key, .. }
            | Field::Path { key, .. }
            | Field::Bool { key, .. }
            | Field::Select { key, .. } => key,
        }
    }

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Field::Text { label, .. }
            | Field::Path { label, .. }
            | Field::Bool { label, .. }
            | Field::Select { label, .. } => label,
        }
    }

    #[must_use]
    pub fn help(&self) -> &'static str {
        match self {
            Field::Text { help, .. }
            | Field::Path { help, .. }
            | Field::Bool { help, .. }
            | Field::Select { help, .. } => help,
        }
    }

    #[must_use]
    pub fn default_value(&self) -> serde_json::Value {
        match self {
            Field::Text { default, .. }
            | Field::Path { default, .. }
            | Field::Select { default, .. } => serde_json::Value::String((*default).to_string()),
            Field::Bool { default, .. } => serde_json::Value::Bool(*default),
        }
    }
}

/// Everything the app needs to list, toggle, scaffold, and configure a plugin.
pub struct PluginDef {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub settings: &'static [Field],
    pub assets: &'static [Asset],
}

impl PluginDef {
    #[must_use]
    pub fn field(&self, key: &str) -> Option<&Field> {
        self.settings.iter().find(|f| f.key() == key)
    }

    /// The settings map a freshly enabled plugin starts from.
    #[must_use]
    pub fn defaults(&self) -> store::Values {
        self.settings
            .iter()
            .map(|f| (f.key().to_string(), f.default_value()))
            .collect()
    }
}

/// Every plugin compiled into the app. External JS plugins, when they land,
/// will be appended to this list at runtime rather than replacing it.
static BUILTINS: &[&PluginDef] = &[&bujo::DEF];

#[must_use]
pub fn builtins() -> &'static [&'static PluginDef] {
    BUILTINS
}

#[must_use]
pub fn find(id: &str) -> Option<&'static PluginDef> {
    builtins().iter().copied().find(|d| d.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_scoped_to_the_plugin_folder() {
        assert_eq!(plugin_dir("bujo"), ".oxidian/plugins/bujo");
        assert_eq!(settings_path("bujo"), ".oxidian/plugins/bujo/settings.json");
    }

    #[test]
    fn plugin_templates_are_recognised_but_other_files_are_not() {
        assert!(is_plugin_template(".oxidian/plugins/bujo/templates/weekly-log.md"));
        // Settings and notes must not be mistaken for templates.
        assert!(!is_plugin_template(".oxidian/plugins/bujo/settings.json"));
        assert!(!is_plugin_template(".oxidian/templates/daily-note.md"));
        assert!(!is_plugin_template("journal/2026-01-01.md"));
    }

    #[test]
    fn defaults_cover_every_declared_field() {
        for def in builtins() {
            let d = def.defaults();
            assert_eq!(d.len(), def.settings.len(), "{}", def.id);
            for f in def.settings {
                assert!(d.contains_key(f.key()), "{}/{}", def.id, f.key());
            }
        }
    }

    #[test]
    fn builtin_ids_are_unique_and_findable() {
        let mut ids: Vec<&str> = builtins().iter().map(|d| d.id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n);
        assert!(find("bujo").is_some());
        assert!(find("nope").is_none());
    }
}
