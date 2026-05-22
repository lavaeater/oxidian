use dioxus::prelude::*;
use vault::GithubConfig;

use crate::state;

#[component]
pub fn Settings(
    /// Called with the validated config when the user saves.
    on_save: EventHandler<GithubConfig>,
    /// Optional existing config pre-fills the form.
    existing: Option<GithubConfig>,
) -> Element {
    let mut token = use_signal(|| existing.as_ref().map(|c| c.token.clone()).unwrap_or_default());
    let mut owner = use_signal(|| existing.as_ref().map(|c| c.owner.clone()).unwrap_or_default());
    let mut repo = use_signal(|| existing.as_ref().map(|c| c.repo.clone()).unwrap_or_default());
    let mut branch = use_signal(|| {
        existing
            .as_ref()
            .map(|c| c.branch.clone())
            .unwrap_or_else(|| "main".to_string())
    });
    let mut error = use_signal(|| None::<String>);
    let mut saving = use_signal(|| false);

    let handle_save = move |_| {
        let t = token.read().trim().to_string();
        let o = owner.read().trim().to_string();
        let r = repo.read().trim().to_string();
        let b = branch.read().trim().to_string();

        if t.is_empty() || o.is_empty() || r.is_empty() {
            error.set(Some("Token, owner, and repo are required.".to_string()));
            return;
        }

        saving.set(true);
        error.set(None);

        let cfg = GithubConfig { token: t, owner: o, repo: r, branch: b };
        state::save_config(&cfg);
        on_save(cfg);
    };

    rsx! {
        div { class: "settings-wrap",
            div { class: "settings-card",
                h2 { class: "settings-title", "Connect your vault" }
                p { class: "settings-sub",
                    "Oxidian reads and writes markdown files in a GitHub repository. "
                    "Generate a Personal Access Token at "
                    a {
                        href: "https://github.com/settings/tokens",
                        target: "_blank",
                        rel: "noopener noreferrer",
                        "github.com/settings/tokens"
                    }
                    " with the "
                    code { "repo" }
                    " scope."
                }

                label { class: "settings-label", "GitHub Token"
                    input {
                        class: "settings-input",
                        r#type: "password",
                        placeholder: "ghp_xxxxxxxxxxxxxxxxxxxx",
                        value: "{token}",
                        oninput: move |e| token.set(e.value()),
                    }
                }
                label { class: "settings-label", "Owner (user or org)"
                    input {
                        class: "settings-input",
                        placeholder: "octocat",
                        value: "{owner}",
                        oninput: move |e| owner.set(e.value()),
                    }
                }
                label { class: "settings-label", "Repository"
                    input {
                        class: "settings-input",
                        placeholder: "my-notes",
                        value: "{repo}",
                        oninput: move |e| repo.set(e.value()),
                    }
                }
                label { class: "settings-label", "Branch"
                    input {
                        class: "settings-input",
                        placeholder: "main",
                        value: "{branch}",
                        oninput: move |e| branch.set(e.value()),
                    }
                }

                if let Some(err) = error() {
                    p { class: "settings-error", "{err}" }
                }

                button {
                    class: "settings-btn",
                    disabled: saving(),
                    onclick: handle_save,
                    if saving() { "Connecting…" } else { "Connect vault" }
                }
            }
        }
    }
}
