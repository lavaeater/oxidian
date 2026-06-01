use dioxus::prelude::*;
use ui::{MarkdownArea, MarkdownAreaVariant};
use vault::{FileMeta, GithubConfig, SearchResult};

use crate::console_log;
use crate::export;
use crate::state;
use crate::template::{self, TemplateMeta, JS_DATE_VARS};
use crate::wikilink_index::WikiLinkIndex;
use super::graph::GraphView;
use super::kanban::KanbanBoard;
use super::properties::PropertiesPanel;
use super::slash::{SlashMenu, JS_NO_SLASH, JS_SLASH_QUERY, js_apply_slash};
use super::toolbar::FormattingToolbar;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Applies a template: creates the target file (or navigates to it if it already
/// exists for filepath templates) and returns the path that was opened.
async fn apply_template(
    meta: &TemplateMeta,
    cfg: &GithubConfig,
    mut files: Signal<Vec<FileMeta>>,
    mut active_path: Signal<Option<String>>,
    mut load_error: Signal<Option<String>>,
    current_dir: &str,
) {
    let mut eval = document::eval(JS_DATE_VARS);
    let date_json = eval.recv::<String>().await.unwrap_or_default();
    let vars = template::TemplateVars::from_json(&date_json, "", current_dir);

    if vars.year.is_empty() || vars.month.is_empty() || vars.date.is_empty() {
        load_error.set(Some("Could not read current date — please try again.".to_string()));
        return;
    }

    if let Some(ref fp_tmpl) = meta.filepath {
        let path = template::substitute_vars(fp_tmpl, &vars)
            .trim_start_matches('/').to_string();
        if files.read().iter().any(|f| f.path == path) {
            active_path.set(Some(path));
        } else {
            let body = template::strip_tabstops(&template::substitute_vars(&meta.body, &vars));
            match vault::dispatch::create_file(cfg, &path, &body, &format!("Create {path}")).await {
                Ok(_) => {
                    if let Ok(mut list) = vault::dispatch::list_files(cfg).await {
                        list.sort_by(|a, b| a.path.cmp(&b.path));
                        files.set(list);
                    }
                    active_path.set(Some(path));
                }
                Err(ref e) if e.to_string().contains("File already exists") => {
                    // File exists on remote but wasn't in the local list — just navigate.
                    if let Ok(mut list) = vault::dispatch::list_files(cfg).await {
                        list.sort_by(|a, b| a.path.cmp(&b.path));
                        files.set(list);
                    }
                    active_path.set(Some(path));
                }
                Err(e) => load_error.set(Some(e.to_string())),
            }
        }
    } else {
        // Insert-only template: open as a new untitled note pre-filled with body.
        let body = template::strip_tabstops(&template::substitute_vars(&meta.body, &vars));
        let mut eval2 = document::eval("dioxus.send(new Date().toISOString().split('T')[0]);");
        let date_json2 = eval2.recv::<String>().await.unwrap_or_default();
        let path = format!("{date_json2}-note.md");
        match vault::dispatch::create_file(cfg, &path, &body, &format!("Create {path}")).await {
            Ok(_) => {
                if let Ok(mut list) = vault::dispatch::list_files(cfg).await {
                    list.sort_by(|a, b| a.path.cmp(&b.path));
                    files.set(list);
                }
                active_path.set(Some(path));
            }
            Err(e) if e.to_string().contains("File already exists") => {
                if let Ok(mut list) = vault::dispatch::list_files(cfg).await {
                    list.sort_by(|a, b| a.path.cmp(&b.path));
                    files.set(list);
                }
                active_path.set(Some(path));
            }
            Err(e) => load_error.set(Some(e.to_string())),
        }
    }
}

use crate::sleep_ms;

fn fuzzy_match(haystack: &str, needle: &str) -> bool {
    let mut it = haystack.chars();
    needle.chars().all(|nc| it.any(|hc| hc == nc))
}

fn fuzzy_score(path: &str, needle: &str) -> usize {
    let name = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    let q = needle.to_lowercase();
    if name == q { return 1000; }
    if name.starts_with(&q) { return 500; }
    if name.contains(&q) { return 200; }
    let mut score = 0usize;
    let mut last = 0usize;
    for nc in q.chars() {
        if let Some(pos) = name[last..].find(nc) {
            score += if pos == 0 { 10 } else { 1 };
            last += pos + nc.len_utf8();
        }
    }
    score
}

fn extract_headings(content: &str) -> Vec<(u8, String)> {
    content.lines().filter_map(|line| {
        let level = line.bytes().take_while(|&b| b == b'#').count();
        if level >= 1 && level <= 6 && line.as_bytes().get(level) == Some(&b' ') {
            Some((level as u8, line[level + 1..].trim().to_string()))
        } else {
            None
        }
    }).collect()
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

// ── Save status ───────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum SaveStatus { Idle, Countdown(u8), Saving, Saved, Error(String) }

impl SaveStatus {
    fn label(&self) -> String {
        match self {
            SaveStatus::Idle | SaveStatus::Saved => "Saved".into(),
            SaveStatus::Countdown(n) => format!("Saving in {n}s…"),
            SaveStatus::Saving  => "Saving…".into(),
            SaveStatus::Error(_) => "Save failed".into(),
        }
    }
    fn css_class(&self) -> &str {
        match self {
            SaveStatus::Error(_)   => "save-status save-status--error",
            SaveStatus::Countdown(_) => "save-status save-status--unsaved",
            SaveStatus::Saving     => "save-status save-status--saving",
            _ => "save-status",
        }
    }
}

#[derive(Clone, PartialEq)]
enum Panel { Files, Search, Backlinks, Graph, Bookmarks, Kanban }

#[derive(Clone, PartialEq, Copy)]
enum NavStyle { Tree, Flat, Columns }

// ── VaultBrowser ──────────────────────────────────────────────────────────────

#[component]
pub fn VaultBrowser(config: GithubConfig, on_logout: EventHandler<()>) -> Element {
    let mut files: Signal<Vec<FileMeta>> = use_signal(Vec::new);
    let mut active_path: Signal<Option<String>> = use_signal(|| None);
    let mut content = use_signal(String::new);
    let mut file_sha: Signal<String> = use_signal(String::new);
    let mut load_error: Signal<Option<String>> = use_signal(|| None);
    let mut loading_list = use_signal(|| true);
    let mut loading_file = use_signal(|| false);
    let mut save_status: Signal<SaveStatus> = use_signal(|| SaveStatus::Idle);
    let mut saved_content: Signal<String> = use_signal(String::new);
    // Incremented on every edit; used to debounce saves — a spawned save task
    // that finds a higher generation than it captured knows a newer edit supersedes it.
    let mut edit_gen: Signal<u64> = use_signal(|| 0);
    let mut panel: Signal<Panel> = use_signal(|| Panel::Files);
    // Mobile: controls whether the sidebar drawer is visible.
    // Web CSS ignores this class; mobile CSS uses it to slide the sidebar in/out.
    let mut sidebar_open = use_signal(|| true);
    let mut bookmarks: Signal<Vec<String>> = use_signal(Vec::new);
    let mut show_switcher = use_signal(|| false);
    let mut show_new_file = use_signal(|| false);
    let mut show_new_folder = use_signal(|| false);
    let mut new_file_result: Signal<Option<String>> = use_signal(|| None);
    let mut index: Signal<WikiLinkIndex> = use_signal(WikiLinkIndex::new);
    // Slash command query: Some("query") when `/query` is at cursor, None otherwise.
    let mut slash_query: Signal<Option<String>> = use_signal(|| None);
    // Path of the file whose content is currently in the editor (set after successful load).
    let mut loaded_path: Signal<Option<String>> = use_signal(|| None);
    let mut templates: Signal<Vec<TemplateMeta>> = use_signal(Vec::new);
    let mut board_root: Signal<String> = use_signal(String::new);
    let mut board_input: Signal<String> = use_signal(String::new);
    // The "current" folder: set when a folder is clicked or derived from the
    // folder of the currently open file. Drives the new-folder default parent.
    let mut selected_dir: Signal<Option<String>> = use_signal(|| None);
    let mut nav_style: Signal<NavStyle> = use_signal(|| NavStyle::Tree);

    // Load file list and bookmarks on mount.
    let cfg = config.clone();
    use_effect(move || {
        let cfg = cfg.clone();
        spawn(async move {
            let file_result = vault::dispatch::list_files(&cfg).await;
            let bm = state::load_bookmarks().await;
            match file_result {
                Ok(mut list) => { list.sort_by(|a, b| a.path.cmp(&b.path)); files.set(list); }
                Err(e) => load_error.set(Some(e.to_string())),
            }
            bookmarks.set(bm);
            loading_list.set(false);
            // Load saved kanban board root.
            let saved_board = {
                let mut ev = document::eval("dioxus.send(localStorage.getItem('oxidian_board') || '');");
                ev.recv::<String>().await.unwrap_or_default()
            };
            if !saved_board.is_empty() {
                board_root.set(saved_board.clone());
                board_input.set(saved_board);
            }
        });
    });

    // Load templates whenever the file list changes.
    let cfg_tmpl = config.clone();
    use_effect(move || {
        let cfg = cfg_tmpl.clone();
        let prefix = format!("{}/", cfg.templates_dir);
        let paths: Vec<String> = files.read()
            .iter()
            .filter(|f| f.path.starts_with(&prefix))
            .map(|f| f.path.clone())
            .collect();
        spawn(async move {
            if paths.is_empty() { templates.set(vec![]); return; }
            let contents = vault::dispatch::read_many(&cfg, &paths).await;
            templates.set(contents.iter()
                .map(|(p, c)| template::parse_template(p, c))
                .collect());
        });
    });

    // Load file content when active_path changes; save any pending changes first.
    let cfg = config.clone();
    use_effect(move || {
        let new_path = active_path.read().clone();
        let Some(p) = new_path else { return };
        loading_file.set(true);
        save_status.set(SaveStatus::Idle);
        let cfg = cfg.clone();
        let mut content = content.clone();
        // peek() reads without creating reactive subscriptions — these signals are
        // written by the async block below, and subscribing would re-run this
        // effect after every load, causing an infinite reload loop.
        let old_path = loaded_path.peek().clone();
        let old_sha = file_sha.peek().clone();
        let old_content = content.peek().clone();
        let old_saved = saved_content.peek().clone();
        spawn(async move {
            // Save pending changes for the previous file before switching.
            if let Some(ref old_p) = old_path {
                if !old_sha.is_empty() && old_content != old_saved {
                    let name = old_p.rsplit('/').next().unwrap_or(old_p).to_string();
                    if let Ok(new_sha) = vault::dispatch::write_file(
                        &cfg, old_p, &old_content, &old_sha, &format!("Update {name}")
                    ).await {
                        file_sha.set(new_sha);
                    }
                }
            }
            match vault::dispatch::read_file(&cfg, &p).await {
                Ok(fc) => {
                    console_log(&format!("[oxidian] loaded {p} sha={}", fc.sha));
                    index.with_mut(|idx| idx.index_file(&p, &fc.content));
                    content.set(fc.content.clone());
                    saved_content.set(fc.content);
                    file_sha.set(fc.sha);
                    loaded_path.set(Some(p));
                }
                Err(e) => {
                    console_log(&format!("[oxidian] load error: {e}"));
                    load_error.set(Some(e.to_string()));
                }
            }
            loading_file.set(false);
        });
    });

    // Debounced auto-save: each edit increments edit_gen and spawns a one-shot
    // save task.  The task counts down 5 s (1 s ticks), bailing if a newer edit
    // arrived (higher generation), so only the last edit in a burst actually saves.
    let cfg = config.clone();
    use_effect(move || {
        let current = content();
        if loading_file() || current.is_empty() || current == saved_content() { return; }

        // peek() avoids subscribing to edit_gen — writing it would otherwise
        // immediately re-trigger this effect.
        let this_gen = *edit_gen.peek() + 1;
        edit_gen.set(this_gen);
        save_status.set(SaveStatus::Countdown(5));
        tracing::debug!("auto-save: edit detected, gen={this_gen}, starting 5s countdown");
        console_log(&format!("[oxidian] auto-save: edit detected gen={this_gen}"));

        let cfg = cfg.clone();
        spawn(async move {
            // Countdown 5→4→3→2→1 with 1-second ticks; bail if superseded.
            for remaining in (1u8..5).rev() {
                sleep_ms(1000).await;
                if edit_gen() != this_gen {
                    tracing::debug!("auto-save: superseded at countdown {remaining} (cur_gen={}, this_gen={this_gen})", edit_gen());
                    return;
                }
                save_status.set(SaveStatus::Countdown(remaining));
            }
            sleep_ms(1000).await;
            if edit_gen() != this_gen {
                tracing::debug!("auto-save: superseded before save (cur_gen={}, this_gen={this_gen})", edit_gen());
                return;
            }

            let Some(path) = active_path() else {
                tracing::warn!("auto-save: no active path at save time, skipping (gen={this_gen})");
                return;
            };
            let sha = file_sha();
            if sha.is_empty() {
                tracing::warn!("auto-save: sha empty for {path}, skipping (gen={this_gen})");
                return;
            }
            let snapshot = content();
            if snapshot == saved_content() {
                tracing::debug!("auto-save: content unchanged for {path}, nothing to save (gen={this_gen})");
                return;
            }
            tracing::debug!("auto-save: saving {path} (sha={sha}, gen={this_gen})");
            console_log(&format!("[oxidian] auto-save: saving {path} sha={sha}"));
            save_status.set(SaveStatus::Saving);
            let name = path.rsplit('/').next().unwrap_or(&path).to_string();
            match vault::dispatch::write_file(&cfg, &path, &snapshot, &sha, &format!("Update {name}")).await {
                Ok(new_sha) => {
                    tracing::debug!("auto-save: saved {path}, new_sha={new_sha}");
                    console_log(&format!("[oxidian] auto-save: OK new_sha={new_sha}"));
                    index.with_mut(|idx| idx.reindex_file(&path, &snapshot));
                    file_sha.set(new_sha);
                    saved_content.set(snapshot);
                    save_status.set(SaveStatus::Saved);
                }
                Err(e) => {
                    tracing::error!("auto-save: write_file failed for {path}: {e}");
                    console_log(&format!("[oxidian] auto-save: FAILED {e}"));
                    save_status.set(SaveStatus::Error(e.to_string()));
                }
            }
        });
    });

    // Poll for slash command query every 150ms when a file is open.
    use_effect(move || {
        spawn(async move {
            loop {
                sleep_ms(150).await;
                if active_path().is_none() { slash_query.set(None); continue; }
                let q = {
                    let mut e = document::eval(JS_SLASH_QUERY);
                    e.recv::<String>().await.unwrap_or(JS_NO_SLASH.to_string())
                };
                if active_path().is_some() {
                    if q == JS_NO_SLASH {
                        slash_query.set(None);
                    } else {
                        slash_query.set(Some(q));
                    }
                }
            }
        });
    });

    // Handle new-file result: refresh list then open the file.
    let cfg = config.clone();
    use_effect(move || {
        let Some(path) = new_file_result() else { return };
        new_file_result.set(None);
        show_new_file.set(false);
        let cfg = cfg.clone();
        spawn(async move {
            if let Ok(mut list) = vault::dispatch::list_files(&cfg).await {
                list.sort_by(|a, b| a.path.cmp(&b.path));
                files.set(list);
            }
            active_path.set(Some(path));
            show_switcher.set(false);
        });
    });

    // Scroll active file entry into view whenever the active path changes.
    use_effect(move || {
        let _ = active_path();
        document::eval("setTimeout(() => { const el = document.querySelector('.file-entry--active'); if (el) el.scrollIntoView({ block: 'nearest' }); }, 50);");
    });

    // Keep the "current folder" in sync with the open file's folder. A folder
    // click overrides this (writes selected_dir directly); since this effect
    // only re-runs when active_path changes, it won't clobber that.
    use_effect(move || {
        if let Some(p) = active_path() {
            let dir = p.rfind('/').map(|i| p[..i].to_string());
            selected_dir.set(dir.filter(|d| !d.is_empty()));
        }
    });

    // Pre-compute values that can't borrow across rsx!.
    let status_class = save_status.read().css_class().to_string();
    let status_label = save_status.read().label();
    let status_title = match &*save_status.read() { SaveStatus::Error(e) => e.clone(), _ => String::new() };
    let words = word_count(&content.read());
    let headings = extract_headings(&content.read());
    let has_file = active_path.read().is_some();
    let is_bookmarked = active_path.read().as_ref()
        .map(|p| bookmarks.read().contains(p))
        .unwrap_or(false);

    // Pre-clone config for closures that need it. Signals are Copy so no issue there.
    let cfg_daily = config.clone();
    let cfg_search = config.clone();
    let cfg_newfile = config.clone();
    let cfg_delete = config.clone();

    let handle_delete = move |file: FileMeta| {
        let cfg = cfg_delete.clone();
        spawn(async move {
            let name = file.name().to_string();
            let confirmed = document::eval(&format!(
                "dioxus.send(!!window.confirm('Delete \\'{name}\\'? This cannot be undone.'));"
            ))
            .join::<bool>()
            .await
            .unwrap_or(false);
            if !confirmed { return; }
            match vault::dispatch::delete_file(&cfg, &file.path, &file.sha, &format!("Delete {name}")).await {
                Ok(()) => {
                    files.with_mut(|f| f.retain(|fi| fi.path != file.path));
                    if active_path().as_deref() == Some(file.path.as_str()) {
                        active_path.set(None);
                        content.set(String::new());
                        saved_content.set(String::new());
                        file_sha.set(String::new());
                        loaded_path.set(None);
                        save_status.set(SaveStatus::Idle);
                    }
                }
                Err(e) => load_error.set(Some(format!("Delete failed: {e}"))),
            }
        });
    };

    rsx! {
        div { class: "app-layout",

            // ── Sidebar ─────────────────────────────────────────────────────
            aside { class: if sidebar_open() { "sidebar sidebar--open" } else { "sidebar" },
                div { class: "sidebar-header",
                    span { class: "sidebar-title", "Oxidian" }
                    div { class: "sidebar-header-actions",
                        button {
                            class: "sidebar-icon-btn",
                            title: "New note",
                            onclick: move |_| show_new_file.set(true),
                            "✏"
                        }
                        button {
                            class: "sidebar-icon-btn",
                            title: "New folder",
                            onclick: move |_| show_new_folder.set(true),
                            "📁+"
                        }
                        button {
                            class: "sidebar-icon-btn",
                            title: "Today's note",
                            onclick: move |_| {
                                let cfg = cfg_daily.clone();
                                let tmpl_path = cfg.daily_note_template.clone();
                                let tmpl = templates.read().iter()
                                    .find(|t| t.source_path == tmpl_path)
                                    .cloned();
                                spawn(async move {
                                    if let Some(meta) = tmpl {
                                        apply_template(&meta, &cfg, files, active_path, load_error, "").await;
                                    } else {
                                        // Fallback: simple YYYY-MM-DD.md note
                                        let date = {
                                            let mut e = document::eval("dioxus.send(new Date().toISOString().split('T')[0]);");
                                            e.recv::<String>().await.unwrap_or_default()
                                        };
                                        if date.is_empty() { return; }
                                        let path = format!("{date}.md");
                                        let _ = vault::dispatch::create_file(
                                            &cfg, &path,
                                            &format!("# {date}\n\n"),
                                            &format!("Daily note {date}"),
                                        ).await;
                                        if let Ok(mut list) = vault::dispatch::list_files(&cfg).await {
                                            list.sort_by(|a, b| a.path.cmp(&b.path));
                                            files.set(list);
                                        }
                                        active_path.set(Some(path));
                                    }
                                    show_switcher.set(false);
                                    sidebar_open.set(false);
                                });
                            },
                            "📅"
                        }
                        button {
                            class: "sidebar-icon-btn",
                            title: "Disconnect vault",
                            onclick: move |_| { state::clear_config(); on_logout(()); },
                            "⚙"
                        }
                        // Close button — hidden on desktop via web CSS, visible on mobile
                        button {
                            class: "sidebar-icon-btn sidebar-close-btn",
                            title: "Close",
                            onclick: move |_| sidebar_open.set(false),
                            "✕"
                        }
                    }
                }

                div { class: "panel-tabs",
                    button { class: if panel() == Panel::Files { "panel-tab panel-tab--active" } else { "panel-tab" }, onclick: move |_| panel.set(Panel::Files), title: "Files", "📁" }
                    button { class: if panel() == Panel::Search { "panel-tab panel-tab--active" } else { "panel-tab" }, onclick: move |_| panel.set(Panel::Search), title: "Search", "🔍" }
                    button { class: if panel() == Panel::Backlinks { "panel-tab panel-tab--active" } else { "panel-tab" }, onclick: move |_| panel.set(Panel::Backlinks), title: "Backlinks", "↩" }
                    button { class: if panel() == Panel::Graph { "panel-tab panel-tab--active" } else { "panel-tab" }, onclick: move |_| panel.set(Panel::Graph), title: "Graph", "◉" }
                    button { class: if panel() == Panel::Bookmarks { "panel-tab panel-tab--active" } else { "panel-tab" }, onclick: move |_| panel.set(Panel::Bookmarks), title: "Bookmarks", "🔖" }
                    button { class: if panel() == Panel::Kanban { "panel-tab panel-tab--active" } else { "panel-tab" }, onclick: move |_| panel.set(Panel::Kanban), title: "Kanban", "🗂" }
                }

                div { class: "panel-content",
                    match panel() {
                        Panel::Files => rsx! {
                            div { class: "nav-style-picker",
                                button {
                                    class: if nav_style() == NavStyle::Tree { "nav-style-btn nav-style-btn--active" } else { "nav-style-btn" },
                                    title: "Tree view",
                                    onclick: move |_| nav_style.set(NavStyle::Tree),
                                    "🌲"
                                }
                                button {
                                    class: if nav_style() == NavStyle::Flat { "nav-style-btn nav-style-btn--active" } else { "nav-style-btn" },
                                    title: "Flat list",
                                    onclick: move |_| nav_style.set(NavStyle::Flat),
                                    "≡"
                                }
                                button {
                                    class: if nav_style() == NavStyle::Columns { "nav-style-btn nav-style-btn--active" } else { "nav-style-btn" },
                                    title: "Column view",
                                    onclick: move |_| nav_style.set(NavStyle::Columns),
                                    "⫼"
                                }
                            }
                            if loading_list() {
                                div { class: "sidebar-status", "Loading…" }
                            } else {
                                if let Some(err) = load_error() {
                                    div { class: "sidebar-error",
                                        span { "{err}" }
                                        button {
                                            class: "sidebar-error-close",
                                            onclick: move |_| load_error.set(None),
                                            "✕"
                                        }
                                    }
                                }
                                if files.read().is_empty() {
                                    div { class: "sidebar-status", "No markdown files found." }
                                } else {
                                    match nav_style() {
                                        NavStyle::Tree => rsx! {
                                            FileTree {
                                                files: files.read().clone(),
                                                active: active_path,
                                                selected_dir,
                                                on_select: move |path: String| {
                                                    active_path.set(Some(path));
                                                    show_switcher.set(false);
                                                    sidebar_open.set(false);
                                                },
                                                on_select_dir: move |dir: String| {
                                                    selected_dir.set(if dir.is_empty() { None } else { Some(dir) });
                                                },
                                                on_delete: handle_delete,
                                            }
                                        },
                                        NavStyle::Flat => rsx! {
                                            FlatList {
                                                files: files.read().clone(),
                                                active: active_path,
                                                selected_dir,
                                                on_select: move |path: String| {
                                                    active_path.set(Some(path));
                                                    show_switcher.set(false);
                                                    sidebar_open.set(false);
                                                },
                                                on_select_dir: move |dir: String| {
                                                    selected_dir.set(if dir.is_empty() { None } else { Some(dir) });
                                                },
                                                on_delete: handle_delete,
                                            }
                                        },
                                        NavStyle::Columns => rsx! {
                                            ColumnView {
                                                files: files.read().clone(),
                                                active: active_path,
                                                selected_dir,
                                                on_select: move |path: String| {
                                                    active_path.set(Some(path));
                                                    show_switcher.set(false);
                                                    sidebar_open.set(false);
                                                },
                                                on_select_dir: move |dir: String| {
                                                    selected_dir.set(if dir.is_empty() { None } else { Some(dir) });
                                                },
                                                on_delete: handle_delete,
                                            }
                                        },
                                    }
                                }
                            }
                            if has_file && !headings.is_empty() {
                                OutlinePane { headings }
                            }
                        },
                        Panel::Search => rsx! {
                            SearchPanel {
                                config: cfg_search,
                                on_select: move |path: String| {
                                    active_path.set(Some(path));
                                    show_switcher.set(false);
                                    sidebar_open.set(false);
                                },
                            }
                        },
                        Panel::Backlinks => rsx! {
                            BacklinksPanel {
                                active: active_path.read().clone(),
                                backlinks: {
                                    let idx = index.read();
                                    active_path.read().as_ref().map(|p| idx.backlinks(p).into_iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap_or_default()
                                },
                                on_select: move |path: String| {
                                    active_path.set(Some(path));
                                    show_switcher.set(false);
                                    sidebar_open.set(false);
                                },
                            }
                        },
                        Panel::Graph => rsx! {
                            GraphPanel {
                                files: files.read().iter().map(|f| f.path.clone()).collect::<Vec<_>>(),
                                active: active_path.read().clone(),
                                index: index.read().clone(),
                                on_select: move |path: String| {
                                    active_path.set(Some(path));
                                    show_switcher.set(false);
                                    sidebar_open.set(false);
                                },
                                config: cfg_search.clone(),
                            }
                        },
                        Panel::Bookmarks => rsx! {
                            BookmarksPanel {
                                bookmarks: bookmarks.read().clone(),
                                active: active_path.read().clone(),
                                on_select: move |path: String| {
                                    active_path.set(Some(path));
                                    show_switcher.set(false);
                                    sidebar_open.set(false);
                                },
                                on_remove: move |path: String| {
                                    bookmarks.with_mut(|bm| bm.retain(|p| p != &path));
                                    state::save_bookmarks(&bookmarks.read());
                                },
                            }
                        },
                        Panel::Kanban => rsx! {
                            div { class: "kanban-panel",
                                div { class: "kanban-panel-header",
                                    input {
                                        class: "kanban-root-input",
                                        placeholder: "Board (e.g. kanban or kanban/board.md)",
                                        value: "{board_input}",
                                        oninput: move |e| board_input.set(e.value()),
                                        onkeydown: move |e| {
                                            if e.key() == Key::Enter {
                                                let v = board_input.read().trim().to_string();
                                                board_root.set(v.clone());
                                                document::eval(&format!(
                                                    "localStorage.setItem('oxidian_board', {})",
                                                    serde_json::to_string(&v).unwrap_or_default()
                                                ));
                                            }
                                        },
                                    }
                                    button {
                                        class: "kanban-go-btn",
                                        title: "Open board",
                                        onclick: move |_| {
                                            let v = board_input.read().trim().to_string();
                                            board_root.set(v.clone());
                                            document::eval(&format!(
                                                "localStorage.setItem('oxidian_board', {})",
                                                serde_json::to_string(&v).unwrap_or_default()
                                            ));
                                        },
                                        "→"
                                    }
                                }
                                if board_root.read().is_empty() {
                                    div { class: "kanban-hint",
                                        "Enter a board name and press Enter. Oxidian creates/opens a "
                                        code { "kanban.md" }
                                        " document that defines the columns and card order; cards are notes in per-column subfolders."
                                    }
                                }
                            }
                        },
                    }
                }
            }

            // ── Sidebar resize handle ────────────────────────────────────────
            div {
                class: "sidebar-resize-handle",
                onpointerdown: move |_| {
                    document::eval(r#"
                        (function() {
                            const root = document.documentElement;
                            function onMove(e) {
                                const w = Math.max(160, Math.min(600, e.clientX));
                                root.style.setProperty('--sidebar-w', w + 'px');
                            }
                            function onUp() {
                                window.removeEventListener('pointermove', onMove);
                                window.removeEventListener('pointerup', onUp);
                                document.body.style.cursor = '';
                                document.body.style.userSelect = '';
                            }
                            document.body.style.cursor = 'col-resize';
                            document.body.style.userSelect = 'none';
                            window.addEventListener('pointermove', onMove);
                            window.addEventListener('pointerup', onUp);
                        })();
                    "#);
                },
            }

            // ── Editor pane ─────────────────────────────────────────────────
            main { class: "editor-pane",
                if panel() == Panel::Kanban && !board_root.read().is_empty() {
                    {
                        // Resolve the input into a board *document* path. A bare
                        // folder name (e.g. "kanban") maps to "kanban/kanban.md".
                        let raw = board_root.read().trim().trim_matches('/').to_string();
                        let board_path = if raw.ends_with(".md") { raw } else { format!("{raw}/kanban.md") };
                        rsx! {
                            KanbanBoard {
                                key: "{board_path}",
                                config: config.clone(),
                                board_path,
                                files: files.read().clone(),
                                on_open: move |path: String| {
                                    active_path.set(Some(path));
                                    panel.set(Panel::Files);
                                    sidebar_open.set(false);
                                },
                                on_files_changed: move |updated: Vec<FileMeta>| {
                                    files.set(updated);
                                },
                            }
                        }
                    }
                } else if let Some(ref path) = active_path() {
                    div { class: "editor-titlebar",
                        // Back button — hidden on desktop, visible on mobile
                        button {
                            class: "editor-icon-btn editor-back-btn",
                            title: "Back to files",
                            onclick: move |_| sidebar_open.set(true),
                            "‹"
                        }
                        span { class: "editor-filename", "{path}" }
                        div { class: "editor-meta",
                            button {
                                class: if is_bookmarked { "editor-icon-btn editor-icon-btn--active" } else { "editor-icon-btn" },
                                title: if is_bookmarked { "Remove bookmark" } else { "Add bookmark" },
                                onclick: move |_| {
                                    if let Some(p) = active_path() {
                                        if is_bookmarked {
                                            bookmarks.with_mut(|bm| bm.retain(|b| b != &p));
                                        } else {
                                            bookmarks.with_mut(|bm| { if !bm.contains(&p) { bm.push(p); } });
                                        }
                                        state::save_bookmarks(&bookmarks.read());
                                    }
                                },
                                "🔖"
                            }
                            if loading_file() {
                                span { class: "save-status", "Loading…" }
                            } else {
                                span { class: "word-count", "{words} words" }
                                span { class: "{status_class}", title: "{status_title}", "{status_label}" }
                                button {
                                    class: "editor-icon-btn",
                                    title: "Export as HTML",
                                    onclick: move |_| {
                                        if let Some(ref path) = active_path() {
                                            let title = path.rsplit('/').next().unwrap_or(path)
                                                .trim_end_matches(".md").to_string();
                                            let filename = format!("{title}.html");
                                            let html = export::to_html(&title, &content.read());
                                            document::eval(&export::download_html(&filename, &html));
                                        }
                                    },
                                    "↓"
                                }
                            }
                        }
                    }
                    PropertiesPanel { content }
                    FormattingToolbar { content }
                    MarkdownArea {
                        content,
                        variant: MarkdownAreaVariant::Ghost,
                        placeholder: "Empty file.",
                    }
                } else {
                    div { class: "editor-empty",
                        p { "Select a file to start editing." }
                        p { class: "editor-empty-sub",
                            "Connected to "
                            strong { "{config.owner}/{config.repo}" }
                            " · " code { "{config.branch}" }
                        }
                    }
                }
            }

            // ── Bottom nav (mobile only — hidden by web CSS) ─────────────────
            div { class: "bottom-nav",
                button {
                    class: if panel() == Panel::Files { "bottom-nav-btn bottom-nav-btn--active" } else { "bottom-nav-btn" },
                    onclick: move |_| { panel.set(Panel::Files); sidebar_open.set(true); },
                    span { "📁" }
                    span { class: "bottom-nav-label", "Files" }
                }
                button {
                    class: if panel() == Panel::Search { "bottom-nav-btn bottom-nav-btn--active" } else { "bottom-nav-btn" },
                    onclick: move |_| { panel.set(Panel::Search); sidebar_open.set(true); },
                    span { "🔍" }
                    span { class: "bottom-nav-label", "Search" }
                }
                button {
                    class: if panel() == Panel::Backlinks { "bottom-nav-btn bottom-nav-btn--active" } else { "bottom-nav-btn" },
                    onclick: move |_| { panel.set(Panel::Backlinks); sidebar_open.set(true); },
                    span { "↩" }
                    span { class: "bottom-nav-label", "Links" }
                }
                button {
                    class: if panel() == Panel::Graph { "bottom-nav-btn bottom-nav-btn--active" } else { "bottom-nav-btn" },
                    onclick: move |_| { panel.set(Panel::Graph); sidebar_open.set(true); },
                    span { "◉" }
                    span { class: "bottom-nav-label", "Graph" }
                }
                button {
                    class: if panel() == Panel::Bookmarks { "bottom-nav-btn bottom-nav-btn--active" } else { "bottom-nav-btn" },
                    onclick: move |_| { panel.set(Panel::Bookmarks); sidebar_open.set(true); },
                    span { "🔖" }
                    span { class: "bottom-nav-label", "Saved" }
                }
            }

            // ── Quick Switcher ───────────────────────────────────────────────
            if show_switcher() {
                QuickSwitcher {
                    files: files.read().clone(),
                    on_select: move |path: String| {
                        active_path.set(Some(path));
                        show_switcher.set(false);
                    },
                    on_close: move |_| show_switcher.set(false),
                }
            }

            // ── Slash command menu ───────────────────────────────────────────
            if let Some(ref q) = slash_query() {
                {
                    let cfg_t = config.clone();
                    rsx! {
                        SlashMenu {
                            query: q.clone(),
                            templates: templates.read().clone(),
                            on_select: move |insert: String| {
                                let query_len = slash_query().unwrap_or_default().len();
                                slash_query.set(None);
                                document::eval(&js_apply_slash(&insert, 1 + query_len));
                            },
                            on_template: move |meta: TemplateMeta| {
                                let query_len = slash_query().unwrap_or_default().len();
                                slash_query.set(None);
                                let cfg = cfg_t.clone();
                                let current_dir = active_path().and_then(|p| {
                                    p.rfind('/').map(|i| p[..i].to_string())
                                }).unwrap_or_default();
                                spawn(async move {
                                    if meta.filepath.is_some() {
                                        apply_template(&meta, &cfg, files, active_path, load_error, &current_dir).await;
                                    } else {
                                        // Insert-only: substitute vars and paste at cursor
                                        let date_json = {
                                            let mut e = document::eval(JS_DATE_VARS);
                                            e.recv::<String>().await.unwrap_or_default()
                                        };
                                        let vars = template::TemplateVars::from_json(&date_json, "", &current_dir);
                                        let body = template::strip_tabstops(
                                            &template::substitute_vars(&meta.body, &vars));
                                        document::eval(&js_apply_slash(&body, 1 + query_len));
                                    }
                                });
                            },
                            on_close: move |_| slash_query.set(None),
                        }
                    }
                }
            }

            // ── New file modal ───────────────────────────────────────────────
            if show_new_file() {
                NewFileModal {
                    config: cfg_newfile.clone(),
                    result: new_file_result,
                    current_dir: selected_dir.read().clone(),
                    on_close: move |_| show_new_file.set(false),
                }
            }

            if show_new_folder() {
                NewFolderModal {
                    config: cfg_newfile,
                    parent: selected_dir.read().clone(),
                    on_created: move |_| {
                        show_new_folder.set(false);
                        let cfg = config.clone();
                        spawn(async move {
                            if let Ok(mut list) = vault::dispatch::list_files(&cfg).await {
                                list.sort_by(|a, b| a.path.cmp(&b.path));
                                files.set(list);
                            }
                        });
                    },
                    on_close: move |_| show_new_folder.set(false),
                }
            }
        }
    }
}

// ── New file modal ────────────────────────────────────────────────────────────

#[component]
fn NewFileModal(
    config: GithubConfig,
    result: Signal<Option<String>>,
    current_dir: Option<String>,
    on_close: EventHandler<()>,
) -> Element {
    let mut name = use_signal(String::new);
    let mut creating = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    // Signal used to fire the create action from both onkeydown and onclick
    // without needing to move a non-Copy closure into two places.
    let mut trigger = use_signal(|| false);

    let current_dir_effect = current_dir.clone();
    use_effect(move || {
        if !trigger() { return; }
        trigger.set(false);
        let raw = name.read().trim().to_string();
        if raw.is_empty() { error.set(Some("Enter a file name.".into())); return; }
        // Leading "/" means root-relative; otherwise place in current_dir.
        let resolved = resolve_folder_path(&current_dir_effect, &raw);
        let path = if resolved.ends_with(".md") { resolved } else { format!("{resolved}.md") };
        let title = path.trim_end_matches(".md").to_string();
        let cfg = config.clone();
        creating.set(true);
        error.set(None);
        spawn(async move {
            match vault::dispatch::create_file(
                &cfg, &path, &format!("# {title}\n\n"), &format!("Create {path}")
            ).await {
                Ok(_)  => result.set(Some(path)),
                Err(e) => { error.set(Some(e.to_string())); creating.set(false); }
            }
        });
    });

    let preview = {
        let raw = name.read();
        let r = raw.trim();
        if r.is_empty() { String::new() } else {
            let resolved = resolve_folder_path(&current_dir, r);
            if resolved.ends_with(".md") { resolved } else { format!("{resolved}.md") }
        }
    };

    rsx! {
        div {
            class: "qs-overlay",
            onclick: move |_| on_close(()),
            div {
                class: "qs-modal", style: "max-width: 400px;",
                onclick: move |e| e.stop_propagation(),
                div { style: "padding: 16px 16px 8px; font-weight: 600;", "New note" }
                if let Some(ref p) = current_dir {
                    div { style: "padding: 0 16px 8px; font-size: 0.8rem; color: var(--text-muted);",
                        "Creating inside " code { "{p}/" } " — start with " code { "/" } " for vault root."
                    }
                }
                input {
                    class: "qs-input",
                    placeholder: "note-name  (or  /root-level  or  a/b/c)",
                    autofocus: true,
                    value: "{name}",
                    oninput: move |e| name.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter  { trigger.set(true); }
                        if e.key() == Key::Escape { on_close(()); }
                    },
                }
                if !preview.is_empty() {
                    div { style: "padding: 4px 16px 0; font-size: 0.8rem; color: var(--text-muted);",
                        "Will create: " code { "{preview}" }
                    }
                }
                if let Some(ref err) = error() {
                    div { style: "padding: 0 16px 8px; color: var(--danger); font-size: 0.85rem;", "{err}" }
                }
                div { style: "padding: 8px 16px 14px; display: flex; gap: 8px; justify-content: flex-end;",
                    button {
                        class: "settings-btn", style: "padding: 7px 16px;",
                        disabled: creating(),
                        onclick: move |_| trigger.set(true),
                        if creating() { "Creating…" } else { "Create" }
                    }
                }
            }
        }
    }
}

// ── New folder modal ──────────────────────────────────────────────────────────

/// Resolve a folder-name input into a full vault path:
///  - leading "/"  → root-relative (ignore the current folder)
///  - otherwise    → relative to `parent` (the current folder) if present
/// Intermediate folders are created implicitly (mkdir -p style) since git
/// derives directories from the blob path.
fn resolve_folder_path(parent: &Option<String>, raw: &str) -> String {
    let raw = raw.trim();
    if let Some(stripped) = raw.strip_prefix('/') {
        stripped.trim_matches('/').to_string()
    } else if let Some(p) = parent {
        format!("{}/{}", p.trim_end_matches('/'), raw.trim_matches('/'))
    } else {
        raw.trim_matches('/').to_string()
    }
}

#[component]
fn NewFolderModal(
    config: GithubConfig,
    parent: Option<String>,
    on_created: EventHandler<()>,
    on_close: EventHandler<()>,
) -> Element {
    let mut name = use_signal(String::new);
    let mut creating = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut trigger = use_signal(|| false);

    let parent_effect = parent.clone();
    use_effect(move || {
        if !trigger() { return; }
        trigger.set(false);
        let raw = name.read().trim().to_string();
        if raw.is_empty() { error.set(Some("Enter a folder name.".into())); return; }
        let folder = resolve_folder_path(&parent_effect, &raw);
        if folder.is_empty() { error.set(Some("Enter a folder name.".into())); return; }
        let path = format!("{folder}/.gitkeep");
        let cfg = config.clone();
        creating.set(true);
        error.set(None);
        spawn(async move {
            match vault::dispatch::create_file(&cfg, &path, "", &format!("Create folder {folder}")).await {
                Ok(_)  => on_created(()),
                Err(e) => { error.set(Some(e.to_string())); creating.set(false); }
            }
        });
    });

    let preview = {
        let raw = name.read();
        let r = raw.trim();
        if r.is_empty() { String::new() } else { resolve_folder_path(&parent, r) }
    };

    rsx! {
        div {
            class: "qs-overlay",
            onclick: move |_| on_close(()),
            div {
                class: "qs-modal", style: "max-width: 400px;",
                onclick: move |e| e.stop_propagation(),
                div { style: "padding: 16px 16px 8px; font-weight: 600;", "New folder" }
                if let Some(ref p) = parent {
                    div { style: "padding: 0 16px 8px; font-size: 0.8rem; color: var(--text-muted);",
                        "Creating inside " code { "{p}/" } " — start with " code { "/" } " for vault root."
                    }
                }
                input {
                    class: "qs-input",
                    placeholder: "folder-name  (or  /root-level  or  a/b/c)",
                    autofocus: true,
                    value: "{name}",
                    oninput: move |e| name.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter  { trigger.set(true); }
                        if e.key() == Key::Escape { on_close(()); }
                    },
                }
                if !preview.is_empty() {
                    div { style: "padding: 4px 16px 0; font-size: 0.8rem; color: var(--text-muted);",
                        "Will create: " code { "{preview}/" }
                    }
                }
                if let Some(ref err) = error() {
                    div { style: "padding: 0 16px 8px; color: var(--danger); font-size: 0.85rem;", "{err}" }
                }
                div { style: "padding: 8px 16px 14px; display: flex; gap: 8px; justify-content: flex-end;",
                    button {
                        class: "settings-btn", style: "padding: 7px 16px;",
                        disabled: creating(),
                        onclick: move |_| trigger.set(true),
                        if creating() { "Creating…" } else { "Create" }
                    }
                }
            }
        }
    }
}

// ── Search panel ──────────────────────────────────────────────────────────────

#[component]
fn SearchPanel(config: GithubConfig, on_select: EventHandler<String>) -> Element {
    let mut query = use_signal(String::new);
    let mut results: Signal<Vec<SearchResult>> = use_signal(Vec::new);
    let mut searching = use_signal(|| false);
    let mut search_error: Signal<Option<String>> = use_signal(|| None);

    use_effect(move || {
        document::eval(
            "requestAnimationFrame(() => { document.querySelector('.search-input')?.focus(); });"
        );
    });

    use_effect(move || {
        let q = query();
        let cfg = config.clone();
        if q.trim().is_empty() { results.set(vec![]); return; }
        searching.set(true);
        search_error.set(None);
        spawn(async move {
            sleep_ms(500).await;
            if query() != q { return; }
            match vault::github::search_code(&cfg, &q).await {
                Ok(r)  => results.set(r),
                Err(e) => search_error.set(Some(e.to_string())),
            }
            searching.set(false);
        });
    });

    let items: Vec<(String, String, String)> = results.read().iter()
        .map(|r| (r.path.clone(), r.path.rsplit('/').next().unwrap_or(&r.path).to_string(), r.fragment.clone()))
        .collect();

    rsx! {
        div { class: "search-panel",
            div { class: "search-input-wrap",
                input {
                    class: "search-input",
                    placeholder: "Search notes…",
                    value: "{query}",
                    oninput: move |e| query.set(e.value()),
                }
                if searching() { span { class: "search-spinner", "⟳" } }
            }
            if let Some(err) = search_error() {
                div { class: "search-error", "{err}" }
            } else if items.is_empty() && !query.read().is_empty() && !searching() {
                div { class: "search-empty", "No results." }
            } else {
                div { class: "search-results",
                    for (path, name, fragment) in items {
                        div {
                            class: "search-item",
                            onclick: move |_| on_select(path.clone()),
                            div { class: "search-item-name", "{name}" }
                            if !fragment.is_empty() {
                                div { class: "search-item-fragment", "{fragment}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Bookmarks panel ───────────────────────────────────────────────────────────

#[component]
fn BookmarksPanel(
    bookmarks: Vec<String>,
    active: Option<String>,
    on_select: EventHandler<String>,
    on_remove: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "bookmarks-panel",
            if bookmarks.is_empty() {
                div { class: "sidebar-status",
                    "No bookmarks yet."
                    br {}
                    "Click 🔖 in the editor to bookmark a file."
                }
            } else {
                for path in &bookmarks {
                    {
                        let p = path.clone();
                        let p2 = path.clone();
                        let name = path.rsplit('/').next().unwrap_or(path).to_string();
                        let is_active = active.as_deref() == Some(path.as_str());
                        rsx! {
                            div {
                                class: if is_active { "bookmark-item bookmark-item--active" } else { "bookmark-item" },
                                onclick: move |_| on_select(p.clone()),
                                span { class: "bookmark-name", "🔖 {name}" }
                                button {
                                    class: "bookmark-remove",
                                    title: "Remove bookmark",
                                    onclick: move |e| { e.stop_propagation(); on_remove(p2.clone()); },
                                    "×"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Outline pane ──────────────────────────────────────────────────────────────

#[component]
fn OutlinePane(headings: Vec<(u8, String)>) -> Element {
    rsx! {
        div { class: "outline-pane",
            div { class: "outline-title", "Outline" }
            for (level, text) in &headings {
                div {
                    class: "outline-item",
                    style: "padding-left: {(*level as usize - 1) * 12}px",
                    "{text}"
                }
            }
        }
    }
}

// ── Quick Switcher ────────────────────────────────────────────────────────────

#[component]
fn QuickSwitcher(
    files: Vec<FileMeta>,
    on_select: EventHandler<String>,
    on_close: EventHandler<()>,
) -> Element {
    let mut query = use_signal(String::new);

    use_effect(move || {
        document::eval(
            "requestAnimationFrame(() => { document.querySelector('.qs-input')?.focus(); });"
        );
    });

    let q = query.read().to_lowercase();
    let first_path: Option<String>;
    let items: Vec<(String, String, String)> = {
        let mut v: Vec<_> = if q.is_empty() {
            files.iter().map(|f| (0usize, f)).take(8).collect()
        } else {
            let mut ranked: Vec<_> = files.iter()
                .filter(|f| fuzzy_match(&f.path.to_lowercase(), &q))
                .map(|f| (fuzzy_score(&f.path, &q), f))
                .collect();
            ranked.sort_by(|a, b| b.0.cmp(&a.0));
            ranked.truncate(8);
            ranked
        };
        first_path = v.first().map(|(_, f)| f.path.clone());
        v.drain(..).map(|(_, f)| (f.path.clone(), f.name().to_string(), f.dir().to_string())).collect()
    };

    rsx! {
        div {
            class: "qs-overlay",
            onclick: move |_| on_close(()),
            div {
                class: "qs-modal",
                onclick: move |e| e.stop_propagation(),
                input {
                    class: "qs-input", placeholder: "Go to file…", autofocus: true,
                    value: "{query}",
                    oninput: move |e| query.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Escape { on_close(()); }
                        if e.key() == Key::Enter {
                            if let Some(ref p) = first_path { on_select(p.clone()); }
                        }
                    },
                }
                if items.is_empty() {
                    div { class: "qs-empty", "No matching files" }
                } else {
                    div { class: "qs-results",
                        for (path, name, dir) in items {
                            div {
                                class: "qs-item",
                                onclick: move |_| on_select(path.clone()),
                                span { class: "qs-item-name", "{name}" }
                                if !dir.is_empty() { span { class: "qs-item-dir", "{dir}" } }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Backlinks panel ───────────────────────────────────────────────────────────

#[component]
fn BacklinksPanel(
    active: Option<String>,
    backlinks: Vec<String>,
    on_select: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "backlinks-panel",
            div { class: "outline-title",
                if let Some(ref p) = active {
                    "Linked to "{p.rsplit('/').next().unwrap_or(p)}""
                } else {
                    "Backlinks"
                }
            }
            if backlinks.is_empty() {
                div { class: "sidebar-status", "No notes link here yet." }
            } else {
                for path in &backlinks {
                    {
                        let p = path.clone();
                        let name = path.rsplit('/').next().unwrap_or(path).to_string();
                        rsx! {
                            div {
                                class: "file-entry",
                                onclick: move |_| on_select(p.clone()),
                                "← {name}"
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Graph panel ───────────────────────────────────────────────────────────────

#[component]
fn GraphPanel(
    files: Vec<String>,
    active: Option<String>,
    index: WikiLinkIndex,
    on_select: EventHandler<String>,
    config: GithubConfig,
) -> Element {
    let indexed = index.indexed.len();
    let total = files.len();
    let edges = index.edges(&files);

    let connected: std::collections::HashSet<String> = edges.iter()
        .flat_map(|(s, t)| [s.clone(), t.clone()])
        .collect();

    let nodes: Vec<(String, String, bool)> = files.iter()
        .filter(|f| active.as_deref() == Some(f.as_str()) || connected.contains(*f))
        .map(|f| {
            let label = f.rsplit('/').next().unwrap_or(f).trim_end_matches(".md").to_string();
            let is_active = active.as_deref() == Some(f.as_str());
            (f.clone(), label, is_active)
        })
        .collect();

    rsx! {
        div { class: "graph-panel",
            div { class: "graph-toolbar",
                span { class: "outline-title", "Graph" }
                span { class: "save-status", "{indexed}/{total} indexed" }
            }
            if nodes.is_empty() {
                div { class: "sidebar-status", "Open linked notes to see connections." }
            } else {
                GraphView { nodes, edges, on_select }
            }
        }
    }
}

// ── File tree ─────────────────────────────────────────────────────────────────

fn group_by_dir(files: &[FileMeta], prefix: &str) -> (Vec<FileMeta>, Vec<(String, Vec<FileMeta>)>) {
    let strip = if prefix.is_empty() { String::new() } else { format!("{prefix}/") };
    let mut root: Vec<FileMeta> = Vec::new();
    let mut dirs: Vec<(String, Vec<FileMeta>)> = Vec::new();
    for file in files {
        let relative = if strip.is_empty() { file.path.as_str() }
                       else { file.path.strip_prefix(&strip).unwrap_or(&file.path) };
        if let Some(slash) = relative.find('/') {
            let child_name = &relative[..slash];
            let child_prefix = if prefix.is_empty() {
                child_name.to_string()
            } else {
                format!("{prefix}/{child_name}")
            };
            if let Some(g) = dirs.iter_mut().find(|(p, _)| p == &child_prefix) {
                g.1.push(file.clone());
            } else {
                dirs.push((child_prefix, vec![file.clone()]));
            }
        } else {
            root.push(file.clone());
        }
    }
    (root, dirs)
}

#[component]
fn FileTree(
    files: Vec<FileMeta>,
    active: ReadOnlySignal<Option<String>>,
    selected_dir: ReadOnlySignal<Option<String>>,
    on_select: EventHandler<String>,
    on_select_dir: EventHandler<String>,
    on_delete: EventHandler<FileMeta>,
) -> Element {
    let (root, dirs) = group_by_dir(&files, "");
    rsx! {
        div { class: "file-tree",
            for (dir_prefix, dir_files) in dirs {
                {
                    let name = dir_prefix.rsplit('/').next().unwrap_or(&dir_prefix).to_string();
                    rsx! {
                        FileTreeDir {
                            key: "{dir_prefix}",
                            name,
                            prefix: dir_prefix,
                            files: dir_files,
                            active,
                            selected_dir,
                            on_select,
                            on_select_dir,
                            on_delete,
                            depth: 0,
                        }
                    }
                }
            }
            for file in root {
                if file.name() != ".gitkeep" {
                    FileEntry {
                        key: "{file.path}",
                        file: file.clone(),
                        active: active().as_deref() == Some(file.path.as_str()),
                        on_select,
                        on_delete,
                        depth: 0,
                    }
                }
            }
        }
    }
}

#[component]
fn FileTreeDir(
    name: String,
    prefix: String,
    files: Vec<FileMeta>,
    active: ReadOnlySignal<Option<String>>,
    selected_dir: ReadOnlySignal<Option<String>>,
    on_select: EventHandler<String>,
    on_select_dir: EventHandler<String>,
    on_delete: EventHandler<FileMeta>,
    depth: u32,
) -> Element {
    let prefix_slash = format!("{prefix}/");
    let contains_active = |a: &Option<String>| {
        a.as_deref().map(|p| p.starts_with(&prefix_slash)).unwrap_or(false)
    };
    let mut collapsed = use_signal(|| !contains_active(&active()));
    // Auto-expand when the active file moves into this directory.
    let prefix_slash_effect = prefix_slash.clone();
    use_effect(move || {
        if active().as_deref().map(|a| a.starts_with(&prefix_slash_effect)).unwrap_or(false) {
            collapsed.set(false);
        }
    });
    let (root, subdirs) = group_by_dir(&files, &prefix);
    let dir_pl = 14 + depth * 14;
    let is_selected = selected_dir().as_deref() == Some(prefix.as_str());
    let prefix_click = prefix.clone();
    rsx! {
        div { class: "file-tree-dir",
            div {
                class: if is_selected { "file-tree-dir-name file-tree-dir-name--active" } else { "file-tree-dir-name" },
                style: "padding-left: {dir_pl}px",
                onclick: move |_| {
                    collapsed.set(!collapsed());
                    on_select_dir(prefix_click.clone());
                },
                span { class: "file-tree-dir-chevron", if collapsed() { "▶" } else { "▼" } }
                " 📁 {name}"
            }
            if !collapsed() {
                for (sub_prefix, sub_files) in subdirs {
                    {
                        let sub_name = sub_prefix.rsplit('/').next().unwrap_or(&sub_prefix).to_string();
                        rsx! {
                            FileTreeDir {
                                key: "{sub_prefix}",
                                name: sub_name,
                                prefix: sub_prefix,
                                files: sub_files,
                                active,
                                selected_dir,
                                on_select,
                                on_select_dir,
                                on_delete,
                                depth: depth + 1,
                            }
                        }
                    }
                }
                for file in root {
                    if file.name() != ".gitkeep" {
                        FileEntry {
                            key: "{file.path}",
                            file: file.clone(),
                            active: active().as_deref() == Some(file.path.as_str()),
                            on_select,
                            on_delete,
                            depth: depth + 1,
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn FileEntry(
    file: FileMeta,
    active: bool,
    on_select: EventHandler<String>,
    on_delete: EventHandler<FileMeta>,
    depth: u32,
) -> Element {
    let path = file.path.clone();
    let file_pl = 22 + depth * 14;
    let file_clone = file.clone();
    rsx! {
        div {
            class: if active { "file-entry file-entry--active" } else { "file-entry" },
            style: "padding-left: {file_pl}px",
            tabindex: "0",
            onclick: move |_| on_select(path.clone()),
            onkeydown: move |e| {
                if e.key() == Key::Delete || e.key() == Key::Backspace {
                    on_delete(file_clone.clone());
                }
            },
            span { class: "file-entry-name", "📄 {file.name()}" }
            button {
                class: "file-entry-delete",
                title: "Delete file",
                tabindex: "-1",
                onclick: move |e| {
                    e.stop_propagation();
                    on_delete(file.clone());
                },
                "🗑"
            }
        }
    }
}

// ── Flat list navigation ───────────────────────────────────────────────────────
//
// Shows all files as a flat, filtered list with sticky folder headers.
// Clicking a folder header selects it as the current dir (does not expand/collapse).

#[component]
fn FlatList(
    files: Vec<FileMeta>,
    active: ReadOnlySignal<Option<String>>,
    selected_dir: ReadOnlySignal<Option<String>>,
    on_select: EventHandler<String>,
    on_select_dir: EventHandler<String>,
    on_delete: EventHandler<FileMeta>,
) -> Element {
    let mut filter = use_signal(String::new);
    let q = filter.read().to_lowercase();

    // Build sorted, filtered file list.
    let filtered: Vec<&FileMeta> = files.iter()
        .filter(|f| f.name() != ".gitkeep")
        .filter(|f| q.is_empty() || f.path.to_lowercase().contains(&q))
        .collect();

    // Group into (folder_or_root, files) sections preserving sort order.
    let mut sections: Vec<(String, Vec<FileMeta>)> = Vec::new();
    for file in &filtered {
        let dir = file.path.rfind('/').map(|i| file.path[..i].to_string()).unwrap_or_default();
        if let Some(s) = sections.iter_mut().find(|(d, _)| d == &dir) {
            s.1.push((*file).clone());
        } else {
            sections.push((dir, vec![(*file).clone()]));
        }
    }

    rsx! {
        div { class: "flat-list",
            div { class: "flat-list-search",
                input {
                    class: "flat-list-input",
                    placeholder: "Filter…",
                    value: "{filter}",
                    oninput: move |e| filter.set(e.value()),
                }
            }
            if filtered.is_empty() && !q.is_empty() {
                div { class: "sidebar-status", "No matches." }
            } else {
                for (dir, dir_files) in sections {
                    {
                        let dir_clone = dir.clone();
                        let is_sel = selected_dir().as_deref() == Some(dir.as_str())
                            || (dir.is_empty() && selected_dir().is_none());
                        rsx! {
                            if !dir.is_empty() {
                                div {
                                    class: if is_sel { "flat-list-dir flat-list-dir--active" } else { "flat-list-dir" },
                                    onclick: move |_| on_select_dir(dir_clone.clone()),
                                    "📁 {dir}"
                                }
                            }
                            for file in dir_files {
                                {
                                    let p = file.path.clone();
                                    let is_active = active().as_deref() == Some(p.as_str());
                                    let file_clone = file.clone();
                                    rsx! {
                                        div {
                                            class: if is_active { "flat-list-item flat-list-item--active" } else { "flat-list-item" },
                                            onclick: move |_| on_select(p.clone()),
                                            span { class: "flat-list-name", "📄 {file.name()}" }
                                            button {
                                                class: "file-entry-delete",
                                                title: "Delete",
                                                tabindex: "-1",
                                                onclick: move |e| { e.stop_propagation(); on_delete(file_clone.clone()); },
                                                "🗑"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Column / Miller-columns navigation ────────────────────────────────────────
//
// Renders two panes side-by-side: left shows top-level folders + root files,
// right shows the contents of the selected folder (one level deep).
// Clicking a subfolder in the right pane drills into it (updating selected_dir).

#[component]
fn ColumnView(
    files: Vec<FileMeta>,
    active: ReadOnlySignal<Option<String>>,
    selected_dir: ReadOnlySignal<Option<String>>,
    on_select: EventHandler<String>,
    on_select_dir: EventHandler<String>,
    on_delete: EventHandler<FileMeta>,
) -> Element {
    let (root_files, top_dirs) = group_by_dir(&files, "");

    // Determine which column is currently "selected" (for highlight).
    let sel = selected_dir();

    // For the right column, show the contents of `sel` if it is one of the top-level dirs.
    // If sel is a subdir of a top-level dir, highlight the top-level dir and show its contents.
    let right_prefix: Option<String> = sel.clone().and_then(|s| {
        // Find the top-level ancestor.
        let top = top_dirs.iter().find(|(p, _)| s.starts_with(p.as_str()));
        top.map(|(p, _)| p.clone())
    }).or_else(|| sel.clone().and_then(|s| {
        if top_dirs.iter().any(|(p, _)| p == &s) { Some(s) } else { None }
    }));

    let right_files: Vec<FileMeta> = right_prefix.as_ref()
        .and_then(|p| top_dirs.iter().find(|(tp, _)| tp == p).map(|(_, f)| f.clone()))
        .unwrap_or_default();

    let (right_root, right_subdirs) = if let Some(ref p) = right_prefix {
        group_by_dir(&right_files, p)
    } else {
        (vec![], vec![])
    };

    rsx! {
        div { class: "col-view",
            // Left column: top-level dirs + root files
            div { class: "col-view-col col-view-left",
                for (dir_prefix, _) in &top_dirs {
                    {
                        let dp = dir_prefix.clone();
                        let dp2 = dir_prefix.clone();
                        let name = dir_prefix.rsplit('/').next().unwrap_or(dir_prefix).to_string();
                        let is_open = right_prefix.as_deref() == Some(dir_prefix.as_str());
                        rsx! {
                            div {
                                class: if is_open { "col-item col-item--dir col-item--open" } else { "col-item col-item--dir" },
                                onclick: move |_| on_select_dir(dp.clone()),
                                span { "📁 {name}" }
                                span { class: "col-chevron", if is_open { "›" } else { "›" } }
                            }
                        }
                    }
                }
                for file in &root_files {
                    if file.name() != ".gitkeep" {
                        {
                            let p = file.path.clone();
                            let is_active = active().as_deref() == Some(p.as_str());
                            let fc = file.clone();
                            rsx! {
                                div {
                                    class: if is_active { "col-item col-item--active" } else { "col-item" },
                                    onclick: move |_| on_select(p.clone()),
                                    span { "📄 {file.name()}" }
                                    button {
                                        class: "file-entry-delete",
                                        tabindex: "-1",
                                        onclick: move |e| { e.stop_propagation(); on_delete(fc.clone()); },
                                        "🗑"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Right column: contents of selected top-level dir
            if right_prefix.is_some() {
                div { class: "col-view-col col-view-right",
                    for (sub_prefix, _) in &right_subdirs {
                        {
                            let sp = sub_prefix.clone();
                            let name = sub_prefix.rsplit('/').next().unwrap_or(sub_prefix).to_string();
                            let is_sel = sel.as_deref() == Some(sub_prefix.as_str());
                            rsx! {
                                div {
                                    class: if is_sel { "col-item col-item--dir col-item--open" } else { "col-item col-item--dir" },
                                    onclick: move |_| on_select_dir(sp.clone()),
                                    span { "📁 {name}" }
                                    span { class: "col-chevron", "›" }
                                }
                            }
                        }
                    }
                    for file in &right_root {
                        if file.name() != ".gitkeep" {
                            {
                                let p = file.path.clone();
                                let is_active = active().as_deref() == Some(p.as_str());
                                let fc = file.clone();
                                rsx! {
                                    div {
                                        class: if is_active { "col-item col-item--active" } else { "col-item" },
                                        onclick: move |_| on_select(p.clone()),
                                        span { "📄 {file.name()}" }
                                        button {
                                            class: "file-entry-delete",
                                            tabindex: "-1",
                                            onclick: move |e| { e.stop_propagation(); on_delete(fc.clone()); },
                                            "🗑"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
