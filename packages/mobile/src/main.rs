use dioxus::prelude::*;
use vault::GithubConfig;

use app::MAIN_CSS;
use app::state;
use app::views::{Settings, VaultBrowser};

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut config: Signal<Option<GithubConfig>> = use_signal(|| None);
    let mut booted = use_signal(|| false);

    use_effect(move || {
        spawn(async move {
            config.set(state::load_config().await);
            booted.set(true);
        });
    });

    rsx! {
        document::Meta { name: "viewport", content: "width=device-width, initial-scale=1, viewport-fit=cover" }
        document::Link { rel: "stylesheet", href: MAIN_CSS }

        if !booted() {
            // blank while loading
        } else if let Some(cfg) = config() {
            VaultBrowser {
                config: cfg,
                on_logout: move |_| config.set(None),
            }
        } else {
            Settings {
                existing: None,
                on_save: move |cfg| config.set(Some(cfg)),
            }
        }
    }
}
