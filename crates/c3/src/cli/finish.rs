//! `c3 finish` — the coordinator-only commit-and-hand-off call (port spec, Part A).
//! Supervisor-only: workers never call it. The logic lives in [`crate::finish`].

use std::path::PathBuf;

use clap::Args;

use crate::finish::{self, FinishError, FinishOpts};
use crate::providers;

/// Arguments of `c3 finish`.
#[derive(Args, Debug)]
pub struct FinishArgs {
    /// The commit message (verbatim; the coordinator adds its own trailers).
    #[arg(long, allow_hyphen_values = true)]
    pub message: String,
    /// The agent report written to AnswerToSA.md and the journal.
    #[arg(long, allow_hyphen_values = true)]
    pub status: Option<String>,
    /// Read the agent report from a file instead of `--status`.
    #[arg(long)]
    pub status_file: Option<String>,
    /// How many snapshots to keep in `.eck/snapshots/`.
    #[arg(long, default_value_t = 10)]
    pub keep: usize,
    /// Skip the hand-off snapshot.
    #[arg(long)]
    pub no_snapshot: bool,
    /// Commit even when nothing is staged.
    #[arg(long)]
    pub allow_empty: bool,
    /// Let a reviewed file past the secret gate (repeatable; repo-relative path).
    #[arg(long = "allow-secret")]
    pub allow_secret: Vec<String>,
    /// Where consultations are stored; a relative path resolves against the repo root.
    #[arg(long, default_value = ".collab")]
    pub collab_dir: String,
    /// Emit a JSON object instead of the text summary.
    #[arg(long)]
    pub json: bool,
}

/// Resolve the agent report from `--status` or `--status-file`.
pub fn resolve_status(
    status: &Option<String>,
    status_file: &Option<String>,
) -> Result<String, String> {
    match (status, status_file) {
        (Some(_), Some(_)) => Err("pass either --status or --status-file, not both".to_string()),
        (Some(s), None) => Ok(s.clone()),
        (None, Some(f)) => {
            std::fs::read_to_string(f).map_err(|e| format!("read status file '{f}': {e}"))
        }
        (None, None) => Err("one of --status or --status-file is required".to_string()),
    }
}

/// Run `c3 finish`. Returns the process exit code.
pub fn run(args: FinishArgs) -> i32 {
    let status = match resolve_status(&args.status, &args.status_file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("c3 finish: {e}");
            return 1;
        }
    };

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = providers::resolve_repo_root(&cwd);
    let collab_root = providers::resolve_collab_root(&repo_root, &args.collab_dir);

    let opts = FinishOpts {
        repo_root,
        collab_root,
        message: args.message,
        status,
        keep: args.keep,
        snapshot: !args.no_snapshot,
        allow_empty: args.allow_empty,
        allow_secret: args.allow_secret,
    };

    match finish::run_finish(&opts) {
        Ok(r) => {
            if args.json {
                let obj = serde_json::json!({
                    "commit": r.commit,
                    "staged": r.staged.len(),
                    "staged_files": r.staged,
                    "unstaged": r.unstaged,
                    "snapshot": r.snapshot_path.as_ref().map(|p| p.display().to_string()),
                    "snapshot_note": r.snapshot_note,
                    "pruned": r.pruned,
                });
                println!("{obj}");
            } else {
                let short = r.commit.chars().take(7).collect::<String>();
                println!("Committed {short}");
                println!("Staged {} file(s):", r.staged.len());
                for s in &r.staged {
                    println!("  {s}");
                }
                if !r.unstaged.is_empty() {
                    println!("Unstaged {} never-commit file(s).", r.unstaged.len());
                }
                match (&r.snapshot_path, &r.snapshot_note) {
                    (Some(p), _) => {
                        let pruned = if r.pruned > 0 {
                            format!(" (pruned {})", r.pruned)
                        } else {
                            String::new()
                        };
                        println!("Snapshot: {}{pruned}", p.display());
                    }
                    (None, Some(note)) => println!("{note}"),
                    (None, None) => {}
                }
            }
            0
        }
        Err(e) => {
            // The refusal text names the files/kinds but never the values.
            eprintln!("c3 finish: {e}");
            match e {
                FinishError::NothingToCommit | FinishError::Secrets(_) | FinishError::Other(_) => 1,
            }
        }
    }
}
