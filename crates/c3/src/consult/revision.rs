//! The revision fingerprint (`Get-RevisionInfo` / `Compare-TreeContent`,
//! `codex-consult-common.ps1:543-730`).
//!
//! Two hashes are computed from the git working tree, both excluding the collab dir:
//! * `tree_sha256` — the review binding, over a manifest of the *changed* files
//!   (`git status --porcelain=v1 -uall -z`) with their status, mode and blob id;
//! * `content_sha256` — the wave-24c content-only fingerprint over the index's blob of every
//!   tracked path plus what the worktree says, so a commit or a `git add` that leaves file
//!   contents identical is NOT a change (`Compare-TreeContent`); a moved HEAD with an
//!   identical content hash is recorded as `revision_moved`, informational only.
//!
//! All git output is read as UTF-8 bytes; paths come through `-z` unquoted.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use c3_core::paths::repo_relative;

/// The fingerprint of the tree under review.
#[derive(Debug, Clone, Default)]
pub struct RevisionInfo {
    pub base_commit: String,
    pub short_sha: String,
    pub dirty: bool,
    pub reviewed_revision: String,
    pub tree_sha256: String,
    pub changed_files: i64,
    pub fingerprint_note: String,
    pub content_sha256: String,
}

/// Run a git command from `root`, returning its stdout bytes on exit 0, else `None`.
fn git_bytes(root: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(out.stdout)
}

/// The measured `-Range` (`Get-RangeStat`, `codex-consult-common.ps1:355`). `git diff
/// --shortstat <spec> --` run once from `root`; the counts feed the prompt, the ledger
/// `range{}` record and the size warning. `error` (non-empty) is a refusal produced before
/// anything is started — a range git does not know, or git that could not be started. The
/// syntactic refusals (single revision, spaces) are already enforced by `args::validate`;
/// this reproduces the plugin's git-side error wording for a range git rejects.
#[derive(Debug, Clone, Default)]
pub struct RangeStat {
    pub files: i64,
    pub insertions: i64,
    pub deletions: i64,
    pub lines: i64,
    pub error: String,
}

pub fn range_stat(root: &Path, spec: &str) -> RangeStat {
    let mut r = RangeStat::default();
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["diff", "--shortstat", spec, "--"])
        .output();
    let out = match out {
        Ok(o) => o,
        Err(_) => {
            r.error = format!(
                "-Range '{spec}': git could not be started in {}",
                root.display()
            );
            return r;
        }
    };
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr)
            .split(['\r', '\n'])
            .map(|l| l.trim())
            .find(|l| !l.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("exit {}", out.status.code().unwrap_or(-1)));
        r.error = format!(
            "-Range '{spec}' is not a revision range git knows in {} (git diff --shortstat: {why})",
            root.display()
        );
        return r;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    if let Some(c) = capture_int(&text, r"(\d+) files? changed") {
        r.files = c;
    }
    if let Some(c) = capture_int(&text, r"(\d+) insertions?\(\+\)") {
        r.insertions = c;
    }
    if let Some(c) = capture_int(&text, r"(\d+) deletions?\(-\)") {
        r.deletions = c;
    }
    r.lines = r.insertions + r.deletions;
    r
}

fn capture_int(text: &str, pat: &str) -> Option<i64> {
    regex::Regex::new(pat)
        .ok()?
        .captures(text)?
        .get(1)?
        .as_str()
        .parse()
        .ok()
}

/// The reviewer note text `the range changes N file(s), N line(s)` (`$rangeText`).
pub fn range_text(files: i64, lines: i64) -> String {
    format!(
        "the range changes {files} file{}, {lines} line{}",
        if files == 1 { "" } else { "s" },
        if lines == 1 { "" } else { "s" }
    )
}

fn git_line(root: &Path, args: &[&str]) -> String {
    git_bytes(root, args)
        .map(|b| {
            String::from_utf8_lossy(&b)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string()
        })
        .unwrap_or_default()
}

