//! `c3 finish` / `c3 fail` / `c3 manifest` — the coordinator-only commit-and-hand-off path
//! that replaces eck's MCP server (port spec, Part A).
//!
//! The rule "C3 never commits" becomes "C3 commits only via `c3 finish`": [`run_finish`] is
//! the only code path in C3 that writes a git ref/commit (`c3 work` also writes loose objects
//! with `git hash-object -w` for its `patch.diff` baseline, but no ref or commit). Each of the
//! three first makes sure C3's project folder `.c3/` is in place ([`crate::state_dir::ensure`]:
//! an existing `.eck/` is moved once). [`run_finish`] writes the agent report
//! (`.c3/last-report.md`), prepends a journal entry, stages everything with
//! `git add -A`, unstages the machine-local never-commit set ([`never_commit`]), runs the
//! secret gate over the staged content ([`secret_gate`]), commits, and - only when asked
//! (`--snapshot`, 0.2.5) - writes a delta snapshot into the hand-off folder. (0.2.5) Before any of
//! that it runs the repository's own pre-finish checks ([`checks`]: `"finish_checks"` in
//! `.c3/config.json`) and refuses when one fails; `--skip-checks` goes on without them and says
//! so. [`run_fail`] does the same minus the checks, the staging and the commit (its snapshot,
//! when asked, is an emergency one). [`run_manifest`] edits a `.c3/*.md` manifest with no git and
//! no snapshot.

pub mod checks;
pub mod git;
pub mod journal;
pub mod manifest;
pub mod never_commit;
pub mod secret_gate;

use std::path::{Path, PathBuf};

use crate::pack::snapshot::{self, SnapshotOpts};
use crate::state_dir;
use secret_gate::SecretHit;

/// Options for [`run_finish`].
#[derive(Debug, Clone)]
pub struct FinishOpts {
    pub repo_root: PathBuf,
    pub collab_root: PathBuf,
    pub message: String,
    pub status: String,
    pub keep: usize,
    /// Take the hand-off snapshot (`--snapshot`; off by default since 0.2.5).
    pub snapshot: bool,
    /// Commit even when nothing is staged (`--allow-empty`).
    pub allow_empty: bool,
    /// Repo-relative paths the secret gate lets through (`--allow-secret`).
    pub allow_secret: Vec<String>,
    /// Go on without the pre-finish checks (`--skip-checks`); said in the output and the journal.
    pub skip_checks: bool,
}

/// What became of the pre-finish checks of a successful [`run_finish`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChecksOutcome {
    /// The checks that ran and passed, in order.
    pub passed: Vec<String>,
    /// `--skip-checks` was given: the configured checks (listed here) did not run.
    pub skipped: Vec<String>,
    /// `--skip-checks` was given (whether or not any check is configured).
    pub skip_requested: bool,
    /// (F46-6, F45-6) `--skip-checks` was given and the configuration could not be read or parsed
    /// (why): whether any check is configured is unknown, and none ran.
    pub config_error: Option<String>,
}

impl ChecksOutcome {
    /// The one line `c3 finish` prints (and the journal carries when the checks were skipped);
    /// `None` when no check is configured and none was skipped.
    pub fn line(&self) -> Option<String> {
        if self.skip_requested {
            if let Some(e) = &self.config_error {
                return Some(format!(
                    "Pre-finish checks skipped (--skip-checks): the configuration could not be read ({e}); no check was run."
                ));
            }
            return Some(if self.skipped.is_empty() {
                "Pre-finish checks skipped (--skip-checks; none configured).".to_string()
            } else {
                format!(
                    "Pre-finish checks skipped (--skip-checks): {}.",
                    self.skipped.join("; ")
                )
            });
        }
        (!self.passed.is_empty())
            .then(|| format!("Pre-finish checks passed: {}.", self.passed.join("; ")))
    }
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
    /// What was done to bring the project folder into place ([`crate::state_dir::Ensured::notes`]).
    pub folder_notes: Vec<String>,
    /// The pre-finish checks.
    pub checks: ChecksOutcome,
}

