use dioxus::prelude::*;
use crate::icons::{
    IcoBookmark, IcoBookmarkCheck, IcoCalendar, IcoChevronDown, IcoChevronLeft,
    IcoChevronRight, IcoDownload, IcoFileText, IcoFilePlus, IcoFolderClosed,
    IcoFolderOpen, IcoFolderPlus, IcoFolderTree, IcoLayoutList, IcoLink2,
    IcoNetwork, IcoSearch, IcoSettings, IcoTrash2, IcoX, IcoFolderKanban,
};
use ui::{MarkdownArea, MarkdownAreaVariant};
use vault::{FileMeta, GithubConfig, SearchResult};

use crate::console_log;
use crate::export;
use crate::js;
use crate::state;
use crate::template::{self, TemplateMeta};
use crate::wikilink_index::WikiLinkIndex;
use super::graph::GraphView;
use super::kanban::KanbanBoard;
use super::properties::PropertiesPanel;
use super::slash::SlashMenu;
use super::toolbar::FormattingToolbar;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Applies a template: creates the target file (or navigates to it if it already
/// exists for filepath templates) and returns the path that was opened.
async fn apply_template(
    meta: &TemplateMeta,
    cfg: &GithubConfig,
    mut files: Signal<Vec<FileMeta>>,
    // Open mailbox of the target pane: setting it makes that pane open the path.
    mut open: Signal<Option<String>>,
    mut load_error: Signal<Option<String>>,
    current_dir: &str,
) {
    let date_json = crate::dates::date_vars_json().await;
    let vars = template::TemplateVars::from_json(&date_json, "", current_dir);

    if vars.year.is_empty() || vars.month.is_empty() || vars.date.is_empty() {
        load_error.set(Some("Could not read current date — please try again.".to_string()));
        return;
    }

    if let Some(ref fp_tmpl) = meta.filepath {
        let path = template::substitute_vars(fp_tmpl, &vars)
            .trim_start_matches('/').to_string();
        if files.read().iter().any(|f| f.path == path) {
            open.set(Some(path));
        } else {
            let body = template::strip_tabstops(&template::substitute_vars(&meta.body, &vars));
            match vault::dispatch::create_file(cfg, &path, &body, &format!("Create {path}")).await {
                Ok(_) => {
                    if let Ok(mut list) = vault::dispatch::list_files(cfg).await {
                        list.sort_by(|a, b| a.path.cmp(&b.path));
                        files.set(list);
                    }
                    open.set(Some(path));
                }
                Err(ref e) if e.to_string().contains("File already exists") => {
                    // File exists on remote but wasn't in the local list — just navigate.
                    if let Ok(mut list) = vault::dispatch::list_files(cfg).await {
                        list.sort_by(|a, b| a.path.cmp(&b.path));
                        files.set(list);
                    }
                    open.set(Some(path));
                }
                Err(e) => load_error.set(Some(e.to_string())),
            }
        }
    } else {
        // Insert-only template: open as a new untitled note pre-filled with body.
        let body = template::strip_tabstops(&template::substitute_vars(&meta.body, &vars));
        let today = crate::dates::today().await;
        let path = format!("{today}-note.md");
        match vault::dispatch::create_file(cfg, &path, &body, &format!("Create {path}")).await {
            Ok(_) => {
                if let Ok(mut list) = vault::dispatch::list_files(cfg).await {
                    list.sort_by(|a, b| a.path.cmp(&b.path));
                    files.set(list);
                }
                open.set(Some(path));
            }
            Err(e) if e.to_string().contains("File already exists") => {
                if let Ok(mut list) = vault::dispatch::list_files(cfg).await {
                    list.sort_by(|a, b| a.path.cmp(&b.path));
                    files.set(list);
                }
                open.set(Some(path));
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

// ── Command palette ─────────────────────────────────────────────────────────
// Every action reachable from the Command Palette (Ctrl/⌘-P). `VaultBrowser`
// owns the dispatch (`run_cmd`); the palette UI just lists these and reports the
// chosen one back. Keep `ALL` in display order.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Cmd {
    QuickSwitcher,
    NewFile,
    NewFolder,
    DailyNote,
    ExportHtml,
    ToggleSidebar,
    ToggleSplit,
    GoFiles,
    GoSearch,
    GoBacklinks,
    GoGraph,
    GoBookmarks,
    GoKanban,
}

impl Cmd {
    const ALL: &'static [Cmd] = &[
        Cmd::QuickSwitcher, Cmd::NewFile, Cmd::NewFolder, Cmd::DailyNote, Cmd::ExportHtml,
        Cmd::ToggleSidebar, Cmd::ToggleSplit,
        Cmd::GoFiles, Cmd::GoSearch, Cmd::GoBacklinks, Cmd::GoGraph, Cmd::GoBookmarks, Cmd::GoKanban,
    ];

    fn title(self) -> &'static str {
        match self {
            Cmd::QuickSwitcher => "Go to file…",
            Cmd::NewFile       => "New note",
            Cmd::NewFolder     => "New folder",
            Cmd::DailyNote     => "Open today's daily note",
            Cmd::ExportHtml    => "Export current note as HTML",
            Cmd::ToggleSidebar => "Toggle sidebar",
            Cmd::ToggleSplit   => "Toggle editor split",
            Cmd::GoFiles       => "Show: Files",
            Cmd::GoSearch      => "Show: Search",
            Cmd::GoBacklinks   => "Show: Backlinks",
            Cmd::GoGraph       => "Show: Graph",
            Cmd::GoBookmarks   => "Show: Bookmarks",
            Cmd::GoKanban      => "Show: Kanban",
        }
    }

    /// Shortcut hint shown on the right of the row (empty if none).
    fn hint(self) -> &'static str {
        match self {
            Cmd::QuickSwitcher => "Ctrl/⌘ O",
            _ => "",
        }
    }

    /// Extra search terms so commands are findable by intent, not just title.
    fn keywords(self) -> &'static str {
        match self {
            Cmd::QuickSwitcher => "switch open jump find",
            Cmd::NewFile       => "create page",
            Cmd::NewFolder     => "create directory",
            Cmd::DailyNote     => "today journal calendar",
            Cmd::ExportHtml    => "download save html",
            Cmd::ToggleSidebar => "hide show panel",
            Cmd::ToggleSplit   => "pane two columns",
            Cmd::GoFiles       => "tree explorer",
            Cmd::GoSearch      => "find grep",
            Cmd::GoBacklinks   => "links references",
            Cmd::GoGraph       => "network",
            Cmd::GoBookmarks   => "saved pinned",
            Cmd::GoKanban      => "board",
        }
    }
}

// ── Tabs ────────────────────────────────────────────────────────────────────
//
// Each editor pane keeps an ordered list of open tabs plus its active path.
// A tab is a "preview" tab while `pinned == false`: opening another file reuses
// the single preview slot (Obsidian behaviour). Editing the doc or double-
// clicking the tab pins it, so the next open spawns a fresh preview tab instead.

#[derive(Clone, PartialEq)]
struct Tab { path: String, pinned: bool }

/// Open `path` in a pane: activate it if already open, otherwise reuse the
/// pane's preview (unpinned) tab, or append a new preview tab. Sets `active`,
/// which drives the pane's load effect.
fn open_in_pane(mut tabs: Signal<Vec<Tab>>, mut active: Signal<Option<String>>, path: String) {
    if active.peek().as_deref() == Some(path.as_str()) { return; }
    let already = tabs.peek().iter().any(|t| t.path == path);
    if !already {
        tabs.with_mut(|ts| {
            if let Some(preview) = ts.iter_mut().find(|t| !t.pinned) {
                preview.path = path.clone();
            } else {
                ts.push(Tab { path: path.clone(), pinned: false });
            }
        });
    }
    active.set(Some(path));
}

