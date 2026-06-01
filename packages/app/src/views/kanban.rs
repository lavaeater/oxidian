use dioxus::prelude::*;
use vault::{FileMeta, GithubConfig, VaultError};

use crate::console_log;

// ── Board document model ────────────────────────────────────────────────────
//
// A Kanban board *is* a markdown document (e.g. `kanban/kanban.md`). It owns the
// board structure and ordering so the individual note files stay clean:
//
//     ---
//     kanban-plugin: board
//     ---
//
//     ## Todo
//
//     - [[redesign-homepage]]
//     - [[fix-login-bug]]
//
//     ## Doing
//
//     - [[write-tests]]
//
// Columns are `## headings` (in order); cards are `- [[Title]]` items (in order).
// Each card's note lives at `<board-dir>/<Column>/<Title>.md`. Moving a card
// rewrites both the board doc (ordering) and moves the underlying file between
// the column folders.

const DEFAULT_PREAMBLE: &str = "---\nkanban-plugin: board\n---";

/// Parse a board document into `(preamble, columns)`.
/// `preamble` is everything before the first `## heading` (frontmatter, title…)
/// and is preserved verbatim across edits.
fn parse_board(content: &str) -> (String, Vec<(String, Vec<String>)>) {
    let lines: Vec<&str> = content.lines().collect();
    let Some(start) = lines.iter().position(|l| l.starts_with("## ")) else {
        return (content.trim_end().to_string(), Vec::new());
    };
    let preamble = lines[..start].join("\n");
    let mut columns: Vec<(String, Vec<String>)> = Vec::new();
    for line in &lines[start..] {
        if let Some(name) = line.strip_prefix("## ") {
            columns.push((name.trim().to_string(), Vec::new()));
        } else if let Some(item) = line.trim_start().strip_prefix("- ") {
            let item = item.trim();
            let title = item
                .strip_prefix("[[")
                .and_then(|s| s.strip_suffix("]]"))
                .unwrap_or(item)
                .trim()
                .to_string();
            if !title.is_empty() {
                if let Some(col) = columns.last_mut() {
                    col.1.push(title);
                }
            }
        }
    }
    (preamble, columns)
}

/// Serialise the board model back to markdown, preserving the preamble.
fn serialize_board(preamble: &str, columns: &[(String, Vec<String>)]) -> String {
    let mut out = String::new();
    let pre = preamble.trim_end();
    if !pre.is_empty() {
        out.push_str(pre);
        out.push_str("\n\n");
    }
    for (name, cards) in columns {
        out.push_str("## ");
        out.push_str(name);
        out.push_str("\n\n");
        for c in cards {
            out.push_str("- [[");
            out.push_str(c);
            out.push_str("]]\n");
        }
        out.push('\n');
    }
    out
}

/// Build initial columns by scanning the vault for existing subfolders of the
/// board directory that already contain notes. Used when first creating the
/// board doc so existing folder structures are picked up automatically.
fn import_columns(files: &[FileMeta], board_dir: &str, board_path: &str) -> Vec<(String, Vec<String>)> {
    let prefix = if board_dir.is_empty() { String::new() } else { format!("{board_dir}/") };
    let mut columns: Vec<(String, Vec<String>)> = Vec::new();
    let mut sorted: Vec<&FileMeta> = files.iter().collect();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    for file in sorted {
        if file.path == board_path { continue; }
        let rest = if prefix.is_empty() { file.path.as_str() } else {
            match file.path.strip_prefix(&prefix) { Some(r) => r, None => continue }
        };
        let Some(slash) = rest.find('/') else { continue };
        let col = &rest[..slash];
        let filename = &rest[slash + 1..];
        if filename.contains('/') || filename == ".gitkeep" || !filename.ends_with(".md") { continue; }
        let title = filename.trim_end_matches(".md").to_string();
        if let Some(entry) = columns.iter_mut().find(|(c, _)| c == col) {
            entry.1.push(title);
        } else {
            columns.push((col.to_string(), vec![title]));
        }
    }
    columns
}