/// The outcome of a successful [`run_fail`].
#[derive(Debug, Clone)]
pub struct FailResult {
    pub snapshot_path: Option<PathBuf>,
    pub snapshot_note: Option<String>,
    pub pruned: usize,
    /// What was done to bring the project folder into place ([`crate::state_dir::Ensured::notes`]).
    pub folder_notes: Vec<String>,
}

/// Why `c3 finish` refused or failed.
#[derive(Debug)]
pub enum FinishError {
    /// Nothing was staged and `--allow-empty` was not given.
    NothingToCommit,
    /// Staged files carry secret-shaped values; nothing was unstaged.
    Secrets(Vec<SecretHit>),
    /// The commit MESSAGE carries a secret-shaped value (the kinds). `--allow-secret` cannot
    /// exempt it — the coordinator must rewrite the message.
    MessageSecret(String),
    /// (0.2.5) A pre-finish check failed; nothing was written, staged or committed.
    CheckFailed(checks::CheckFailure),
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
                // The unreadable-staged-LIST pseudo-hit is not a real file, so --allow-secret can
                // never exempt it: the advice for that line is repair-only (GLM, review-2026-10-30).
                let staged_list_unreadable = hits
                    .iter()
                    .any(|h| h.path == secret_gate::STAGED_LIST_PSEUDO_PATH);
                writeln!(
                    f,
                    "refusing to commit: {} staged file(s) carry a secret-shaped value (nothing was unstaged so you can inspect them):",
                    hits.len()
                )?;
                for h in hits {
                    writeln!(f, "  {}", h.line())?;
                }
                if staged_list_unreadable {
                    write!(
                        f,
                        "git could not list the staged files - repair the repository (git status must work) and run c3 finish again; --allow-secret does not apply"
                    )
                } else {
                    write!(
                        f,
                        "pass --allow-secret <path> for each file you have reviewed"
                    )
                }
            }
            FinishError::MessageSecret(kinds) => write!(
                f,
                "refusing to commit: the commit message carries a secret-shaped value ({kinds}); --allow-secret cannot exempt the message - rewrite the message and run finish again"
            ),
            FinishError::CheckFailed(c) => write!(f, "{c}"),
            FinishError::Other(e) => write!(f, "{e}"),
        }
    }
}

fn now_stamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()
}

/// Write the report to `<state dir>/last-report.md` (`.c3/`, or the legacy folder for a run whose
/// move had to wait). (0.2.5, S2 F36-1/F36-3) Through [`state_dir::write_in`]: the folder as it is
/// now (never a stale legacy path, never a new `.eck/`), never through a link.
fn write_answer(repo: &Path, dir: &Path, body: &str) -> Result<(), String> {
    state_dir::write_in(repo, dir, state_dir::LAST_REPORT, |_, _| {
        Ok(body.as_bytes().to_vec())
    })
    .map(|_| ())
}

/// Prepend a journal entry to `<state dir>/JOURNAL.md` (create it when missing), read and written
/// at the folder's place now ([`state_dir::write_in`]).
fn write_journal(repo: &Path, dir: &Path, title: &str, body: &str) -> Result<(), String> {
    let stamp = now_stamp();
    state_dir::write_in(repo, dir, "JOURNAL.md", |_, current| {
        let existing = current.ok().flatten().unwrap_or_default();
        Ok(journal::insert_journal_entry(&existing, &stamp, title, body).into_bytes())
    })
    .map(|_| ())
}

