/// YAML frontmatter properties editor.
/// Renders the `---\nkey: value\n---` block at the top of a note
/// as a collapsible key→value UI. Edits write back into the content signal.
use dioxus::prelude::*;

// The frontmatter parser lives in the `index` crate so that the editor and the
// vault index can never disagree about what a note declares — see
// docs/dataview.md §4.1. Re-exported here because callers (and tests) have
// always reached for `properties::split_frontmatter`.
pub use index::frontmatter::{parse_pairs, set_frontmatter, split_frontmatter};

// ── Component ─────────────────────────────────────────────────────────────────

#[component]
pub fn PropertiesPanel(mut content: Signal<String>) -> Element {
    let mut collapsed = use_signal(|| false);
    let mut new_key = use_signal(String::new);
    let mut new_val = use_signal(String::new);

    let raw = content.read();
    let Some((fm, _)) = split_frontmatter(&raw) else {
        // No frontmatter — offer to add it
        return rsx! {
            div { class: "props-empty",
                button {
                    class: "props-add-btn",
                    onclick: move |_| {
                        content.with_mut(|c| {
                            *c = format!("---\n\n---\n\n{c}");
                        });
                    },
                    "+ Add properties"
                }
            }
        };
    };

    let pairs = parse_pairs(fm);
    let pairs_display: Vec<(String, String)> = pairs.clone();

    rsx! {
        div { class: "props-panel",
            div {
                class: "props-header",
                onclick: move |_| collapsed.with_mut(|c| *c = !*c),
                span { class: "props-title", "Properties" }
                span { class: "props-toggle", if collapsed() { "▸" } else { "▾" } }
            }
            if !collapsed() {
                div { class: "props-body",
                    for (idx, (key, val)) in pairs_display.iter().enumerate() {
                        {
                            let key = key.clone();
                            let val = val.clone();
                            let pairs_key = pairs.clone();
                            let pairs_val = pairs.clone();
                            let pairs_del = pairs.clone();
                            rsx! {
                                div { class: "props-row", key: "{key}",
                                    input {
                                        class: "props-key",
                                        value: "{key}",
                                        oninput: move |e| {
                                            let mut p = pairs_key.clone();
                                            p[idx].0 = e.value();
                                            content.with_mut(|c| *c = set_frontmatter(c, &p));
                                        },
                                    }
                                    input {
                                        class: "props-val",
                                        value: "{val}",
                                        oninput: move |e| {
                                            let mut p = pairs_val.clone();
                                            p[idx].1 = e.value();
                                            content.with_mut(|c| *c = set_frontmatter(c, &p));
                                        },
                                    }
                                    button {
                                        class: "props-del",
                                        title: "Remove property",
                                        onclick: move |_| {
                                            let updated: Vec<_> = pairs_del.iter()
                                                .filter(|(k, _)| k != &key)
                                                .cloned()
                                                .collect();
                                            content.with_mut(|c| *c = set_frontmatter(c, &updated));
                                        },
                                        "×"
                                    }
                                }
                            }
                        }
                    }
                    // Add new property row
                    {
                        let pairs_add1 = pairs.clone();
                        let pairs_add2 = pairs.clone();
                        rsx! {
                            div { class: "props-row props-new-row",
                                input {
                                    class: "props-key",
                                    placeholder: "key",
                                    value: "{new_key}",
                                    oninput: move |e| new_key.set(e.value()),
                                }
                                input {
                                    class: "props-val",
                                    placeholder: "value",
                                    value: "{new_val}",
                                    oninput: move |e| new_val.set(e.value()),
                                    onkeydown: move |e| {
                                        if e.key() != Key::Enter { return; }
                                        let k = new_key.read().trim().to_string();
                                        let v = new_val.read().trim().to_string();
                                        if k.is_empty() { return; }
                                        let mut p = pairs_add1.clone();
                                        p.push((k, v));
                                        content.with_mut(|c| *c = set_frontmatter(c, &p));
                                        new_key.set(String::new()); new_val.set(String::new());
                                    },
                                }
                                button {
                                    class: "props-del",
                                    title: "Add property (Enter)",
                                    onclick: move |_| {
                                        let k = new_key.read().trim().to_string();
                                        let v = new_val.read().trim().to_string();
                                        if k.is_empty() { return; }
                                        let mut p = pairs_add2.clone();
                                        p.push((k, v));
                                        content.with_mut(|c| *c = set_frontmatter(c, &p));
                                        new_key.set(String::new()); new_val.set(String::new());
                                    },
                                    "+"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
