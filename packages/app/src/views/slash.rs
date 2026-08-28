use dioxus::prelude::*;
use crate::template::TemplateMeta;

// The slash-query read and the `/query` → snippet replacement now live in
// `assets/oxidian.js`, bound as `js::slash_query()` / `js::apply_slash()`.

// ── Slash commands catalogue ──────────────────────────────────────────────────

const COMMANDS: &[(&str, &str, &str)] = &[
    // (name, description, insert-text)
    ("Heading 1",  "# Large heading",     "# "),
    ("Heading 2",  "## Medium heading",   "## "),
    ("Heading 3",  "### Small heading",   "### "),
    ("Bold",       "**bold** text",       "****"),
    ("Italic",     "*italic* text",       "**"),
    ("Bullet",     "- Bullet list",       "- "),
    ("Numbered",   "1. Numbered list",    "1. "),
    ("Task",       "- [ ] Task item",     "- [ ] "),
    // Bullet Journal signifiers (docs/bujo-roadmap.md §3). Ctrl/⌘-Enter cycles
    // these on an existing line; these are for starting one outright.
    ("Event",      "- [o] Something that happened", "- [o] "),
    ("Note",       "- A note, not a task",          "- "),
    ("Migrated",   "- [>] Moved to the next log",   "- [>] "),
    ("Scheduled",  "- [<] Deferred to a date",      "- [<] "),
    ("Dropped",    "- [-] Consciously abandoned",   "- [-] "),
    ("Quote",      "> Blockquote",        "> "),
    ("Code block", "``` code fence",      "```\n\n```"),
    ("Table",      "Markdown table",      "| Col 1 | Col 2 |\n| --- | --- |\n| | |"),
    ("Divider",    "Horizontal rule",     "\n---\n"),
    ("WikiLink",   "[[link to note]]",    "[[]]"),
    // Task metadata (Obsidian-Tasks emoji). `{{today}}` / `{{tomorrow}}` are
    // substituted with the real date by the editor's on_select handler.
    ("Due today",       "📅 today's date",      "📅 {{today}} "),
    ("Due tomorrow",    "📅 tomorrow's date",   "📅 {{tomorrow}} "),
    ("Due date",        "📅 (then type date)",  "📅 "),
    ("Priority urgent", "🔺 highest priority",  "🔺 "),
    ("Priority high",   "⏫ high priority",      "⏫ "),
    ("Priority medium", "🔼 medium priority",   "🔼 "),
    ("Priority low",    "🔽 low priority",      "🔽 "),
    ("Priority lowest", "⏬ lowest priority",   "⏬ "),
    ("Done today",      "✅ mark done today",   "✅ {{today}} "),
];

// ── Component ─────────────────────────────────────────────────────────────────

/// `on_select(insert_text)` for built-in commands; `on_template(meta)` for templates.
#[component]
pub fn SlashMenu(
    query: String,
    templates: Vec<TemplateMeta>,
    on_select: EventHandler<String>,
    on_template: EventHandler<TemplateMeta>,
    on_close: EventHandler<()>,
) -> Element {
    let q = query.to_lowercase();
    let cmds: Vec<(String, String, String)> = COMMANDS.iter()
        .filter(|(name, _, _)| q.is_empty() || name.to_lowercase().contains(&q))
        .map(|(n, d, i)| (n.to_string(), d.to_string(), i.to_string()))
        .take(8)
        .collect();
    let tmpls: Vec<TemplateMeta> = templates.into_iter()
        .filter(|t| q.is_empty() || t.name.to_lowercase().contains(&q) || q.contains("template"))
        .take(5)
        .collect();

    if cmds.is_empty() && tmpls.is_empty() { return rsx! { div {} }; }

    rsx! {
        div {
            class: "slash-overlay",
            onclick: move |_| on_close(()),
            div {
                class: "slash-menu",
                onclick: move |e| e.stop_propagation(),
                for (name, desc, insert) in cmds {
                    div {
                        class: "slash-item",
                        // Without this, clicking the item steals focus/collapses
                        // the editor's Selection before on_select runs, so
                        // apply_slash has nowhere to insert into. Same pattern as
                        // the formatting toolbar (`views::toolbar`).
                        onmousedown: move |e| e.prevent_default(),
                        onclick: move |_| on_select(insert.clone()),
                        span { class: "slash-name", "{name}" }
                        span { class: "slash-desc", "{desc}" }
                    }
                }
                for tmpl in tmpls {
                    {
                        let t = tmpl.clone();
                        let name = tmpl.name.clone();
                        let kind = if tmpl.filepath.is_some() { "→ new note" } else { "insert" };
                        rsx! {
                            div {
                                class: "slash-item slash-item--template",
                                onmousedown: move |e| e.prevent_default(),
                                onclick: move |_| on_template(t.clone()),
                                span { class: "slash-name", "{name}" }
                                span { class: "slash-desc", "Template · {kind}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