/// The hand-off snapshot shared by finish (delta/full) and fail (`--fail`). Returns the
/// written path and the prune count, or a human note when nothing was written.
fn hand_off_snapshot(
    repo_root: &Path,
    collab_root: &Path,
    state: &Path,
    keep: usize,
    report: &str,
    fail: bool,
) -> (Option<PathBuf>, Option<String>, usize) {
    let state = state.to_path_buf();
    let has_anchor = snapshot::anchor_exists(&state);
    // finish: a delta when an anchor exists, else a full snapshot (which establishes it).
    // fail: a `--fail` working-tree delta when an anchor exists, else a FULL emergency
    //       snapshot (no baseline to diff against — and an emergency is exactly when the
    //       full capture is wanted).
    let opts = SnapshotOpts {
        repo_root: repo_root.to_path_buf(),
        collab_root: collab_root.to_path_buf(),
        state_dir: state,
        out: None,
        delta: !fail && has_anchor,
        depth: 9,
        budget: 0,
        focus: vec![],
        tree: true,
        max_file_size: 2 * 1024 * 1024,
        // A `--fail` snapshot needs an anchor; without one fall back to a full snapshot.
        fail: fail && has_anchor,
        state: true,
        keep,
        embed_report: Some(report.to_string()),
        base: None,
        profile: vec![],
    };
    match snapshot::run(&opts) {
        Ok(r) if r.wrote => (Some(r.path), None, r.pruned),
        Ok(_) => (None, Some("no snapshot: nothing changed".to_string()), 0),
        Err(e) => (None, Some(format!("no snapshot: {e}")), 0),
    }
}

/// `c3 finish`: make sure the project folder is in place, write the report and journal, stage,
/// unstage the never-commit set, gate on secrets, commit, and - when asked - write the hand-off
/// snapshot.
pub fn run_finish(opts: &FinishOpts) -> Result<FinishResult, FinishError> {
    let repo = &opts.repo_root;

    // 0. The commit message is itself scanned for secrets, before any side effect; a hit refuses
    //    (the kinds, never the value) and cannot be waved through with --allow-secret - the
    //    coordinator rewrites the message (Muse, review-2026-10-30).
    if let Some(kinds) = secret_gate::scan_message(&opts.message) {
        return Err(FinishError::MessageSecret(kinds));
    }

    // 0a. (0.2.5) The repository's own pre-finish checks, before anything is written or staged: a
    //     failing one refuses with nothing changed. `--skip-checks` goes on and says so.
    let configured = checks::configured(repo);
    let check_outcome = if opts.skip_checks {
        let (skipped, config_error) = match configured {
            Ok(list) => (list, None),
            Err(e) => (Vec::new(), Some(e)),
        };
        ChecksOutcome {
            skipped,
            skip_requested: true,
            config_error,
            ..Default::default()
        }
    } else {
        let list = configured.map_err(|e| {
            FinishError::Other(format!(
                "refusing to finish: the pre-finish checks cannot be read - {e}; fix the file or pass --skip-checks"
            ))
        })?;
        checks::run_all(&list, repo).map_err(FinishError::CheckFailed)?;
        ChecksOutcome {
            passed: list,
            ..Default::default()
        }
    };

    // 0b. The project folder (an existing `.eck/` moved once, stubs, the `.gitignore` line - before
    //     the staging, so a `.gitignore` change is part of the commit).
    let ensured = state_dir::ensure(repo, Some(&opts.collab_root)).map_err(FinishError::Other)?;

    // 1-2. Report + journal (before any git write, so they are part of the commit). Skipped checks
    //      are said in the journal entry.
    let state = ensured.dir.clone();
    write_answer(repo, &state, &journal::answer_finish(&opts.status))
        .map_err(FinishError::Other)?;
    let title = journal::first_line(&opts.message, "Agent Report").to_string();
    let journal_body = match check_outcome.line() {
        Some(l) if check_outcome.skip_requested => {
            format!("{}\n\n_{l}_", opts.status.trim_end())
        }
        _ => opts.status.clone(),
    };
    write_journal(repo, &state, &title, &journal_body).map_err(FinishError::Other)?;
    // (S2, F36-1) the folder as it is now - a move made meanwhile is followed, never undone
    let state = state_dir::moved_with_folder(repo, &state);
    let collab = state_dir::moved_with_folder(repo, &opts.collab_root);

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

    // 8. The hand-off snapshot, only when asked (`--snapshot`).
    let (snapshot_path, snapshot_note, pruned) = if opts.snapshot {
        hand_off_snapshot(repo, &collab, &state, opts.keep, &opts.status, false)
    } else {
        (None, None, 0)
    };

    Ok(FinishResult {
        commit,
        staged,
        unstaged: to_unstage,
        snapshot_path,
        snapshot_note,
        pruned,
        folder_notes: ensured.notes(),
        checks: check_outcome,
    })
}

