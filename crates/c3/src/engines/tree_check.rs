//! The read-only tree fingerprint taken before and after an `agy`/`muse` run.
//!
//! `agy`'s `--sandbox` restricts the terminal only and `muse` runs with its write/shell tools
//! off, so neither CLI's sandbox actually blocks a write to the repository. The bridge instead
//! guarantees read-only by *evidence*: it fingerprints the working tree before the first turn
//! and again after the last one, and FAILS the run (a forced `provider_failure`, class
//! `tampered`/`unknown` decided by the orchestrator) when the content changed
//! (`codex-consult.ps1` `Get-EngineTreeProblem`, wave 24b F08-2).
//!
//! This is the *content-only* fingerprint of [`crate::consult::revision`]: a commit or a
//! `git add` that leaves file contents identical is NOT a change, only a `revision_moved`
//! (informational). The collab directory is fingerprinted SEPARATELY by the orchestrator (a
//! directory snapshot with the member's own handoff prefixes ignored — `Compare-DirectorySnapshot`),
//! because `revision`'s content hash excludes the collab dir; this module owns only the
//! tracked+untracked working-tree half, exactly as the plugin's tree check reuses
//! `Compare-TreeContent` for it.

use std::collections::HashMap;
use std::path::Path;

use crate::consult::revision::{self, RevisionInfo, TreeComparison};

/// Fingerprint the working tree under `root` (tracked + untracked file contents), excluding
/// `collab_root`. Call it once before the first turn of a run and once after the last turn.
pub fn snapshot(root: &Path, collab_root: Option<&Path>) -> RevisionInfo {
    revision::revision_info(root, collab_root)
}

/// Compare two [`snapshot`]s by content. `changed` is set when a file's content appeared,
/// disappeared or changed during the run (the read-only rule is violated); `revision_moved`
/// records a moved HEAD with identical contents (informational, not a change).
pub fn compare(before: &RevisionInfo, after: &RevisionInfo) -> TreeComparison {
    revision::compare_tree_content(before, after)
}

/// Whether the tree content changed between `before` and `after` — the one bit the read-only
/// rule turns on (a convenience over [`compare`] for callers that ignore `revision_moved`).
pub fn changed(before: &RevisionInfo, after: &RevisionInfo) -> bool {
    compare(before, after).changed
}

/// `Get-CollabSnapshot`: a map of `<'/'-separated relative path> -> "<length>|<sha256>"` for every
/// file under `dir`, recursively. The bridge's own control files (`.consult.*` / `..consult.*`,
/// case-insensitive) and the `.git` directory are excluded; symlinks/junctions are not followed.
/// An absent directory yields an empty map. `sessions.json`, `findings.json`, `state.md` and the
/// handoffs ARE included (the caller ignores this run's own handoff prefix in the comparison).
pub fn collab_snapshot(dir: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if !dir.is_dir() {
        return out;
    }
    walk_collab(dir, dir, &mut out);
    out
}

