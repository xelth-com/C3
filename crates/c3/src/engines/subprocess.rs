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
    /// (wave 26b, D12) The ISO time of the last event-stream growth seen, `None` when none.
    pub last_event: Option<String>,
    /// (wave 26b, D12) Seconds without an event at the stall kill (`0` unless a stall fired).
    pub silent_seconds: i64,
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
            last_event: None,
            silent_seconds: 0,
            survivors: Vec::new(),
            wall_seconds: 0.0,
            stderr: String::new(),
            error: Some(error),
        }
    }
}

fn round1(secs: f64) -> f64 {
    (secs * 10.0).round() / 10.0
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
    let exit_code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) => {
                if start.elapsed() >= req.timeout {
                    survivors = kill_now(&mut child);
                    stop = TurnStop::Timeout;
                    break None;
                }
                // (wave 26b, D10) the operator's kick, checked on every poll.
                if let Some(kp) = req.kick_path {
                    if kp.exists() {
                        survivors = kill_now(&mut child);
                        stop = TurnStop::Kick;
                        break None;
                    }
                }
                // (wave 26b, D12 / 26c D3) the stall cut.
                if stall_on {
                    let grew = read_stream_growth(
                        req.events_path,
                        &mut offset,
                        &mut line_buf,
                        req.tool_delta,
                        &mut open_tools,
                    );
                    if grew {
                        last_activity = Instant::now();
                        last_event = Some(iso_now());
                    }
                    // The timer is suspended while a tool call is in flight.
                    if open_tools <= 0 {
                        let silent = last_activity.elapsed();
                        if silent.as_secs() as i64 >= req.stall_sec {
                            silent_seconds = silent.as_secs() as i64;
                            survivors = kill_now(&mut child);
                            stop = TurnStop::Stall;
                            break None;
                        }
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
                    survivors: Vec::new(),
                    wall_seconds: round1(start.elapsed().as_secs_f64()),
                    stderr: read_text(req.stderr_path),
                    error: Some(format!("waiting on the child failed: {e}")),
                };
            }
        }
    };
    let wall_seconds = round1(start.elapsed().as_secs_f64());

    TurnResult {
        started: true,
        exit_code,
        stop,
        last_event,
        silent_seconds,
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
fn read_stream_growth(
    path: &Path,
    offset: &mut u64,
    line_buf: &mut String,
    tool_delta: Option<&dyn Fn(&str) -> i64>,
    open_tools: &mut i64,
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
        line_buf.push_str(&String::from_utf8_lossy(&buf));
        while let Some(nl) = line_buf.find('\n') {
            let line: String = line_buf.drain(..=nl).collect();
            let line = line.trim();
            if !line.is_empty() {
                *open_tools = (*open_tools + delta(line)).max(0);
            }
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
    std::env::var("CODEX_CONSULT_TEST_SURVIVORS")
        .ok()
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

        // No growth yet.
        assert!(!read_stream_growth(
            &path,
            &mut offset,
            &mut line_buf,
            td,
            &mut open
        ));

        // A plain agent-message line: growth, no open tool call.
        writeln!(
            f,
            r#"{{"type":"item.completed","item":{{"type":"agent_message","text":"a"}}}}"#
        )
        .unwrap();
        f.flush().unwrap();
        assert!(read_stream_growth(
            &path,
            &mut offset,
            &mut line_buf,
            td,
            &mut open
        ));
        assert_eq!(open, 0);

        // A tool call starts: the count rises (the timer would suspend).
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
            &mut open
        ));
        assert_eq!(open, 1);

        // It completes: back to zero (the timer resumes).
        writeln!(
            f,
            r#"{{"type":"item.completed","item":{{"type":"command_execution"}}}}"#
        )
        .unwrap();
        f.flush().unwrap();
        assert!(read_stream_growth(
            &path,
            &mut offset,
            &mut line_buf,
            td,
            &mut open
        ));
        assert_eq!(open, 0);

        // No further growth.
        assert!(!read_stream_growth(
            &path,
            &mut offset,
            &mut line_buf,
            td,
            &mut open
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
