pub mod export;
pub mod state;
pub mod template;
pub mod views;
pub mod wikilink_index;

pub async fn sleep_ms(ms: u32) {
    use dioxus::prelude::document;
    let _ = document::eval(&format!(
        "await new Promise(r => setTimeout(r, {ms})); dioxus.send(null);"
    ))
    .join::<serde_json::Value>()
    .await;
}
