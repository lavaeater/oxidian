//! Proves the dioxus-ssr test harness works in this workspace before we build
//! real component tests on top of it. If this fails to compile or render, the
//! alpha versions are mismatched.

use dioxus::prelude::*;

/// Render a zero-prop component to a static HTML string by driving a VirtualDom
/// one rebuild pass. This is the base harness the component tests reuse.
fn render(app: fn() -> Element) -> String {
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

#[test]
fn renders_static_markup() {
    fn app() -> Element {
        rsx! {
            div { class: "greeting", "hello oxidian" }
        }
    }
    let html = render(app);
    assert!(html.contains("hello oxidian"), "got: {html}");
    assert!(html.contains("class=\"greeting\""), "got: {html}");
}

#[test]
fn renders_signal_derived_content() {
    fn app() -> Element {
        let count = use_signal(|| 41);
        let next = count() + 1;
        rsx! { span { "count is {next}" } }
    }
    assert!(render(app).contains("count is 42"));
}
