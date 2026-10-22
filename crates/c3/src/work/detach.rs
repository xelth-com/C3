//! The work run's detach/status/wait lifecycle (design v1): `--detach` starts the run in a
//! background process (the record's `run.json` in state `running` carries the background's
//! options wire, as the consult detach's status file does), `--status` prints every work run
//! of the task and `--wait` blocks for it. A small module of its own rather than a reuse of
//! `crate::consult::detach`: the consult machinery is welded to `Options` (its `DetachArgs`
//! wire mirrors every consult flag) and to the consult status-file shape; the work record
//! already is the status file here. The Windows spawn (a `CreateProcessW` that holds none of
//! the caller's handles) IS reused from the consult detach.

use std::path::Path;
#[cfg(not(windows))]
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use super::record::{read_record, run_dirs, write_record, RunRecord};
use super::{Options, Prepared};

const TOOL: &str = "c3 work";

fn iso_now() -> String {
    chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string()
}

/// An RFC3339 instant as a local wall-clock time for a status line (the string unchanged when it
/// does not parse).
fn local_of(rfc3339: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(rfc3339) {
        Ok(t) => t
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S %Z")
            .to_string(),
        Err(_) => rfc3339.to_string(),
    }
}

fn refuse(msg: &str) -> i32 {
    eprintln!("{TOOL}: {msg}");
    1
}

/// The background's options as a JSON wire (the `running` record's `args`): every field a
/// work run needs; paths absolute, an inline prompt in `prompt` (it is not a secret and the
/// record directory is private to the task).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct WorkWire {
    #[serde(default)]
    task: String,
    #[serde(default)]
    brief: String,
    #[serde(default)]
    provider: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    effort: String,
    #[serde(default = "eighty")]
    max_turns: i64,
    #[serde(default = "thirty_six_hundred")]
    timeout_sec: i64,
    #[serde(default)]
    allow: Vec<String>,
    #[serde(default)]
    collab_dir: String,
    #[serde(default)]
    quota_window_sec: i64,
    #[serde(default)]
    max_resumes: i64,
    #[serde(default)]
    unrestricted: bool,
}

fn eighty() -> i64 {
    c3_core::claude::WORK_MAX_TURNS_DEFAULT as i64
}

fn thirty_six_hundred() -> i64 {
    3600
}

impl WorkWire {
    fn of(o: &Options, collab_abs: &str) -> WorkWire {
        WorkWire {
            task: o.task.clone(),
            brief: o.brief.clone(),
            provider: o.provider.clone(),
            model: o.model.clone(),
            prompt: o.prompt.clone(),
            effort: o.effort.clone(),
            max_turns: o.max_turns,
            timeout_sec: o.timeout_sec,
            allow: o.allow.clone(),
            collab_dir: collab_abs.to_string(),
            quota_window_sec: o.quota_window_sec,
            max_resumes: o.max_resumes,
            unrestricted: o.unrestricted,
        }
    }

    fn into_options(self) -> Options {
        Options {
            task: self.task,
            brief: self.brief,
            provider: self.provider,
            model: self.model,
            prompt: self.prompt,
            effort: self.effort,
            max_turns: self.max_turns,
            timeout_sec: self.timeout_sec,
            allow: self.allow,
            collab_dir: self.collab_dir,
            quota_window_sec: self.quota_window_sec,
            max_resumes: self.max_resumes,
            unrestricted: self.unrestricted,
            ..Default::default()
        }
    }
}

/// The `running` record's `args` of a detached run, as JSON (the background's options
/// wire; never a secret). Called by the module's `run` when it writes that record.
pub(super) fn wire_of(o: &Options, collab_root: &Path) -> String {
    let wire = WorkWire::of(o, &collab_root.to_string_lossy());
    serde_json::to_string(&wire).unwrap_or_default()
}

