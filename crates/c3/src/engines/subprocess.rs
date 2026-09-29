//! Launching a subprocess engine turn (`Start-EngineProcess` / `Invoke-EngineTurn`,
//! `codex-consult.ps1:695-818`).
//!
//! One turn is: write the prompt where the engine expects it (per [`PromptDelivery`]),
//! spawn the launcher from the repo root with stdout redirected to the run's
//! `.events.jsonl`, stderr to a sidecar file, feed stdin, then wait up to `timeout`. On a
//! timeout the process tree is killed (Windows `taskkill /T`, Unix a plain kill of the
//! child) and the wall time is measured and rounded to one decimal exactly like the plugin
//! (`[math]::Round($watch.Elapsed.TotalSeconds, 1)`).
//!
//! This module is engine-agnostic: it delivers the prompt and captures the streams; parsing
//! the events into an [`crate::engines`] outcome is the engine adapter's job ([`super::codex`]).

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use c3_core::engine::PromptDelivery;

/// One subprocess turn to run.
pub struct SpawnRequest<'a> {
    /// The resolved launcher path (may be a `.cmd`/`.bat` on Windows).
    pub launcher: &'a str,
    /// Everything after the launcher (already built by `c3_core::engine`).
    pub argv: &'a [String],
    /// The working directory (the git repo root).
    pub cwd: &'a Path,
    /// How the prompt reaches the engine; decides whether stdin carries it.
    pub prompt_delivery: PromptDelivery,
    /// The bytes to write to child stdin: the prompt for codex, the NDJSON line for agy,
    /// empty for muse (its prompt is the `--prompt-file` named in `argv`).
    pub stdin_text: &'a str,
    /// stdout is redirected here (the `.events.jsonl`).
    pub events_path: &'a Path,
    /// stderr is redirected here.
    pub stderr_path: &'a Path,
    /// The wall-clock budget for the turn.
    pub timeout: Duration,
    /// (wave 26b, D12) The stall cut in seconds (`0` = off): the turn is stopped like a timeout
    /// when its event stream produces no growth for this long while the process lives. Per wave
    /// 26c D3 the silent timer resets on any growth of the stream in bytes and is SUSPENDED while
    /// a tool call is in flight (see `tool_delta`).
    pub stall_sec: i64,
    /// (wave 26b, D10) The operator's kick file: when it appears the turn is stopped and recorded
    /// as `stopped by the operator (-Kick)`. `None` disables the check.
    pub kick_path: Option<&'a Path>,
    /// (wave 26c, D3) Classifies one event-stream line for the stall's tool-call suspension:
    /// `+1` when the line STARTS a tool call, `-1` when it ENDS one, `0` otherwise. While the
    /// running count is above zero the stall timer is suspended. `None` = no suspension (the
    /// timer still resets on byte growth).
    pub tool_delta: Option<&'a dyn Fn(&str) -> i64>,
    /// Called once, right after the child is spawned and before it is waited on, with the
    /// child pid and its start time (.NET `o` string, or empty when unavailable). The
    /// orchestrator uses it to flip the recovery record `launching` -> `running` while the
    /// child is live (`codex-consult.ps1:3330-3339`).
    pub on_running: Option<&'a dyn Fn(u32, String)>,
}

/// Why [`run_turn`] stopped watching the child.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnStop {
    /// The child exited on its own.
    Exited,
    /// The wall-clock timeout fired and the tree was killed.
    Timeout,
    /// The stall cut fired (no event for `stall_sec` while the process lived) and the tree was killed.
    Stall,
    /// The operator's kick file appeared and the tree was killed.
    Kick,
}

