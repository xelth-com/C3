//! `-Kick -Member <NN> [-Id <id8>]` (wave 26b, D10): stop ONE running member of a task (a panel
//! seat or a single run) by its handoff number. Ported from `codex-consult.ps1`'s `-Kick` block.
//!
//! The command writes `<task>/.consult.kick-<NN>`; the run's primary turn polls that file (see
//! [`crate::engines::subprocess::run_turn`]), stops its engine's process tree, salvages the
//! partial output and records `failed: stopped by the operator (-Kick)` (class `operator`); the
//! kicked run removes the file as its acknowledgement, which this command waits for. Exit `0`
//! done, `1` no such member / not running, `4` the command is misused.

use std::path::Path;

use super::args::Options;
use super::detached::{detached_paths, read_detached_status};
use crate::liveness::pending;
use crate::providers;

const TOOL: &str = "codex-consult";
/// How long the command waits for the run to take the kick (`codex-consult.ps1`: 60 s). A test may
/// shorten it with `CODEX_CONSULT_TEST_KICK_WAIT_MS`.
fn kick_wait_ms() -> u64 {
    std::env::var("CODEX_CONSULT_TEST_KICK_WAIT_MS")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(60_000)
}

fn misuse(msg: &str) -> i32 {
    eprintln!("{TOOL}: {msg}");
    4
}
fn fail(msg: &str) -> i32 {
    eprintln!("{TOOL}: -Kick: {msg}");
    1
}

/// Run the `-Kick` command. Called from `run_inner` before any run dispatch.
pub fn run(o: &Options) -> i32 {
    // -Member without -Kick, and the other misuses (exit 4).
    if !o.kick {
        return misuse("-Member goes with -Kick.");
    }
    if o.task.is_empty() || !c3_core::task_slug::is_slug(&o.task) {
        return misuse("-Task must be a slug (letters, digits, dot, dash, underscore).");
    }
    let member = o.member.trim();
    let nn_num: Option<u32> =
        if (1..=4).contains(&member.len()) && member.chars().all(|c| c.is_ascii_digit()) {
            member.parse().ok()
        } else {
            None
        };
    let nn = match nn_num {
        Some(n) => format!("{n:02}"),
        None => {
            return misuse(&format!(
                "-Kick needs -Member <NN>: the member's handoff number as the panel prints it (e.g. -Member 03); got '{member}'."
            ))
        }
    };

    let cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
    let repo = providers::resolve_repo_root(&cwd);
    let collab = providers::resolve_collab_root(&repo, &o.collab_dir);
    let task_dir = collab.join(&o.task);
    if !task_dir.is_dir() {
        return fail(&format!("no task directory '{}'.", task_dir.display()));
    }

    // A detached run named by -Id: the member must be one of ITS members, running.
    if o.id_given {
        let paths = detached_paths(&task_dir, &o.id);
        match read_detached_status(&paths.status) {
            Ok(Some(rec)) => match rec.members.iter().find(|m| m.handoff == nn) {
                None => {
                    return fail(&format!(
                        "detached run {} has no member with handoff {nn}.",
                        rec.id8
                    ))
                }
                Some(m) if m.state != "running" => {
                    return fail(&format!(
                        "member {nn} of detached run {} is not running (state {}).",
                        rec.id8, m.state
                    ))
                }
                _ => {}
            },
            _ => {
                return fail(&format!("no detached run {} in task {}.", o.id, o.task));
            }
        }
    }

    // The run's recovery record: a member's `.consult.pending-<NN>.json`, a single run's
    // `.consult.pending.json`. It must name NN, be `running`, and its engine child alive.
    let mut record: Option<serde_json::Value> = None;
    for p in pending::pending_paths(&task_dir) {
        let rd = pending::read_pending_file(&p);
        if let Some(rec) = rd.record {
            if rec.get("nn").and_then(|v| v.as_str()) == Some(nn.as_str()) {
                record = Some(rec);
                break;
            }
        }
    }
    let record = match record {
        Some(r) => r,
        None => {
            return fail(&format!(
                "no run with handoff {nn} is in progress in task {} (no recovery record names it).",
                o.task
            ))
        }
    };
    let state = record.get("state").and_then(|v| v.as_str()).unwrap_or("");
    let child_pid = record
        .get("child_pid")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let child_start = record
        .get("child_start_time")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if state != "running"
        || child_pid == 0
        || !crate::liveness::proc::pid_alive(child_pid, child_start)
    {
        return fail(&format!(
            "the run with handoff {nn} has no engine turn running (state {state})."
        ));
    }

    // Write the kick file and wait for the run to remove it (its acknowledgement).
    let kick_path = task_dir.join(format!(".consult.kick-{nn}"));
    let stamp = c3_core::health::format_offset_iso(chrono::Local::now().into());
    if std::fs::write(
        &kick_path,
        format!("{stamp} -Kick from pid {}\n", std::process::id()),
    )
    .is_err()
    {
        return fail(&format!(
            "could not write the kick file {}.",
            kick_path.display()
        ));
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(kick_wait_ms());
    while kick_path.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    if kick_path.exists() {
        let _ = std::fs::remove_file(&kick_path);
        return fail(&format!(
            "the run with handoff {nn} did not take the kick within 60 s (its engine turn may have ended meanwhile); the kick file was removed."
        ));
    }
    println!(
        "{TOOL}: -Kick: member {nn} of task {} stopped - its engine's process tree is stopped, its partial output salvaged; it records \"failed: stopped by the operator (-Kick)\" (the panel goes on with the others).",
        o.task
    );
    0
}
