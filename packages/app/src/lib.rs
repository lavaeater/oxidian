pub mod export;
pub mod icons;
pub mod js;
pub mod state;
pub mod template;
pub mod views;
pub mod wikilink_index;

#[cfg(target_arch = "wasm32")]
pub async fn sleep_ms(ms: u32) {
    gloo_timers::future::TimeoutFuture::new(ms).await;
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn sleep_ms(ms: u32) {
    tokio::time::sleep(std::time::Duration::from_millis(ms as u64)).await;
}

/// Direct browser console.log — works even without a tracing subscriber.
/// Use this for debugging when you're not sure tracing is wired up.
#[cfg(target_arch = "wasm32")]
pub fn console_log(msg: &str) {
    web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(msg));
}

#[cfg(not(target_arch = "wasm32"))]
pub fn console_log(msg: &str) {
    println!("{msg}");
}