fn walk_collab(root: &Path, cur: &Path, out: &mut HashMap<String, String>) {
    let rd = match std::fs::read_dir(cur) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    for entry in rd.flatten() {
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().to_string();
        if meta.file_type().is_symlink() {
            continue; // never follow a reparse point (junction/symlink)
        }
        if meta.is_dir() {
            if name == ".git" {
                continue;
            }
            walk_collab(root, &entry.path(), out);
            continue;
        }
        // Skip the bridge's own control files (lock, recovery records, atomic-write temps).
        let low = name.to_ascii_lowercase();
        if low.starts_with(".consult.") || low.starts_with("..consult.") {
            continue;
        }
        let path = entry.path();
        let rel = match path.strip_prefix(root) {
            Ok(p) => p.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        let bytes = std::fs::read(&path).unwrap_or_default();
        let sha = c3_core::sha256_hex(&bytes);
        out.insert(rel, format!("{}|{}", bytes.len(), sha));
    }
}

/// `Compare-DirectorySnapshot`: the collab-relative paths whose `length|sha256` differs (or which
/// appeared/disappeared) between `before` and `after` AND belong to THIS run. A path is skipped
/// when it:
///   * lives under ANOTHER task's directory (`<other-task>/...`) — those belong to other
///     consultations/sessions running in the same collab dir (TECH_DEBT instances 1-4);
///   * is a bridge control/status/log/lock file (`.consult.*` / `..consult.*`) at any depth;
///   * is one of this task's coordinator-owned ledger/state files — `<task>/state.md`,
///     `<task>/findings.json`, `<task>/sessions.json` and their atomic-write temps
///     (`<task>/.findings.json.*`, `<task>/.sessions.json.*`);
///   * starts with one of `ignore_prefixes` (this run's own handoff prefix and, for a panel
///     member, its siblings' handoffs — case-insensitive).
///
/// Ordinal-sorted.
pub fn compare_collab(
    before: &HashMap<String, String>,
    after: &HashMap<String, String>,
    task: &str,
    ignore_prefixes: &[String],
) -> Vec<String> {
    let task_ignores = task_owned_ignores(task);
    let mut keys: std::collections::BTreeSet<&String> = std::collections::BTreeSet::new();
    keys.extend(before.keys());
    keys.extend(after.keys());
    let mut changed: Vec<String> = Vec::new();
    for k in keys {
        if belongs_to_other_task(k, task) {
            continue;
        }
        if is_control_file(k) {
            continue;
        }
        let low = k.to_ascii_lowercase();
        if task_ignores.iter().any(|p| low.starts_with(p)) {
            continue;
        }
        if ignore_prefixes
            .iter()
            .any(|p| low.starts_with(&p.to_ascii_lowercase()))
        {
            continue;
        }
        match (before.get(k), after.get(k)) {
            (Some(a), Some(b)) if a == b => {}
            _ => changed.push(k.clone()),
        }
    }
    changed.sort();
    changed
}

/// Whether a collab-relative `key` lives under a task directory OTHER than `task`. A top-level file
/// (no `/`) is not under any task directory and is kept under watch.
fn belongs_to_other_task(key: &str, task: &str) -> bool {
    match key.split_once('/') {
        Some((seg, _)) => !seg.eq_ignore_ascii_case(task),
        None => false,
    }
}

/// Whether `key`'s basename is one of the bridge's control/status/log/lock files
/// (`.consult.*` / `..consult.*`, case-insensitive). `collab_snapshot` already drops these, so this
/// is a defensive second gate that also covers such a file under any task subdirectory.
fn is_control_file(key: &str) -> bool {
    let base = key.rsplit('/').next().unwrap_or(key).to_ascii_lowercase();
    base.starts_with(".consult.") || base.starts_with("..consult.")
}

/// The lowercased prefixes of THIS task's coordinator-owned ledger/state files that every engine's
/// tree check ignores: `state.md` (the coordinator writes it mid-run), the two JSON stores and
/// their `Write-TextAtomic` temps (`.<store>.<id>`).
fn task_owned_ignores(task: &str) -> Vec<String> {
    let t = task.to_ascii_lowercase();
    vec![
        format!("{t}/state.md"),
        format!("{t}/findings.json"),
        format!("{t}/.findings.json."),
        format!("{t}/sessions.json"),
        format!("{t}/.sessions.json."),
    ]
}

/// Whether an `agy` tool name is a writing/command-running tool BY NAME ALONE (the first test of
/// [`agy_tool_step_is_writing`], which also reads the step's logged parameters). The name test uses
/// an EXACT allow-list, never a substring match (a substring match let writing tools pass: `ls`
/// matched `tools`/`calls`, `find` matched `find_and_replace`, `read` matched `find_and_replace`'s
/// neighbours). Only a name on [`AGY_READ_ONLY_TOOLS`] is read-only; EVERYTHING else — an unknown
/// name, a writing/command tool, and an empty name — is writing (fail-safe: the tree check only
/// consults this once the tree already changed, so classifying an unlisted tool as writing turns a
/// `warned` into a `failed`, never the reverse).
pub fn agy_tool_is_writing(tool: &str) -> bool {
    let t = tool.trim().to_ascii_lowercase();
    !AGY_READ_ONLY_TOOLS.contains(&t.as_str())
}

/// (A1, F11-1, F10-1, F10-2) Whether ONE agy tool step may have written, decided from the step's
/// whole tool-log evidence (its ACTIVE / DONE / ERROR events merged by the reader:
/// `engines::agy::read_agy_events`). A step is NOT writing only when:
///   * it is a VERIFIED finish call (`verified_finish`: a tool named `finish` whose `step_index` the
///     stream closed with a `step_type: "finish"` event - agy's reply hand-over, whose parameters
///     ARE the reply; an index alone exempts nothing); or
///   * its name is on [`AGY_READ_ONLY_TOOLS`] (a known reader - so a pure read never fails the run)
///     and its parameters, searched at every depth, show no command or written content
///     ([`agy_params_show_write`]).
///
/// Everything else is writing (fail-safe: a writing call FAILS the run even when the monitored
/// compare is clean): a tool not on the reader allow-list - with object parameters (whatever they
/// are named; a command nested in `{"config": {...}}` or carried in a value is still a command), with
/// other parameters, or with none logged (its nature is unknown) - and an unnamed step.
pub fn agy_tool_step_is_writing(
    name: &str,
    parameters: Option<&serde_json::Value>,
    verified_finish: bool,
) -> bool {
    if verified_finish && name.trim().eq_ignore_ascii_case("finish") {
        return false;
    }
    if agy_tool_is_writing(name) {
        return true;
    }
    parameters.is_some_and(agy_params_show_write)
}

/// (F10-1) Whether a reader's logged parameters, searched at EVERY depth, show a command or written
/// content: a key whose name (lowercased, non-alphanumerics dropped) contains one of
/// [`AGY_WRITE_PARAM_MARKERS`], or a string value - outside a search key (`query`, `pattern`,
/// `regex`, `search`, `glob`, `includes`, `excludes`) - shaped like a command
/// ([`command_shaped`]). A false positive only makes a reader count as writing (fail-safe).
pub fn agy_params_show_write(v: &serde_json::Value) -> bool {
    fn fold(k: &str) -> String {
        k.chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase()
    }
    fn walk(v: &serde_json::Value, search_key: bool) -> bool {
        match v {
            serde_json::Value::Object(m) => m.iter().any(|(k, x)| {
                let f = fold(k);
                AGY_WRITE_PARAM_MARKERS.iter().any(|w| f.contains(w))
                    || walk(x, AGY_SEARCH_PARAM_MARKERS.iter().any(|s| f.contains(s)))
            }),
            serde_json::Value::Array(a) => a.iter().any(|x| walk(x, search_key)),
            serde_json::Value::String(s) => !search_key && command_shaped(s),
            _ => false,
        }
    }
    walk(v, false)
}

/// (F10-1) Whether a string value reads as a command line: its first word (a path's file name,
/// extension dropped, lowercased) is one of [`AGY_COMMAND_WORDS`] and more follows it, or it chains
/// commands (`&&`, `||`, `;` followed by a command word, a redirection `>`).
pub fn command_shaped(s: &str) -> bool {
    let t = s.trim();
    let first_word = |w: &str| -> bool {
        let base = w
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(w)
            .to_ascii_lowercase();
        let base = base.split('.').next().unwrap_or("").to_string();
        AGY_COMMAND_WORDS.contains(&base.as_str())
    };
    let mut words = t.split_whitespace();
    if let Some(w) = words.next() {
        if first_word(w) && words.next().is_some() {
            return true;
        }
    }
    t.contains("&&")
        || t.contains("||")
        || t.contains(" > ")
        || t.contains(">>")
        || t.split(';')
            .skip(1)
            .any(|part| part.split_whitespace().next().is_some_and(first_word))
}

/// (F10-1) Parameter-name fragments that mark a command or written content.
pub const AGY_WRITE_PARAM_MARKERS: &[&str] = &[
    "command",
    "cmd",
    "script",
    "shell",
    "content",
    "replacement",
    "overwrite",
    "write",
    "delete",
    "patch",
];

/// (F10-1) Parameter-name fragments of a search term, whose value is matched, never run.
const AGY_SEARCH_PARAM_MARKERS: &[&str] = &[
    "query", "pattern", "regex", "search", "glob", "include", "exclude",
];

/// (F10-1) First words of a command line that runs or writes something.
pub const AGY_COMMAND_WORDS: &[&str] = &[
    "rm",
    "del",
    "erase",
    "rmdir",
    "rd",
    "mv",
    "move",
    "cp",
    "copy",
    "xcopy",
    "robocopy",
    "mkdir",
    "md",
    "touch",
    "ren",
    "rename",
    "git",
    "npm",
    "npx",
    "pnpm",
    "yarn",
    "cargo",
    "make",
    "python",
    "python3",
    "node",
    "powershell",
    "pwsh",
    "cmd",
    "bash",
    "sh",
    "zsh",
    "sed",
    "tee",
    "chmod",
    "chown",
    "ln",
    "mklink",
    "curl",
    "wget",
    "remove-item",
    "new-item",
    "set-content",
    "add-content",
    "out-file",
    "copy-item",
    "move-item",
    "rename-item",
];

/// agy's known read-only tool names. `read_file` and `view_file` are CONFIRMED by the agy fixtures
/// (`crates/c3/src/engines/agy.rs`, `crates/c3-cli/tests/compat_wave2b.rs`). (F11-1) agy's writing
/// tools in those fixtures are the Cascade vocabulary (`write_to_file` / `TargetFile`,
/// `replace_file_content`, `run_command` / `CommandLine`), so its read-only tools of the same
/// vocabulary are listed too - a pure read with them must not fail the run - and the read-only
/// file tools of agy's Gemini-CLI lineage. Not all are fixture-confirmed. Anything not on this list
/// is treated as writing.
pub const AGY_READ_ONLY_TOOLS: &[&str] = &[
    // confirmed by the agy fixtures
    "read_file",
    "view_file",
    // agy's Cascade-vocabulary readers (not fixture-confirmed in this repo)
    "view_file_outline",
    "view_code_item",
    "view_content_chunk",
    "list_dir",
    "find_by_name",
    "grep_search",
    "codebase_search",
    "search_web",
    "read_url_content",
    // agy's Gemini-CLI read-only file tools (not fixture-confirmed in this repo)
    "read_many_files",
    "list_directory",
    "glob",
    "search_file_content",
];

/// `Get-PanelIgnorePrefixes` (D7, F04-1): the collab paths a panel member's siblings write while
/// it runs — the task's two stores (each with its `Write-TextAtomic` temp) and every sibling's
/// handoff files (with their temps). Empty when the member has no active siblings. The nns are
/// zero-padded to two digits to match the handoff filenames (`NN-<prefix>-<slug>.*`).
pub fn panel_ignore_prefixes(task: &str, sibling_nns: &[i64]) -> Vec<String> {
    let nns: Vec<i64> = sibling_nns.iter().copied().filter(|n| *n != 0).collect();
    let mut p: Vec<String> = Vec::new();
    if nns.is_empty() {
        return p;
    }
    for store in ["sessions.json", "findings.json"] {
        p.push(format!("{task}/{store}"));
        p.push(format!("{task}/.{store}."));
    }
    for s in &nns {
        p.push(format!("{task}/handoffs/{s:02}-"));
        p.push(format!("{task}/handoffs/.{s:02}-"));
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_ignore_prefixes_covers_stores_and_sibling_handoffs() {
        assert!(panel_ignore_prefixes("t", &[]).is_empty());
        let p = panel_ignore_prefixes("t", &[2, 3]);
        assert_eq!(
            p,
            vec![
                "t/sessions.json".to_string(),
                "t/.sessions.json.".to_string(),
                "t/findings.json".to_string(),
                "t/.findings.json.".to_string(),
                "t/handoffs/02-".to_string(),
                "t/handoffs/.02-".to_string(),
                "t/handoffs/03-".to_string(),
                "t/handoffs/.03-".to_string(),
            ]
        );
    }

    #[test]
    fn identical_content_is_not_a_change_but_a_moved_head_is_noted() {
        let a = RevisionInfo {
            base_commit: "aaa".into(),
            content_sha256: "h1".into(),
            ..Default::default()
        };
        let mut b = a.clone();
        b.base_commit = "bbb".into();
        // Same content, moved HEAD → not a read-only violation, only revision_moved.
        assert!(!changed(&a, &b));
        assert_eq!(compare(&a, &b).revision_moved, "aaa -> bbb");
        // Content changed → the run must fail.
        b.content_sha256 = "h2".into();
        assert!(changed(&a, &b));
        assert_eq!(compare(&a, &b).revision_moved, "");
    }

    #[test]
    fn collab_compare_detects_writes_and_honours_ignore_prefix() {
        let mut before = HashMap::new();
        before.insert("t/handoffs/01-agy-run.md".to_string(), "10|aaa".to_string());
        before.insert(
            "t/handoffs/02-codex-run.md".to_string(),
            "7|kkk".to_string(),
        );
        let mut after = before.clone();
        // The reviewer wrote a new file into the collab dir.
        after.insert("t/handoffs/99-agy-note.md".to_string(), "3|ccc".to_string());
        // This run's own handoff prefix changed (ignored).
        after.insert("t/handoffs/01-agy-run.md".to_string(), "20|zzz".to_string());
        let own = vec!["t/handoffs/01-agy-run.".to_string()];
        let changed = compare_collab(&before, &after, "t", &own);
        assert_eq!(changed, vec!["t/handoffs/99-agy-note.md".to_string()]);
        // A content change to a non-ignored handoff (a sibling not in own/panel) is caught.
        after.insert(
            "t/handoffs/02-codex-run.md".to_string(),
            "8|www".to_string(),
        );
        let changed = compare_collab(&before, &after, "t", &own);
        assert!(changed.contains(&"t/handoffs/02-codex-run.md".to_string()));
        assert_eq!(changed.len(), 2);
    }

    #[test]
    fn collab_compare_ignores_another_tasks_directory() {
        // The sibling runs / other sessions write under their OWN task dirs (TECH_DEBT 1-4);
        // changes there are never this run's and must not be counted.
        let before = HashMap::new();
        let mut after = HashMap::new();
        after.insert(
            "other-task/handoffs/02-codex.md".to_string(),
            "3|aaa".to_string(),
        );
        after.insert("other-task/findings.json".to_string(), "9|bbb".to_string());
        after.insert(
            "lb120-bt-connect/.consult.detached-x.log".to_string(),
            "9|ccc".to_string(),
        );
        assert!(compare_collab(&before, &after, "t", &[]).is_empty());
        // A change under THIS task's own handoff IS caught.
        after.insert("t/handoffs/02-codex.md".to_string(), "3|ddd".to_string());
        assert_eq!(
            compare_collab(&before, &after, "t", &[]),
            vec!["t/handoffs/02-codex.md".to_string()]
        );
    }

    #[test]
    fn collab_compare_ignores_this_tasks_ledger_state_and_control_files() {
        let before = HashMap::new();
        let mut after = HashMap::new();
        // state.md (coordinator writes it), the two stores and their atomic-write temps, and the
        // task's own control/status/log/lock files — all ignored for every engine.
        for (p, v) in [
            ("t/state.md", "6|ddd"),
            ("t/findings.json", "2|f1"),
            ("t/.findings.json.tmp-123", "2|f2"),
            ("t/sessions.json", "2|s1"),
            ("t/.sessions.json.tmp-456", "2|s2"),
            ("t/.consult.pending.json", "2|c1"),
            ("t/..consult.lock", "2|c2"),
        ] {
            after.insert(p.to_string(), v.to_string());
        }
        assert!(compare_collab(&before, &after, "t", &[]).is_empty());
    }

    #[test]
    fn agy_tool_classifier_reads_vs_writes() {
        // Every name on the exact allow-list is read-only (case/whitespace tolerant).
        for ro in AGY_READ_ONLY_TOOLS {
            assert!(!agy_tool_is_writing(ro), "{ro} should be read-only");
        }
        assert!(!agy_tool_is_writing(" View_File "));
        // A substring of an allow-list name must NOT pass as read-only (the old `contains` bug):
        // `find_and_replace` is writing even though read-only `search_file_content`/`read_file`
        // share letters with it, `edit_tools` though `tools` is a substring of no allowed name,
        // and `ls`/`find` on their own are not allowed names.
        assert!(agy_tool_is_writing("find_and_replace"));
        assert!(agy_tool_is_writing("edit_tools"));
        assert!(agy_tool_is_writing("run_shell_command"));
        assert!(agy_tool_is_writing("write_file"));
        assert!(agy_tool_is_writing("run_command"));
        assert!(agy_tool_is_writing("apply_patch"));
        assert!(agy_tool_is_writing("ls"));
        assert!(agy_tool_is_writing("find"));
        // An unknown/empty name is treated as writing.
        assert!(agy_tool_is_writing(""));
    }

    // (A1, F11-1, F10-1) the step classifier: a verified finish call and a known reader (whose
    // parameters show no command at any depth) are not writing; everything else is.
    #[test]
    fn agy_tool_step_classifier_reads_the_tool_log() {
        use serde_json::json;
        // agy's `finish` is the reply hand-over: not writing as a VERIFIED finish call - the live
        // 2026-11-07 shape. Unverified (its index was never closed by a finish event), or a
        // verified flag on a tool not named finish, exempts nothing.
        let finish = json!({"findings": [{"locations": [{"path": "a.rs", "line": 1}]}],
            "first_run_checklist": [], "prior_findings": [], "reply_markdown": "m",
            "schema_version": "1", "unproven": [], "verdict": "HOLD", "verdict_reason": "r"});
        assert!(!agy_tool_step_is_writing("finish", Some(&finish), true));
        assert!(!agy_tool_step_is_writing("finish", None, true));
        assert!(agy_tool_step_is_writing("finish", Some(&finish), false));
        assert!(agy_tool_step_is_writing(
            "write_to_file",
            Some(&json!({"TargetFile": "a.rs"})),
            true
        ));
        // a tool not on the reader allow-list with object parameters is writing, whatever they
        // are named (F10-1: `{"action": "rm -rf x"}`, `{"config": {"path": "a.rs"}}`, a fixed-target
        // writer taking only text) - and so is one with an empty object.
        for p in [
            json!({"action": "rm -rf x"}),
            json!({"config": {"path": "a.rs"}}),
            json!({"text": "new content"}),
            json!({"query": "rust tokio"}),
            json!({}),
        ] {
            assert!(agy_tool_step_is_writing("do_thing", Some(&p), false), "{p}");
        }
        // known readers stay read-only even though they name a path.
        assert!(!agy_tool_step_is_writing(
            "read_file",
            Some(&json!({"Path": "C:/r/a.rs", "StartLine": 1})),
            false
        ));
        assert!(!agy_tool_step_is_writing(
            "view_file",
            Some(&json!({"AbsolutePath": "C:/r/a.rs"})),
            false
        ));
        // writing tools name a target file, a command line or a working directory.
        assert!(agy_tool_step_is_writing(
            "write_to_file",
            Some(&json!({"TargetFile": "a.rs", "CodeContent": "x"})),
            false
        ));
        assert!(agy_tool_step_is_writing(
            "run_command",
            Some(&json!({"CommandLine": "rm -rf x", "Cwd": "."})),
            false
        ));
        assert!(agy_tool_step_is_writing(
            "replace_file_content",
            Some(&json!({"target_file": "a.rs"})),
            false
        ));
        // an unknown tool whose parameters were not logged is writing (fail-safe), as is an
        // unnamed one.
        assert!(agy_tool_step_is_writing("run_command", None, false));
        assert!(agy_tool_step_is_writing("", None, false));
        // (F10-1) a reader whose parameters carry a command at any depth, or a command-shaped
        // value outside a search key, is writing; a search term that looks like one is not.
        assert!(agy_tool_step_is_writing(
            "read_file",
            Some(&json!({"Path": "a.rs", "extra": {"CommandLine": "x"}})),
            false
        ));
        assert!(agy_tool_step_is_writing(
            "view_file",
            Some(&json!({"AbsolutePath": "a.rs", "opts": ["del a.rs"]})),
            false
        ));
        assert!(!agy_tool_step_is_writing(
            "grep_search",
            Some(&json!({"Query": "git diff && x", "SearchPath": "C:/r"})),
            false
        ));
        assert!(command_shaped("rm -rf x"));
        assert!(command_shaped("C:\\Windows\\cmd.exe /c del x"));
        assert!(command_shaped("echo x > a.txt"));
        assert!(command_shaped("ls; rm x"));
        assert!(!command_shaped("C:/r/a.rs"));
        assert!(!command_shaped("rm"));
        assert!(!command_shaped("the reply text"));
    }

    #[test]
    fn collab_snapshot_excludes_control_files() {
        let dir = std::env::temp_dir().join(format!("c3-collab-{}", std::process::id()));
        let task = dir.join("t");
        std::fs::create_dir_all(&task).unwrap();
        std::fs::write(task.join("sessions.json"), b"{}").unwrap();
        std::fs::write(task.join(".consult.lock"), b"x").unwrap();
        std::fs::write(task.join(".consult.pending.json"), b"x").unwrap();
        let snap = collab_snapshot(&dir);
        assert!(snap.contains_key("t/sessions.json"));
        assert!(!snap.keys().any(|k| k.contains(".consult.")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshot_of_a_non_repo_is_inert() {
        // A directory that is not a git repo yields an empty content hash; two such snapshots
        // fall back to the tree-fingerprint comparison and report no change.
        let dir = std::env::temp_dir();
        let before = snapshot(&dir, None);
        let after = snapshot(&dir, None);
        assert!(!changed(&before, &after));
    }
}
