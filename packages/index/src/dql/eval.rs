//! Query execution: `Query` + `Index` → rows. Pure, and fast enough that the
//! cost of a dataview block is dominated by I/O and extraction, never by this
//! (`docs/dataview.md` §8).

use crate::extract::PageData;
use crate::tasks::Task;
use crate::value::{Date, Value};
use crate::Index;

use super::{BinOp, Expr, Query, QueryKind, Sort, Source};

/// Everything a query needs beyond the index itself.
#[derive(Clone, Debug, Default)]
pub struct Context {
    /// The note the query block lives in — resolves `this.…` and makes
    /// `WHERE file.path != this.file.path` work.
    pub current_path: String,
    /// Today's date, for `date(today)`. Supplied by the caller because this
    /// crate has no clock (`app::dates` owns that).
    pub today: Option<Date>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResultTable {
    pub headers: Vec<String>,
    /// One entry per row: the page it came from, and its cells.
    pub rows: Vec<(String, Vec<Value>)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum QueryResult {
    Table(ResultTable),
    /// `(path, rendered value)` per bullet.
    List(Vec<(String, Value)>),
    Tasks(Vec<Task>),
}

impl QueryResult {
    pub fn len(&self) -> usize {
        match self {
            QueryResult::Table(t) => t.rows.len(),
            QueryResult::List(l) => l.len(),
            QueryResult::Tasks(t) => t.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Run a query against the index.
pub fn execute(query: &Query, index: &Index, ctx: &Context) -> QueryResult {
    let mut pages = select(query.from.as_ref(), index);

    for filter in &query.filters {
        pages.retain(|p| eval(filter, p, index, ctx).truthy());
    }

    sort_pages(&mut pages, &query.sorts, index, ctx);

    if let Some(limit) = query.limit
        && !matches!(query.kind, QueryKind::Task)
    {
        pages.truncate(limit);
    }

    match &query.kind {
        QueryKind::Table { columns, without_id } => {
            let mut headers: Vec<String> = Vec::new();
            if !without_id {
                headers.push("File".to_string());
            }
            headers.extend(columns.iter().map(|(_, h)| h.clone()));

            let rows = pages
                .iter()
                .map(|p| {
                    let mut cells = Vec::with_capacity(headers.len());
                    if !without_id {
                        cells.push(Value::Link(p.stem().to_string()));
                    }
                    cells.extend(columns.iter().map(|(e, _)| eval(e, p, index, ctx)));
                    (p.path.clone(), cells)
                })
                .collect();
            QueryResult::Table(ResultTable { headers, rows })
        }
        QueryKind::List(expr) => QueryResult::List(
            pages
                .iter()
                .map(|p| {
                    let v = match expr {
                        Some(e) => eval(e, p, index, ctx),
                        None => Value::Link(p.stem().to_string()),
                    };
                    (p.path.clone(), v)
                })
                .collect(),
        ),
        QueryKind::Task => {
            let mut tasks: Vec<Task> =
                pages.iter().flat_map(|p| p.tasks.iter().cloned()).collect();
            tasks.sort_by(crate::tasks::cmp);
            if let Some(limit) = query.limit {
                tasks.truncate(limit);
            }
            QueryResult::Tasks(tasks)
        }
    }
}

/// Resolve `FROM` to a candidate page set. A missing source means the whole
/// vault, matching Dataview.
fn select<'a>(source: Option<&Source>, index: &'a Index) -> Vec<&'a PageData> {
    let Some(source) = source else {
        return index.pages().collect();
    };
    match source {
        Source::Folder(f) => index.pages_in_folder(f),
        Source::Tag(t) => index.pages_with_tag(t),
        Source::LinkedTo(target) => {
            let target = target.to_lowercase();
            index
                .pages()
                .filter(|p| p.links.iter().any(|l| l.to_lowercase() == target))
                .collect()
        }
        Source::Outgoing(target) => {
            // Pages the named note links to.
            let targets: Vec<String> = index
                .pages()
                .find(|p| p.stem().eq_ignore_ascii_case(target))
                .map(|p| p.links.iter().map(|l| l.to_lowercase()).collect())
                .unwrap_or_default();
            index
                .pages()
                .filter(|p| targets.contains(&p.stem().to_lowercase()))
                .collect()
        }
        Source::And(a, b) => {
            let right = select(Some(b), index);
            select(Some(a), index)
                .into_iter()
                .filter(|p| right.iter().any(|q| q.path == p.path))
                .collect()
        }
        Source::Or(a, b) => {
            let mut out = select(Some(a), index);
            for p in select(Some(b), index) {
                if !out.iter().any(|q| q.path == p.path) {
                    out.push(p);
                }
            }
            out
        }
        Source::Not(inner) => {
            let excluded = select(Some(inner), index);
            index
                .pages()
                .filter(|p| !excluded.iter().any(|q| q.path == p.path))
                .collect()
        }
    }
}

fn sort_pages(pages: &mut [&PageData], sorts: &[Sort], index: &Index, ctx: &Context) {
    if sorts.is_empty() {
        // Stable default: path order, which is folder-grouped and predictable.
        pages.sort_by(|a, b| a.path.cmp(&b.path));
        return;
    }
    pages.sort_by(|a, b| {
        for s in sorts {
            let va = eval(&s.expr, a, index, ctx);
            let vb = eval(&s.expr, b, index, ctx);
            let ord = va.compare(&vb);
            let ord = if s.descending { ord.reverse() } else { ord };
            if ord != std::cmp::Ordering::Equal {
                return ord;
            }
        }
        a.path.cmp(&b.path)
    });
}

// ── Expression evaluation ────────────────────────────────────────────────────

/// Evaluate `expr` for one page. Never fails: an unknown field or a nonsensical
/// operation yields `Null`, so one bad column can't take out the whole table.
pub fn eval(expr: &Expr, page: &PageData, index: &Index, ctx: &Context) -> Value {
    match expr {
        Expr::Lit(v) => v.clone(),
        Expr::Var(name) => match name.to_lowercase().as_str() {
            "file" => Value::Str(page.path.clone()),
            "this" => Value::Str(ctx.current_path.clone()),
            "tags" | "etags" => tag_list(page),
            // Bare `today` is a date, as in `due <= today`. It shadows a field
            // of the same name, which is what Dataview does too.
            "today" | "now" => ctx.today.map(Value::Date).unwrap_or(Value::Null),
            _ => page.field(name).cloned().unwrap_or(Value::Null),
        },
        Expr::Field(base, name) => {
            match page_ref(base, page, index, ctx) {
                Some(p) => implicit(p, name, index),
                // `a.b` on a plain value: only meaningful for a link, which we
                // resolve to the page it names.
                None => match eval(base, page, index, ctx) {
                    Value::Link(target) => index
                        .pages()
                        .find(|p| p.stem().eq_ignore_ascii_case(&target))
                        .map(|p| implicit(p, name, index))
                        .unwrap_or(Value::Null),
                    _ => Value::Null,
                },
            }
        }
        Expr::ListLit(items) => {
            Value::List(items.iter().map(|e| eval(e, page, index, ctx)).collect())
        }
        Expr::Not(e) => Value::Bool(!eval(e, page, index, ctx).truthy()),
        Expr::Neg(e) => match eval(e, page, index, ctx) {
            Value::Num(n) => Value::Num(-n),
            Value::Duration(d) => Value::Duration(-d),
            _ => Value::Null,
        },
        Expr::Bin(op, l, r) => {
            // Short-circuit so `x and y` doesn't evaluate y needlessly.
            match op {
                BinOp::And => {
                    return Value::Bool(
                        eval(l, page, index, ctx).truthy() && eval(r, page, index, ctx).truthy(),
                    );
                }
                BinOp::Or => {
                    return Value::Bool(
                        eval(l, page, index, ctx).truthy() || eval(r, page, index, ctx).truthy(),
                    );
                }
                _ => {}
            }
            binary(*op, eval(l, page, index, ctx), eval(r, page, index, ctx))
        }
        Expr::Call(name, args) => {
            let vals: Vec<Value> = args.iter().map(|a| eval(a, page, index, ctx)).collect();
            call(name, &vals, ctx)
        }
    }
}

/// Which page an expression names, if any. Handles the chains that appear in
/// real queries: `file.x`, `this.file.x`, and `this.x`. Returning `None` means
/// "not a page reference", and the caller falls back to value semantics.
fn page_ref<'a>(
    expr: &Expr,
    page: &'a PageData,
    index: &'a Index,
    ctx: &Context,
) -> Option<&'a PageData> {
    match expr {
        Expr::Var(v) if v.eq_ignore_ascii_case("file") => Some(page),
        Expr::Var(v) if v.eq_ignore_ascii_case("this") => index.get(&ctx.current_path),
        // `<page>.file` is still that page — this is what makes `this.file.name`
        // resolve rather than looking up a field literally called "file".
        Expr::Field(base, name) if name.eq_ignore_ascii_case("file") => {
            page_ref(base, page, index, ctx)
        }
        _ => None,
    }
}

fn tag_list(page: &PageData) -> Value {
    Value::List(page.tags.iter().map(|t| Value::Str(t.clone())).collect())
}

/// `file.<name>` — the implicit fields every page has.
fn implicit(page: &PageData, name: &str, index: &Index) -> Value {
    match name.to_lowercase().as_str() {
        "name" => Value::Str(page.stem().to_string()),
        "path" => Value::Str(page.path.clone()),
        "folder" => Value::Str(page.folder().to_string()),
        "link" => Value::Link(page.stem().to_string()),
        "ext" => Value::Str("md".to_string()),
        "size" => Value::Num(page.tasks.len() as f64),
        "tags" | "etags" => tag_list(page),
        "outlinks" => Value::List(page.links.iter().map(|l| Value::Link(l.clone())).collect()),
        "inlinks" => Value::List(
            index
                .backlinks(&page.path)
                .into_iter()
                .map(|p| Value::Link(p.rsplit('/').next().unwrap_or(p).trim_end_matches(".md").to_string()))
                .collect(),
        ),
        "tasks" => Value::Num(page.tasks.len() as f64),
        // `updated` is stamped into frontmatter on save — see docs §2.5. `day`
        // is the date in the filename, the daily-note convention.
        "mtime" | "updated" => page.field("updated").cloned().unwrap_or(Value::Null),
        "ctime" | "created" => page.field("created").cloned().unwrap_or(Value::Null),
        "day" => day_from_name(page.stem()),
        _ => page.field(name).cloned().unwrap_or(Value::Null),
    }
}

/// A `YYYY-MM-DD` anywhere in the note name, like Obsidian's daily notes.
fn day_from_name(stem: &str) -> Value {
    let chars: Vec<char> = stem.chars().collect();
    for start in 0..chars.len().saturating_sub(9) {
        let candidate: String = chars[start..start + 10].iter().collect();
        if let Some(d) = Date::parse(&candidate) {
            return Value::Date(d);
        }
    }
    Value::Null
}

fn binary(op: BinOp, l: Value, r: Value) -> Value {
    use BinOp::*;
    match op {
        Eq => Value::Bool(l.loose_eq(&r)),
        Ne => Value::Bool(!l.loose_eq(&r)),
        Lt | Le | Gt | Ge => {
            // Null never satisfies an ordering comparison, so an undated note
            // is excluded by `due < date(today)` rather than silently included.
            if matches!(l, Value::Null) || matches!(r, Value::Null) {
                return Value::Bool(false);
            }
            let ord = l.compare(&r);
            Value::Bool(match op {
                Lt => ord.is_lt(),
                Le => ord.is_le(),
                Gt => ord.is_gt(),
                _ => ord.is_ge(),
            })
        }
        Add => match (&l, &r) {
            (Value::Num(a), Value::Num(b)) => Value::Num(a + b),
            (Value::Date(d), Value::Duration(days)) => Value::Date(d.plus_days(*days)),
            (Value::Duration(days), Value::Date(d)) => Value::Date(d.plus_days(*days)),
            (Value::Duration(a), Value::Duration(b)) => Value::Duration(a + b),
            (Value::List(a), Value::List(b)) => {
                Value::List(a.iter().chain(b.iter()).cloned().collect())
            }
            (Value::Null, _) | (_, Value::Null) => Value::Null,
            // Anything else concatenates as text, which is how Dataview builds
            // labels: `"Due: " + due`.
            _ => Value::Str(format!("{}{}", l.to_display(), r.to_display())),
        },
        Sub => match (&l, &r) {
            (Value::Num(a), Value::Num(b)) => Value::Num(a - b),
            (Value::Date(a), Value::Date(b)) => Value::Duration(a.to_days() - b.to_days()),
            (Value::Date(d), Value::Duration(days)) => Value::Date(d.plus_days(-days)),
            (Value::Duration(a), Value::Duration(b)) => Value::Duration(a - b),
            _ => Value::Null,
        },
        Mul => match (&l, &r) {
            (Value::Num(a), Value::Num(b)) => Value::Num(a * b),
            (Value::Duration(d), Value::Num(n)) | (Value::Num(n), Value::Duration(d)) => {
                Value::Duration((*d as f64 * n) as i64)
            }
            _ => Value::Null,
        },
        Div => match (&l, &r) {
            (Value::Num(_), Value::Num(b)) if *b == 0.0 => Value::Null,
            (Value::Num(a), Value::Num(b)) => Value::Num(a / b),
            _ => Value::Null,
        },
        Mod => match (&l, &r) {
            (Value::Num(_), Value::Num(b)) if *b == 0.0 => Value::Null,
            (Value::Num(a), Value::Num(b)) => Value::Num(a % b),
            _ => Value::Null,
        },
        And | Or => unreachable!("short-circuited in eval"),
    }
}

/// The built-in function library. Unknown functions and bad arities return
/// `Null` rather than erroring — a typo in one column shouldn't blank the table.
fn call(name: &str, args: &[Value], ctx: &Context) -> Value {
    let arg = |i: usize| args.get(i).cloned().unwrap_or(Value::Null);
    match name {
        "length" => match arg(0) {
            Value::List(l) => Value::Num(l.len() as f64),
            Value::Str(s) => Value::Num(s.chars().count() as f64),
            Value::Null => Value::Num(0.0),
            other => Value::Num(other.to_display().chars().count() as f64),
        },
        "contains" => Value::Bool(arg(0).contains(&arg(1))),
        "lower" => Value::Str(arg(0).to_display().to_lowercase()),
        "upper" => Value::Str(arg(0).to_display().to_uppercase()),
        "default" => match arg(0) {
            Value::Null => arg(1),
            v => v,
        },
        "choice" => {
            if arg(0).truthy() { arg(1) } else { arg(2) }
        }
        "startswith" => Value::Bool(
            arg(0).to_display().to_lowercase().starts_with(&arg(1).to_display().to_lowercase()),
        ),
        "endswith" => Value::Bool(
            arg(0).to_display().to_lowercase().ends_with(&arg(1).to_display().to_lowercase()),
        ),
        "number" => match arg(0) {
            Value::Num(n) => Value::Num(n),
            v => v.to_display().trim().parse::<f64>().map(Value::Num).unwrap_or(Value::Null),
        },
        "string" => Value::Str(arg(0).to_display()),
        "join" => {
            let sep = match args.get(1) {
                Some(v) => v.to_display(),
                None => ", ".to_string(),
            };
            match arg(0) {
                Value::List(l) => {
                    Value::Str(l.iter().map(Value::to_display).collect::<Vec<_>>().join(&sep))
                }
                v => Value::Str(v.to_display()),
            }
        }
        "reverse" => match arg(0) {
            Value::List(mut l) => {
                l.reverse();
                Value::List(l)
            }
            v => v,
        },
        "sort" => match arg(0) {
            Value::List(mut l) => {
                l.sort_by(|a, b| a.compare(b));
                Value::List(l)
            }
            v => v,
        },
        "sum" => match arg(0) {
            Value::List(l) => Value::Num(
                l.iter().filter_map(|v| match v {
                    Value::Num(n) => Some(*n),
                    _ => None,
                }).sum(),
            ),
            Value::Num(n) => Value::Num(n),
            _ => Value::Num(0.0),
        },
        "min" | "max" => match arg(0) {
            Value::List(l) if !l.is_empty() => {
                let pick = if name == "min" {
                    l.iter().min_by(|a, b| a.compare(b))
                } else {
                    l.iter().max_by(|a, b| a.compare(b))
                };
                pick.cloned().unwrap_or(Value::Null)
            }
            _ => Value::Null,
        },
        "round" => match arg(0) {
            Value::Num(n) => {
                let digits = match arg(1) {
                    Value::Num(d) => d.max(0.0) as u32,
                    _ => 0,
                };
                let f = 10f64.powi(digits as i32);
                Value::Num((n * f).round() / f)
            }
            _ => Value::Null,
        },
        "date" => match arg(0) {
            Value::Date(d) => Value::Date(d),
            Value::Str(s) if s.eq_ignore_ascii_case("today") => {
                ctx.today.map(Value::Date).unwrap_or(Value::Null)
            }
            v => Date::parse(&v.to_display()).map(Value::Date).unwrap_or(Value::Null),
        },
        // `dur(7)` / `dur(7 days)` — days are the only unit notes use in
        // practice, and the parser hands us `7` followed by a bare word.
        "dur" => match arg(0) {
            Value::Num(n) => Value::Duration(n as i64),
            Value::Duration(d) => Value::Duration(d),
            v => v
                .to_display()
                .split_whitespace()
                .next()
                .and_then(|n| n.parse::<i64>().ok())
                .map(Value::Duration)
                .unwrap_or(Value::Null),
        },
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dql::parse;
    use vault::FileMeta;

    fn meta(path: &str) -> FileMeta {
        FileMeta { path: path.into(), sha: "s".into(), size: 0 }
    }

    /// A small vault modelled on the doc's motivating example.
    fn vault() -> Index {
        let files: Vec<FileMeta> = [
            "games/Deus Ex.md",
            "games/Thief.md",
            "games/Pong.md",
            "notes/Alice.md",
            "daily/2026-08-01.md",
        ]
        .iter()
        .map(|p| meta(p))
        .collect();

        let mut idx = Index::new();
        idx.apply(
            vec![
                (
                    "games/Deus Ex.md".into(),
                    "---\ntags: [games, rpg]\nrating: 9\nlength: 20\nreleased: 2000-06-23\n---\n\
                     Played with [[Alice]].\n- [ ] replay 📅 2026-09-01\n- [x] finish\n"
                        .into(),
                ),
                (
                    "games/Thief.md".into(),
                    "---\ntags: [games]\nrating: 8\nlength: 15\n---\nLinks to [[Deus Ex]].\n\
                     - [ ] play again\n"
                        .into(),
                ),
                (
                    "games/Pong.md".into(),
                    "---\ntags: [games]\nrating: 4\n---\nNo frills.\n".into(),
                ),
                ("notes/Alice.md".into(), "A person who likes [[Deus Ex]].\n".into()),
                ("daily/2026-08-01.md".into(), "Daily note.\n".into()),
            ],
            &files,
        );
        idx
    }

    fn ctx() -> Context {
        Context {
            current_path: "notes/Alice.md".into(),
            today: Date::parse("2026-08-01"),
        }
    }

    fn run(src: &str) -> QueryResult {
        let q = parse(src).unwrap_or_else(|e| panic!("parse {src:?}: {e}"));
        execute(&q, &vault(), &ctx())
    }

    fn paths(r: &QueryResult) -> Vec<String> {
        match r {
            QueryResult::Table(t) => t.rows.iter().map(|(p, _)| p.clone()).collect(),
            QueryResult::List(l) => l.iter().map(|(p, _)| p.clone()).collect(),
            QueryResult::Tasks(t) => t.iter().map(|t| t.path.clone()).collect(),
        }
    }

    #[test]
    fn the_motivating_query_produces_the_expected_table() {
        let r = run("table length, rating\nfrom \"games\"\nsort rating desc");
        let QueryResult::Table(t) = &r else { panic!("expected a table") };
        assert_eq!(t.headers, vec!["File", "length", "rating"]);
        assert_eq!(
            paths(&r),
            vec!["games/Deus Ex.md", "games/Thief.md", "games/Pong.md"],
            "sorted by rating, descending"
        );
        // First row: link, length, rating.
        assert_eq!(t.rows[0].1[0], Value::Link("Deus Ex".into()));
        assert_eq!(t.rows[0].1[1], Value::Num(20.0));
        assert_eq!(t.rows[0].1[2], Value::Num(9.0));
        // Pong has no `length`; the cell is null, not an error.
        assert_eq!(t.rows[2].1[1], Value::Null);
    }

    #[test]
    fn without_id_drops_the_file_column() {
        let QueryResult::Table(t) = run("table without id rating from \"games\"") else {
            panic!()
        };
        assert_eq!(t.headers, vec!["rating"]);
        assert_eq!(t.rows[0].1.len(), 1);
    }

    #[test]
    fn where_filters_on_typed_frontmatter() {
        // `rating > 8` only works because 9 parsed as a number, not "9".
        assert_eq!(paths(&run("list from \"games\" where rating > 8")), vec!["games/Deus Ex.md"]);
        assert_eq!(
            paths(&run("list from \"games\" where rating >= 8")),
            vec!["games/Deus Ex.md", "games/Thief.md"]
        );
    }

    #[test]
    fn missing_fields_are_null_and_fail_comparisons_quietly() {
        // Pong has no `length`, so it must not sneak into either result.
        assert_eq!(paths(&run("list from \"games\" where length > 0")).len(), 2);
        assert_eq!(paths(&run("list from \"games\" where length < 1000")).len(), 2);
        assert_eq!(paths(&run("list from \"games\" where length")).len(), 2);
    }

    #[test]
    fn sources_select_by_folder_tag_and_link() {
        assert_eq!(paths(&run("list from \"games\"")).len(), 3);
        assert_eq!(paths(&run("list from #rpg")), vec!["games/Deus Ex.md"]);
        // Pages linking TO Deus Ex.
        assert_eq!(
            paths(&run("list from [[Deus Ex]]")),
            vec!["games/Thief.md", "notes/Alice.md"]
        );
        // Pages Deus Ex links to.
        assert_eq!(paths(&run("list from outgoing([[Deus Ex]])")), vec!["notes/Alice.md"]);
    }

    #[test]
    fn source_combinators_intersect_and_exclude() {
        assert_eq!(paths(&run("list from #games and #rpg")), vec!["games/Deus Ex.md"]);
        assert_eq!(
            paths(&run("list from #games -#rpg")),
            vec!["games/Pong.md", "games/Thief.md"]
        );
        assert_eq!(paths(&run("list from #rpg or \"notes\"")).len(), 2);
    }

    #[test]
    fn implicit_file_fields_resolve() {
        let QueryResult::Table(t) =
            run("table without id file.name, file.folder, file.day from \"daily\"")
        else {
            panic!()
        };
        assert_eq!(t.rows[0].1[0], Value::Str("2026-08-01".into()));
        assert_eq!(t.rows[0].1[1], Value::Str("daily".into()));
        // file.day is parsed out of the filename — the daily-note convention.
        assert_eq!(t.rows[0].1[2], Value::Date(Date::parse("2026-08-01").unwrap()));
    }

    #[test]
    fn this_refers_to_the_note_holding_the_query() {
        // Alice's own outlinks, via `this`.
        let QueryResult::Table(t) = run("table without id this.file.name from \"notes\"") else {
            panic!()
        };
        assert_eq!(t.rows[0].1[0], Value::Str("Alice".into()));
    }

    #[test]
    fn inlinks_and_outlinks_are_available_as_lists() {
        let QueryResult::Table(t) =
            run("table without id length(file.inlinks) from \"games\" where file.name = \"Deus Ex\"")
        else {
            panic!()
        };
        assert_eq!(t.rows[0].1[0], Value::Num(2.0), "Thief and Alice link here");
    }

    #[test]
    fn date_arithmetic_drives_due_date_queries() {
        // Every game due within a week of "today" (2026-08-01) — none — versus
        // within two months — the replay task's page.
        let soon = run("list from \"games\" where released < date(today)");
        assert_eq!(paths(&soon), vec!["games/Deus Ex.md"], "released in 2000");

        let never = run("list from \"games\" where released > date(today) + dur(7)");
        assert!(never.is_empty());
    }

    #[test]
    fn task_queries_collect_tasks_from_matching_pages() {
        let QueryResult::Tasks(tasks) = run("task from \"games\"") else { panic!() };
        assert_eq!(tasks.len(), 3);
        // Open tasks first (tasks::cmp), so the completed one is last.
        assert!(!tasks[0].checked);
        assert!(tasks[2].checked);
    }

    #[test]
    fn limit_truncates_after_sorting() {
        let r = run("list from \"games\" sort rating desc limit 2");
        assert_eq!(paths(&r), vec!["games/Deus Ex.md", "games/Thief.md"]);
    }

    #[test]
    fn functions_cover_the_common_cases() {
        let QueryResult::Table(t) = run(
            "table without id \
             length(tags) as A, \
             contains(tags, \"rpg\") as B, \
             upper(file.name) as C, \
             default(length, 0) as D, \
             choice(rating > 5, \"good\", \"bad\") as E, \
             round(rating / 3, 2) as F \
             from \"games\" where file.name = \"Pong\"",
        ) else {
            panic!()
        };
        let row = &t.rows[0].1;
        assert_eq!(row[0], Value::Num(1.0));
        assert_eq!(row[1], Value::Bool(false));
        assert_eq!(row[2], Value::Str("PONG".into()));
        assert_eq!(row[3], Value::Num(0.0), "default() fills the missing length");
        assert_eq!(row[4], Value::Str("bad".into()));
        assert_eq!(row[5], Value::Num(1.33));
    }

    #[test]
    fn unknown_functions_and_fields_yield_null_not_chaos() {
        let QueryResult::Table(t) =
            run("table without id nosuchfn(1), nosuchfield from \"games\"")
        else {
            panic!()
        };
        assert_eq!(t.rows[0].1, vec![Value::Null, Value::Null]);
        assert_eq!(t.rows.len(), 3, "every page still renders");
    }

    #[test]
    fn division_by_zero_is_null_rather_than_infinity() {
        let QueryResult::Table(t) =
            run("table without id rating / 0 from \"games\" where file.name = \"Pong\"")
        else {
            panic!()
        };
        assert_eq!(t.rows[0].1[0], Value::Null);
    }

    #[test]
    fn default_ordering_is_stable_by_path() {
        assert_eq!(
            paths(&run("list from \"games\"")),
            vec!["games/Deus Ex.md", "games/Pong.md", "games/Thief.md"]
        );
    }
}
