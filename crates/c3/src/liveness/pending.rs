//! Recovery records (`.consult.pending.json`, `.consult.pending-<NN>.json`), ported from
//! `Read-PendingFile` / `Read-TaskPendingRecords` / `Test-PendingActive` /
//! `Get-PendingOriginalNote` (`codex-consult-common.ps1`). A findings status change or a
//! rating is refused while any recovery record of the task is still active — an interrupted
//! consultation's bridge or codex process may still be running. `-List`/`-Stats` only read
//! these records (never modifying them) to describe every interrupted consultation.
//!
//! The liveness rules ported here are the writer rule (the bridge that wrote the record,
//! alive by pid + start time), `reserved` (inactive), and the recorded-pid rule (child_pid
//! and survivors, alive on this host). The machine-wide "looks like codex" descendant scan
//! the plugin falls through to for a `launching` record — or once every recorded pid is gone
//! — is reduced to an inactive verdict here (noted in the port status); the acceptance rows
//! that exercise a live recorded pid (F06-1, panel TIMEOUT) go through the recorded-pid rule.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::proc;

const PENDING_STATES: [&str; 5] = [
    "reserved",
    "launching",
    "running",
    "survivors",
    "committing",
];

/// Get-PropertyValue with a default (a present, non-null value, else the default).
fn pv<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.get(key).filter(|x| !x.is_null())
}

fn pv_str(v: &Value, key: &str, default: &str) -> String {
    match pv(v, key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => (if *b { "True" } else { "False" }).to_string(),
        Some(other) => other.to_string(),
        None => default.to_string(),
    }
}

fn pv_i64(v: &Value, key: &str) -> Option<i64> {
    match pv(v, key) {
        Some(Value::Number(n)) => n.as_i64(),
        Some(Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    }
}

/// The result of reading one recovery record.
pub struct PendingRead {
    pub exists: bool,
    pub record: Option<Value>,
    pub error: Option<String>,
}

/// The result of judging one record (`Test-PendingActive`).
pub struct PendingCheck {
    pub active: bool,
    /// The refusal message when active (empty when inactive).
    pub message: String,
    #[allow(dead_code)]
    pub check: String,
}

/// Every recovery record of a task, read and judged.
pub struct TaskPending {
    /// The first unusable record's refusal (corruption refuses); `None` when all parse.
    pub error: Option<String>,
    /// The first active record's refusal message; `None` when none is active.
    pub active_message: Option<String>,
}

/// `Get-PendingPaths`: `.consult.pending.json` first, then `.consult.pending-*.json` sorted
/// (ordinal, case-insensitive).
pub fn pending_paths(task_dir: &Path) -> Vec<PathBuf> {
    let mut single: Vec<PathBuf> = Vec::new();
    let mut members: Vec<PathBuf> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(task_dir) {
        for ent in rd.flatten() {
            let name = ent.file_name().to_string_lossy().to_string();
            if name.eq_ignore_ascii_case(".consult.pending.json") {
                single.push(ent.path());
            } else if name.starts_with(".consult.pending-")
                && name.ends_with(".json")
                && !name.contains(['\\', '/'])
            {
                members.push(ent.path());
            }
        }
    }
    members.sort_by_key(|p| {
        p.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase()
    });
    single.into_iter().chain(members).collect()
}

/// `Read-PendingFile`.
pub fn read_pending_file(path: &Path) -> PendingRead {
    if !path.exists() {
        return PendingRead {
            exists: false,
            record: None,
            error: None,
        };
    }
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let why: Option<String> = if text.trim().is_empty() {
        Some("it is empty or could not be read".to_string())
    } else {
        match serde_json::from_str::<Value>(&text) {
            Err(e) => Some(format!(
                "it does not parse: {}",
                c3_core::one_line(&e.to_string())
            )),
            Ok(v) => {
                if !v.is_object() {
                    Some("it is not a JSON object".to_string())
                } else {
                    let state = pv_str(&v, "state", "");
                    if !PENDING_STATES.contains(&state.as_str()) {
                        Some(format!(
                            "its state '{state}' is not one of {}",
                            PENDING_STATES.join("|")
                        ))
                    } else {
                        return PendingRead {
                            exists: true,
                            record: Some(v),
                            error: None,
                        };
                    }
                }
            }
        }
    };
    match why {
        Some(w) => PendingRead {
            exists: true,
            record: None,
            error: Some(format!(
                "the recovery record '{}' is unusable: {w}. It describes an interrupted consultation - inspect it (and any codex process it may name), then repair or delete it deliberately.",
                path.display()
            )),
        },
        None => PendingRead {
            exists: true,
            record: serde_json::from_str(&text).ok(),
            error: None,
        },
    }
}

/// `Read-TaskPendingRecords`: the first unusable record refuses; otherwise the first active
/// record's message is returned (a status change or rating is refused while it is active).
pub fn read_task_pending_records(task_dir: &Path) -> TaskPending {
    let mut active_message: Option<String> = None;
    for p in pending_paths(task_dir) {
        let rd = read_pending_file(&p);
        if let Some(err) = rd.error {
            return TaskPending {
                error: Some(err),
                active_message: None,
            };
        }
        if !rd.exists {
            continue;
        }
        if let Some(record) = rd.record {
            let check = test_pending_active(&record, &p);
            if check.active && active_message.is_none() {
                active_message = Some(check.message);
            }
        }
    }
    TaskPending {
        error: None,
        active_message,
    }
}

/// `Get-PendingOriginalNote`.
pub fn pending_original_note(record: &Value) -> String {
    let o = pv_str(record, "original", "");
    let ev = pv_str(record, "events", "");
    let rj = pv_str(record, "reply_json", "");
    let rr = pv_str(record, "raw_reply", "");
    let mut parts: Vec<String> = Vec::new();
    if !rj.is_empty() {
        parts.push(format!(
            "the reply of that run is kept at {rj} (its commit did not complete); no ledger entry was written for it"
        ));
    }
    if !rr.is_empty() {
        parts.push(format!("the raw last message of that run is kept at {rr}"));
    }
    if !o.is_empty() {
        parts.push(format!(
            "a usable prose reply of that run exists at {o}; no ledger entry was written for it"
        ));
    }
    if !ev.is_empty() {
        if !o.is_empty() || !rj.is_empty() {
            parts.push(format!(
                "the raw event stream of that run is at {ev} (it may hold a usable reply)"
            ));
        } else {
            parts.push(format!(
                "the raw event stream of that run is at {ev} (it may hold a usable reply); no ledger entry was written"
            ));
        }
    }
    parts.join("; ")
}

fn machine_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_default()
}

