//! App-wide keyboard shortcuts.
//!
//! Uses the same proven cross-platform pattern as the sidebar/graph views: a
//! single `document::eval` installs one `window` `keydown` listener that posts
//! the matched command id back to Rust via `dioxus.send`, consumed by a long-
//! lived `recv()` loop. This is the raw eval streaming channel — NOT the
//! `use_js!`/`import()` path that breaks in the Android WebView — so it works on
//! web, desktop, and Android alike (where there's a hardware/soft keyboard).
//!
//! The listener is registered once and rebound on remount (removing any stale
//! handler first, so re-registration can't leak or send to a dead channel), and
//! removed on drop.

use dioxus::prelude::*;

// Install (or re-bind) the single global keydown handler. `dioxus` is the eval
// channel injected by Dioxus; the closure captures it, so each remount rebinds
// to the live channel. Modifier = Ctrl (Win/Linux) or ⌘ (macOS); Alt excluded.
const INSTALL_JS: &str = r#"
function oxHandle(e) {
    if (!(e.metaKey || e.ctrlKey) || e.altKey || e.repeat) return;
    var k = (e.key || '').toLowerCase();
    var id = null;
    if (k === 'p') id = 'palette';
    else if (k === 'o') id = 'switcher';
    if (id) { e.preventDefault(); dioxus.send(id); }
}
if (window.__oxShortcuts) window.removeEventListener('keydown', window.__oxShortcuts);
window.__oxShortcuts = oxHandle;
window.addEventListener('keydown', window.__oxShortcuts);
"#;

const REMOVE_JS: &str = r#"
if (window.__oxShortcuts) {
    window.removeEventListener('keydown', window.__oxShortcuts);
    delete window.__oxShortcuts;
}
return null;
"#;

/// Registers the global keyboard shortcuts for the lifetime of the calling
/// component. `on_shortcut` is invoked with the command id (`"palette"`,
/// `"switcher"`, …) whenever a matching chord is pressed.
pub fn use_global_shortcuts(on_shortcut: Callback<String>) {
    use_effect(move || {
        spawn(async move {
            let mut eval = document::eval(INSTALL_JS);
            loop {
                match eval.recv::<String>().await {
                    Ok(id) => on_shortcut.call(id),
                    Err(_) => break,
                }
            }
        });
    });

    use_drop(|| {
        let _ = document::eval(REMOVE_JS);
    });
}
