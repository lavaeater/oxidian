//! The Bullet Journal plugin.
//!
//! Bullet Journalling is compiled-in Rust (see `docs/bujo-roadmap.md`), but it
//! is described here as a [`PluginDef`] so it is listed, toggled, and configured
//! through exactly the same path an external plugin will be. Everything it owns
//! — its settings and its three log templates — lives under
//! `.oxidian/plugins/bujo/`.

use serde_json::Value;
use vault::GithubConfig;

use super::{Asset, Field, PathKind, PluginDef, PluginStore};
use crate::dates::Period;

pub const ID: &str = "bujo";

pub const DEFAULT_DAILY_TEMPLATE: &str = ".oxidian/plugins/bujo/templates/daily-log.md";
pub const DEFAULT_WEEKLY_TEMPLATE: &str = ".oxidian/plugins/bujo/templates/weekly-log.md";
pub const DEFAULT_MONTHLY_TEMPLATE: &str = ".oxidian/plugins/bujo/templates/monthly-log.md";

/// The daily-note path a vault gets when nothing has been configured. Used to
/// tell "the user chose this" from "nobody ever touched it" when seeding.
const CONFIG_DAILY_DEFAULT: &str = ".oxidian/templates/daily-note.md";

pub static DEF: PluginDef = PluginDef {
    id: ID,
    name: "Bullet Journal",
    description: "Daily, weekly, and monthly logs; rapid-logging signifiers; \
                  and the migration ritual that closes one period into the next.",
    settings: &[
        Field::Path {
            key: "daily_template",
            label: "Daily log template",
            help: "The template's filepath: decides where each day's log lands.",
            kind: PathKind::File,
            default: DEFAULT_DAILY_TEMPLATE,
        },
        Field::Path {
            key: "weekly_template",
            label: "Weekly log template",
            help: "Use ${OXID_DATE_WEEK_YEAR}-W${OXID_DATE_WEEK} in its filepath — \
                   the ISO week-year differs from the calendar year at New Year.",
            kind: PathKind::File,
            default: DEFAULT_WEEKLY_TEMPLATE,
        },
        Field::Path {
            key: "monthly_template",
            label: "Monthly log template",
            help: "Leave empty to turn monthly logs off.",
            kind: PathKind::File,
            default: DEFAULT_MONTHLY_TEMPLATE,
        },
    ],
    assets: &[
        Asset { path: "templates/daily-log.md", contents: DAILY_LOG },
        Asset { path: "templates/weekly-log.md", contents: WEEKLY_LOG },
        Asset { path: "templates/monthly-log.md", contents: MONTHLY_LOG },
    ],
};

/// Is Bullet Journalling live for this vault?
///
/// Once the plugin is named in `plugins.json` the manifest is the only
/// authority — toggling it off hides the period switcher and the review. Before
/// that it falls back to the pre-plugin config, so a vault that already had
/// weekly or monthly templates keeps working without anyone enabling anything.
#[must_use]
pub fn active(store: &PluginStore, cfg: &GithubConfig) -> bool {
    if store.installed(ID) {
        store.is_enabled(ID)
    } else {
        !cfg.weekly_note_template.is_empty() || !cfg.monthly_note_template.is_empty()
    }
}

/// The template path for a period, from the plugin when it owns the setting and
/// from the legacy config otherwise. Empty means "this log is turned off".
#[must_use]
pub fn template_for(store: &PluginStore, cfg: &GithubConfig, period: Period) -> String {
    if store.is_enabled(ID) {
        let key = match period {
            Period::Day => "daily_template",
            Period::Week => "weekly_template",
            Period::Month => "monthly_template",
        };
        return store.string(&DEF, key);
    }
    match period {
        Period::Day => cfg.daily_note_template.clone(),
        Period::Week => cfg.weekly_note_template.clone(),
        Period::Month => cfg.monthly_note_template.clone(),
    }
}

/// Settings to pre-fill on first enable, taken from wherever they lived before
/// the plugin existed.
///
/// Only values the user actually chose carry over: a daily-note path still at
/// its factory default says nothing, and letting it win would override the
/// template this plugin is about to install.
#[must_use]
pub fn seed_from_config(cfg: &GithubConfig) -> super::store::Values {
    let mut seed = super::store::Values::new();
    let mut put = |key: &str, val: &str| {
        if !val.is_empty() {
            seed.insert(key.to_string(), Value::String(val.to_string()));
        }
    };
    if cfg.daily_note_template != CONFIG_DAILY_DEFAULT {
        put("daily_template", &cfg.daily_note_template);
    }
    put("weekly_template", &cfg.weekly_note_template);
    put("monthly_template", &cfg.monthly_note_template);
    seed
}

