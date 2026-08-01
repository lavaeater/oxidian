//! YAML frontmatter: split, parse, and write back.
//!
//! Deliberately not a full YAML implementation — Obsidian frontmatter in
//! practice is scalars and flat lists (see `docs/dataview.md` §4.1). Values are
//! kept as strings for now; the typed `Value` model arrives with the query
//! evaluator in phase 2, and this module is where it will land so that the
//! properties editor and the index never disagree about what a note declares.

/// Returns `(frontmatter_text, body_after_fence)` if the content starts with `---`.
pub fn split_frontmatter(content: &str) -> Option<(&str, &str)> {
    let content = content.strip_prefix("---")?;
    // Accept `---\n` or `---\r\n`
    let content = content.strip_prefix('\n').or_else(|| content.strip_prefix("\r\n"))?;
    // Find the closing `---`
    for (i, line) in content.lines().enumerate() {
        if line.trim() == "---" {
            // Calculate byte offset of the end of this line
            let prefix_len: usize = content.lines().take(i).map(|l| l.len() + 1).sum();
            let fm = &content[..prefix_len.saturating_sub(1).min(content.len())];
            let rest_start = prefix_len + line.len() + 1;
            let rest = if rest_start <= content.len() { &content[rest_start..] } else { "" };
            return Some((fm, rest));
        }
    }
    None
}

/// Parse simple `key: value` pairs from YAML frontmatter.
/// Only handles string/number/boolean scalar values (not nested objects or arrays).
pub fn parse_pairs(fm: &str) -> Vec<(String, String)> {
    fm.lines()
        .filter_map(|line| {
            let (key, val) = line.split_once(':')?;
            let key = key.trim().to_string();
            let val = val.trim().trim_matches('"').trim_matches('\'').to_string();
            if key.is_empty() { return None; }
            Some((key, val))
        })
        .collect()
}

/// Serialise key-value pairs back to YAML frontmatter (simple scalar values only).
fn pairs_to_yaml(pairs: &[(String, String)]) -> String {
    pairs.iter()
        .filter(|(k, _)| !k.is_empty())
        .map(|(k, v)| {
            // Quote values that contain special chars
            if v.contains(':') || v.starts_with(['#', '[', '{', '\'', '"', '&', '*']) {
                format!("{k}: \"{v}\"")
            } else {
                format!("{k}: {v}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Rebuild content with updated frontmatter.
pub fn set_frontmatter(content: &str, pairs: &[(String, String)]) -> String {
    let yaml = pairs_to_yaml(pairs);
    match split_frontmatter(content) {
        Some((_, body)) => format!("---\n{yaml}\n---\n{body}"),
        None => format!("---\n{yaml}\n---\n\n{content}"),
    }
}

/// Set (or insert) a single frontmatter key, leaving every other key and the
/// body untouched. Used by the save path to stamp `updated:`.
pub fn set_key(content: &str, key: &str, value: &str) -> String {
    let mut pairs = match split_frontmatter(content) {
        Some((fm, _)) => parse_pairs(fm),
        None => Vec::new(),
    };
    match pairs.iter_mut().find(|(k, _)| k == key) {
        Some(pair) => pair.1 = value.to_string(),
        None => pairs.push((key.to_string(), value.to_string())),
    }
    set_frontmatter(content, &pairs)
}

/// The value of a single frontmatter key, if present.
pub fn get_key(content: &str, key: &str) -> Option<String> {
    let (fm, _) = split_frontmatter(content)?;
    parse_pairs(fm).into_iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// Values of a `tags:`-style key, accepting both `tags: [a, b]` and `tags: a b`.
/// Leading `#` is stripped so these compare equal to inline `#tags`.
pub fn parse_tag_list(raw: &str) -> Vec<String> {
    raw.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split([',', ' '])
        .map(|t| t.trim().trim_matches('"').trim_matches('\'').trim_start_matches('#'))
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_frontmatter_from_body() {
        let (fm, body) = split_frontmatter("---\ntitle: Hi\n---\nBody text\n").unwrap();
        assert_eq!(fm, "title: Hi");
        assert_eq!(body, "Body text\n");
    }

    #[test]
    fn no_frontmatter_returns_none() {
        assert!(split_frontmatter("# Just a heading\n").is_none());
        // An opening fence with no closing fence is not frontmatter either.
        assert!(split_frontmatter("---\ntitle: Hi\nno close\n").is_none());
    }

    #[test]
    fn set_key_updates_existing_and_appends_missing() {
        let updated = set_key("---\ntitle: Hi\n---\nBody\n", "title", "Bye");
        assert_eq!(get_key(&updated, "title").as_deref(), Some("Bye"));

        let added = set_key("---\ntitle: Hi\n---\nBody\n", "updated", "2026-08-01");
        assert_eq!(get_key(&added, "title").as_deref(), Some("Hi"));
        assert_eq!(get_key(&added, "updated").as_deref(), Some("2026-08-01"));
        assert!(added.contains("Body"), "body must survive the rewrite");
    }

    #[test]
    fn set_key_creates_frontmatter_when_absent() {
        let out = set_key("# Note\n", "updated", "2026-08-01");
        assert!(out.starts_with("---\nupdated: 2026-08-01\n---\n"));
        assert!(out.contains("# Note"));
    }

    #[test]
    fn tag_lists_parse_in_both_yaml_shapes() {
        assert_eq!(parse_tag_list("[a, b]"), vec!["a", "b"]);
        assert_eq!(parse_tag_list("a b"), vec!["a", "b"]);
        // Leading '#' is normalised away so these match inline #tags.
        assert_eq!(parse_tag_list("[#games, #rpg]"), vec!["games", "rpg"]);
        assert!(parse_tag_list("  ").is_empty());
    }
}
