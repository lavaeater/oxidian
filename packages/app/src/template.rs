use serde::Deserialize;

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct TemplateMeta {
    pub name: String,
    pub source_path: String,
    pub filepath: Option<String>,
    pub body: String,
}

// ── Date variables via JS ─────────────────────────────────────────────────────

pub const JS_DATE_VARS: &str = r#"
(function() {
    const d = new Date();
    const months = ['January','February','March','April','May','June',
                    'July','August','September','October','November','December'];
    const days = ['Sunday','Monday','Tuesday','Wednesday','Thursday','Friday','Saturday'];
    const pad = n => String(n).padStart(2, '0');
    const jan4 = new Date(d.getFullYear(), 0, 4);
    const dow = jan4.getDay() || 7;
    const weekStart = new Date(jan4);
    weekStart.setDate(jan4.getDate() - dow + 1);
    const week = Math.max(1, Math.ceil((d - weekStart) / 604800000) + 1);
    dioxus.send(JSON.stringify({
        year:      String(d.getFullYear()),
        yearShort: String(d.getFullYear()).slice(-2),
        month:     pad(d.getMonth() + 1),
        monthName: months[d.getMonth()],
        date:      pad(d.getDate()),
        dayName:   days[d.getDay()],
        week:      pad(week)
    }));
})();
"#;

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
        out.push(b[i] as char);
        i += 1;
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
