//! The space-triggered task-metadata menu (Obsidian-Tasks-style emoji entry).
//!
//! Shown while the caret sits at the end of a task line right after a space —
//! see `js::task_menu_armed()` and the arming logic in `ui`'s
//! `markdown_area.js`. Offers exactly the metadata `index::tasks` actually
//! parses: due date, priority, done-today.
//!
//! Space rather than Enter, because Enter also moves the caret to the next
//! line: the menu would offer to annotate a task you had already finished and
//! left. Space fires more often, which is the price of it firing at the moment
//! you can still act on it. Nothing is intercepted — keep typing and the menu
//! disarms on the next poll, so ignoring it never costs more than a keystroke.

use dioxus::prelude::*;
use ui::TaskDatePicker;

const PRIORITIES: &[(&str, &str, &str)] = &[
    // (name, emoji, insert-text)
    ("Highest priority", "🔺", "🔺 "),
    ("High priority",    "⏫", "⏫ "),
    ("Medium priority",  "🔼", "🔼 "),
    ("Low priority",     "🔽", "🔽 "),
    ("Lowest priority",  "⏬", "⏬ "),
];

#[derive(Clone, Copy, PartialEq)]
enum MenuView {
    Options,
    DatePicker,
}

/// `on_select(insert_text)` — may carry a `{{today}}` placeholder for the
/// caller to fill in (see `dates::fill_placeholders`).
#[component]
pub fn TaskMenu(on_select: EventHandler<String>, on_close: EventHandler<()>) -> Element {
    let mut view = use_signal(|| MenuView::Options);

    rsx! {
        div {
            class: "task-menu-overlay",
            onclick: move |_| on_close(()),
            div {
                class: "task-menu",
                onclick: move |e| e.stop_propagation(),
                match view() {
                    MenuView::Options => rsx! {
                        div {
                            class: "task-menu-item",
                            // Clicking anywhere outside the editor's contenteditable
                            // steals focus and collapses its Selection — preventing
                            // the default mousedown action (not click) keeps the
                            // caret exactly where insertion needs it. Same pattern
                            // as the formatting toolbar (`views::toolbar`).
                            onmousedown: move |e| e.prevent_default(),
                            onclick: move |_| view.set(MenuView::DatePicker),
                            span { class: "task-menu-emoji", "📅" }
                            span { class: "task-menu-name", "Due date" }
                        }
                        for (name, emoji, insert) in PRIORITIES {
                            div {
                                class: "task-menu-item",
                                onmousedown: move |e| e.prevent_default(),
                                onclick: move |_| on_select(insert.to_string()),
                                span { class: "task-menu-emoji", "{emoji}" }
                                span { class: "task-menu-name", "{name}" }
                            }
                        }
                        div {
                            class: "task-menu-item",
                            onmousedown: move |e| e.prevent_default(),
                            onclick: move |_| on_select("✅ {{today}} ".to_string()),
                            span { class: "task-menu-emoji", "✅" }
                            span { class: "task-menu-name", "Done today" }
                        }
                    },
                    MenuView::DatePicker => rsx! {
                        div { class: "task-menu-date-header",
                            button {
                                class: "task-menu-back",
                                onmousedown: move |e| e.prevent_default(),
                                onclick: move |_| view.set(MenuView::Options),
                                "‹ Back"
                            }
                        }
                        TaskDatePicker {
                            on_select: move |ymd: String| on_select(format!("📅 {ymd} ")),
                        }
                    },
                }
            }
        }
    }
}
