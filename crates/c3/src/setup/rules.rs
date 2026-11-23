//! New rule text for an old project (0.2.5): `c3 init` writes `.claude/rules/02-delegation.md` only
//! when it is missing, so a project set up before a section was added to C3's template never got it.
//! `c3 doctor` reports such a file and `c3 doctor --fix` appends the missing section - and nothing
//! else.
//!
//! The section is "BRANCHES AND WORKTREES" (the last section of the template, taken from the
//! embedded template itself, so it cannot drift from what `c3 init` writes). (0.2.5 R4, F17-6) The
//! file counts as one C3 wrote when it still carries the template's own title line - that line is
//! the whole test, whatever the body says; a file without it is reported and never edited. The
//! repair only ever APPENDS the missing section: no existing byte changes. The edit is the model
//! repair's: a sibling temporary file,
//! read back, the original re-read against the snapshot the edit was computed from, a rename over
//! the original, links and hard links refused ([`crate::setup::repo_models::replace_file`]). The
//! SessionStart hook does not do this, and nothing happens inside a C3 child (`C3_CHILD`).

use std::path::Path;

use crate::setup::repo_models::{edit_refusal, replace_file, Edit};

/// The rule file, relative to the repository root.
pub const DELEGATION_RULE: &str = ".claude/rules/02-delegation.md";

/// The template `c3 init` writes (the same bytes as `init`'s `PROJECT_FILES` entry).
const DELEGATION_TEMPLATE: &str = include_str!("../../templates/02-delegation.md");

/// The heading the missing section starts with.
pub const BRANCHES_HEADING: &str = "## BRANCHES AND WORKTREES";

/// What a scan found about the delegation rule file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RuleStatus {
    /// There is no such file (or it is not readable text): nothing to say.
    #[default]
    Absent,
    /// The file has the section.
    Current,
    /// The file lacks the section. `c3_written` is true when it still carries the template's title
    /// line (the repair may append the section), false when the user rewrote it (reported only).
    MissingSection { c3_written: bool },
}

impl RuleStatus {
    /// A finding that makes `c3 doctor` exit 1: a file C3 wrote that lacks the section (`--fix`
    /// repairs it). A rewritten file is the user's own and only warned about.
    pub fn is_finding(self) -> bool {
        matches!(self, RuleStatus::MissingSection { c3_written: true })
    }
}

/// The template's title line (the first line, without a BOM or a line end).
fn template_title() -> &'static str {
    DELEGATION_TEMPLATE
        .lines()
        .next()
        .unwrap_or("")
        .trim_start_matches('\u{feff}')
        .trim_end()
}

/// Whether some line of `content` is exactly `line` (trailing white space ignored).
fn has_line(content: &str, line: &str) -> bool {
    content.lines().any(|l| l.trim_end() == line)
}

/// Classify a rule file's text.
pub fn status_of(content: &str) -> RuleStatus {
    if has_line(content, BRANCHES_HEADING) {
        return RuleStatus::Current;
    }
    RuleStatus::MissingSection {
        c3_written: has_line(content, template_title()),
    }
}

/// The section as the template carries it: from its heading to the end, `\n` line ends, one
/// trailing newline. `None` only if the embedded template lost the heading (a build-time bug the
/// unit test catches).
pub fn template_section() -> Option<String> {
    let mut out = String::new();
    let mut inside = false;
    for l in DELEGATION_TEMPLATE.lines() {
        if l.trim_end() == BRANCHES_HEADING {
            inside = true;
        }
        if inside {
            out.push_str(l.trim_end_matches('\r'));
            out.push('\n');
        }
    }
    inside.then(|| format!("{}\n", out.trim_end()))
}

/// `content` with the template's section appended, or `None` when nothing is to be appended: the
/// file already has the section, or it is not a file C3 wrote. The existing bytes are kept as they
/// are; the appended text follows after a blank line, in the file's own line ending.
pub fn with_branches_section(content: &str) -> Option<String> {
    match status_of(content) {
        RuleStatus::MissingSection { c3_written: true } => {}
        _ => return None,
    }
    let section = template_section()?;
    let eol = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut out = String::with_capacity(content.len() + section.len() + 4);
    out.push_str(content);
    if !out.ends_with('\n') {
        out.push_str(eol);
    }
    out.push_str(eol);
    out.push_str(&section.replace('\n', eol));
    Some(out)
}