fn manifest_path(p: &str) -> String {
    p.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

struct Entry {
    xy: String,
    path: String,
    orig: Option<String>,
    blob: String,
    mode: String,
}

/// Compute the revision fingerprint (`Get-RevisionInfo`). `collab_root` is excluded from
/// both hashes when it lies under `root`.
pub fn revision_info(root: &Path, collab_root: Option<&Path>) -> RevisionInfo {
    let mut info = RevisionInfo {
        base_commit: "unknown".into(),
        short_sha: "unknown".into(),
        reviewed_revision: "unknown".into(),
        fingerprint_note: "no git".into(),
        ..Default::default()
    };
    let status = match git_bytes(
        root,
        &[
            "--no-optional-locks",
            "status",
            "--porcelain=v1",
            "-uall",
            "-z",
        ],
    ) {
        Some(b) => b,
        None => return info,
    };

    let mut notes: Vec<String> = Vec::new();
    let head = git_line(root, &["rev-parse", "--verify", "-q", "HEAD"]);
    let base = if head.is_empty() {
        notes.push("no commits yet".into());
        "none".to_string()
    } else {
        info.base_commit = head.clone();
        info.short_sha = git_line(root, &["rev-parse", "--short", "HEAD"]);
        head.clone()
    };

    // Collab-dir exclusion (repo-relative).
    let collab_rel = collab_root.and_then(|c| {
        let rel = repo_relative(root, c);
        match rel.as_deref() {
            None => {
                notes.push("collab dir outside the repository".into());
                None
            }
            Some("") => {
                notes.push("collab dir is the repository root: nothing excluded".into());
                None
            }
            Some(_) => rel,
        }
    });
    let ci = cfg!(windows);
    let excluded = |p: &str| -> bool {
        match &collab_rel {
            None => false,
            Some(c) => path_eq_or_under(p, c, ci),
        }
    };

    // Parse the NUL-separated status entries.
    let text = String::from_utf8_lossy(&status);
    let tokens: Vec<&str> = text.split('\0').collect();
    let mut entries: Vec<Entry> = Vec::new();
    let mut excluded_count = 0;
    let mut i = 0;
    while i < tokens.len() {
        let tok = tokens[i];
        i += 1;
        if tok.len() < 4 {
            continue;
        }
        let xy = &tok[0..2];
        let path = &tok[3..];
        let xyb: Vec<char> = xy.chars().collect();
        let mut orig = None;
        if (matches!(xyb[0], 'R' | 'C') || matches!(xyb[1], 'R' | 'C')) && i < tokens.len() {
            orig = Some(tokens[i].to_string());
            i += 1;
        }
        if excluded(path) {
            excluded_count += 1;
            continue;
        }
        entries.push(Entry {
            xy: xy.to_string(),
            path: path.to_string(),
            orig,
            blob: String::new(),
            mode: "u".to_string(),
        });
    }

    // Mode map (only with a HEAD to diff against).
    let mode_map = if head.is_empty() {
        notes.push("tracked file modes not recorded (no HEAD to compare with)".into());
        BTreeMap::new()
    } else {
        git_mode_map(root)
    };
    for e in entries.iter_mut() {
        if e.xy == "??" || head.is_empty() {
            continue;
        }
        e.mode = mode_map
            .get(&e.path)
            .cloned()
            .unwrap_or_else(|| "=".to_string());
    }

    // Blob ids for changed files that exist on disk.
    let mut to_hash: Vec<String> = Vec::new();
    let mut dirs = 0;
    for e in entries.iter_mut() {
        let full = root.join(&e.path);
        if full.is_dir() {
            e.blob = "dir".into();
            dirs += 1;
        } else if full.is_file() {
            to_hash.push(e.path.clone());
        } else {
            e.blob = "deleted".into();
        }
    }
    if !to_hash.is_empty() {
        let ids = git_blob_ids(root, &to_hash);
        for e in entries.iter_mut() {
            if !e.blob.is_empty() {
                continue;
            }
            e.blob = ids
                .get(&e.path)
                .cloned()
                .unwrap_or_else(|| "unreadable".into());
        }
    }
    let unreadable = entries.iter().filter(|e| e.blob == "unreadable").count();

    // Manifest: sort by (path\0line) ordinal, keep the parallel line.
    let mut keyed: Vec<(String, String)> = Vec::new();
    for e in &entries {
        let mut line = format!("{} {} {} {}", e.xy, e.mode, e.blob, manifest_path(&e.path));
        if let Some(o) = &e.orig {
            line.push('\t');
            line.push_str(&manifest_path(o));
        }
        keyed.push((format!("{}\0{}", e.path, line), line));
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    let mut manifest = format!("base {base}\n");
    for (_, line) in &keyed {
        manifest.push_str(line);
        manifest.push('\n');
    }

    if let Some(c) = &collab_rel {
        notes.push(format!(
            "collab dir '{c}' excluded ({excluded_count} entries)"
        ));
    }
    notes.push("ignored files excluded".into());
    notes.push("untracked file modes not recorded".into());
    if dirs > 0 {
        notes.push(format!(
            "submodules not recursed ({dirs} directory entries)"
        ));
    } else {
        notes.push("submodules not recursed".into());
    }
    if unreadable > 0 {
        notes.push(format!("{unreadable} files unreadable"));
    }

    // Content fingerprint (wave 24c): the index's blob of every tracked path, then the
    // worktree's own view (a changed file's working blob, a deleted file removed).
    if let Some(ls) = git_bytes(root, &["--no-optional-locks", "ls-files", "-s", "-z"]) {
        let ls_text = String::from_utf8_lossy(&ls);
        let mut content: BTreeMap<String, String> = BTreeMap::new();
        for rec in ls_text.split('\0') {
            if let Some(tab) = rec.find('\t') {
                let meta: Vec<&str> = rec[..tab].split(' ').collect();
                let p = &rec[tab + 1..];
                if meta.len() < 2 || p.is_empty() {
                    continue;
                }
                if excluded(p) {
                    continue;
                }
                content.insert(p.to_string(), meta[1].to_string());
            }
        }
        for e in &entries {
            if e.blob == "deleted" {
                content.remove(&e.path);
            } else {
                content.insert(e.path.clone(), e.blob.clone());
            }
        }
        // BTreeMap already iterates in ordinal (byte) key order.
        let mut csb = String::new();
        for (k, v) in &content {
            csb.push_str(&format!("{} {}\n", v, manifest_path(k)));
        }
        info.content_sha256 = c3_core::sha256_hex(csb.as_bytes());
    }

    info.tree_sha256 = c3_core::sha256_hex(manifest.as_bytes());
    info.changed_files = entries.len() as i64;
    info.dirty = !entries.is_empty();
    info.reviewed_revision = if info.dirty {
        format!("{} + uncommitted", info.short_sha)
    } else {
        info.short_sha.clone()
    };
    info.fingerprint_note = notes.join("; ");
    info
}

/// The result of comparing two fingerprints by content (`Compare-TreeContent`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TreeComparison {
    /// A file's content appeared, disappeared or changed during the run.
    pub changed: bool,
    /// `"<old> -> <new>"` when HEAD moved but no content changed; empty otherwise.
    pub revision_moved: String,
}

/// Compare two fingerprints by content: HEAD moving or a `git add` with identical file
/// contents is `revision_moved`, not a `changed`.
pub fn compare_tree_content(before: &RevisionInfo, after: &RevisionInfo) -> TreeComparison {
    let mut r = TreeComparison::default();
    if !before.content_sha256.is_empty() && !after.content_sha256.is_empty() {
        r.changed = before.content_sha256 != after.content_sha256;
    } else {
        // Fallback: the tree fingerprints decide (as before wave 24c).
        r.changed = before.tree_sha256 != after.tree_sha256;
    }
    if !r.changed && before.base_commit != after.base_commit {
        r.revision_moved = format!("{} -> {}", before.base_commit, after.base_commit);
    }
    r
}

/// The header/summary `Note:` line for a moved HEAD (`codex-consult.ps1:3732`).
pub fn revision_moved_note(revision_moved: &str, tree_changed: bool) -> String {
    let shortened: Vec<String> = revision_moved
        .split(" -> ")
        .map(|s| {
            if s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit()) {
                s[..7].to_string()
            } else {
                s.to_string()
            }
        })
        .collect();
    let tail = if !tree_changed {
        " - no file content changed: not a tree change"
    } else {
        ""
    };
    format!(
        "Note: HEAD moved during the review ({}){}.",
        shortened.join(" -> "),
        tail
    )
}

