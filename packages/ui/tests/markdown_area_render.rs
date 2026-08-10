//! Component-level tests for `MarkdownArea`: render the real Dioxus component
//! on the native target and assert on the emitted DOM. This validates the wiring
//! (props → initial `rendered_html` → contenteditable div), complementing the pure
//! `tokens_to_html` unit tests in the component module.
//!
//! Note: the editor's live behaviour (focus reveals raw markdown, typing, cursor
//! restoration) is driven by JS in `assets/markdown_area.js` via `use_js!`, which
//! has no JS runtime here — that belongs to the Playwright E2E layer. A single
//! `rebuild_in_place` pass renders the *initial, unfocused* state, which is
//! exactly the "notes render as formatted markdown" guarantee.

use dioxus::prelude::*;
use ui::{MarkdownArea, MarkdownAreaVariant};

fn render(app: fn() -> Element) -> String {
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

#[test]
fn renders_contenteditable_with_formatted_markdown() {
    fn app() -> Element {
        let content = use_signal(|| "# Heading\nsome **bold** text".to_string());
        rsx! { MarkdownArea { content } }
    }
    let html = render(app);

    // The editable surface exists...
    assert!(html.contains("contenteditable=\"true\""), "got: {html}");
    assert!(html.contains("class=\"md-area\""), "got: {html}");
    // ...and the initial (unfocused) render already shows formatted markdown,
    // not raw source dumped verbatim.
    assert!(html.contains("md-heading"), "got: {html}");
    assert!(html.contains("md-bold"), "got: {html}");
    assert!(html.contains("Heading"));
}

#[test]
fn applies_variant_and_placeholder_attributes() {
    fn app() -> Element {
        let content = use_signal(String::new);
        rsx! {
            MarkdownArea {
                content,
                variant: MarkdownAreaVariant::Ghost,
                placeholder: "Write here…",
            }
        }
    }
    let html = render(app);
    assert!(html.contains("data-style=\"ghost\""), "got: {html}");
    assert!(html.contains("data-placeholder=\"Write here…\""), "got: {html}");
}

#[test]
fn empty_content_still_renders_editable_surface() {
    fn app() -> Element {
        let content = use_signal(String::new);
        rsx! { MarkdownArea { content } }
    }
    let html = render(app);
    assert!(html.contains("contenteditable=\"true\""), "got: {html}");
}
