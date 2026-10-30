//! `c3 finish` / `c3 fail` / `c3 manifest` — the coordinator-only commit-and-hand-off path
//! that replaces eck's MCP server (port spec, Part A).
//!
//! The rule "C3 never commits" becomes "C3 commits only via `c3 finish`": [`run_finish`] is
//! the one code path in C3 that runs a git write. It writes the agent report
//! (`.eck/lastsnapshot/AnswerToSA.md`), prepends a journal entry, stages everything with
//! `git add -A`, unstages the machine-local never-commit set ([`never_commit`]), runs the
//! secret gate over the staged content ([`secret_gate`]), commits, and writes a delta
//! snapshot into the architect's hand-off folder. [`run_fail`] does the same minus the
//! staging and commit, as an emergency snapshot. [`run_manifest`] edits a `.eck/*.md`
//! manifest with no git and no snapshot.

pub mod git;
pub mod journal;
pub mod manifest;
pub mod never_commit;
pub mod secret_gate;

use std::path::{Path, PathBuf};

use crate::pack::snapshot::{self, SnapshotOpts};
use secret_gate::SecretHit;

/// Options for [`run_finish`].
#[derive(Debug, Clone)]
pub struct FinishOpts {
    pub repo_root: PathBuf,
    pub collab_root: PathBuf,
    pub message: String,
    pub status: String,
    pub keep: usize,
    /// Take the hand-off snapshot (false = `--no-snapshot`).
    pub snapshot: bool,
    /// Commit even when nothing is staged (`--allow-empty`).
    pub allow_empty: bool,
    /// Repo-relative paths the secret gate lets through (`--allow-secret`).
    pub allow_secret: Vec<String>,
}

/// Options for [`run_fail`].
#[derive(Debug, Clone)]
pub struct FailOpts {
    pub repo_root: PathBuf,
    pub collab_root: PathBuf,
    pub status: String,
    pub keep: usize,
    pub snapshot: bool,
}

/// The outcome of a successful [`run_finish`].
#[derive(Debug, Clone)]
pub struct FinishResult {
    pub commit: String,
    /// Staged entries as `A/M/D path` lines.
    pub staged: Vec<String>,
    /// The never-commit paths that were unstaged.
    pub unstaged: Vec<String>,
    pub snapshot_path: Option<PathBuf>,
    pub snapshot_note: Option<String>,
    pub pruned: usize,
}

/// The outcome of a successful [`run_fail`].
#[derive(Debug, Clone)]
pub struct FailResult {
    pub snapshot_path: Option<PathBuf>,
    pub snapshot_note: Option<String>,
    pub pruned: usize,
}

/// Why `c3 finish` refused or failed.
#[derive(Debug)]
pub enum FinishError {
    /// Nothing was staged and `--allow-empty` was not given.
    NothingToCommit,
    /// Staged files carry secret-shaped values; nothing was unstaged.
    Secrets(Vec<SecretHit>),
    /// A git or I/O failure.
    Other(String),
}

impl std::fmt::Display for FinishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FinishError::NothingToCommit => {
                write!(f, "nothing to commit (use --allow-empty to commit anyway)")
            }
            FinishError::Secrets(hits) => {
                writeln!(
                    f,
                    "refusing to commit: {} staged file(s) carry a secret-shaped value (nothing was unstaged so you can inspect them):",
                    hits.len()
                )?;
                for h in hits {
                    writeln!(f, "  {}", h.line())?;
                }
                write!(
                    f,
                    "pass --allow-secret <path> for each file you have reviewed"
                )
            }
            FinishError::Other(e) => write!(f, "{e}"),
        }
    }
}

fn now_stamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()
}

fn eck_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".eck")
}

/// Write `.eck/lastsnapshot/AnswerToSA.md`.
fn write_answer(repo_root: &Path, body: &str) -> Result<(), String> {
    let dir = eck_dir(repo_root).join("lastsnapshot");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    std::fs::write(dir.join("AnswerToSA.md"), body).map_err(|e| format!("write AnswerToSA.md: {e}"))
}

/// Prepend a journal entry to `.eck/JOURNAL.md` (create it when missing).
fn write_journal(repo_root: &Path, title: &str, body: &str) -> Result<(), String> {
    let dir = eck_dir(repo_root);
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let path = dir.join("JOURNAL.md");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let updated = journal::insert_journal_entry(&existing, &now_stamp(), title, body);
    std::fs::write(&path, updated).map_err(|e| format!("write JOURNAL.md: {e}"))
}

