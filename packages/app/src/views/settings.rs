use dioxus::prelude::*;
use vault::{GithubConfig, Provider, poll_device_token, request_device_code, get_username, PollOutcome};

use crate::{sleep_ms, state};

#[derive(Clone, PartialEq)]
enum OAuthPhase {
    Idle,
    AwaitingAuth { user_code: String, verification_uri: String, verification_uri_complete: String },
    Done,
}

#[component]
pub fn Settings(
    on_save: EventHandler<GithubConfig>,
    existing: Option<GithubConfig>,
) -> Element {
    let mut token    = use_signal(|| existing.as_ref().map(|c| c.token.clone()).unwrap_or_default());
    let mut owner    = use_signal(|| existing.as_ref().map(|c| c.owner.clone()).unwrap_or_default());
    let mut repo     = use_signal(|| existing.as_ref().map(|c| c.repo.clone()).unwrap_or_default());
    let mut branch        = use_signal(|| existing.as_ref().map(|c| c.branch.clone()).unwrap_or_else(|| "main".to_string()));
    let mut provider      = use_signal(|| existing.as_ref().map(|c| c.provider.clone()).unwrap_or_default());
    let mut templates_dir       = use_signal(|| existing.as_ref().map(|c| c.templates_dir.clone()).unwrap_or_else(|| ".oxidian/templates".to_string()));
    let mut daily_note_template = use_signal(|| existing.as_ref().map(|c| c.daily_note_template.clone()).unwrap_or_else(|| ".oxidian/templates/daily-note.md".to_string()));
    let mut error    = use_signal(|| None::<String>);
    let mut saving   = use_signal(|| false);
    let mut show_token  = use_signal(|| false);
    let mut oauth_phase = use_signal(|| OAuthPhase::Idle);

    let start_oauth = move |_| {
        if !matches!(oauth_phase(), OAuthPhase::Idle) { return; }
        error.set(None);
        spawn(async move {
            match request_device_code().await {
                Err(e) => { error.set(Some(format!("Failed to start sign-in: {e}"))); }
                Ok(dc) => {
                    let device_code = dc.device_code.clone();
                    let mut interval = dc.interval;
                    let uri_complete = dc.verification_uri_complete
                        .unwrap_or_else(|| format!("{}?user_code={}", dc.verification_uri, dc.user_code));
                    oauth_phase.set(OAuthPhase::AwaitingAuth {
                        user_code: dc.user_code,
                        verification_uri: dc.verification_uri,
                        verification_uri_complete: uri_complete,
                    });
                    loop {
                        sleep_ms(interval as u32 * 1000).await;
                        // User may have cancelled
                        if !matches!(oauth_phase(), OAuthPhase::AwaitingAuth { .. }) { break; }
                        match poll_device_token(&device_code).await {
                            Ok(PollOutcome::Token(t)) => {
                                if let Ok(username) = get_username(&t).await {
                                    owner.set(username);
                                }
                                token.set(t);
                                oauth_phase.set(OAuthPhase::Done);
                                break;
                            }
                            Ok(PollOutcome::SlowDown(new_interval)) => { interval = new_interval; }
                            Ok(PollOutcome::Expired) => {
                                oauth_phase.set(OAuthPhase::Idle);
                                error.set(Some("Code expired — please try again.".into()));
                                break;
                            }
                            Ok(PollOutcome::Denied) => {
                                oauth_phase.set(OAuthPhase::Idle);
                                error.set(Some("Access denied.".into()));
                                break;
                            }
                            _ => {} // Pending — keep polling
                        }
                    }
                }
            }
        });
    };

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
        let td  = templates_dir.read().trim().to_string();
        let td  = if td.is_empty() { ".oxidian/templates".to_string() } else { td };
        let dnt = daily_note_template.read().trim().to_string();
        let dnt = if dnt.is_empty() { ".oxidian/templates/daily-note.md".to_string() } else { dnt };
        let cfg = GithubConfig {
            token: t, owner: o, repo: r, branch: b, provider: provider(),
            templates_dir: td, daily_note_template: dnt,
        };
        state::save_config(&cfg);
        on_save(cfg);
    };

    let token_url = match provider() {
        Provider::GitHub => "https://github.com/settings/tokens",
        Provider::GitLab => "https://gitlab.com/-/user_settings/personal_access_tokens",
    };
    let token_hint = match provider() {
        Provider::GitHub => "ghp_xxxxxxxxxxxxxxxxxxxx",
        Provider::GitLab => "glpat-xxxxxxxxxxxxxxxxxxxx",
    };

    // github.com/login/... OAuth endpoints don't set CORS headers, so the device
    // flow only works in native builds (desktop/mobile), not in the browser.
    let is_wasm = cfg!(target_arch = "wasm32");

    // Extract OAuth phase data for the template
    let phase = oauth_phase();
    let is_awaiting = matches!(phase, OAuthPhase::AwaitingAuth { .. });
    let is_done = phase == OAuthPhase::Done;
    let (user_code, verification_uri, verification_uri_complete) =
        if let OAuthPhase::AwaitingAuth { ref user_code, ref verification_uri, ref verification_uri_complete } = phase {
            (user_code.clone(), verification_uri.clone(), verification_uri_complete.clone())
        } else {
            (String::new(), String::new(), String::new())
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
                                    onclick: move |_| { provider.set(p.clone()); oauth_phase.set(OAuthPhase::Idle); },
                                    "{label}"
                                }
                            }
                        }
                    }
                }

                // GitHub OAuth Device Flow.
                if provider() == Provider::GitHub && !is_wasm {
                    if is_awaiting {
                        div { class: "settings-device-box",
                            p { class: "settings-device-instruction",
                                "Click the link to authorize — the code will be pre-filled:"
                            }
                            a {
                                class: "settings-device-link",
                                href: "{verification_uri_complete}",
                                target: "_blank",
                                rel: "noopener noreferrer",
                                "{verification_uri}"
                            }
                            div { class: "settings-device-code-row",
                                p { class: "settings-device-code", "{user_code}" }
                                button {
                                    class: "settings-copy-btn",
                                    r#type: "button",
                                    title: "Copy code",
                                    onclick: move |_| {
                                        let code = user_code.clone();
                                        spawn(async move {
                                            let _ = document::eval(&format!(
                                                "navigator.clipboard.writeText('{}').catch(()=>{{}})",
                                                code
                                            )).await;
                                        });
                                    },
                                    "Copy"
                                }
                            }
                            p { class: "settings-device-waiting", "Waiting for authorization…" }
                            button {
                                class: "settings-cancel-btn",
                                onclick: move |_| oauth_phase.set(OAuthPhase::Idle),
                                "Cancel"
                            }
                        }
                    } else if is_done {
                        p { class: "settings-oauth-done", "✓ Signed in with GitHub" }
                    } else {
                        button { class: "settings-oauth-btn", onclick: start_oauth,
                            "Sign in with GitHub"
                        }
                        p { class: "settings-divider", "— or enter a token manually —" }
                    }
                }
                if provider() == Provider::GitHub && is_wasm && !is_awaiting && !is_done {
                    p { class: "settings-sub",
                        "In the browser, paste a "
                        a {
                            href: "https://github.com/settings/tokens",
                            target: "_blank",
                            rel: "noopener noreferrer",
                            "Personal Access Token"
                        }
                        " with "
                        code { "repo" }
                        " scope below."
                    }
                }

                // Token field — hidden while OAuth is pending
                if !is_awaiting {
                    label { class: "settings-label", "Token"
                        div { class: "settings-input-row",
                            input {
                                class: "settings-input",
                                r#type: if show_token() { "text" } else { "password" },
                                placeholder: if is_done { "filled via OAuth" } else { token_hint },
                                value: "{token}",
                                oninput: move |e| token.set(e.value()),
                            }
                            button {
                                class: "settings-eye-btn",
                                r#type: "button",
                                title: if show_token() { "Hide token" } else { "Show token" },
                                onclick: move |_| show_token.set(!show_token()),
                                if show_token() { "🙈" } else { "👁" }
                            }
                        }
                    }

                    if provider() == Provider::GitLab {
                        p { class: "settings-sub",
                            "Generate a Personal Access Token with "
                            code { "api" }
                            " scope at "
                            a { href: "{token_url}", target: "_blank", rel: "noopener noreferrer", "{token_url}" }
                            "."
                        }
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
                label { class: "settings-label", "Templates folder"
                    input {
                        class: "settings-input", placeholder: ".oxidian/templates",
                        value: "{templates_dir}", oninput: move |e| templates_dir.set(e.value()),
                    }
                }
                label { class: "settings-label", "Daily note template"
                    input {
                        class: "settings-input", placeholder: ".oxidian/templates/daily-note.md",
                        value: "{daily_note_template}", oninput: move |e| daily_note_template.set(e.value()),
                    }
                }

                if let Some(err) = error() {
                    p { class: "settings-error", "{err}" }
                }

                if !is_awaiting {
                    button {
                        class: "settings-btn", disabled: saving(),
                        onclick: handle_save,
                        if saving() { "Connecting…" } else { "Connect vault" }
                    }
                }
            }
        }
    }
}
