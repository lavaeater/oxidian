//! Wikilink target resolution and the path a *new* note gets when a link has no
//! target yet.
//!
//! Pure string work, no I/O — the vault is passed in as a list of paths — so the
//! rules below are unit-testable and behave identically on web, desktop and
//! mobile.
//!
//! Resolution is deliberately more lenient than creation. A bare `[[Ideas]]`
//! resolves to an `Ideas.md` *anywhere* in the vault (Obsidian's shortest-path
//! matching), because that is what a reader means by the link. Creation cannot
//! be lenient — it has to pick exactly one path — so it follows the explicit
//! rule in [`new_note_path`].

/// Strips a `#heading` / `^block-id` suffix and surrounding whitespace, leaving
/// the note part of a wikilink target.
///
/// `[[Note#Section]]` and `[[Note]]` point at the same note; only the scroll
/// position differs, and that is not our concern here.
fn note_part(target: &str) -> &str {
    let t = target.trim();
    let end = t.find(['#', '^']).unwrap_or(t.len());
    t[..end].trim_end()
}

/// Adds `.md` unless the target already carries an extension we should respect.
fn with_md(path: &str) -> String {
    if path.to_lowercase().ends_with(".md") {
        path.to_string()
    } else {
        format!("{path}.md")
    }
}

