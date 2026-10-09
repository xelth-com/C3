//! Recovery records (`.consult.pending.json`, `.consult.pending-<NN>.json`), ported from
//! `Read-PendingFile` / `Read-TaskPendingRecords` / `Test-PendingActive` /
//! `Get-PendingOriginalNote` (`codex-consult-common.ps1`). A findings status change or a
//! rating is refused while any recovery record of the task is still active — an interrupted
//! consultation's bridge or codex process may still be running. `-List`/`-Stats` only read
//! these records (never modifying them) to describe every interrupted consultation.
//!
//! The liveness rules ported here are the writer rule (the bridge that wrote the record,
//! alive by pid + start time), `reserved` (inactive), the recorded-pid rule (child_pid and
//! survivors, `Test-RecordedProcess`) and - 0.6.0 wave 28e - the descendants a kill could not
//! verify (`unverified[]`, `Test-UnverifiedProcess`, fail-closed on the evidence: E1, E19), the
//! unknown tree of a kill that was not confirmed (`kill_unconfirmed`: released only after a clean
//! scan by parent pid AND a clean machine-wide scan, E23, E25), and the descendant scan
//! (`Find-CodexProcesses`) by parent pid and by the machine-wide "looks like codex" rule.

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
    c3_core::host::machine_name()
}

/// A re-check of one recorded pid (`Test-UnverifiedProcess` / `Test-RecordedProcess`):
/// `{ Alive; How }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub alive: bool,
    pub how: String,
}

impl Verdict {
    fn new(alive: bool, how: impl Into<String>) -> Verdict {
        Verdict {
            alive,
            how: how.into(),
        }
    }
}

/// (wave 28e, E1 / F54-1; E19 / F27-2) `Test-UnverifiedProcess`: a pid of the record's
/// `unverified[]` - a descendant whose start time could not be read at the kill - checked again,
/// FAIL-CLOSED on the evidence, in this order:
///   no such process - dropped (`gone`);
///   its start time STILL cannot be read - running (it blocks the task as a survivor does);
///   started before that run (`since`, the record's `started`) - dropped (not its process);
///   (wave 3c, F23-1) it exists but its name and parent cannot be read (a failed or incomplete
///     process-table lookup: [`proc::ProcessInfo::inspected`]) - running (`... cannot be read -
///     counted as running (fail-closed)`), never `gone`;
///   the "looks like codex" rule matches - running;
///   its command line cannot be read (`""` - access denied -, ps's `[name]`, or a generic runtime
///     such as node or powershell with nothing after the executable) - running (`command line not
///     readable - counted as running (fail-closed)`);
///   a child of a recorded pid (`recorded_pids`: the writer, the child, the survivors, the other
///     unverified pids) - running;
///   else - its command line was read and is not codex-like - dropped (`not codex`).
/// Never by pid alone. The same rule re-checks a survivor recorded without a start time, or whose
/// start time cannot be read now ([`test_recorded_process`]).
pub fn test_unverified_process(
    pid: u32,
    since: Option<chrono::DateTime<chrono::Utc>>,
    launcher: &str,
    recorded_pids: &[u32],
) -> Verdict {
    let Some(st) = proc::process_start_iso(pid) else {
        return Verdict::new(false, "gone");
    };
    if st.is_empty() {
        return Verdict::new(
            true,
            "its start time still cannot be read - counted as running (fail-closed)",
        );
    }
    if let Some(since) = since {
        if let Ok(at) = chrono::DateTime::parse_from_rfc3339(&st) {
            if at.with_timezone(&chrono::Utc) < since {
                return Verdict::new(
                    false,
                    format!("started {st}, before that run: not its process"),
                );
            }
        }
    }
    let Some(info) = proc::process_info(pid) else {
        return Verdict::new(false, "gone");
    };
    if !info.inspected {
        return Verdict::new(true, uninspected_how(pid));
    }
    // (wave 29, E27) a codex-named server or helper of the Codex app is not codex-like; it still has
    // to pass the command-line and parent checks below, and its drop says what it was taken for
    let m = proc::codex_match(&info.name, &info.cmd, launcher);
    if !m.rule.is_empty() {
        return Verdict::new(true, format!("start time readable now; {}", m.rule));
    }
    let gap = proc::command_line_gap(&info.name, &info.cmd);
    if !gap.is_empty() {
        let generic = if gap == "no arguments" {
            " (a generic runtime, no arguments on its command line)"
        } else {
            ""
        };
        return Verdict::new(
            true,
            format!(
                "start time readable now; pid {pid} runs {}{generic}; command line not readable - counted as running (fail-closed)",
                info.name
            ),
        );
    }
    if info.ppid > 0 && info.ppid != pid && recorded_pids.contains(&info.ppid) {
        return Verdict::new(
            true,
            format!(
                "start time readable now; pid {pid} runs {}, a child of the recorded pid {} - counted as running",
                info.name, info.ppid
            ),
        );
    }
    let excluded = if m.excluded.is_empty() {
        String::new()
    } else {
        format!(" (excluded: {})", m.excluded)
    };
    Verdict::new(
        false,
        format!(
            "start time readable now; pid {pid} runs {}, not codex{excluded}",
            info.name
        ),
    )
}

/// (wave 3c, F23-1) The verdict's why for a process that exists but whose name and parent cannot be
/// read.
fn uninspected_how(pid: u32) -> String {
    format!(
        "start time readable now; pid {pid} runs a process whose name and parent cannot be read - counted as running (fail-closed)"
    )
}

