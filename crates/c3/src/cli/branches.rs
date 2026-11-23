//! `c3 branches` — every local branch and worktree with its recorded state, and the start folder.
//! The scan and the record format live in [`crate::setup::branches`]. The listing is a report and
//! always exits 0; `--set` / `--clear` / `--default` write one config value and exit 1 on a
//! refusal (all three always refuse inside a child C3 started, `C3_CHILD`). Nothing is
//! ever deleted or merged.

use std::path::{Path, PathBuf};

use clap::Args;
use serde_json::{json, Value};

use crate::providers;
use crate::setup::branches::{
    self, BranchInfo, BranchReport, BranchState, ClearOutcome, ScanOpts, StaleWorktree,
    StartFolder, WorktreeInfo, PATCH_CHECK_CAP, SET_HINT,
};

/// Arguments of `c3 branches`.
#[derive(Args, Debug, Default)]
pub struct BranchesArgs {
    /// Emit a JSON object instead of the text report.
    #[arg(long)]
    pub json: bool,
    /// Record the state of this branch (with --state and --note).
    #[arg(long, value_name = "BRANCH", conflicts_with = "clear")]
    pub set: Option<String>,
    /// The state to record: work, waits or parked.
    #[arg(long, requires = "set", value_parser = ["work", "waits", "parked"])]
    pub state: Option<String>,
    /// `waits`: the question put to the operator; `parked`: the condition for merging it.
    #[arg(long, requires = "set", allow_hyphen_values = true)]
    pub note: Option<String>,
    /// The owning session or person (default: CODEX_CONSULT_COORDINATOR, else empty).
    #[arg(long, requires = "set", allow_hyphen_values = true)]
    pub owner: Option<String>,
    /// Remove C3's recorded state from this branch (a foreign description is never touched).
    #[arg(long, value_name = "BRANCH")]
    pub clear: Option<String>,
    /// Name the repository's default branch (stored as `c3.defaultBranch` in the local git config;
    /// it wins over origin/HEAD, init.defaultBranch, main and master).
    #[arg(long = "default", value_name = "BRANCH", conflicts_with_all = ["set", "clear"])]
    pub default_branch: Option<String>,
}

/// Run `c3 branches`. Returns the process exit code.
pub fn run(args: BranchesArgs) -> i32 {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = providers::resolve_repo_root(&cwd);

    if let Some(branch) = &args.set {
        return run_set(&repo_root, branch, &args);
    }
    if let Some(branch) = &args.clear {
        return run_clear(&repo_root, branch, args.json);
    }
    if let Some(branch) = &args.default_branch {
        return run_default(&repo_root, branch, args.json);
    }

    let Some(report) = branches::scan(
        &repo_root,
        ScanOpts {
            content: true,
            dirty: true,
        },
    ) else {
        if args.json {
            println!(
                "{}",
                json!({ "error": "not a git repository, or no local branch" })
            );
        } else {
            println!("c3 branches: not a git repository, or no local branch");
        }
        return 0;
    };

    if args.json {
        println!("{}", report_json(&report));
    } else {
        print_report(&report);
    }
    0
}

fn run_set(repo_root: &Path, branch: &str, args: &BranchesArgs) -> i32 {
    let Some(state) = args.state.as_deref().and_then(BranchState::parse) else {
        eprintln!("c3 branches: --set needs --state work|waits|parked");
        return 1;
    };
    let note = args.note.clone().unwrap_or_default();
    let owner = args.owner.clone().unwrap_or_else(branches::default_owner);
    match branches::set_state(repo_root, branch, state, &note, &owner, &branches::today()) {
        Ok(line) => {
            if args.json {
                println!("{}", json!({ "branch": branch, "set": line }));
            } else {
                println!("c3 branches: {branch}: {line}");
            }
            0
        }
        Err(e) => {
            eprintln!("c3 branches: {e}");
            1
        }
    }
}

