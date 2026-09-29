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
    /// Called once, right after the child is spawned and before it is waited on, with the
    /// child pid and its start time (.NET `o` string, or empty when unavailable). The
    /// orchestrator uses it to flip the recovery record `launching` -> `running` while the
    /// child is live (`codex-consult.ps1:3330-3339`).
    pub on_running: Option<&'a dyn Fn(u32, String)>,
}

/// The result of one subprocess turn.
#[derive(Debug, Clone)]
pub struct TurnResult {
    /// Whether a process was actually started.
    pub started: bool,
    /// The child's exit code, or `None` when it was killed/timed out or never started.
    pub exit_code: Option<i32>,
    /// The turn was killed on its timeout.
    pub timed_out: bool,
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
            timed_out: false,
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
    let mut timed_out = false;
    let mut survivors = Vec::new();
    let exit_code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) => {
                if start.elapsed() >= req.timeout {
                    survivors = kill_tree(&mut child);
                    // TEST HOOK: CODEX_CONSULT_TEST_SURVIVORS=<pid>[,<pid>] — these pids, when
                    // alive, are reported as survivors of this kill (no test can make a real
                    // process outlive a kill). Only ever adds (a stricter outcome), matching
                    // the plugin's `$env:CODEX_CONSULT_TEST_SURVIVORS` hook.
                    for hook in test_survivor_pids() {
                        if crate::liveness::proc::pid_alive(hook, "") && !survivors.contains(&hook)
                        {
                            survivors.push(hook);
                        }
                    }
                    timed_out = true;
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                return TurnResult {
                    started: true,
                    exit_code: None,
                    timed_out: false,
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
        timed_out,
        survivors,
        wall_seconds,
        stderr: read_text(req.stderr_path),
        error: None,
    }
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