/// `Test-RecordedProcess`: is a process recorded in a pending record still that process? With a
/// recorded start time that can be read now too: alive only when the pid runs with that start time
/// (and that name, when one is recorded) - another start time means the pid was reused. Without
/// one (a bare pid, or `start_time` empty), or when the live start time cannot be read: (wave 28e,
/// E19 / F27-2) the same fail-closed evidence rule and messages as an unverified pid
/// ([`test_unverified_process`]). (wave 3c, F23-1) Only a pid that no longer answers is `gone`; one
/// whose name and parent cannot be read keeps its recorded start time's verdict (a name it cannot
/// show is never "reused"), else the evidence rule counts it as running.
pub fn test_recorded_process(
    pid: u32,
    start_time: &str,
    name: &str,
    launcher: &str,
    since: Option<chrono::DateTime<chrono::Utc>>,
    recorded_pids: &[u32],
) -> Verdict {
    let Some(info) = proc::process_info(pid) else {
        return Verdict::new(false, "gone");
    };
    if !start_time.is_empty() && !info.start.is_empty() {
        if !proc::same_start_time(&info.start, start_time) {
            return Verdict::new(false, "pid reused (start time differs)");
        }
        if !name.is_empty() && info.inspected && !info.name.eq_ignore_ascii_case(name) {
            return Verdict::new(false, format!("pid reused (now {})", info.name));
        }
        return Verdict::new(true, "pid + start time");
    }
    test_unverified_process(pid, since, launcher, recorded_pids)
}

/// What the re-check reads from this machine's process table (wave 3c, F23-4: a seam, so a fixture
/// can hand [`test_pending_active_with`] a table of its own): the whole table, and one process's
/// command line.
pub struct ProcessTable<'a> {
    /// The table (`Err` - the scan failed).
    pub read: &'a dyn Fn() -> Result<Vec<proc::ScanProc>, String>,
    /// One process's command line (`""` when it cannot be read).
    pub command_line: &'a dyn Fn(u32) -> String,
}

impl ProcessTable<'static> {
    /// This machine's table ([`proc::enumerate_processes_checked`], [`proc::process_command_line`]).
    pub fn live() -> ProcessTable<'static> {
        ProcessTable {
            read: &proc::enumerate_processes_checked,
            command_line: &proc::process_command_line,
        }
    }
}

/// One pass of `Find-CodexProcesses` over the table read for this check: by parent pid (a
/// `parent` above 0, on Windows) or the machine-wide "looks like codex" rule (`parent` 0: the command lines of the
/// recent candidates are read first - (wave 29, E27) a codex-named process too; TEST HOOK (test
/// mode only): `CODEX_CONSULT_TEST_CMDLINE_UNREADABLE=<pid>[,<pid>]` - these pids are scanned with
/// the command line `''`). A table that could not be read: `failed`, `<check> could not run`.
/// (wave 3c, F23-2) A row whose start time could not be read is a recent candidate too (its command
/// line is read; [`proc::find_codex_processes`] counts it).
fn scan_pass(
    table: &Result<Vec<proc::ScanProc>, String>,
    since: chrono::DateTime<chrono::Utc>,
    since_text: &str,
    launcher: &str,
    parent: u32,
    on_windows: bool,
    command_line: &dyn Fn(u32) -> String,
) -> proc::ScanOutcome {
    let self_pid = std::process::id();
    let procs = match table {
        Ok(t) => t,
        Err(e) => {
            let mut s = proc::find_codex_processes(
                &[],
                since,
                since_text,
                launcher,
                parent,
                self_pid,
                on_windows,
            );
            s.check = format!("{} could not run: {e}", s.check);
            s.failed = true;
            return s;
        }
    };
    if parent > 0 && on_windows {
        return proc::find_codex_processes(
            procs, since, since_text, launcher, parent, self_pid, on_windows,
        );
    }
    let unreadable = proc::pid_list_hook("CODEX_CONSULT_TEST_CMDLINE_UNREADABLE");
    let mut procs = procs.clone();
    for p in procs.iter_mut() {
        if p.created.map(|c| c >= since).unwrap_or(true) {
            p.command_line = if unreadable.contains(&p.pid) {
                String::new()
            } else {
                command_line(p.pid)
            };
        }
    }
    proc::find_codex_processes(
        &procs, since, since_text, launcher, parent, self_pid, on_windows,
    )
}

/// `Test-PendingActive`: whether a recovery record still belongs to a live consultation. The rules,
/// in this order:
///   1. THE WRITER (wave 21, D1) decides first: a record that names the bridge that wrote it (pid +
///      start time) is active while that process runs on this host, in every state. Never by pid
///      alone, never this very process, not across hosts.
///   2. When the writer is gone or unknown, the state decides: `reserved` - inactive; `running`,
///      `survivors`, `committing` - active while `child_pid` (with its start time) or any survivor
///      is alive ([`test_recorded_process`]) or - (wave 28e, E1) - an unverified pid
///      ([`test_unverified_process`]); every one is named; pids from another host: active; all
///      recorded pids gone: the process scan below; `launching` - the scan below.
///      (wave 28e, E23 / F30-1; E25 / F32-1) a record with `kill_unconfirmed` (a kill that was not
///      confirmed and named no pid): its tree is unknown - released only by a clean scan by parent
///      pid (the writer, the child, every other recorded pid) AND the machine-wide rule, a panel
///      member's record too (a live sibling's reviewer merely postpones its release); a failed
///      scan, a find, a host outside Windows or another host: active.
///   3. The process scan: children of the recorded writer and pids (Windows keeps an orphan's
///      parent id), then - outside a panel only - the machine-wide "looks like codex" rule
///      (`Find-CodexProcesses`, "task not verifiable"). READ-ONLY: it never stops anything.
///      (wave 3c, F23-2) A process whose start time cannot be read is never skipped by a scan.
pub fn test_pending_active(record: &Value, path: &Path) -> PendingCheck {
    test_pending_active_with(record, path, &ProcessTable::live())
}

