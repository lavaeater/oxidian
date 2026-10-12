//! Reading and writing plugin state — `.oxidian/plugins.json` plus one
//! `settings.json` per plugin.
//!
//! State lives in the vault rather than `localStorage` on purpose: plugin
//! configuration is part of *the vault*, not part of *this browser*, so it
//! travels to the phone through git exactly like the notes it configures.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use vault::{GithubConfig, VaultError};

use super::{Asset, PluginDef, MANIFEST_PATH};

/// One plugin's settings: a flat map, so the JSON on disk stays readable and
/// hand-editable and so an unknown key round-trips instead of being dropped.
pub type Values = BTreeMap<String, Value>;

#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Eq)]
struct Entry {
    #[serde(default)]
    enabled: bool,
}

#[derive(Default, Clone, PartialEq)]
pub struct PluginStore {
    /// Plugins named in `plugins.json`, and whether each is on. A plugin absent
    /// from this map has never been installed, which is distinct from "off".
    entries: BTreeMap<String, Entry>,
    settings: BTreeMap<String, Values>,
    /// Blob SHAs of the files we have read, so a save can write them back
    /// without a extra round trip. Missing means "create, don't update".
    shas: BTreeMap<String, String>,
    /// True once [`load`](Self::load) has finished, so callers can tell "no
    /// plugins installed" from "not read yet" — the same trap the template
    /// scan hit (see `views::vault::templates_when_ready`).
    pub loaded: bool,
}

impl PluginStore {
    /// Read the manifest and every installed plugin's settings.
    ///
    /// A missing file is not an error: a vault that has never used a plugin
    /// simply has none of these, which is the common case.
    pub async fn load(cfg: &GithubConfig) -> Self {
        let mut store = PluginStore { loaded: true, ..PluginStore::default() };

        if let Some(text) = store.read(cfg, MANIFEST_PATH).await {
            match serde_json::from_str::<BTreeMap<String, Entry>>(&text) {
                Ok(entries) => store.entries = entries,
                Err(e) => crate::console_log(&format!("[oxidian] plugins.json: {e}")),
            }
        }

        for id in store.entries.keys().cloned().collect::<Vec<_>>() {
            let path = super::settings_path(&id);
            if let Some(text) = store.read(cfg, &path).await {
                match serde_json::from_str::<Values>(&text) {
                    Ok(v) => {
                        store.settings.insert(id, v);
                    }
                    Err(e) => crate::console_log(&format!("[oxidian] {path}: {e}")),
                }
            }
        }
        store
    }

    /// Has this plugin ever been installed? Once it has, the manifest is the
    /// authority on whether its features are live; before that, callers fall
    /// back to whatever configured them previously.
    #[must_use]
    pub fn installed(&self, id: &str) -> bool {
        self.entries.contains_key(id)
    }

    #[must_use]
    pub fn is_enabled(&self, id: &str) -> bool {
        self.entries.get(id).is_some_and(|e| e.enabled)
    }

    /// The current value of a setting, falling back to the field's default.
    #[must_use]
    pub fn value(&self, def: &PluginDef, key: &str) -> Value {
        self.settings
            .get(def.id)
            .and_then(|v| v.get(key))
            .cloned()
            .or_else(|| def.field(key).map(super::Field::default_value))
            .unwrap_or(Value::Null)
    }

    /// A string setting, or `""` when it is unset or not a string.
    #[must_use]
    pub fn string(&self, def: &PluginDef, key: &str) -> String {
        match self.value(def, key) {
            Value::String(s) => s,
            _ => String::new(),
        }
    }

    #[must_use]
    pub fn bool(&self, def: &PluginDef, key: &str) -> bool {
        matches!(self.value(def, key), Value::Bool(true))
    }

    /// Every setting for a plugin, defaults filled in — what the form edits.
    #[must_use]
    pub fn values(&self, def: &PluginDef) -> Values {
        let mut out = def.defaults();
        if let Some(stored) = self.settings.get(def.id) {
            for (k, v) in stored {
                out.insert(k.clone(), v.clone());
            }
        }
        out
    }

    /// Turn a plugin on, scaffolding its folder the first time.
    ///
    /// `seed` pre-fills settings from wherever they lived before this plugin
    /// existed; it only applies on the first enable, and only for keys the
    /// plugin actually declares.
    pub async fn enable(
        &mut self,
        cfg: &GithubConfig,
        def: &PluginDef,
        seed: Values,
    ) -> Result<(), String> {
        if !self.settings.contains_key(def.id) {
            let mut values = def.defaults();
            for (k, v) in seed {
                if def.field(&k).is_some() {
                    values.insert(k, v);
                }
            }
            self.write_settings(cfg, def.id, &values).await?;
        }
        self.scaffold(cfg, def).await;
        self.entries.insert(def.id.to_string(), Entry { enabled: true });
        self.write_manifest(cfg).await
    }

