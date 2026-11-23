//! `c3 init` — provision a repository for the C3 workflow (port spec, Part C).
//!
//! It makes sure C3's project folder `.c3/` exists ([`crate::state_dir::ensure`]: an existing
//! `.eck/` is moved once, the missing manifests are created, each with a `[STUB: ...]` marker,
//! and `.c3/` is added to `.gitignore` when no line ignores it), writes the protocol/delegation
//! rules, the three worker-agent definitions and a `CLAUDE.md` stub ONLY when they are missing
//! (never overwriting), and registers `c3 mcp` in `.mcp.json` (adding a server, keeping every
//! other one). The project-file templates are this repository's own files, embedded with
//! `include_str!` from `crates/c3/templates/`.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::state_dir::{self, GITIGNORE_LINE, STATE_DIR};

/// The `.c3/<NAME>.md` manifests C3 stubs out, in the spec's order.
pub const STUB_MANIFESTS: &[&str] = &[
    "CONTEXT",
    "ARCHITECTURE",
    "ROADMAP",
    "TECH_DEBT",
    "JOURNAL",
    "OPERATIONS",
    "ENVIRONMENT",
    "RUNTIME_STATE",
    "DEPLOY_CHECKLIST",
];

/// The project files written only when missing: (repo-relative path, embedded template).
/// The rules and agents are this repository's own files; `01-c3-protocol.md` is its
/// `01-eck-protocol.md` with the `eck_*` tool names and "eckSnapshot" renamed to C3.
pub const PROJECT_FILES: &[(&str, &str)] = &[
    ("CLAUDE.md", include_str!("../../templates/CLAUDE.md")),
    (
        ".claude/rules/01-c3-protocol.md",
        include_str!("../../templates/01-c3-protocol.md"),
    ),
    (
        ".claude/rules/02-delegation.md",
        include_str!("../../templates/02-delegation.md"),
    ),
    (
        ".claude/agents/sonnet-worker.md",
        include_str!("../../templates/sonnet-worker.md"),
    ),
    (
        ".claude/agents/opus-worker.md",
        include_str!("../../templates/opus-worker.md"),
    ),
    (
        ".claude/agents/haiku-worker.md",
        include_str!("../../templates/haiku-worker.md"),
    ),
];

/// The MCP server name `c3 init` registers in `.mcp.json`.
pub const MCP_SERVER_NAME: &str = "c3";

/// Options for [`run`].
#[derive(Debug, Clone)]
pub struct InitOpts {
    pub repo_root: PathBuf,
    /// The collab root, whose legacy cache `<collab>/.c3/` moves into `<repo>/.c3/`.
    pub collab_root: Option<PathBuf>,
    /// Overwrite the `.c3/*.md` manifests with a fresh stub even when they already exist.
    pub force_stubs: bool,
    /// Absolute path of the running `c3` executable, written as the `.mcp.json` command.
    pub c3_exe: String,
}

/// What one `c3 init` run did.
#[derive(Debug, Clone, Default)]
pub struct InitResult {
    /// Paths created (or, for `--force-stubs`, overwritten).
    pub created: Vec<String>,
    /// Paths skipped because they already existed.
    pub skipped: Vec<String>,
    /// A human line describing the `.gitignore` action.
    pub gitignore: String,
    /// A human line describing the `.mcp.json` action.
    pub mcp: String,
    /// One line per move of the project folder (`.eck/` -> `.c3/`, the legacy collab cache).
    pub moved: Vec<String>,
}

/// The stub body of a manifest that does not exist yet: `# <NAME>\n\n[STUB: ...]\n`.
pub fn stub_content(name: &str) -> String {
    format!(
        "# {name}\n\n[STUB: describe {name} for this project. \
Resolve this marker (remove `[STUB:` once filled); `c3 doctor` reports every manifest that \
still carries one.]\n"
    )
}