// ── Default templates ─────────────────────────────────────────────────────────
//
// Written into the plugin's folder on first enable and never overwritten after
// that. They are ordinary Oxidian templates: the `filepath:` decides where the
// note lands, and `${OXID_DATE_*}` is substituted at creation time.

const DAILY_LOG: &str = r#"---
oxid_template:
  filepath: "journal/${OXID_DATE_YEAR}/${OXID_DATE_MONTH}/${OXID_DATE_YEAR}-${OXID_DATE_MONTH}-${OXID_DATE_DATE}.md"
  description: "Daily log"
---
# ${OXID_DATE_DAY_NAME} ${OXID_DATE_YEAR}-${OXID_DATE_MONTH}-${OXID_DATE_DATE}

## Log

- [ ] 
"#;

const WEEKLY_LOG: &str = r#"---
oxid_template:
  filepath: "journal/${OXID_DATE_WEEK_YEAR}/W${OXID_DATE_WEEK}.md"
  description: "Weekly log"
---
# Week ${OXID_DATE_WEEK}, ${OXID_DATE_WEEK_YEAR}

## Log

- [ ] 

## Notes
"#;

const MONTHLY_LOG: &str = r#"---
oxid_template:
  filepath: "journal/${OXID_DATE_YEAR}/${OXID_DATE_MONTH}-${OXID_DATE_MONTH_NAME}.md"
  description: "Monthly log"
---
# ${OXID_DATE_MONTH_NAME} ${OXID_DATE_YEAR}

## Log

- [ ] 

## Notes
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> GithubConfig {
        GithubConfig {
            token: String::new(),
            owner: String::new(),
            repo: String::new(),
            branch: "main".into(),
            provider: vault::Provider::GitHub,
            templates_dir: ".oxidian/templates".into(),
            daily_note_template: CONFIG_DAILY_DEFAULT.into(),
            weekly_note_template: String::new(),
            monthly_note_template: String::new(),
        }
    }

    #[test]
    fn an_untouched_vault_has_no_bullet_journal() {
        assert!(!active(&PluginStore::default(), &cfg()));
    }

    #[test]
    fn a_pre_plugin_vault_with_a_weekly_template_stays_active() {
        let mut c = cfg();
        c.weekly_note_template = ".oxidian/templates/weekly.md".into();
        assert!(active(&PluginStore::default(), &c));
        // ...and reads its templates from the config it already had.
        assert_eq!(
            template_for(&PluginStore::default(), &c, Period::Week),
            ".oxidian/templates/weekly.md"
        );
    }

    #[test]
    fn every_default_template_declares_a_filepath() {
        for Asset { path, contents } in DEF.assets {
            let meta = crate::template::parse_template(path, contents);
            assert!(meta.filepath.is_some(), "{path} has no filepath:");
        }
    }

    #[test]
    fn the_weekly_template_uses_the_iso_week_year() {
        // Using OXID_DATE_YEAR here silently misfiles the last days of December.
        let meta = crate::template::parse_template("w.md", WEEKLY_LOG);
        let fp = meta.filepath.unwrap();
        assert!(fp.contains("OXID_DATE_WEEK_YEAR"), "{fp}");
    }

    #[test]
    fn the_default_settings_point_at_the_templates_the_plugin_installs() {
        for (key, asset) in [
            ("daily_template", "templates/daily-log.md"),
            ("weekly_template", "templates/weekly-log.md"),
            ("monthly_template", "templates/monthly-log.md"),
        ] {
            let want = format!("{}/{asset}", super::super::plugin_dir(ID));
            assert_eq!(PluginStore::default().string(&DEF, key), want);
        }
    }

    #[test]
    fn seeding_carries_chosen_paths_but_not_the_factory_daily_default() {
        let mut c = cfg();
        c.weekly_note_template = "w.md".into();
        let seed = seed_from_config(&c);
        assert_eq!(seed.get("weekly_template").and_then(Value::as_str), Some("w.md"));
        // Untouched daily path and an unset monthly one say nothing, so the
        // plugin's own defaults must survive the seed.
        assert!(!seed.contains_key("daily_template"));
        assert!(!seed.contains_key("monthly_template"));

        c.daily_note_template = "journal/today.md".into();
        assert_eq!(
            seed_from_config(&c).get("daily_template").and_then(Value::as_str),
            Some("journal/today.md")
        );
    }
}