/// The result of one subprocess turn.
#[derive(Debug, Clone)]
pub struct TurnResult {
    /// Whether a process was actually started.
    pub started: bool,
    /// The child's exit code, or `None` when it was killed/timed out or never started.
    pub exit_code: Option<i32>,
    /// Why the watch ended (exited / timeout / stall / kick).
    pub stop: TurnStop,
    /// (wave 26c, D1) The process had already finished when a kick was found (checked once more
    /// after it exited): the kick is taken LATE (`kick_late`), the outcome unchanged.
    pub kick_late: bool,
    /// (wave 26b, D12) The ISO time of the last event-stream growth seen, `None` when none.
    pub last_event: Option<String>,
    /// (wave 26b, D12) Seconds without an event at the stall kill (`0` unless a stall fired).
    pub silent_seconds: i64,
    /// (wave 27c, D5) How many over-long unfinished lines (> 1 MiB with no line end) the bounded
    /// stream reader discarded during the turn; `0` normally. A non-zero count warns once per run.
    pub oversized_lines: u64,
    /// Pids that survived the kill (best-effort; empty when the tree died cleanly).
    pub survivors: Vec<u32>,
    /// Wall time, rounded to one decimal.
    pub wall_seconds: f64,
    /// The captured stderr text (UTF-8, lossily decoded).
    pub stderr: String,
    /// A launch error, when the process could not be started.
    pub error: Option<String>,
}

impl TurnResult {
    fn not_started(error: String) -> TurnResult {
        TurnResult {
            started: false,
            exit_code: None,
            stop: TurnStop::Exited,
            kick_late: false,
            last_event: None,
            silent_seconds: 0,
            oversized_lines: 0,
            survivors: Vec::new(),
            wall_seconds: 0.0,
            stderr: String::new(),
            error: Some(error),
        }
    }
}

/// (wave 27c, D5) The bounded stream reader's carry cap: the text after the last line end is kept
/// only up to 1 MiB; a longer unfinished line is discarded up to its end and counted.
const STREAM_CARRY_CAP: usize = 1024 * 1024;

/// (wave 27c, D6) An open tool call cannot suspend the stall timer for ever: it may hold the timer
/// for at most `max(3 x stall seconds, 1800 s)` with no growth of the stream at all.
fn tool_suspension_cap_secs(stall_sec: i64) -> i64 {
    (stall_sec.saturating_mul(3)).max(1800)
}

fn round1(secs: f64) -> f64 {
    (secs * 10.0).round() / 10.0
}

/// `<kick file>.ack` — the acknowledgement the run writes for the `-Kick` command.
pub fn kick_ack_path(kick_path: &Path) -> std::path::PathBuf {
    let mut s = kick_path.as_os_str().to_os_string();
    s.push(".ack");
    std::path::PathBuf::from(s)
}

/// One acknowledgement of a kick request (wave 27c, D1): the result (`stopped` | `late`) and the
/// request id it acknowledges.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KickAck {
    pub result: String,
    pub id: String,
}