fn git_mode_map(root: &Path) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let out = match git_bytes(
        root,
        &[
            "--no-optional-locks",
            "diff",
            "--raw",
            "-z",
            "--no-renames",
            "--no-ext-diff",
            "--no-abbrev",
            "HEAD",
        ],
    ) {
        Some(b) => b,
        None => return map,
    };
    let text = String::from_utf8_lossy(&out);
    let tokens: Vec<&str> = text.split('\0').collect();
    let mut i = 0;
    while i + 1 < tokens.len() {
        let meta = tokens[i];
        if !meta.starts_with(':') {
            i += 1;
            continue;
        }
        let path = tokens[i + 1];
        i += 2;
        let parts: Vec<&str> = meta[1..].split(' ').collect();
        if parts.len() < 2 {
            continue;
        }
        let mode = if parts[0] != parts[1] {
            format!("{}>{}", parts[0], parts[1])
        } else {
            parts[0].to_string()
        };
        map.insert(path.to_string(), mode);
    }
    map
}

fn git_blob_ids(root: &Path, paths: &[String]) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    // One `git hash-object -- <paths>`; map by line order when the counts match.
    let mut args: Vec<&str> = vec!["hash-object", "--"];
    for p in paths {
        args.push(p.as_str());
    }
    if let Some(b) = git_bytes(root, &args) {
        let lines: Vec<String> = String::from_utf8_lossy(&b)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        if lines.len() == paths.len() {
            for (p, id) in paths.iter().zip(lines) {
                result.insert(p.clone(), id);
            }
            return result;
        }
    }
    // Fallback: per-file.
    for p in paths {
        let id = git_bytes(root, &["hash-object", "--", p])
            .map(|b| String::from_utf8_lossy(&b).trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unreadable".into());
        result.insert(p.clone(), id);
    }
    result
}

