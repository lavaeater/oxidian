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

/// What an entry is, and where it stands — the Bullet Journal *signifier*.
///
/// Stored as the character between the checkbox brackets, so a journal stays
/// ordinary Markdown that Obsidian and a text editor can both read. The
/// characters follow the Obsidian-Tasks conventions rather than inventing our
/// own, so vaults stay portable. See `docs/bujo-roadmap.md` §3.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Status {
    /// `- [ ]` — awaiting a decision.
    #[default]
    Open,
    /// `- [x]` — completed.
    Done,
    /// `- [>]` — moved forward into the next log.
    Migrated,
    /// `- [<]` — deferred to a specific future date.
    Scheduled,
    /// `- [-]` — consciously abandoned. Deliberately still visible.
    Dropped,
    /// `- [o]` — an event: something that happened, not something to do.
    Event,
}

impl Status {
    /// The character between the brackets.
    pub fn marker(self) -> char {
        match self {
            Status::Open => ' ',
            Status::Done => 'x',
            Status::Migrated => '>',
            Status::Scheduled => '<',
            Status::Dropped => '-',
            Status::Event => 'o',
        }
    }

    /// Parse a marker character. Returns `None` for anything unrecognised —
    /// `- [q] thing` has to stay a plain list item, or every stray bracket
    /// becomes an entry with a mystery state.
    pub fn from_marker(c: char) -> Option<Self> {
        match c {
            ' ' => Some(Status::Open),
            'x' | 'X' => Some(Status::Done),
            '>' => Some(Status::Migrated),
            '<' => Some(Status::Scheduled),
            '-' => Some(Status::Dropped),
            'o' | 'O' => Some(Status::Event),
            _ => None,
        }
    }