/// The foreground of `--detach` once every guard passed and the `running` record (with the
/// wire) is on disk: spawn the background, print the come-back lines, exit 0. On a spawn
/// failure the record is removed and the caller sees exit 1.
pub fn start_detached(o: &Options, pre: &Prepared, id: &str, rec_dir: &Path) -> i32 {
    let log = rec_dir.join(".log");
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            let _ = std::fs::remove_file(rec_dir.join("run.json"));
            return refuse(&format!(
                "could not find this executable to start the background ({e}); nothing was started."
            ));
        }
    };
    let collab_abs = pre.collab_root.to_string_lossy().to_string();
    if let Err(e) = spawn_background(&exe, &o.task, &collab_abs, id, &log, &pre.repo_root) {
        let _ = std::fs::remove_file(rec_dir.join("run.json"));
        return refuse(&format!("{e}; nothing was started."));
    }
    let collab_for_cmd = (o.collab_dir != ".collab").then(|| collab_abs.clone());
    println!(
        "Detached work run {id}: it runs in the background ({} :: {} [{}]).",
        pre.provider,
        pre.model,
        pre.engine.as_str()
    );
    println!("record dir : {}", rec_dir.display());
    println!("console log: {}", log.display());
    println!(
        "come back  : c3 work --task {}{} --status --id {id} (exit 0 done, 1 died/timed out, 2 running); c3 work --task {}{} --wait --id {id} [--wait-timeout-sec <s>] waits until it is done",
        o.task,
        collab_for_cmd.as_deref().map(|c| format!(" --collab-dir \"{c}\"")).unwrap_or_default(),
        o.task,
        collab_for_cmd.as_deref().map(|c| format!(" --collab-dir \"{c}\"")).unwrap_or_default(),
    );
    0
}

/// Spawn the background so it holds none of the caller's handles. On Windows this is the
/// consult detach's `CreateProcessW` (no inherited handles, its own hidden console so the
/// `>log` redirection works); elsewhere a plain spawn with the log on stdout/stderr.
#[cfg(windows)]
fn spawn_background(
    exe: &Path,
    task: &str,
    collab_abs: &str,
    id: &str,
    log: &Path,
    cwd: &Path,
) -> Result<(), String> {
    let exe_s = exe.to_string_lossy().to_string();
    let log_s = log.to_string_lossy().to_string();
    // cmd.exe would expand a %NAME% in these paths: refuse rather than corrupt.
    for p in [&exe_s, collab_abs, &log_s, id, task] {
        if p.contains('%') {
            return Err(format!(
                "--detach starts its background through cmd.exe, which would expand the '%' in '{p}'; run without --detach"
            ));
        }
    }
    let inner = format!(
        "\"{exe_s}\" work --task {task} --collab-dir \"{collab_abs}\" --detach-id {id} <NUL 1>\"{log_s}\" 2>&1"
    );
    let raw = format!("/d /v:off /s /c \"{inner}\"");
    let comspec = std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".into());
    // SAFETY: the consult detach's straight CreateProcessW (bInheritHandles=FALSE, its own
    // hidden console); every pointer is to a local, NUL-terminated buffer.
    unsafe { crate::consult::detach::create_process_no_inherit(&comspec, &raw, cwd, None) }
}

#[cfg(not(windows))]
fn spawn_background(
    exe: &Path,
    task: &str,
    collab_abs: &str,
    id: &str,
    log: &Path,
    cwd: &Path,
) -> Result<(), String> {
    let log_file = std::fs::File::create(log)
        .map_err(|e| format!("could not open the log file '{}' ({e})", log.display()))?;
    let log_err = log_file
        .try_clone()
        .map_err(|e| format!("could not open the log file ({e})"))?;
    let mut cmd = Command::new(exe);
    cmd.arg("work")
        .arg("--task")
        .arg(task)
        .arg("--collab-dir")
        .arg(collab_abs)
        .arg("--detach-id")
        .arg(id)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_err));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("could not start the background process ({e})"))
}