/// Read the delegation rule of `repo_root` and classify it (the only I/O of a scan).
pub fn scan(repo_root: &Path) -> RuleStatus {
    match std::fs::read(repo_root.join(DELEGATION_RULE)) {
        Ok(bytes) => match std::str::from_utf8(&bytes) {
            Ok(text) => status_of(text),
            Err(_) => RuleStatus::Absent,
        },
        Err(_) => RuleStatus::Absent,
    }
}

/// What a repair of the rule file did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuleRepair {
    /// The section was appended to this file (repo-relative).
    pub fixed: Option<String>,
    /// The repair that was due and could not be made: `(file, why)`; the file is as it was.
    pub problem: Option<(String, String)>,
}

/// Append the missing section to a rule file C3 wrote. Does nothing for a file that has the section,
/// that the user rewrote, or that does not exist. The caller keeps this out of a C3 child.
pub fn repair_delegation_rule(repo_root: &Path) -> RuleRepair {
    let path = repo_root.join(DELEGATION_RULE);
    let Ok(snapshot) = std::fs::read(&path) else {
        return RuleRepair::default();
    };
    let Ok(content) = std::str::from_utf8(&snapshot) else {
        return RuleRepair::default();
    };
    let Some(new_content) = with_branches_section(content) else {
        return RuleRepair::default();
    };
    let claude = repo_root.join(".claude");
    let rules = claude.join("rules");
    let dirs = [
        (claude.as_path(), ".claude"),
        (rules.as_path(), ".claude/rules"),
    ];
    let edit = Edit {
        file: DELEGATION_RULE.to_string(),
        path: &path,
        dirs: &dirs,
        snapshot: &snapshot,
        new_content: new_content.as_bytes(),
    };
    let result = match edit_refusal(edit.dirs, edit.path) {
        Some(why) => Err(why),
        None => replace_file(&edit),
    };
    match result {
        Ok(()) => RuleRepair {
            fixed: Some(DELEGATION_RULE.to_string()),
            problem: None,
        },
        Err(why) => RuleRepair {
            fixed: None,
            problem: Some((DELEGATION_RULE.to_string(), why)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The template without its section: what an old C3-written file looks like.
    fn old_file() -> String {
        let all = DELEGATION_TEMPLATE.replace("\r\n", "\n");
        let cut = all
            .find(BRANCHES_HEADING)
            .expect("the template has the section");
        all[..cut].trim_end().to_string() + "\n"
    }

    #[test]
    fn the_embedded_template_has_a_title_and_the_section() {
        assert!(template_title().contains("SUPERVISOR / WORKER DELEGATION"));
        let s = template_section().expect("section");
        assert!(s.starts_with(BRANCHES_HEADING), "{s}");
        assert!(s.contains("never `--force` over a link"), "{s}");
        assert_eq!(status_of(DELEGATION_TEMPLATE), RuleStatus::Current);
    }

    #[test]
    fn an_old_template_file_is_reported_and_completed_to_the_template() {
        let old = old_file();
        assert_eq!(
            status_of(&old),
            RuleStatus::MissingSection { c3_written: true }
        );
        assert!(status_of(&old).is_finding());
        let fixed = with_branches_section(&old).expect("appended");
        assert!(fixed.starts_with(&old));
        assert_eq!(status_of(&fixed), RuleStatus::Current);
        // only the section was added: the file equals the template apart from line-end whitespace
        let want = DELEGATION_TEMPLATE.replace("\r\n", "\n");
        assert_eq!(fixed.trim_end(), want.trim_end());
        // idempotent
        assert_eq!(with_branches_section(&fixed), None);
    }

    #[test]
    fn a_rewritten_file_is_reported_never_edited() {
        let mine = "# My own delegation rules\n\nDo it my way.\n";
        assert_eq!(
            status_of(mine),
            RuleStatus::MissingSection { c3_written: false }
        );
        assert!(!status_of(mine).is_finding());
        assert_eq!(with_branches_section(mine), None);
    }

    #[test]
    fn crlf_and_a_missing_final_newline_are_kept_consistent() {
        let crlf = old_file().replace('\n', "\r\n");
        let fixed = with_branches_section(&crlf).unwrap();
        assert!(fixed.starts_with(&crlf));
        assert!(
            !fixed.replace("\r\n", "").contains('\n'),
            "a bare LF in a CRLF file"
        );
        let no_nl = old_file().trim_end().to_string();
        let fixed = with_branches_section(&no_nl).unwrap();
        assert!(
            fixed.contains(&format!("\n\n{BRANCHES_HEADING}\n")),
            "{fixed}"
        );
    }
}