/// Parse the `id=<request id>` line of a kick file (wave 27c, D1). `None` when the file is absent
/// or carries no id.
pub fn read_kick_id(kick_path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(kick_path).ok()?;
    for line in text.lines() {
        if let Some(v) = line.trim().strip_prefix("id=") {
            let v = v.trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// Write the kick file atomically (temp file, then rename) so a caller never reads a half-written
/// request (wave 27c, D1). The file carries the request `id` and a human line.
pub fn write_kick_atomic(kick_path: &Path, id: &str) -> std::io::Result<()> {
    let body = format!(
        "id={id}\n{} -Kick from pid {}\n",
        iso_now(),
        std::process::id()
    );
    let mut tmp = kick_path.as_os_str().to_os_string();
    tmp.push(format!(".tmp-{}", std::process::id()));
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, body.as_bytes())?;
    match std::fs::rename(&tmp, kick_path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Write `<kick>.ack` holding the result and the request id it acknowledges (wave 27c, D1).
pub fn write_kick_ack(kick_path: &Path, result: &str, id: &str) {
    let _ = std::fs::write(
        kick_ack_path(kick_path),
        format!(
            "result={result}\nid={id}\n{} pid {}\n",
            iso_now(),
            std::process::id()
        ),
    );
}

/// Read `<kick>.ack` (wave 27c, D1). `None` when it is absent or unreadable.
pub fn read_kick_ack(ack_path: &Path) -> Option<KickAck> {
    let text = std::fs::read_to_string(ack_path).ok()?;
    let mut ack = KickAck::default();
    for line in text.lines() {
        let l = line.trim();
        if let Some(v) = l.strip_prefix("result=") {
            ack.result = v.trim().to_string();
        } else if let Some(v) = l.strip_prefix("id=") {
            ack.id = v.trim().to_string();
        }
    }
    Some(ack)
}

/// Sweep a stale acknowledgement (wave 27c, D1): remove `<kick>.ack` when it is older than
/// `max_age_secs` and its id is NOT `my_id` (a caller never sweeps its own live acknowledgement).
/// Best-effort; never fails.
pub fn sweep_stale_ack(kick_path: &Path, my_id: Option<&str>, max_age_secs: u64) {
    let ack_path = kick_ack_path(kick_path);
    let meta = match std::fs::metadata(&ack_path) {
        Ok(m) => m,
        Err(_) => return,
    };
    let age = meta
        .modified()
        .ok()
        .and_then(|m| m.elapsed().ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if age < max_age_secs {
        return;
    }
    if let Some(mine) = my_id {
        if read_kick_ack(&ack_path).map(|a| a.id).as_deref() == Some(mine) {
            return; // never sweep the caller's own acknowledgement
        }
    }
    let _ = std::fs::remove_file(&ack_path);
}

/// `Confirm-Kick` (wave 26c D1 / 27c D1): the member acknowledges the kick with `<kick>.ack`
/// holding the `result` (`stopped` | `late`) and the request id read from the kick file, then
/// removes the kick file. Never fails the run.
fn confirm_kick(kick_path: &Path, result: &str) {
    let id = read_kick_id(kick_path).unwrap_or_default();
    write_kick_ack(kick_path, result, &id);
    let _ = std::fs::remove_file(kick_path);
}

/// Build `(program, args)` for the launcher spawn.
///
/// The launcher path is always the program and the caller's `argv` are always the args —
/// including for a Windows `.cmd`/`.bat` launcher. We deliberately do **not** wrap a batch
/// launcher in an explicit `cmd /c`: doing so makes `cmd.exe` the program (an `.exe`), so
/// Rust escapes the embedded quotes in each arg the MSVC way (`model_provider=\"ZAI\"`),
/// which is not what a batch file's `%*` expander produces. Handing the `.cmd`/`.bat` path
/// straight to `Command` lets Rust std's own batch-file handling run it: it invokes the file
/// through `cmd.exe` with the plugin's quote-doubling (`model_provider=""ZAI""`) and refuses
/// (a spawn `io::Error`) any argument it cannot escape safely (newlines, `%`, unbalanced
/// quotes) — matching the plugin's own launch rule and the fake codex's `%*` matching. The
/// child pid is still `cmd.exe`'s (std spawns it), so `kill_tree`/`on_running` are unchanged.
/// `.exe` launcher escaping is untouched.
fn program_and_args(launcher: &str, argv: &[String]) -> (String, Vec<String>) {
    (launcher.to_string(), argv.to_vec())
}

/// Run one subprocess turn (spawn, feed stdin, capture streams, wait with a timeout kill).
pub fn run_turn(req: &SpawnRequest) -> TurnResult {
    let stdout_file = match File::create(req.events_path) {
        Ok(f) => f,
        Err(e) => return TurnResult::not_started(format!("could not open the events file: {e}")),
    };
    let stderr_file = match File::create(req.stderr_path) {
        Ok(f) => f,
        Err(e) => return TurnResult::not_started(format!("could not open the stderr file: {e}")),
    };

    let (program, args) = program_and_args(req.launcher, req.argv);
    let mut cmd = Command::new(&program);
    cmd.args(&args)
        .current_dir(req.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file));
    // (wave 27 / 27b) the reviewer CLI never inherits the coordinator's host markers.
    super::scrub_host_markers(&mut cmd);

    let mut child: Child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return TurnResult::not_started(format!("could not start {program}: {e}")),
    };

    // Register the running child (recovery record `launching` -> `running`) before it is
    // waited on. The pid is the launcher's; on Windows the real codex is a descendant of the
    // `cmd /c` wrapper, so a mid-run crash leaves the record naming this pid and the next run
    // scans the tree — matching the plugin recording `$proc.Id`.
    if let Some(cb) = req.on_running {
        let pid = child.id();
        let start = crate::liveness::proc::process_start_iso(pid).unwrap_or_default();
        cb(pid, start);
    }

    // Feed stdin. muse gets an empty stdin; codex/agy get their text. Closing the handle
    // (drop) sends EOF so the child's ReadToEnd returns.
    if let Some(mut stdin) = child.stdin.take() {
        // A muse turn writes nothing (its prompt is a file) — still close stdin.
        if !matches!(req.prompt_delivery, PromptDelivery::PromptFile) {
            let _ = stdin.write_all(req.stdin_text.as_bytes());
        }
        // drop closes stdin
    }

    let start = Instant::now();
    let mut stop = TurnStop::Exited;
    let mut kick_late = false;
    let mut survivors = Vec::new();
    // Stall tracking (wave 26b D12 + 26c D3): byte offset consumed so far, the last time the
    // stream grew, the count of tool calls currently in flight, and the last event time seen.
    let stall_on = req.stall_sec > 0;
    let mut offset: u64 = 0;
    let mut line_buf = String::new();
    let mut last_activity = Instant::now();
    let mut open_tools: i64 = 0;
    let mut last_event: Option<String> = None;
    let mut silent_seconds: i64 = 0;
    // (wave 27c, D5) over-long unfinished lines discarded, and whether we are skipping the tail of
    // one until its line end. (wave 27c, D6) when the current tool-call suspension began.
    let mut oversized_lines: u64 = 0;
    let mut discarding = false;
    let mut tool_open_since: Option<Instant> = None;
    let kill_now = |child: &mut Child| -> Vec<u32> {
        let mut s = kill_tree(child);
        // TEST HOOK: CODEX_CONSULT_TEST_SURVIVORS=<pid>[,<pid>] — these pids, when alive, are
        // reported as survivors of this kill (no test can make a real process outlive a kill).
        // Only ever adds (a stricter outcome), matching `$env:CODEX_CONSULT_TEST_SURVIVORS`.
        for hook in test_survivor_pids() {
            if crate::liveness::proc::pid_alive(hook, "") && !s.contains(&hook) {
                s.push(hook);
            }
        }
        let _ = child.wait();
        s
    };
    // (wave 26c, D1) a kick that arrived before the wait loop, on a live turn, is taken at once.
    if let Some(kp) = req.kick_path {
        if kp.exists() {
            match child.try_wait() {
                Ok(Some(_)) => {} // already exited — handled after the loop as a late kick
                _ => {
                    survivors = kill_now(&mut child);
                    stop = TurnStop::Kick;
                    confirm_kick(kp, "stopped");
                }
            }
        }
    }
    let exit_code = if stop == TurnStop::Kick {
        // A kick was taken before the loop; do not wait.
        None
    } else {
        loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.code(),
                Ok(None) => {
                    if start.elapsed() >= req.timeout {
                        survivors = kill_now(&mut child);
                        stop = TurnStop::Timeout;
                        break None;
                    }
                    // (wave 26b, D10 / 26c D1) the operator's kick, checked on every poll; on a live
                    // turn it is taken at once and acknowledged.
                    if let Some(kp) = req.kick_path {
                        if kp.exists() {
                            survivors = kill_now(&mut child);
                            stop = TurnStop::Kick;
                            confirm_kick(kp, "stopped");
                            break None;
                        }
                    }
                    // (wave 26b, D12 / 26c D3 / 27c D5+D6) the stall cut.
                    if stall_on {
                        let grew = read_stream_growth(
                            req.events_path,
                            &mut offset,
                            &mut line_buf,
                            req.tool_delta,
                            &mut open_tools,
                            &mut oversized_lines,
                            &mut discarding,
                        );
                        if grew {
                            last_activity = Instant::now();
                            last_event = Some(iso_now());
                        }
                        // (wave 27c, D6) track when the current tool-call suspension began.
                        if open_tools > 0 {
                            tool_open_since.get_or_insert_with(Instant::now);
                        } else {
                            tool_open_since = None;
                        }
                        let silent = last_activity.elapsed().as_secs() as i64;
                        // The timer is suspended while a tool call is in flight — but only up to
                        // `max(3 x stall, 1800 s)` with no growth of the stream at all (wave 27c D6).
                        let fire = if open_tools <= 0 {
                            silent >= req.stall_sec
                        } else {
                            silent >= tool_suspension_cap_secs(req.stall_sec)
                        };
                        if fire {
                            silent_seconds = silent;
                            if open_tools > 0 {
                                let tool_open = tool_open_since
                                    .map(|t| t.elapsed().as_secs() as i64)
                                    .unwrap_or(0);
                                eprintln!(
                                    "codex-consult: no output for {silent_seconds} s (a tool call open for {tool_open} s)"
                                );
                            }
                            survivors = kill_now(&mut child);
                            stop = TurnStop::Stall;
                            break None;
                        }
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    return TurnResult {
                        started: true,
                        exit_code: None,
                        stop: TurnStop::Exited,
                        last_event,
                        silent_seconds: 0,
                        oversized_lines,
                        survivors: Vec::new(),
                        wall_seconds: round1(start.elapsed().as_secs_f64()),
                        stderr: read_text(req.stderr_path),
                        kick_late: false,
                        error: Some(format!("waiting on the child failed: {e}")),
                    };
                }
            }
        }
    };
    // (wave 26c, D1) check once more after the process ended on its own: a kick found now had no
    // live turn to stop — it is taken LATE (the outcome is unchanged), acknowledged as `late`.
    if stop == TurnStop::Exited {
        if let Some(kp) = req.kick_path {
            if kp.exists() {
                kick_late = true;
                confirm_kick(kp, "late");
            }
        }
    }
    let wall_seconds = round1(start.elapsed().as_secs_f64());

    // (wave 27c, D5) one warning per run when an over-long unfinished line was discarded.
    if oversized_lines > 0 {
        eprintln!(
            "codex-consult: {oversized_lines} over-long unfinished line(s) (> 1 MiB) were discarded from the event stream"
        );
    }

    TurnResult {
        started: true,
        exit_code,
        stop,
        kick_late,
        last_event,
        silent_seconds,
        oversized_lines,
        survivors,
        wall_seconds,
        stderr: read_text(req.stderr_path),
        error: None,
    }
}

/// Read any new bytes of the events stream since `offset`, advancing it and the carried
/// partial-line buffer. Applies `tool_delta` to each COMPLETE new line to keep `open_tools`
/// (the count of tool calls in flight) current. Returns whether the stream grew at all (any new
/// bytes) — the caller resets the silent timer on that (wave 26c D3: reset on byte growth).
#[allow(clippy::too_many_arguments)]
fn read_stream_growth(
    path: &Path,
    offset: &mut u64,
    line_buf: &mut String,
    tool_delta: Option<&dyn Fn(&str) -> i64>,
    open_tools: &mut i64,
    oversized: &mut u64,
    discarding: &mut bool,
) -> bool {
    use std::io::{Seek, SeekFrom};
    let mut f = match File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let len = match f.metadata() {
        Ok(m) => m.len(),
        Err(_) => return false,
    };
    if len <= *offset {
        return false;
    }
    if f.seek(SeekFrom::Start(*offset)).is_err() {
        return false;
    }
    let mut buf = Vec::new();
    let n = match f.read_to_end(&mut buf) {
        Ok(n) => n,
        Err(_) => return false,
    };
    if n == 0 {
        return false;
    }
    *offset += n as u64;
    if let Some(delta) = tool_delta {
        let mut chunk = String::from_utf8_lossy(&buf).into_owned();
        // (wave 27c, D5) if we are discarding the tail of an over-long line, skip to its end.
        if *discarding {
            match chunk.find('\n') {
                Some(nl) => {
                    *discarding = false;
                    chunk = chunk[nl + 1..].to_string();
                }
                None => return true, // still no line end — keep discarding; the stream grew
            }
        }
        line_buf.push_str(&chunk);
        while let Some(nl) = line_buf.find('\n') {
            let line: String = line_buf.drain(..=nl).collect();
            let line = line.trim();
            if !line.is_empty() {
                *open_tools = (*open_tools + delta(line)).max(0);
            }
        }
        // (wave 27c, D5) the carry is the text after the last line end, capped at 1 MiB: a longer
        // unfinished line is discarded up to its end and counted.
        if line_buf.len() > STREAM_CARRY_CAP {
            line_buf.clear();
            *oversized += 1;
            *discarding = true;
        }
    }
    true
}

fn iso_now() -> String {
    chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string()
}

/// Kill the process tree. On Windows `taskkill /F /T /PID <pid>` kills the whole tree
/// (the `cmd`/`powershell` wrapper the fakes use plus its grandchildren); on Unix the child
/// is killed directly. Returns pids that appear to have survived (best-effort; empty here).
fn kill_tree(child: &mut Child) -> Vec<u32> {
    #[cfg(windows)]
    {
        let pid = child.id();
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = child.kill();
        Vec::new()
    }
    #[cfg(not(windows))]
    {
        let _ = child.kill();
        Vec::new()
    }
}

/// The pids named by `CODEX_CONSULT_TEST_SURVIVORS` (comma-separated), for the survivor hook.
fn test_survivor_pids() -> Vec<u32> {
    c3_core::test_hooks::hook("CODEX_CONSULT_TEST_SURVIVORS")
        .map(|v| {
            v.split(',')
                .filter_map(|s| s.trim().parse::<u32>().ok())
                .filter(|p| *p > 0)
                .collect()
        })
        .unwrap_or_default()
}

fn read_text(path: &Path) -> String {
    let mut s = Vec::new();
    if let Ok(mut f) = File::open(path) {
        let _ = f.read_to_end(&mut s);
    }
    String::from_utf8_lossy(&s).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // (wave 26b, D12 / 26c D3) the stall detection's pure helper: any byte growth is seen, and a
    // tool-call classifier keeps the in-flight count so the timer can suspend while one runs.
    #[test]
    fn stream_growth_and_tool_call_suspension() {
        let dir = std::env::temp_dir().join(format!("c3-sg-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("events.jsonl");
        let mut f = File::create(&path).unwrap();
        let delta = |line: &str| crate::engines::codex::codex_tool_delta(line);
        let td: Option<&dyn Fn(&str) -> i64> = Some(&delta);

        let mut offset = 0u64;
        let mut line_buf = String::new();
        let mut open = 0i64;
        let mut oversized = 0u64;
        let mut discarding = false;
        let grow = |offset: &mut u64,
                    line_buf: &mut String,
                    open: &mut i64,
                    oversized: &mut u64,
                    discarding: &mut bool| {
            read_stream_growth(&path, offset, line_buf, td, open, oversized, discarding)
        };

        // No growth yet.
        assert!(!grow(
            &mut offset,
            &mut line_buf,
            &mut open,
            &mut oversized,
            &mut discarding
        ));

        // A plain agent-message line: growth, no open tool call.
        writeln!(
            f,
            r#"{{"type":"item.completed","item":{{"type":"agent_message","text":"a"}}}}"#
        )
        .unwrap();
        f.flush().unwrap();
        assert!(grow(
            &mut offset,
            &mut line_buf,
            &mut open,
            &mut oversized,
            &mut discarding
        ));
        assert_eq!(open, 0);

        // A tool call starts: the count rises (the timer would suspend).
        writeln!(
            f,
            r#"{{"type":"item.started","item":{{"type":"command_execution"}}}}"#
        )
        .unwrap();
        f.flush().unwrap();
        assert!(grow(
            &mut offset,
            &mut line_buf,
            &mut open,
            &mut oversized,
            &mut discarding
        ));
        assert_eq!(open, 1);

        // It completes: back to zero (the timer resumes).
        writeln!(
            f,
            r#"{{"type":"item.completed","item":{{"type":"command_execution"}}}}"#
        )
        .unwrap();
        f.flush().unwrap();
        assert!(grow(
            &mut offset,
            &mut line_buf,
            &mut open,
            &mut oversized,
            &mut discarding
        ));
        assert_eq!(open, 0);

        // No further growth.
        assert!(!grow(
            &mut offset,
            &mut line_buf,
            &mut open,
            &mut oversized,
            &mut discarding
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // (wave 27c, D5) an unfinished line longer than the 1 MiB carry cap is discarded up to its
    // end and counted; the carry never grows without bound.
    #[test]
    fn oversized_unfinished_line_is_capped_and_counted() {
        let dir = std::env::temp_dir().join(format!("c3-sg-cap-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("events.jsonl");
        let mut f = File::create(&path).unwrap();
        let delta = |line: &str| crate::engines::codex::codex_tool_delta(line);
        let td: Option<&dyn Fn(&str) -> i64> = Some(&delta);

        let mut offset = 0u64;
        let mut line_buf = String::new();
        let mut open = 0i64;
        let mut oversized = 0u64;
        let mut discarding = false;

        // A run of > 1 MiB with NO line end: the carry is discarded up to its end and counted.
        let blob = "x".repeat(STREAM_CARRY_CAP + 4096);
        f.write_all(blob.as_bytes()).unwrap();
        f.flush().unwrap();
        assert!(read_stream_growth(
            &path,
            &mut offset,
            &mut line_buf,
            td,
            &mut open,
            &mut oversized,
            &mut discarding
        ));
        assert_eq!(oversized, 1, "the over-long line was counted");
        assert!(discarding, "still discarding until the line end");
        assert!(line_buf.is_empty(), "the carry did not grow without bound");

        // The line end arrives with a fresh valid line after it: discarding stops and the count
        // does not rise again.
        writeln!(f, "tail-of-huge-line").unwrap();
        writeln!(
            f,
            r#"{{"type":"item.started","item":{{"type":"command_execution"}}}}"#
        )
        .unwrap();
        f.flush().unwrap();
        assert!(read_stream_growth(
            &path,
            &mut offset,
            &mut line_buf,
            td,
            &mut open,
            &mut oversized,
            &mut discarding
        ));
        assert_eq!(oversized, 1, "no double-count of the same over-long line");
        assert!(!discarding);
        assert_eq!(
            open, 1,
            "the tool-call line after the discard is still parsed"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // (wave 27c, D6) an open tool call suspends the stall timer for at most max(3 x stall, 1800 s).
    #[test]
    fn tool_suspension_cap_is_bounded() {
        assert_eq!(tool_suspension_cap_secs(10), 1800); // 30 < 1800 -> 1800 floor
        assert_eq!(tool_suspension_cap_secs(0), 1800);
        assert_eq!(tool_suspension_cap_secs(1000), 3000); // 3 x 1000
    }

    // (wave 27c, D1) a kick request carries an id written atomically; the member acknowledges with
    // `<kick>.ack` holding that id and the result; the id round-trips through the helpers.
    #[test]
    fn kick_request_and_ack_carry_the_request_id() {
        let dir = std::env::temp_dir().join(format!("c3-kick-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let kick = dir.join(".consult.kick-03");

        write_kick_atomic(&kick, "req-123").unwrap();
        assert_eq!(read_kick_id(&kick).as_deref(), Some("req-123"));

        // The member acknowledges (`stopped`) with the id read from the kick file, then removes it.
        confirm_kick(&kick, "stopped");
        assert!(
            !kick.exists(),
            "the kick file is removed on acknowledgement"
        );
        let ack = read_kick_ack(&kick_ack_path(&kick)).unwrap();
        assert_eq!(ack.result, "stopped");
        assert_eq!(ack.id, "req-123");

        // A `late` acknowledgement of a fresh request carries its own id.
        write_kick_atomic(&kick, "req-456").unwrap();
        confirm_kick(&kick, "late");
        let ack = read_kick_ack(&kick_ack_path(&kick)).unwrap();
        assert_eq!(ack.result, "late");
        assert_eq!(ack.id, "req-456");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
