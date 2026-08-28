//! The Plugins panel: list what's available, switch it on or off, configure it.
//!
//! The form is driven entirely by the plugin's declared [`Field`]s rather than
//! hand-written per-plugin markup — that is the whole point of describing a
//! plugin as data. When external plugins land they get this screen for free.

use dioxus::prelude::*;
use serde_json::Value;
use vault::GithubConfig;

use crate::plugins::{self, store::Values, Field, PathKind, PluginDef, PluginStore};

#[component]
pub fn PluginsModal(
    config: GithubConfig,
    store: Signal<PluginStore>,
    on_close: EventHandler<()>,
) -> Element {
    // Which plugin's settings are open; `None` shows the list.
    let mut editing = use_signal(|| None::<&'static str>);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let cfg_toggle = config.clone();
    let toggle = use_callback(move |def: &'static PluginDef| {
        let cfg = cfg_toggle.clone();
        let turning_on = !store.peek().is_enabled(def.id);
        let mut next = store.peek().clone();
        spawn(async move {
            busy.set(true);
            error.set(None);
            let result = if turning_on {
                // Seeding happens here rather than in the store so the store
                // stays ignorant of where a plugin's settings used to live.
                let seed = if def.id == plugins::bujo::ID {
                    plugins::bujo::seed_from_config(&cfg)
                } else {
                    Values::new()
                };
                next.enable(&cfg, def, seed).await
            } else {
                next.disable(&cfg, def.id).await
            };
            store.set(next);
            busy.set(false);
            if let Err(e) = result {
                error.set(Some(e));
            }
        });
    });

    let cfg_save = config.clone();
    let save = use_callback(move |(id, values): (&'static str, Values)| {
        let cfg = cfg_save.clone();
        let mut next = store.peek().clone();
        spawn(async move {
            busy.set(true);
            error.set(None);
            let result = next.save_settings(&cfg, id, &values).await;
            store.set(next);
            busy.set(false);
            match result {
                Ok(()) => editing.set(None),
                Err(e) => error.set(Some(e)),
            }
        });
    });

    let open = editing().and_then(plugins::find);

    rsx! {
        div { class: "move-picker-backdrop", onclick: move |_| on_close(()),
            div {
                class: "plugins-modal",
                onclick: move |e: Event<MouseData>| e.stop_propagation(),

                if let Some(def) = open {
                    div { class: "move-picker-header",
                        button {
                            class: "plugins-back",
                            "aria-label": "Back to the plugin list",
                            onclick: move |_| editing.set(None),
                            "‹"
                        }
                        "{def.name}"
                    }
                    // No key needed: switching plugins goes through the list,
                    // which unmounts this branch and resets the form's state.
                    PluginSettingsForm {
                        def_id: def.id,
                        initial: store.read().values(def),
                        busy: busy(),
                        on_save: move |v| save.call((def.id, v)),
                        on_cancel: move |()| editing.set(None),
                    }
                } else {
                    div { class: "move-picker-header", "Plugins" }
                    div { class: "plugins-list",
                        for def in plugins::builtins().iter().copied() {
                            {
                                let on = store.read().is_enabled(def.id);
                                rsx! {
                                    div { key: "{def.id}", class: "plugin-row",
                                        div { class: "plugin-info",
                                            span { class: "plugin-name", "{def.name}" }
                                            span { class: "plugin-desc", "{def.description}" }
                                            span { class: "plugin-path", "{plugins::plugin_dir(def.id)}/" }
                                        }
                                        div { class: "plugin-actions",
                                            button {
                                                class: "plugin-configure",
                                                disabled: !on,
                                                title: if on { "Configure" } else { "Enable it first" },
                                                onclick: move |_| editing.set(Some(def.id)),
                                                "Configure"
                                            }
                                            button {
                                                class: if on { "plugin-toggle plugin-toggle--on" } else { "plugin-toggle" },
                                                disabled: busy(),
                                                "aria-pressed": "{on}",
                                                "aria-label": if on { "Disable {def.name}" } else { "Enable {def.name}" },
                                                onclick: move |_| toggle.call(def),
                                                if on { "On" } else { "Off" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    p { class: "plugins-hint",
                        "Enabling a plugin creates its folder in the vault with default \
                         settings and templates. Turning one off changes nothing on disk — \
                         your notes and templates stay exactly where they are."
                    }
                }

                if let Some(e) = error() {
                    p { class: "settings-error", "{e}" }
                }
            }
        }
    }
}

/// A form rendered from a plugin's declared fields. Knows nothing about any
/// particular plugin — add a `Field` variant and every plugin can use it.
#[component]
fn PluginSettingsForm(
    def_id: &'static str,
    initial: Values,
    busy: bool,
    on_save: EventHandler<Values>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut values = use_signal(|| initial.clone());
    let Some(def) = plugins::find(def_id) else {
        return rsx! { div {} };
    };

    rsx! {
        div { class: "plugins-form",
            for field in def.settings {
                {
                    let key = field.key();
                    let label = field.label();
                    let help = field.help();
                    let current = values.read().get(key).cloned().unwrap_or(Value::Null);
                    // Anything non-string edits as empty rather than showing
                    // `null` or `true` in a text box.
                    let text = current.as_str().unwrap_or("").to_string();
                    rsx! {
                        label { key: "{key}", class: "settings-label", "{label}"
                            match field {
                                Field::Bool { .. } => rsx! {
                                    input {
                                        r#type: "checkbox",
                                        checked: matches!(current, Value::Bool(true)),
                                        onchange: move |e| {
                                            values.write().insert(key.to_string(), Value::Bool(e.checked()));
                                        },
                                    }
                                },
                                Field::Select { options, .. } => rsx! {
                                    select {
                                        class: "settings-input",
                                        value: "{text}",
                                        onchange: move |e| {
                                            values.write().insert(key.to_string(), Value::String(e.value()));
                                        },
                                        for (val, text) in options.iter() {
                                            option { key: "{val}", value: "{val}", "{text}" }
                                        }
                                    }
                                },
                                Field::Path { kind, default, .. } => rsx! {
                                    input {
                                        class: "settings-input",
                                        placeholder: if *kind == PathKind::Dir { "path/to/folder" } else { "{default}" },
                                        value: "{text}",
                                        oninput: move |e| {
                                            values.write().insert(key.to_string(), Value::String(e.value()));
                                        },
                                    }
                                },
                                Field::Text { default, .. } => rsx! {
                                    input {
                                        class: "settings-input",
                                        placeholder: "{default}",
                                        value: "{text}",
                                        oninput: move |e| {
                                            values.write().insert(key.to_string(), Value::String(e.value()));
                                        },
                                    }
                                },
                            }
                        }
                        if !help.is_empty() {
                            p { class: "settings-sub", "{help}" }
                        }
                    }
                }
            }
            div { class: "review-footer",
                span { class: "review-count" }
                button { class: "review-cancel", onclick: move |_| on_cancel(()), "Cancel" }
                button {
                    class: "review-apply",
                    disabled: busy,
                    onclick: move |_| on_save(values.read().clone()),
                    if busy { "Saving…" } else { "Save" }
                }
            }
        }
    }
}