    /// Turn a plugin off. Deliberately destructive of nothing: the settings and
    /// templates stay, because they are the user's files and re-enabling should
    /// pick up exactly where they left off.
    pub async fn disable(&mut self, cfg: &GithubConfig, id: &str) -> Result<(), String> {
        self.entries.insert(id.to_string(), Entry { enabled: false });
        self.write_manifest(cfg).await
    }

    /// Write a plugin's default files, skipping every path that already exists.
    ///
    /// `create_file` fails when the destination is present, which is exactly the
    /// "never clobber a template you have edited" rule — so a failure here is
    /// the expected outcome on re-enable, not something to report.
    async fn scaffold(&mut self, cfg: &GithubConfig, def: &PluginDef) {
        for Asset { path, contents } in def.assets {
            let full = format!("{}/{path}", super::plugin_dir(def.id));
            let msg = format!("Add {} default {path}", def.name);
            let _ = vault::dispatch::create_file(cfg, &full, contents, &msg).await;
        }
    }

    pub async fn save_settings(
        &mut self,
        cfg: &GithubConfig,
        id: &str,
        values: &Values,
    ) -> Result<(), String> {
        self.write_settings(cfg, id, values).await
    }

    async fn write_settings(
        &mut self,
        cfg: &GithubConfig,
        id: &str,
        values: &Values,
    ) -> Result<(), String> {
        let json = serde_json::to_string_pretty(values).map_err(|e| e.to_string())?;
        self.write(cfg, &super::settings_path(id), &format!("{json}\n")).await?;
        self.settings.insert(id.to_string(), values.clone());
        Ok(())
    }

    async fn write_manifest(&mut self, cfg: &GithubConfig) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&self.entries).map_err(|e| e.to_string())?;
        self.write(cfg, MANIFEST_PATH, &format!("{json}\n")).await
    }

    /// Read a vault file, returning `None` when it isn't there. Records the
    /// blob SHA so a later write can update rather than collide.
    async fn read(&mut self, cfg: &GithubConfig, path: &str) -> Option<String> {
        match vault::dispatch::read_file(cfg, path).await {
            Ok(fc) => {
                self.shas.insert(path.to_string(), fc.sha);
                Some(fc.content)
            }
            Err(VaultError::NotFound(_)) => None,
            Err(e) => {
                crate::console_log(&format!("[oxidian] read {path}: {e}"));
                None
            }
        }
    }

    /// Create or update a vault file, keeping the resulting SHA for next time.
    async fn write(&mut self, cfg: &GithubConfig, path: &str, content: &str) -> Result<(), String> {
        let msg = format!("Update {path}");
        let sha = self.shas.get(path).cloned();
        let result = match sha {
            Some(sha) => vault::dispatch::write_file(cfg, path, content, &sha, &msg).await,
            None => vault::dispatch::create_file(cfg, path, content, &msg).await,
        };
        match result {
            Ok(new_sha) => {
                self.shas.insert(path.to_string(), new_sha);
                Ok(())
            }
            Err(e) => Err(format!("Could not save {path}: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::bujo;

    #[test]
    fn absent_plugin_is_neither_installed_nor_enabled() {
        let store = PluginStore::default();
        assert!(!store.installed("bujo"));
        assert!(!store.is_enabled("bujo"));
    }

    #[test]
    fn an_installed_but_disabled_plugin_is_distinguishable_from_an_absent_one() {
        let mut store = PluginStore::default();
        store.entries.insert("bujo".into(), Entry { enabled: false });
        assert!(store.installed("bujo"));
        assert!(!store.is_enabled("bujo"));
    }

    #[test]
    fn values_fall_back_to_field_defaults() {
        let store = PluginStore::default();
        assert_eq!(
            store.string(&bujo::DEF, "weekly_template"),
            bujo::DEFAULT_WEEKLY_TEMPLATE
        );
    }

    #[test]
    fn stored_values_win_over_defaults_and_unknown_keys_survive() {
        let mut store = PluginStore::default();
        let mut v = Values::new();
        v.insert("weekly_template".into(), Value::String("w.md".into()));
        v.insert("from_the_future".into(), Value::String("keep me".into()));
        store.settings.insert("bujo".into(), v);

        assert_eq!(store.string(&bujo::DEF, "weekly_template"), "w.md");
        // A key the plugin does not declare is not in the form's view...
        let merged = store.values(&bujo::DEF);
        assert_eq!(merged.get("from_the_future").and_then(Value::as_str), Some("keep me"));
        // ...but the untouched defaults still appear.
        assert_eq!(
            merged.get("monthly_template").and_then(Value::as_str),
            Some(bujo::DEFAULT_MONTHLY_TEMPLATE)
        );
    }

    #[test]
    fn a_non_string_setting_reads_as_empty_rather_than_panicking() {
        let mut store = PluginStore::default();
        let mut v = Values::new();
        v.insert("weekly_template".into(), Value::Bool(true));
        store.settings.insert("bujo".into(), v);
        assert_eq!(store.string(&bujo::DEF, "weekly_template"), "");
    }
}
