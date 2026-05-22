use dioxus::prelude::*;
use ui::{MarkdownArea, MarkdownAreaVariant};
use vault::{FileMeta, GithubConfig, SearchResult};

use crate::state;
use super::toolbar::FormattingToolbar;

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn sleep_ms(ms: u32) {
    let _ = document::eval(&format!(
        "await new Promise(r => setTimeout(r, {ms})); dioxus.send(1);"
    ))
    .join::<i32>()
    .await;
}

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
enum SaveStatus { Idle, Unsaved, Saving, Saved, Error(String) }

impl SaveStatus {
    fn label(&self) -> &str {
        match self {
            SaveStatus::Idle | SaveStatus::Saved => "Saved",
            SaveStatus::Unsaved => "Unsaved changes",
            SaveStatus::Saving  => "Saving…",
            SaveStatus::Error(_) => "Save failed",
        }
    }
    fn css_class(&self) -> &str {
        match self {
            SaveStatus::Error(_)  => "save-status save-status--error",
            SaveStatus::Unsaved   => "save-status save-status--unsaved",
            SaveStatus::Saving    => "save-status save-status--saving",
            _ => "save-status",
        }
    }
}

#[derive(Clone, PartialEq)]
enum Panel { Files, Search, Bookmarks }

// ── VaultBrowser ──────────────────────────────────────────────────────────────

