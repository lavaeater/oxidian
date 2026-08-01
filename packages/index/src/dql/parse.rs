//! Hand-written lexer + recursive-descent parser for DQL.
//!
//! No parser generator: the grammar is small, and a hand-written parser is the
//! only way to produce the error messages a live-typed query needs.

use std::fmt;

use crate::value::Value;

use super::{BinOp, Expr, Query, QueryKind, Sort, Source};

#[derive(Clone, Debug, PartialEq)]
pub struct ParseError {
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

fn err<T>(msg: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError { message: msg.into() })
}

// ── Lexer ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    /// Bare word — keyword or identifier; keywords are matched case-insensitively.
    Word(String),
    Str(String),
    Num(f64),
    /// `[[Note]]`
    Link(String),
    /// `#tag`
    Tag(String),
    Op(&'static str),
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Dot,
}

fn lex(src: &str) -> Result<Vec<Tok>, ParseError> {
    let b: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // `[[Note]]` before `[`, so links beat list literals.
        if c == '[' && b.get(i + 1) == Some(&'[') {
            let rest: String = b[i + 2..].iter().collect();
            let Some(end) = rest.find("]]") else {
                return err("unterminated [[link]]");
            };
            let inner = &rest[..end];
            let target = inner.split('|').next().unwrap_or(inner).trim();
            out.push(Tok::Link(target.to_string()));
            i += 2 + rest[..end].chars().count() + 2;
            continue;
        }
        match c {
            '(' => { out.push(Tok::LParen); i += 1; }
            ')' => { out.push(Tok::RParen); i += 1; }
            '[' => { out.push(Tok::LBracket); i += 1; }
            ']' => { out.push(Tok::RBracket); i += 1; }
            ',' => { out.push(Tok::Comma); i += 1; }
            '.' => { out.push(Tok::Dot); i += 1; }
            '"' | '\'' => {
                let quote = c;
                let mut s = String::new();
                i += 1;
                while i < b.len() && b[i] != quote {
                    s.push(b[i]);
                    i += 1;
                }
                if i >= b.len() {
                    return err(format!("unterminated string, expected a closing {quote}"));
                }
                i += 1;
                out.push(Tok::Str(s));
            }
            '#' => {
                let mut s = String::new();
                i += 1;
                while i < b.len() && (b[i].is_alphanumeric() || "-_/".contains(b[i])) {
                    s.push(b[i]);
                    i += 1;
                }
                if s.is_empty() {
                    return err("expected a tag name after '#'");
                }
                out.push(Tok::Tag(s));
            }
            '>' | '<' | '!' | '=' => {
                let two = b.get(i + 1) == Some(&'=');
                let op = match (c, two) {
                    ('>', true) => ">=", ('>', false) => ">",
                    ('<', true) => "<=", ('<', false) => "<",
                    ('!', true) => "!=",
                    ('!', false) => return err("expected '!=' — '!' alone is not an operator"),
                    ('=', _) => "=",
                    _ => unreachable!(),
                };
                out.push(Tok::Op(op));
                i += if two || op == "=" && b.get(i + 1) == Some(&'=') { 2 } else { 1 };
            }
            '+' | '-' | '*' | '/' | '%' => {
                let op: &'static str = match c {
                    '+' => "+", '-' => "-", '*' => "*", '/' => "/", _ => "%",
                };
                out.push(Tok::Op(op));
                i += 1;
            }
            _ if c.is_ascii_digit() => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_digit() || b[i] == '.') {
                    // A '.' only continues the number if a digit follows.
                    if b[i] == '.' && !b.get(i + 1).is_some_and(|d| d.is_ascii_digit()) {
                        break;
                    }
                    i += 1;
                }
                let s: String = b[start..i].iter().collect();
                match s.parse::<f64>() {
                    Ok(n) => out.push(Tok::Num(n)),
                    Err(_) => return err(format!("'{s}' is not a number")),
                }
            }
            _ if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < b.len() && (b[i].is_alphanumeric() || b[i] == '_' || b[i] == '-') {
                    i += 1;
                }
                out.push(Tok::Word(b[start..i].iter().collect()));
            }
            _ => return err(format!("unexpected character '{c}'")),
        }
    }
    Ok(out)
}

// ── Parser ───────────────────────────────────────────────────────────────────

