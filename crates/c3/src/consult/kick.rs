//! `-Kick -Member <NN> [-Id <id8>]` (wave 26b, D10): stop ONE running member of a task (a panel
//! seat or a single run) by its handoff number. Ported from `codex-consult.ps1`'s `-Kick` block.
//!
//! The command writes `<task>/.consult.kick-<NN>`; the run's primary turn polls that file (see
//! [`crate::engines::subprocess::run_turn`]), stops its engine's process tree, salvages the
//! partial output and records `failed: stopped by the operator (-Kick)` (class `operator`); the
//! kicked run writes `<kick file>.ack` (`kicked` | `late`) and removes the kick file, which this
//! command waits up to 10 s for (wave 26c, D1). Exit `0` acknowledged (the message says whether it
//! was taken live or LATE), `1` no such member / not running (a stale kick file of that number is
//! removed) - (wave 7c, F40-1) or the task's recovery records cannot be listed, and (F40-2) a kick
//! acknowledged `not killed` (the kill's record could be written nowhere: the tree is left alive and
//! the task refused), and (wave 7d, F42-1) one of the task's recovery records cannot be inspected -
//! `3` no acknowledgement in time (the kick file stays for the member's next poll), `4` the command
//! is misused.

use std::path::Path;

use super::args::Options;
use super::detached::{detached_paths, read_detached_status};
use crate::liveness::pending;
use crate::providers;