/// The hand-off snapshot shared by finish (delta/full) and fail (`--fail`). Returns the
/// written path and the prune count, or a human note when nothing was written.
fn hand_off_snapshot(
    repo_root: &Path,
    collab_root: &Path,
    keep: usize,
    report: &str,
    fail: bool,
) -> (Option<PathBuf>, Option<String>, usize) {
    let has_anchor = snapshot::anchor_exists(collab_root);
    // finish: a delta when an anchor exists, else a full snapshot (which establishes it).
    // fail: a `--fail` working-tree delta when an anchor exists, else a FULL emergency
    //       snapshot (no baseline to diff against — and an emergency is exactly when the
    //       full capture is wanted).
    let opts = SnapshotOpts {
        repo_root: repo_root.to_path_buf(),
        collab_root: collab_root.to_path_buf(),
        out: None,
        delta: !fail && has_anchor,
        depth: 9,
        budget: 0,
        focus: vec![],
        tree: true,
        max_file_size: 2 * 1024 * 1024,
        // A `--fail` snapshot needs an anchor; without one fall back to a full snapshot.
        fail: fail && has_anchor,
        eck: true,
        keep,
        embed_report: Some(report.to_string()),
    };
    match snapshot::run(&opts) {
        Ok(r) if r.wrote => (Some(r.path), None, r.pruned),
        Ok(_) => (None, Some("no snapshot: nothing changed".to_string()), 0),
        Err(e) => (None, Some(format!("no snapshot: {e}")), 0),
    }
}

/// `c3 finish`: write the report and journal, stage, unstage the never-commit set, gate on
/// secrets, commit, and write the hand-off snapshot.
pub fn run_finish(opts: &FinishOpts) -> Result<FinishResult, FinishError> {
    let repo = &opts.repo_root;

    // 1-2. Report + journal (before any git write, so they are part of the commit).
    write_answer(repo, &journal::answer_finish(&opts.status)).map_err(FinishError::Other)?;
    let title = journal::first_line(&opts.message, "Agent Report").to_string();
    write_journal(repo, &title, &opts.status).map_err(FinishError::Other)?;

    // 3. Stage everything (honours .gitignore).
    git::add_all(repo).map_err(FinishError::Other)?;

    // 4. Unstage the never-commit set.
    let status_z = git::status_porcelain_z(repo).map_err(FinishError::Other)?;
    let to_unstage = never_commit::paths_to_unstage(&status_z);
    git::unstage(repo, &to_unstage).map_err(FinishError::Other)?;

    // 5. Secret gate over the STAGED content. On a hit, nothing is unstaged.
    let hits = secret_gate::scan_staged(repo, &opts.allow_secret);
    if !hits.is_empty() {
        return Err(FinishError::Secrets(hits));
    }

    // 6. The staged list; refuse an empty commit unless --allow-empty.
    let staged_entries = git::staged_name_status(repo).map_err(FinishError::Other)?;
    if staged_entries.is_empty() && !opts.allow_empty {
        return Err(FinishError::NothingToCommit);
    }
    let staged: Vec<String> = staged_entries
        .iter()
        .map(|(c, p)| format!("{c} {p}"))
        .collect();

    // 7. Commit (the message verbatim; the coordinator adds its trailers).
    git::commit(repo, &opts.message, opts.allow_empty).map_err(FinishError::Other)?;
    let commit = git::head(repo).unwrap_or_default();

    // 8. The hand-off snapshot.
    let (snapshot_path, snapshot_note, pruned) = if opts.snapshot {
        hand_off_snapshot(repo, &opts.collab_root, opts.keep, &opts.status, false)
    } else {
        (None, Some("no snapshot: --no-snapshot".to_string()), 0)
    };

    Ok(FinishResult {
        commit,
        staged,
        unstaged: to_unstage,
        snapshot_path,
        snapshot_note,
        pruned,
    })
}

/// `c3 fail`: write the BLOCKED/FAILED report and journal and an emergency `--fail`
/// snapshot. No staging, no commit.
pub fn run_fail(opts: &FailOpts) -> Result<FailResult, FinishError> {
    let repo = &opts.repo_root;
    write_answer(repo, &journal::answer_fail(&opts.status)).map_err(FinishError::Other)?;
    write_journal(repo, "BLOCKED/FAILED", &opts.status).map_err(FinishError::Other)?;

    let (snapshot_path, snapshot_note, pruned) = if opts.snapshot {
        hand_off_snapshot(repo, &opts.collab_root, opts.keep, &opts.status, true)
    } else {
        (None, Some("no snapshot: --no-snapshot".to_string()), 0)
    };

    Ok(FailResult {
        snapshot_path,
        snapshot_note,
        pruned,
    })
}

/// `c3 manifest`: apply one action to a `.eck/<name>.md` manifest. No git, no snapshot.
pub fn run_manifest(
    repo_root: &Path,
    name: &str,
    action: manifest::Action,
    content: &str,
    section: Option<&str>,
    target: Option<&str>,
) -> Result<PathBuf, String> {
    if !manifest::manifest_name_ok(name) {
        return Err(format!(
            "manifest name '{name}' is refused: it must be a plain NAME.md directly under .eck/ (no path separator, no '..')"
        ));
    }
    let path = manifest::manifest_path(repo_root, name);
    let current = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => manifest::initial_content(name),
        Err(e) => return Err(format!("read {}: {e}", path.display())),
    };
    let updated = manifest::apply(&current, action, content, section, target)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    std::fs::write(&path, updated).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(path)
}
