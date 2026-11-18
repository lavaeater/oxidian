//! The editor layout menu (ui-polish.md: "Conserve vertical space"): which
//! bars show, whether each sits above or below the document, and Super
//! Collapse. Opened from the tab strip's layout button.
//!
//! Preferences live in a `Signal<UiPrefs>` context provided by `VaultBrowser`
//! and are persisted on every change.

use dioxus::prelude::*;

use crate::state::{self, Bar, BarPos, UiPrefs};

/// The shared layout preferences. Falls back to a local default signal when no
/// provider is mounted (isolated component tests), so callers never panic.
pub fn use_ui_prefs() -> Signal<UiPrefs> {
    let fallback = use_signal(UiPrefs::default);
    try_use_context::<Signal<UiPrefs>>().unwrap_or(fallback)
}

/// Applies `f` to the preferences and persists the result.
pub fn update_prefs(mut prefs: Signal<UiPrefs>, f: impl FnOnce(&mut UiPrefs)) {
    prefs.with_mut(f);
    state::save_ui_prefs(&prefs.peek());
}

#[component]
pub fn LayoutMenu(on_close: EventHandler<()>) -> Element {
    let prefs = use_ui_prefs();
    let p = prefs.read().clone();

    rsx! {
        // Click-away scrim; the menu itself stops propagation.
        div { class: "layout-menu-scrim", onclick: move |_| on_close(()) }
        div {
            class: "layout-menu",
            role: "menu",
            onclick: move |e| e.stop_propagation(),
            div { class: "layout-menu-title", "Bars" }
            for bar in Bar::ALL {
                {
                    let bp = p.bar(bar);
                    let dim = if p.super_collapse { " layout-menu-row--dim" } else { "" };
                    rsx! {
                        div { key: "{bar:?}", class: "layout-menu-row{dim}",
                            label { class: "layout-menu-check",
                                input {
                                    r#type: "checkbox",
                                    checked: bp.visible,
                                    onchange: move |e| {
                                        let on = e.checked();
                                        update_prefs(prefs, |p| p.bar_mut(bar).visible = on);
                                    },
                                }
                                span { "{bar.label()}" }
                            }
                            div { class: "layout-menu-pos", role: "group", "aria-label": "{bar.label()} position",
                                for (pos, label) in [(BarPos::Top, "Top"), (BarPos::Bottom, "Bottom")] {
                                    button {
                                        key: "{label}",
                                        class: if bp.pos == pos { "layout-pos-btn layout-pos-btn--active" } else { "layout-pos-btn" },
                                        "aria-pressed": if bp.pos == pos { "true" } else { "false" },
                                        onclick: move |_| update_prefs(prefs, |p| p.bar_mut(bar).pos = pos),
                                        "{label}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Wide screens only (the rail is hidden on phones), so is this row.
            div { class: "layout-menu-sep layout-menu-wide" }
            div { class: "layout-menu-title layout-menu-wide", "Side panel" }
            label { class: "layout-menu-check layout-menu-rail layout-menu-wide",
                input {
                    r#type: "checkbox",
                    checked: p.right_rail,
                    onchange: move |e| {
                        let on = e.checked();
                        update_prefs(prefs, |p| p.right_rail = on);
                    },
                }
                span { "Outline & backlinks on the right" }
            }
            div { class: "layout-menu-sep" }
            label { class: "layout-menu-check layout-menu-super",
                input {
                    r#type: "checkbox",
                    checked: p.super_collapse,
                    onchange: move |e| {
                        let on = e.checked();
                        update_prefs(prefs, |p| p.super_collapse = on);
                    },
                }
                span { "Super Collapse" }
            }
            div { class: "layout-menu-hint", "Hides every bar except the tabs." }
        }
    }
}