/// Clause keywords, which also delimit the token span of the previous clause.
const CLAUSES: [&str; 5] = ["from", "where", "sort", "limit", "group"];

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn peek_word(&self) -> Option<String> {
        match self.peek() {
            Some(Tok::Word(w)) => Some(w.to_lowercase()),
            _ => None,
        }
    }

    fn eat_word(&mut self, want: &str) -> bool {
        if self.peek_word().as_deref() == Some(want) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn at_clause(&self) -> bool {
        self.peek().is_none() || self.peek_word().is_some_and(|w| CLAUSES.contains(&w.as_str()))
    }

    // ── Expressions (precedence climbing) ────────────────────────────────────

    fn expr(&mut self) -> Result<Expr, ParseError> {
        self.or_expr()
    }

    fn or_expr(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.and_expr()?;
        while self.peek_word().as_deref() == Some("or") {
            self.pos += 1;
            let rhs = self.and_expr()?;
            lhs = Expr::Bin(BinOp::Or, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn and_expr(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.cmp_expr()?;
        while self.peek_word().as_deref() == Some("and") {
            self.pos += 1;
            let rhs = self.cmp_expr()?;
            lhs = Expr::Bin(BinOp::And, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn cmp_expr(&mut self) -> Result<Expr, ParseError> {
        let lhs = self.add_expr()?;
        let op = match self.peek() {
            Some(Tok::Op("=")) => BinOp::Eq,
            Some(Tok::Op("!=")) => BinOp::Ne,
            Some(Tok::Op("<")) => BinOp::Lt,
            Some(Tok::Op("<=")) => BinOp::Le,
            Some(Tok::Op(">")) => BinOp::Gt,
            Some(Tok::Op(">=")) => BinOp::Ge,
            _ => return Ok(lhs),
        };
        self.pos += 1;
        let rhs = self.add_expr()?;
        Ok(Expr::Bin(op, Box::new(lhs), Box::new(rhs)))
    }

    fn add_expr(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.mul_expr()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Op("+")) => BinOp::Add,
                Some(Tok::Op("-")) => BinOp::Sub,
                _ => return Ok(lhs),
            };
            self.pos += 1;
            let rhs = self.mul_expr()?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
    }

    fn mul_expr(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.unary()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Op("*")) => BinOp::Mul,
                Some(Tok::Op("/")) => BinOp::Div,
                Some(Tok::Op("%")) => BinOp::Mod,
                _ => return Ok(lhs),
            };
            self.pos += 1;
            let rhs = self.unary()?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        if self.peek_word().as_deref() == Some("not") {
            self.pos += 1;
            return Ok(Expr::Not(Box::new(self.unary()?)));
        }
        if self.peek() == Some(&Tok::Op("-")) {
            self.pos += 1;
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        self.postfix()
    }

    /// `a.b.c` — field access binds tighter than any operator.
    fn postfix(&mut self) -> Result<Expr, ParseError> {
        let mut e = self.primary()?;
        while self.peek() == Some(&Tok::Dot) {
            self.pos += 1;
            match self.next() {
                Some(Tok::Word(w)) => e = Expr::Field(Box::new(e), w),
                _ => return err("expected a field name after '.'"),
            }
        }
        Ok(e)
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        match self.next() {
            Some(Tok::Num(n)) => Ok(Expr::Lit(Value::Num(n))),
            Some(Tok::Str(s)) => Ok(Expr::Lit(Value::Str(s))),
            Some(Tok::Link(l)) => Ok(Expr::Lit(Value::Link(l))),
            Some(Tok::Tag(t)) => Ok(Expr::Lit(Value::Str(t))),
            Some(Tok::LParen) => {
                let e = self.expr()?;
                if self.next() != Some(Tok::RParen) {
                    return err("expected ')'");
                }
                Ok(e)
            }
            Some(Tok::LBracket) => {
                let mut items = Vec::new();
                if self.peek() != Some(&Tok::RBracket) {
                    loop {
                        items.push(self.expr()?);
                        if self.peek() == Some(&Tok::Comma) {
                            self.pos += 1;
                        } else {
                            break;
                        }
                    }
                }
                if self.next() != Some(Tok::RBracket) {
                    return err("expected ']'");
                }
                Ok(Expr::ListLit(items))
            }
            Some(Tok::Word(w)) => {
                let lower = w.to_lowercase();
                match lower.as_str() {
                    "true" => return Ok(Expr::Lit(Value::Bool(true))),
                    "false" => return Ok(Expr::Lit(Value::Bool(false))),
                    "null" => return Ok(Expr::Lit(Value::Null)),
                    _ => {}
                }
                if self.peek() == Some(&Tok::LParen) {
                    self.pos += 1;
                    let mut args = Vec::new();
                    if self.peek() != Some(&Tok::RParen) {
                        loop {
                            args.push(self.expr()?);
                            if self.peek() == Some(&Tok::Comma) {
                                self.pos += 1;
                            } else {
                                break;
                            }
                        }
                    }
                    if self.next() != Some(Tok::RParen) {
                        return err(format!("expected ')' to close {lower}("));
                    }
                    return Ok(Expr::Call(lower, args));
                }
                Ok(Expr::Var(w))
            }
            Some(t) => err(format!("unexpected {t:?} in expression")),
            None => err("unexpected end of query"),
        }
    }

    // ── Sources ─────────────────────────────────────────────────────────────

    fn source(&mut self) -> Result<Source, ParseError> {
        let mut lhs = self.source_and()?;
        while self.peek_word().as_deref() == Some("or") {
            self.pos += 1;
            let rhs = self.source_and()?;
            lhs = Source::Or(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn source_and(&mut self) -> Result<Source, ParseError> {
        let mut lhs = self.source_atom()?;
        loop {
            // `and`, or a bare `-x` meaning "and not x".
            if self.peek_word().as_deref() == Some("and") {
                self.pos += 1;
                let rhs = self.source_atom()?;
                lhs = Source::And(Box::new(lhs), Box::new(rhs));
            } else if self.peek() == Some(&Tok::Op("-")) {
                self.pos += 1;
                let rhs = self.source_atom()?;
                lhs = Source::And(Box::new(lhs), Box::new(Source::Not(Box::new(rhs))));
            } else {
                return Ok(lhs);
            }
        }
    }

    fn source_atom(&mut self) -> Result<Source, ParseError> {
        if self.peek_word().as_deref() == Some("not") {
            self.pos += 1;
            return Ok(Source::Not(Box::new(self.source_atom()?)));
        }
        if self.peek() == Some(&Tok::Op("-")) {
            self.pos += 1;
            return Ok(Source::Not(Box::new(self.source_atom()?)));
        }
        match self.next() {
            Some(Tok::Str(s)) => Ok(Source::Folder(s)),
            Some(Tok::Tag(t)) => Ok(Source::Tag(t)),
            Some(Tok::Link(l)) => Ok(Source::LinkedTo(l)),
            Some(Tok::LParen) => {
                let s = self.source()?;
                if self.next() != Some(Tok::RParen) {
                    return err("expected ')' in FROM");
                }
                Ok(s)
            }
            Some(Tok::Word(w)) if w.eq_ignore_ascii_case("outgoing") => {
                if self.next() != Some(Tok::LParen) {
                    return err("expected '(' after outgoing");
                }
                let target = match self.next() {
                    Some(Tok::Link(l)) => l,
                    Some(Tok::Str(s)) => s,
                    _ => return err("outgoing() takes a [[link]]"),
                };
                if self.next() != Some(Tok::RParen) {
                    return err("expected ')' after outgoing(...)");
                }
                Ok(Source::Outgoing(target))
            }
            Some(t) => err(format!(
                "expected a FROM source — \"folder\", #tag, [[link]], or outgoing([[link]]) — got {t:?}"
            )),
            None => err("FROM needs a source: \"folder\", #tag, or [[link]]"),
        }
    }
}

/// Parse a `dataview` block body into a [`Query`].
pub fn parse(src: &str) -> Result<Query, ParseError> {
    let toks = lex(src)?;
    if toks.is_empty() {
        return err("empty query — start with TABLE, LIST, or TASK");
    }
    let mut p = Parser { toks, pos: 0 };

    let kind = match p.peek_word().as_deref() {
        Some("table") => {
            p.pos += 1;
            let without_id = p.eat_word("without") && p.eat_word("id");
            let mut columns = Vec::new();
            while !p.at_clause() {
                let e = p.expr()?;
                let header = if p.eat_word("as") {
                    match p.next() {
                        Some(Tok::Str(s)) => s,
                        Some(Tok::Word(w)) => w,
                        _ => return err("expected a column name after AS"),
                    }
                } else {
                    default_header(&e)
                };
                columns.push((e, header));
                if p.peek() == Some(&Tok::Comma) {
                    p.pos += 1;
                } else {
                    break;
                }
            }
            QueryKind::Table { columns, without_id }
        }
        Some("list") => {
            p.pos += 1;
            if p.at_clause() {
                QueryKind::List(None)
            } else {
                QueryKind::List(Some(p.expr()?))
            }
        }
        Some("task") => {
            p.pos += 1;
            QueryKind::Task
        }
        Some(other) => {
            return err(format!(
                "'{other}' is not a query type — expected TABLE, LIST, or TASK"
            ));
        }
        None => return err("expected TABLE, LIST, or TASK"),
    };

    let mut q = Query { kind, from: None, filters: Vec::new(), sorts: Vec::new(), limit: None };

    while let Some(word) = p.peek_word() {
        match word.as_str() {
            "from" => {
                p.pos += 1;
                if q.from.is_some() {
                    return err("only one FROM clause is allowed");
                }
                q.from = Some(p.source()?);
            }
            "where" => {
                p.pos += 1;
                q.filters.push(p.expr()?);
            }
            "sort" => {
                p.pos += 1;
                loop {
                    let expr = p.expr()?;
                    let descending = if p.eat_word("desc") {
                        true
                    } else {
                        p.eat_word("asc");
                        false
                    };
                    q.sorts.push(Sort { expr, descending });
                    if p.peek() == Some(&Tok::Comma) {
                        p.pos += 1;
                    } else {
                        break;
                    }
                }
            }
            "limit" => {
                p.pos += 1;
                match p.next() {
                    Some(Tok::Num(n)) if n >= 0.0 => q.limit = Some(n as usize),
                    _ => return err("LIMIT takes a non-negative number"),
                }
            }
            "group" => {
                return err("GROUP BY is not supported yet");
            }
            other => {
                return err(format!("unexpected '{other}' — expected FROM, WHERE, SORT, or LIMIT"));
            }
        }
    }

    if p.pos < p.toks.len() {
        return err(format!("unexpected trailing input: {:?}", p.toks[p.pos]));
    }
    Ok(q)
}

/// The header shown for a column with no `AS` — the source text, near enough.
fn default_header(e: &Expr) -> String {
    match e {
        Expr::Var(v) => v.clone(),
        Expr::Field(base, name) => format!("{}.{}", default_header(base), name),
        Expr::Call(name, args) => {
            let inner: Vec<String> = args.iter().map(default_header).collect();
            format!("{name}({})", inner.join(", "))
        }
        Expr::Lit(v) => v.to_display(),
        _ => "expr".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(src: &str) -> Query {
        parse(src).unwrap_or_else(|e| panic!("parse failed for {src:?}: {e}"))
    }

    #[test]
    fn parses_the_motivating_query() {
        // The example from docs/dataview.md §1.
        let parsed = q("table time-played, length, rating\nfrom \"games\"\nsort rating desc");
        let QueryKind::Table { columns, without_id } = &parsed.kind else {
            panic!("expected a table");
        };
        assert!(!without_id);
        let headers: Vec<&str> = columns.iter().map(|(_, h)| h.as_str()).collect();
        assert_eq!(headers, vec!["time-played", "length", "rating"]);
        assert_eq!(parsed.from, Some(Source::Folder("games".into())));
        assert_eq!(parsed.sorts.len(), 1);
        assert!(parsed.sorts[0].descending);
    }

    #[test]
    fn keywords_are_case_insensitive() {
        let a = q("TABLE rating FROM \"games\"");
        let b = q("table rating from \"games\"");
        assert_eq!(a, b);
    }

    #[test]
    fn table_columns_take_aliases() {
        let parsed = q("table rating as \"Score\", file.name as Note");
        let QueryKind::Table { columns, .. } = &parsed.kind else { panic!() };
        assert_eq!(columns[0].1, "Score");
        assert_eq!(columns[1].1, "Note");
        assert_eq!(columns[1].0, Expr::Field(Box::new(Expr::Var("file".into())), "name".into()));
    }

    #[test]
    fn table_without_id_is_recognised() {
        let parsed = q("table without id rating from #games");
        let QueryKind::Table { without_id, columns } = &parsed.kind else { panic!() };
        assert!(without_id);
        assert_eq!(columns.len(), 1);
    }

    #[test]
    fn list_and_task_forms() {
        assert_eq!(q("list").kind, QueryKind::List(None));
        assert_eq!(q("LIST file.name").kind, QueryKind::List(Some(Expr::Field(
            Box::new(Expr::Var("file".into())),
            "name".into()
        ))));
        assert_eq!(q("task from #todo").kind, QueryKind::Task);
    }

    #[test]
    fn sources_compose_with_and_or_and_negation() {
        assert_eq!(q("list from #games").from, Some(Source::Tag("games".into())));
        assert_eq!(q("list from [[Deus Ex]]").from, Some(Source::LinkedTo("Deus Ex".into())));
        assert_eq!(
            q("list from outgoing([[Deus Ex]])").from,
            Some(Source::Outgoing("Deus Ex".into()))
        );
        assert_eq!(
            q("list from #games and \"rpg\"").from,
            Some(Source::And(
                Box::new(Source::Tag("games".into())),
                Box::new(Source::Folder("rpg".into()))
            ))
        );
        // A bare `-source` means "and not".
        assert_eq!(
            q("list from #games -#done").from,
            Some(Source::And(
                Box::new(Source::Tag("games".into())),
                Box::new(Source::Not(Box::new(Source::Tag("done".into()))))
            ))
        );
    }

    #[test]
    fn operator_precedence_follows_arithmetic_convention() {
        // rating > 5 and length < 3  ==  (rating > 5) and (length < 3)
        let parsed = q("list where rating > 5 and length < 3");
        let Expr::Bin(BinOp::And, l, r) = &parsed.filters[0] else {
            panic!("expected AND at the top: {:?}", parsed.filters[0])
        };
        assert!(matches!(**l, Expr::Bin(BinOp::Gt, _, _)));
        assert!(matches!(**r, Expr::Bin(BinOp::Lt, _, _)));

        // 1 + 2 * 3 groups as 1 + (2 * 3)
        let parsed = q("list where 1 + 2 * 3");
        let Expr::Bin(BinOp::Add, _, r) = &parsed.filters[0] else { panic!() };
        assert!(matches!(**r, Expr::Bin(BinOp::Mul, _, _)));
    }

    #[test]
    fn multiple_where_clauses_accumulate() {
        let parsed = q("list where rating > 5 where status = \"done\"");
        assert_eq!(parsed.filters.len(), 2);
    }

    #[test]
    fn sort_takes_several_keys_with_directions() {
        let parsed = q("table rating sort rating desc, file.name asc");
        assert_eq!(parsed.sorts.len(), 2);
        assert!(parsed.sorts[0].descending);
        assert!(!parsed.sorts[1].descending);
    }

    #[test]
    fn limit_is_parsed() {
        assert_eq!(q("list limit 10").limit, Some(10));
    }

    #[test]
    fn errors_are_values_with_useful_messages() {
        // A live-typed query must never panic; every one of these is a state
        // the editor will pass us mid-keystroke.
        for (src, needle) in [
            ("", "empty query"),
            ("select * from notes", "not a query type"),
            ("table rating from", "FROM needs a source"),
            ("list where (rating > 5", "expected ')'"),
            ("list where rating > \"unterminated", "unterminated string"),
            ("list from [[Unclosed", "unterminated [[link]]"),
            ("list limit many", "LIMIT takes a non-negative number"),
            ("list group by rating", "GROUP BY is not supported yet"),
        ] {
            let e = parse(src).expect_err(&format!("{src:?} should not parse"));
            assert!(
                e.message.contains(needle),
                "for {src:?} expected a message containing {needle:?}, got {:?}",
                e.message
            );
        }
    }

    #[test]
    fn function_calls_and_list_literals_parse() {
        let parsed = q("list where contains(tags, \"rpg\") and contains([8, 9], rating)");
        assert!(matches!(parsed.filters[0], Expr::Bin(BinOp::And, _, _)));
        let parsed = q("table length(tasks) as Count");
        let QueryKind::Table { columns, .. } = &parsed.kind else { panic!() };
        assert_eq!(columns[0].1, "Count");
    }
}
