//! `c3 doctor` — report what still needs doing in a C3 workspace (port spec, Part C).
//!
//! It reports: C3's project folder (`.c3/` missing, only the legacy `.eck/`, or both -
//! [`crate::state_dir::Layout`]); `[STUB` markers left in `.c3/*.md` (or in `.eck/*.md` until a
//! writing command moved it); whether `c3 mcp` is registered in `.mcp.json`; leftovers of the old
//! tool (an `eck-core` server in `.mcp.json` / `.codex/config.toml` / `opencode.json`, which C3
//! does not need, and any `.claude/rules/*.md` that still names an `eck_*` tool); a C3-written
//! rules file that still carries an old template sentence naming `.eck/`
//! ([`crate::setup::rules::scan_state_sentences`]); any
//! `.claude/agents/*.md` whose frontmatter pins a concrete model id instead of a floating
//! alias; branches not merged and without a recorded state, worktrees without one, an unknown
//! default branch and a kimi review's stale throwaway worktree ([`crate::setup::branches`]); and
//! the size and file count of `.c3/snapshots/`. Exit 0 when
//! there is nothing to fix, 1 otherwise. The folder notes, the snapshot-folder size and the start
//! folder's branch / uncommitted changes are diagnostic context only and never force exit 1.

use std::path::{Path, PathBuf};

use serde_json::Value;

use c3_core::claude::{
    is_published_claude_shape, model_family, CLAUDE_MODELS, CLAUDE_MODEL_ALIASES,
};

use crate::setup::branches::{self, BranchReport};

/// The eck MCP tool names whose presence in a rule marks an unported project.
const ECK_TOOL_NAMES: &[&str] = &["eck_finish_task", "eck_fail_task", "eck_manifest_edit"];

/// The eck MCP server name left in a config.
const ECK_CORE: &str = "eck-core";

/// A `.claude/agents/*.md` whose frontmatter pins a concrete model id instead of a floating
/// alias.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedAgent {
    /// Repo-relative path of the agent file.
    pub file: String,
    /// The pinned model id, exactly as the `model:` line spells it.
    pub model: String,
    /// The alias to suggest instead (`opus` / `sonnet` / `haiku` / `fable`), or empty when the
    /// id names no known family.
    pub alias: String,
}

/// Everything `c3 doctor` found.
#[derive(Debug, Clone, Default)]
pub struct DoctorReport {
    /// Which of `.c3/` and the legacy `.eck/` the repository has.
    pub state_layout: crate::state_dir::Layout,
    /// The folder the scan read the manifests from (`.c3`, or `.eck` until a writing command moved
    /// it); empty when neither exists.
    pub state_read: String,
    /// `.c3/*.md` (or `.eck/*.md`) files that still carry a `[STUB` marker (repo-relative).
    pub stub_files: Vec<String>,
    /// Whether a `c3 mcp` server is registered in `.mcp.json`.
    pub mcp_registered: bool,
    /// Config files that still reference an `eck-core` MCP server (repo-relative).
    pub eck_core_in: Vec<String>,
    /// `.claude/rules/*.md` files that still name an `eck_*` tool (repo-relative).
    pub eck_rule_files: Vec<String>,
    /// `.claude/agents/*.md` files whose `model:` pins a concrete id instead of an alias.
    pub pinned_agents: Vec<PinnedAgent>,
    /// Number of `*.md` files in `.c3/snapshots/`.
    pub snapshot_count: usize,
    /// Total bytes of those snapshot files.
    pub snapshot_bytes: u64,
    /// Branches and worktrees (`c3 branches`, cheap scan); `None` outside a git repository.
    pub branches: Option<BranchReport>,
    /// `.claude/rules/02-delegation.md` against the current template's "BRANCHES AND WORKTREES"
    /// section ([`crate::setup::rules`]).
    pub delegation_rule: crate::setup::rules::RuleStatus,
    /// The rules files that still name `.eck/`: an old template sentence (`--fix` replaces it) or
    /// text the user wrote (reported only) - [`crate::setup::rules::scan_state_sentences`].
    pub state_sentences: Vec<crate::setup::rules::StateSentenceFinding>,
    /// (0.2.5) The installed git (`2.45.1`), empty when `git --version` did not answer.
    pub git_version: String,
    /// (0.2.5) Why the installed git does not do ([`git_version_problem`]); `None` when it does.
    pub git_problem: Option<String>,
}

