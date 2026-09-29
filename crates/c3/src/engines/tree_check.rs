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
/// appeared/disappeared) between `before` and `after`, ignoring any path that starts with one of
/// `ignore_prefixes` (case-insensitive). Ordinal-sorted.
pub fn compare_collab(
    before: &HashMap<String, String>,
    after: &HashMap<String, String>,
    ignore_prefixes: &[String],
) -> Vec<String> {
    let mut keys: std::collections::BTreeSet<&String> = std::collections::BTreeSet::new();
    keys.extend(before.keys());
    keys.extend(after.keys());
    let mut changed: Vec<String> = Vec::new();
    for k in keys {
        if ignore_prefixes
            .iter()
            .any(|p| k.to_ascii_lowercase().starts_with(&p.to_ascii_lowercase()))
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
        before.insert("t/state.md".to_string(), "5|bbb".to_string());
        let mut after = before.clone();
        // The reviewer wrote a new file into the collab dir.
        after.insert("t/handoffs/99-agy-note.md".to_string(), "3|ccc".to_string());
        // This run's own handoff prefix changed (ignored).
        after.insert("t/handoffs/01-agy-run.md".to_string(), "20|zzz".to_string());
        let own = vec!["t/handoffs/01-agy-run.".to_string()];
        let changed = compare_collab(&before, &after, &own);
        assert_eq!(changed, vec!["t/handoffs/99-agy-note.md".to_string()]);
        // A content change to a non-ignored file is caught.
        after.insert("t/state.md".to_string(), "6|ddd".to_string());
        let changed = compare_collab(&before, &after, &own);
        assert!(changed.contains(&"t/state.md".to_string()));
        assert_eq!(changed.len(), 2);
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
