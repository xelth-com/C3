//! `c3 manifest`: atomic edits to a `.eck/<NAME>.md` manifest. The name check and the
//! `append_to_section` text surgery are pure functions so they are tested without disk.
//! No git, no snapshot.

use std::path::Path;

/// The manifest actions (`--action`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    ReplaceText,
    AppendToSection,
    ReplaceFile,
}

impl Action {
    pub fn parse(s: &str) -> Option<Action> {
        match s {
            "replace_text" => Some(Action::ReplaceText),
            "append_to_section" => Some(Action::AppendToSection),
            "replace_file" => Some(Action::ReplaceFile),
            _ => None,
        }
    }
}

/// Whether `name` is an acceptable manifest file name: a plain name ending `.md` directly
/// under `.eck/` — no path separator, no `..`, and a non-empty stem (`.md` alone refused).
pub fn manifest_name_ok(name: &str) -> bool {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        return false;
    }
    match name.strip_suffix(".md") {
        Some(stem) => !stem.is_empty(),
        None => false,
    }
}

/// The starting content of a manifest that does not exist yet: `# <NAME>\n\n`.
pub fn initial_content(name: &str) -> String {
    let title = name.strip_suffix(".md").unwrap_or(name);
    format!("# {title}\n\n")
}

/// Insert `content` at the END of the section headed by `header` (the line exactly equal
/// to `header`), i.e. just before the next header of the same or higher level (fewer or
/// equal `#`). When the section is missing, append `\n<header>\n<content>\n` at the end of
/// the file. Returns the new document (always `\n`-terminated).
pub fn append_to_section(doc: &str, header: &str, content: &str) -> String {
    let header = header.trim_end();
    let level = header.bytes().take_while(|&b| b == b'#').count();
    let content = content.trim_end_matches('\n');
    let lines: Vec<&str> = doc.lines().collect();

    let hidx = lines.iter().position(|l| l.trim_end() == header);
    let Some(hidx) = hidx else {
        // Section missing: append it at the end of the file.
        let mut out = doc.trim_end_matches('\n').to_string();
        if !out.is_empty() {
            out.push('\n');
        }
        out.push('\n');
        out.push_str(header);
        out.push('\n');
        out.push_str(content);
        out.push('\n');
        return out;
    };

    // End of the section: the first later line that is a header of level <= this one.
    let mut end = lines.len();
    for (i, l) in lines.iter().enumerate().skip(hidx + 1) {
        let lv = l.bytes().take_while(|&b| b == b'#').count();
        if lv > 0 && lv <= level && l.as_bytes().get(lv) == Some(&b' ') {
            end = i;
            break;
        }
    }
    // Trim trailing blank lines inside the section so the new content sits right after the
    // section's existing body.
    let mut body_end = end;
    while body_end > hidx + 1 && lines[body_end - 1].trim().is_empty() {
        body_end -= 1;
    }

    let mut out: Vec<String> = lines[..body_end].iter().map(|s| s.to_string()).collect();
    out.extend(content.split('\n').map(|s| s.to_string()));
    if end < lines.len() {
        out.push(String::new());
        out.extend(lines[end..].iter().map(|s| s.to_string()));
    }
    let mut s = out.join("\n");
    s.push('\n');
    s
}

/// Apply an action to `current` content, returning the new content or an error message.
pub fn apply(
    current: &str,
    action: Action,
    content: &str,
    section: Option<&str>,
    target: Option<&str>,
) -> Result<String, String> {
    match action {
        Action::ReplaceFile => Ok(content.to_string()),
        Action::ReplaceText => {
            let target = target
                .filter(|t| !t.is_empty())
                .ok_or_else(|| "replace_text requires --target".to_string())?;
            if !current.contains(target) {
                return Err("target text not found".to_string());
            }
            Ok(current.replacen(target, content, 1))
        }
        Action::AppendToSection => {
            let section = section
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "append_to_section requires --section".to_string())?;
            Ok(append_to_section(current, section, content))
        }
    }
}

/// The on-disk path of a manifest `name` under `<repo>/.eck/`. The name must already have
/// passed [`manifest_name_ok`].
pub fn manifest_path(repo_root: &Path, name: &str) -> std::path::PathBuf {
    repo_root.join(".eck").join(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_check_accepts_a_plain_md_name() {
        assert!(manifest_name_ok("ROADMAP.md"));
        assert!(manifest_name_ok("TECH_DEBT.md"));
    }

    #[test]
    fn name_check_refuses_traversal_separators_and_bad_extensions() {
        assert!(!manifest_name_ok("../x.md"));
        assert!(!manifest_name_ok("a/b.md"));
        assert!(!manifest_name_ok("a\\b.md"));
        assert!(!manifest_name_ok("x.txt"));
        assert!(!manifest_name_ok(""));
        assert!(!manifest_name_ok(".md"));
    }

    #[test]
    fn append_inserts_at_end_of_section_before_next_header() {
        let doc = "# Title\n\n## A\nfirst\n\n## B\nx\n";
        let out = append_to_section(doc, "## A", "second");
        assert_eq!(out, "# Title\n\n## A\nfirst\nsecond\n\n## B\nx\n");
    }

    #[test]
    fn append_to_last_section_goes_to_the_end() {
        let doc = "# Title\n\n## A\nfirst\n";
        let out = append_to_section(doc, "## A", "second");
        assert_eq!(out, "# Title\n\n## A\nfirst\nsecond\n");
    }

    #[test]
    fn append_stops_at_a_higher_level_header() {
        // A deeper `### sub` is part of section A; the next `## B` ends it.
        let doc = "## A\nfirst\n\n### sub\ndeep\n\n## B\ny\n";
        let out = append_to_section(doc, "## A", "added");
        assert_eq!(out, "## A\nfirst\n\n### sub\ndeep\nadded\n\n## B\ny\n");
    }

    #[test]
    fn append_missing_section_appends_at_end_of_file() {
        let doc = "# Title\n\nbody\n";
        let out = append_to_section(doc, "## New", "content");
        assert_eq!(out, "# Title\n\nbody\n\n## New\ncontent\n");
    }

    #[test]
    fn replace_text_needs_target_and_replaces_first_occurrence() {
        assert!(apply("abc abc", Action::ReplaceText, "X", None, None).is_err());
        let out = apply("abc abc", Action::ReplaceText, "X", None, Some("abc")).unwrap();
        assert_eq!(out, "X abc");
        assert!(apply("abc", Action::ReplaceText, "X", None, Some("zzz")).is_err());
    }

    #[test]
    fn replace_file_writes_the_whole_content() {
        let out = apply("old", Action::ReplaceFile, "brand new", None, None).unwrap();
        assert_eq!(out, "brand new");
    }

    #[test]
    fn initial_content_titles_the_file() {
        assert_eq!(initial_content("ROADMAP.md"), "# ROADMAP\n\n");
    }
}