fn join(dir: &str, rest: &str) -> String {
    if dir.is_empty() { rest.to_string() } else { format!("{dir}/{rest}") }
}

// ── KanbanBoard ───────────────────────────────────────────────────────────────

#[component]
pub fn KanbanBoard(
    config: GithubConfig,
    /// Path to the board's markdown document, e.g. `kanban/kanban.md`.
    board_path: String,
    files: Vec<FileMeta>,
    on_open: EventHandler<String>,
    on_files_changed: EventHandler<Vec<FileMeta>>,
) -> Element {
    let mut preamble: Signal<String> = use_signal(|| DEFAULT_PREAMBLE.to_string());
    let mut columns: Signal<Vec<(String, Vec<String>)>> = use_signal(Vec::new);
    let mut board_sha: Signal<String> = use_signal(String::new);
    let mut loading = use_signal(|| true);
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut busy = use_signal(|| false);
    let mut adding_col = use_signal(|| false);
    let mut new_col_name: Signal<String> = use_signal(String::new);

    let board_dir = board_path.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();

    // Load (or create) the board document on mount. The parent gives this
    // component a `key` of the board path, so changing boards remounts and
    // re-runs this effect.
    {
        let cfg = config.clone();
        let bp = board_path.clone();
        let dir = board_dir.clone();
        let files = files.clone();
        use_effect(move || {
            let cfg = cfg.clone();
            let bp = bp.clone();
            let dir = dir.clone();
            let files = files.clone();
            spawn(async move {
                loading.set(true);
                error.set(None);
                match vault::dispatch::read_file(&cfg, &bp).await {
                    Ok(fc) => {
                        let (pre, cols) = parse_board(&fc.content);
                        preamble.set(pre);
                        columns.set(cols);
                        board_sha.set(fc.sha);
                    }
                    Err(VaultError::NotFound(_)) => {
                        // No board doc yet — bootstrap one from existing subfolders.
                        let cols = import_columns(&files, &dir, &bp);
                        let pre = DEFAULT_PREAMBLE.to_string();
                        let content = serialize_board(&pre, &cols);
                        match vault::dispatch::create_file(&cfg, &bp, &content, "Create kanban board").await {
                            Ok(sha) => {
                                preamble.set(pre);
                                columns.set(cols);
                                board_sha.set(sha);
                                if let Ok(mut l) = vault::dispatch::list_files(&cfg).await {
                                    l.sort_by(|a, b| a.path.cmp(&b.path));
                                    on_files_changed(l);
                                }
                            }
                            Err(e) => error.set(Some(format!("Could not create board: {e}"))),
                        }
                    }
                    Err(e) => error.set(Some(e.to_string())),
                }
                loading.set(false);
            });
        });
    }

    // ── Persist helper: write the current model back to the board doc. ──
    // Returns nothing; updates board_sha / error. Callers update `columns`
    // first (optimistically) then call this inside their spawn.
    let persist = move |cfg: GithubConfig, bp: String| async move {
        let content = serialize_board(&preamble.peek(), &columns.peek());
        let sha = board_sha.peek().clone();
        match vault::dispatch::write_file(&cfg, &bp, &content, &sha, "Update kanban board").await {
            Ok(new_sha) => { board_sha.set(new_sha); true }
            Err(e) => { error.set(Some(format!("Board save failed: {e}"))); false }
        }
    };

    if loading() {
        return rsx! { div { class: "kanban-empty", "Loading board…" } };
    }

    let cols_now = columns();

    rsx! {
        div { class: "kanban-board",
            if busy() {
                div { class: "kanban-moving", "Working…" }
            }
            if let Some(ref err) = error() {
                div { class: "kanban-error",
                    "{err}"
                    button { class: "kanban-error-close", onclick: move |_| error.set(None), "✕" }
                }
            }
            div { class: "kanban-columns",
                for (idx, (col_name, cards)) in cols_now.iter().enumerate() {
                    {
                        let col = col_name.clone();
                        let cards = cards.clone();
                        let cfg_drop = config.clone();
                        let bp_drop = board_path.clone();
                        let dir_drop = board_dir.clone();
                        let col_drop = col.clone();
                        let cfg_add = config.clone();
                        let bp_add = board_path.clone();
                        let dir_add = board_dir.clone();
                        let col_add = col.clone();
                        let dir_open = board_dir.clone();
                        let col_open = col.clone();
                        rsx! {
                            KanbanColumn {
                                key: "{col}-{idx}",
                                name: col.clone(),
                                cards,
                                on_open: move |title: String| {
                                    on_open(join(&dir_open, &format!("{col_open}/{title}.md")));
                                },
                                on_add_card: move |title: String| {
                                    let title = title.trim().to_string();
                                    if title.is_empty() { return; }
                                    let cfg = cfg_add.clone();
                                    let bp = bp_add.clone();
                                    let path = join(&dir_add, &format!("{col_add}/{title}.md"));
                                    let col_add = col_add.clone();
                                    busy.set(true);
                                    error.set(None);
                                    spawn(async move {
                                        match vault::dispatch::create_file(
                                            &cfg, &path, &format!("# {title}\n\n"),
                                            &format!("Add {title} to {col_add}"),
                                        ).await {
                                            Ok(_) => {
                                                columns.with_mut(|cols| {
                                                    if let Some(c) = cols.iter_mut().find(|(n, _)| *n == col_add) {
                                                        c.1.push(title.clone());
                                                    }
                                                });
                                                persist(cfg.clone(), bp).await;
                                                if let Ok(mut l) = vault::dispatch::list_files(&cfg).await {
                                                    l.sort_by(|a, b| a.path.cmp(&b.path));
                                                    on_files_changed(l);
                                                }
                                            }
                                            Err(e) => error.set(Some(format!("Add card failed: {e}"))),
                                        }
                                        busy.set(false);
                                    });
                                },
                                on_drop: move |(src_col, title): (String, String)| {
                                    if src_col == col_drop { return; }
                                    let cfg = cfg_drop.clone();
                                    let bp = bp_drop.clone();
                                    let dir = dir_drop.clone();
                                    let dst = col_drop.clone();
                                    busy.set(true);
                                    error.set(None);
                                    spawn(async move {
                                        let src_path = join(&dir, &format!("{src_col}/{title}.md"));
                                        let dst_path = join(&dir, &format!("{dst}/{title}.md"));
                                        console_log(&format!("[kanban] move {src_path} → {dst_path}"));
                                        // Move the underlying note between column folders.
                                        match vault::dispatch::read_file(&cfg, &src_path).await {
                                            Ok(fc) => {
                                                if let Err(e) = vault::dispatch::create_file(
                                                    &cfg, &dst_path, &fc.content,
                                                    &format!("Move {title} to {dst}"),
                                                ).await {
                                                    error.set(Some(format!("Create failed: {e}")));
                                                    busy.set(false);
                                                    return;
                                                }
                                                if let Err(e) = vault::dispatch::delete_file(
                                                    &cfg, &src_path, &fc.sha,
                                                    &format!("Move {title} to {dst}"),
                                                ).await {
                                                    error.set(Some(format!("Delete failed: {e}")));
                                                    busy.set(false);
                                                    return;
                                                }
                                            }
                                            Err(e) => {
                                                error.set(Some(format!("Read failed: {e}")));
                                                busy.set(false);
                                                return;
                                            }
                                        }
                                        // Update board ordering: remove from src, append to dst.
                                        columns.with_mut(|cols| {
                                            if let Some(c) = cols.iter_mut().find(|(n, _)| *n == src_col) {
                                                c.1.retain(|t| t != &title);
                                            }
                                            if let Some(c) = cols.iter_mut().find(|(n, _)| *n == dst) {
                                                if !c.1.contains(&title) { c.1.push(title.clone()); }
                                            }
                                        });
                                        persist(cfg.clone(), bp).await;
                                        if let Ok(mut l) = vault::dispatch::list_files(&cfg).await {
                                            l.sort_by(|a, b| a.path.cmp(&b.path));
                                            on_files_changed(l);
                                        }
                                        busy.set(false);
                                    });
                                },
                            }
                        }
                    }
                }

                // ── New column ──────────────────────────────────────────────
                {
                    let cfg = config.clone();
                    let bp = board_path.clone();
                    let dir = board_dir.clone();
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
                                                if columns.read().iter().any(|(n, _)| n == &name) {
                                                    error.set(Some(format!("Column \"{name}\" already exists.")));
                                                    return;
                                                }
                                                let cfg = cfg.clone();
                                                let bp = bp.clone();
                                                let keep = join(&dir, &format!("{name}/.gitkeep"));
                                                adding_col.set(false);
                                                new_col_name.set(String::new());
                                                error.set(None);
                                                busy.set(true);
                                                spawn(async move {
                                                    // Add the column to the board model + a .gitkeep so
                                                    // the (otherwise empty) folder exists in git.
                                                    columns.with_mut(|cols| cols.push((name.clone(), vec![])));
                                                    let _ = vault::dispatch::create_file(
                                                        &cfg, &keep, "", &format!("Create column {name}"),
                                                    ).await;
                                                    persist(cfg.clone(), bp).await;
                                                    if let Ok(mut l) = vault::dispatch::list_files(&cfg).await {
                                                        l.sort_by(|a, b| a.path.cmp(&b.path));
                                                        on_files_changed(l);
                                                    }
                                                    busy.set(false);
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
    cards: Vec<String>,
    on_open: EventHandler<String>,
    on_add_card: EventHandler<String>,
    on_drop: EventHandler<(String, String)>,
) -> Element {
    let mut drag_over = use_signal(|| false);
    let mut adding = use_signal(|| false);
    let mut new_card: Signal<String> = use_signal(String::new);

    rsx! {
        div {
            class: if drag_over() { "kanban-col kanban-col--over" } else { "kanban-col" },
            ondragover: move |e| { e.prevent_default(); drag_over.set(true); },
            ondragleave: move |_| drag_over.set(false),
            ondrop: move |e| {
                e.prevent_default();
                drag_over.set(false);
                spawn(async move {
                    let mut ev = document::eval("dioxus.send(window.__oxidianDragData || '');");
                    let data = ev.recv::<String>().await.unwrap_or_default();
                    if data.is_empty() { return; }
                    // data = "src_col\x1ecard_title"
                    if let Some((src, title)) = data.split_once('\x1e') {
                        on_drop((src.to_string(), title.to_string()));
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
                        let title = card.clone();
                        let drag_col = name.clone();
                        let drag_title = card.clone();
                        rsx! {
                            div {
                                class: "kanban-card",
                                draggable: true,
                                ondragstart: move |_| {
                                    let data = format!("{}\x1e{}", drag_col, drag_title);
                                    document::eval(&format!(
                                        "window.__oxidianDragData = {};",
                                        serde_json::to_string(&data).unwrap_or_default()
                                    ));
                                },
                                onclick: move |_| on_open(title.clone()),
                                "{card}"
                            }
                        }
                    }
                }
                if adding() {
                    input {
                        class: "kanban-col-input",
                        placeholder: "Card title",
                        autofocus: true,
                        value: "{new_card}",
                        oninput: move |e| new_card.set(e.value()),
                        onkeydown: move |e| {
                            if e.key() == Key::Escape { adding.set(false); new_card.set(String::new()); }
                            if e.key() == Key::Enter {
                                let t = new_card.read().trim().to_string();
                                if !t.is_empty() { on_add_card(t); }
                                adding.set(false);
                                new_card.set(String::new());
                            }
                        },
                    }
                } else {
                    button {
                        class: "kanban-add-card-btn",
                        onclick: move |_| adding.set(true),
                        "+ Add card"
                    }
                }
            }
        }
    }
}
