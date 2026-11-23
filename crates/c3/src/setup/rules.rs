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
//!
//! (0.2.5, S1) The same path replaces the template sentences that named the old folder `.eck/`
//! ([`STATE_SENTENCES`]) in `01-c3-protocol.md` / `02-delegation.md`: only a line that is the old
//! sentence byte for byte is replaced by the current one; a file that names `.eck/` in text the user
//! wrote is reported and left alone.

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

// ------------------------------------------------------------------ the `.eck/` sentences (S1)

/// (0.2.5, S1) The template sentences that named the old folder `.eck/`, with the sentence the
/// current template has in their place: (repo-relative file, old line, new line). The old lines are
/// the exact bytes C3 wrote; the new ones are lines of the embedded templates (a unit test holds
/// both to that).
pub const STATE_SENTENCES: &[(&str, &str, &str)] = &[
    (
        ".claude/rules/01-c3-protocol.md",
        "1. Find all `.eck/*.md` files with `[STUB]` markers. Analyze the codebase to resolve them.",
        "1. Find all `.c3/*.md` files with `[STUB]` markers. Analyze the codebase to resolve them.",
    ),
    (
        ".claude/rules/02-delegation.md",
        "(in a C3-managed repository `.eck/` is ignored, so there is no commit).",
        "(in a C3-managed repository `.c3/` is ignored, so there is no commit).",
    ),
];

/// What a rules file says about the old folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateSentenceFinding {
    /// The rules file (repo-relative).
    pub file: String,
    /// It carries an old template sentence, byte for byte: `c3 doctor --fix` replaces it.
    pub old_sentence: bool,
    /// It names `.eck/` in text C3 did not write (after the old sentences): reported, never edited.
    pub rewritten: bool,
}

impl StateSentenceFinding {
    /// A finding that makes `c3 doctor` exit 1: an old template sentence (`--fix` repairs it). Text
    /// the user wrote is warned about only.
    pub fn is_finding(&self) -> bool {
        self.old_sentence
    }
}

/// The files of [`STATE_SENTENCES`], each once, in order.
fn state_sentence_files() -> Vec<&'static str> {
    let mut v: Vec<&str> = Vec::new();
    for (f, _, _) in STATE_SENTENCES {
        if !v.contains(f) {
            v.push(f);
        }
    }
    v
}

/// `content` with every line that is exactly an old sentence of `file` replaced by its new one
/// (the line end kept as it is), or `None` when no line is.
pub fn with_state_sentences(file: &str, content: &str) -> Option<String> {
    let pairs: Vec<(&str, &str)> = STATE_SENTENCES
        .iter()
        .filter(|(f, _, _)| *f == file)
        .map(|(_, old, new)| (*old, *new))
        .collect();
    let mut out = String::with_capacity(content.len());
    let mut changed = false;
    for piece in content.split_inclusive('\n') {
        let (body, eol) = match piece.strip_suffix("\r\n") {
            Some(b) => (b, "\r\n"),
            None => match piece.strip_suffix('\n') {
                Some(b) => (b, "\n"),
                None => (piece, ""),
            },
        };
        match pairs.iter().find(|(old, _)| *old == body) {
            Some((_, new)) => {
                out.push_str(new);
                out.push_str(eol);
                changed = true;
            }
            None => out.push_str(piece),
        }
    }
    changed.then_some(out)
}

/// Classify one rules file's text: does it carry an old sentence, and does it name `.eck/`
/// elsewhere (in text the user wrote)?
pub fn state_sentence_status(file: &str, content: &str) -> StateSentenceFinding {
    let (old_sentence, rest) = match with_state_sentences(file, content) {
        Some(fixed) => (true, fixed),
        None => (false, content.to_string()),
    };
    let legacy = format!("{}/", crate::state_dir::LEGACY_STATE_DIR);
    StateSentenceFinding {
        file: file.to_string(),
        old_sentence,
        rewritten: rest.contains(&legacy),
    }
}

