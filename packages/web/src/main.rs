use dioxus::prelude::*;
use tracing::Level;
use vault::GithubConfig;

use app::state;
use app::views::{Settings, VaultBrowser};

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default_with_config(
        tracing_wasm::WASMLayerConfigBuilder::new()
            .set_max_level(Level::WARN)
            .build(),
    );
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
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }

        if !booted() {
            // Blank while checking localStorage — avoids a settings flash.
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