/// (0.2.5) The oldest git C3 works with: since wave R5 revisions reach git after
/// `--end-of-options`, which git 2.30 introduced.
pub const MIN_GIT: (u32, u32) = (2, 30);

/// The version in `git --version` output (`git version 2.45.1.windows.1`, `git version 2.39.3
/// (Apple Git-145)`): major, minor, patch (a missing patch reads 0).
pub fn parse_git_version(out: &str) -> Option<(u32, u32, u32)> {
    let v = out.trim().strip_prefix("git version ")?;
    let mut parts = v
        .split(|c: char| !c.is_ascii_digit())
        .take_while(|p| !p.is_empty());
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    Some((major, minor, patch))
}

/// The installed git's version text (`git --version`, the part after `git version `), `Err` (why)
/// when git does not answer or the answer does not parse. TEST HOOK (test mode only):
/// `CODEX_CONSULT_TEST_GIT_VERSION=<version>` stands in for the answer.
pub fn installed_git_version() -> Result<(String, (u32, u32, u32)), String> {
    let out = match c3_core::test_hooks::hook("CODEX_CONSULT_TEST_GIT_VERSION") {
        Some(v) => format!("git version {}", v.trim()),
        None => {
            let o = std::process::Command::new("git")
                .arg("--version")
                .stdin(std::process::Stdio::null())
                .output()
                .map_err(|e| format!("git could not be started ({e})"))?;
            if !o.status.success() {
                return Err("git --version failed".to_string());
            }
            String::from_utf8_lossy(&o.stdout).into_owned()
        }
    };
    let parsed = parse_git_version(&out)
        .ok_or_else(|| format!("git --version answered '{}'", out.trim()))?;
    let text = out
        .trim()
        .strip_prefix("git version ")
        .unwrap_or(out.trim())
        .to_string();
    Ok((text, parsed))
}

/// (0.2.5) One line when the installed git does not do: older than [`MIN_GIT`] (naming the installed
/// and the required version), or no answer at all; `None` when it does.
pub fn git_version_problem() -> Option<String> {
    match installed_git_version() {
        Ok((_, v)) if (v.0, v.1) >= MIN_GIT => None,
        Ok((text, _)) => Some(format!(
            "git {text} is installed; C3 needs git {}.{} or later (revisions reach git after --end-of-options)",
            MIN_GIT.0, MIN_GIT.1
        )),
        Err(e) => Some(format!(
            "{e}; C3 needs git {}.{} or later",
            MIN_GIT.0, MIN_GIT.1
        )),
    }
}

impl DoctorReport {
    /// A clean workspace: no stubs left, `c3 mcp` registered, no eck leftovers, no agent file
    /// pinning an old model, no C3-written delegation rule without the branches section, no unknown
    /// branch and no worktree without a recorded state. The
    /// snapshot folder size and the start folder's branch / uncommitted changes are informational
    /// and do not count.
    pub fn is_clean(&self) -> bool {
        self.stub_files.is_empty()
            && self.mcp_registered
            && self.eck_core_in.is_empty()
            && self.eck_rule_files.is_empty()
            && self.pinned_agents.is_empty()
            && !self.delegation_rule.is_finding()
            && !self.state_sentences.iter().any(|s| s.is_finding())
            && self.git_problem.is_none()
            && self
                .branches
                .as_ref()
                .is_none_or(|b| b.is_clean() && b.stale_worktrees.is_empty())
    }
}

