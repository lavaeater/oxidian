//! Vault-wide task aggregation.
//!
//! Scans every markdown file for checkbox tasks (`- [ ]` / `- [x]`, also `*`/`+`
//! bullets) and parses the Obsidian-Tasks emoji metadata we care about:
//! due 📅, done ✅, and priority (🔺 highest, ⏫ high, 🔼 medium, 🔽 low, ⏬ lowest).
//! The editor's tokenizer handles rendering; this is a separate, line-based
//! parser for the aggregated Tasks view.

use serde::{Deserialize, Serialize};

const PRIO_EMOJI: [(&str, Priority); 5] = [
    ("🔺", Priority::Highest),
    ("⏫", Priority::High),
    ("🔼", Priority::Medium),
    ("🔽", Priority::Low),
    ("⏬", Priority::Lowest),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Priority {
    Highest,
    High,
    Medium,
    Low,
    Lowest,
    None,
}

impl Priority {
    pub fn emoji(self) -> &'static str {
        match self {
            Priority::Highest => "🔺",
            Priority::High => "⏫",
            Priority::Medium => "🔼",
            Priority::Low => "🔽",
            Priority::Lowest => "⏬",
            Priority::None => "",
        }
    }
    /// Lower rank = more important (used for sorting).
    pub fn rank(self) -> u8 {
        match self {
            Priority::Highest => 0,
            Priority::High => 1,
            Priority::Medium => 2,
            Priority::None => 3, // unmarked sorts between medium and low, like Obsidian
            Priority::Low => 4,
            Priority::Lowest => 5,
        }
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Task {
    pub path: String,
    /// 0-based line index in the source file.
    pub line: usize,
    pub checked: bool,
    /// Display text with the metadata emoji stripped out.
    pub text: String,
    /// The original source line, used to locate it again for write-back.
    pub raw: String,
    pub due: Option<String>,  // YYYY-MM-DD
    pub done: Option<String>, // YYYY-MM-DD
    pub priority: Priority,
}

/// Parse all checkbox tasks out of one file's content.
pub fn parse_file(path: &str, content: &str) -> Vec<Task> {
    content
        .lines()
        .enumerate()
        .filter_map(|(i, line)| parse_line(path, i, line))
        .collect()
}

fn parse_line(path: &str, idx: usize, line: &str) -> Option<Task> {
    let trimmed = line.trim_start();
    let after_bullet = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .or_else(|| trimmed.strip_prefix("+ "))?;
    let (checked, rest) = if let Some(r) = after_bullet.strip_prefix("[ ] ") {
        (false, r)
    } else if let Some(r) = after_bullet
        .strip_prefix("[x] ")
        .or_else(|| after_bullet.strip_prefix("[X] "))
    {
        (true, r)
    } else {
        return None;
    };

    let (due, rest) = extract_dated(rest, "📅");
    let (done, rest) = extract_dated(&rest, "✅");
    let (priority, rest) = extract_priority(&rest);
    let text = normalize_ws(&rest);

    Some(Task {
        path: path.to_string(),
        line: idx,
        checked,
        text,
        raw: line.to_string(),
        due,
        done,
        priority,
    })
}

/// Find `<emoji> YYYY-MM-DD`, returning the date and the text with that span removed.
fn extract_dated(text: &str, emoji: &str) -> (Option<String>, String) {
    if let Some(pos) = text.find(emoji) {
        let after = &text[pos + emoji.len()..];
        let after_trim = after.trim_start();
        let skipped = after.len() - after_trim.len();
        let date: String = after_trim.chars().take(10).collect();
        if is_ymd(&date) {
            let remove_end = pos + emoji.len() + skipped + date.len();
            let cleaned = format!("{}{}", &text[..pos], &text[remove_end..]);
            return (Some(date), cleaned);
        }
    }
    (None, text.to_string())
}

fn extract_priority(text: &str) -> (Priority, String) {
    for (emoji, prio) in PRIO_EMOJI {
        if let Some(pos) = text.find(emoji) {
            let cleaned = format!("{}{}", &text[..pos], &text[pos + emoji.len()..]);
            return (prio, cleaned);
        }
    }
    (Priority::None, text.to_string())
}

fn is_ymd(s: &str) -> bool {
    let b = s.as_bytes();
    s.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 { *c == b'-' } else { c.is_ascii_digit() }
        })
}

fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Returns `content` with the given task's checkbox flipped and its done-date
/// (`✅ <today>`) stamped/removed to match, or `None` if the line can't be
/// located. Locates the line by index, falling back to a parsed-text match — so
/// it stays correct even after a previous toggle added/removed the ✅ stamp.
pub fn toggled_content(content: &str, task: &Task, today: &str) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    let idx = locate(&lines, task)?;
    // The new state is derived from the file's *current* line, not the (possibly
    // stale) scanned task, so rapid re-toggles stay consistent.
    let now_checked = lines[idx].contains("[ ]");
    let mut new_line = flip_checkbox(lines[idx])?;
    if now_checked {
        if !new_line.contains("✅") && !today.is_empty() {
            new_line = format!("{} ✅ {today}", new_line.trim_end());
        }
    } else {
        new_line = strip_done(&new_line);
    }

    let mut out = String::new();
    for (i, l) in lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if i == idx {
            out.push_str(&new_line);
        } else {
            out.push_str(l);
        }
    }
    if content.ends_with('\n') {
        out.push('\n');
    }
    Some(out)
}

/// Find the task's line: prefer its original index, else the first line whose
/// parsed task text matches (ignoring checkbox state and metadata).
fn locate(lines: &[&str], task: &Task) -> Option<usize> {
    let matches = |line: &str| {
        parse_line(&task.path, 0, line)
            .map(|t| t.text == task.text)
            .unwrap_or(false)
    };
    if lines.get(task.line).map(|l| matches(l)).unwrap_or(false) {
        return Some(task.line);
    }
    lines.iter().position(|l| matches(l))
}

/// Remove a trailing `✅ <date>` (and the space before it) from a line.
fn strip_done(line: &str) -> String {
    if let Some(pos) = line.find("✅") {
        let after = &line[pos + "✅".len()..];
        let after_trim = after.trim_start();
        let skipped = after.len() - after_trim.len();
        let date: String = after_trim.chars().take(10).collect();
        let date_len = if is_ymd(&date) { date.len() } else { 0 };
        let remove_end = pos + "✅".len() + skipped + date_len;
        return format!("{}{}", line[..pos].trim_end(), &line[remove_end..]);
    }
    line.to_string()
}

fn flip_checkbox(line: &str) -> Option<String> {
    if let Some(pos) = line.find("[ ]") {
        return Some(format!("{}[x]{}", &line[..pos], &line[pos + 3..]));
    }
    if let Some(pos) = line.find("[x]").or_else(|| line.find("[X]")) {
        return Some(format!("{}[ ]{}", &line[..pos], &line[pos + 3..]));
    }
    None
}

/// Removes `tasks` from `content`, returning the remaining text and the raw
/// lines that were taken out (in file order).
///
/// Lines are located the same way [`toggled_content`] locates them — by index,
/// falling back to a parsed-text match — so a stale scan still finds the right
/// line. Tasks that can't be found are simply skipped: the caller is moving
/// lines between files, and inventing a line would be worse than moving fewer.
///
/// Indentation is preserved, because a task's indent carries meaning (a subtask
/// under a parent bullet), and so does whatever the line says after the
/// checkbox — due dates, priority, tags all ride along untouched.
pub fn extract_lines(content: &str, tasks: &[Task]) -> (String, Vec<String>) {
    let lines: Vec<&str> = content.lines().collect();
    let mut taken: Vec<usize> = Vec::new();
    for task in tasks {
        if let Some(idx) = locate_excluding(&lines, task, &taken) {
            taken.push(idx);
        }
    }
    taken.sort_unstable();

    let moved: Vec<String> = taken.iter().map(|&i| lines[i].to_string()).collect();
    let mut out = String::new();
    let mut first = true;
    for (i, l) in lines.iter().enumerate() {
        if taken.contains(&i) {
            continue;
        }
        if !first {
            out.push('\n');
        }
        out.push_str(l);
        first = false;
    }
    if content.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    (out, moved)
}