/// `Test-PendingActive` (see the module note for the reduced descendant scan).
pub fn test_pending_active(record: &Value, path: &Path) -> PendingCheck {
    let state = pv_str(record, "state", "");
    let rec_host = pv_str(record, "host", "");
    let other_host = !rec_host.is_empty() && !rec_host.eq_ignore_ascii_case(&machine_name());
    let mut what = format!(
        "state '{}', consult n={}, handoff {}, started {}",
        state,
        pv_str(record, "n", "?"),
        pv_str(record, "nn", "?"),
        pv_str(record, "started", "?")
    );
    let is_panel = pv(record, "panel").is_some();
    if let Some(panel) = pv(record, "panel") {
        let id = pv_str(panel, "id", "");
        let short: String = id.chars().take(8).collect();
        what += &format!(
            ", review panel {} member {}",
            short,
            pv_str(panel, "position", "?")
        );
    }
    let cli = {
        let e = pv_str(record, "engine", "");
        if e.is_empty() {
            "codex".to_string()
        } else {
            e
        }
    };

    // Recorded pids: child_pid, then survivors ({pid,start_time,name} or a bare pid).
    struct Pid {
        pid: u32,
        start: String,
    }
    let mut pids: Vec<Pid> = Vec::new();
    if let Some(cp) = pv_i64(record, "child_pid") {
        if cp > 0 {
            pids.push(Pid {
                pid: cp as u32,
                start: pv_str(record, "child_start_time", ""),
            });
        }
    }
    if let Some(Value::Array(survivors)) = pv(record, "survivors") {
        for s in survivors {
            if let Some(obj) = s.as_object() {
                if let Some(sp) = obj.get("pid").and_then(pv_num) {
                    if sp > 0 {
                        pids.push(Pid {
                            pid: sp as u32,
                            start: pv_str(s, "start_time", ""),
                        });
                    }
                }
            } else if let Some(sp) = s.as_i64() {
                if sp > 0 {
                    pids.push(Pid {
                        pid: sp as u32,
                        start: String::new(),
                    });
                }
            }
        }
    }

    let original_note = pending_original_note(record);
    let inactive = |c: String| PendingCheck {
        active: false,
        message: String::new(),
        check: if original_note.is_empty() {
            c
        } else {
            format!("{c}; {original_note}")
        },
    };
    let active = |m: String, c: String| PendingCheck {
        active: true,
        message: if original_note.is_empty() {
            m
        } else {
            format!("{}; {}.", m.trim_end_matches('.'), original_note)
        },
        check: c,
    };

    // D1: the bridge that wrote the record, while it runs — by pid + start time, never this
    // process, never across hosts.
    let writer_pid = pv_i64(record, "pid").unwrap_or(0);
    let writer_start = pv_str(record, "start_time", "");
    let mut writer_gone = String::new();
    if writer_pid > 0 && !writer_start.is_empty() && writer_pid as u32 != std::process::id() {
        if other_host {
            // cannot be checked from here; the rules below decide.
        } else if proc::pid_alive(writer_pid as u32, &writer_start) {
            return active(
                format!(
                    "a consultation of this task is still running: its bridge (pid {writer_pid}) wrote {} ({what}). Wait for it to finish; the record is removed when it commits.",
                    path.display()
                ),
                format!("writer pid {writer_pid} alive (pid + start time)"),
            );
        } else {
            writer_gone = format!("writer pid {writer_pid} gone");
        }
    }

    if state == "reserved" {
        let suffix = if writer_gone.is_empty() {
            String::new()
        } else {
            format!("; {writer_gone}")
        };
        return inactive(format!("reserved: {cli} was never started{suffix}"));
    }

    let mut recorded_gone = String::new();
    if !pids.is_empty() && state != "launching" {
        let pid_list = pids
            .iter()
            .map(|p| p.pid.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        if other_host {
            return active(
                format!(
                    "an interrupted consultation on host {rec_host} left {cli} process(es) pid {pid_list} ({what}); they cannot be checked from this host. Delete {} only after making sure they are gone.",
                    path.display()
                ),
                format!("pids on host {rec_host}"),
            );
        }
        let mut alive: Vec<String> = Vec::new();
        let mut gone: Vec<String> = Vec::new();
        let launcher = pv_str(record, "launcher", "");
        let mut seen: Vec<u32> = Vec::new();
        let table = proc::enumerate_processes();
        for p in &pids {
            // Dedup: the same pid can appear as both `child_pid` and a bare survivor.
            if seen.contains(&p.pid) {
                continue;
            }
            seen.push(p.pid);
            // `Test-RecordedProcess`: with a recorded start time, alive only when the pid runs with
            // it (a different start time is a reused pid). A bare pid (no start time) is judged by
            // the "looks like codex" rule (name / recorded launcher / command line), never by mere
            // existence — a reused pid running an unrelated process is not the recorded one.
            let live = if !p.start.is_empty() {
                proc::pid_alive(p.pid, &p.start)
            } else if let Some(pr) = table.iter().find(|x| x.pid == p.pid) {
                let cmd = proc::process_command_line(p.pid);
                !proc::codex_rule(&pr.name, &cmd, &launcher).is_empty()
            } else {
                false
            };
            if live {
                alive.push(p.pid.to_string());
            } else {
                gone.push(p.pid.to_string());
            }
        }
        if !alive.is_empty() {
            return active(
                format!(
                    "a previous consultation's {cli} process (pid {}) is still running ({what}). Wait for it to exit or stop it, then retry; {} keeps its record until then.",
                    alive.join(", "),
                    path.display()
                ),
                format!("pid {} alive", alive.join(", ")),
            );
        }
        recorded_gone = format!("{cli} pid(s) {} no longer running", gone.join(", "));
    }
    if !writer_gone.is_empty() {
        recorded_gone = if recorded_gone.is_empty() {
            writer_gone.clone()
        } else {
            format!("{writer_gone}; {recorded_gone}")
        };
    }

    if other_host {
        return inactive(format!(
            "state '{state}' from host {rec_host} without pids: treated as dead"
        ));
    }
    let on_windows = cfg!(windows);
    if is_panel && !on_windows {
        // Orphans are reparented outside Windows: only the recorded pids tell.
        let prefix = if recorded_gone.is_empty() {
            String::new()
        } else {
            format!("{recorded_gone}; ")
        };
        return inactive(format!(
            "{prefix}panel member record: judged by its recorded pids only (no process scan outside Windows)"
        ));
    }

    // The machine-wide "looks like codex" scan (`Find-CodexProcesses`): the child may exist
    // unregistered (launching) or as a descendant of a dead recorded process. READ-ONLY — it only
    // enumerates and compares, it never stops anything.
    let launcher = pv_str(record, "launcher", "");
    let started = pv_str(record, "started", "");
    let (since, since_text) = match chrono::DateTime::parse_from_rfc3339(&started) {
        Ok(dt) => (
            dt.with_timezone(&chrono::Utc),
            dt.with_timezone(&chrono::Local)
                .format("%Y-%m-%dT%H:%M:%S")
                .to_string(),
        ),
        Err(_) => {
            // No usable start: no age cut-off (everything counts), as the plugin's MinValue does.
            let epoch = chrono::DateTime::from_timestamp(0, 0).unwrap_or_default();
            (
                epoch,
                epoch
                    .with_timezone(&chrono::Local)
                    .format("%Y-%m-%dT%H:%M:%S")
                    .to_string(),
            )
        }
    };
    let self_pid = std::process::id();

    // Parent pids whose orphaned children would be ours: the bridge that wrote the record and every
    // recorded pid (the launcher shim, the survivors). On Windows an orphan keeps its dead parent's
    // pid; elsewhere only the command-line rule below can find them.
    let mut parent_pids: Vec<u32> = Vec::new();
    if writer_pid > 0 {
        parent_pids.push(writer_pid as u32);
    }
    for p in &pids {
        if p.pid > 0 && !parent_pids.contains(&p.pid) {
            parent_pids.push(p.pid);
        }
    }

    let all_procs = proc::enumerate_processes();
    let mut checks: Vec<String> = Vec::new();
    if !recorded_gone.is_empty() {
        checks.push(recorded_gone.clone());
    }
    let mut scan: Option<proc::ScanOutcome> = None;
    for parent in &parent_pids {
        let s = proc::find_codex_processes(
            &all_procs,
            since,
            &since_text,
            &launcher,
            *parent,
            self_pid,
            on_windows,
        );
        if !s.found.is_empty() {
            scan = Some(s);
            break;
        }
        checks.push(format!("{}: none found", s.check));
    }

    // A panel member's record stops here (recorded pids + their children only): the machine-wide
    // name rule would take a live sibling member's reviewer for its orphan.
    if is_panel && scan.as_ref().map(|s| s.found.is_empty()).unwrap_or(true) {
        checks.push("panel member record: no machine-wide name scan".to_string());
        return inactive(checks.join("; "));
    }

    // The parent-pid rule sees only direct children of a dead parent; a shim that died leaving its
    // own child alive is found only by the name rule (labelled "task not verifiable"). No age
    // cut-off.
    if scan.as_ref().map(|s| s.found.is_empty()).unwrap_or(true) {
        // Fetch command lines for the few recent candidates the name rules need (`Get-CodexRule`
        // over the command line), then run the name-branch scan.
        let mut procs = all_procs;
        for p in procs.iter_mut() {
            let recent = p.created.map(|c| c >= since).unwrap_or(false);
            if recent && proc::codex_rule(&p.name, "", &launcher).is_empty() {
                p.command_line = proc::process_command_line(p.pid);
            }
        }
        scan = Some(proc::find_codex_processes(
            &procs,
            since,
            &since_text,
            &launcher,
            0,
            self_pid,
            on_windows,
        ));
    }
    let mut scan = scan.expect("scan is set by the name-branch fallback");
    if !checks.is_empty() {
        scan.check = format!("{}; then {}", checks.join("; "), scan.check);
    }
    if !scan.found.is_empty() {
        let list = scan
            .found
            .iter()
            .map(|f| format!("pid {} {} [{}]", f.pid, f.name, f.rule))
            .collect::<Vec<_>>()
            .join(", ");
        return active(
            format!(
                "an interrupted consultation ({what}) may still have its {cli} process running: {list}, found by {}. Wait for it to exit or stop it, then retry (or delete {} once you know it is unrelated).",
                scan.check,
                path.display()
            ),
            scan.check.clone(),
        );
    }
    inactive(format!("{}: none found", scan.check))
}

fn pv_num(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}
