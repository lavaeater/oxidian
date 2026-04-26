use dioxus::prelude::*;
use ui::MarkdownArea;

const SAMPLE: &str = "# Welcome to Oxidian

This is a **hybrid markdown editor**. Click on any formatted text to edit the raw markdown, then click away to see it rendered again.

## Try these

- *Italic*, **bold**, ***bold italic***
- ~~strikethrough~~, `inline code`
- [[WikiLink]] or [[target|custom label]]
- [Dioxus](https://dioxuslabs.com)
- A list item below:

1. First ordered item
2. Second ordered item

> This is a blockquote

---

Start typing to try it out!";

#[component]
pub fn Home() -> Element {
    let content = use_signal(|| SAMPLE.to_string());

    rsx! {
        div {
            style: "max-width: 720px; margin: 2rem auto; padding: 0 1rem;",
            MarkdownArea {
                content,
                placeholder: "Start writing…",
            }
        }
    }
}