/// Remove a tab from a pane. If it was active, activate a neighbour (or clear).
fn close_tab(mut tabs: Signal<Vec<Tab>>, mut active: Signal<Option<String>>, path: &str) {
    let idx = tabs.peek().iter().position(|t| t.path == path);
    let Some(idx) = idx else { return };
    tabs.with_mut(|ts| { ts.remove(idx); });
    if active.peek().as_deref() == Some(path) {
        let next = {
            let ts = tabs.peek();
            ts.get(idx).or_else(|| ts.get(idx.wrapping_sub(1))).map(|t| t.path.clone())
        };
        active.set(next);
    }
}

// ── Nav plugin registry ───────────────────────────────────────────────────────
//
// Each nav view is a `NavPlugin` entry in the static registry below.
// To add a new view: (1) push an entry here, (2) add a match arm in
// `nav_dispatch` at the bottom of this file.
//
// For runtime-loaded (truly third-party) plugins, the next step is to expose a
// `NavPluginRegistry` Dioxus Context holding `Vec<Box<dyn NavPluginDyn>>` where
// the trait has a `render(&self, props: NavPluginProps) -> Element` method using
// Dioxus's `VNode` API. That is left as future work.

pub struct NavPlugin {
    pub id: &'static str,
    pub label: &'static str,
}

static NAV_PLUGINS: &[NavPlugin] = &[
    NavPlugin { id: "tree", label: "Tree" },
    NavPlugin { id: "flat", label: "Flat list" },
];

// ── VaultBrowser ──────────────────────────────────────────────────────────────

