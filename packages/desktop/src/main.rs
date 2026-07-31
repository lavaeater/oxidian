use dioxus::prelude::*;
use vault::GithubConfig;

use app::MAIN_CSS;
use app::state;
use app::views::{Settings, VaultBrowser};

const FAVICON: Asset = asset!("/assets/favicon.ico");

fn main() {
    // The default menu bar is disabled: on Linux/GTK3, muda's `init_for_gtk_window`
    // recurses inside `gtk_widget_realize` until the main thread's stack overflows,
    // so the app aborts before the first frame. We don't use a native menu anyway —
    // all commands live in the in-app toolbar.
    dioxus::LaunchBuilder::desktop()
        .with_cfg(dioxus::desktop::Config::new().with_menu(None))
        .launch(App);
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
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }

        if !booted() {
            // Blank while checking storage — avoids a settings flash.
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
