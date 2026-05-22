use dioxus::prelude::*;
use ui::MarkdownArea;
use vault::{FileMeta, GithubConfig};

use crate::state;

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

// Sleep helper via JS setTimeout — works in WASM without extra deps.
async fn sleep_ms(ms: u32) {
    let _ = document::eval(&format!(
        "await new Promise(r => setTimeout(r, {ms})); dioxus.send(1);"
    ))
    .join::<i32>()
    .await;
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
    // Snapshot of content at last load or save — used to detect unsaved changes.
    let mut saved_content: Signal<String> = use_signal(String::new);

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

    // Mark unsaved whenever content diverges from the last save snapshot.
    use_effect(move || {
        let current = content();
        if !loading_file() && !current.is_empty() && current != saved_content() {
            save_status.set(SaveStatus::Unsaved);
        }
    });

    // Auto-save loop: every 2 seconds, if there are unsaved changes, commit.
    let cfg = config.clone();
    use_effect(move || {
        let cfg = cfg.clone();
        spawn(async move {
            loop {
                sleep_ms(2000).await;

                // Only save if there are actual unsaved changes.
                if save_status() != SaveStatus::Unsaved {
                    continue;
                }
                let Some(path) = active_path() else { continue };
                let sha = file_sha();
                let current = content();
                if sha.is_empty() || current == saved_content() {
                    continue;
                }

                save_status.set(SaveStatus::Saving);
                let name = path.rsplit('/').next().unwrap_or(&path).to_string();
                let message = format!("Update {name}");

                match vault::github::write_file(&cfg, &path, &current, &sha, &message).await {
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

    let status_class = save_status.read().css_class().to_string();
    let status_label = save_status.read().label().to_string();
    let status_title = match &*save_status.read() {
        SaveStatus::Error(e) => e.clone(),
        _ => String::new(),
    };

    rsx! {
        div { class: "app-layout",

            // ── Sidebar ─────────────────────────────────────────────────────
            aside { class: "sidebar",
                div { class: "sidebar-header",
                    span { class: "sidebar-title", "Oxidian" }
                    button {
                        class: "sidebar-settings-btn",
                        title: "Disconnect vault",
                        onclick: move |_| {
                            state::clear_config();
                            on_logout(());
                        },
                        "⚙"
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
            }

            // ── Editor pane ─────────────────────────────────────────────────
            main { class: "editor-pane",
                if let Some(ref path) = active_path() {
                    div { class: "editor-titlebar",
                        span { class: "editor-filename", "{path}" }
                        span {
                            class: "{status_class}",
                            title: "{status_title}",
                            "{status_label}"
                        }
                    }
                    if loading_file() {
                        div { class: "editor-loading", "Loading…" }
                    } else {
                        MarkdownArea { content, placeholder: "Empty file." }
                    }
                } else {
                    div { class: "editor-empty",
                        p { "Select a file from the sidebar to start editing." }
                        p { class: "editor-empty-sub",
                            "Connected to "
                            strong { "{config.owner}/{config.repo}" }
                            " · "
                            code { "{config.branch}" }
                        }
                    }
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