/// The top-level `model` key of a YAML frontmatter, as both `c3 doctor` and the repair
/// ([`crate::setup::repo_models`]) read it - ONE rule for both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelLine {
    /// The value: unquoted, without a trailing `# comment`, trailing white space trimmed.
    pub value: String,
    /// The byte range of the value in the content - inside the quotes of a quoted value, so a
    /// replacement of exactly this range keeps the quotes, the key spelling and a comment.
    pub span: std::ops::Range<usize>,
}

/// Find the top-level `model` key of the leading `---`-delimited YAML frontmatter (a BOM before
/// it is fine). The key is an UN-indented `model`, optionally followed by spaces or tabs before
/// the `:` (`model:` and `model :` are the same YAML key), and the `:` is followed by white space
/// or the end of the line (`model:x` is a plain scalar, not a key). An indented `model:` (inside a
/// block scalar such as the `description`, or a nested map) is not the key. The value is a single-
/// or double-quoted string (its inside), or a plain scalar up to a ` #` comment or the end of the
/// line. An unterminated quote is no value (`None`), as is a frontmatter without the key.
pub fn frontmatter_model(content: &str) -> Option<ModelLine> {
    let mut idx = 0usize;
    let mut first = true;
    while idx < content.len() {
        let rest = &content[idx..];
        let (line_end, next) = match rest.find('\n') {
            Some(p) => {
                let mut end = idx + p;
                if end > idx && content.as_bytes()[end - 1] == b'\r' {
                    end -= 1;
                }
                (end, idx + p + 1)
            }
            None => (content.len(), content.len()),
        };
        let mut start = idx;
        if first && content[start..line_end].starts_with('\u{feff}') {
            start += '\u{feff}'.len_utf8();
        }
        let line = &content[start..line_end];
        if first {
            if line.trim() != "---" {
                return None; // no frontmatter at the top
            }
            first = false;
            idx = next;
            continue;
        }
        if line.trim() == "---" {
            return None; // the frontmatter closed without the key
        }
        if let Some(after_key) = line.strip_prefix("model") {
            let after_ws = after_key.trim_start_matches([' ', '\t']);
            if let Some(after_colon) = after_ws.strip_prefix(':') {
                if after_colon.is_empty() || after_colon.starts_with([' ', '\t']) {
                    let vstart = line_end - after_colon.trim_start_matches([' ', '\t']).len();
                    return model_value(content, vstart, line_end);
                }
            }
        }
        idx = next;
    }
    None
}

/// The value of a `model` key that starts at byte `vstart` of `content` and whose line ends at
/// `line_end` (see [`frontmatter_model`]).
fn model_value(content: &str, vstart: usize, line_end: usize) -> Option<ModelLine> {
    let raw = &content[vstart..line_end];
    if let Some(q) = raw.chars().next().filter(|c| *c == '"' || *c == '\'') {
        let inner = &raw[1..];
        let close = inner.find(q)?;
        let span = vstart + 1..vstart + 1 + close;
        return Some(ModelLine {
            value: content[span.clone()].to_string(),
            span,
        });
    }
    // A plain scalar ends at a comment (`#` at its start or after white space) or the line end.
    let mut end = raw.len();
    let mut prev_ws = true;
    for (i, c) in raw.char_indices() {
        if c == '#' && prev_ws {
            end = i;
            break;
        }
        prev_ws = c == ' ' || c == '\t';
    }
    let value = raw[..end].trim_end_matches([' ', '\t']);
    let span = vstart..vstart + value.len();
    Some(ModelLine {
        value: value.to_string(),
        span,
    })
}

/// Extract the top-level `model:` value from a `.claude/agents/*.md` YAML frontmatter (the
/// leading `---`-delimited block): [`frontmatter_model`]'s value (unquoted, without a comment), or
/// `None` without the key.
pub fn agent_model(content: &str) -> Option<String> {
    frontmatter_model(content).map(|m| m.value)
}

