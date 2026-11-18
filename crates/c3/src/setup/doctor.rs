//! `c3 doctor` — report what still needs doing in a C3 workspace (port spec, Part C).
//!
//! It reports: `[STUB` markers left in `.eck/*.md`; whether `c3 mcp` is registered in
//! `.mcp.json`; eck leftovers (an `eck-core` server in `.mcp.json` / `.codex/config.toml` /
//! `opencode.json`, and any `.claude/rules/*.md` that still names an `eck_*` tool); any
//! `.claude/agents/*.md` whose frontmatter pins a concrete model id instead of a floating
//! alias; branches not merged and without a recorded state, and worktrees without one
//! ([`crate::setup::branches`]); and the size and file count of `.eck/snapshots/`. Exit 0 when
//! there is nothing to fix, 1 otherwise. The snapshot-folder size and the start folder's branch /
//! uncommitted changes are diagnostic context only and never force exit 1.

use std::path::{Path, PathBuf};

use serde_json::Value;

use c3_core::claude::{model_family, CLAUDE_MODEL_ALIASES};

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
    /// `.eck/*.md` files that still carry a `[STUB` marker (repo-relative).
    pub stub_files: Vec<String>,
    /// Whether a `c3 mcp` server is registered in `.mcp.json`.
    pub mcp_registered: bool,
    /// Config files that still reference an `eck-core` MCP server (repo-relative).
    pub eck_core_in: Vec<String>,
    /// `.claude/rules/*.md` files that still name an `eck_*` tool (repo-relative).
    pub eck_rule_files: Vec<String>,
    /// `.claude/agents/*.md` files whose `model:` pins a concrete id instead of an alias.
    pub pinned_agents: Vec<PinnedAgent>,
    /// Number of `*.md` files in `.eck/snapshots/`.
    pub snapshot_count: usize,
    /// Total bytes of those snapshot files.
    pub snapshot_bytes: u64,
    /// Branches and worktrees (`c3 branches`, cheap scan); `None` outside a git repository.
    pub branches: Option<BranchReport>,
}

impl DoctorReport {
    /// A clean workspace: no stubs left, `c3 mcp` registered, no eck leftovers, no agent file
    /// pinning an old model, no unknown branch and no worktree without a recorded state. The
    /// snapshot folder size and the start folder's branch / uncommitted changes are informational
    /// and do not count.
    pub fn is_clean(&self) -> bool {
        self.stub_files.is_empty()
            && self.mcp_registered
            && self.eck_core_in.is_empty()
            && self.eck_rule_files.is_empty()
            && self.pinned_agents.is_empty()
            && self.branches.as_ref().is_none_or(BranchReport::is_clean)
    }
}

/// Extract the top-level `model:` value from a `.claude/agents/*.md` YAML frontmatter (the
/// leading `---`-delimited block). Returns the value (trimmed, unquoted) when a `model:` key
/// is present, else `None`. Only un-indented keys of the frontmatter are considered, so a
/// `model:` inside an indented block scalar (e.g. the `description`) is not mistaken for one.
pub fn agent_model(content: &str) -> Option<String> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut lines = content.lines();
    if lines.next().map(str::trim) != Some("---") {
        return None;
    }
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if let Some(rest) = line.strip_prefix("model:") {
            let v = rest.trim().trim_matches('"').trim_matches('\'').trim();
            return Some(v.to_string());
        }
    }
    None
}

/// Classify an agent file's model pin. A missing `model:` line, `inherit`, and the floating
/// aliases (`opus` / `sonnet` / `haiku` / `fable`) are fine and return `None`. Any other value
/// is a concrete pin that should be an alias: returns `Some((pinned_id, suggested_alias))`,
/// where the alias is derived from the id's family (`claude-opus-4-8` -> `opus`) or empty when
/// the id names no known family.
pub fn pinned_agent_model(content: &str) -> Option<(String, String)> {
    let model = agent_model(content)?;
    let m = model.trim();
    if m.is_empty() || m == "inherit" || CLAUDE_MODEL_ALIASES.contains(&m) {
        return None;
    }
    Some((model.clone(), model_family(m)))
}

/// Whether a `.eck/*.md` body still carries a stub marker: a LINE that starts with `[STUB:` (the
/// form `c3 init` and eck write). Prose that merely mentions `[STUB]` - a journal entry, the
/// description of this very check - is not a stub.
pub fn has_stub_marker(content: &str) -> bool {
    content
        .lines()
        .any(|l| l.trim_start().starts_with("[STUB:"))
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
    let mut report = DoctorReport::default();

    // `.eck/*.md` stub markers (direct children of `.eck/`).
    let eck = repo_root.join(".eck");
    if let Ok(rd) = std::fs::read_dir(&eck) {
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
                names.push(format!(".eck/{name}"));
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

    // `.eck/snapshots/` size and count.
    let snaps = eck.join("snapshots");
    if let Ok(rd) = std::fs::read_dir(&snaps) {
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

/// Resolve the `.eck/snapshots` path (for messages/tests).
pub fn snapshots_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".eck").join("snapshots")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_and_rule_detectors() {
        assert!(has_stub_marker("# X\n\n[STUB: fill me]\n"));
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

    #[test]
    fn report_is_clean_only_when_all_good() {
        let mut r = DoctorReport {
            mcp_registered: true,
            snapshot_count: 3,
            snapshot_bytes: 9000,
            ..DoctorReport::default()
        };
        assert!(r.is_clean(), "snapshots alone must not make it dirty");
        r.stub_files.push(".eck/ROADMAP.md".into());
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
}