/// Ensure `line` is present in a `.gitignore` body. Returns `Some(new_body)` when the line
/// had to be added (idempotent: `None` when it was already present as a whole line).
pub fn ensure_gitignore_line(existing: &str, line: &str) -> Option<String> {
    let present = existing
        .lines()
        .any(|l| l.trim_end() == line || l.trim_end() == line.trim_end_matches('/'));
    if present {
        return None;
    }
    let mut out = existing.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(line);
    out.push('\n');
    Some(out)
}

/// Merge a `c3 mcp` server entry into a `.mcp.json` body, keeping every other server.
/// `existing` is `None` when the file is absent (a fresh `{ "mcpServers": { ... } }` is
/// created). Returns `(new_json, added)` — `added` is `false` when a server of this name is
/// already present (idempotent; the existing entry is left untouched). The JSON is
/// pretty-printed with a trailing newline; `serde_json`'s `preserve_order` keeps the other
/// servers in their original order.
pub fn merge_mcp_json(
    existing: Option<&str>,
    server: &str,
    command: &str,
    args: &[&str],
) -> Result<(String, bool), String> {
    let mut root: Value = match existing {
        Some(text) if !text.trim().is_empty() => {
            serde_json::from_str(text).map_err(|e| format!("parse .mcp.json: {e}"))?
        }
        _ => json!({}),
    };
    if !root.is_object() {
        return Err(".mcp.json is not a JSON object".to_string());
    }
    let obj = root.as_object_mut().unwrap();
    let servers = obj.entry("mcpServers").or_insert_with(|| json!({}));
    let servers = servers
        .as_object_mut()
        .ok_or_else(|| ".mcp.json \"mcpServers\" is not an object".to_string())?;
    if servers.contains_key(server) {
        // Idempotent: leave an existing entry of this name alone.
        let text = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
        return Ok((format!("{text}\n"), false));
    }
    servers.insert(
        server.to_string(),
        json!({ "command": command, "args": args }),
    );
    let text = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    Ok((format!("{text}\n"), true))
}

/// Write `path` with `content`, creating parent directories. Records the repo-relative
/// label into `created`.
fn write_new(
    repo_root: &Path,
    rel: &str,
    content: &str,
    created: &mut Vec<String>,
) -> Result<(), String> {
    let path = repo_root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    std::fs::write(&path, content).map_err(|e| format!("write {rel}: {e}"))?;
    created.push(rel.to_string());
    Ok(())
}

