use serde::Deserialize;

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct TemplateMeta {
    pub name: String,
    pub source_path: String,
    pub filepath: Option<String>,
    pub body: String,
}

// ── Date variables ────────────────────────────────────────────────────────────
//
// The date variables themselves are produced by `js::date_vars()` (see
// `assets/oxidian.js`) and parsed into `TemplateVars` below.

pub struct TemplateVars {
    pub year: String,
    pub year_short: String,
    pub month: String,
    pub month_name: String,
    pub date: String,
    pub day_name: String,
    pub week: String,
    pub title: String,
    pub title_safe: String,
    pub current_dir: String,
}

impl TemplateVars {
    pub fn from_json(json: &str, title: &str, current_dir: &str) -> Self {
        #[derive(Deserialize, Default)]
        struct DateParts {
            #[serde(default)] year: String,
            #[serde(rename = "yearShort", default)] year_short: String,
            #[serde(default)] month: String,
            #[serde(rename = "monthName", default)] month_name: String,
            #[serde(default)] date: String,
            #[serde(rename = "dayName", default)] day_name: String,
            #[serde(default)] week: String,
        }
        let parts: DateParts = serde_json::from_str(json).unwrap_or_default();
        let title_safe = title
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' { c.to_ascii_lowercase() } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        TemplateVars {
            year: parts.year,
            year_short: parts.year_short,
            month: parts.month,
            month_name: parts.month_name,
            date: parts.date,
            day_name: parts.day_name,
            week: parts.week,
            title: title.to_string(),
            title_safe,
            current_dir: current_dir.to_string(),
        }
    }
}

// ── Variable substitution ─────────────────────────────────────────────────────

pub fn substitute_vars(content: &str, v: &TemplateVars) -> String {
    content
        // Oxidian date vars
        .replace("${OXID_DATE_YEAR}",       &v.year)
        .replace("${OXID_DATE_YEAR_SHORT}", &v.year_short)
        .replace("${OXID_DATE_MONTH}",      &v.month)
        .replace("${OXID_DATE_MONTH_NAME}", &v.month_name)
        .replace("${OXID_DATE_DATE}",       &v.date)
        .replace("${OXID_DATE_DAY_NAME}",   &v.day_name)
        .replace("${OXID_DATE_WEEK}",       &v.week)
        // Oxidian path/title vars (brace and bare forms)
        .replace("${OXID_TITLE}",           &v.title)
        .replace("${OXID_TITLE_SAFE}",      &v.title_safe)
        .replace("${OXID_CURRENT_DIR}",     &v.current_dir)
        .replace("$OXID_TITLE_SAFE",        &v.title_safe)
        .replace("$OXID_TITLE",             &v.title)
        .replace("$OXID_CURRENT_DIR",       &v.current_dir)
        // Foam compatibility aliases
        .replace("${FOAM_DATE_YEAR}",       &v.year)
        .replace("${FOAM_DATE_YEAR_SHORT}", &v.year_short)
        .replace("${FOAM_DATE_MONTH}",      &v.month)
        .replace("${FOAM_DATE_MONTH_NAME}", &v.month_name)
        .replace("${FOAM_DATE_DATE}",       &v.date)
        .replace("${FOAM_DATE_DAY_NAME}",   &v.day_name)
        .replace("${FOAM_DATE_WEEK}",       &v.week)
        .replace("${FOAM_TITLE}",           &v.title)
        .replace("${FOAM_TITLE_SAFE}",      &v.title_safe)
        .replace("${FOAM_CURRENT_DIR}",     &v.current_dir)
        .replace("$FOAM_TITLE_SAFE",        &v.title_safe)
        .replace("$FOAM_TITLE",             &v.title)
        .replace("$FOAM_CURRENT_DIR",       &v.current_dir)
        // VS Code snippet vars
        .replace("${CURRENT_YEAR}",         &v.year)
        .replace("${CURRENT_MONTH}",        &v.month)
        .replace("${CURRENT_DATE}",         &v.date)
}

