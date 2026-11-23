//! `c3 doctor` — report what still needs doing in a C3 workspace (port spec, Part C).
//! The scan lives in [`crate::setup::doctor`]. Exit 0 when clean, 1 otherwise.

use std::path::PathBuf;

use clap::Args;

use crate::providers;
use crate::setup::branches::{self, BranchReport, SET_HINT};
use crate::setup::doctor;
use crate::setup::rules::{RuleStatus, BRANCHES_HEADING, DELEGATION_RULE};
use crate::state_dir::Layout;

/// Arguments of `c3 doctor`.
#[derive(Args, Debug, Default)]
pub struct DoctorArgs {
    /// Emit a JSON object instead of the text report.
    #[arg(long)]
    pub json: bool,
    /// Repair what can be repaired automatically (an existing `.eck/` -> moved to `.c3/` once, and a
    /// legacy `<collab>/.c3/` cache into it; stale agent model pins -> the alias; AGENTS.md
    /// -> the roster's successor; a kimi review's stale throwaway worktree -> removed; the
    /// "BRANCHES AND WORKTREES" section appended to a C3-written `02-delegation.md` that lacks it;
    /// an old template sentence naming `.eck/` in a rules file -> the current one), then report
    /// what is left. The edits are left uncommitted; a repair that cannot be made safely is
    /// reported; inside a C3 child nothing is repaired, moved or removed.
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

/// The branch/worktree lines: an unknown default branch, unknown branches, unrecorded worktrees and
/// stale c3 worktrees are findings (`[!]`); the start folder off the default branch or dirty is a
/// `[warn]` that does not change the exit code.
fn print_branch_findings(b: &BranchReport) {
    if let Some(why) = b.default_unknown_text() {
        println!("[!] {why} - until then no branch counts as merged");
    }
    if b.ancestry_failed {
        println!(
            "[!] the ancestry query against {} failed - no branch counts as merged",
            b.default_label()
        );
    }
    let unknown = b.unknown_branches();
    if unknown.is_empty() {
        println!(
            "[ok] every branch is merged into {} or has a recorded state",
            b.default_label()
        );
    } else {
        let names: Vec<&str> = unknown.iter().map(|x| x.name.as_str()).collect();
        println!(
            "[!] {} branch(es) not merged into {} and without a recorded state: {} - see c3 branches; record one with {SET_HINT}",
            unknown.len(),
            b.default_label(),
            branches::name_list(&names, 8)
        );
    }
    for w in b.unrecorded_worktrees() {
        println!(
            "[!] worktree {}: no recorded state - see c3 branches",
            branches::worktree_label(w)
        );
    }
    for s in &b.stale_worktrees {
        if s.removable {
            println!(
                "[!] stale c3 worktree {}: {} - c3 doctor --fix removes it",
                s.path, s.why
            );
        } else {
            println!(
                "[!] c3 worktree {}: {} (c3 doctor --fix does not remove it)",
                s.path, s.why
            );
        }
    }
    if let Some(s) = &b.start {
        if !s.on_default && b.default_branch.is_some() {
            let behind = s
                .behind
                .filter(|n| *n > 0)
                .map(|n| format!(", {n} behind {}", s.behind_of))
                .unwrap_or_default();
            println!(
                "[warn] the start folder {} is on {}, not {}{behind} - see c3 branches",
                s.path,
                s.branch.as_deref().unwrap_or("a detached HEAD"),
                b.default_label()
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
    // the remaining findings reflect the post-fix state). Inside a child C3 started (`C3_CHILD`)
    // the repair does nothing and says so (F04-4): a reviewer is read-only, a worker's patch holds
    // only its own change.
    let in_child = crate::engines::in_c3_child();
    // (0.2.5, S1) First the project folder: an existing `.eck/` moves to `.c3/` once, a legacy
    // `<collab>/.c3/` cache moves into it (no stubs - doctor reports, `c3 init` provisions).
    let (state_moves, state_problem) = if args.fix && !in_child {
        let collab_root = providers::resolve_collab_root(&repo_root, ".collab");
        match crate::state_dir::migrate(&repo_root, &collab_root) {
            Ok(v) => (v, None),
            Err(e) => (Vec::new(), Some(e)),
        }
    } else {
        (Vec::new(), None)
    };
    // ...and the old template sentences naming `.eck/` in a rules file C3 wrote.
    let sentence_repair = if args.fix && !in_child {
        crate::setup::rules::repair_state_sentences(&repo_root)
    } else {
        crate::setup::rules::StateSentenceRepair::default()
    };
    let repair = if args.fix {
        let models = providers::read_reviewer_roster()
            .map(|r| crate::setup::repo_models::roster_models(&r))
            .unwrap_or_default();
        crate::setup::repo_models::refresh_repo_models(&repo_root, &models)
    } else {
        crate::setup::repo_models::RepairReport::default()
    };
    let fixes = &repair.fixes;
    // `--fix` also appends the template's "BRANCHES AND WORKTREES" section to a delegation rule file
    // C3 wrote before the section existed (only that, only when the file still carries C3's title
    // line; a rewritten file is reported by the scan, never edited) - never inside a C3 child.
    let rule_repair = if args.fix && !in_child {
        crate::setup::rules::repair_delegation_rule(&repo_root)
    } else {
        crate::setup::rules::RuleRepair::default()
    };
    // `--fix` also removes a kimi review's throwaway worktree no live run owns (the kimi engine's
    // own link-safe removal) - never inside a C3 child (F04-4: a reviewer is read-only).
    let removed_worktrees = if args.fix && !in_child {
        branches::remove_stale_worktrees(&repo_root)
    } else {
        Vec::new()
    };

    let report = doctor::scan(&repo_root);
    // a repair that could not be made is a finding of its own (an AGENTS.md one has no scan line)
    let clean = report.is_clean()
        && repair.problems.is_empty()
        && rule_repair.problem.is_none()
        && state_problem.is_none()
        && sentence_repair.problems.is_empty();
    let state_read = if report.state_read.is_empty() {
        crate::state_dir::STATE_DIR.to_string()
    } else {
        report.state_read.clone()
    };

    if args.json {
        let obj = serde_json::json!({
            "clean": clean,
            "state_folder": {
                "layout": match report.state_layout {
                    Layout::Absent => "absent",
                    Layout::Current => "current",
                    Layout::LegacyOnly => "legacy_only",
                    Layout::Both => "both",
                },
                "read": report.state_read,
            },
            "state_moves": state_moves,
            "state_move_problem": state_problem,
            "state_sentences": report.state_sentences.iter().map(|s| serde_json::json!({
                "file": s.file,
                "old_sentence": s.old_sentence,
                "rewritten": s.rewritten,
            })).collect::<Vec<_>>(),
            "fixed_state_sentences": sentence_repair.fixed,
            "state_sentence_problems": sentence_repair.problems.iter().map(|(file, why)| serde_json::json!({
                "file": file,
                "why": why,
            })).collect::<Vec<_>>(),
            "fixed": fixes.iter().map(|f| serde_json::json!({
                "file": f.file,
                "old": f.old,
                "new": f.new,
            })).collect::<Vec<_>>(),
            "fix_problems": repair.problems.iter().map(|p| serde_json::json!({
                "file": p.file,
                "old": p.old,
                "new": p.new,
                "why": p.why,
            })).collect::<Vec<_>>(),
            "fixed_rules": rule_repair.fixed.iter().collect::<Vec<_>>(),
            "rule_fix_problems": rule_repair.problem.iter().map(|(file, why)| serde_json::json!({
                "file": file,
                "why": why,
            })).collect::<Vec<_>>(),
            "delegation_rule": match report.delegation_rule {
                RuleStatus::Absent => serde_json::json!({ "file": DELEGATION_RULE, "status": "absent" }),
                RuleStatus::Current => serde_json::json!({ "file": DELEGATION_RULE, "status": "current" }),
                RuleStatus::MissingSection { c3_written } => serde_json::json!({
                    "file": DELEGATION_RULE,
                    "status": "missing_section",
                    "section": BRANCHES_HEADING,
                    "c3_written": c3_written,
                }),
            },
            "fix_skipped": repair
                .skipped_in_child
                .then_some(crate::setup::repo_models::SKIPPED_IN_CHILD),
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
            "git": { "version": report.git_version, "problem": report.git_problem },
            "branches": report.branches.as_ref().map(|b| serde_json::json!({
                "default_branch": b.default_branch,
                "default_branch_error": b.default_unknown_text(),
                "ancestry_failed": b.ancestry_failed,
                "unknown_branches": b.unknown_branches().iter().map(|x| x.name.clone()).collect::<Vec<_>>(),
                "unrecorded_worktrees": b.unrecorded_worktrees().iter().map(|w| crate::cli::branches::worktree_json(w)).collect::<Vec<_>>(),
                "stale_worktrees": b.stale_worktrees.iter().map(crate::cli::branches::stale_json).collect::<Vec<_>>(),
                "start_folder": b.start.as_ref().map(crate::cli::branches::start_json),
            })),
            "removed_worktrees": removed_worktrees.iter().map(|r| serde_json::json!({
                "path": r.path,
                "removed": r.removed,
            })).collect::<Vec<_>>(),
        });
        println!("{obj}");
    } else {
        if repair.skipped_in_child {
            println!(
                "[i] --fix {}: C3_CHILD is set, nothing was repaired",
                crate::setup::repo_models::SKIPPED_IN_CHILD
            );
        }
        for m in &state_moves {
            if m.starts_with("moved ") || m.starts_with("added ") {
                println!("[fixed] {m}");
            } else {
                println!("[warn] {m}");
            }
        }
        if let Some(e) = &state_problem {
            println!("[!] {e}");
        }
        for f in &sentence_repair.fixed {
            println!(
                "[fixed] {f}: the template sentence naming {}/ now names {}/",
                crate::state_dir::LEGACY_STATE_DIR,
                crate::state_dir::STATE_DIR
            );
        }
        for (f, why) in &sentence_repair.problems {
            println!(
                "[!] {f}: the template sentence naming {}/ not replaced - {why}",
                crate::state_dir::LEGACY_STATE_DIR
            );
        }
        for f in fixes {
            println!("[fixed] {}: model {} -> {}", f.file, f.old, f.new);
        }
        for p in &repair.problems {
            println!(
                "[!] {}: model {} -> {} not repaired - {}",
                p.file, p.old, p.new, p.why
            );
        }
        if let Some(file) = &rule_repair.fixed {
            println!(
                "[fixed] {file}: appended the \"BRANCHES AND WORKTREES\" section of C3's template"
            );
        }
        if let Some((file, why)) = &rule_repair.problem {
            println!("[!] {file}: the \"BRANCHES AND WORKTREES\" section not appended - {why}");
        }
        for r in &removed_worktrees {
            if r.removed {
                println!("[fixed] removed the stale c3 worktree {}", r.path);
            } else {
                println!("[!] could not remove the stale c3 worktree {}", r.path);
            }
        }
        match report.state_layout {
            Layout::Absent => println!(
                "[i] no {}/ folder - `c3 init` creates it",
                crate::state_dir::STATE_DIR
            ),
            Layout::LegacyOnly if crate::state_dir::migration_enabled() => println!(
                "[i] no {}/ folder yet - {}/ is read until a writing command (c3 init, c3 finish, c3 fail, c3 manifest, c3 doctor --fix) moves it",
                crate::state_dir::STATE_DIR,
                crate::state_dir::LEGACY_STATE_DIR
            ),
            Layout::LegacyOnly => println!(
                "[i] no {}/ folder - {}/ is read and left as it is ({}=off or \"state_migrate\": false); a writing command creates a fresh {}/",
                crate::state_dir::STATE_DIR,
                crate::state_dir::LEGACY_STATE_DIR,
                crate::state_dir::MIGRATE_ENV,
                crate::state_dir::STATE_DIR
            ),
            Layout::Both => println!(
                "[warn] both {}/ and {}/ exist - C3 uses {}/; merge or remove {}/ by hand",
                crate::state_dir::STATE_DIR,
                crate::state_dir::LEGACY_STATE_DIR,
                crate::state_dir::STATE_DIR,
                crate::state_dir::LEGACY_STATE_DIR
            ),
            Layout::Current => {}
        }
        if report.stub_files.is_empty() {
            println!("[ok] no [STUB] markers left in {state_read}/*.md");
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

        // (0.2.5) revisions reach git after `--end-of-options` (git 2.30 or later)
        match &report.git_problem {
            Some(p) => println!("[!] {p}"),
            None => println!("[ok] git {}", report.git_version),
        }

        if report.eck_core_in.is_empty() && report.eck_rule_files.is_empty() {
            println!("[ok] no leftovers of the old eck tooling");
        } else {
            for loc in &report.eck_core_in {
                println!(
                    "[!] eck-core server still referenced in {loc} - C3 does not need that server (`c3 mcp` has the finish, fail and manifest tools); remove the entry"
                );
            }
            for f in &report.eck_rule_files {
                println!("[!] rule still names an eck_* tool: {f}");
            }
        }

        for s in &report.state_sentences {
            if s.old_sentence {
                println!(
                    "[!] {}: an old C3 template sentence names {}/ - c3 doctor --fix replaces it with the current one",
                    s.file,
                    crate::state_dir::LEGACY_STATE_DIR
                );
            }
            if s.rewritten {
                println!(
                    "[warn] {}: names {}/ in text that is not C3's template - left as it is (C3's folder is {}/)",
                    s.file,
                    crate::state_dir::LEGACY_STATE_DIR,
                    crate::state_dir::STATE_DIR
                );
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

        match report.delegation_rule {
            RuleStatus::Absent => {}
            RuleStatus::Current => {
                println!("[ok] {DELEGATION_RULE} has the \"BRANCHES AND WORKTREES\" section");
            }
            RuleStatus::MissingSection { c3_written: true } => println!(
                "[!] {DELEGATION_RULE}: C3 wrote it before the \"BRANCHES AND WORKTREES\" section existed - c3 doctor --fix appends that section"
            ),
            RuleStatus::MissingSection { c3_written: false } => println!(
                "[warn] {DELEGATION_RULE}: no \"BRANCHES AND WORKTREES\" section, and the file is not C3's template any more - left as it is (the section is in crates/c3/templates/02-delegation.md of the C3 repository)"
            ),
        }

        if let Some(b) = &report.branches {
            print_branch_findings(b);
        }

        println!(
            "[i] {state_read}/snapshots/: {} file(s), {}",
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
