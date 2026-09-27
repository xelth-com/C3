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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn snapshot_of_a_non_repo_is_inert() {
        // A directory that is not a git repo yields an empty content hash; two such snapshots
        // fall back to the tree-fingerprint comparison and report no change.
        let dir = std::env::temp_dir();
        let before = snapshot(&dir, None);
        let after = snapshot(&dir, None);
        assert!(!changed(&before, &after));
    }
}