/// Read the rules files C3 writes and report those that still name `.eck/` (the only I/O of the
/// scan). A file that is absent or not text is not reported.
pub fn scan_state_sentences(repo_root: &Path) -> Vec<StateSentenceFinding> {
    let mut v = Vec::new();
    for file in state_sentence_files() {
        let Ok(bytes) = std::fs::read(repo_root.join(file)) else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        let s = state_sentence_status(file, text);
        if s.old_sentence || s.rewritten {
            v.push(s);
        }
    }
    v
}

/// What a repair of the `.eck/` sentences did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StateSentenceRepair {
    /// The files whose old sentences were replaced (repo-relative).
    pub fixed: Vec<String>,
    /// The repairs that were due and could not be made: `(file, why)`; the file is as it was.
    pub problems: Vec<(String, String)>,
}

/// Replace the old template sentences naming `.eck/` with the current ones - exactly those lines,
/// nothing else - through the model repair's link-safe temporary-file-and-rename path. A file
/// without such a line is not touched. The caller keeps this out of a C3 child.
pub fn repair_state_sentences(repo_root: &Path) -> StateSentenceRepair {
    let mut out = StateSentenceRepair::default();
    let claude = repo_root.join(".claude");
    let rules = claude.join("rules");
    for file in state_sentence_files() {
        let path = repo_root.join(file);
        let Ok(snapshot) = std::fs::read(&path) else {
            continue;
        };
        let Ok(content) = std::str::from_utf8(&snapshot) else {
            continue;
        };
        let Some(new_content) = with_state_sentences(file, content) else {
            continue;
        };
        let dirs = [
            (claude.as_path(), ".claude"),
            (rules.as_path(), ".claude/rules"),
        ];
        let edit = Edit {
            file: file.to_string(),
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
            Ok(()) => out.fixed.push(file.to_string()),
            Err(why) => out.problems.push((file.to_string(), why)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // (0.2.5, S1) Each old sentence is gone from the template and its replacement is a line of it.
    #[test]
    fn the_state_sentences_match_the_embedded_templates() {
        for (file, old, new) in STATE_SENTENCES {
            let template = crate::setup::init::PROJECT_FILES
                .iter()
                .find(|(rel, _)| rel == file)
                .map(|(_, t)| *t)
                .expect("a template");
            assert!(
                template.lines().any(|l| l.trim_end() == *new),
                "{file}: {new}"
            );
            assert!(
                !template.lines().any(|l| l.trim_end() == *old),
                "{file}: {old}"
            );
            assert!(old.contains(".eck/") && new.contains(".c3/"));
        }
    }

    // (0.2.5, S1) Only a byte-for-byte old line is replaced; the line end is kept; text the user
    // wrote about `.eck/` is reported, never edited.
    #[test]
    fn an_old_sentence_is_replaced_and_rewritten_text_is_left() {
        let (file, old, new) = STATE_SENTENCES[1];
        let crlf = format!("# t\r\n\r\n{old}\r\nmine\r\n");
        let fixed = with_state_sentences(file, &crlf).expect("replaced");
        assert_eq!(fixed, format!("# t\r\n\r\n{new}\r\nmine\r\n"));
        let s = state_sentence_status(file, &crlf);
        assert!(s.old_sentence && !s.rewritten && s.is_finding());
        // a rewritten sentence is not the old line
        let mine = "# t\n\nIn this repository `.eck/` holds our notes.\n";
        assert_eq!(with_state_sentences(file, mine), None);
        let s = state_sentence_status(file, mine);
        assert!(!s.old_sentence && s.rewritten && !s.is_finding());
        // the old line indented or extended is no match either
        assert_eq!(with_state_sentences(file, &format!("  {old}\n")), None);
        assert_eq!(with_state_sentences(file, &format!("{old} (sic)\n")), None);
        // the other file's sentence is not replaced here
        assert_eq!(with_state_sentences(file, STATE_SENTENCES[0].1), None);
    }

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