#[component]
pub fn VaultBrowser(config: GithubConfig, on_logout: EventHandler<()>) -> Element {
    let mut files: Signal<Vec<FileMeta>> = use_signal(Vec::new);
    let mut active_path: Signal<Option<String>> = use_signal(|| None);
    let content = use_signal(String::new);
    let mut file_sha: Signal<String> = use_signal(String::new);
    let mut load_error: Signal<Option<String>> = use_signal(|| None);
    let mut loading_list = use_signal(|| true);
    let mut loading_file = use_signal(|| false);
    let mut save_status: Signal<SaveStatus> = use_signal(|| SaveStatus::Idle);
    let mut saved_content: Signal<String> = use_signal(String::new);
    let mut panel: Signal<Panel> = use_signal(|| Panel::Files);
    let mut bookmarks: Signal<Vec<String>> = use_signal(Vec::new);
    let mut show_switcher = use_signal(|| false);
    let mut show_new_file = use_signal(|| false);
    // Signal-based result channel: NewFileModal sets this, use_effect acts on it.
    let mut new_file_result: Signal<Option<String>> = use_signal(|| None);

    // Load file list and bookmarks on mount.
    let cfg = config.clone();
    use_effect(move || {
        let cfg = cfg.clone();
        spawn(async move {
            let file_result = vault::github::list_files(&cfg).await;
            let bm = state::load_bookmarks().await;
            match file_result {
                Ok(mut list) => { list.sort_by(|a, b| a.path.cmp(&b.path)); files.set(list); }
                Err(e) => load_error.set(Some(e.to_string())),
            }
            bookmarks.set(bm);
            loading_list.set(false);
        });
    });

    // Load file content when active_path changes.
    let cfg = config.clone();
    use_effect(move || {
        let path = active_path.read().clone();
        let Some(p) = path else { return };
        loading_file.set(true);
        save_status.set(SaveStatus::Idle);
        let cfg = cfg.clone();
        let mut content = content.clone();
        spawn(async move {
            match vault::github::read_file(&cfg, &p).await {
                Ok(fc) => {
                    content.set(fc.content.clone());
                    saved_content.set(fc.content);
                    file_sha.set(fc.sha);
                }
                Err(e) => load_error.set(Some(e.to_string())),
            }
            loading_file.set(false);
        });
    });

    // Mark unsaved when content diverges from last save.
    use_effect(move || {
        let current = content();
        if !loading_file() && !current.is_empty() && current != saved_content() {
            save_status.set(SaveStatus::Unsaved);
        }
    });

    // Auto-save every 2 seconds when unsaved.
    let cfg = config.clone();
    use_effect(move || {
        let cfg = cfg.clone();
        spawn(async move {
            loop {
                sleep_ms(2000).await;
                if save_status() != SaveStatus::Unsaved { continue; }
                let Some(path) = active_path() else { continue };
                let sha = file_sha();
                let current = content();
                if sha.is_empty() || current == saved_content() { continue; }
                save_status.set(SaveStatus::Saving);
                let name = path.rsplit('/').next().unwrap_or(&path).to_string();
                match vault::github::write_file(&cfg, &path, &current, &sha, &format!("Update {name}")).await {
                    Ok(new_sha) => { file_sha.set(new_sha); saved_content.set(current); save_status.set(SaveStatus::Saved); }
                    Err(e) => save_status.set(SaveStatus::Error(e.to_string())),
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
            if let Ok(mut list) = vault::github::list_files(&cfg).await {
                list.sort_by(|a, b| a.path.cmp(&b.path));
                files.set(list);
            }
            active_path.set(Some(path));
            show_switcher.set(false);
        });
    });

    // Pre-compute values that can't borrow across rsx!.
    let status_class = save_status.read().css_class().to_string();
    let status_label = save_status.read().label().to_string();
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

    rsx! {
        div { class: "app-layout",

            // ── Sidebar ─────────────────────────────────────────────────────
            aside { class: "sidebar",
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
                            title: "Today's note",
                            onclick: move |_| {
                                let cfg = cfg_daily.clone();
                                spawn(async move {
                                    let date = document::eval(
                                        "dioxus.send(new Date().toISOString().split('T')[0]);"
                                    ).join::<String>().await.unwrap_or_default();
                                    if date.is_empty() { return; }
                                    let path = format!("{date}.md");
                                    let _ = vault::github::create_file(
                                        &cfg, &path,
                                        &format!("# {date}\n\n"),
                                        &format!("Daily note {date}"),
                                    ).await;
                                    if let Ok(mut list) = vault::github::list_files(&cfg).await {
                                        list.sort_by(|a, b| a.path.cmp(&b.path));
                                        files.set(list);
                                    }
                                    active_path.set(Some(path));
                                    show_switcher.set(false);
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
                    }
                }

                div { class: "panel-tabs",
                    button {
                        class: if panel() == Panel::Files { "panel-tab panel-tab--active" } else { "panel-tab" },
                        onclick: move |_| panel.set(Panel::Files), title: "Files", "📁"
                    }
                    button {
                        class: if panel() == Panel::Search { "panel-tab panel-tab--active" } else { "panel-tab" },
                        onclick: move |_| panel.set(Panel::Search), title: "Search", "🔍"
                    }
                    button {
                        class: if panel() == Panel::Bookmarks { "panel-tab panel-tab--active" } else { "panel-tab" },
                        onclick: move |_| panel.set(Panel::Bookmarks), title: "Bookmarks", "🔖"
                    }
                }

                div { class: "panel-content",
                    match panel() {
                        Panel::Files => rsx! {
                            if loading_list() {
                                div { class: "sidebar-status", "Loading…" }
                            } else if let Some(err) = load_error() {
                                div { class: "sidebar-error", "{err}" }
                            } else if files.read().is_empty() {
                                div { class: "sidebar-status", "No markdown files found." }
                            } else {
                                FileTree {
                                    files: files.read().clone(),
                                    active: active_path.read().clone(),
                                    on_select: move |path: String| {
                                        active_path.set(Some(path));
                                        show_switcher.set(false);
                                    },
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
                                },
                            }
                        },
                        Panel::Bookmarks => rsx! {
                            BookmarksPanel {
                                bookmarks: bookmarks.read().clone(),
                                active: active_path.read().clone(),
                                on_select: move |path: String| {
                                    active_path.set(Some(path));
                                    show_switcher.set(false);
                                },
                                on_remove: move |path: String| {
                                    bookmarks.with_mut(|bm| bm.retain(|p| p != &path));
                                    state::save_bookmarks(&bookmarks.read());
                                },
                            }
                        },
                    }
                }
            }

            // ── Editor pane ─────────────────────────────────────────────────
            main { class: "editor-pane",
                if let Some(ref path) = active_path() {
                    div { class: "editor-titlebar",
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
                            }
                        }
                    }
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

            // ── New file modal ───────────────────────────────────────────────
            if show_new_file() {
                NewFileModal {
                    config: cfg_newfile,
                    result: new_file_result,
                    on_close: move |_| show_new_file.set(false),
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
    on_close: EventHandler<()>,
) -> Element {
    let mut name = use_signal(String::new);
    let mut creating = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    // Signal used to fire the create action from both onkeydown and onclick
    // without needing to move a non-Copy closure into two places.
    let mut trigger = use_signal(|| false);

    use_effect(move || {
        if !trigger() { return; }
        trigger.set(false);
        let raw = name.read().trim().to_string();
        if raw.is_empty() { error.set(Some("Enter a file name.".into())); return; }
        let path = if raw.ends_with(".md") { raw } else { format!("{raw}.md") };
        let title = path.trim_end_matches(".md").to_string();
        let cfg = config.clone();
        creating.set(true);
        error.set(None);
        spawn(async move {
            match vault::github::create_file(
                &cfg, &path, &format!("# {title}\n\n"), &format!("Create {path}")
            ).await {
                Ok(_)  => result.set(Some(path)),
                Err(e) => { error.set(Some(e.to_string())); creating.set(false); }
            }
        });
    });

    rsx! {
        div {
            class: "qs-overlay",
            onclick: move |_| on_close(()),
            div {
                class: "qs-modal", style: "max-width: 400px;",
                onclick: move |e| e.stop_propagation(),
                div { style: "padding: 16px 16px 8px; font-weight: 600;", "New note" }
                input {
                    class: "qs-input",
                    placeholder: "note-name  (or  folder/note-name)",
                    autofocus: true,
                    value: "{name}",
                    oninput: move |e| name.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter  { trigger.set(true); }
                        if e.key() == Key::Escape { on_close(()); }
                    },
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

// ── File tree ─────────────────────────────────────────────────────────────────

#[component]
fn FileTree(files: Vec<FileMeta>, active: Option<String>, on_select: EventHandler<String>) -> Element {
    let mut root: Vec<&FileMeta> = Vec::new();
    let mut dirs: Vec<(&str, Vec<&FileMeta>)> = Vec::new();
    for file in &files {
        let dir = file.dir();
        if dir.is_empty() { root.push(file); }
        else {
            let top = dir.splitn(2, '/').next().unwrap_or(dir);
            if let Some(g) = dirs.iter_mut().find(|(d, _)| *d == top) { g.1.push(file); }
            else { dirs.push((top, vec![file])); }
        }
    }
    rsx! {
        div { class: "file-tree",
            for file in root {
                FileEntry { key: "{file.path}", file: file.clone(), active: active.as_deref() == Some(file.path.as_str()), on_select }
            }
            for (dir, dir_files) in dirs {
                div { class: "file-tree-dir",
                    div { class: "file-tree-dir-name", "📁 {dir}" }
                    for file in dir_files {
                        FileEntry { key: "{file.path}", file: file.clone(), active: active.as_deref() == Some(file.path.as_str()), on_select }
                    }
                }
            }
        }
    }
}

#[component]
fn FileEntry(file: FileMeta, active: bool, on_select: EventHandler<String>) -> Element {
    let path = file.path.clone();
    rsx! {
        div {
            class: if active { "file-entry file-entry--active" } else { "file-entry" },
            onclick: move |_| on_select(path.clone()),
            "📄 {file.name()}"
        }
    }
}