#[component]
pub fn VaultBrowser(config: GithubConfig, on_logout: EventHandler<()>) -> Element {
    let mut files: Signal<Vec<FileMeta>> = use_signal(Vec::new);
    // ── Editor panes ──
    // Each pane (`EditorPane`) owns its own editor state and effects. Here we
    // hold only the per-pane *tab list* + *active path* + an *open mailbox*
    // (writing a path tells that pane to open it), plus the split/focus state.
    // `active_path` / `content` below are read-only MIRRORS of the focused pane,
    // kept so the sidebar (file highlight, outline) needs no pane awareness.
    let tabs_a: Signal<Vec<Tab>> = use_signal(Vec::new);
    let active_a: Signal<Option<String>> = use_signal(|| None);
    let open_a: Signal<Option<String>> = use_signal(|| None);
    let tabs_b: Signal<Vec<Tab>> = use_signal(Vec::new);
    let active_b: Signal<Option<String>> = use_signal(|| None);
    let open_b: Signal<Option<String>> = use_signal(|| None);
    let mut split = use_signal(|| false);
    let mut focused: Signal<usize> = use_signal(|| 0);
    // Focused-pane mirrors (written by the focused EditorPane).
    let active_path: Signal<Option<String>> = use_signal(|| None);
    let content: Signal<String> = use_signal(String::new);

    let mut load_error: Signal<Option<String>> = use_signal(|| None);
    let mut loading_list = use_signal(|| true);
    let mut panel: Signal<Panel> = use_signal(|| Panel::Files);
    // Mobile: controls whether the sidebar drawer is visible.
    // Web CSS ignores this class; mobile CSS uses it to slide the sidebar in/out.
    let mut sidebar_open = use_signal(|| true);
    let mut bookmarks: Signal<Vec<String>> = use_signal(Vec::new);
    let mut show_switcher = use_signal(|| false);
    let mut show_palette = use_signal(|| false);
    let mut show_new_file = use_signal(|| false);
    let mut show_new_folder = use_signal(|| false);
    let mut new_file_result: Signal<Option<String>> = use_signal(|| None);
    let index: Signal<WikiLinkIndex> = use_signal(WikiLinkIndex::new);
    let mut templates: Signal<Vec<TemplateMeta>> = use_signal(Vec::new);
    let mut board_root: Signal<String> = use_signal(String::new);
    let mut board_input: Signal<String> = use_signal(String::new);
    // The "current" folder: set when a folder is clicked or derived from the
    // folder of the currently open file. Drives the new-folder default parent.
    let mut selected_dir: Signal<Option<String>> = use_signal(|| None);
    let mut nav_style: Signal<&'static str> = use_signal(|| "tree");

    // Route an open request to whichever pane is focused. Copy (captures only
    // Copy signals) so it can be used inside many event closures.
    let open_focused = move |path: String| {
        if focused() == 1 { open_in_pane(tabs_b, active_b, path); }
        else { open_in_pane(tabs_a, active_a, path); }
    };
    // The focused pane's open mailbox, for APIs that take a Signal (templates).
    let focused_open_mbx = move || if focused() == 1 { open_b } else { open_a };

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
            let saved_board = js::ls_get("oxidian_board").await;
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

    // (Editor load / auto-save / slash-poll effects now live in `EditorPane`.)

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
            open_focused(path);
            show_switcher.set(false);
        });
    });

    // Scroll active file entry into view whenever the active path changes.
    use_effect(move || {
        let _ = active_path();
        js::scroll_active_into_view();
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

    // Pre-compute values for the sidebar (from the focused-pane mirrors).
    let headings = extract_headings(&content.read());
    let has_file = active_path.read().is_some();

    // Pre-clone config for closures that need it. Signals are Copy so no issue there.
    let cfg_daily = config.clone();
    let cfg_search = config.clone();
    let cfg_newfile = config.clone();
    let cfg_delete = config.clone();
    let cfg_move = config.clone();

    // ── Command actions ───────────────────────────────────────────────────────
    // Shared, Copy callbacks so the same logic runs from a toolbar button, the
    // command palette, and a keyboard shortcut.

    // Open (or create) today's daily note. Extracted from the sidebar button so
    // the palette can trigger it too.
    let run_daily = use_callback(move |_: ()| {
        let cfg = cfg_daily.clone();
        let tmpl_path = cfg.daily_note_template.clone();
        let tmpl = templates.read().iter().find(|t| t.source_path == tmpl_path).cloned();
        let open_mbx = focused_open_mbx();
        spawn(async move {
            if let Some(meta) = tmpl {
                apply_template(&meta, &cfg, files, open_mbx, load_error, "").await;
            } else {
                let date = crate::dates::today().await;
                if date.is_empty() { return; }
                let path = format!("{date}.md");
                let _ = vault::dispatch::create_file(
                    &cfg, &path, &format!("# {date}\n\n"), &format!("Daily note {date}"),
                ).await;
                if let Ok(mut list) = vault::dispatch::list_files(&cfg).await {
                    list.sort_by(|a, b| a.path.cmp(&b.path));
                    files.set(list);
                }
                open_focused(path);
            }
            show_switcher.set(false);
            sidebar_open.set(false);
        });
    });

    // Export the focused note as standalone HTML (uses the focused-pane mirrors).
    let run_export = use_callback(move |_: ()| {
        if let Some(p) = active_path() {
            let title = p.rsplit('/').next().unwrap_or(&p).trim_end_matches(".md").to_string();
            let html = export::to_html(&title, &content.read());
            js::download_file(format!("{title}.html"), html);
        }
    });

    // Central palette dispatcher.
    let run_cmd = use_callback(move |cmd: Cmd| {
        match cmd {
            Cmd::QuickSwitcher => show_switcher.set(true),
            Cmd::NewFile       => show_new_file.set(true),
            Cmd::NewFolder     => show_new_folder.set(true),
            Cmd::DailyNote     => run_daily.call(()),
            Cmd::ExportHtml    => run_export.call(()),
            Cmd::ToggleSidebar => sidebar_open.set(!sidebar_open()),
            Cmd::ToggleSplit   => split.set(!split()),
            Cmd::GoFiles       => panel.set(Panel::Files),
            Cmd::GoSearch      => panel.set(Panel::Search),
            Cmd::GoBacklinks   => panel.set(Panel::Backlinks),
            Cmd::GoGraph       => panel.set(Panel::Graph),
            Cmd::GoBookmarks   => panel.set(Panel::Bookmarks),
            Cmd::GoKanban      => panel.set(Panel::Kanban),
        }
        show_palette.set(false);
    });

    // Global keyboard shortcuts (web/desktop; harmless no-op on mobile).
    let on_shortcut = use_callback(move |id: String| match id.as_str() {
        "palette"  => show_palette.set(true),
        "switcher" => show_switcher.set(true),
        _ => {}
    });
    crate::shortcuts::use_global_shortcuts(on_shortcut);

    let handle_delete = move |file: FileMeta| {
        let cfg = cfg_delete.clone();
        spawn(async move {
            let name = file.name().to_string();
            let confirmed = js::confirm_dialog(
                &format!("Delete '{name}'? This cannot be undone.")
            ).await;
            if !confirmed { return; }
            match vault::dispatch::delete_file(&cfg, &file.path, &file.sha, &format!("Delete {name}")).await {
                Ok(()) => {
                    files.with_mut(|f| f.retain(|fi| fi.path != file.path));
                    // Drop the (now-gone) file from any pane that had it open.
                    close_tab(tabs_a, active_a, &file.path);
                    close_tab(tabs_b, active_b, &file.path);
                }
                Err(e) => load_error.set(Some(format!("Delete failed: {e}"))),
            }
        });
    };

    // Move a file or folder via drag-and-drop. `payload` is "file\x1e<path>" or
    // "dir\x1e<prefix>"; `dest_dir` is the target folder ("" = vault root).
    let handle_move = move |(payload, dest_dir): (String, String)| {
        let cfg = cfg_move.clone();
        spawn(async move {
            let mut parts = payload.splitn(2, '\x1e');
            let kind = parts.next().unwrap_or("").to_string();
            let src = parts.next().unwrap_or("").trim_matches('/').to_string();
            if src.is_empty() { return; }
            let dest = dest_dir.trim_matches('/').to_string();
            let name = src.rsplit('/').next().unwrap_or(&src).to_string();
            let src_parent = src.rfind('/').map(|i| &src[..i]).unwrap_or("");

            // No-op: already directly inside the destination folder.
            if src_parent == dest { return; }
            let new_path = if dest.is_empty() { name.clone() } else { format!("{dest}/{name}") };
            // Can't drop a folder into itself or a descendant.
            if kind == "dir" && (dest == src || dest.starts_with(&format!("{src}/"))) { return; }

            let dest_label = if dest.is_empty() { "vault root".to_string() } else { dest.clone() };
            let confirmed = js::confirm_dialog(&format!("Move '{name}' into '{dest_label}'?")).await;
            if !confirmed { return; }

            let snapshot = files.peek().clone();
            let result = if kind == "dir" {
                vault::dispatch::move_dir(&cfg, &src, &new_path, &snapshot).await
            } else {
                let sha = snapshot.iter().find(|f| f.path == src).map(|f| f.sha.clone()).unwrap_or_default();
                vault::dispatch::move_file(&cfg, &src, &sha, &new_path).await
            };
            match result {
                Ok(()) => {
                    if let Ok(mut list) = vault::dispatch::list_files(&cfg).await {
                        list.sort_by(|a, b| a.path.cmp(&b.path));
                        files.set(list);
                    }
                    // Rewrite any open tabs / active paths that pointed at the
                    // moved file or a file inside the moved folder, in both panes.
                    let rewrite = |p: &str| -> Option<String> {
                        if kind == "dir" {
                            if p == src { Some(new_path.clone()) }
                            else { p.strip_prefix(&format!("{src}/")).map(|rel| format!("{new_path}/{rel}")) }
                        } else if p == src { Some(new_path.clone()) } else { None }
                    };
                    for (mut tabs, mut active) in [(tabs_a, active_a), (tabs_b, active_b)] {
                        tabs.with_mut(|ts| {
                            for t in ts.iter_mut() {
                                if let Some(n) = rewrite(&t.path) { t.path = n; }
                            }
                        });
                        let cur = active.peek().clone();
                        if let Some(n) = cur.and_then(|c| rewrite(&c)) { active.set(Some(n)); }
                    }
                }
                Err(e) => load_error.set(Some(format!("Move failed: {e}"))),
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
                            IcoFilePlus { size: 16 }
                        }
                        button {
                            class: "sidebar-icon-btn",
                            title: "New folder",
                            onclick: move |_| show_new_folder.set(true),
                            IcoFolderPlus { size: 16 }
                        }
                        button {
                            class: "sidebar-icon-btn",
                            title: "Today's note",
                            onclick: move |_| run_daily.call(()),
                            IcoCalendar { size: 16 }
                        }
                        button {
                            class: "sidebar-icon-btn",
                            title: "Disconnect vault",
                            onclick: move |_| { state::clear_config(); on_logout(()); },
                            IcoSettings { size: 16 }
                        }
                        // Close button — hidden on desktop via web CSS, visible on mobile
                        button {
                            class: "sidebar-icon-btn sidebar-close-btn",
                            title: "Close",
                            onclick: move |_| sidebar_open.set(false),
                            IcoX { size: 16 }
                        }
                    }
                }

                div { class: "panel-tabs",
                    button { class: if panel() == Panel::Files { "panel-tab panel-tab--active" } else { "panel-tab" }, onclick: move |_| panel.set(Panel::Files), title: "Files", IcoFolderTree { size: 15 } }
                    button { class: if panel() == Panel::Search { "panel-tab panel-tab--active" } else { "panel-tab" }, onclick: move |_| panel.set(Panel::Search), title: "Search", IcoSearch { size: 15 } }
                    button { class: if panel() == Panel::Backlinks { "panel-tab panel-tab--active" } else { "panel-tab" }, onclick: move |_| panel.set(Panel::Backlinks), title: "Backlinks", IcoLink2 { size: 15 } }
                    button { class: if panel() == Panel::Graph { "panel-tab panel-tab--active" } else { "panel-tab" }, onclick: move |_| panel.set(Panel::Graph), title: "Graph", IcoNetwork { size: 15 } }
                    button { class: if panel() == Panel::Bookmarks { "panel-tab panel-tab--active" } else { "panel-tab" }, onclick: move |_| panel.set(Panel::Bookmarks), title: "Bookmarks", IcoBookmark { size: 15 } }
                    button { class: if panel() == Panel::Kanban { "panel-tab panel-tab--active" } else { "panel-tab" }, onclick: move |_| panel.set(Panel::Kanban), title: "Kanban", IcoFolderKanban { size: 15 } }
                }

                div { class: "panel-content",
                    match panel() {
                        Panel::Files => rsx! {
                            // Plugin picker — icons hardcoded for builtins; label from registry
                            div { class: "nav-style-picker",
                                button {
                                    class: if nav_style() == "tree" { "nav-style-btn nav-style-btn--active" } else { "nav-style-btn" },
                                    title: "Tree",
                                    onclick: move |_| nav_style.set("tree"),
                                    IcoFolderTree { size: 14 }
                                }
                                button {
                                    class: if nav_style() == "flat" { "nav-style-btn nav-style-btn--active" } else { "nav-style-btn" },
                                    title: "Flat list",
                                    onclick: move |_| nav_style.set("flat"),
                                    IcoLayoutList { size: 14 }
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
                                            IcoX { size: 13 }
                                        }
                                    }
                                }
                                if files.read().is_empty() {
                                    div { class: "sidebar-status", "No markdown files found." }
                                } else {
                                    { nav_dispatch(nav_style(), NavCallbacks {
                                        files: files.read().clone(),
                                        active: active_path,
                                        selected_dir,
                                        on_select: EventHandler::new(move |path: String| {
                                            open_focused(path);
                                            show_switcher.set(false);
                                            sidebar_open.set(false);
                                        }),
                                        on_select_dir: EventHandler::new(move |dir: String| {
                                            selected_dir.set(if dir.is_empty() { None } else { Some(dir) });
                                        }),
                                        on_delete: EventHandler::new(handle_delete),
                                        on_move: EventHandler::new(handle_move),
                                    }) }
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
                                    open_focused(path);
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
                                    open_focused(path);
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
                                    open_focused(path);
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
                                    open_focused(path);
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
                                                js::ls_set("oxidian_board", v);
                                            }
                                        },
                                    }
                                    button {
                                        class: "kanban-go-btn",
                                        title: "Open board",
                                        onclick: move |_| {
                                            let v = board_input.read().trim().to_string();
                                            board_root.set(v.clone());
                                            js::ls_set("oxidian_board", v);
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

            // ── Drawer scrim (mobile only — `.sidebar-scrim` is display:none on
            // desktop, so this is a no-op there even though sidebar_open is true).
            if sidebar_open() {
                div {
                    class: "sidebar-scrim",
                    onclick: move |_| sidebar_open.set(false),
                }
            }

            // ── Sidebar resize handle ────────────────────────────────────────
            div {
                class: "sidebar-resize-handle",
                onpointerdown: move |_| js::start_sidebar_resize(),
            }

            // ── Editor pane(s) ───────────────────────────────────────────────
            main { class: if split() { "editor-pane editor-pane--split" } else { "editor-pane" },
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
                                    open_focused(path);
                                    panel.set(Panel::Files);
                                    sidebar_open.set(false);
                                },
                                on_files_changed: move |updated: Vec<FileMeta>| {
                                    files.set(updated);
                                },
                            }
                        }
                    }
                } else {
                    EditorPane {
                        config: config.clone(),
                        pane_idx: 0,
                        focused,
                        to_open: open_a,
                        tabs: tabs_a,
                        active: active_a,
                        files,
                        index,
                        bookmarks,
                        templates,
                        load_error,
                        mirror_active: active_path,
                        mirror_content: content,
                        can_split: !split(),
                        on_split: move |_| split.set(true),
                        on_close_pane: None,
                        on_send_other: move |p: String| { open_in_pane(tabs_b, active_b, p); focused.set(1); split.set(true); },
                        on_back: move |_| sidebar_open.set(true),
                        on_palette: move |_| show_palette.set(true),
                    }
                    if split() {
                        EditorPane {
                            config: config.clone(),
                            pane_idx: 1,
                            focused,
                            to_open: open_b,
                            tabs: tabs_b,
                            active: active_b,
                            files,
                            index,
                            bookmarks,
                            templates,
                            load_error,
                            mirror_active: active_path,
                            mirror_content: content,
                            can_split: false,
                            on_split: move |_| {},
                            on_close_pane: Some(EventHandler::new(move |_| { split.set(false); focused.set(0); })),
                            on_send_other: move |p: String| { open_in_pane(tabs_a, active_a, p); focused.set(0); },
                            on_back: move |_| sidebar_open.set(true),
                            on_palette: move |_| show_palette.set(true),
                        }
                    }
                }
            }

            // ── Bottom nav (mobile only — hidden by web CSS) ─────────────────
            div { class: "bottom-nav",
                button {
                    class: if panel() == Panel::Files { "bottom-nav-btn bottom-nav-btn--active" } else { "bottom-nav-btn" },
                    onclick: move |_| { panel.set(Panel::Files); sidebar_open.set(true); },
                    IcoFolderTree { size: 18 }
                    span { class: "bottom-nav-label", "Files" }
                }
                button {
                    class: if panel() == Panel::Search { "bottom-nav-btn bottom-nav-btn--active" } else { "bottom-nav-btn" },
                    onclick: move |_| { panel.set(Panel::Search); sidebar_open.set(true); },
                    IcoSearch { size: 18 }
                    span { class: "bottom-nav-label", "IcoSearch" }
                }
                button {
                    class: if panel() == Panel::Backlinks { "bottom-nav-btn bottom-nav-btn--active" } else { "bottom-nav-btn" },
                    onclick: move |_| { panel.set(Panel::Backlinks); sidebar_open.set(true); },
                    IcoLink2 { size: 18 }
                    span { class: "bottom-nav-label", "Links" }
                }
                button {
                    class: if panel() == Panel::Graph { "bottom-nav-btn bottom-nav-btn--active" } else { "bottom-nav-btn" },
                    onclick: move |_| { panel.set(Panel::Graph); sidebar_open.set(true); },
                    IcoNetwork { size: 18 }
                    span { class: "bottom-nav-label", "Graph" }
                }
                button {
                    class: if panel() == Panel::Bookmarks { "bottom-nav-btn bottom-nav-btn--active" } else { "bottom-nav-btn" },
                    onclick: move |_| { panel.set(Panel::Bookmarks); sidebar_open.set(true); },
                    IcoBookmark { size: 18 }
                    span { class: "bottom-nav-label", "Saved" }
                }
            }

            // ── Quick Switcher ───────────────────────────────────────────────
            if show_switcher() {
                QuickSwitcher {
                    files: files.read().clone(),
                    on_select: move |path: String| {
                        open_focused(path);
                        show_switcher.set(false);
                    },
                    on_close: move |_| show_switcher.set(false),
                }
            }

            // ── Command Palette ──────────────────────────────────────────────
            if show_palette() {
                CommandPalette {
                    on_run: move |cmd: Cmd| run_cmd.call(cmd),
                    on_close: move |_| show_palette.set(false),
                }
            }

            // (Slash command menu now lives inside each `EditorPane`.)

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

