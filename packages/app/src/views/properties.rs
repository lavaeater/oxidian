/// YAML frontmatter properties editor.
/// Renders the `---\nkey: value\n---` block at the top of a note
/// as a collapsible key→value UI. Edits write back into the content signal.

use dioxus::prelude::*;

// ── YAML parser (minimal) ─────────────────────────────────────────────────────

/// Returns `(frontmatter_text, body_after_fence)` if the content starts with `---`.
pub fn split_frontmatter(content: &str) -> Option<(&str, &str)> {
    let content = content.strip_prefix("---")?;
    // Accept `---\n` or `---\r\n`
    let content = content.strip_prefix('\n').or_else(|| content.strip_prefix("\r\n"))?;
    // Find the closing `---`
    for (i, line) in content.lines().enumerate() {
        if line.trim() == "---" {
            // Calculate byte offset of the end of this line
            let prefix_len: usize = content.lines().take(i).map(|l| l.len() + 1).sum();
            let fm = &content[..prefix_len.saturating_sub(1).min(content.len())];
            let rest_start = prefix_len + line.len() + 1;
            let rest = if rest_start <= content.len() { &content[rest_start..] } else { "" };
            return Some((fm, rest));
        }
    }
    None
}

/// Parse simple `key: value` pairs from YAML frontmatter.
/// Only handles string/number/boolean scalar values (not nested objects or arrays).
pub fn parse_pairs(fm: &str) -> Vec<(String, String)> {
    fm.lines()
        .filter_map(|line| {
            let (key, val) = line.split_once(':')?;
            let key = key.trim().to_string();
            let val = val.trim().trim_matches('"').trim_matches('\'').to_string();
            if key.is_empty() { return None; }
            Some((key, val))
        })
        .collect()
}

/// Serialise key-value pairs back to YAML frontmatter (simple scalar values only).
fn pairs_to_yaml(pairs: &[(String, String)]) -> String {
    pairs.iter()
        .filter(|(k, _)| !k.is_empty())
        .map(|(k, v)| {
            // Quote values that contain special chars
            if v.contains(':') || v.starts_with(['#', '[', '{', '\'', '"', '&', '*']) {
                format!("{k}: \"{v}\"")
            } else {
                format!("{k}: {v}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Rebuild content with updated frontmatter.
pub fn set_frontmatter(content: &str, pairs: &[(String, String)]) -> String {
    let yaml = pairs_to_yaml(pairs);
    match split_frontmatter(content) {
        Some((_, body)) => format!("---\n{yaml}\n---\n{body}"),
        None => format!("---\n{yaml}\n---\n\n{content}"),
    }
}

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
