use dioxus::prelude::*;
use vault::{FileMeta, GithubConfig};

use crate::console_log;

// ── KanbanBoard ───────────────────────────────────────────────────────────────

#[component]
pub fn KanbanBoard(
    config: GithubConfig,
    board_root: String,
    files: Vec<FileMeta>,
    on_open: EventHandler<String>,
    on_files_changed: EventHandler<Vec<FileMeta>>,
) -> Element {
    let mut moving = use_signal(|| false);
    let mut move_error: Signal<Option<String>> = use_signal(|| None);
    let mut new_col_name: Signal<String> = use_signal(String::new);
    let mut adding_col = use_signal(|| false);

    // Derive columns: each direct subfolder of board_root that contains .md files.
    let prefix = format!("{board_root}/");
    let mut columns: Vec<(String, Vec<FileMeta>)> = Vec::new();
    for file in &files {
        let Some(rest) = file.path.strip_prefix(&prefix) else { continue };
        // rest = "<column>/<filename>" — must be exactly one slash
        let Some(slash) = rest.find('/') else { continue };
        let col = &rest[..slash];
        let filename = &rest[slash + 1..];
        if filename.contains('/') { continue; }
        // Ensure the column entry exists (even for .gitkeep-only columns).
        if columns.iter().find(|(c, _)| c == col).is_none() {
            columns.push((col.to_string(), vec![]));
        }
        if filename == ".gitkeep" { continue; }
        if !filename.ends_with(".md") { continue; }
        if let Some(entry) = columns.iter_mut().find(|(c, _)| c == col) {
            entry.1.push(file.clone());
        }
    }
    for (_, cards) in &mut columns {
        cards.sort_by(|a, b| a.path.cmp(&b.path));
    }

    if columns.is_empty() {
        return rsx! {
            div { class: "kanban-empty",
                p { "No columns found in "" {board_root} ""." }
                p { "Create subfolders (e.g. " code { "{board_root}/Todo/" } ") and add .md files." }
            }
        };
    }

    rsx! {
        div { class: "kanban-board",
            if moving() {
                div { class: "kanban-moving", "Moving card…" }
            }
            if let Some(ref err) = move_error() {
                div { class: "kanban-error",
                    "{err}"
                    button {
                        class: "kanban-error-close",
                        onclick: move |_| move_error.set(None),
                        "✕"
                    }
                }
            }
            div { class: "kanban-columns",
                for (col_name, cards) in columns {
                    {
                        // Clone non-Copy values per column; signals are Copy.
                        let cfg = config.clone();
                        let board = board_root.clone();
                        let all_files = files.clone();
                        let dst_col = col_name.clone();
                        rsx! {
                            KanbanColumn {
                                key: "{col_name}",
                                name: col_name,
                                cards,
                                on_open,
                                on_drop: move |(src_col, filename): (String, String)| {
                                    if src_col == dst_col { return; }
                                    let cfg = cfg.clone();
                                    let board = board.clone();
                                    let all_files = all_files.clone();
                                    let dst = dst_col.clone();
                                    move_error.set(None);
                                    moving.set(true);
                                    spawn(async move {
                                        let src_path = format!("{board}/{src_col}/{filename}");
                                        let dst_path = format!("{board}/{dst}/{filename}");
                                        console_log(&format!("[kanban] move {src_path} → {dst_path}"));

                                        let sha = all_files.iter()
                                            .find(|f| f.path == src_path)
                                            .map(|f| f.sha.clone())
                                            .unwrap_or_default();

                                        let fc = match vault::dispatch::read_file(&cfg, &src_path).await {
                                            Ok(fc) => fc,
                                            Err(e) => { move_error.set(Some(format!("Read failed: {e}"))); moving.set(false); return; }
                                        };
                                        if let Err(e) = vault::dispatch::create_file(
                                            &cfg, &dst_path, &fc.content,
                                            &format!("Move {filename} to {dst}"),
                                        ).await {
                                            move_error.set(Some(format!("Create failed: {e}")));
                                            moving.set(false);
                                            return;
                                        }
                                        let del_sha = if sha.is_empty() { fc.sha } else { sha };
                                        if let Err(e) = vault::dispatch::delete_file(
                                            &cfg, &src_path, &del_sha,
                                            &format!("Move {filename} to {dst}"),
                                        ).await {
                                            move_error.set(Some(format!("Delete failed: {e}")));
                                            moving.set(false);
                                            return;
                                        }
                                        match vault::dispatch::list_files(&cfg).await {
                                            Ok(mut list) => {
                                                list.sort_by(|a, b| a.path.cmp(&b.path));
                                                on_files_changed(list);
                                            }
                                            Err(e) => move_error.set(Some(format!("Refresh failed: {e}"))),
                                        }
                                        moving.set(false);
                                    });
                                },
                            }
                        }
                    }
                }
                // ── New column button ──────────────────────────────────────
                {
                    let cfg = config.clone();
                    let board = board_root.clone();
                    rsx! {
                        div { class: "kanban-col kanban-col--new",
                            if adding_col() {
                                div { class: "kanban-col-header", "New column" }
                                div { class: "kanban-col-body",
                                    input {
                                        class: "kanban-col-input",
                                        placeholder: "Column name",
                                        autofocus: true,
                                        value: "{new_col_name}",
                                        oninput: move |e| new_col_name.set(e.value()),
                                        onkeydown: move |e| {
                                            if e.key() == Key::Escape {
                                                adding_col.set(false);
                                                new_col_name.set(String::new());
                                            }
                                            if e.key() == Key::Enter {
                                                let name = new_col_name.read().trim().to_string();
                                                if name.is_empty() { return; }
                                                let cfg = cfg.clone();
                                                let board = board.clone();
                                                adding_col.set(false);
                                                new_col_name.set(String::new());
                                                move_error.set(None);
                                                spawn(async move {
                                                    // GitHub doesn't support empty dirs; create a .gitkeep placeholder.
                                                    let path = format!("{board}/{name}/.gitkeep");
                                                    match vault::dispatch::create_file(
                                                        &cfg, &path, "",
                                                        &format!("Create column {name}"),
                                                    ).await {
                                                        Ok(_) => {
                                                            if let Ok(mut list) = vault::dispatch::list_files(&cfg).await {
                                                                list.sort_by(|a, b| a.path.cmp(&b.path));
                                                                on_files_changed(list);
                                                            }
                                                        }
                                                        Err(e) => move_error.set(Some(format!("Create column failed: {e}"))),
                                                    }
                                                });
                                            }
                                        },
                                    }
                                    div { class: "kanban-col-new-hint", "Press Enter to create · Esc to cancel" }
                                }
                            } else {
                                button {
                                    class: "kanban-add-col-btn",
                                    onclick: move |_| adding_col.set(true),
                                    "+ New column"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── KanbanColumn ──────────────────────────────────────────────────────────────

#[component]
fn KanbanColumn(
    name: String,
    cards: Vec<FileMeta>,
    on_open: EventHandler<String>,
    on_drop: EventHandler<(String, String)>,
) -> Element {
    let mut drag_over = use_signal(|| false);

    rsx! {
        div {
            class: if drag_over() { "kanban-col kanban-col--over" } else { "kanban-col" },
            ondragover: move |e| { e.prevent_default(); drag_over.set(true); },
            ondragleave: move |_| drag_over.set(false),
            ondrop: move |e| {
                e.prevent_default();
                drag_over.set(false);
                // Read drag data stored in window.__oxidianDragData during dragstart.
                spawn(async move {
                    let mut ev = document::eval("dioxus.send(window.__oxidianDragData || '');");
                    let data = ev.recv::<String>().await.unwrap_or_default();
                    if data.is_empty() { return; }
                    // data = "src_col\x1efilename"
                    if let Some((src, fname)) = data.split_once('\x1e') {
                        on_drop((src.to_string(), fname.to_string()));
                    }
                });
                document::eval("window.__oxidianDragData = '';");
            },
            div { class: "kanban-col-header",
                "{name}"
                span { class: "kanban-col-count", "{cards.len()}" }
            }
            div { class: "kanban-col-body",
                for card in cards {
                    {
                        let path = card.path.clone();
                        let col_name = name.clone();
                        let filename = card.path.rsplit('/').next().unwrap_or(&card.path).to_string();
                        let title = filename.trim_end_matches(".md").to_string();
                        let drag_col = col_name.clone();
                        let drag_filename = filename.clone();
                        rsx! {
                            div {
                                class: "kanban-card",
                                draggable: true,
                                ondragstart: move |_| {
                                    let data = format!("{}\x1e{}", drag_col, drag_filename);
                                    document::eval(&format!(
                                        "window.__oxidianDragData = {};",
                                        serde_json::to_string(&data).unwrap_or_default()
                                    ));
                                },
                                onclick: move |_| on_open(path.clone()),
                                "{title}"
                            }
                        }
                    }
                }
            }
        }
    }
}