fn run_clear(repo_root: &Path, branch: &str, json_out: bool) -> i32 {
    match branches::clear_state(repo_root, branch) {
        Ok(outcome) => {
            let (word, text) = match outcome {
                ClearOutcome::Cleared => ("cleared", "recorded state removed"),
                ClearOutcome::NothingRecorded => ("nothing", "no recorded state"),
                ClearOutcome::ForeignKept => (
                    "foreign_kept",
                    "the description is not a C3 record; left untouched",
                ),
            };
            if json_out {
                println!("{}", json!({ "branch": branch, "clear": word }));
            } else {
                println!("c3 branches: {branch}: {text}");
            }
            0
        }
        Err(e) => {
            eprintln!("c3 branches: {e}");
            1
        }
    }
}

fn run_default(repo_root: &Path, branch: &str, json_out: bool) -> i32 {
    match branches::set_default_branch(repo_root, branch) {
        Ok(()) => {
            if json_out {
                println!("{}", json!({ "default_branch": branch }));
            } else {
                println!(
                    "c3 branches: default branch set to {branch} ({})",
                    branches::DEFAULT_BRANCH_KEY
                );
            }
            0
        }
        Err(e) => {
            eprintln!("c3 branches: {e}");
            1
        }
    }
}

/// The JSON of one branch. `merged` is null when the ancestry is not known; `patches_on_default` is
/// `git cherry`'s historical patch equivalence (null when not checked).
pub fn branch_json(b: &BranchInfo) -> Value {
    json!({
        "name": b.name,
        "state": b.record.as_ref().map(|r| r.state.as_str()),
        "owner": b.record.as_ref().map(|r| r.owner.clone()),
        "date": b.record.as_ref().map(|r| r.date.clone()),
        "note": b.record.as_ref().map(|r| r.text.clone()),
        "foreign_description": b.foreign_note,
        "merged": b.merged,
        "patches_on_default": b.patches_on_default,
        "patches_checked": b.patches_checked,
        "ahead": b.ahead,
        "last_commit": b.last_commit,
        "worktree": b.worktree,
        "unknown": b.is_unknown(),
    })
}

/// The JSON of one worktree.
pub fn worktree_json(w: &WorktreeInfo) -> Value {
    json!({
        "path": w.path,
        "branch": w.branch,
        "recorded": w.recorded,
        "prunable": w.prunable,
    })
}

/// The JSON of one reported throwaway worktree (`removable`: `c3 doctor --fix` removes it; `false`:
/// it is left alone - a link, no owner record, an owner that cannot be checked).
pub fn stale_json(s: &StaleWorktree) -> Value {
    json!({ "path": s.path, "why": s.why, "removable": s.removable })
}

/// The JSON of the start folder.
pub fn start_json(s: &StartFolder) -> Value {
    json!({
        "path": s.path,
        "branch": s.branch,
        "on_default": s.on_default,
        "behind": s.behind,
        "behind_of": s.behind_of,
        "dirty": s.dirty,
    })
}

fn report_json(r: &BranchReport) -> Value {
    json!({
        "default_branch": r.default_branch,
        "default_branch_error": r.default_unknown_text(),
        "ancestry_failed": r.ancestry_failed,
        "start_folder": r.start.as_ref().map(start_json),
        "branches": r.branches.iter().map(branch_json).collect::<Vec<_>>(),
        "worktrees": r.worktrees.iter().map(worktree_json).collect::<Vec<_>>(),
        "stale_worktrees": r.stale_worktrees.iter().map(stale_json).collect::<Vec<_>>(),
        "merged": r.branches.iter().filter(|b| b.merged == Some(true)).count(),
        "unknown": r.unknown_branches().len(),
        "unknown_worktrees": r.unrecorded_worktrees().len(),
        "patches_not_checked": r.patches_unchecked,
    })
}

