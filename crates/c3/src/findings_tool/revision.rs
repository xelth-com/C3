//! The working-tree revision fingerprint, ported from `Get-RevisionInfo`
//! (`codex-consult-common.ps1`). A status change records the moment's `base_commit` and
//! `tree_sha256` in the finding's history. `tree_sha256` is the SHA-256 of a deterministic
//! manifest:
//!
//! ```text
//! base <full sha, or 'none' before the first commit>
//! <XY> <mode> <blob id | deleted | dir> <path>[<TAB><rename source>]
//! ```
//!
//! one line per `git status --porcelain=v1 -uall -z` entry (entries under the collaboration
//! directory excluded, so the consultation's own output never moves the fingerprint),
//! sorted by path (ordinal), LF-joined with a trailing LF. No git repository -> `base_commit
//! = "unknown"` and `tree_sha256 = ""`, exactly as the plugin's `fingerprint_note = "no git"`
//! path.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use c3_core::paths::repo_relative;
use c3_core::sha256_hex;

/// The subset of `Get-RevisionInfo` a findings status change records.
#[derive(Debug, Clone, Default)]
pub struct RevisionInfo {
    pub base_commit: String,
    pub tree_sha256: String,
}

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

fn git_line(root: &Path, args: &[&str]) -> Option<String> {
    let bytes = git_bytes(root, args)?;
    let s = String::from_utf8_lossy(&bytes);
    s.lines().next().map(|l| l.trim().to_string())
}

/// `ConvertTo-ManifestPath`: escape backslash, tab, CR and LF (backslash first).
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
    mode: String,
    blob: String,
}

/// `Get-RevisionInfo -Root <repo> -CollabRoot <collab>`. Returns `base_commit = "unknown"`
/// and `tree_sha256 = ""` when the tree is not a git repository (or git cannot be run).
pub fn revision_info(root: &Path, collab_root: &Path) -> RevisionInfo {
    let status = git_bytes(
        root,
        &[
            "--no-optional-locks",
            "status",
            "--porcelain=v1",
            "-uall",
            "-z",
        ],
    );
    let status = match status {
        Some(s) => s,
        None => {
            return RevisionInfo {
                base_commit: "unknown".to_string(),
                tree_sha256: String::new(),
            }
        }
    };

    let head = git_line(root, &["rev-parse", "--verify", "-q", "HEAD"]);
    let base = head.clone().unwrap_or_else(|| "none".to_string());

    // The collaboration directory, repo-relative, so its entries are excluded.
    let collab_rel: Option<String> = match repo_relative(root, collab_root) {
        Some(s) if s.is_empty() => None, // collab dir is the repo root: nothing excluded
        other => other,
    };
    let ci = cfg!(windows);
    let under_collab = |path: &str| -> bool {
        let rel = match &collab_rel {
            Some(r) => r,
            None => return false,
        };
        let (p, r) = if ci {
            (path.to_lowercase(), rel.to_lowercase())
        } else {
            (path.to_string(), rel.clone())
        };
        p == r || p.starts_with(&format!("{r}/"))
    };

    // Parse the NUL-separated entries.
    let text = String::from_utf8_lossy(&status);
    let tokens: Vec<&str> = text.split('\0').collect();
    let mut entries: Vec<Entry> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let tok = tokens[i];
        i += 1;
        if tok.len() < 4 {
            continue;
        }
        let xy = &tok[0..2];
        let path = &tok[3..];
        let bytes = xy.as_bytes();
        let mut orig = None;
        if (bytes[0] == b'R' || bytes[0] == b'C' || bytes[1] == b'R' || bytes[1] == b'C')
            && i < tokens.len()
        {
            orig = Some(tokens[i].to_string());
            i += 1;
        }
        if under_collab(path) {
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

    // Modes for tracked entries (git diff --raw HEAD); '=' when the worktree matches HEAD.
    let mode_map = if head.is_some() {
        git_mode_map(root)
    } else {
        BTreeMap::new()
    };
    for e in &mut entries {
        if e.xy == "??" || head.is_none() {
            continue;
        }
        e.mode = mode_map
            .get(&e.path)
            .cloned()
            .unwrap_or_else(|| "=".to_string());
    }

    // Blob ids for regular files that exist; 'dir'/'deleted' otherwise.
    let mut to_hash: Vec<String> = Vec::new();
    for e in &mut entries {
        let full = root.join(&e.path);
        if full.is_dir() {
            e.blob = "dir".to_string();
        } else if full.is_file() {
            to_hash.push(e.path.clone());
        } else {
            e.blob = "deleted".to_string();
        }
    }
    if !to_hash.is_empty() {
        let ids = git_blob_ids(root, &to_hash);
        for e in &mut entries {
            if !e.blob.is_empty() {
                continue;
            }
            e.blob = ids
                .get(&e.path)
                .cloned()
                .unwrap_or_else(|| "unreadable".to_string());
        }
    }

    // Manifest lines, sorted by path (ordinal), then the SHA-256.
    let mut keyed: Vec<(String, String)> = Vec::new();
    for e in &entries {
        let mut line = format!("{} {} {} {}", e.xy, e.mode, e.blob, manifest_path(&e.path));
        if let Some(orig) = &e.orig {
            line.push('\t');
            line.push_str(&manifest_path(orig));
        }
        let key = format!("{}\0{}", e.path, line);
        keyed.push((key, line));
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0));

    let mut manifest = format!("base {base}\n");
    for (_, line) in &keyed {
        manifest.push_str(line);
        manifest.push('\n');
    }
    RevisionInfo {
        base_commit: head.unwrap_or_else(|| "none".to_string()),
        tree_sha256: sha256_hex(manifest.as_bytes()),
    }
}

fn git_mode_map(root: &Path) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let bytes = match git_bytes(
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
    let text = String::from_utf8_lossy(&bytes);
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
    // Batch to respect the command-line limit, as Get-GitBlobIds does (~24000 chars).
    let mut batches: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut chars = 0usize;
    for p in paths {
        if !current.is_empty() && chars + p.len() + 3 > 24000 {
            batches.push(std::mem::take(&mut current));
            chars = 0;
        }
        current.push(p.clone());
        chars += p.len() + 3;
    }
    if !current.is_empty() {
        batches.push(current);
    }
    for batch in &batches {
        let mut args: Vec<&str> = vec!["hash-object", "--"];
        for p in batch {
            args.push(p);
        }
        let ids: Option<Vec<String>> = git_bytes(root, &args).and_then(|b| {
            let s = String::from_utf8_lossy(&b);
            let lines: Vec<String> = s
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            if lines.len() == batch.len() {
                Some(lines)
            } else {
                None
            }
        });
        match ids {
            Some(ids) => {
                for (p, id) in batch.iter().zip(ids) {
                    result.insert(p.clone(), id);
                }
            }
            None => {
                for p in batch {
                    let one = git_bytes(root, &["hash-object", "--", p])
                        .map(|b| String::from_utf8_lossy(&b).trim().to_string())
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "unreadable".to_string());
                    result.insert(p.clone(), one);
                }
            }
        }
    }
    result
}
