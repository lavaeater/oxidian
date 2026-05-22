use dioxus::prelude::*;
use ui::{MarkdownArea, MarkdownAreaVariant};
use vault::{FileMeta, GithubConfig};

use crate::state;

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn sleep_ms(ms: u32) {
    let _ = document::eval(&format!(
        "await new Promise(r => setTimeout(r, {ms})); dioxus.send(1);"
    ))
    .join::<i32>()
    .await;
}

/// True if every char of `needle` appears in order in `haystack`.
fn fuzzy_match(haystack: &str, needle: &str) -> bool {
    let mut it = haystack.chars();
    needle.chars().all(|nc| it.any(|hc| hc == nc))
}

/// Score a fuzzy match: higher = better. Used to rank quick-switcher results.
fn fuzzy_score(path: &str, needle: &str) -> usize {
    let name = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    let q = needle.to_lowercase();
    if name == q { return 1000; }
    if name.starts_with(&q) { return 500; }
    if name.contains(&q) { return 200; }
    // Count consecutive matching chars as a tiebreaker.
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

/// Parse heading lines (# / ## / ...) from markdown content.
fn extract_headings(content: &str) -> Vec<(u8, String)> {
    content
        .lines()
        .filter_map(|line| {
            let level = line.bytes().take_while(|&b| b == b'#').count();
            if level >= 1 && level <= 6 && line.as_bytes().get(level) == Some(&b' ') {
                Some((level as u8, line[level + 1..].trim().to_string()))
            } else {
                None
            }
        })
        .collect()
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

// ── Save status ───────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum SaveStatus {
    Idle,
    Unsaved,
    Saving,
    Saved,
    Error(String),
}

impl SaveStatus {
    fn label(&self) -> &str {
        match self {
            SaveStatus::Idle | SaveStatus::Saved => "Saved",
            SaveStatus::Unsaved => "Unsaved changes",
            SaveStatus::Saving => "Saving…",
            SaveStatus::Error(_) => "Save failed",
        }
    }
    fn css_class(&self) -> &str {
        match self {
            SaveStatus::Error(_) => "save-status save-status--error",
            SaveStatus::Unsaved => "save-status save-status--unsaved",
            SaveStatus::Saving => "save-status save-status--saving",
            _ => "save-status",
        }
    }
}

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
    let mut show_switcher = use_signal(|| false);

    // Load file list on mount.
    let cfg = config.clone();
    use_effect(move || {
        let cfg = cfg.clone();
        spawn(async move {
            match vault::github::list_files(&cfg).await {
                Ok(mut list) => {
                    list.sort_by(|a, b| a.path.cmp(&b.path));
                    files.set(list);
                }
                Err(e) => load_error.set(Some(e.to_string())),
            }
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

    // Mark unsaved when content diverges from the last save snapshot.
    use_effect(move || {
        let current = content();
        if !loading_file() && !current.is_empty() && current != saved_content() {
            save_status.set(SaveStatus::Unsaved);
        }
    });

    // Auto-save every 2 seconds when there are unsaved changes.
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
                    Ok(new_sha) => {
                        file_sha.set(new_sha);
                        saved_content.set(current);
                        save_status.set(SaveStatus::Saved);
                    }
                    Err(e) => save_status.set(SaveStatus::Error(e.to_string())),
                }
            }
        });
    });

    // TODO: global keyboard shortcuts (Ctrl+K for switcher, Escape to close)
    // Need a wasm-bindgen Closure or document-level capture before contenteditable
    // absorbs key events. Deferred — use the toolbar button for now.

    // Pre-compute values that can't borrow across rsx!.
    let status_class = save_status.read().css_class().to_string();
    let status_label = save_status.read().label().to_string();
    let status_title = match &*save_status.read() {
        SaveStatus::Error(e) => e.clone(),
        _ => String::new(),
    };
    let words = word_count(&content.read());
    let headings = extract_headings(&content.read());
    let has_file = active_path.read().is_some();

    rsx! {
        div { class: "app-layout",

            // ── Sidebar ─────────────────────────────────────────────────────
            aside { class: "sidebar",
                div { class: "sidebar-header",
                    span { class: "sidebar-title", "Oxidian" }
                    div { class: "sidebar-header-actions",
                        button {
                            class: "sidebar-icon-btn",
                            title: "Quick open (Ctrl+K)",
                            onclick: move |_| show_switcher.set(true),
                            "🔍"
                        }
                        button {
                            class: "sidebar-icon-btn",
                            title: "Disconnect vault",
                            onclick: move |_| { state::clear_config(); on_logout(()); },
                            "⚙"
                        }
                    }
                }

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
                        on_select: move |path: String| active_path.set(Some(path)),
                    }
                }

                // Outline pane — shown when a file with headings is open.
                if has_file && !headings.is_empty() {
                    OutlinePane { headings }
                }
            }

            // ── Editor pane ─────────────────────────────────────────────────
            main { class: "editor-pane",
                if let Some(ref path) = active_path() {
                    div { class: "editor-titlebar",
                        span { class: "editor-filename", "{path}" }
                        div { class: "editor-meta",
                            if loading_file() {
                                span { class: "save-status", "Loading…" }
                            } else {
                                span { class: "word-count", "{words} words" }
                                span {
                                    class: "{status_class}",
                                    title: "{status_title}",
                                    "{status_label}"
                                }
                            }
                        }
                    }
                    // Always keep MarkdownArea mounted so its CSS <link> stays in
                    // the document — unmounting it drops the stylesheet and markers
                    // become visible (unstyled raw markdown).
                    MarkdownArea {
                        content,
                        variant: MarkdownAreaVariant::Ghost,
                        placeholder: "Empty file.",
                    }
                } else {
                    div { class: "editor-empty",
                        p { "Select a file from the sidebar to start editing." }
                        p { class: "editor-empty-hint",
                            "Tip: use the 🔍 button to open the quick switcher."
                        }
                        p { class: "editor-empty-sub",
                            "Connected to "
                            strong { "{config.owner}/{config.repo}" }
                            " · "
                            code { "{config.branch}" }
                        }
                    }
                }
            }

            // ── Quick Switcher modal ─────────────────────────────────────────
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

    // Focus the input on mount (autofocus doesn't work on dynamic elements).
    use_effect(move || {
        document::eval(
            "requestAnimationFrame(() => { document.querySelector('.qs-input')?.focus(); });"
        );
    });

    // Pre-compute owned display data to avoid borrow-in-rsx issues.
    let q = query.read().to_lowercase();
    let first_path: Option<String>;
    let items: Vec<(String, String, String)> = {
        let mut v: Vec<_> = if q.is_empty() {
            files.iter().map(|f| (0usize, f)).take(8).collect()
        } else {
            let mut ranked: Vec<_> = files
                .iter()
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
                    class: "qs-input",
                    placeholder: "Go to file…",
                    autofocus: true,
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
                                if !dir.is_empty() {
                                    span { class: "qs-item-dir", "{dir}" }
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

// ── File tree ─────────────────────────────────────────────────────────────────

#[component]
fn FileTree(
    files: Vec<FileMeta>,
    active: Option<String>,
    on_select: EventHandler<String>,
) -> Element {
    let mut root: Vec<&FileMeta> = Vec::new();
    let mut dirs: Vec<(&str, Vec<&FileMeta>)> = Vec::new();

    for file in &files {
        let dir = file.dir();
        if dir.is_empty() {
            root.push(file);
        } else {
            let top = dir.splitn(2, '/').next().unwrap_or(dir);
            if let Some(group) = dirs.iter_mut().find(|(d, _)| *d == top) {
                group.1.push(file);
            } else {
                dirs.push((top, vec![file]));
            }
        }
    }

    rsx! {
        div { class: "file-tree",
            for file in root {
                FileEntry {
                    key: "{file.path}",
                    file: file.clone(),
                    active: active.as_deref() == Some(file.path.as_str()),
                    on_select,
                }
            }
            for (dir, dir_files) in dirs {
                div { class: "file-tree-dir",
                    div { class: "file-tree-dir-name", "📁 {dir}" }
                    for file in dir_files {
                        FileEntry {
                            key: "{file.path}",
                            file: file.clone(),
                            active: active.as_deref() == Some(file.path.as_str()),
                            on_select,
                        }
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