/// Like [`locate`], but never returns a line that was already claimed — two
/// tasks with identical text must map to two different lines, not the same one
/// twice.
fn locate_excluding(lines: &[&str], task: &Task, taken: &[usize]) -> Option<usize> {
    let matches = |line: &str| {
        parse_line(&task.path, 0, line)
            .map(|t| t.text == task.text)
            .unwrap_or(false)
    };
    if !taken.contains(&task.line)
        && lines.get(task.line).map(|l| matches(l)).unwrap_or(false)
    {
        return Some(task.line);
    }
    lines
        .iter()
        .enumerate()
        .find(|(i, l)| !taken.contains(i) && matches(l))
        .map(|(i, _)| i)
}

/// Appends task lines to the end of `content`, separated by a blank line when
/// the note already has body text.
///
/// Returns `content` unchanged when there is nothing to append, so a caller that
/// over-eagerly asks to move zero tasks doesn't dirty the file.
pub fn append_lines(content: &str, lines: &[String]) -> String {
    if lines.is_empty() {
        return content.to_string();
    }
    let body = content.trim_end_matches('\n');
    let mut out = String::with_capacity(content.len() + lines.len() * 40);
    if !body.trim().is_empty() {
        out.push_str(body);
        out.push_str("\n\n");
    }
    out.push_str(&lines.join("\n"));
    out.push('\n');
    out
}

