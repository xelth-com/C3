//! Repair a repository's model settings in place (follow-up to `c3 doctor`): C3 rewrites a
//! stale agent model pin to the floating alias, and lets `AGENTS.md` (the OpenCode agent file
//! eckSnapshot writes) follow the roster's successor. It makes exactly two kinds of edit, each
//! changing ONE line of a file and nothing else (every other byte - the line endings, a BOM -
//! is preserved). Nothing here runs git, commits or writes outside the repository root; the
//! edits are left uncommitted for `c3 finish`.
//!
//! It is used from two places: `c3 doctor --fix` (which then re-reports what is left) and the
//! `c3 hook` SessionStart line (automatic, default ON, off with `C3_REPO_MODELS=off` or
//! `"repo_models": false` in `<codex home>/c3/config.json`).

use std::path::Path;

use c3_core::claude::model_family;
use c3_core::roster::Roster;

use crate::maintain::version::{parse_model_id, successor};
use crate::setup::doctor::{agent_model, pinned_agent_model};

/// One model line C3 rewrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFix {
    /// Repo-relative path of the edited file.
    pub file: String,
    /// The model id before the edit.
    pub old: String,
    /// The model id after the edit.
    pub new: String,
}

/// Replace the value of the top-level `model:` key in a YAML frontmatter with `new_value`,
/// keeping every other byte of `content` (the BOM, the line endings, every other line). Returns
/// the new content, or `None` when the content has no `---`-delimited frontmatter with a
/// `model:` key. Only the single frontmatter `model:` line is changed.
fn replace_frontmatter_model_value(content: &str, new_value: &str) -> Option<String> {
    let mut idx = 0usize;
    let mut first = true;
    let mut in_front = false;
    while idx <= content.len() {
        if idx == content.len() {
            break;
        }
        let rest = &content[idx..];
        let (content_end, next) = match rest.find('\n') {
            Some(p) => {
                let mut ce = idx + p;
                if ce > idx && content.as_bytes()[ce - 1] == b'\r' {
                    ce -= 1;
                }
                (ce, idx + p + 1)
            }
            None => (content.len(), content.len()),
        };
        let line = &content[idx..content_end];
        let line_cmp = if first {
            line.strip_prefix('\u{feff}').unwrap_or(line)
        } else {
            line
        };
        if !in_front {
            if line_cmp.trim() == "---" {
                in_front = true;
                first = false;
                idx = next;
                continue;
            }
            return None; // no frontmatter at the top
        }
        if line_cmp.trim() == "---" {
            return None; // frontmatter closed before a model line
        }
        if line.strip_prefix("model:").is_some() {
            let mut out = String::with_capacity(content.len() + new_value.len());
            out.push_str(&content[..idx]);
            out.push_str("model: ");
            out.push_str(new_value);
            out.push_str(&content[content_end..]);
            return Some(out);
        }
        first = false;
        idx = next;
    }
    None
}

/// The rewrite for a `.claude/agents/*.md` file: a frontmatter `model:` that is pinned AND names
/// a Claude family (`claude-opus-4-8` -> `opus`) becomes the alias. A pinned id that names no
/// known family is left alone (doctor keeps reporting it). Returns `(new_content, old, new)`.
pub fn rewrite_agent_model(content: &str) -> Option<(String, String, String)> {
    let (old, alias) = pinned_agent_model(content)?;
    if alias.is_empty() {
        return None;
    }
    let new_content = replace_frontmatter_model_value(content, &alias)?;
    Some((new_content, old, alias))
}

/// The rewrite for the repository-root `AGENTS.md`: its frontmatter `model:` follows the roster.
/// The value is lower-cased and the successor rule ([`successor`]: same prefix and suffix, same
/// major, higher minor) is asked whether one of `roster_models` is its successor; if so the value
/// is rewritten to that id, keeping the casing of the original prefix/suffix (`GLM-5.3` ->
/// `GLM-5.4`). No roster, no successor, a higher major only, a different family, or no frontmatter
/// model line: `None`. Returns `(new_content, old, new)`.
pub fn rewrite_agents_md_model(
    content: &str,
    roster_models: &[String],
) -> Option<(String, String, String)> {
    let value = agent_model(content)?;
    // A value that already resolves to a Claude family is handled by the agent-file rule, not here;
    // AGENTS.md follows the roster for non-Claude families only.
    if !model_family(&value).is_empty() {
        return None;
    }
    let lowered = value.to_lowercase();
    let cands: Vec<String> = roster_models.iter().map(|m| m.to_lowercase()).collect();
    let succ = successor(&lowered, &cands)?;
    // Reconstruct with the ORIGINAL casing: same prefix/suffix and major, the successor's minor.
    let orig = parse_model_id(&value)?;
    let succ_parsed = parse_model_id(succ)?;
    let new_value = format!(
        "{}{}.{}{}",
        orig.prefix, orig.major, succ_parsed.minor, orig.suffix
    );
    if new_value == value {
        return None;
    }
    let new_content = replace_frontmatter_model_value(content, &new_value)?;
    Some((new_content, value, new_value))
}