/// The background of `--detach-id <id>`: find the `running` record, take the wire, report
/// this process's pid, run the work child in this process and make the record final.
pub fn background(o: &Options) -> i32 {
    let rec_dir = match super::prepare_paths(o) {
        Ok(p) => p.2.join("work").join(&o.detach_id),
        Err(why) => return refuse(&why),
    };
    let mut rec = match read_record(&rec_dir) {
        Ok(Some(r)) => r,
        Ok(None) => {
            return refuse(&format!(
                "--detach-id is internal to --detach: there is no run.json in {}.",
                rec_dir.display()
            ))
        }
        Err(e) => return refuse(&format!("--detach-id is internal to --detach: {e}")),
    };
    if rec.status != "running" {
        return refuse(&format!(
            "--detach-id is internal to --detach: the work run {} is in state {}, not this process's.",
            rec.id, rec.status
        ));
    }
    let wire = rec.args.take().unwrap_or_default();
    let opts = match serde_json::from_str::<WorkWire>(&wire) {
        Ok(w) => w.into_options(),
        Err(e) => {
            rec.status = "refused".into();
            rec.ended = Some(iso_now());
            let _ = write_record(&rec_dir, &rec);
            return refuse(&format!(
                "the work run's recorded arguments are unusable: {e}"
            ));
        }
    };
    // (0.2.4, F89-1/F89-2) Claim the record under the task admission lock: write this background's
    // pid + start time (the liveness owner) and refuse if another run of the task went live in the
    // meantime, all BEFORE the child is spawned.
    let work_root = rec_dir.parent().map(Path::to_path_buf).unwrap_or_default();
    if let Err(e) = super::claim_start(&work_root, &rec_dir) {
        rec.status = "refused".into();
        rec.ended = Some(iso_now());
        let _ = write_record(&rec_dir, &rec);
        return refuse(&format!("the detached work run could not be claimed: {e}"));
    }
    println!(
        "{TOOL}: work run {} ({} :: {}) - pid {} on {}, started {}",
        rec.id,
        rec.provider,
        rec.model,
        std::process::id(),
        c3_core::host::machine_name(),
        iso_now()
    );
    super::run_prepared(&opts, &rec_dir, &rec.id, rec.seq, true)
}

// ------------------------------------------------------------------ --status / --wait

/// One work run as the query sees it.
struct WorkRun {
    id: String,
    record: Option<RunRecord>,
    error: String,
}

fn read_runs(work_root: &Path) -> Vec<WorkRun> {
    // Status/--wait is a read-only listing (not the admission boundary): an enumeration failure
    // shows nothing, while a per-record inspection error is surfaced as that run's `error`.
    run_dirs(work_root)
        .unwrap_or_default()
        .into_iter()
        .map(|(_, dir)| {
            let (record, error) = match read_record(&dir) {
                Ok(r) => (r, String::new()),
                Err(e) => (None, e),
            };
            let id = record.as_ref().map(|r| r.id.clone()).unwrap_or_else(|| {
                dir.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            });
            WorkRun { id, record, error }
        })
        .collect()
}

/// The exit code of a status listing: 2 while any run is running (or `waiting_quota`, a detached
/// run whose alarm sleeps until the quota window resets - the background is alive), else 1 when
/// one died/timed out/was refused/unreadable or is parked on `quota` (it needs a hand - a
/// `--resume`/`--kick`, exhausted or not), else 0 (an empty task is 0 too).
fn exit_of(runs: &[WorkRun]) -> i32 {
    for r in runs {
        let st = r
            .record
            .as_ref()
            .map(|x| x.status.as_str())
            .unwrap_or("unreadable");
        if super::record::is_pending(st) {
            return 2;
        }
    }
    for r in runs {
        let st = r
            .record
            .as_ref()
            .map(|x| x.status.as_str())
            .unwrap_or("unreadable");
        if st != "done" {
            return 1;
        }
    }
    0
}

