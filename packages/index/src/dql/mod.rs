//! DQL — the Dataview Query Language.
//!
//! ```text
//! TABLE [WITHOUT ID] <expr> [AS <name>], …
//! LIST [<expr>]
//! TASK
//!   FROM   <source>
//!   WHERE  <expr>
//!   SORT   <expr> [ASC|DESC], …
//!   LIMIT  <n>
//! ```
//!
//! Deliberately compatible with obsidian-dataview for the query types we
//! support, so existing vaults work unchanged and its documentation doubles as
//! our spec (`docs/dataview.md` §10). `CALENDAR`, `GROUP BY`, `FLATTEN`, and
//! DataviewJS are not implemented.
//!
//! Parse errors are values, never panics: the query is being typed live in the
//! editor, so a half-written query has to render as a message rather than take
//! the note down with it.

pub mod eval;
pub mod parse;
pub mod render;

use crate::value::Value;

pub use eval::{Context, QueryResult, ResultTable, execute};
pub use render::{error_html, result_html};
pub use parse::{ParseError, parse};

/// Parse, execute, and render a `dataview` block in one call — the whole
/// pipeline as the editor needs it. Always returns HTML: a bad query renders as
/// an error block, never as a failure the caller has to handle.
pub fn run(source: &str, index: &crate::Index, ctx: &eval::Context) -> String {
    match parse(source) {
        Ok(query) => render::result_html(&execute(&query, index, ctx)),
        Err(e) => render::error_html(&e.message),
    }
}

/// What the query renders as.
#[derive(Clone, Debug, PartialEq)]
pub enum QueryKind {
    /// One row per page. `columns` are `(expression, header)`.
    Table { columns: Vec<(Expr, String)>, without_id: bool },
    /// One bullet per page; the expression (if any) replaces the file link.
    List(Option<Expr>),
    /// One checkbox per task, from the pages the query selects.
    Task,
}

/// `FROM` — which pages the query starts with.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// `"folder"` — that folder and everything under it.
    Folder(String),
    /// `#tag`
    Tag(String),
    /// `[[Note]]` — pages linking *to* that note.
    LinkedTo(String),
    /// `outgoing([[Note]])` — pages that note links to.
    Outgoing(String),
    And(Box<Source>, Box<Source>),
    Or(Box<Source>, Box<Source>),
    Not(Box<Source>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod,
    Eq, Ne, Lt, Le, Gt, Ge,
    And, Or,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Lit(Value),
    /// A bare name: a page field, or an implicit like `file`.
    Var(String),
    /// `a.b`
    Field(Box<Expr>, String),
    /// `f(a, b)`
    Call(String, Vec<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    Neg(Box<Expr>),
    /// `[a, b]`
    ListLit(Vec<Expr>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sort {
    pub expr: Expr,
    pub descending: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Query {
    pub kind: QueryKind,
    pub from: Option<Source>,
    /// Successive `WHERE` clauses are ANDed, like Dataview.
    pub filters: Vec<Expr>,
    pub sorts: Vec<Sort>,
    pub limit: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Index;
    use vault::FileMeta;

    fn vault() -> Index {
        let files = vec![FileMeta { path: "games/Pong.md".into(), sha: "s".into(), size: 0 }];
        let mut idx = Index::new();
        idx.apply(vec![("games/Pong.md".into(), "---\nrating: 4\n---\n".into())], &files);
        idx
    }

    #[test]
    fn run_returns_html_for_a_good_query() {
        let html = run("table rating from \"games\"", &vault(), &eval::Context::default());
        assert!(html.contains("dataview-table"));
        assert!(html.contains("<td>4</td>"));
    }

    #[test]
    fn run_returns_an_error_block_instead_of_failing() {
        // The whole point: a half-typed query renders a message, never a panic
        // and never a `Result` the editor has to handle.
        let html = run("tabl rating", &vault(), &eval::Context::default());
        assert!(html.contains("dataview-error"));
        assert!(html.contains("not a query type"));
    }
}