// ── Editor pane ─────────────────────────────────────────────────────────────
//
// One independent editor: its own tab list (`tabs`/`active`), document state,
// and the load / auto-save / slash effects. Two of these render side by side in
// split mode. `to_open` is a mailbox the parent writes to ask this pane to open
// a path; `mirror_active`/`mirror_content` are written while this pane is
// focused so the sidebar can stay pane-agnostic.
#[component]
fn EditorPane(
    config: GithubConfig,
    pane_idx: usize,
    mut focused: Signal<usize>,
    mut to_open: Signal<Option<String>>,
    mut tabs: Signal<Vec<Tab>>,
    active: Signal<Option<String>>,
    mut files: Signal<Vec<FileMeta>>,
    mut index: Signal<WikiLinkIndex>,
    mut bookmarks: Signal<Vec<String>>,
    templates: Signal<Vec<TemplateMeta>>,
    mut load_error: Signal<Option<String>>,
    mut mirror_active: Signal<Option<String>>,
    mut mirror_content: Signal<String>,
    can_split: bool,
    on_split: EventHandler<()>,
    on_close_pane: Option<EventHandler<()>>,
    on_send_other: EventHandler<String>,
    on_back: EventHandler<()>,
    on_palette: EventHandler<()>,
) -> Element {
    let content = use_signal(String::new);
    let mut file_sha: Signal<String> = use_signal(String::new);
    let mut saved_content: Signal<String> = use_signal(String::new);
    let mut save_status: Signal<SaveStatus> = use_signal(|| SaveStatus::Idle);
    let mut edit_gen: Signal<u64> = use_signal(|| 0);
    let mut loaded_path: Signal<Option<String>> = use_signal(|| None);
    let mut loading_file = use_signal(|| false);
    let mut slash_query: Signal<Option<String>> = use_signal(|| None);

    // Mailbox: parent writes a path here to ask this pane to open it.
    use_effect(move || {
        let req = to_open.read().clone();
        if let Some(p) = req {
            to_open.set(None);
            open_in_pane(tabs, active, p);
        }
    });

    // Mirror this pane's active/content up while it is the focused pane.
    use_effect(move || {
        let foc = focused();
        let a = active();
        let c = content();
        if foc == pane_idx {
            mirror_active.set(a);
            mirror_content.set(c);
        }
    });

    // Pin the active tab once its document is edited (so the next open spawns a
    // fresh preview tab instead of replacing this one).
    use_effect(move || {
        if loading_file() { return; }
        if content() != saved_content() {
            if let Some(p) = active.peek().clone() {
                tabs.with_mut(|ts| {
                    if let Some(t) = ts.iter_mut().find(|t| t.path == p && !t.pinned) { t.pinned = true; }
                });
            }
        }
    });

    // Load file content when `active` changes; save any pending changes first.
    let cfg = config.clone();
    use_effect(move || {
        let new_path = active.read().clone();
        let Some(p) = new_path else { return };
        loading_file.set(true);
        save_status.set(SaveStatus::Idle);
        let cfg = cfg.clone();
        let mut content = content;
        let old_path = loaded_path.peek().clone();
        let old_sha = file_sha.peek().clone();
        let old_content = content.peek().clone();
        let old_saved = saved_content.peek().clone();
        spawn(async move {
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

    // Debounced auto-save (5s, superseded by newer edits via edit_gen).
    let cfg = config.clone();
    use_effect(move || {
        let current = content();
        if loading_file() || current.is_empty() || current == saved_content() {
            console_log(&format!(
                "[oxidian] auto-save: skip (loading={}, empty={}, unchanged={})",
                loading_file(), current.is_empty(), current == saved_content()
            ));
            return;
        }
        console_log(&format!("[oxidian] auto-save: edit detected, {} bytes, starting countdown", current.len()));
        let this_gen = *edit_gen.peek() + 1;
        edit_gen.set(this_gen);
        save_status.set(SaveStatus::Countdown(5));
        let cfg = cfg.clone();
        spawn(async move {
            for remaining in (1u8..5).rev() {
                sleep_ms(1000).await;
                if edit_gen() != this_gen { return; }
                save_status.set(SaveStatus::Countdown(remaining));
            }
            sleep_ms(1000).await;
            if edit_gen() != this_gen { return; }
            let Some(path) = active() else { return };
            let sha = file_sha();
            if sha.is_empty() {
                console_log("[oxidian] auto-save: aborted — file_sha is empty");
                return;
            }
            let snapshot = content();
            if snapshot == saved_content() { return; }
            save_status.set(SaveStatus::Saving);
            console_log(&format!("[oxidian] auto-save: writing {path} ({} bytes, sha={sha})", snapshot.len()));
            let name = path.rsplit('/').next().unwrap_or(&path).to_string();
            match vault::dispatch::write_file(&cfg, &path, &snapshot, &sha, &format!("Update {name}")).await {
                Ok(new_sha) => {
                    console_log(&format!("[oxidian] auto-save: OK, new sha={new_sha}"));
                    index.with_mut(|idx| idx.reindex_file(&path, &snapshot));
                    file_sha.set(new_sha);
                    saved_content.set(snapshot);
                    save_status.set(SaveStatus::Saved);
                }
                Err(e) => {
                    console_log(&format!("[oxidian] auto-save: write_file FAILED for {path}: {e}"));
                    tracing::error!("auto-save: write_file failed for {path}: {e}");
                    save_status.set(SaveStatus::Error(e.to_string()));
                }
            }
        });
    });

    // Poll for slash query — only act when this is the focused pane (the JS is
    // focus-aware, so an unfocused pane would otherwise mirror the same query).
    use_effect(move || {
        spawn(async move {
            loop {
                sleep_ms(150).await;
                if focused() != pane_idx || active().is_none() { slash_query.set(None); continue; }
                let q = js::slash_query().await;
                if q == js::NO_SLASH { slash_query.set(None); } else { slash_query.set(Some(q)); }
            }
        });
    });

    // Pre-compute for rsx.
    let path_opt = active.read().clone();
    let is_bookmarked = path_opt.as_ref().map(|p| bookmarks.read().contains(p)).unwrap_or(false);
    let words = word_count(&content.read());
    let status_class = save_status.read().css_class().to_string();
    let status_label = save_status.read().label();
    let status_title = match &*save_status.read() { SaveStatus::Error(e) => e.clone(), _ => String::new() };
    let is_focused = focused() == pane_idx;
    let col_class = if is_focused { "editor-pane-col editor-pane-col--focused" } else { "editor-pane-col" };
    let cfg_slash = config.clone();

    rsx! {
        div { class: "{col_class}",
            // ── Tab strip (hidden until at least one doc is open) ──
            if !tabs.read().is_empty() {
            div { class: "tab-strip",
                for tab in tabs.read().iter().cloned() {
                    {
                        let p = tab.path.clone();
                        let p_dbl = tab.path.clone();
                        let p_close = tab.path.clone();
                        let name = tab.path.rsplit('/').next().unwrap_or(&tab.path).trim_end_matches(".md").to_string();
                        let is_active = active.read().as_deref() == Some(tab.path.as_str());
                        let cls = match (is_active, tab.pinned) {
                            (true, true)  => "tab tab--active",
                            (true, false) => "tab tab--active tab--preview",
                            (false, true) => "tab",
                            (false, false)=> "tab tab--preview",
                        };
                        rsx! {
                            div {
                                key: "{p}",
                                class: "{cls}",
                                onclick: move |_| { focused.set(pane_idx); open_in_pane(tabs, active, p.clone()); },
                                ondoubleclick: move |_| {
                                    tabs.with_mut(|ts| { if let Some(t) = ts.iter_mut().find(|t| t.path == p_dbl) { t.pinned = true; } });
                                },
                                span { class: "tab-name", "{name}" }
                                button {
                                    class: "tab-close",
                                    title: "Close tab",
                                    onclick: move |e| { e.stop_propagation(); close_tab(tabs, active, &p_close); },
                                    IcoX { size: 11 }
                                }
                            }
                        }
                    }
                }
                div { class: "tab-strip-actions",
                    if can_split {
                        button { class: "editor-icon-btn", title: "Split right", onclick: move |_| on_split(()), "⊟" }
                    }
                    if let Some(close) = on_close_pane {
                        button { class: "editor-icon-btn", title: "Close pane", onclick: move |_| close(()), IcoX { size: 14 } }
                    }
                }
            }
            }

            if let Some(path) = path_opt {
                div { class: "editor-titlebar",
                    button {
                        class: "editor-icon-btn editor-back-btn",
                        title: "Back to files",
                        onclick: move |_| on_back(()),
                        IcoChevronLeft { size: 18 }
                    }
                    span { class: "editor-filename", "{path}" }
                    div { class: "editor-meta",
                        button {
                            class: "editor-icon-btn",
                            title: "Command palette (Ctrl/⌘ P)",
                            onclick: move |_| on_palette.call(()),
                            IcoLayoutList { size: 15 }
                        }
                        button {
                            class: if is_bookmarked { "editor-icon-btn editor-icon-btn--active" } else { "editor-icon-btn" },
                            title: if is_bookmarked { "Remove bookmark" } else { "Add bookmark" },
                            onclick: move |_| {
                                if let Some(p) = active() {
                                    if is_bookmarked {
                                        bookmarks.with_mut(|bm| bm.retain(|b| b != &p));
                                    } else {
                                        bookmarks.with_mut(|bm| { if !bm.contains(&p) { bm.push(p); } });
                                    }
                                    state::save_bookmarks(&bookmarks.read());
                                }
                            },
                            if is_bookmarked { IcoBookmarkCheck { size: 15 } } else { IcoBookmark { size: 15 } }
                        }
                        if loading_file() {
                            span { class: "save-status", "Loading…" }
                        } else {
                            span { class: "word-count", "{words} words" }
                            span { class: "{status_class}", title: "{status_title}", "{status_label}" }
                            button {
                                class: "editor-icon-btn",
                                title: "Move to other pane",
                                onclick: move |_| {
                                    if let Some(p) = active.peek().clone() {
                                        on_send_other(p.clone());
                                        close_tab(tabs, active, &p);
                                    }
                                },
                                "⇄"
                            }
                            button {
                                class: "editor-icon-btn",
                                title: "Export as HTML",
                                onclick: move |_| {
                                    if let Some(ref path) = active() {
                                        let title = path.rsplit('/').next().unwrap_or(path)
                                            .trim_end_matches(".md").to_string();
                                        let filename = format!("{title}.html");
                                        let html = export::to_html(&title, &content.read());
                                        js::download_file(filename, html);
                                    }
                                },
                                IcoDownload { size: 15 }
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
                    onfocus: move |_| focused.set(pane_idx),
                }
            } else {
                div {
                    class: "editor-empty",
                    onclick: move |_| focused.set(pane_idx),
                    p { "Select a file to start editing." }
                    p { class: "editor-empty-sub",
                        "Connected to "
                        strong { "{config.owner}/{config.repo}" }
                        " · " code { "{config.branch}" }
                    }
                }
            }

            // ── Slash command menu (focused pane only) ──
            if is_focused {
                if let Some(ref q) = slash_query() {
                    {
                        let cfg_t = cfg_slash.clone();
                        rsx! {
                            SlashMenu {
                                query: q.clone(),
                                templates: templates.read().clone(),
                                on_select: move |insert: String| {
                                    let query_len = slash_query().unwrap_or_default().len();
                                    slash_query.set(None);
                                    js::apply_slash(insert, 1 + query_len);
                                },
                                on_template: move |meta: TemplateMeta| {
                                    let query_len = slash_query().unwrap_or_default().len();
                                    slash_query.set(None);
                                    let cfg = cfg_t.clone();
                                    let current_dir = active().and_then(|p| {
                                        p.rfind('/').map(|i| p[..i].to_string())
                                    }).unwrap_or_default();
                                    spawn(async move {
                                        if meta.filepath.is_some() {
                                            apply_template(&meta, &cfg, files, to_open, load_error, &current_dir).await;
                                        } else {
                                            let date_json = crate::dates::date_vars_json().await;
                                            let vars = template::TemplateVars::from_json(&date_json, "", &current_dir);
                                            let body = template::strip_tabstops(
                                                &template::substitute_vars(&meta.body, &vars));
                                            js::apply_slash(body, 1 + query_len);
                                        }
                                    });
                                },
                                on_close: move |_| slash_query.set(None),
                            }
                        }
                    }
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

// ── IcoSearch panel ──────────────────────────────────────────────────────────────

#[component]
fn SearchPanel(config: GithubConfig, on_select: EventHandler<String>) -> Element {
    let mut query = use_signal(String::new);
    let mut results: Signal<Vec<SearchResult>> = use_signal(Vec::new);
    let mut searching = use_signal(|| false);
    let mut search_error: Signal<Option<String>> = use_signal(|| None);

    use_effect(move || {
        js::focus_selector(".search-input");
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
                    placeholder: "IcoSearch notes…",
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
                                IcoBookmark { size: 13 }
                                span { class: "bookmark-name", "{name}" }
                                button {
                                    class: "bookmark-remove",
                                    title: "Remove bookmark",
                                    onclick: move |e| { e.stop_propagation(); on_remove(p2.clone()); },
                                    IcoX { size: 12 }
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
        js::focus_selector(".qs-input");
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

// ── Command palette ───────────────────────────────────────────────────────────

#[component]
fn CommandPalette(on_run: EventHandler<Cmd>, on_close: EventHandler<()>) -> Element {
    let mut query = use_signal(String::new);

    use_effect(move || {
        js::focus_selector(".qs-input");
    });

    let q = query.read().to_lowercase();
    let mut matches: Vec<Cmd> = Cmd::ALL.iter().copied()
        .filter(|c| {
            q.is_empty()
                || fuzzy_match(&c.title().to_lowercase(), &q)
                || c.keywords().contains(&q)
        })
        .collect();
    if !q.is_empty() {
        matches.sort_by(|a, b| fuzzy_score(b.title(), &q).cmp(&fuzzy_score(a.title(), &q)));
    }
    let first = matches.first().copied();
    let empty = matches.is_empty();

    rsx! {
        div {
            class: "qs-overlay",
            onclick: move |_| on_close(()),
            div {
                class: "qs-modal",
                onclick: move |e| e.stop_propagation(),
                input {
                    class: "qs-input", placeholder: "Run a command…", autofocus: true,
                    value: "{query}",
                    oninput: move |e| query.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Escape { on_close(()); }
                        if e.key() == Key::Enter {
                            if let Some(c) = first { on_run(c); }
                        }
                    },
                }
                if empty {
                    div { class: "qs-empty", "No matching commands" }
                } else {
                    div { class: "qs-results",
                        for c in matches {
                            div {
                                class: "qs-item",
                                onclick: move |_| on_run(c),
                                span { class: "qs-item-name", "{c.title()}" }
                                if !c.hint().is_empty() {
                                    span { class: "qs-item-dir", "{c.hint()}" }
                                }
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

// ── Nav dispatch ─────────────────────────────────────────────────────────────
//
// Central routing point for the nav plugin system. Accepts the active plugin id
// and a `NavCallbacks` bag, returns the right component. When adding a plugin:
//  1. Push a `NavPlugin` to `NAV_PLUGINS`.
//  2. Add a match arm here.

struct NavCallbacks {
    files: Vec<FileMeta>,
    active: Signal<Option<String>>,
    selected_dir: Signal<Option<String>>,
    on_select: EventHandler<String>,
    on_select_dir: EventHandler<String>,
    on_delete: EventHandler<FileMeta>,
    // (drag payload, destination dir). Payload is `"file\x1e<path>"` or `"dir\x1e<prefix>"`.
    on_move: EventHandler<(String, String)>,
}

fn nav_dispatch(id: &'static str, cb: NavCallbacks) -> Element {
    let NavCallbacks { files, active, selected_dir, on_select, on_select_dir, on_delete, on_move } = cb;
    match id {
        "flat" => rsx! {
            FlatList {
                files,
                active,
                selected_dir,
                on_select,
                on_select_dir,
                on_delete,
                on_move,
            }
        },
        "columns" => rsx! {
            ColumnView {
                files,
                active,
                selected_dir,
                on_select,
                on_select_dir,
                on_delete,
                on_move,
            }
        },
        // "tree" is the default / fallback
        _ => rsx! {
            FileTree {
                files,
                active,
                selected_dir,
                on_select,
                on_select_dir,
                on_delete,
                on_move,
            }
        },
    }
}

// ── Drag-and-drop move helpers ─────────────────────────────────────────────────
//
// Drag payloads are encoded as `"<kind>\x1e<path>"` (kind = "file" | "dir"),
// matching the Kanban drag convention. `set_drag_data` / `get_drag_data` are the
// same JS-backed helpers the Kanban board uses.

/// Builds the `ondrop` payload for a draggable file.
fn file_drag_payload(path: &str) -> String { format!("file\x1e{path}") }
/// Builds the `ondrop` payload for a draggable folder.
fn dir_drag_payload(prefix: &str) -> String { format!("dir\x1e{prefix}") }

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
    active: Signal<Option<String>>,
    selected_dir: Signal<Option<String>>,
    on_select: EventHandler<String>,
    on_select_dir: EventHandler<String>,
    on_delete: EventHandler<FileMeta>,
    on_move: EventHandler<(String, String)>,
) -> Element {
    let (root, dirs) = group_by_dir(&files, "");
    let mut root_drop = use_signal(|| false);
    rsx! {
        // The root container is itself a drop target → moves to the vault root.
        // Dir rows call stop_propagation() so a drop on a folder doesn't also
        // bubble up here.
        div {
            class: if root_drop() { "file-tree file-tree--drop" } else { "file-tree" },
            ondragover: move |e| { e.prevent_default(); root_drop.set(true); },
            ondragleave: move |_| root_drop.set(false),
            ondrop: move |_| {
                root_drop.set(false);
                spawn(async move {
                    let data = js::get_drag_data().await;
                    js::clear_drag_data();
                    if !data.is_empty() { on_move((data, String::new())); }
                });
            },
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
                            on_move,
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
    active: Signal<Option<String>>,
    selected_dir: Signal<Option<String>>,
    on_select: EventHandler<String>,
    on_select_dir: EventHandler<String>,
    on_delete: EventHandler<FileMeta>,
    on_move: EventHandler<(String, String)>,
    depth: u32,
) -> Element {
    let prefix_slash = format!("{prefix}/");
    let contains_active = |a: &Option<String>| {
        a.as_deref().map(|p| p.starts_with(&prefix_slash)).unwrap_or(false)
    };
    let mut collapsed = use_signal(|| !contains_active(&active()));
    let mut drag_over = use_signal(|| false);
    // Auto-expand when the active file moves into this directory.
    let prefix_slash_effect = prefix_slash.clone();
    use_effect(move || {
        if active().as_deref().map(|a| a.starts_with(&prefix_slash_effect)).unwrap_or(false) {
            collapsed.set(false);
        }
    });
    let (root, subdirs) = group_by_dir(&files, &prefix);
    let dir_pl = 10 + depth * 10;
    let is_selected = selected_dir().as_deref() == Some(prefix.as_str());
    let prefix_click = prefix.clone();
    let prefix_drag = prefix.clone();
    let prefix_drop = prefix.clone();
    let dir_class = if drag_over() { "file-tree-dir-name file-tree-dir-name--drop" }
                    else if is_selected { "file-tree-dir-name file-tree-dir-name--active" }
                    else { "file-tree-dir-name" };
    rsx! {
        div { class: "file-tree-dir",
            div {
                class: "{dir_class}",
                style: "padding-left: {dir_pl}px",
                draggable: true,
                ondragstart: move |_| js::set_drag_data(dir_drag_payload(&prefix_drag)),
                ondragover: move |e| { e.prevent_default(); drag_over.set(true); },
                ondragleave: move |_| drag_over.set(false),
                ondrop: move |e| {
                    e.stop_propagation();
                    drag_over.set(false);
                    let dest = prefix_drop.clone();
                    spawn(async move {
                        let data = js::get_drag_data().await;
                        js::clear_drag_data();
                        if !data.is_empty() { on_move((data, dest)); }
                    });
                },
                onclick: move |_| {
                    collapsed.set(!collapsed());
                    on_select_dir(prefix_click.clone());
                },
                span { class: "file-tree-dir-chevron", if collapsed() { IcoChevronRight { size: 11 } } else { IcoChevronDown { size: 11 } } }
                if collapsed() { IcoFolderClosed { size: 14 } } else { IcoFolderOpen { size: 14 } }
                span { class: "file-tree-dir-label", "{name}" }
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
                                on_move,
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
    let path_drag = file.path.clone();
    let file_pl = 18 + depth * 10;
    let file_clone = file.clone();
    rsx! {
        div {
            class: if active { "file-entry file-entry--active" } else { "file-entry" },
            style: "padding-left: {file_pl}px",
            tabindex: "0",
            draggable: true,
            ondragstart: move |_| js::set_drag_data(file_drag_payload(&path_drag)),
            onclick: move |_| on_select(path.clone()),
            onkeydown: move |e| {
                if e.key() == Key::Delete || e.key() == Key::Backspace {
                    on_delete(file_clone.clone());
                }
            },
            span { class: "file-entry-icon", IcoFileText { size: 13 } }
            span { class: "file-entry-name", "{file.name()}" }
            button {
                class: "file-entry-delete",
                title: "Delete file",
                tabindex: "-1",
                onclick: move |e| {
                    e.stop_propagation();
                    on_delete(file.clone());
                },
                IcoTrash2 { size: 12 }
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
    active: Signal<Option<String>>,
    selected_dir: Signal<Option<String>>,
    on_select: EventHandler<String>,
    on_select_dir: EventHandler<String>,
    on_delete: EventHandler<FileMeta>,
    on_move: EventHandler<(String, String)>,
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
                        let dir_drag = dir.clone();
                        let dir_drop = dir.clone();
                        let is_sel = selected_dir().as_deref() == Some(dir.as_str())
                            || (dir.is_empty() && selected_dir().is_none());
                        rsx! {
                            if !dir.is_empty() {
                                div {
                                    class: if is_sel { "flat-list-dir flat-list-dir--active" } else { "flat-list-dir" },
                                    draggable: true,
                                    ondragstart: move |_| js::set_drag_data(dir_drag_payload(&dir_drag)),
                                    ondragover: move |e| e.prevent_default(),
                                    ondrop: move |_| {
                                        let dest = dir_drop.clone();
                                        spawn(async move {
                                            let data = js::get_drag_data().await;
                                            js::clear_drag_data();
                                            if !data.is_empty() { on_move((data, dest)); }
                                        });
                                    },
                                    onclick: move |_| on_select_dir(dir_clone.clone()),
                                    IcoFolderClosed { size: 12 }
                                    " {dir}"
                                }
                            }
                            for file in dir_files {
                                {
                                    let p = file.path.clone();
                                    let p_drag = file.path.clone();
                                    let is_active = active().as_deref() == Some(p.as_str());
                                    let file_clone = file.clone();
                                    rsx! {
                                        div {
                                            class: if is_active { "flat-list-item flat-list-item--active" } else { "flat-list-item" },
                                            draggable: true,
                                            ondragstart: move |_| js::set_drag_data(file_drag_payload(&p_drag)),
                                            onclick: move |_| on_select(p.clone()),
                                            span { class: "file-entry-icon", IcoFileText { size: 12 } }
                                            span { class: "flat-list-name", "{file.name()}" }
                                            button {
                                                class: "file-entry-delete",
                                                title: "Delete",
                                                tabindex: "-1",
                                                onclick: move |e| { e.stop_propagation(); on_delete(file_clone.clone()); },
                                                IcoTrash2 { size: 12 }
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
// Two-pane view. `col_path` is the directory whose contents appear in the LEFT
// column (empty = vault root). The RIGHT column shows the direct children of
// whichever folder is selected in the left column.
//
// Clicking a folder in the RIGHT pane advances `col_path` to that folder (the
// right column becomes the new left) and clears the inner selection.
// A breadcrumb bar at the top lets you jump back to any ancestor, including root.

#[component]
fn ColumnView(
    files: Vec<FileMeta>,
    active: Signal<Option<String>>,
    selected_dir: Signal<Option<String>>,
    on_select: EventHandler<String>,
    on_select_dir: EventHandler<String>,
    on_delete: EventHandler<FileMeta>,
    on_move: EventHandler<(String, String)>,
) -> Element {
    // The directory currently shown in the LEFT column. Empty = vault root.
    let mut col_path: Signal<String> = use_signal(String::new);
    // Which item in the left column is "open" (shown in the right column).
    let mut open_child: Signal<Option<String>> = use_signal(|| None);

    // Sync open_child with external selected_dir when col_path is root (initial load).
    use_effect(move || {
        if !col_path.read().is_empty() { return; }
        if let Some(ref s) = selected_dir() {
            // If s is a direct child of root (no second slash), open it.
            if !s.contains('/') {
                open_child.set(Some(s.clone()));
            } else {
                // Ancestor is the first path segment.
                let anc = s.split('/').next().unwrap_or("").to_string();
                if !anc.is_empty() { open_child.set(Some(anc)); }
            }
        }
    });

    let cp = col_path.read().clone();

    // Build breadcrumb segments from col_path (e.g. "a/b/c" → ["a", "a/b", "a/b/c"]).
    let crumbs: Vec<(String, String)> = {
        let mut acc = String::new();
        let mut v = vec![("⌂".to_string(), String::new())]; // root
        for seg in cp.split('/').filter(|s| !s.is_empty()) {
            if !acc.is_empty() { acc.push('/'); }
            acc.push_str(seg);
            v.push((seg.to_string(), acc.clone()));
        }
        v
    };

    let (left_root, left_dirs) = group_by_dir(&files, &cp);

    // Right-column contents: the open child directory.
    let oc = open_child.read().clone();
    let (right_root, right_dirs, right_base) = if let Some(ref child) = oc {
        let right_files: Vec<FileMeta> = left_dirs.iter()
            .find(|(p, _)| p == child)
            .map(|(_, f)| f.clone())
            .unwrap_or_default();
        let (rr, rd) = group_by_dir(&right_files, child);
        (rr, rd, child.clone())
    } else {
        (vec![], vec![], String::new())
    };

    rsx! {
        div { class: "col-view-wrap",
            // Breadcrumb bar
            div { class: "col-breadcrumb",
                for (label, path) in crumbs {
                    {
                        let p = path.clone();
                        let is_last = p == cp;
                        rsx! {
                            span {
                                key: "{p}-crumb",
                                class: if is_last { "col-crumb col-crumb--active" } else { "col-crumb" },
                                onclick: move |_| {
                                    col_path.set(p.clone());
                                    open_child.set(None);
                                    on_select_dir(p.clone());
                                },
                                "{label}"
                            }
                            if !is_last { span { class: "col-crumb-sep", "/" } }
                        }
                    }
                }
            }
            div { class: "col-view",
                // Left column
                div { class: "col-view-col col-view-left",
                    for (dir_prefix, _) in &left_dirs {
                        {
                            let dp = dir_prefix.clone();
                            let dp_drag = dir_prefix.clone();
                            let dp_drop = dir_prefix.clone();
                            let name = dir_prefix.rsplit('/').next().unwrap_or(dir_prefix).to_string();
                            let is_open = oc.as_deref() == Some(dir_prefix.as_str());
                            rsx! {
                                div {
                                    key: "{dir_prefix}",
                                    class: if is_open { "col-item col-item--dir col-item--open" } else { "col-item col-item--dir" },
                                    draggable: true,
                                    ondragstart: move |_| js::set_drag_data(dir_drag_payload(&dp_drag)),
                                    ondragover: move |e| e.prevent_default(),
                                    ondrop: move |_| {
                                        let dest = dp_drop.clone();
                                        spawn(async move {
                                            let data = js::get_drag_data().await;
                                            js::clear_drag_data();
                                            if !data.is_empty() { on_move((data, dest)); }
                                        });
                                    },
                                    onclick: move |_| {
                                        open_child.set(Some(dp.clone()));
                                        on_select_dir(dp.clone());
                                    },
                                    span { "📁 {name}" }
                                    span { class: "col-chevron", "›" }
                                }
                            }
                        }
                    }
                    for file in &left_root {
                        if file.name() != ".gitkeep" {
                            {
                                let p = file.path.clone();
                                let p_drag = file.path.clone();
                                let is_active = active().as_deref() == Some(p.as_str());
                                let fc = file.clone();
                                rsx! {
                                    div {
                                        key: "{p}",
                                        class: if is_active { "col-item col-item--active" } else { "col-item" },
                                        draggable: true,
                                        ondragstart: move |_| js::set_drag_data(file_drag_payload(&p_drag)),
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
                // Right column — only shown when a folder is open in the left
                if oc.is_some() {
                    div { class: "col-view-col col-view-right",
                        for (sub_prefix, _) in &right_dirs {
                            {
                                let sp = sub_prefix.clone();
                                let sp_drag = sub_prefix.clone();
                                let sp_drop = sub_prefix.clone();
                                let name = sub_prefix.rsplit('/').next().unwrap_or(sub_prefix).to_string();
                                // Clicking a subfolder in the right pane drills down:
                                // it becomes the new left column.
                                let sp_drill = sp.clone();
                                rsx! {
                                    div {
                                        key: "{sp}",
                                        class: "col-item col-item--dir",
                                        draggable: true,
                                        ondragstart: move |_| js::set_drag_data(dir_drag_payload(&sp_drag)),
                                        ondragover: move |e| e.prevent_default(),
                                        ondrop: move |_| {
                                            let dest = sp_drop.clone();
                                            spawn(async move {
                                                let data = js::get_drag_data().await;
                                                js::clear_drag_data();
                                                if !data.is_empty() { on_move((data, dest)); }
                                            });
                                        },
                                        onclick: move |_| {
                                            col_path.set(sp_drill.clone());
                                            open_child.set(None);
                                            on_select_dir(sp_drill.clone());
                                        },
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
                                    let p_drag = file.path.clone();
                                    let is_active = active().as_deref() == Some(p.as_str());
                                    let fc = file.clone();
                                    rsx! {
                                        div {
                                            key: "{p}",
                                            class: if is_active { "col-item col-item--active" } else { "col-item" },
                                            draggable: true,
                                            ondragstart: move |_| js::set_drag_data(file_drag_payload(&p_drag)),
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
}