fn path_eq_or_under(p: &str, base: &str, ci: bool) -> bool {
    let under = format!("{base}/");
    if ci {
        p.eq_ignore_ascii_case(base) || p.to_lowercase().starts_with(&under.to_lowercase())
    } else {
        p == base || p.starts_with(&under)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_path_escapes() {
        assert_eq!(manifest_path("a\\b\tc"), "a\\\\b\\tc");
    }

    #[test]
    fn revision_moved_note_shortens_shas() {
        let note = revision_moved_note(
            "1234567890abcdef1234567890abcdef12345678 -> abcdef1234567890abcdef1234567890abcdef12",
            false,
        );
        assert_eq!(
            note,
            "Note: HEAD moved during the review (1234567 -> abcdef1) - no file content changed: not a tree change."
        );
    }

    #[test]
    fn range_text_pluralization() {
        assert_eq!(range_text(1, 1), "the range changes 1 file, 1 line");
        assert_eq!(range_text(3, 42), "the range changes 3 files, 42 lines");
        assert_eq!(range_text(0, 0), "the range changes 0 files, 0 lines");
    }

    #[test]
    fn range_capture_parses_shortstat() {
        assert_eq!(
            capture_int(
                " 3 files changed, 12 insertions(+), 4 deletions(-)",
                r"(\d+) files? changed"
            ),
            Some(3)
        );
        assert_eq!(
            capture_int(
                " 1 file changed, 1 insertion(+)",
                r"(\d+) insertions?\(\+\)"
            ),
            Some(1)
        );
        assert_eq!(
            capture_int(" 1 file changed, 1 insertion(+)", r"(\d+) deletions?\(-\)"),
            None
        );
    }

    #[test]
    fn range_stat_unknown_range_errors() {
        // A repo path with no such revision range yields the git-side refusal wording.
        let dir = std::env::temp_dir();
        let rs = range_stat(&dir, "deadbeef..cafebabe");
        assert!(!rs.error.is_empty());
        assert!(rs.error.contains("deadbeef..cafebabe"));
    }

    #[test]
    fn compare_detects_move_vs_change() {
        let a = RevisionInfo {
            base_commit: "aaa".into(),
            content_sha256: "hash1".into(),
            ..Default::default()
        };
        let mut b = a.clone();
        b.base_commit = "bbb".into();
        // same content, moved HEAD → revision_moved, not changed.
        let c = compare_tree_content(&a, &b);
        assert!(!c.changed);
        assert_eq!(c.revision_moved, "aaa -> bbb");
        // content changed → changed, no revision_moved.
        b.content_sha256 = "hash2".into();
        let c2 = compare_tree_content(&a, &b);
        assert!(c2.changed);
        assert_eq!(c2.revision_moved, "");
    }
}
