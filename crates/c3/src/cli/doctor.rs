//! `c3 doctor` — report what still needs doing in a C3 workspace (port spec, Part C).
//! The scan lives in [`crate::setup::doctor`]. Exit 0 when clean, 1 otherwise.

use std::path::PathBuf;

use clap::Args;

use crate::providers;
use crate::setup::branches::{self, BranchReport, SET_HINT};
use crate::setup::doctor;

/// Arguments of `c3 doctor`.
#[derive(Args, Debug, Default)]
pub struct DoctorArgs {
    /// Emit a JSON object instead of the text report.
    #[arg(long)]
    pub json: bool,
    /// Repair what can be repaired automatically (stale agent model pins -> the alias; AGENTS.md
    /// -> the roster's successor), then report what is left. The edits are left uncommitted.
    #[arg(long)]
    pub fix: bool,
}

fn human_bytes(n: u64) -> String {
    if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    }
}

/// The branch/worktree lines: unknown branches and unrecorded worktrees are findings (`[!]`); the
/// start folder off the default branch or dirty is a `[warn]` that does not change the exit code.
fn print_branch_findings(b: &BranchReport) {
    let unknown = b.unknown_branches();
    if unknown.is_empty() {
        println!(
            "[ok] every branch is merged into {} or has a recorded state",
            b.default_branch
        );
    } else {
        let names: Vec<&str> = unknown.iter().map(|x| x.name.as_str()).collect();
        println!(
            "[!] {} branch(es) not merged into {} and without a recorded state: {} - see c3 branches; record one with {SET_HINT}",
            unknown.len(),
            b.default_branch,
            branches::name_list(&names, 8)
        );
    }
    for w in b.unrecorded_worktrees() {
        println!(
            "[!] worktree {}: no recorded state - see c3 branches",
            branches::worktree_label(w)
        );
    }
    if let Some(s) = &b.start {
        if !s.on_default {
            let behind = s
                .behind
                .filter(|n| *n > 0)
                .map(|n| format!(", {n} behind {}", s.behind_of))
                .unwrap_or_default();
            println!(
                "[warn] the start folder {} is on {}, not {}{behind} - see c3 branches",
                s.path,
                s.branch.as_deref().unwrap_or("a detached HEAD"),
                b.default_branch
            );
        }
        if let Some(n) = s.dirty.filter(|n| *n > 0) {
            println!(
                "[warn] the start folder {} has {n} uncommitted change(s)",
                s.path
            );
        }
    }
}

/// Run `c3 doctor`. Returns the process exit code (0 clean, 1 with findings).
pub fn run(args: DoctorArgs) -> i32 {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = providers::resolve_repo_root(&cwd);

    // `--fix`: apply the automatic repairs first, then report what is LEFT (so the exit code and
    // the remaining findings reflect the post-fix state).
    let fixes = if args.fix {
        let models = providers::read_reviewer_roster()
            .map(|r| crate::setup::repo_models::roster_models(&r))
            .unwrap_or_default();
        crate::setup::repo_models::refresh_repo_models(&repo_root, &models)
    } else {
        Vec::new()
    };

    let report = doctor::scan(&repo_root);
    let clean = report.is_clean();

    if args.json {
        let obj = serde_json::json!({
            "clean": clean,
            "fixed": fixes.iter().map(|f| serde_json::json!({
                "file": f.file,
                "old": f.old,
                "new": f.new,
            })).collect::<Vec<_>>(),
            "stub_files": report.stub_files,
            "mcp_registered": report.mcp_registered,
            "eck_core_in": report.eck_core_in,
            "eck_rule_files": report.eck_rule_files,
            "pinned_agents": report.pinned_agents.iter().map(|p| serde_json::json!({
                "file": p.file,
                "model": p.model,
                "alias": p.alias,
            })).collect::<Vec<_>>(),
            "snapshot_count": report.snapshot_count,
            "snapshot_bytes": report.snapshot_bytes,
            "branches": report.branches.as_ref().map(|b| serde_json::json!({
                "default_branch": b.default_branch,
                "unknown_branches": b.unknown_branches().iter().map(|x| x.name.clone()).collect::<Vec<_>>(),
                "unrecorded_worktrees": b.unrecorded_worktrees().iter().map(|w| crate::cli::branches::worktree_json(w)).collect::<Vec<_>>(),
                "start_folder": b.start.as_ref().map(crate::cli::branches::start_json),
            })),
        });
        println!("{obj}");
    } else {
        for f in &fixes {
            println!("[fixed] {}: model {} -> {}", f.file, f.old, f.new);
        }
        if report.stub_files.is_empty() {
            println!("[ok] no [STUB] markers left in .eck/*.md");
        } else {
            println!(
                "[!] {} manifest(s) still carry a [STUB] marker:",
                report.stub_files.len()
            );
            for f in &report.stub_files {
                println!("    {f}");
            }
        }

        if report.mcp_registered {
            println!("[ok] c3 mcp is registered in .mcp.json");
        } else {
            println!("[!] c3 mcp is NOT registered in .mcp.json (run `c3 init`)");
        }

        if report.eck_core_in.is_empty() && report.eck_rule_files.is_empty() {
            println!("[ok] no eck leftovers");
        } else {
            for loc in &report.eck_core_in {
                println!("[!] eck-core server still referenced in {loc}");
            }
            for f in &report.eck_rule_files {
                println!("[!] rule still names an eck_* tool: {f}");
            }
        }

        if report.pinned_agents.is_empty() {
            println!("[ok] no agent file pins an old model");
        } else {
            for p in &report.pinned_agents {
                if p.alias.is_empty() {
                    println!(
                        "[warn] {}: model pinned to {} - write a floating alias so the worker follows the newest model",
                        p.file, p.model
                    );
                } else {
                    println!(
                        "[warn] {}: model pinned to {} - write the alias ({}) so the worker follows the newest model",
                        p.file, p.model, p.alias
                    );
                }
            }
        }

        if let Some(b) = &report.branches {
            print_branch_findings(b);
        }

        println!(
            "[i] .eck/snapshots/: {} file(s), {}",
            report.snapshot_count,
            human_bytes(report.snapshot_bytes)
        );

        println!(
            "{}",
            if clean {
                "doctor: clean"
            } else {
                "doctor: issues found (see above)"
            }
        );
    }

    if clean {
        0
    } else {
        1
    }
}