/// `--status` / `--wait`: read the task's work records only (no roster, no launcher, no
/// lock), print one block per run and return the exit code.
pub fn query(o: &Options) -> i32 {
    let (repo_root, _collab_root, task_dir) = match super::prepare_paths(o) {
        Ok(p) => p,
        Err(why) => return refuse(&why),
    };
    let _ = repo_root;
    let want = o.id.trim().to_lowercase();
    let work_root = task_dir.join("work");
    let select = |want: &str| -> Vec<WorkRun> {
        read_runs(&work_root)
            .into_iter()
            .filter(|r| want.is_empty() || r.id.to_lowercase().starts_with(want))
            .collect()
    };
    let mut runs = select(&want);
    if !want.is_empty() && runs.is_empty() {
        let known: Vec<String> = read_runs(&work_root).into_iter().map(|r| r.id).collect();
        return refuse(&format!(
            "no work run of task '{}' has an id starting with '{want}' ({}).",
            o.task,
            if known.is_empty() {
                "it has none".to_string()
            } else {
                format!("its work runs: {}", known.join(", "))
            }
        ));
    }
    if o.wait {
        if o.wait_timeout_sec_given && o.wait_timeout_sec <= 0 {
            return refuse(&format!(
                "--wait-timeout-sec must be greater than 0 (got {}); leave it out for the run's own timeout.",
                o.wait_timeout_sec
            ));
        }
        let limit = if o.wait_timeout_sec > 0 {
            o.wait_timeout_sec
        } else {
            runs.iter()
                .filter(|r| {
                    r.record
                        .as_ref()
                        .map(|x| super::record::is_pending(&x.status))
                        .unwrap_or(false)
                })
                .filter_map(|r| r.record.as_ref().map(|x| x.timeout_sec))
                .max()
                .filter(|t| *t > 0)
                .unwrap_or(3600)
        };
        let waiting: Vec<String> = runs.iter().map(|r| r.id.clone()).collect();
        println!(
            "{TOOL}: waiting for {} work run{} of task '{}' - up to {limit} s, checking every 2 s",
            waiting.len(),
            if waiting.len() != 1 { "s" } else { "" },
            o.task
        );
        let watch = std::time::Instant::now();
        loop {
            runs = select(&want);
            let still = runs.iter().any(|r| {
                r.record
                    .as_ref()
                    .map(|x| super::record::is_pending(&x.status))
                    .unwrap_or(false)
            });
            if !still || watch.elapsed().as_secs() >= limit as u64 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    }
    if runs.is_empty() {
        println!(
            "{TOOL}: no work run in task '{}' ({}).",
            o.task,
            work_root.display()
        );
        return 0;
    }
    let mut first = true;
    for r in &runs {
        if !first {
            println!();
        }
        first = false;
        let Some(rec) = &r.record else {
            println!("work {}: unreadable ({})", r.id, r.error);
            continue;
        };
        println!(
            "work {} ({}/{} [{}]): {} (started {}{}, exit {})",
            rec.id,
            rec.provider,
            rec.model,
            rec.engine,
            rec.status,
            rec.started,
            rec.ended
                .as_deref()
                .map(|e| format!(", ended {e}"))
                .unwrap_or_default(),
            rec.exit_code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "(none)".into())
        );
        let turns = rec
            .turns
            .map(|t| t.to_string())
            .unwrap_or_else(|| "(none)".into());
        let tools = if rec.tool_calls.is_empty() {
            String::new()
        } else {
            format!(
                "; tool calls {}",
                rec.tool_calls
                    .iter()
                    .map(|(k, v)| format!("{k} x{v}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        println!("  turns {turns}{tools}");
        if rec.files_changed.is_empty() {
            println!("  files changed: (none)");
        } else {
            println!("  files changed: {}", rec.files_changed.join(", "));
        }
        if let Some(q) = &rec.quota {
            if rec.status == "waiting_quota" {
                // (0.2.4, F89-6) The persisted cap, unless the operator overrode it on this call.
                let max = if o.max_resumes > 0 {
                    o.max_resumes
                } else if rec.max_resumes > 0 {
                    rec.max_resumes
                } else {
                    super::MAX_RESUMES_DEFAULT
                };
                println!(
                    "  waiting_quota until {} (resume {}/{})",
                    local_of(&q.resume_at),
                    q.resumes,
                    max
                );
            }
            println!(
                "  quota: {} (resumes {}{}; resume_at {})",
                q.text,
                q.resumes,
                if q.exhausted { ", exhausted" } else { "" },
                q.resume_at
            );
        }
    }
    exit_of(&runs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(id: &str, status: &str) -> WorkRun {
        WorkRun {
            id: id.into(),
            record: Some(RunRecord {
                id: id.into(),
                status: status.into(),
                ..Default::default()
            }),
            error: String::new(),
        }
    }

    #[test]
    fn the_status_exit_codes() {
        assert_eq!(exit_of(&[]), 0);
        assert_eq!(exit_of(&[run("01-a-b", "done")]), 0);
        assert_eq!(exit_of(&[run("01-a-b", "done"), run("02-a-b", "done")]), 0);
        assert_eq!(exit_of(&[run("01-a-b", "died")]), 1);
        assert_eq!(exit_of(&[run("01-a-b", "timeout")]), 1);
        assert_eq!(exit_of(&[run("01-a-b", "refused")]), 1);
        assert_eq!(
            exit_of(&[WorkRun {
                id: "x".into(),
                record: None,
                error: "bad".into()
            }]),
            1
        );
        // running outranks a failure
        assert_eq!(
            exit_of(&[run("01-a-b", "died"), run("02-a-b", "running")]),
            2
        );
        assert_eq!(
            exit_of(&[run("01-a-b", "done"), run("02-a-b", "running")]),
            2
        );
    }

    /// `--status`/`--wait` over synthetic `run.json` files of a scratch task.
    #[test]
    fn the_query_reads_synthetic_records() {
        let d = std::env::temp_dir().join(format!(
            "c3-work-query-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let work = d.join(".collab").join("t").join("work");
        let mk = |id: &str, status: &str, seq: i64| {
            let dir = work.join(id);
            std::fs::create_dir_all(&dir).unwrap();
            write_record(
                &dir,
                &RunRecord {
                    id: id.into(),
                    task: "t".into(),
                    seq,
                    provider: "ZAI-claude".into(),
                    model: "glm-5.3".into(),
                    engine: "claude".into(),
                    started: "2026-10-20T10:00:00+00:00".into(),
                    ended: Some("2026-10-20T10:01:00+00:00".into()),
                    exit_code: Some(0),
                    status: status.into(),
                    turns: Some(3),
                    timeout_sec: 60,
                    files_changed: vec!["src/a.rs".into()],
                    // a running record needs a live pid, else it reads as died
                    pid: (status == "running").then(|| std::process::id() as i64),
                    ..Default::default()
                },
            )
            .unwrap();
        };
        // only done: exit 0
        mk("01-zai-claude-glm-5.3", "done", 1);
        let o = Options {
            task: "t".into(),
            collab_dir: d.join(".collab").to_string_lossy().to_string(),
            status: true,
            ..Default::default()
        };
        assert_eq!(query(&o), 0);
        // a died run: exit 1; a running one outranks it: exit 2
        mk("02-zai-claude-glm-5.3", "died", 2);
        assert_eq!(query(&o), 1);
        mk("03-zai-claude-glm-5.3", "running", 3);
        assert_eq!(query(&o), 2);
        // --id narrows to one run
        let one = Options {
            id: "01".into(),
            ..o.clone()
        };
        assert_eq!(query(&one), 0);
        let none = Options {
            id: "99".into(),
            ..o.clone()
        };
        assert_eq!(query(&none), 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The background's options wire round-trips through JSON.
    #[test]
    fn the_wire_round_trips() {
        let w = WorkWire {
            task: "t".into(),
            brief: "b.md".into(),
            provider: "ZAI-claude".into(),
            model: "glm-5.3".into(),
            prompt: "the ask".into(),
            effort: "high".into(),
            max_turns: 40,
            timeout_sec: 120,
            allow: vec!["Bash(cargo test:*)".into()],
            collab_dir: "C:/r/.collab".into(),
            quota_window_sec: 900,
            max_resumes: 2,
            unrestricted: true,
        };
        let text = serde_json::to_string(&w).unwrap();
        let back: WorkWire = serde_json::from_str(&text).unwrap();
        let o = back.into_options();
        assert_eq!(o.task, "t");
        assert_eq!(o.model, "glm-5.3");
        assert_eq!(o.max_turns, 40);
        assert_eq!(o.timeout_sec, 120);
        assert_eq!(o.quota_window_sec, 900);
        assert_eq!(o.max_resumes, 2);
        assert!(o.unrestricted);
        assert_eq!(o.allow, vec!["Bash(cargo test:*)".to_string()]);
        assert!(!o.detach && !o.status && !o.wait && o.detach_id.is_empty());
        // the defaults of the wire are the CLI's defaults
        let empty: WorkWire = serde_json::from_str("{}").unwrap();
        assert_eq!(empty.max_turns, 80);
        assert_eq!(empty.timeout_sec, 3600);
    }
}