const TOOL: &str = "codex-consult";
/// How long the command waits for the run to acknowledge the kick (`codex-consult.ps1`, wave 26c:
/// 10 s). A test may shorten it with `CODEX_CONSULT_TEST_KICK_WAIT_MS`.
fn kick_wait_ms() -> u64 {
    c3_core::test_hooks::hook("CODEX_CONSULT_TEST_KICK_WAIT_MS")
        .and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(10_000)
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
    // (wave 7d, F42-1 sweep) only an absent task directory is "no task directory"
    match std::fs::metadata(&task_dir) {
        Ok(m) if m.is_dir() => {}
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            let why = c3_core::one_line(&e.to_string());
            return fail(&format!(
                "the task directory {} cannot be read ({why}): the kick is refused until it can be read.",
                task_dir.display()
            ));
        }
        _ => return fail(&format!("no task directory '{}'.", task_dir.display())),
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
    // (Wave 7c, F40-1) records that cannot all be listed refuse the kick, as any consumer; (wave
    // 7d, F42-1) so does a record that cannot be inspected or is unusable.
    let record = match kick_record(pending::pending_paths(&task_dir), &nn) {
        Err(err) => return fail(&err),
        Ok(Some(r)) => r,
        Ok(None) => {
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
    let kick_path = task_dir.join(format!(".consult.kick-{nn}"));
    let ack_path = crate::engines::subprocess::kick_ack_path(&kick_path);
    if state != "running"
        || child_pid == 0
        || !crate::liveness::proc::pid_alive(child_pid, child_start)
    {
        // (wave 26c, D1) no engine turn runs: remove any stale kick file of this number.
        let _ = std::fs::remove_file(&kick_path);
        return fail(&format!(
            "the run with handoff {nn} has no engine turn running (state {state})."
        ));
    }

    // (wave 26c D1 / 27c D1) the kick carries a request id and is written atomically. A caller that
    // finds a kick file present JOINS it (reads its id) and never overwrites it; only a creator
    // writes a fresh id. A stale acknowledgement (older than 60 s) whose id is not ours is swept
    // first. The member acknowledges with `<kick>.ack` holding the id and result (`stopped`/`late`).
    crate::engines::subprocess::sweep_stale_ack(&kick_path, None, 60);
    // (D1) Resolve the request id: JOIN an existing kick file's id, or CREATE a fresh one. The
    // write never overwrites (hard_link), so two concurrent callers converge on ONE id — the loser
    // of the create race re-reads and joins. Retry up to 20 x 100 ms, like `New-KickRequest`.
    let mut request_id = String::new();
    let mut is_creator = false;
    for _ in 0..20 {
        if let Some(id) = crate::engines::subprocess::read_kick_id(&kick_path) {
            if kick_path.exists() {
                request_id = id;
                is_creator = false;
                break;
            }
        }
        let id = uuid::Uuid::new_v4().simple().to_string();
        match crate::engines::subprocess::write_kick_atomic(&kick_path, &id) {
            Ok(()) => {
                request_id = id;
                is_creator = true;
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // Another caller created it first: re-read and join on the next pass.
                std::thread::sleep(std::time::Duration::from_millis(100));
                continue;
            }
            Err(_) => {
                return fail(&format!(
                    "could not write the kick file {}.",
                    kick_path.display()
                ));
            }
        }
    }
    if request_id.is_empty() {
        match crate::engines::subprocess::read_kick_id(&kick_path) {
            Some(id) => request_id = id,
            None => {
                return fail(&format!(
                    "could not write the kick file {}.",
                    kick_path.display()
                ))
            }
        }
    }
    // Wait up to 10 s for an acknowledgement that names OUR request id, polling every 200 ms.
    let ack_matches = |id: &str| {
        crate::engines::subprocess::read_kick_ack(&ack_path)
            .map(|a| a.id == id || id.is_empty())
            .unwrap_or(false)
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(kick_wait_ms());
    while !ack_matches(&request_id) && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    let ack = crate::engines::subprocess::read_kick_ack(&ack_path);
    let ack = match ack {
        Some(a) if a.id == request_id || request_id.is_empty() => a,
        _ => {
            // Exit 3: no acknowledgement in time. The kick file STAYS for the member's next poll (a
            // run that has ended leaves it to the next run of that number, which removes it).
            eprintln!(
                "{TOOL}: -Kick: the run with handoff {nn} did not acknowledge the kick within 10 s - the kick file stays ({}): the member takes it at its next poll; a run that has ended leaves it to the next run of that number, which removes it.",
                kick_path.display()
            );
            return 3;
        }
    };
    let mut code = 0;
    if ack.result == "late" {
        println!(
            "{TOOL}: -Kick: member {nn} of task {} had already finished - the kick is recorded as kick_late in its warnings, its outcome unchanged.",
            o.task
        );
    } else if ack.result == crate::engines::subprocess::KICK_NOT_KILLED {
        // (Wave 7c, F40-2) the member took the kick but its kill did not happen: the record naming
        // the tree could be written nowhere, so the tree is left alive as the evidence - no stop
        // is claimed and the kick fails.
        eprintln!("{TOOL}: -Kick: {}", not_killed_text(&nn, &o.task, &ack));
        code = 1;
    } else {
        println!(
            "{TOOL}: -Kick: member {nn} of task {} stopped - it took the kick; its engine's process tree is being stopped and its partial output salvaged; it records \"failed: stopped by the operator (-Kick)\" (a format repair only: its first reply stands) - the panel goes on with the others.",
            o.task
        );
    }
    // (wave 27c, D1) only the CREATOR of the request retires the acknowledgement, and only after a
    // 1 s grace + recheck (so a joiner has time to read it before it is swept); a joiner never does.
    if is_creator {
        std::thread::sleep(std::time::Duration::from_millis(1000));
        let still_ours = crate::engines::subprocess::read_kick_ack(&ack_path)
            .map(|a| a.id == request_id)
            .unwrap_or(false);
        if still_ours {
            let _ = std::fs::remove_file(&ack_path);
        }
    }
    code
}

/// (Wave 7c, F40-2) The `-Kick` report of a member that acknowledged `not killed`: the live root
/// pid and why, the tree left alive and the task refused - never a stop.
fn not_killed_text(nn: &str, task: &str, ack: &crate::engines::subprocess::KickAck) -> String {
    let root = if ack.root > 0 {
        format!("root pid {}", ack.root)
    } else {
        "its root".to_string()
    };
    let why = if ack.reason.is_empty() {
        "the kill record could not be written anywhere".to_string()
    } else {
        ack.reason.clone()
    };
    format!(
        "member {nn} of task {task} took the kick but was NOT stopped - not killed ({root}): {why}. Its engine's process tree ({root}) is left alive and the task is refused until it is gone or recorded; the member ends failed with the tree as the evidence."
    )
}

/// The recovery record that names the handoff `nn` among the task's records `listed`. (Wave 7c,
/// F40-1) A listing that failed is the error; (wave 7d, F42-1) so is ANY record that cannot be
/// inspected or is unusable ([`pending::read_pending_file`]'s error) - it may be the one naming
/// `nn`, so the kick refuses on it rather than report "no run in progress"; every record is read.
fn kick_record(
    listed: pending::PendingPaths,
    nn: &str,
) -> Result<Option<serde_json::Value>, String> {
    if let Some(err) = listed.error {
        return Err(err);
    }
    let mut record: Option<serde_json::Value> = None;
    for p in listed.paths {
        let rd = pending::read_pending_file(&p);
        if let Some(err) = rd.error {
            return Err(err);
        }
        if let Some(rec) = rd.record {
            if record.is_none() && rec.get("nn").and_then(|v| v.as_str()) == Some(nn) {
                record = Some(rec);
            }
        }
    }
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (Wave 7d, F42-1) The kick refuses on a record it cannot inspect (a directory where a record
    /// goes) or an unusable one, whether or not another record names the member; a record that is
    /// gone is no refusal.
    #[test]
    fn a_kick_refuses_on_a_record_it_cannot_inspect() {
        let dir = std::env::temp_dir().join(format!(
            "c3-kick-records-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mine = dir.join(".consult.pending-03.json");
        std::fs::write(&mine, r#"{"state":"running","nn":"03"}"#).unwrap();
        let other = dir.join(".consult.pending-04.json");
        let listed = |paths: Vec<std::path::PathBuf>| pending::PendingPaths { paths, error: None };
        let found = kick_record(listed(vec![mine.clone(), other.clone()]), "03").unwrap();
        assert_eq!(
            found.unwrap()["nn"],
            "03",
            "a record that is gone is no refusal"
        );
        std::fs::create_dir_all(&other).unwrap();
        let e = kick_record(listed(vec![mine.clone(), other.clone()]), "03").unwrap_err();
        assert!(
            e.starts_with(&format!(
                "the recovery record {} cannot be read (",
                other.display()
            )),
            "{e}"
        );
        std::fs::remove_dir_all(&other).unwrap();
        std::fs::write(&other, "{ not json").unwrap();
        let e = kick_record(listed(vec![other.clone(), mine.clone()]), "03").unwrap_err();
        assert!(e.contains("is unusable"), "{e}");
        let failed = pending::PendingPaths {
            paths: vec![mine],
            error: Some("the listing failed".into()),
        };
        assert_eq!(kick_record(failed, "03").unwrap_err(), "the listing failed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// (Wave 7c, F40-2) The report of a `not killed` acknowledgement names the live root and why,
    /// says the tree is left alive and the task refused, and claims no stop.
    #[test]
    fn a_not_killed_acknowledgement_is_reported_as_no_stop() {
        let ack = crate::engines::subprocess::KickAck {
            result: crate::engines::subprocess::KICK_NOT_KILLED.to_string(),
            id: "x".to_string(),
            root: 4242,
            reason: "the kill record could not be written anywhere (a; b; c): the reviewer tree (pid 4242) is left alive as the evidence; the task is refused until it is gone or recorded".to_string(),
        };
        let t = not_killed_text("03", "t", &ack);
        assert!(t.starts_with("member 03 of task t took the kick but was NOT stopped - not killed (root pid 4242): the kill record could not be written anywhere (a; b; c)"), "{t}");
        assert!(t.contains("Its engine's process tree (root pid 4242) is left alive and the task is refused until it is gone or recorded"), "{t}");
        assert!(!t.contains("is being stopped"), "{t}");
        let bare = crate::engines::subprocess::KickAck {
            result: crate::engines::subprocess::KICK_NOT_KILLED.to_string(),
            ..Default::default()
        };
        assert!(not_killed_text("03", "t", &bare)
            .contains("not killed (its root): the kill record could not be written anywhere."));
    }
}
