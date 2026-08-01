//! Rendering a [`QueryResult`] to HTML.
//!
//! Kept pure and separate from the editor component so the output is unit
//! testable — the Dioxus side only has to drop the string in. Every value that
//! originates from a note is escaped: note content is untrusted input as far as
//! the renderer is concerned, and it is being injected via `dangerous_inner_html`.
//!
//! Links render as `<a data-wikilink="…">`, which is the same hook the editor
//! already uses for `[[wikilinks]]`, so navigation comes for free.

use crate::tasks::Task;
use crate::value::Value;

use super::eval::{QueryResult, ResultTable};

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// One cell / bullet value.
fn value_html(v: &Value) -> String {
    match v {
        Value::Link(target) => format!(
            "<a class=\"md-wikilink\" data-wikilink=\"{0}\">{0}</a>",
            escape(target)
        ),
        Value::List(items) => {
            items.iter().map(value_html).collect::<Vec<_>>().join(", ")
        }
        Value::Null => String::new(),
        other => escape(&other.to_display()),
    }
}

/// The full block, including the wrapper the stylesheet targets.
pub fn result_html(result: &QueryResult) -> String {
    let body = match result {
        QueryResult::Table(t) => table_html(t),
        QueryResult::List(items) => list_html(items),
        QueryResult::Tasks(tasks) => tasks_html(tasks),
    };
    format!("<div class=\"dataview\" contenteditable=\"false\">{body}</div>")
}

/// A parse or evaluation failure, shown in place of the block. The query is
/// typed live, so this is a normal state, not an exceptional one.
pub fn error_html(message: &str) -> String {
    format!(
        "<div class=\"dataview dataview-error\" contenteditable=\"false\">\
         <span class=\"dataview-error-label\">Dataview:</span> {}</div>",
        escape(message)
    )
}

fn empty_html(what: &str) -> String {
    format!("<div class=\"dataview-empty\">No {what} matched.</div>")
}

fn table_html(t: &ResultTable) -> String {
    if t.rows.is_empty() {
        return empty_html("notes");
    }
    let head: String = t
        .headers
        .iter()
        .map(|h| format!("<th>{}</th>", escape(h)))
        .collect();
    let body: String = t
        .rows
        .iter()
        .map(|(path, cells)| {
            let tds: String = cells
                .iter()
                .map(|c| format!("<td>{}</td>", value_html(c)))
                .collect();
            format!("<tr data-path=\"{}\">{tds}</tr>", escape(path))
        })
        .collect();
    format!("<table class=\"dataview-table\"><thead><tr>{head}</tr></thead><tbody>{body}</tbody></table>")
}

fn list_html(items: &[(String, Value)]) -> String {
    if items.is_empty() {
        return empty_html("notes");
    }
    let lis: String = items
        .iter()
        .map(|(path, v)| {
            format!("<li data-path=\"{}\">{}</li>", escape(path), value_html(v))
        })
        .collect();
    format!("<ul class=\"dataview-list\">{lis}</ul>")
}

fn tasks_html(tasks: &[Task]) -> String {
    if tasks.is_empty() {
        return empty_html("tasks");
    }
    let lis: String = tasks
        .iter()
        .map(|t| {
            let checked = if t.checked { "true" } else { "false" };
            let due = match &t.due {
                Some(d) => format!(" <span class=\"dataview-due\">📅 {}</span>", escape(d)),
                None => String::new(),
            };
            let prio = match t.priority.emoji() {
                "" => String::new(),
                e => format!(" <span class=\"dataview-prio\">{e}</span>"),
            };
            // data-path/data-line are the write-back coordinates: clicking the
            // box toggles the source line in that file, as the Tasks panel does.
            format!(
                "<li class=\"dataview-task\" data-path=\"{}\" data-line=\"{}\">\
                 <span class=\"md-task-checkbox\" data-checked=\"{checked}\">{}</span>\
                 <span class=\"dataview-task-text\">{}</span>{prio}{due}</li>",
                escape(&t.path),
                t.line,
                if t.checked { "[x]" } else { "[ ]" },
                escape(&t.text),
            )
        })
        .collect();
    format!("<ul class=\"dataview-tasks\">{lis}</ul>")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::Priority;

    fn table() -> QueryResult {
        QueryResult::Table(ResultTable {
            headers: vec!["File".into(), "rating".into()],
            rows: vec![
                ("games/Deus Ex.md".into(), vec![Value::Link("Deus Ex".into()), Value::Num(9.0)]),
                ("games/Pong.md".into(), vec![Value::Link("Pong".into()), Value::Null]),
            ],
        })
    }

    #[test]
    fn tables_render_headers_rows_and_link_cells() {
        let html = result_html(&table());
        assert!(html.contains("<th>File</th><th>rating</th>"));
        assert!(html.contains("data-path=\"games/Deus Ex.md\""));
        assert!(html.contains("data-wikilink=\"Deus Ex\""));
        assert!(html.contains("<td>9</td>"), "numbers lose the .0");
        assert!(html.contains("<td></td>"), "a null cell is blank, not 'null'");
        // The block is inert: the editor must not let the caret inside it.
        assert!(html.contains("contenteditable=\"false\""));
    }

    #[test]
    fn note_content_cannot_inject_html() {
        let evil = QueryResult::List(vec![(
            "a.md".into(),
            Value::Str("<script>alert('x')</script>".into()),
        )]);
        let html = result_html(&evil);
        assert!(!html.contains("<script>"), "got: {html}");
        assert!(html.contains("&lt;script&gt;"));
        // Quotes inside a value can't break out of an attribute either.
        let quoted = QueryResult::List(vec![("a\" onmouseover=\"evil".into(), Value::Null)]);
        assert!(!result_html(&quoted).contains("onmouseover=\"evil"));
    }

    #[test]
    fn empty_results_say_so_rather_than_rendering_an_empty_table() {
        let empty = QueryResult::Table(ResultTable { headers: vec!["x".into()], rows: vec![] });
        assert!(result_html(&empty).contains("No notes matched"));
        assert!(result_html(&QueryResult::Tasks(vec![])).contains("No tasks matched"));
    }

    #[test]
    fn tasks_carry_their_write_back_coordinates() {
        let t = Task {
            path: "games/Deus Ex.md".into(),
            line: 7,
            checked: false,
            text: "replay".into(),
            raw: "- [ ] replay".into(),
            due: Some("2026-09-01".into()),
            done: None,
            priority: Priority::High,
        };
        let html = result_html(&QueryResult::Tasks(vec![t]));
        assert!(html.contains("data-path=\"games/Deus Ex.md\""));
        assert!(html.contains("data-line=\"7\""));
        assert!(html.contains("data-checked=\"false\""));
        assert!(html.contains("📅 2026-09-01"));
        assert!(html.contains('⏫'), "priority emoji is shown");
    }

    #[test]
    fn errors_render_as_a_labelled_block() {
        let html = error_html("'select' is not a query type");
        assert!(html.contains("dataview-error"));
        assert!(html.contains("&#39;select&#39; is not a query type"));
    }
}
