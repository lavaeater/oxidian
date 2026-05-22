use dioxus::prelude::*;
use vault::{GithubConfig, Provider};

use crate::state;

#[component]
pub fn Settings(
    on_save: EventHandler<GithubConfig>,
    existing: Option<GithubConfig>,
) -> Element {
    let mut token = use_signal(|| existing.as_ref().map(|c| c.token.clone()).unwrap_or_default());
    let mut owner = use_signal(|| existing.as_ref().map(|c| c.owner.clone()).unwrap_or_default());
    let mut repo  = use_signal(|| existing.as_ref().map(|c| c.repo.clone()).unwrap_or_default());
    let mut branch = use_signal(|| existing.as_ref().map(|c| c.branch.clone()).unwrap_or_else(|| "main".to_string()));
    let mut provider = use_signal(|| existing.as_ref().map(|c| c.provider.clone()).unwrap_or_default());
    let mut error  = use_signal(|| None::<String>);
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
        let cfg = GithubConfig { token: t, owner: o, repo: r, branch: b, provider: provider() };
        state::save_config(&cfg);
        on_save(cfg);
    };

    let (token_hint, token_url) = match provider() {
        Provider::GitHub => ("ghp_xxxxxxxxxxxxxxxxxxxx", "https://github.com/settings/tokens"),
        Provider::GitLab => ("glpat-xxxxxxxxxxxxxxxxxxxx", "https://gitlab.com/-/user_settings/personal_access_tokens"),
    };

    rsx! {
        div { class: "settings-wrap",
            div { class: "settings-card",
                h2 { class: "settings-title", "Connect your vault" }

                // Provider selector
                div { class: "settings-provider-row",
                    for p in [Provider::GitHub, Provider::GitLab] {
                        {
                            let is_active = provider() == p;
                            let label = p.label();
                            rsx! {
                                button {
                                    class: if is_active { "provider-btn provider-btn--active" } else { "provider-btn" },
                                    onclick: move |_| provider.set(p.clone()),
                                    "{label}"
                                }
                            }
                        }
                    }
                }

                p { class: "settings-sub",
                    "Generate a Personal Access Token with "
                    code { "api" }
                    " / "
                    code { "repo" }
                    " scope at "
                    a { href: "{token_url}", target: "_blank", rel: "noopener noreferrer", "{token_url}" }
                    "."
                }

                label { class: "settings-label", "Token"
                    input {
                        class: "settings-input", r#type: "password",
                        placeholder: "{token_hint}",
                        value: "{token}",
                        oninput: move |e| token.set(e.value()),
                    }
                }
                label { class: "settings-label", "Owner (user or namespace)"
                    input {
                        class: "settings-input", placeholder: "octocat",
                        value: "{owner}", oninput: move |e| owner.set(e.value()),
                    }
                }
                label { class: "settings-label", "Repository"
                    input {
                        class: "settings-input", placeholder: "my-notes",
                        value: "{repo}", oninput: move |e| repo.set(e.value()),
                    }
                }
                label { class: "settings-label", "Branch"
                    input {
                        class: "settings-input", placeholder: "main",
                        value: "{branch}", oninput: move |e| branch.set(e.value()),
                    }
                }

                if let Some(err) = error() {
                    p { class: "settings-error", "{err}" }
                }

                button {
                    class: "settings-btn", disabled: saving(),
                    onclick: handle_save,
                    if saving() { "Connecting…" } else { "Connect vault" }
                }
            }
        }
    }
}
