use dioxus::prelude::*;
use ui::MarkdownArea;

const SAMPLE: &str = "# Welcome to Oxidian

This is a **hybrid markdown editor**. The markdown syntax peeks through at low opacity while you edit.

## Formatting

- *Italic*, **bold**, ***bold italic***
- ~~strikethrough~~, `inline code`
- [[WikiLink]] or [[target|custom label]]
- [Dioxus](https://dioxuslabs.com)

## Tasks

- [ ] Buy milk
- [x] Build an editor
- [ ] Add GitHub login
  - [ ] Nested task

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