/// `c3 fail`: make sure the project folder is in place, write the BLOCKED/FAILED report and
/// journal and - when asked - an emergency `--fail` snapshot. No staging, no commit.
pub fn run_fail(opts: &FailOpts) -> Result<FailResult, FinishError> {
    let repo = &opts.repo_root;
    let ensured = state_dir::ensure(repo, Some(&opts.collab_root)).map_err(FinishError::Other)?;
    let state = ensured.dir.clone();
    write_answer(repo, &state, &journal::answer_fail(&opts.status)).map_err(FinishError::Other)?;
    write_journal(repo, &state, "BLOCKED/FAILED", &opts.status).map_err(FinishError::Other)?;
    // (S2, F36-1) the folder as it is now - a move made meanwhile is followed, never undone
    let state = state_dir::moved_with_folder(repo, &state);
    let collab = state_dir::moved_with_folder(repo, &opts.collab_root);

    let (snapshot_path, snapshot_note, pruned) = if opts.snapshot {
        hand_off_snapshot(repo, &collab, &state, opts.keep, &opts.status, true)
    } else {
        (None, None, 0)
    };

    Ok(FailResult {
        snapshot_path,
        snapshot_note,
        pruned,
        folder_notes: ensured.notes(),
    })
}

/// `c3 manifest`: apply one action to a `.c3/<name>.md` manifest (the project folder made sure of
/// first). No git, no snapshot. Returns the edited path and the folder notes.
pub fn run_manifest(
    repo_root: &Path,
    collab_root: Option<&Path>,
    name: &str,
    action: manifest::Action,
    content: &str,
    section: Option<&str>,
    target: Option<&str>,
) -> Result<(PathBuf, Vec<String>), String> {
    if !manifest::manifest_name_ok(name) {
        return Err(format!(
            "manifest name '{name}' is refused: it must be a plain NAME.md directly under {}/ (no path separator, no '..')",
            state_dir::STATE_DIR
        ));
    }
    let ensured = state_dir::ensure(repo_root, collab_root)?;
    // (S2, F36-1/F36-3) read and written at the folder's place now, never through a link
    let path = state_dir::write_in(repo_root, &ensured.dir, name, |path, current| {
        let current = match current {
            Ok(Some(s)) => s,
            Ok(None) => manifest::initial_content(name),
            Err(e) => return Err(format!("read {}: {e}", path.display())),
        };
        manifest::apply(&current, action, content, section, target).map(String::into_bytes)
    })?;
    Ok((path, ensured.notes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use secret_gate::{STAGED_LIST_PSEUDO_PATH, UNREADABLE};

    #[test]
    fn staged_list_unreadable_refusal_says_allow_secret_does_not_apply() {
        let err = FinishError::Secrets(vec![SecretHit {
            path: STAGED_LIST_PSEUDO_PATH.into(),
            kinds: UNREADABLE.into(),
        }]);
        let msg = err.to_string();
        assert!(
            msg.contains("git could not list the staged files"),
            "message: {msg}"
        );
        assert!(
            msg.contains("--allow-secret does not apply"),
            "message: {msg}"
        );
        assert!(
            !msg.contains("pass --allow-secret <path>"),
            "the repair-only advice must replace the per-file advice: {msg}"
        );
    }

    #[test]
    fn ordinary_secret_refusal_keeps_the_allow_secret_advice() {
        let err = FinishError::Secrets(vec![SecretHit {
            path: "config/app.toml".into(),
            kinds: "openai-key".into(),
        }]);
        let msg = err.to_string();
        assert!(
            msg.contains("pass --allow-secret <path> for each file you have reviewed"),
            "message: {msg}"
        );
        assert!(
            !msg.contains("--allow-secret does not apply"),
            "message: {msg}"
        );
    }
}