fn print_report(r: &BranchReport) {
    let d = r.default_label();
    match r.default_unknown_text() {
        Some(why) => println!("[!] {why}"),
        None => println!("default branch: {d}"),
    }
    if r.ancestry_failed {
        println!("[!] the ancestry query against {d} failed: no branch counts as merged");
    }

    if let Some(s) = &r.start {
        let branch = s.branch.as_deref().unwrap_or("detached HEAD");
        let behind = match (s.behind, s.behind_of.is_empty()) {
            (_, true) => "behind unknown".to_string(),
            (Some(n), false) => format!("{n} behind {}", s.behind_of),
            (None, false) => format!("behind {} unknown", s.behind_of),
        };
        let dirty = match s.dirty {
            Some(0) => "clean".to_string(),
            Some(n) => format!("{n} uncommitted change(s)"),
            None => "uncommitted changes unknown".to_string(),
        };
        let mark = if s.on_default && s.dirty.unwrap_or(0) == 0 {
            "[ok]"
        } else {
            "[warn]"
        };
        println!(
            "{mark} start folder {}: on {branch}{}, {behind}, {dirty}",
            s.path,
            if s.on_default || r.default_branch.is_none() {
                String::new()
            } else {
                format!(" (not {d})")
            }
        );
    }

    if r.branches.is_empty() {
        println!("no branches besides {d}");
    } else {
        println!("branches:");
        for b in &r.branches {
            let mark = if b.is_unknown() { "[!]" } else { "   " };
            let state = match &b.record {
                Some(rec) => rec.describe(),
                None if b.foreign_note => "- (own description, no recorded state)".to_string(),
                None => "-".to_string(),
            };
            let mut line = format!(
                "{mark} {}: {state}; {}",
                b.name,
                branches::merge_words(b, d)
            );
            if b.merged == Some(false) && !b.patches_checked {
                line.push_str("; patches not checked");
            }
            if let Some(n) = b.ahead {
                if b.merged != Some(true) {
                    line.push_str(&format!("; {n} ahead"));
                }
            }
            if !b.last_commit.is_empty() {
                line.push_str(&format!("; last commit {}", b.last_commit));
            }
            if let Some(w) = &b.worktree {
                line.push_str(&format!("; worktree {w}"));
            }
            if b.is_unknown() {
                line.push_str("; UNKNOWN");
            }
            println!("{line}");
        }
        if r.patches_unchecked > 0 {
            println!(
                "patches not checked for {} unmerged branch(es): the patch check (git cherry, two git calls per branch) runs for at most {PATCH_CHECK_CAP}, and a check whose git call failed counts here too",
                r.patches_unchecked
            );
        }
    }

    if !r.worktrees.is_empty() {
        println!("worktrees:");
        for w in &r.worktrees {
            let mark = if w.recorded { "   " } else { "[!]" };
            let tail = if w.recorded {
                "recorded"
            } else {
                "no recorded state"
            };
            println!("{mark} {}: {tail}", branches::worktree_label(w));
        }
    }

    let (removable, left_alone): (Vec<&StaleWorktree>, Vec<&StaleWorktree>) =
        r.stale_worktrees.iter().partition(|s| s.removable);
    if !removable.is_empty() {
        println!("stale c3 worktrees (a review's throwaway no live run owns; c3 doctor --fix removes them):");
        for s in &removable {
            println!("[!] {}: {}", s.path, s.why);
        }
    }
    if !left_alone.is_empty() {
        println!("c3 worktrees c3 doctor --fix leaves alone (a review's throwaway it cannot judge safely):");
        for s in &left_alone {
            println!("[!] {}: {}", s.path, s.why);
        }
    }

    let unknown = r.unknown_branches().len();
    let unrec = r.unrecorded_worktrees().len();
    let mut summary = format!(
        "branches: {} listed, {} merged, {} unknown; worktrees: {}, {} without a recorded state",
        r.branches.len(),
        r.branches.iter().filter(|b| b.merged == Some(true)).count(),
        unknown,
        r.worktrees.len(),
        unrec
    );
    if !removable.is_empty() {
        summary.push_str(&format!("; {} stale c3 worktree(s)", removable.len()));
    }
    if !left_alone.is_empty() {
        summary.push_str(&format!("; {} c3 worktree(s) left alone", left_alone.len()));
    }
    println!("{summary}");
    if unknown > 0 || unrec > 0 {
        println!("record a state: {SET_HINT}; or integrate (merge, not cherry-pick) and delete it");
    }
}