/// Sort order for the view: open tasks first, then by due date (earliest first,
/// undated last), then priority, then text.
pub fn cmp(a: &Task, b: &Task) -> std::cmp::Ordering {
    a.checked
        .cmp(&b.checked)
        .then_with(|| match (&a.due, &b.due) {
            (Some(x), Some(y)) => x.cmp(y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        })
        .then_with(|| a.priority.rank().cmp(&b.priority.rank()))
        .then_with(|| a.text.cmp(&b.text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_metadata() {
        let t = &parse_file("notes/a.md", "- [ ] Pay rent 📅 2026-06-15 ⏫")[0];
        assert!(!t.checked);
        assert_eq!(t.text, "Pay rent");
        assert_eq!(t.due.as_deref(), Some("2026-06-15"));
        assert_eq!(t.priority, Priority::High);
    }

    #[test]
    fn parses_done_and_checked() {
        let t = &parse_file("a.md", "* [x] Ship it ✅ 2026-06-10")[0];
        assert!(t.checked);
        assert_eq!(t.text, "Ship it");
        assert_eq!(t.done.as_deref(), Some("2026-06-10"));
    }

    #[test]
    fn ignores_non_tasks() {
        assert!(parse_file("a.md", "- just a bullet\n# heading\nplain").is_empty());
    }

    #[test]
    fn toggle_checks_and_stamps_done() {
        let src = "# h\n- [ ] a\n- [ ] b\n";
        let task = &parse_file("a.md", src)[1];
        let out = toggled_content(src, task, "2026-06-11").unwrap();
        assert_eq!(out, "# h\n- [ ] a\n- [x] b ✅ 2026-06-11\n");
    }

    #[test]
    fn toggle_unchecks_and_removes_done() {
        let src = "- [x] done ✅ 2026-06-11\n";
        let task = &parse_file("a.md", src)[0];
        let out = toggled_content(src, task, "2026-06-11").unwrap();
        assert_eq!(out, "- [ ] done\n");
    }

    #[test]
    fn extract_pulls_task_lines_and_leaves_the_rest() {
        let src = "# Notes\n\n- [ ] one\nsome prose\n- [ ] two\n";
        let tasks = parse_file("a.md", src);
        let (rest, moved) = extract_lines(src, &tasks);
        assert_eq!(rest, "# Notes\n\nsome prose\n");
        assert_eq!(moved, vec!["- [ ] one", "- [ ] two"]);
    }

    #[test]
    fn extract_preserves_indentation_and_metadata() {
        let src = "- [ ] parent\n    - [ ] child 📅 2026-06-15 ⏫\n";
        let tasks = parse_file("a.md", src);
        let (_, moved) = extract_lines(src, &tasks[1..]);
        assert_eq!(moved, vec!["    - [ ] child 📅 2026-06-15 ⏫"]);
    }

    #[test]
    fn extract_returns_lines_in_file_order_not_argument_order() {
        let src = "- [ ] a\n- [ ] b\n- [ ] c\n";
        let all = parse_file("a.md", src);
        let scrambled = vec![all[2].clone(), all[0].clone()];
        let (rest, moved) = extract_lines(src, &scrambled);
        assert_eq!(moved, vec!["- [ ] a", "- [ ] c"]);
        assert_eq!(rest, "- [ ] b\n");
    }

    #[test]
    fn extract_maps_duplicate_texts_to_distinct_lines() {
        // Two identical tasks must take two lines, not the same line twice.
        let src = "- [ ] same\n- [ ] same\n- [ ] other\n";
        let tasks = parse_file("a.md", src);
        let (rest, moved) = extract_lines(src, &tasks[..2]);
        assert_eq!(moved, vec!["- [ ] same", "- [ ] same"]);
        assert_eq!(rest, "- [ ] other\n");
    }

    #[test]
    fn extract_skips_tasks_that_are_no_longer_there() {
        let src = "- [ ] still here\n";
        let ghost = parse_file("a.md", "- [ ] deleted elsewhere");
        let (rest, moved) = extract_lines(src, &ghost);
        assert_eq!(rest, src);
        assert!(moved.is_empty());
    }

    #[test]
    fn extract_can_empty_a_file() {
        let src = "- [ ] only\n";
        let tasks = parse_file("a.md", src);
        let (rest, moved) = extract_lines(src, &tasks);
        assert_eq!(rest, "");
        assert_eq!(moved.len(), 1);
    }

    #[test]
    fn append_separates_from_existing_body() {
        let out = append_lines("# Today\n", &["- [ ] a".to_string()]);
        assert_eq!(out, "# Today\n\n- [ ] a\n");
    }

    #[test]
    fn append_to_empty_note_adds_no_leading_blank_line() {
        assert_eq!(append_lines("", &["- [ ] a".to_string()]), "- [ ] a\n");
        assert_eq!(append_lines("\n\n", &["- [ ] a".to_string()]), "- [ ] a\n");
    }

    #[test]
    fn append_of_nothing_leaves_content_untouched() {
        assert_eq!(append_lines("# Today\n", &[]), "# Today\n");
    }

    #[test]
    fn extract_then_append_round_trips_a_move() {
        let src = "# Source\n\n- [ ] move me 📅 2026-06-15\n- [x] done\n";
        let open: Vec<Task> = parse_file("a.md", src).into_iter().filter(|t| !t.checked).collect();
        let (rest, moved) = extract_lines(src, &open);
        assert_eq!(rest, "# Source\n\n- [x] done\n");
        assert_eq!(
            append_lines("# Today\n", &moved),
            "# Today\n\n- [ ] move me 📅 2026-06-15\n"
        );
    }

    #[test]
    fn locate_survives_done_stamp_change() {
        // After a check added the stamp, a re-toggle must still find the line.
        let scanned = &parse_file("a.md", "- [ ] write tests")[0];
        let current = "- [x] write tests ✅ 2026-06-11\n";
        let out = toggled_content(current, scanned, "2026-06-11").unwrap();
        assert_eq!(out, "- [ ] write tests\n");
    }
}