    /// The glyph a bullet journalist would draw.
    pub fn glyph(self) -> &'static str {
        match self {
            Status::Open => "•",
            Status::Done => "✗",
            Status::Migrated => "›",
            Status::Scheduled => "‹",
            Status::Dropped => "–",
            Status::Event => "○",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::Done => "done",
            Status::Migrated => "migrated",
            Status::Scheduled => "scheduled",
            Status::Dropped => "dropped",
            Status::Event => "event",
        }
    }

    /// Still awaiting a decision — the set the migration review sweeps up.
    ///
    /// An event is *not* open: it records something that happened, so there is
    /// nothing to carry forward.
    pub fn is_open(self) -> bool {
        self == Status::Open
    }

    /// Resolved in some way — but note that "resolved" is not "done". Keeping
    /// migrated and dropped distinct from done is the whole point of the
    /// review: `docs/bujo-roadmap.md` §5.
    pub fn is_closed(self) -> bool {
        !self.is_open()
    }

    /// View order: what still needs doing first, what was abandoned last.
    pub fn rank(self) -> u8 {
        match self {
            Status::Open => 0,
            Status::Scheduled => 1,
            Status::Migrated => 2,
            Status::Event => 3,
            Status::Done => 4,
            Status::Dropped => 5,
        }
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Task {
    pub path: String,
    /// 0-based line index in the source file.
    pub line: usize,
    pub status: Status,
    /// Display text with the metadata emoji stripped out.
    pub text: String,
    /// The original source line, used to locate it again for write-back.
    pub raw: String,
    pub due: Option<String>,  // YYYY-MM-DD
    pub done: Option<String>, // YYYY-MM-DD
    pub priority: Priority,
}

impl Task {
    /// Completed. Not the same as "no longer open" — see [`Status::is_closed`].
    pub fn is_done(&self) -> bool {
        self.status == Status::Done
    }

    /// Still awaiting a decision.
    pub fn is_open(&self) -> bool {
        self.status.is_open()
    }
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
    // `[<marker>] ` — one character between brackets, then a space. An
    // unrecognised marker is not a task at all (see `Status::from_marker`).
    let mut chars = after_bullet.chars();
    if chars.next() != Some('[') {
        return None;
    }
    let status = Status::from_marker(chars.next()?)?;
    if chars.next() != Some(']') || chars.next() != Some(' ') {
        return None;
    }
    // Every accepted marker is ASCII, so the prefix is exactly 4 bytes.
    let rest = &after_bullet[4..];

    let (due, rest) = extract_dated(rest, "📅");
    let (done, rest) = extract_dated(&rest, "✅");
    let (priority, rest) = extract_priority(&rest);
    let text = normalize_ws(&rest);

    Some(Task {
        path: path.to_string(),
        line: idx,
        status,
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
    let current = marker_status(lines[idx])?;
    // Clicking a checkbox means "done", whatever it was — except when it is
    // already done, which means "not after all".
    let target = if current == Status::Done { Status::Open } else { Status::Done };
    set_status_content(content, task, target, today)
}

/// Returns `content` with the given task's marker set to `status`, stamping or
/// removing the `✅ <today>` done-date to match, or `None` if the line can't be
/// located.
///
/// This is the one write path for changing an entry's state: the toggle above
/// and the migration review (`docs/bujo-roadmap.md` §5) both go through it, so
/// the done-stamp can only ever be attached to a genuinely-done entry.
pub fn set_status_content(
    content: &str,
    task: &Task,
    status: Status,
    today: &str,
) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    let idx = locate(&lines, task)?;
    let new_line = restamped(lines[idx], status, today)?;

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
            .is_some_and(|t| t.text == task.text)
    };
    if lines.get(task.line).is_some_and(|l| matches(l)) {
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

/// The byte position of a line's `[<marker>]`, and the status it encodes.
fn marker_at(line: &str) -> Option<(usize, Status)> {
    let bytes = line.as_bytes();
    let pos = line.find('[')?;
    if bytes.get(pos + 2) != Some(&b']') {
        return None;
    }
    let status = Status::from_marker(*bytes.get(pos + 1)? as char)?;
    Some((pos, status))
}

/// The status a line's checkbox currently encodes.
fn marker_status(line: &str) -> Option<Status> {
    marker_at(line).map(|(_, s)| s)
}

/// A line re-marked to `status`, with the `✅ <today>` stamp attached or removed
/// to match. `None` when the line has no checkbox to re-mark.
fn restamped(line: &str, status: Status, today: &str) -> Option<String> {
    let new_line = set_marker(line, status)?;
    if status == Status::Done {
        if new_line.contains("✅") || today.is_empty() {
            Some(new_line)
        } else {
            Some(format!("{} ✅ {today}", new_line.trim_end()))
        }
    } else {
        Some(strip_done(&new_line))
    }
}

/// Rewrite a line's `[<marker>]` to `status`, leaving everything else — indent,
/// text, due date, priority, tags — exactly as it was.
fn set_marker(line: &str, status: Status) -> Option<String> {
    let (pos, _) = marker_at(line)?;
    Some(format!(
        "{}[{}]{}",
        &line[..pos],
        status.marker(),
        &line[pos + 3..]
    ))
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
            .is_some_and(|t| t.text == task.text)
    };
    if !taken.contains(&task.line)
        && lines.get(task.line).is_some_and(|l| matches(l))
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

/// Re-mark several of one file's entries in a single rewrite.
///
/// The migration review resolves a whole period at once, and writing per entry
/// would mean one request — and one SHA round trip — per line. Entries that
/// can't be found are skipped rather than invented, as everywhere else here.
///
/// Two entries with identical text map to two different lines, never the same
/// line twice.
pub fn set_status_many(content: &str, tasks: &[Task], status: Status, today: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut taken: Vec<usize> = Vec::new();
    for task in tasks {
        if let Some(idx) = locate_excluding(&lines, task, &taken) {
            taken.push(idx);
        }
    }

    let mut out = String::new();
    for (i, l) in lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if taken.contains(&i) {
            out.push_str(&restamped(l, status, today).unwrap_or_else(|| (*l).to_string()));
        } else {
            out.push_str(l);
        }
    }
    if content.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    out
}

/// The line to write into the *destination* when an entry is carried forward.
///
/// Migration re-marks the original and writes a fresh copy onward — it never
/// moves the line — so the carried copy has to come back to `Open` and shed the
/// completion stamp. `due` sets (or replaces) the `📅` date, which is what
/// scheduling an entry into the Future Log means.
///
/// Everything else rides along untouched: indent, priority, tags, text.
pub fn carried_line(task: &Task, due: Option<&str>) -> String {
    let line = set_marker(&task.raw, Status::Open).unwrap_or_else(|| task.raw.clone());
    let line = strip_done(&line);
    match due {
        Some(d) => {
            let (_, without) = extract_dated(&line, "📅");
            format!("{} 📅 {d}", without.trim_end())
        }
        None => line,
    }
}

/// Sort order for the view: open tasks first, then by due date (earliest first,
/// undated last), then priority, then text.
pub fn cmp(a: &Task, b: &Task) -> std::cmp::Ordering {
    a.status
        .rank()
        .cmp(&b.status.rank())
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
        assert!(!t.is_done());
        assert_eq!(t.text, "Pay rent");
        assert_eq!(t.due.as_deref(), Some("2026-06-15"));
        assert_eq!(t.priority, Priority::High);
    }

    #[test]
    fn parses_done_and_checked() {
        let t = &parse_file("a.md", "* [x] Ship it ✅ 2026-06-10")[0];
        assert!(t.is_done());
        assert_eq!(t.text, "Ship it");
        assert_eq!(t.done.as_deref(), Some("2026-06-10"));
    }

    #[test]
    fn ignores_non_tasks() {
        assert!(parse_file("a.md", "- just a bullet\n# heading\nplain").is_empty());
    }

    // ── Bullet Journal signifiers (docs/bujo-roadmap.md §3) ──────────────────

    #[test]
    fn parses_every_signifier() {
        let src = "\
- [ ] open
- [x] done
- [>] migrated
- [<] scheduled
- [-] dropped
- [o] event";
        let got: Vec<Status> = parse_file("a.md", src).iter().map(|t| t.status).collect();
        assert_eq!(
            got,
            [
                Status::Open,
                Status::Done,
                Status::Migrated,
                Status::Scheduled,
                Status::Dropped,
                Status::Event
            ]
        );
    }

    #[test]
    fn an_unknown_marker_is_not_a_task_at_all() {
        // Otherwise every stray bracket becomes an entry in a mystery state.
        assert!(parse_file("a.md", "- [q] what even is this").is_empty());
        assert!(parse_file("a.md", "- [] no marker").is_empty());
        assert!(parse_file("a.md", "- [ ]no space after").is_empty());
    }

    #[test]
    fn uppercase_markers_are_accepted() {
        // Obsidian writes `[x]`, but plenty of tools and humans write `[X]`.
        assert_eq!(parse_file("a.md", "- [X] done")[0].status, Status::Done);
        assert_eq!(parse_file("a.md", "- [O] event")[0].status, Status::Event);
    }

    #[test]
    fn only_open_entries_are_open() {
        // The migration review sweeps up exactly these, so the distinction is
        // load-bearing: a migrated task is resolved, not outstanding.
        let src = "- [ ] a\n- [x] b\n- [>] c\n- [<] d\n- [-] e\n- [o] f";
        let tasks = parse_file("a.md", src);
        assert_eq!(tasks.iter().filter(|t| t.is_open()).count(), 1);
        assert_eq!(tasks.iter().filter(|t| t.is_done()).count(), 1);
        // ...and "resolved" is not "done" — that gap is the point of the review.
        assert!(tasks[2].status.is_closed() && !tasks[2].is_done());
    }

    #[test]
    fn setting_a_status_rewrites_only_the_marker() {
        let src = "  - [ ] indented 📅 2026-06-15 ⏫ #tag";
        let t = &parse_file("a.md", src)[0];
        let out = set_status_content(src, t, Status::Migrated, "2026-08-13").unwrap();
        assert_eq!(out, "  - [>] indented 📅 2026-06-15 ⏫ #tag");
    }

    #[test]
    fn only_a_done_entry_carries_a_done_date() {
        let src = "- [ ] thing";
        let t = &parse_file("a.md", src)[0];

        let done = set_status_content(src, t, Status::Done, "2026-08-13").unwrap();
        assert_eq!(done, "- [x] thing ✅ 2026-08-13");

        // Moving on from done takes the stamp with it — a migrated entry that
        // still claimed a completion date would be a lie.
        let done_task = &parse_file("a.md", &done)[0];
        let migrated = set_status_content(&done, done_task, Status::Migrated, "2026-08-14").unwrap();
        assert_eq!(migrated, "- [>] thing");
    }

    #[test]
    fn toggling_any_unfinished_state_completes_it() {
        for (src, expected) in [
            ("- [ ] a", "- [x] a ✅ 2026-08-13"),
            ("- [>] a", "- [x] a ✅ 2026-08-13"),
            ("- [-] a", "- [x] a ✅ 2026-08-13"),
        ] {
            let t = &parse_file("a.md", src)[0];
            assert_eq!(toggled_content(src, t, "2026-08-13").unwrap(), expected, "{src}");
        }
        // ...and toggling a done one reopens it.
        let src = "- [x] a ✅ 2026-08-13";
        let t = &parse_file("a.md", src)[0];
        assert_eq!(toggled_content(src, t, "2026-08-13").unwrap(), "- [ ] a");
    }

    // ── Migration (docs/bujo-roadmap.md §5) ─────────────────────────────────

    #[test]
    fn migration_re_marks_the_original_instead_of_removing_it() {
        // The whole difference between migrating and moving: the source log
        // still shows what was on it, and that it moved on.
        let src = "# Monday\n\n- [ ] call the plumber\n- [ ] pay rent\n- [x] done\n";
        let open: Vec<Task> = parse_file("a.md", src).into_iter().filter(Task::is_open).collect();
        let out = set_status_many(src, &open, Status::Migrated, "2026-08-13");
        assert_eq!(
            out,
            "# Monday\n\n- [>] call the plumber\n- [>] pay rent\n- [x] done\n"
        );
    }

    #[test]
    fn a_carried_line_arrives_open_and_unstamped() {
        let src = "  - [x] ship it ⏫ #work ✅ 2026-08-01";
        let t = &parse_file("a.md", src)[0];
        // Indent, priority and tags ride along; the state does not.
        assert_eq!(carried_line(t, None), "  - [ ] ship it ⏫ #work");
    }

    #[test]
    fn scheduling_sets_the_due_date_replacing_any_existing_one() {
        let t = &parse_file("a.md", "- [ ] book flights 📅 2026-08-01")[0];
        assert_eq!(
            carried_line(t, Some("2026-12-24")),
            "- [ ] book flights 📅 2026-12-24"
        );
        let undated = &parse_file("a.md", "- [ ] book flights")[0];
        assert_eq!(
            carried_line(undated, Some("2026-12-24")),
            "- [ ] book flights 📅 2026-12-24"
        );
    }

    #[test]
    fn a_full_migration_round_trip_leaves_both_logs_correct() {
        let monday = "# Monday\n\n- [ ] call the plumber 📅 2026-08-12\n- [x] pay rent ✅ 2026-08-12\n";
        let open: Vec<Task> = parse_file("mon.md", monday).into_iter().filter(Task::is_open).collect();

        let carried: Vec<String> = open.iter().map(|t| carried_line(t, None)).collect();
        let tuesday = append_lines("# Tuesday\n", &carried);
        let monday_after = set_status_many(monday, &open, Status::Migrated, "2026-08-13");

        // Forward: open again, in the new log, with its due date intact.
        assert_eq!(tuesday, "# Tuesday\n\n- [ ] call the plumber 📅 2026-08-12\n");
        // Behind: the record of what happened, including that it moved.
        assert_eq!(
            monday_after,
            "# Monday\n\n- [>] call the plumber 📅 2026-08-12\n- [x] pay rent ✅ 2026-08-12\n"
        );
        // Nothing is open in the old log any more, so a second review of Monday
        // sweeps up nothing and can't duplicate the entry.
        assert!(parse_file("mon.md", &monday_after).iter().all(|t| !t.is_open()));
    }

    #[test]
    fn two_entries_with_the_same_text_map_to_two_different_lines() {
        let src = "- [ ] follow up\n- [ ] follow up\n";
        let tasks = parse_file("a.md", src);
        assert_eq!(tasks.len(), 2);
        let out = set_status_many(src, &tasks, Status::Dropped, "");
        assert_eq!(out, "- [-] follow up\n- [-] follow up\n");
    }

    #[test]
    fn dropping_an_entry_never_stamps_it_as_completed() {
        // A dropped task is abandoned, not finished — a ✅ date would be a lie,
        // and phase 6's statistics read exactly this distinction.
        let src = "- [x] gave up on this ✅ 2026-08-01";
        let t = &parse_file("a.md", src)[0];
        let out = set_status_many(src, std::slice::from_ref(t), Status::Dropped, "2026-08-13");
        assert_eq!(out, "- [-] gave up on this");
    }

    #[test]
    fn open_entries_sort_ahead_of_resolved_ones() {
        let src = "- [x] done\n- [-] dropped\n- [ ] open\n- [>] migrated";
        let mut tasks = parse_file("a.md", src);
        tasks.sort_by(cmp);
        let order: Vec<&str> = tasks.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(order, ["open", "migrated", "done", "dropped"]);
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
        let open: Vec<Task> = parse_file("a.md", src).into_iter().filter(Task::is_open).collect();
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