/// Split a trailing 1M-context modifier `[1m]` (any case) off a model value: `(base, modifier as
/// spelled)`; the modifier is `""` without one.
pub fn split_context_modifier(model: &str) -> (&str, &str) {
    if let Some(s) = model.len().checked_sub(4) {
        if let Some(tail) = model.get(s..) {
            if tail.eq_ignore_ascii_case("[1m]") {
                return (&model[..s], tail);
            }
        }
    }
    (model, "")
}

/// The alias a validated CONCRETE Claude id floats to, its context modifier kept
/// (`claude-opus-4-8` -> `opus`, `claude-opus-4-8[1m]` -> `opus[1m]`). Only an id of the claude
/// engine's model table or of the published shape `claude-(fable|opus|sonnet|haiku)-<digits>
/// [-<digits>]` qualifies, case-sensitive like the table lookup; a custom `claude-opus-foo`, an older
/// naming (`claude-3-opus-20240229`), an upper-case spelling, an alias (`opus`, `opus[1m]`) or any
/// other value gives `None`.
pub fn concrete_claude_alias(model: &str) -> Option<String> {
    let (base, modifier) = split_context_modifier(model.trim());
    if CLAUDE_MODEL_ALIASES.contains(&base) {
        return None;
    }
    if !CLAUDE_MODELS.contains(&base) && !is_published_claude_shape(base) {
        return None;
    }
    let family = model_family(base);
    if family.is_empty() {
        return None;
    }
    Some(format!("{family}{modifier}"))
}

/// Classify an agent file's model pin. A missing `model:` line, an empty value, `inherit`, and the
/// floating aliases (`opus` / `sonnet` / `haiku` / `fable`, also with a `[1m]` modifier) are fine
/// and return `None`. Any other value is a concrete pin that should be an alias: returns
/// `Some((pinned_id, suggested_alias))`, where the alias is [`concrete_claude_alias`]
/// (`claude-opus-4-8` -> `opus`, `claude-opus-4-8[1m]` -> `opus[1m]`) or empty when the value is
/// not a validated concrete Claude id - such a pin is reported, never rewritten.
pub fn pinned_agent_model(content: &str) -> Option<(String, String)> {
    let model = agent_model(content)?;
    let m = model.trim();
    if m.is_empty() || m == "inherit" {
        return None;
    }
    let (base, _) = split_context_modifier(m);
    if CLAUDE_MODEL_ALIASES.contains(&base) {
        return None;
    }
    Some((model.clone(), concrete_claude_alias(m).unwrap_or_default()))
}

/// Whether a `.c3/*.md` body still carries a stub marker: a LINE that starts with `[STUB:` (the
/// form C3 writes) or with `[STUB]` (the form the rules name). Prose that merely mentions
/// `[STUB]` inside a line - a journal entry, the description of this very check - is not a stub.
pub fn has_stub_marker(content: &str) -> bool {
    content.lines().any(|l| {
        let l = l.trim_start();
        l.starts_with("[STUB:") || l.starts_with("[STUB]")
    })
}

/// Whether a `.claude/rules/*.md` body names any `eck_*` MCP tool.
pub fn rule_names_eck_tool(content: &str) -> bool {
    ECK_TOOL_NAMES.iter().any(|t| content.contains(t))
}

/// Whether a `.mcp.json` body registers a `c3 mcp` server: an entry whose command resolves
/// to a `c3` executable and whose args are exactly `["mcp"]` (the server name is
/// conventionally `c3` but is not required to be). Malformed JSON reads as "not registered".
pub fn mcp_registered(mcp_json: &str) -> bool {
    let Ok(v) = serde_json::from_str::<Value>(mcp_json) else {
        return false;
    };
    let Some(servers) = v.get("mcpServers").and_then(Value::as_object) else {
        return false;
    };
    servers.values().any(|entry| {
        let args_ok = entry
            .get("args")
            .and_then(Value::as_array)
            .map(|a| a.iter().any(|x| x.as_str() == Some("mcp")))
            .unwrap_or(false);
        let cmd_is_c3 = entry
            .get("command")
            .and_then(Value::as_str)
            .map(command_is_c3)
            .unwrap_or(false);
        args_ok && cmd_is_c3
    })
}