/// Run `c3 init`. Idempotent: a second run on the same tree creates nothing (and, without
/// `--force-stubs`, overwrites nothing).
pub fn run(opts: &InitOpts) -> Result<InitResult, String> {
    let repo = &opts.repo_root;
    let mut result = InitResult::default();

    // 1-2. The project folder: an existing `.eck/` moved once, `.c3/` created, each missing
    //    `.c3/<NAME>.md` manifest stubbed, `.c3/` git-ignored. Existing manifests are overwritten
    //    only with `--force-stubs`.
    let ensured = state_dir::ensure(repo, opts.collab_root.as_deref())?;
    result.moved = ensured.moved.clone();
    // `.c3`, or the legacy folder for this run when its move had to wait
    let dir_name = ensured
        .dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| STATE_DIR.to_string());
    // (a move that had to wait leaves the legacy folder as it is: no stubs this run)
    for name in STUB_MANIFESTS.iter().filter(|_| ensured.deferred.is_none()) {
        let rel = format!("{dir_name}/{name}.md");
        if ensured.created.contains(&rel) {
            result.created.push(rel);
        } else if opts.force_stubs {
            write_new(repo, &rel, &stub_content(name), &mut result.created)?;
        } else {
            result.skipped.push(rel);
        }
    }
    result.gitignore = if ensured.gitignore_added {
        format!("added `{GITIGNORE_LINE}` to .gitignore")
    } else {
        format!("`{GITIGNORE_LINE}` already ignored by .gitignore")
    };

    // 3. Project files — written ONLY when missing, never overwritten.
    for (rel, content) in PROJECT_FILES {
        if repo.join(rel).exists() {
            result.skipped.push((*rel).to_string());
            continue;
        }
        write_new(repo, rel, content, &mut result.created)?;
    }

    // 4. Register `c3 mcp` in `.mcp.json`, keeping every other server.
    let mcp_path = repo.join(".mcp.json");
    let existing = std::fs::read_to_string(&mcp_path).ok();
    let (merged, added) =
        merge_mcp_json(existing.as_deref(), MCP_SERVER_NAME, &opts.c3_exe, &["mcp"])?;
    if added {
        std::fs::write(&mcp_path, merged).map_err(|e| format!("write .mcp.json: {e}"))?;
        result.mcp = format!("registered `{MCP_SERVER_NAME}` (c3 mcp) in .mcp.json");
    } else {
        result.mcp = format!("`{MCP_SERVER_NAME}` already registered in .mcp.json");
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_content_has_title_and_marker() {
        let s = stub_content("ROADMAP");
        assert!(s.starts_with("# ROADMAP\n"));
        assert!(s.contains("[STUB:"));
        assert!(s.ends_with('\n'));
    }

    #[test]
    fn gitignore_add_is_idempotent() {
        // Missing → added.
        let added = ensure_gitignore_line("target/\n", GITIGNORE_LINE).unwrap();
        assert!(added.contains(".c3/"));
        assert!(added.contains("target/"));
        // Already present (exact) → no change.
        assert!(ensure_gitignore_line(&added, GITIGNORE_LINE).is_none());
        // Present without trailing slash still counts.
        assert!(ensure_gitignore_line("target/\n.c3\n", GITIGNORE_LINE).is_none());
        // Empty file → just the line.
        assert_eq!(ensure_gitignore_line("", GITIGNORE_LINE).unwrap(), ".c3/\n");
        // No trailing newline on the existing body is handled.
        let a = ensure_gitignore_line("target/", GITIGNORE_LINE).unwrap();
        assert_eq!(a, "target/\n.c3/\n");
        // The old folder's line does not ignore the new folder.
        assert_eq!(
            ensure_gitignore_line(".eck/\n", GITIGNORE_LINE).unwrap(),
            ".eck/\n.c3/\n"
        );
    }

    #[test]
    fn mcp_merge_keeps_other_servers_and_is_idempotent() {
        let existing = r#"{
  "mcpServers": {
    "eck-core": { "command": "node", "args": ["mcp-eck-core.js"] }
  }
}"#;
        let (merged, added) = merge_mcp_json(Some(existing), "c3", "/abs/c3", &["mcp"]).unwrap();
        assert!(added);
        let v: Value = serde_json::from_str(&merged).unwrap();
        // Both servers present.
        assert!(v["mcpServers"]["eck-core"].is_object());
        assert_eq!(v["mcpServers"]["c3"]["command"], "/abs/c3");
        assert_eq!(v["mcpServers"]["c3"]["args"][0], "mcp");
        // preserve_order keeps eck-core first.
        let names: Vec<&str> = v["mcpServers"]
            .as_object()
            .unwrap()
            .keys()
            .map(|s| s.as_str())
            .collect();
        assert_eq!(names, vec!["eck-core", "c3"]);

        // A second merge is idempotent: nothing added, the entry untouched.
        let (again, added2) = merge_mcp_json(Some(&merged), "c3", "/other/c3", &["mcp"]).unwrap();
        assert!(!added2);
        let v2: Value = serde_json::from_str(&again).unwrap();
        assert_eq!(v2["mcpServers"]["c3"]["command"], "/abs/c3");
    }

    #[test]
    fn mcp_merge_creates_file_when_absent() {
        let (merged, added) = merge_mcp_json(None, "c3", "/abs/c3", &["mcp"]).unwrap();
        assert!(added);
        let v: Value = serde_json::from_str(&merged).unwrap();
        assert_eq!(v["mcpServers"]["c3"]["command"], "/abs/c3");
        assert!(merged.ends_with('\n'));
    }

    #[test]
    fn mcp_merge_refuses_non_object() {
        assert!(merge_mcp_json(Some("[1,2,3]"), "c3", "/c3", &["mcp"]).is_err());
    }
}