/// Collapses `//`, resolves `.`/`..` segments, and drops any leading slash, so
/// the result is always a clean vault-relative path.
fn normalize(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

/// Joins a directory and a (possibly nested) relative target.
fn join(dir: &str, rel: &str) -> String {
    if dir.is_empty() {
        normalize(rel)
    } else {
        normalize(&format!("{dir}/{rel}"))
    }
}

/// The path a note created from `target` should get, per the vault's link rules:
///
/// - `[[Ideas]]` → beside the current note (`notes/Ideas.md` from `notes/a.md`).
/// - `[[sub/Ideas]]` → the path is taken as relative to the current folder, so
///   `notes/sub/Ideas.md`. The folder is created implicitly by the write.
/// - `[[/sub/Ideas]]` → a leading slash means "from the vault root", giving
///   `sub/Ideas.md` no matter where the link was written.
///
/// `current_dir` is the folder of the note holding the link (`""` at the root).
pub fn new_note_path(target: &str, current_dir: &str) -> String {
    let note = note_part(target);
    if let Some(from_root) = note.strip_prefix('/') {
        with_md(&normalize(from_root))
    } else {
        with_md(&join(current_dir, note))
    }
}

/// Resolves a wikilink target to a path that exists in `files`, or `None` when
/// the link points at a note that hasn't been written yet.
///
/// Explicit paths win over name matches, and matching is case-insensitive
/// throughout (a vault is edited on several machines; case drift is normal).
pub fn resolve(target: &str, files: &[String], current_dir: &str) -> Option<String> {
    let note = note_part(target);
    if note.is_empty() {
        return None;
    }

    // An explicit path — rooted or nested — means exactly that path, tried in
    // the same order `new_note_path` would have created it.
    let mut candidates: Vec<String> = Vec::new();
    if let Some(from_root) = note.strip_prefix('/') {
        candidates.push(with_md(&normalize(from_root)));
    } else {
        candidates.push(with_md(&join(current_dir, note)));
        if note.contains('/') {
            // A nested target that isn't beside this note may still be written
            // from the root, which is how such links usually read.
            candidates.push(with_md(&normalize(note)));
        }
    }
    for cand in &candidates {
        if let Some(hit) = files.iter().find(|f| f.eq_ignore_ascii_case(cand)) {
            return Some(hit.clone());
        }
    }

    // Bare name: match the filename anywhere in the vault. Prefer the shallowest
    // hit so a link resolves predictably when the name repeats across folders.
    if note.contains('/') || note.starts_with('/') {
        return None;
    }
    let wanted = with_md(note).to_lowercase();
    files
        .iter()
        .filter(|f| {
            f.rsplit('/')
                .next()
                .unwrap_or(f)
                .to_lowercase()
                .eq(&wanted)
        })
        .min_by_key(|f| (f.matches('/').count(), f.len()))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> Vec<String> {
        vec![
            "Inbox.md".to_string(),
            "notes/Ideas.md".to_string(),
            "notes/sub/Deep.md".to_string(),
            "archive/Ideas.md".to_string(),
        ]
    }

    // ── new_note_path ──

    #[test]
    fn bare_target_lands_beside_the_current_note() {
        assert_eq!(new_note_path("Ideas", "notes"), "notes/Ideas.md");
        assert_eq!(new_note_path("Ideas", ""), "Ideas.md");
    }

    #[test]
    fn nested_target_is_relative_to_the_current_folder() {
        assert_eq!(new_note_path("sub/Ideas", "notes"), "notes/sub/Ideas.md");
        assert_eq!(new_note_path("a/b/C", ""), "a/b/C.md");
    }

    #[test]
    fn leading_slash_means_from_the_vault_root() {
        assert_eq!(new_note_path("/sub/Ideas", "notes"), "sub/Ideas.md");
        assert_eq!(new_note_path("/Ideas", "deep/folder"), "Ideas.md");
    }

    #[test]
    fn heading_and_block_suffixes_are_not_part_of_the_path() {
        assert_eq!(new_note_path("Ideas#Later", "notes"), "notes/Ideas.md");
        assert_eq!(new_note_path("Ideas ^ref", "notes"), "notes/Ideas.md");
    }

    #[test]
    fn existing_md_extension_is_not_doubled() {
        assert_eq!(new_note_path("Ideas.md", "notes"), "notes/Ideas.md");
        assert_eq!(new_note_path("Ideas.MD", ""), "Ideas.MD");
    }

    #[test]
    fn dot_segments_are_resolved() {
        assert_eq!(new_note_path("../Ideas", "notes/sub"), "notes/Ideas.md");
        assert_eq!(new_note_path("./Ideas", "notes"), "notes/Ideas.md");
    }

    // ── resolve ──

    #[test]
    fn bare_name_matches_anywhere_in_the_vault() {
        assert_eq!(
            resolve("Deep", &vault(), ""),
            Some("notes/sub/Deep.md".to_string())
        );
    }

    #[test]
    fn bare_name_prefers_the_shallowest_match() {
        // "Ideas" exists twice and neither sits beside the linking note, so the
        // shallower path wins (ties break on length).
        let files = vec!["deep/a/Ideas.md".to_string(), "Ideas.md".to_string()];
        assert_eq!(resolve("Ideas", &files, "elsewhere"), Some("Ideas.md".to_string()));
    }

    #[test]
    fn a_note_beside_the_current_one_wins_over_a_distant_match() {
        assert_eq!(
            resolve("Ideas", &vault(), "archive"),
            Some("archive/Ideas.md".to_string())
        );
    }

    #[test]
    fn rooted_target_ignores_the_current_folder() {
        assert_eq!(
            resolve("/notes/Ideas", &vault(), "archive"),
            Some("notes/Ideas.md".to_string())
        );
        assert_eq!(resolve("/Ideas", &vault(), "notes"), None);
    }

    #[test]
    fn nested_target_tries_relative_then_root() {
        assert_eq!(
            resolve("sub/Deep", &vault(), "notes"),
            Some("notes/sub/Deep.md".to_string())
        );
        // Not beside the current note, but present from the root.
        assert_eq!(
            resolve("notes/sub/Deep", &vault(), "archive"),
            Some("notes/sub/Deep.md".to_string())
        );
    }

    #[test]
    fn matching_ignores_case() {
        assert_eq!(resolve("inbox", &vault(), ""), Some("Inbox.md".to_string()));
        assert_eq!(
            resolve("/NOTES/ideas", &vault(), ""),
            Some("notes/Ideas.md".to_string())
        );
    }

    #[test]
    fn heading_suffix_still_resolves_the_note() {
        assert_eq!(resolve("Inbox#Today", &vault(), ""), Some("Inbox.md".to_string()));
    }

    #[test]
    fn missing_and_empty_targets_do_not_resolve() {
        assert_eq!(resolve("Nowhere", &vault(), ""), None);
        assert_eq!(resolve("", &vault(), ""), None);
        assert_eq!(resolve("   ", &vault(), ""), None);
        assert_eq!(resolve("#OnlyHeading", &vault(), ""), None);
    }
}