/// [`test_pending_active`] with the scans' process table given (wave 3c, F23-4: the fixtures'
/// seam; the recorded pids are still checked on this machine).
pub fn test_pending_active_with(
    record: &Value,
    path: &Path,
    machine: &ProcessTable<'_>,
) -> PendingCheck {
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
    let launcher = pv_str(record, "launcher", "");
    let started = pv_str(record, "started", "");

    // Recorded pids: child_pid, then survivors ({pid,start_time,name} or a bare pid).
    struct Pid {
        pid: u32,
        start: String,
        name: String,
    }
    let mut pids: Vec<Pid> = Vec::new();
    if let Some(cp) = pv_i64(record, "child_pid") {
        if cp > 0 {
            pids.push(Pid {
                pid: cp as u32,
                start: pv_str(record, "child_start_time", ""),
                name: String::new(),
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
                            name: pv_str(s, "name", ""),
                        });
                    }
                }
            } else if let Some(sp) = pv_num(s) {
                if sp > 0 {
                    pids.push(Pid {
                        pid: sp as u32,
                        start: String::new(),
                        name: String::new(),
                    });
                }
            }
        }
    }
    // (wave 28e, E1 / F54-1) unverified: {pid, why} entries - descendants whose start time could
    // not be read at the kill; reported and checked again below
    struct Unverified {
        pid: u32,
        why: String,
    }
    let mut unverified: Vec<Unverified> = Vec::new();
    if let Some(Value::Array(list)) = pv(record, "unverified") {
        for u in list {
            if let Some(obj) = u.as_object() {
                if let Some(up) = obj.get("pid").and_then(pv_num) {
                    if up > 0 {
                        unverified.push(Unverified {
                            pid: up as u32,
                            why: pv_str(u, "why", ""),
                        });
                    }
                }
            } else if let Some(up) = pv_num(u) {
                if up > 0 {
                    unverified.push(Unverified {
                        pid: up as u32,
                        why: String::new(),
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

    // The record's start, for every re-check by the evidence rule and every scan: a process that
    // started before the record is not its.
    let started_at = chrono::DateTime::parse_from_rfc3339(&started)
        .ok()
        .map(|d| d.with_timezone(&chrono::Utc));

    let mut recorded_gone = String::new();
    if (!pids.is_empty() || !unverified.is_empty()) && state != "launching" {
        if other_host {
            let pid_list = pids
                .iter()
                .map(|p| p.pid)
                .chain(unverified.iter().map(|u| u.pid))
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            return active(
                format!(
                    "an interrupted consultation on host {rec_host} left {cli} process(es) pid {pid_list} ({what}); they cannot be checked from this host. Delete {} only after making sure they are gone.",
                    path.display()
                ),
                format!("pids on host {rec_host}"),
            );
        }
        // (wave 28e, E19 / F27-2) the record's pids, for every re-check by the evidence rule: a
        // child of a recorded pid is never dropped
        let mut recorded_pids: Vec<u32> = Vec::new();
        for p in std::iter::once(writer_pid.max(0) as u32)
            .chain(pids.iter().map(|p| p.pid))
            .chain(unverified.iter().map(|u| u.pid))
        {
            if p > 0 && !recorded_pids.contains(&p) {
                recorded_pids.push(p);
            }
        }
        let mut alive: Vec<String> = Vec::new();
        let mut alive_how: Vec<String> = Vec::new();
        let mut gone: Vec<String> = Vec::new();
        let mut seen: Vec<u32> = Vec::new();
        for p in &pids {
            // Dedup: the same pid can appear as both `child_pid` and a bare survivor.
            if seen.contains(&p.pid) {
                continue;
            }
            seen.push(p.pid);
            let v = test_recorded_process(
                p.pid,
                &p.start,
                &p.name,
                &launcher,
                started_at,
                &recorded_pids,
            );
            // (E19) a pid judged by the evidence rule (not by its pid + start time) says why
            if v.alive {
                alive.push(if v.how == "pid + start time" {
                    p.pid.to_string()
                } else {
                    format!("{} [{}]", p.pid, v.how)
                });
                alive_how.push(format!("{} [{}]", p.pid, v.how));
            } else {
                gone.push(format!("{} [{}]", p.pid, v.how));
            }
        }
        // (wave 28e, E1 / F54-1) the descendants the kill could not verify, checked again now: one
        // still unreadable, or readable and codex-like, blocks like a survivor; a gone one is
        // dropped - said
        let mut u_alive: Vec<String> = Vec::new();
        let mut u_gone: Vec<String> = Vec::new();
        for u in &unverified {
            let v = test_unverified_process(u.pid, started_at, &launcher, &recorded_pids);
            if v.alive {
                let at_kill = if u.why.is_empty() {
                    String::new()
                } else {
                    format!("; at the kill: {}", u.why)
                };
                u_alive.push(format!("{} [{}{at_kill}]", u.pid, v.how));
            } else {
                u_gone.push(format!("{} [{}]", u.pid, v.how));
            }
        }
        let u_dropped = if u_gone.is_empty() {
            String::new()
        } else {
            format!("unverified pid(s) {} dropped", u_gone.join(", "))
        };
        if !alive.is_empty() || !u_alive.is_empty() {
            let mut parts: Vec<String> = Vec::new();
            let mut check_parts: Vec<String> = Vec::new();
            if !alive.is_empty() {
                parts.push(format!(
                    "a previous consultation's {cli} process (pid {}) is still running ({what})",
                    alive.join(", ")
                ));
                check_parts.push(format!("pid {} alive", alive_how.join(", ")));
            }
            if !u_alive.is_empty() {
                parts.push(if !alive.is_empty() {
                    format!(
                        "a process its kill could not verify blocks too: pid {}",
                        u_alive.join(", ")
                    )
                } else {
                    format!(
                        "a previous consultation's {cli} run left a process its kill could not verify ({what}): pid {} - it blocks the task as a survivor does",
                        u_alive.join(", ")
                    )
                });
                check_parts.push(format!(
                    "unverified pid {} counted as running",
                    u_alive.join(", ")
                ));
            }
            if !u_dropped.is_empty() {
                parts.push(u_dropped.clone());
                check_parts.push(u_dropped.clone());
            }
            return active(
                format!(
                    "{}. Wait for it to exit or stop it, then retry; {} keeps its record until then.",
                    parts.join("; "),
                    path.display()
                ),
                check_parts.join("; "),
            );
        }
        // Every RECORDED pid is gone - but a dead launcher is no proof of a dead tree: the
        // descendant scan below decides (F04-10).
        let mut gone_parts: Vec<String> = Vec::new();
        if !gone.is_empty() {
            gone_parts.push(format!(
                "{cli} pid(s) {} no longer running",
                gone.join(", ")
            ));
        }
        if !u_dropped.is_empty() {
            gone_parts.push(u_dropped);
        }
        recorded_gone = gone_parts.join("; ");
    }
    if !writer_gone.is_empty() {
        recorded_gone = if recorded_gone.is_empty() {
            writer_gone.clone()
        } else {
            format!("{writer_gone}; {recorded_gone}")
        };
    }

    let on_windows = cfg!(windows);
    // The scans' start: `started` (no usable start: no age cut-off - everything counts, as the
    // plugin's MinValue does), and its local stamp for the messages.
    let since =
        started_at.unwrap_or_else(|| chrono::DateTime::from_timestamp(0, 0).unwrap_or_default());
    let since_text = since
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    // Parent pids whose (orphaned) children would be ours: the bridge that wrote the record, every
    // recorded pid (the launcher shim, the survivors) and (wave 28e, E1) every unverified pid. On
    // Windows an orphan keeps its dead parent's pid; elsewhere only the command-line rule finds it.
    let mut parent_pids: Vec<u32> = Vec::new();
    if writer_pid > 0 {
        parent_pids.push(writer_pid as u32);
    }
    for p in pids
        .iter()
        .map(|p| p.pid)
        .chain(unverified.iter().map(|u| u.pid))
    {
        if p > 0 && !parent_pids.contains(&p) {
            parent_pids.push(p);
        }
    }

    // (wave 28e, E23 / F30-1; E25 / F32-1) a kill that was not confirmed and named no pid left an
    // UNKNOWN tree: only a clean scan by parent pid AND a clean machine-wide check release it.
    let kill_unconfirmed = pv_str(record, "kill_unconfirmed", "");
    if !kill_unconfirmed.is_empty() {
        let k_why = format!("the kill of its {cli} run was not confirmed ({kill_unconfirmed})");
        let k_release = format!(
            "Make sure no {cli} process of that run still runs, then delete {} to release it.",
            path.display()
        );
        if other_host {
            return active(
                format!(
                    "an interrupted consultation on host {rec_host} ({what}) left an UNKNOWN process tree - {k_why}; it cannot be checked from this host. {k_release}"
                ),
                format!("unknown tree after an unconfirmed kill on host {rec_host}"),
            );
        }
        if !on_windows {
            return active(
                format!(
                    "an interrupted consultation ({what}) left an UNKNOWN process tree - {k_why} - and this host cannot scan for its processes by parent pid (outside Windows an orphan is reparented). {k_release}"
                ),
                format!(
                    "unknown tree after an unconfirmed kill ({kill_unconfirmed}): no scan by parent pid outside Windows - released only by the operator"
                ),
            );
        }
        if parent_pids.is_empty() {
            return active(
                format!(
                    "an interrupted consultation ({what}) left an UNKNOWN process tree - {k_why} - and the record names no pid to scan under. {k_release}"
                ),
                "unknown tree after an unconfirmed kill: no recorded pid to scan under".to_string(),
            );
        }
        let table = (machine.read)();
        let mut k_checks: Vec<String> = Vec::new();
        if !recorded_gone.is_empty() {
            k_checks.push(recorded_gone.clone());
        }
        // by parent pid, then the machine-wide rule for EVERY record (a panel member's too): a
        // grandchild whose own parent died is invisible by parent
        for parent in parent_pids.iter().copied().chain(std::iter::once(0)) {
            let s = scan_pass(
                &table,
                since,
                &since_text,
                &launcher,
                parent,
                on_windows,
                machine.command_line,
            );
            if s.failed {
                return active(
                    format!(
                        "an interrupted consultation ({what}) left an UNKNOWN process tree - {k_why} - and the scan for its processes failed: {}. {k_release}",
                        s.check
                    ),
                    format!("unknown tree after an unconfirmed kill; {}", s.check),
                );
            }
            if !s.found.is_empty() {
                if is_panel && parent == 0 {
                    // (E25) the machine-wide rule cannot tell a sibling member's reviewer from this
                    // member's orphan: either postpones the release
                    let list = s
                        .found
                        .iter()
                        .map(|f| format!("pid {} {} (task not verifiable)", f.pid, f.name))
                        .collect::<Vec<_>>()
                        .join(", ");
                    return active(
                        format!(
                            "an interrupted consultation ({what}) left an UNKNOWN process tree - {k_why} - and a codex-like process runs: {list} - this panel member's unknown tree is released only when no such process runs. Wait for it to exit or stop it, then retry (or delete {} once you know it is unrelated).",
                            path.display()
                        ),
                        format!("unknown tree after an unconfirmed kill; {}", s.check),
                    );
                }
                let list = s
                    .found
                    .iter()
                    .map(|f| format!("pid {} {} [{}]", f.pid, f.name, f.rule))
                    .collect::<Vec<_>>()
                    .join(", ");
                return active(
                    format!(
                        "an interrupted consultation ({what}) left an UNKNOWN process tree - {k_why} - and a {cli}-like process of it may still run: {list}, found by {}. Wait for it to exit or stop it, then retry (or delete {} once you know it is unrelated).",
                        s.check,
                        path.display()
                    ),
                    format!("unknown tree after an unconfirmed kill; {}", s.check),
                );
            }
            k_checks.push(format!("{}: none found", s.check));
        }
        return inactive(format!(
            "unknown tree after an unconfirmed kill: the scan found no codex-like process under pid {} since {started} - released ({})",
            parent_pids
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            k_checks.join("; ")
        ));
    }

    // The child may exist unregistered (launching) or as a descendant of a dead recorded process
    // (running/survivors). Scan for it; never trust a dead root or elapsed time.
    if other_host {
        return inactive(format!(
            "state '{state}' from host {rec_host} without pids: treated as dead"
        ));
    }
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

    let table = (machine.read)();
    let mut checks: Vec<String> = Vec::new();
    if !recorded_gone.is_empty() {
        checks.push(recorded_gone.clone());
    }
    let mut scan: Option<proc::ScanOutcome> = None;
    for parent in &parent_pids {
        let s = scan_pass(
            &table,
            since,
            &since_text,
            &launcher,
            *parent,
            on_windows,
            machine.command_line,
        );
        if s.failed || !s.found.is_empty() {
            scan = Some(s);
            break;
        }
        checks.push(format!("{}: none found", s.check));
    }
    let clean = |s: &Option<proc::ScanOutcome>| {
        s.as_ref()
            .map(|s| !s.failed && s.found.is_empty())
            .unwrap_or(true)
    };

    // A panel member's record stops here (recorded pids + their children only): the machine-wide
    // name rule would take a live sibling member's reviewer for its orphan.
    if is_panel && clean(&scan) {
        checks.push("panel member record: no machine-wide name scan".to_string());
        return inactive(checks.join("; "));
    }

    // The parent-pid rule sees only direct children of a dead parent; a shim that died leaving its
    // own child alive is found only by the name rule (labelled "task not verifiable"). No age
    // cut-off.
    if clean(&scan) {
        scan = Some(scan_pass(
            &table,
            since,
            &since_text,
            &launcher,
            0,
            on_windows,
            machine.command_line,
        ));
    }
    let mut scan = scan.expect("scan is set by the name-branch fallback");
    if !checks.is_empty() {
        scan.check = format!("{}; then {}", checks.join("; "), scan.check);
    }
    if scan.failed {
        return active(
            format!(
                "an interrupted consultation ({what}) may have left a {cli} process running, and the check failed: {}. Make sure no such process runs, then delete {}.",
                scan.check,
                path.display()
            ),
            scan.check.clone(),
        );
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

#[cfg(test)]
mod tests {
    //! (wave 28e, E1 / E19 / E23) The re-check of a record's pids and the unknown tree, against the
    //! plugin's `Test-UnverifiedProcess` / `Test-RecordedProcess` / `Test-PendingActive` (the
    //! harness-fixes28e RECORD samples that need no test hook; the hooked ones run in
    //! `tests/recovery_wave3a.rs`).
    use super::*;

    fn me() -> u32 {
        std::process::id()
    }

    fn exe() -> String {
        std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .to_string()
    }

    fn at(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
        Some(
            chrono::DateTime::parse_from_rfc3339(s)
                .unwrap()
                .with_timezone(&chrono::Utc),
        )
    }

    fn host() -> String {
        machine_name()
    }

    const NOWHERE: &str = r"C:\nowhere\codex.cmd";

    #[test]
    fn unverified_recheck_follows_the_evidence_order() {
        let past = at("2020-01-01T00:00:00+00:00");
        // gone
        assert_eq!(
            test_unverified_process(999_999, past, "", &[]),
            Verdict::new(false, "gone")
        );
        // readable, but started before that run: not its process
        let future = Some(chrono::Utc::now() + chrono::Duration::hours(1));
        let v = test_unverified_process(me(), future, &exe(), &[]);
        assert!(
            !v.alive
                && v.how.starts_with("started ")
                && v.how.ends_with(", before that run: not its process"),
            "{v:?}"
        );
        // readable and codex-like (the recorded launcher is this process's executable)
        let v = test_unverified_process(me(), past, &exe(), &[]);
        assert!(
            v.alive && v.how.starts_with("start time readable now; "),
            "{v:?}"
        );
        let info = proc::process_info(me()).expect("this process");
        #[cfg(windows)]
        assert_eq!(
            v.how,
            format!(
                "start time readable now; name {} (the recorded launcher)",
                info.name
            )
        );
        // its command line read and not codex-like, not a child of a recorded pid: dropped
        let v = test_unverified_process(me(), past, NOWHERE, &[999_998]);
        assert_eq!(
            v,
            Verdict::new(
                false,
                format!(
                    "start time readable now; pid {} runs {}, not codex",
                    me(),
                    info.name
                )
            )
        );
        // a child of a recorded pid: counted as running
        if info.ppid > 0 {
            let v = test_unverified_process(me(), past, NOWHERE, &[999_998, info.ppid]);
            assert_eq!(
                v,
                Verdict::new(
                    true,
                    format!(
                        "start time readable now; pid {} runs {}, a child of the recorded pid {} - counted as running",
                        me(),
                        info.name,
                        info.ppid
                    )
                )
            );
        }
    }

    #[test]
    fn a_recorded_survivor_is_judged_by_its_start_time_else_by_the_evidence() {
        let info = proc::process_info(me()).expect("this process");
        let past = at("2020-01-01T00:00:00+00:00");
        assert_eq!(
            test_recorded_process(me(), &info.start, "", NOWHERE, past, &[]),
            Verdict::new(true, "pid + start time")
        );
        assert_eq!(
            test_recorded_process(me(), &info.start, &info.name, NOWHERE, past, &[]),
            Verdict::new(true, "pid + start time")
        );
        assert_eq!(
            test_recorded_process(me(), &info.start, "someone-else", NOWHERE, past, &[]),
            Verdict::new(false, format!("pid reused (now {})", info.name))
        );
        if cfg!(windows) {
            assert_eq!(
                test_recorded_process(me(), "2020-01-01T00:00:00.0000000Z", "", NOWHERE, past, &[]),
                Verdict::new(false, "pid reused (start time differs)")
            );
        }
        // (E19) no start time recorded: the evidence rule, never by pid alone
        assert_eq!(
            test_recorded_process(me(), "", "", NOWHERE, past, &[]),
            test_unverified_process(me(), past, NOWHERE, &[])
        );
        assert_eq!(
            test_recorded_process(999_999, "", "", NOWHERE, past, &[]),
            Verdict::new(false, "gone")
        );
    }

    fn record(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn an_unverified_pid_that_is_gone_is_dropped_and_named() {
        let started = chrono::Local::now()
            .format("%Y-%m-%dT%H:%M:%S%:z")
            .to_string();
        let r = record(&format!(
            r#"{{"state":"survivors","n":4,"nn":"05","reply":"r","started":"{started}","pid":999998,"host":"{}","launcher":"{}","engine":"codex","child_pid":null,"child_start_time":"","survivors":[],"unverified":[{{"pid":999999,"why":"start time of pid 999999 unreadable"}}],"note":""}}"#,
            host(),
            NOWHERE.replace('\\', "\\\\")
        ));
        let c = test_pending_active(&r, Path::new("x.json"));
        assert!(
            c.check
                .starts_with("unverified pid(s) 999999 [gone] dropped; "),
            "{} | {}",
            c.message,
            c.check
        );
    }

    #[test]
    fn a_live_codex_like_unverified_pid_blocks_like_a_survivor() {
        let r = record(&format!(
            r#"{{"state":"survivors","n":2,"nn":"03","reply":"r","started":"2020-01-01T00:00:00+00:00","pid":999998,"host":"{}","launcher":"{}","engine":"codex","child_pid":null,"child_start_time":"","survivors":[],"unverified":[{{"pid":{},"why":"start time of pid {} unreadable"}}],"note":""}}"#,
            host(),
            exe().replace('\\', "\\\\"),
            me(),
            me()
        ));
        let c = test_pending_active(&r, Path::new("x.json"));
        assert!(c.active, "{}", c.check);
        assert!(
            c.message.contains(
                "a previous consultation's codex run left a process its kill could not verify (state 'survivors', consult n=2, handoff 03, started 2020-01-01T00:00:00+00:00): pid "
            ),
            "{}",
            c.message
        );
        assert!(
            c.message.contains(&format!(
                "; at the kill: start time of pid {} unreadable] - it blocks the task as a survivor does. Wait for it to exit or stop it, then retry; x.json keeps its record until then.",
                me()
            )),
            "{}",
            c.message
        );
    }

    #[test]
    fn another_hosts_unverified_pids_and_unknown_tree_cannot_be_checked() {
        let r = record(
            r#"{"state":"survivors","n":1,"nn":"02","reply":"r","started":"2026-10-09T10:00:00+02:00","pid":999998,"host":"OTHER-HOST-28E","launcher":"","child_pid":null,"child_start_time":"","survivors":[],"unverified":[{"pid":4242,"why":"start time of pid 4242 unreadable"}],"note":""}"#,
        );
        let c = test_pending_active(&r, Path::new("x.json"));
        assert!(c.active);
        assert!(
            c.message.starts_with("an interrupted consultation on host OTHER-HOST-28E left codex process(es) pid 4242 (")
                && c.message.contains("they cannot be checked from this host"),
            "{}",
            c.message
        );
        let r = record(
            r#"{"state":"survivors","n":1,"nn":"02","reply":"r","started":"2026-10-09T10:00:00+02:00","pid":999998,"host":"OTHER-HOST-28E","launcher":"","child_pid":null,"child_start_time":"","survivors":[],"unverified":[],"kill_unconfirmed":"the children could not be enumerated (test)","note":""}"#,
        );
        let c = test_pending_active(&r, Path::new("x.json"));
        assert!(c.active);
        assert!(
            c.message.starts_with("an interrupted consultation on host OTHER-HOST-28E (")
                && c.message.contains(") left an UNKNOWN process tree - the kill of its codex run was not confirmed (the children could not be enumerated (test)); it cannot be checked from this host. Make sure no codex process of that run still runs, then delete x.json to release it."),
            "{}",
            c.message
        );
    }

    /// (wave 3c, F23-4) The record of an unconfirmed kill whose writer (pid 999998, never a Windows
    /// pid: not a multiple of 4) is gone, started at `STARTED`.
    const STARTED: &str = "2026-10-09T10:00:00+02:00";

    fn unknown_tree_record() -> Value {
        record(&format!(
            r#"{{"state":"survivors","n":1,"nn":"01","reply":"r","started":"{STARTED}","pid":999998,"host":"{}","launcher":"","engine":"codex","child_pid":null,"child_start_time":"","survivors":[],"unverified":[],"kill_unconfirmed":"the children could not be enumerated (test)","note":""}}"#,
            host()
        ))
    }

    /// A row of a synthetic process table: `created` minutes after `STARTED` (`None`: unreadable).
    fn row(pid: u32, ppid: u32, name: &str, created: Option<i64>) -> proc::ScanProc {
        let since = at(STARTED).unwrap();
        proc::ScanProc {
            pid,
            ppid,
            name: name.to_string(),
            created: created.map(|m| since + chrono::Duration::minutes(m)),
            command_line: String::new(),
        }
    }

    /// `test_pending_active_with` over a synthetic table; the command lines come from `cmds`, and
    /// every pid whose command line the scans read is collected.
    fn check_on(
        r: &Value,
        table: Result<Vec<proc::ScanProc>, String>,
        cmds: &[(u32, &str)],
    ) -> (PendingCheck, usize, Vec<u32>) {
        let reads = std::cell::Cell::new(0usize);
        let asked = std::cell::RefCell::new(Vec::new());
        let read = || {
            reads.set(reads.get() + 1);
            table.clone()
        };
        let command_line = |pid: u32| {
            asked.borrow_mut().push(pid);
            cmds.iter()
                .find(|(p, _)| *p == pid)
                .map(|(_, c)| c.to_string())
                .unwrap_or_default()
        };
        let machine = ProcessTable {
            read: &read,
            command_line: &command_line,
        };
        let c = test_pending_active_with(r, Path::new("x.json"), &machine);
        let mut asked = asked.into_inner();
        asked.sort_unstable();
        (c, reads.get(), asked)
    }

    /// The table every unknown-tree fixture starts from: each row is judged by a pass and left out.
    fn quiet_table() -> Vec<proc::ScanProc> {
        vec![
            // a child of the writer, started before the record: the by-parent pass leaves it out
            row(4_000_001, 999_998, "PING.EXE", Some(-60)),
            // codex, started before the record: the machine-wide pass leaves it out
            row(4_000_002, 1, "codex.exe", Some(-60)),
            // recent, not codex-like: read and left out by the name rule
            row(4_000_003, 1, "notepad.exe", Some(60)),
            // recent, the Codex app's server: left out and named (the machine-wide pass ran)
            row(4_000_004, 1, "codex.exe", Some(60)),
            // its start time unreadable, not codex-like: read, left out (no blanket refusal)
            row(4_000_005, 1, "svchost.exe", None),
        ]
    }

    const QUIET_CMDS: &[(u32, &str)] = &[
        (4_000_003, "notepad.exe x.txt"),
        (4_000_004, "codex.exe app-server"),
        (4_000_005, ""),
    ];

    #[test]
    fn an_unknown_tree_is_released_only_after_both_scans() {
        let r = unknown_tree_record();
        let (c, reads, asked) = check_on(&r, Ok(quiet_table()), QUIET_CMDS);
        if !cfg!(windows) {
            // outside Windows there is no scan by parent pid: refused before any scan
            assert!(c.active);
            assert!(c
                .message
                .contains("and this host cannot scan for its processes by parent pid"));
            assert_eq!(reads, 0);
            return;
        }
        assert!(!c.active, "{} | {}", c.message, c.check);
        let since_text = at(STARTED)
            .unwrap()
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();
        assert_eq!(
            c.check,
            format!(
                "unknown tree after an unconfirmed kill: the scan found no codex-like process under pid 999998 since {STARTED} - released (Win32_Process scan (children of the interrupted bridge pid 999998; started at or after {since_text}): none found; Win32_Process scan (name codex*, or a command line containing the recorded launcher or @openai/codex; not the Codex app's servers and helpers; started at or after {since_text}; excluded: pid 4000004 codex.exe [codex app-server]): none found)"
            )
        );
        // ONE table read, both passes over it; the machine-wide pass read the command lines of the
        // recent rows and of the one whose start time is unreadable - never of the older ones
        assert_eq!(reads, 1);
        assert_eq!(asked, vec![4_000_003, 4_000_004, 4_000_005]);
    }

    #[cfg(windows)]
    #[test]
    fn an_unknown_tree_is_refused_by_either_scan_and_by_an_unreadable_start() {
        let r = unknown_tree_record();
        let refused = |extra: proc::ScanProc, cmd: &'static str| {
            let mut table = quiet_table();
            let pid = extra.pid;
            table.push(extra);
            let mut cmds = QUIET_CMDS.to_vec();
            cmds.push((pid, cmd));
            let (c, _, _) = check_on(&r, Ok(table), &cmds);
            assert!(c.active, "pid {pid} refuses: {}", c.check);
            c
        };
        // the by-parent pass: a child of the writer since the record
        let c = refused(row(4_000_010, 999_998, "node.exe", Some(5)), "node x.js");
        assert!(
            c.message.contains(
                "and a codex-like process of it may still run: pid 4000010 node.exe [child of the interrupted bridge (ppid 999998)], found by Win32_Process scan (children of the interrupted bridge pid 999998;"
            ),
            "{}",
            c.message
        );
        // (F23-2, RC2) the same child with its start time unreadable: refused, never skipped
        let c = refused(row(4_000_011, 999_998, "node.exe", None), "node x.js");
        assert!(
            c.message.contains(
                "pid 4000011 node.exe [child of the interrupted bridge (ppid 999998); its start time cannot be read - counted (fail-closed)]"
            ),
            "{}",
            c.message
        );
        // the machine-wide pass, after a clean by-parent pass: codex under another parent
        let c = refused(row(4_000_012, 1, "codex.exe", Some(5)), "codex.exe exec -");
        assert!(
            c.message.contains(
                "pid 4000012 codex.exe [name codex, task not verifiable], found by Win32_Process scan (name codex*"
            ),
            "{}",
            c.message
        );
        // (F23-2, RC2) codex whose start time is unreadable, under another parent: refused
        let c = refused(row(4_000_013, 1, "codex.exe", None), "");
        assert!(
            c.message.contains(
                "pid 4000013 codex.exe [name codex, task not verifiable; its start time cannot be read - counted (fail-closed)]"
            ),
            "{}",
            c.message
        );
        // a table that cannot be read: refused
        let (c, _, _) = check_on(
            &r,
            Err("the process table could not be read (test)".into()),
            &[],
        );
        assert!(c.active);
        assert!(
            c.message.contains(
                "and the scan for its processes failed: Win32_Process scan (children of the interrupted bridge pid 999998;"
            ) && c.message.contains("could not run: the process table could not be read (test)"),
            "{}",
            c.message
        );
    }

    #[cfg(windows)]
    #[test]
    fn the_descendant_scan_counts_a_child_whose_start_cannot_be_read() {
        // (F23-2) a `launching` record outside a panel: the scan by the writer's pid finds its
        // child even though that child's start time cannot be read
        let r = record(&format!(
            r#"{{"state":"launching","n":1,"nn":"01","reply":"r","started":"{STARTED}","pid":999998,"host":"{}","launcher":"","engine":"codex","child_pid":null,"child_start_time":"","survivors":[],"unverified":[],"note":""}}"#,
            host()
        ));
        let (c, _, _) = check_on(&r, Ok(quiet_table()), QUIET_CMDS);
        assert!(!c.active, "{}", c.check);
        let mut table = quiet_table();
        table.push(row(4_000_020, 999_998, "codex.exe", None));
        let (c, _, _) = check_on(&r, Ok(table), QUIET_CMDS);
        assert!(c.active, "{}", c.check);
        assert!(
            c.message.contains(
                "may still have its codex process running: pid 4000020 codex.exe [child of the interrupted bridge (ppid 999998); its start time cannot be read - counted (fail-closed)]"
            ),
            "{}",
            c.message
        );
    }

    #[cfg(windows)]
    #[test]
    fn an_unknown_tree_with_a_live_orphan_is_refused() {
        // the record's writer is a dead process (a `cmd` that started a `ping` and exited - no
        // start time recorded: the writer rule does not apply); its orphan still runs, and the scan
        // by parent pid finds it (Windows keeps an orphan's parent id)
        let mut parent = std::process::Command::new("cmd")
            .args(["/c", "start /b ping 127.0.0.1 -n 30 >nul"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let writer = parent.id();
        let _ = parent.wait();
        let mut orphan = 0;
        let mut image = String::new();
        for _ in 0..100 {
            if let Some(p) = proc::enumerate_processes()
                .into_iter()
                .find(|p| p.ppid == writer && p.name.eq_ignore_ascii_case("ping.exe"))
            {
                orphan = p.pid;
                image = p.name;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(orphan > 0, "the orphan of {writer} runs");
        let r = record(&format!(
            r#"{{"state":"survivors","n":1,"nn":"01","reply":"r","started":"2020-01-01T00:00:00+00:00","pid":{writer},"host":"{}","launcher":"","engine":"codex","child_pid":null,"child_start_time":"","survivors":[],"unverified":[],"kill_unconfirmed":"the children could not be enumerated (test)","note":""}}"#,
            host()
        ));
        let c = test_pending_active(&r, Path::new("x.json"));
        proc::terminate_pid(orphan);
        assert!(c.active, "{}", c.check);
        assert!(
            c.message.contains(
                ") left an UNKNOWN process tree - the kill of its codex run was not confirmed (the children could not be enumerated (test)) - and a codex-like process of it may still run: "
            ) && c.message.contains(&format!(
                "pid {orphan} {image} [child of the interrupted bridge (ppid {writer})]"
            )),
            "{}",
            c.message
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_survivor_without_a_start_time_read_and_not_codex_is_dropped() {
        // a recorded survivor (no start time) that started after the record and runs something
        // that is not codex, not a child of a recorded pid: "codex pid(s) <n> [... not codex] no
        // longer running" (E19)
        let started = chrono::Local::now()
            .format("%Y-%m-%dT%H:%M:%S%:z")
            .to_string();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let mut ping = std::process::Command::new("ping")
            .args(["127.0.0.1", "-n", "30"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = ping.id();
        let r = record(&format!(
            r#"{{"state":"survivors","n":2,"nn":"03","reply":"r","started":"{started}","pid":999998,"host":"{}","launcher":"{}","engine":"codex","child_pid":null,"child_start_time":"","survivors":[{{"pid":{pid},"start_time":"","name":""}}],"unverified":[],"note":""}}"#,
            host(),
            NOWHERE.replace('\\', "\\\\")
        ));
        let c = test_pending_active(&r, Path::new("x.json"));
        let name = proc::process_info(pid).map(|i| i.name).unwrap_or_default();
        let _ = ping.kill();
        let _ = ping.wait();
        let text = format!("{} | {}", c.message, c.check);
        assert!(
            text.contains(&format!(
                "codex pid(s) {pid} [start time readable now; pid {pid} runs {name}, not codex] no longer running"
            )),
            "{text}"
        );
    }
}
