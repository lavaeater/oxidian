use dioxus::prelude::*;
use ui::MarkdownArea;
use vault::{FileMeta, GithubConfig};

use crate::state;

#[component]
pub fn VaultBrowser(config: GithubConfig, on_logout: EventHandler<()>) -> Element {
    let mut files: Signal<Vec<FileMeta>> = use_signal(Vec::new);
    let mut active_path: Signal<Option<String>> = use_signal(|| None);
    let content = use_signal(String::new);
    let mut load_error: Signal<Option<String>> = use_signal(|| None);
    let mut loading_list = use_signal(|| true);
    let mut loading_file = use_signal(|| false);

    // Load the file list once on mount.
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

    // Reload file content whenever active_path changes.
    let cfg = config.clone();
    use_effect(move || {
        let path = active_path.read().clone();
        let Some(p) = path else { return };
        loading_file.set(true);
        let cfg = cfg.clone();
        let mut content = content.clone();
        spawn(async move {
            match vault::github::read_file(&cfg, &p).await {
                Ok(fc) => content.set(fc.content),
                Err(e) => load_error.set(Some(e.to_string())),
            }
            loading_file.set(false);
        });
    });

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
                    div { class: "editor-filename", "{path}" }
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