/// Whether a command path points at a `c3` executable (basename `c3`, `c3.exe`, ...).
fn command_is_c3(command: &str) -> bool {
    let base = command
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(command)
        .to_ascii_lowercase();
    base == "c3" || base.strip_suffix(".exe") == Some("c3")
}

/// Whether a `.mcp.json` body still registers an `eck-core` server.
pub fn mcp_json_has_eck_core(mcp_json: &str) -> bool {
    serde_json::from_str::<Value>(mcp_json)
        .ok()
        .and_then(|v| {
            v.get("mcpServers")
                .and_then(Value::as_object)
                .map(|s| s.contains_key(ECK_CORE))
        })
        .unwrap_or(false)
}

/// Whether a config body (TOML / JSON) still mentions `eck-core`. A plain substring test —
/// enough to flag the leftover in `.codex/config.toml` or `opencode.json`.
pub fn config_mentions_eck_core(content: &str) -> bool {
    content.contains(ECK_CORE)
}

/// Scan `repo_root` and build the report (the only I/O in this module).
pub fn scan(repo_root: &Path) -> DoctorReport {
    // C3's project folder, and the stub markers of its manifests (direct children; the legacy
    // `.eck/` is read until a writing command moved it).
    let mut report = DoctorReport {
        state_layout: crate::state_dir::layout(repo_root),
        git_version: installed_git_version().map(|(t, _)| t).unwrap_or_default(),
        git_problem: git_version_problem(),
        ..DoctorReport::default()
    };
    let state = crate::state_dir::read_path(repo_root);
    if let Some(dir) = &state {
        report.state_read = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
    }
    if let Some(Ok(rd)) = state.as_ref().map(std::fs::read_dir) {
        let mut names: Vec<String> = Vec::new();
        for e in rd.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if !name.to_lowercase().ends_with(".md") {
                continue;
            }
            if std::fs::read_to_string(e.path())
                .map(|c| has_stub_marker(&c))
                .unwrap_or(false)
            {
                names.push(format!("{}/{name}", report.state_read));
            }
        }
        names.sort();
        report.stub_files = names;
    }

    // `.mcp.json`: c3 registration and an eck-core leftover.
    let mcp_path = repo_root.join(".mcp.json");
    if let Ok(text) = std::fs::read_to_string(&mcp_path) {
        report.mcp_registered = mcp_registered(&text);
        if mcp_json_has_eck_core(&text) {
            report.eck_core_in.push(".mcp.json".to_string());
        }
    }

    // eck-core leftovers in the codex / opencode configs.
    for rel in [".codex/config.toml", "opencode.json"] {
        if let Ok(text) = std::fs::read_to_string(repo_root.join(rel)) {
            if config_mentions_eck_core(&text) {
                report.eck_core_in.push(rel.to_string());
            }
        }
    }
    report.eck_core_in.sort();

    // `.claude/rules/*.md` naming an eck_* tool.
    let rules = repo_root.join(".claude").join("rules");
    if let Ok(rd) = std::fs::read_dir(&rules) {
        let mut names: Vec<String> = Vec::new();
        for e in rd.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if !name.to_lowercase().ends_with(".md") {
                continue;
            }
            if std::fs::read_to_string(e.path())
                .map(|c| rule_names_eck_tool(&c))
                .unwrap_or(false)
            {
                names.push(format!(".claude/rules/{name}"));
            }
        }
        names.sort();
        report.eck_rule_files = names;
    }

    // `.claude/agents/*.md` pinning a concrete model id instead of a floating alias.
    let agents = repo_root.join(".claude").join("agents");
    if let Ok(rd) = std::fs::read_dir(&agents) {
        let mut pins: Vec<PinnedAgent> = Vec::new();
        for e in rd.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if !name.to_lowercase().ends_with(".md") {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(e.path()) {
                if let Some((model, alias)) = pinned_agent_model(&content) {
                    pins.push(PinnedAgent {
                        file: format!(".claude/agents/{name}"),
                        model,
                        alias,
                    });
                }
            }
        }
        pins.sort_by(|a, b| a.file.cmp(&b.file));
        report.pinned_agents = pins;
    }

    // The delegation rule against the template's current sections, and the rules files that still
    // name the legacy folder.
    report.delegation_rule = crate::setup::rules::scan(repo_root);
    report.state_sentences = crate::setup::rules::scan_state_sentences(repo_root);

    // `.c3/snapshots/` size and count.
    if let Some(Ok(rd)) = state.map(|d| std::fs::read_dir(d.join("snapshots"))) {
        for e in rd.flatten() {
            if e.file_type().map(|t| t.is_file()).unwrap_or(false) {
                report.snapshot_count += 1;
                report.snapshot_bytes += e.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }

    // Branches and worktrees without a recorded state; the start folder (read-only, no cherry).
    report.branches = branches::quick_scan(repo_root, true);

    report
}

/// Resolve the `.c3/snapshots` path (for messages/tests).
pub fn snapshots_dir(repo_root: &Path) -> PathBuf {
    crate::state_dir::path(repo_root).join("snapshots")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_and_rule_detectors() {
        assert!(has_stub_marker("# X\n\n[STUB: fill me]\n"));
        // (0.2.5, S1) the form the templates and rules name counts too, at the start of a line
        assert!(has_stub_marker("# X\n\n[STUB]\n"));
        assert!(has_stub_marker("# X\n\n[STUB] describe the deploy\n"));
        assert!(!has_stub_marker("# X\n\nresolved\n"));
        // prose that mentions the marker is not a stub (a journal entry, a command description)
        assert!(!has_stub_marker(
            "- `c3 doctor` reports leftover `[STUB]` markers\n- 3 manifests still [STUB]\n"
        ));
        assert!(has_stub_marker("# X\n\n  [STUB: indented]\n"));
        assert!(rule_names_eck_tool("call `eck_finish_task` now"));
        assert!(rule_names_eck_tool("use eck_manifest_edit"));
        assert!(!rule_names_eck_tool("call `c3_finish_task` now"));
    }

    #[test]
    fn mcp_registered_recognises_a_c3_mcp_entry() {
        assert!(mcp_registered(
            r#"{"mcpServers":{"c3":{"command":"/abs/c3","args":["mcp"]}}}"#
        ));
        assert!(mcp_registered(
            r#"{"mcpServers":{"whatever":{"command":"C:\\bin\\c3.exe","args":["mcp"]}}}"#
        ));
        // A non-c3 command, or no `mcp` arg, is not a c3 registration.
        assert!(!mcp_registered(
            r#"{"mcpServers":{"eck-core":{"command":"node","args":["x.js"]}}}"#
        ));
        assert!(!mcp_registered(
            r#"{"mcpServers":{"c3":{"command":"/abs/c3","args":["providers"]}}}"#
        ));
        assert!(!mcp_registered("not json"));
    }

    #[test]
    fn eck_core_detectors() {
        assert!(mcp_json_has_eck_core(
            r#"{"mcpServers":{"eck-core":{"command":"node","args":[]}}}"#
        ));
        assert!(!mcp_json_has_eck_core(
            r#"{"mcpServers":{"c3":{"command":"c3","args":["mcp"]}}}"#
        ));
        assert!(config_mentions_eck_core(
            "[mcp_servers.eck-core]\ncommand = \"node\"\n"
        ));
        assert!(!config_mentions_eck_core("[mcp_servers.other]\n"));
    }

    // (0.2.5) The git version is read from `git --version` in every shape git prints it; 2.30 is
    // the oldest that takes `--end-of-options`; a git problem makes the workspace dirty.
    #[test]
    fn the_git_version_is_parsed_and_compared_with_2_30() {
        assert_eq!(
            parse_git_version("git version 2.45.1.windows.1\n"),
            Some((2, 45, 1))
        );
        assert_eq!(
            parse_git_version("git version 2.39.3 (Apple Git-145)"),
            Some((2, 39, 3))
        );
        assert_eq!(parse_git_version("git version 2.30"), Some((2, 30, 0)));
        assert_eq!(parse_git_version("git version 2.29.2"), Some((2, 29, 2)));
        assert_eq!(parse_git_version("hello"), None);
        assert_eq!(parse_git_version("git version x"), None);
        assert!((2, 29) < MIN_GIT && (2, 30) >= MIN_GIT && (3, 0) >= MIN_GIT);
        let r = DoctorReport {
            mcp_registered: true,
            git_problem: Some("git 2.29.2 is installed; C3 needs git 2.30 or later".into()),
            ..DoctorReport::default()
        };
        assert!(!r.is_clean());
    }

    #[test]
    fn report_is_clean_only_when_all_good() {
        let mut r = DoctorReport {
            mcp_registered: true,
            snapshot_count: 3,
            snapshot_bytes: 9000,
            ..DoctorReport::default()
        };
        assert!(r.is_clean(), "snapshots alone must not make it dirty");
        r.stub_files.push(".c3/ROADMAP.md".into());
        assert!(!r.is_clean());
        r.stub_files.clear();
        r.mcp_registered = false;
        assert!(!r.is_clean());
        r.mcp_registered = true;
        assert!(r.is_clean());
        // A pinned agent model makes the workspace dirty.
        r.pinned_agents.push(PinnedAgent {
            file: ".claude/agents/opus-worker.md".into(),
            model: "claude-opus-4-8".into(),
            alias: "opus".into(),
        });
        assert!(!r.is_clean());
    }

    #[test]
    fn agent_model_reads_the_frontmatter_value() {
        assert_eq!(
            agent_model("---\nname: x\nmodel: opus\n---\nbody\n").as_deref(),
            Some("opus")
        );
        // Quoted value.
        assert_eq!(
            agent_model("---\nmodel: \"claude-opus-4-8\"\n---\n").as_deref(),
            Some("claude-opus-4-8")
        );
        // No model line.
        assert_eq!(agent_model("---\nname: x\n---\nbody\n"), None);
        // No frontmatter at all.
        assert_eq!(agent_model("# just prose\nmodel: opus\n"), None);
        // An indented `model:` inside a block scalar is NOT the frontmatter key.
        assert_eq!(
            agent_model("---\ndescription: >\n  model: not this\nname: x\n---\n"),
            None
        );
    }

    #[test]
    fn pinned_agent_model_accepts_aliases_and_flags_ids() {
        // Aliases, inherit and a missing line are fine.
        assert_eq!(pinned_agent_model("---\nmodel: opus\n---\n"), None);
        assert_eq!(pinned_agent_model("---\nmodel: sonnet\n---\n"), None);
        assert_eq!(pinned_agent_model("---\nmodel: haiku\n---\n"), None);
        assert_eq!(pinned_agent_model("---\nmodel: fable\n---\n"), None);
        assert_eq!(pinned_agent_model("---\nmodel: inherit\n---\n"), None);
        assert_eq!(pinned_agent_model("---\nname: x\n---\n"), None);
        // A concrete id is flagged, with the alias derived from the family.
        assert_eq!(
            pinned_agent_model("---\nmodel: claude-opus-4-8\n---\n"),
            Some(("claude-opus-4-8".into(), "opus".into()))
        );
        assert_eq!(
            pinned_agent_model("---\nmodel: claude-sonnet-5\n---\n"),
            Some(("claude-sonnet-5".into(), "sonnet".into()))
        );
        // An id that names no known family -> no alias suggested.
        assert_eq!(
            pinned_agent_model("---\nmodel: claude-3-opus-20240229\n---\n"),
            Some(("claude-3-opus-20240229".into(), String::new()))
        );
    }

    // (0.2.5, F02-3 / F03-4) Only a validated CONCRETE Claude id gets an alias (and so a repair); an
    // alias with a modifier is floating already; a supported modifier stays on the alias.
    #[test]
    fn only_a_validated_concrete_claude_id_gets_an_alias() {
        // floating already: an alias with the 1M-context modifier
        assert_eq!(pinned_agent_model("---\nmodel: opus[1m]\n---\n"), None);
        assert_eq!(pinned_agent_model("---\nmodel: sonnet[1M]\n---\n"), None);
        // a concrete id with the modifier keeps it on the alias
        assert_eq!(
            pinned_agent_model("---\nmodel: claude-opus-4-8[1m]\n---\n"),
            Some(("claude-opus-4-8[1m]".into(), "opus[1m]".into()))
        );
        // the published shape of a model newer than the table
        assert_eq!(
            pinned_agent_model("---\nmodel: claude-haiku-6-1\n---\n"),
            Some(("claude-haiku-6-1".into(), "haiku".into()))
        );
        // custom or unvalidated values: reported as a pin, never given an alias
        for v in [
            "claude-opus-foo",
            "claude-opus-4-8-custom",
            "CLAUDE-OPUS-4-8",
            "Opus",
            "glm-5.3",
            "claude-opus-4-8[2m]",
        ] {
            assert_eq!(
                pinned_agent_model(&format!("---\nmodel: {v}\n---\n")),
                Some((v.to_string(), String::new())),
                "{v}"
            );
        }
        assert_eq!(concrete_claude_alias("opus"), None);
        assert_eq!(
            concrete_claude_alias("claude-sonnet-5-5"),
            Some("sonnet".into())
        );
    }

    // (0.2.5, F03-4) Doctor and repair read the key by ONE rule: `model :` counts, an indented key
    // does not, `model:x` is no key; the value span excludes the quotes and a trailing comment.
    #[test]
    fn the_frontmatter_model_key_and_value_span() {
        let m = |s: &str| frontmatter_model(s).map(|l| (l.value.clone(), s[l.span].to_string()));
        assert_eq!(
            m("---\nmodel : claude-opus-4-8\n---\n"),
            Some(("claude-opus-4-8".into(), "claude-opus-4-8".into()))
        );
        assert_eq!(
            m("---\nmodel:\tclaude-opus-4-8   # pinned on purpose\n---\n"),
            Some(("claude-opus-4-8".into(), "claude-opus-4-8".into()))
        );
        assert_eq!(
            m("---\nmodel: \"claude-opus-4-8\" # q\n---\n"),
            Some(("claude-opus-4-8".into(), "claude-opus-4-8".into()))
        );
        assert_eq!(
            m("---\nmodel: 'claude-opus-4-8'\n---\n"),
            Some(("claude-opus-4-8".into(), "claude-opus-4-8".into()))
        );
        assert_eq!(
            m("---\nmodel:\n---\n"),
            Some((String::new(), String::new()))
        );
        assert_eq!(
            m("---\nmodel: # only a comment\n---\n"),
            Some((String::new(), String::new()))
        );
        // not the key
        assert_eq!(m("---\n  model: claude-opus-4-8\n---\n"), None);
        assert_eq!(m("---\nmodel:claude-opus-4-8\n---\n"), None);
        assert_eq!(m("---\nmodels: x\n---\n"), None);
        assert_eq!(m("---\nmodel: \"unterminated\n---\n"), None);
        assert_eq!(m("---\nname: x\n---\nmodel: claude-opus-4-8\n"), None);
        // a `#` inside a plain scalar (not after white space) is part of it
        assert_eq!(
            m("---\nmodel: a#b\n---\n"),
            Some(("a#b".into(), "a#b".into()))
        );
        // the doctor's report reads the same key
        assert_eq!(
            pinned_agent_model("---\nmodel : claude-opus-4-8 # c\n---\n"),
            Some(("claude-opus-4-8".into(), "opus".into()))
        );
        assert_eq!(
            pinned_agent_model("---\nmodel:claude-opus-4-8\n---\n"),
            None
        );
    }
}