// ── Tabstop stripping ─────────────────────────────────────────────────────────

/// Replaces VS Code tabstops `${N:placeholder}` with their placeholder text.
/// `${N}` (empty tabstops) are removed entirely.
pub fn strip_tabstops(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'$' && i + 1 < b.len() && b[i + 1] == b'{' {
            let mut j = i + 2;
            while j < b.len() && b[j].is_ascii_digit() { j += 1; }
            if j > i + 2 && j < b.len() {
                if b[j] == b':' {
                    j += 1;
                    let start = j;
                    let mut depth = 1usize;
                    while j < b.len() {
                        match b[j] {
                            b'{' => depth += 1,
                            b'}' => { depth -= 1; if depth == 0 { break; } }
                            _ => {}
                        }
                        j += 1;
                    }
                    out.push_str(&s[start..j]);
                    i = j + 1;
                    continue;
                } else if b[j] == b'}' {
                    i = j + 1;
                    continue;
                }
            }
        }
        let ch = s[i..].chars().next().unwrap_or('\u{FFFD}');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

// ── Parsing ───────────────────────────────────────────────────────────────────

pub fn parse_template(source_path: &str, raw: &str) -> TemplateMeta {
    let mut filepath = None;
    let mut description = None;
    let body;

    if raw.starts_with("---") {
        if let Some(rel) = raw[3..].find("\n---") {
            let yaml = &raw[3..3 + rel];
            let after = &raw[3 + rel + 4..];
            body = after.trim_start_matches('\n').to_string();

            let mut in_template_block = false;
            for line in yaml.lines() {
                let trimmed = line.trim();
                // Recognize both oxid_template (primary) and foam_template (compat)
                if trimmed == "oxid_template:" || trimmed == "foam_template:" {
                    in_template_block = true;
                    continue;
                }
                if in_template_block {
                    if line.starts_with("  ") || line.starts_with('\t') {
                        if let Some(v) = trimmed.strip_prefix("filepath:") {
                            filepath = Some(v.trim().trim_matches('"').trim_matches('\'').to_string());
                        } else if let Some(v) = trimmed.strip_prefix("description:") {
                            description = Some(v.trim().trim_matches('"').trim_matches('\'').to_string());
                        }
                    } else {
                        in_template_block = false;
                    }
                }
            }
        } else {
            body = raw.to_string();
        }
    } else {
        body = raw.to_string();
    }

    let name = description.unwrap_or_else(|| {
        source_path
            .rsplit('/')
            .next()
            .unwrap_or(source_path)
            .trim_end_matches(".md")
            .replace('-', " ")
    });

    TemplateMeta { name, source_path: source_path.to_string(), filepath, body }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> TemplateVars {
        TemplateVars {
            year: "2026".into(),
            year_short: "26".into(),
            month: "07".into(),
            month_name: "July".into(),
            date: "2026-07-21".into(),
            day_name: "Tuesday".into(),
            week: "30".into(),
            title: "My Note".into(),
            title_safe: "my-note".into(),
            current_dir: "notes".into(),
        }
    }

    #[test]
    fn substitutes_oxid_brace_and_bare_forms() {
        let v = vars();
        assert_eq!(substitute_vars("${OXID_DATE_YEAR}", &v), "2026");
        assert_eq!(substitute_vars("${OXID_TITLE}", &v), "My Note");
        assert_eq!(substitute_vars("$OXID_TITLE", &v), "My Note");
        // The longer bare token must win over its prefix.
        assert_eq!(substitute_vars("$OXID_TITLE_SAFE", &v), "my-note");
        assert_eq!(substitute_vars("$OXID_CURRENT_DIR", &v), "notes");
    }

    #[test]
    fn substitutes_foam_and_vscode_aliases() {
        let v = vars();
        assert_eq!(substitute_vars("${FOAM_DATE_MONTH_NAME}", &v), "July");
        assert_eq!(substitute_vars("${FOAM_TITLE}", &v), "My Note");
        assert_eq!(substitute_vars("${CURRENT_YEAR}", &v), "2026");
        assert_eq!(substitute_vars("${CURRENT_DATE}", &v), "2026-07-21");
    }

    #[test]
    fn substitutes_multiple_occurrences_in_context() {
        let v = vars();
        let out = substitute_vars("# ${OXID_TITLE}\nweek ${OXID_DATE_WEEK} of ${OXID_DATE_YEAR}", &v);
        assert_eq!(out, "# My Note\nweek 30 of 2026");
    }

    #[test]
    fn strip_tabstops_replaces_placeholder_and_removes_empty() {
        assert_eq!(strip_tabstops("Hello ${1:World}"), "Hello World");
        assert_eq!(strip_tabstops("cursor${0}here"), "cursorhere");
        assert_eq!(strip_tabstops("${2:a} and ${3:b}"), "a and b");
    }

    #[test]
    fn strip_tabstops_only_strips_outer_of_nested() {
        // Depth tracking keeps the inner tabstop text verbatim.
        assert_eq!(strip_tabstops("${1:a${2:b}c}"), "a${2:b}c");
    }

    #[test]
    fn strip_tabstops_leaves_non_tabstops_untouched() {
        assert_eq!(strip_tabstops("price $5 and ${notavar}"), "price $5 and ${notavar}");
        assert_eq!(strip_tabstops("plain text"), "plain text");
    }

    #[test]
    fn from_json_parses_dates_and_slugifies_title() {
        let json = r#"{"year":"2026","yearShort":"26","month":"07","monthName":"July","date":"2026-07-21","dayName":"Tuesday","week":"30"}"#;
        let v = TemplateVars::from_json(json, "My Note!! Draft", "notes/daily");
        assert_eq!(v.year, "2026");
        assert_eq!(v.month_name, "July");
        assert_eq!(v.day_name, "Tuesday");
        assert_eq!(v.title, "My Note!! Draft");
        // Non-alphanumerics collapse to single dashes, no leading/trailing dash.
        assert_eq!(v.title_safe, "my-note-draft");
        assert_eq!(v.current_dir, "notes/daily");
    }

    #[test]
    fn from_json_tolerates_bad_json() {
        let v = TemplateVars::from_json("not json", "Title", "");
        assert_eq!(v.year, "");
        assert_eq!(v.title, "Title");
        assert_eq!(v.title_safe, "title");
    }

    #[test]
    fn parse_template_reads_frontmatter_block() {
        let raw = "---\noxid_template:\n  filepath: \"notes/${OXID_TITLE_SAFE}.md\"\n  description: \"Daily note\"\n---\n# Body here\n";
        let meta = parse_template("templates/daily-note.md", raw);
        assert_eq!(meta.name, "Daily note");
        assert_eq!(meta.filepath, Some("notes/${OXID_TITLE_SAFE}.md".to_string()));
        assert_eq!(meta.body, "# Body here\n");
        assert_eq!(meta.source_path, "templates/daily-note.md");
    }

    #[test]
    fn parse_template_recognizes_foam_block() {
        let raw = "---\nfoam_template:\n  filepath: out.md\n---\nbody";
        let meta = parse_template("t/x.md", raw);
        assert_eq!(meta.filepath, Some("out.md".to_string()));
        assert_eq!(meta.body, "body");
    }

    #[test]
    fn parse_template_name_falls_back_to_filename() {
        let raw = "# no frontmatter";
        let meta = parse_template("templates/weekly-review.md", raw);
        // Dashes become spaces, extension stripped, no description present.
        assert_eq!(meta.name, "weekly review");
        assert_eq!(meta.filepath, None);
        assert_eq!(meta.body, "# no frontmatter");
    }

    #[test]
    fn parse_template_without_closing_fence_keeps_raw_body() {
        let raw = "---\noxid_template:\n  filepath: x.md\nno closing fence";
        let meta = parse_template("t/a.md", raw);
        assert_eq!(meta.body, raw);
        assert_eq!(meta.filepath, None);
    }
}