/// Apply both kinds of fix to a repository, writing the changed files in place. `roster_models`
/// is the model id of every ACTIVE roster entry (for the AGENTS.md successor lookup). Returns the
/// fixes applied, in file order. Any read/write error on a single file is swallowed (that file is
/// skipped); nothing is written outside `repo_root` and no git command is run.
pub fn refresh_repo_models(repo_root: &Path, roster_models: &[String]) -> Vec<ModelFix> {
    let mut fixes = Vec::new();

    // 1. `.claude/agents/*.md` pins -> the alias. A directory or file that is a link (a symlink, a
    //    Windows junction) is skipped: its target may lie outside the repository.
    let claude_dir = repo_root.join(".claude");
    let agents = claude_dir.join("agents");
    if is_real_dir(&claude_dir) && is_real_dir(&agents) {
        let mut names: Vec<std::ffi::OsString> = std::fs::read_dir(&agents)
            .map(|rd| rd.flatten().map(|e| e.file_name()).collect())
            .unwrap_or_default();
        names.sort();
        for name in names {
            let name = name.to_string_lossy().to_string();
            if !name.to_lowercase().ends_with(".md") {
                continue;
            }
            let path = agents.join(&name);
            if !is_real_file(&path) {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Some((new_content, old, new)) = rewrite_agent_model(&content) {
                if std::fs::write(&path, new_content).is_ok() {
                    fixes.push(ModelFix {
                        file: format!(".claude/agents/{name}"),
                        old,
                        new,
                    });
                }
            }
        }
    }

    // 2. The repository-root `AGENTS.md` -> the roster's successor (a link is skipped, as above).
    let agents_md = repo_root.join("AGENTS.md");
    if is_real_file(&agents_md) {
        if let Ok(content) = std::fs::read_to_string(&agents_md) {
            if let Some((new_content, old, new)) = rewrite_agents_md_model(&content, roster_models)
            {
                if std::fs::write(&agents_md, new_content).is_ok() {
                    fixes.push(ModelFix {
                        file: "AGENTS.md".to_string(),
                        old,
                        new,
                    });
                }
            }
        }
    }

    fixes
}

/// A directory that is itself there, not a link to one (`symlink_metadata` does not follow a
/// symlink, and reports a Windows junction as a link too).
fn is_real_dir(p: &Path) -> bool {
    std::fs::symlink_metadata(p)
        .map(|m| m.file_type().is_dir())
        .unwrap_or(false)
}

/// A regular file that is itself there, not a link to one.
fn is_real_file(p: &Path) -> bool {
    std::fs::symlink_metadata(p)
        .map(|m| m.file_type().is_file())
        .unwrap_or(false)
}

/// The active roster entries' model ids (for the AGENTS.md successor lookup). Empty on any error.
pub fn roster_models(roster: &Roster) -> Vec<String> {
    if !roster.exists {
        return Vec::new();
    }
    roster
        .active_entries()
        .map(|e| e.model.clone())
        .filter(|m| !m.is_empty())
        .collect()
}

/// Whether the automatic repository model refresh (the `c3 hook` action) is ON. Default ON;
/// `C3_REPO_MODELS=off` (environment wins) or `"repo_models": false` in the C3 config turns it
/// off. Mirrors [`crate::maintain::enabled`] / [`crate::update::check_enabled`].
pub fn enabled() -> bool {
    if let Ok(v) = std::env::var("C3_REPO_MODELS") {
        if v.trim().eq_ignore_ascii_case("off") {
            return false;
        }
        return true;
    }
    match crate::c3config::load() {
        Ok(Some(cfg)) => cfg.repo_models,
        _ => true,
    }
}

/// The `c3 hook` line: apply the refresh to the current repository and return ONE line when
/// something changed, else `None`. The feature is default ON (off switch in [`enabled`]). This
/// never fails a session: every error is swallowed, nothing is written outside the repository
/// root, no git command is run and nothing is committed. It does nothing outside a git repository
/// or when there are no agent files to fix.
///
/// It also does nothing inside a child that C3 itself started (`engines::in_c3_child`): a reviewer
/// on the `claude` engine runs this same hook in the repository under review and must stay
/// read-only, and a worker's edit would land in its patch and could break its scope check.
pub fn hook_line() -> Option<String> {
    if !enabled() || crate::engines::in_c3_child() {
        return None;
    }
    let cwd = std::env::current_dir().ok()?;
    let repo_root = crate::providers::resolve_repo_root(&cwd);
    // Only act inside a git repository (a `.git` dir or, for a worktree, a `.git` file).
    if !repo_root.join(".git").exists() {
        return None;
    }
    let models = crate::providers::read_reviewer_roster()
        .map(|r| roster_models(&r))
        .unwrap_or_default();
    let fixes = refresh_repo_models(&repo_root, &models);
    if fixes.is_empty() {
        return None;
    }
    let body = fixes
        .iter()
        .map(|f| format!("{} {} -> {}", f.file, f.old, f.new))
        .collect::<Vec<_>>()
        .join("; ");
    Some(format!(
        "c3: model settings refreshed in this repository - {body} (left uncommitted)"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_pin_becomes_the_alias_and_keeps_every_other_byte() {
        // CRLF line endings and a trailing body are preserved; only the model value changes.
        let src = "---\r\nname: opus-worker\r\nmodel: claude-opus-4-8\r\n---\r\nbody line\r\n";
        let (out, old, new) = rewrite_agent_model(src).unwrap();
        assert_eq!((old.as_str(), new.as_str()), ("claude-opus-4-8", "opus"));
        assert_eq!(
            out,
            "---\r\nname: opus-worker\r\nmodel: opus\r\n---\r\nbody line\r\n"
        );
        // A BOM is kept.
        let bom = "\u{feff}---\nmodel: claude-sonnet-5\n---\n";
        let (out, _, new) = rewrite_agent_model(bom).unwrap();
        assert_eq!(new, "sonnet");
        assert_eq!(out, "\u{feff}---\nmodel: sonnet\n---\n");
    }

    #[test]
    fn an_unknown_family_pin_is_not_rewritten() {
        assert!(rewrite_agent_model("---\nmodel: claude-3-opus-20240229\n---\n").is_none());
        // An alias, inherit and a missing line are not rewritten either.
        assert!(rewrite_agent_model("---\nmodel: opus\n---\n").is_none());
        assert!(rewrite_agent_model("---\nmodel: inherit\n---\n").is_none());
        assert!(rewrite_agent_model("---\nname: x\n---\n").is_none());
    }

    #[test]
    fn agents_md_follows_the_roster_successor_and_keeps_casing() {
        let roster = vec![
            "glm-5.4".to_string(),
            "glm-5.3".to_string(),
            "gpt-6-astra".to_string(),
        ];
        let src = "---\nmodel: GLM-5.3\n---\nagent body\n";
        let (out, old, new) = rewrite_agents_md_model(src, &roster).unwrap();
        assert_eq!((old.as_str(), new.as_str()), ("GLM-5.3", "GLM-5.4"));
        assert_eq!(out, "---\nmodel: GLM-5.4\n---\nagent body\n");
        // Lowercase prefix stays lowercase.
        let (_, _, new) = rewrite_agents_md_model("---\nmodel: glm-5.3\n---\n", &roster).unwrap();
        assert_eq!(new, "glm-5.4");
    }

    #[test]
    fn agents_md_no_edit_without_a_successor() {
        // Empty roster -> nothing.
        assert!(rewrite_agents_md_model("---\nmodel: GLM-5.3\n---\n", &[]).is_none());
        // A higher MAJOR is announced, never swapped.
        assert!(
            rewrite_agents_md_model("---\nmodel: GLM-5.3\n---\n", &["glm-6.0".to_string()])
                .is_none()
        );
        // A different family.
        assert!(
            rewrite_agents_md_model("---\nmodel: GLM-5.3\n---\n", &["qwen3.9".to_string()])
                .is_none()
        );
        // No frontmatter model line.
        assert!(rewrite_agents_md_model("---\nname: x\n---\n", &["glm-5.4".to_string()]).is_none());
        // A Claude-family value is left to the agent-file rule, not the roster rule.
        assert!(rewrite_agents_md_model(
            "---\nmodel: claude-opus-4-8\n---\n",
            &["claude-opus-5-5".to_string()]
        )
        .is_none());
    }
}
