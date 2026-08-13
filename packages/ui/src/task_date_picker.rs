//! A plain calendar-grid date picker for task due dates, built directly on
//! `dioxus_primitives::calendar` (our fork, pinned to Dioxus 0.8 — see the
//! workspace `Cargo.toml`). No popover/text-entry wrapper: the caller already
//! renders this inside its own popup (`app`'s task menu), so it's just the
//! grid, always open.

use dioxus::prelude::*;
use dioxus_primitives::calendar::{
    Calendar, CalendarGrid, CalendarHeader, CalendarMonthTitle, CalendarNavigation,
    CalendarNextMonthButton, CalendarPreviousMonthButton, CalendarView,
};
use time::Date;

/// Renders a one-month calendar grid; calls `on_select` with the clicked date
/// formatted as `YYYY-MM-DD` (the shape `index::tasks` parses).
#[component]
pub fn TaskDatePicker(on_select: EventHandler<String>) -> Element {
    rsx! {
        div {
            class: "task-date-picker",
            // Delegated: the primitive's own nav/day buttons don't take a
            // mousedown handler of ours directly, but preventing default here
            // still cancels their focus-shift (the bubbled event just isn't
            // stopped), which is what keeps the editor's own Selection intact
            // for the eventual insert. See `app::views::task_menu` for the
            // same pattern applied to our own menu items.
            onmousedown: move |e| e.prevent_default(),
            Calendar {
                on_date_change: move |date: Option<Date>| {
                    if let Some(date) = date {
                        on_select.call(format_ymd(date));
                    }
                },
                CalendarView {
                    CalendarHeader {
                        CalendarNavigation {
                            CalendarPreviousMonthButton { "‹" }
                            CalendarMonthTitle {}
                            CalendarNextMonthButton { "›" }
                        }
                    }
                    CalendarGrid {}
                }
            }
        }
    }
}

fn format_ymd(date: Date) -> String {
    format!("{:04}-{:02}-{:02}", date.year(), u8::from(date.month()), date.day())
}
