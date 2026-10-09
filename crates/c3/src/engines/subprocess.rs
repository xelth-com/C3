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
    /// a tool call is in flight (see `tool_flight`).
    pub stall_sec: i64,
    /// (wave 26b, D10) The operator's kick file: when it appears the turn is stopped and recorded
    /// as `stopped by the operator (-Kick)`. `None` disables the check.
    pub kick_path: Option<&'a Path>,
    /// (wave 26c, D3 / 28b, D12) Classifies one event-stream line for the stall's tool-call
    /// suspension (`Update-ToolFlight`): a tool call OPENS (with its key and label), CLOSES, or the
    /// line is neutral. While a call is open the stall timer is suspended (bounded), and a stall cut
    /// names the open call(s). `None` = no suspension (the timer still resets on byte growth).
    pub tool_flight: Option<&'a dyn Fn(&str) -> ToolFlight>,
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
    /// (wave 27c, D6) Seconds a tool call had been open at the stall kill (`0` when none was open);
    /// the stall outcome names it: `no output for N s (a tool call open for M s: <calls>)`.
    pub tool_open_seconds: i64,
    /// (wave 28b, D12) The tool call(s) open at the stall kill (`codex command_execution item_9`,
    /// `muse tool.shell t1`; several joined by `, `), `""` when none.
    pub open_tools: String,
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
            tool_open_seconds: 0,
            open_tools: String::new(),
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

/// (wave 27c, D6 / 28b, D12 / F36-11, F32-7) An open tool call cannot suspend the stall timer for
/// ever, and no completion event is needed to end the suspension: while a tool call is open the
/// stream must still GROW - once it has not grown for 2 x the stall seconds (no floor; the test hook
/// `CODEX_CONSULT_TEST_TOOL_CAP_SEC` = that bound) the suspension ends and the stall cut follows,
/// naming the open call.
fn tool_suspension_cap_secs(stall_sec: i64) -> i64 {
    if let Some(v) = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_TOOL_CAP_SEC") {
        if let Ok(n) = v.trim().parse::<i64>() {
            if n > 0 {
                return n;
            }
        }
    }
    stall_sec.saturating_mul(2)
}

/// (wave 26c, D3 / 28b, D12) One event line's effect on the tool calls in flight
/// (`Update-ToolFlight`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolFlight {
    /// Not a tool call's start or end.
    None,
    /// A tool call starts: its key (a key ending in `#` is made unique by the tracker - a call
    /// without an id) and the label a stall cut names it by.
    Open { key: String, label: String },
    /// A tool call ends: the open call with `key`, else (none open by that key) the first open call
    /// whose key starts with `fallback_prefix` (`""` = none).
    Close {
        key: String,
        fallback_prefix: String,
    },
}

impl ToolFlight {
    /// The old count delta: `+1` open, `-1` close, `0` neutral.
    pub fn delta(&self) -> i64 {
        match self {
            ToolFlight::None => 0,
            ToolFlight::Open { .. } => 1,
            ToolFlight::Close { .. } => -1,
        }
    }
}

/// The tool calls in flight (a set by key, with their labels).
#[derive(Debug, Clone, Default)]
pub struct ToolCalls {
    open: Vec<(String, String)>,
    seq: u64,
}

impl ToolCalls {
    /// Apply one line's [`ToolFlight`].
    pub fn apply(&mut self, f: ToolFlight) {
        match f {
            ToolFlight::None => {}
            ToolFlight::Open { key, label } => {
                let key = if key.ends_with('#') {
                    self.seq += 1;
                    format!("{key}{}", self.seq)
                } else {
                    key
                };
                if !self.open.iter().any(|(k, _)| *k == key) {
                    self.open.push((key, label));
                }
            }
            ToolFlight::Close {
                key,
                fallback_prefix,
            } => {
                if !key.is_empty() {
                    if let Some(i) = self.open.iter().position(|(k, _)| *k == key) {
                        self.open.remove(i);
                        return;
                    }
                }
                if !fallback_prefix.is_empty() {
                    if let Some(i) = self
                        .open
                        .iter()
                        .position(|(k, _)| k.starts_with(&fallback_prefix))
                    {
                        self.open.remove(i);
                    }
                }
            }
        }
    }

    /// How many tool calls are open.
    pub fn count(&self) -> usize {
        self.open.len()
    }

    /// The open calls' labels, by key (ordinal), joined by `, ` (the plugin's `OpenTools`).
    pub fn labels(&self) -> String {
        let mut v: Vec<&(String, String)> = self.open.iter().collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v.iter()
            .map(|(_, l)| l.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
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
    // (D1) Publish WITHOUT overwriting: `hard_link` fails with `AlreadyExists` when the kick file
    // already exists, matching .NET's `File.Move` (which never overwrites). A second concurrent
    // -Kick caller therefore does NOT clobber the first's request — it re-reads and JOINS that id
    // (the caller's retry loop). `std::fs::rename` would silently replace on Windows, breaking that.
    let res = std::fs::hard_link(&tmp, kick_path);
    let _ = std::fs::remove_file(&tmp);
    res
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

/// Whether a launcher is a Windows batch file (`.cmd` / `.bat`, any case).
pub fn is_batch_launcher(launcher: &str) -> bool {
    Path::new(launcher)
        .extension()
        .map(|e| {
            let e = e.to_string_lossy().to_ascii_lowercase();
            e == "cmd" || e == "bat"
        })
        .unwrap_or(false)
}

/// `ConvertTo-ProcArg`'s bare-token rule: `-` alone, or letters, digits and `_ . - : \ / =` only.
/// The plugin hands such an argument to the launcher as it is (no quotes).
pub fn is_bare_proc_arg(arg: &str) -> bool {
    arg == "-"
        || (!arg.is_empty()
            && arg.chars().all(|c| {
                c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-' | ':' | '\\' | '/' | '=')
            }))
}

/// How one argument reaches a batch launcher's command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchArg {
    /// Appended verbatim (`CommandExt::raw_arg`): a bare token by the plugin's rule.
    Raw(String),
    /// Left to Rust std's batch-file escaping (quoted, `"` doubled, `%` neutralised, a line break
    /// refused).
    Std(String),
}

/// (wave 2b, the launcher quoting) The arguments of a batch launcher (`codex.cmd`, an npm shim, the
/// harness fakes) as the plugin's `Start-Process -ArgumentList` hands them over
/// (`ConvertTo-ProcArg`): a bare token (`-c`, `model_context_window=256000`, `-p=`, a plain path)
/// goes as it is - Rust std's own batch escaping would quote every argument holding `=`
/// (`"model_context_window=256000"`, `"-p="`), which a launcher matching its `%*` text then does
/// not see. Everything else (spaces, quotes, `%`, other symbols) keeps std's escaping: quoted with
/// every `"` doubled - the plugin's form - plus std's guards against `%VAR%` expansion and line
/// breaks, which the plugin lacks.
pub fn batch_args(argv: &[String]) -> Vec<BatchArg> {
    argv.iter()
        .map(|a| {
            if is_bare_proc_arg(a) {
                BatchArg::Raw(a.clone())
            } else {
                BatchArg::Std(a.clone())
            }
        })
        .collect()
}

/// Put `argv` on the launcher's command. The launcher path is always the program and the caller's
/// `argv` are always the args - including for a Windows `.cmd`/`.bat` launcher. We deliberately do
/// **not** wrap a batch launcher in an explicit `cmd /c`: doing so makes `cmd.exe` the program (an
/// `.exe`), so Rust escapes the embedded quotes in each arg the MSVC way (`model_provider=\"ZAI\"`),
/// which is not what a batch file's `%*` expander produces. Handing the `.cmd`/`.bat` path straight
/// to `Command` lets Rust std run it through `cmd.exe` (the child pid is still `cmd.exe`'s, so
/// `kill_tree`/`on_running` are unchanged); the arguments follow [`batch_args`]. An `.exe`
/// launcher's escaping is untouched.
pub fn apply_launcher_args(cmd: &mut Command, launcher: &str, argv: &[String]) {
    #[cfg(windows)]
    {
        if is_batch_launcher(launcher) {
            use std::os::windows::process::CommandExt;
            for a in batch_args(argv) {
                match a {
                    BatchArg::Raw(v) => {
                        cmd.raw_arg(v);
                    }
                    BatchArg::Std(v) => {
                        cmd.arg(v);
                    }
                }
            }
            return;
        }
    }
    let _ = launcher;
    cmd.args(argv);
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

    let program = req.launcher.to_string();
    let mut cmd = Command::new(&program);
    apply_launcher_args(&mut cmd, req.launcher, req.argv);
    cmd.current_dir(req.cwd)
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
    let mut open_tools = ToolCalls::default();
    let mut open_tool_labels = String::new();
    let mut last_event: Option<String> = None;
    let mut silent_seconds: i64 = 0;
    let mut tool_open_seconds: i64 = 0;
    // (wave 27c, D5) over-long unfinished lines discarded, and whether we are skipping the tail of
    // one until its line end. (wave 27c, D6) when the current tool-call suspension began.
    let mut oversized_lines: u64 = 0;
    let mut discarding = false;
    let mut tool_open_since: Option<Instant> = None;
    let kill_now = |child: &mut Child| -> Vec<u32> {
        // (wave 27c, D16 / H4) CODEX_CONSULT_TEST_KILL_DENIED simulates a restricted host where
        // process inspection and taskkill are denied: nothing is enumerated or terminated, the tree
        // is left running (the root becomes an orphan the harness stops), and the run proceeds with
        // the kill unconfirmed. Do NOT wait on the child — it is still alive.
        if c3_core::test_hooks::hook("CODEX_CONSULT_TEST_KILL_DENIED")
            .map(|v| v.trim() == "1")
            .unwrap_or(false)
        {
            let mut s: Vec<u32> = vec![child.id()];
            for hook in test_survivor_pids() {
                if crate::liveness::proc::pid_alive(hook, "") && !s.contains(&hook) {
                    s.push(hook);
                }
            }
            return s;
        }
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
                            req.tool_flight,
                            &mut open_tools,
                            &mut oversized_lines,
                            &mut discarding,
                        );
                        if grew {
                            last_activity = Instant::now();
                            last_event = Some(iso_now());
                        }
                        // (wave 27c, D6) track when the current tool-call suspension began.
                        if open_tools.count() > 0 {
                            tool_open_since.get_or_insert_with(Instant::now);
                        } else {
                            tool_open_since = None;
                        }
                        let silent = last_activity.elapsed().as_secs() as i64;
                        // The timer is suspended while a tool call is in flight — but only up to
                        // 2 x stall with no growth of the stream at all (wave 28b D12).
                        let fire = if open_tools.count() == 0 {
                            silent >= req.stall_sec
                        } else {
                            silent >= tool_suspension_cap_secs(req.stall_sec)
                        };
                        if fire {
                            silent_seconds = silent;
                            if open_tools.count() > 0 {
                                tool_open_seconds = tool_open_since
                                    .map(|t| t.elapsed().as_secs() as i64)
                                    .unwrap_or(0);
                                open_tool_labels = open_tools.labels();
                                eprintln!(
                                    "codex-consult: no output for {silent_seconds} s (a tool call open for {tool_open_seconds} s: {open_tool_labels})"
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
                        tool_open_seconds: 0,
                        open_tools: String::new(),
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
        tool_open_seconds,
        open_tools: open_tool_labels,
        oversized_lines,
        survivors,
        wall_seconds,
        stderr: read_text(req.stderr_path),
        error: None,
    }
}

/// Read any new bytes of the events stream since `offset`, advancing it and the carried
/// partial-line buffer. Applies `tool_flight` to each COMPLETE new line to keep `open_tools`
/// (the tool calls in flight) current. Returns whether the stream grew at all (any new
/// bytes) — the caller resets the silent timer on that (wave 26c D3: reset on byte growth).
#[allow(clippy::too_many_arguments)]
fn read_stream_growth(
    path: &Path,
    offset: &mut u64,
    line_buf: &mut String,
    tool_flight: Option<&dyn Fn(&str) -> ToolFlight>,
    open_tools: &mut ToolCalls,
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
    if let Some(flight) = tool_flight {
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
                open_tools.apply(flight(line));
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
        // Best effort without `taskkill /T`: the descendants from the process table first (the
        // launcher's own children would otherwise outlive it as orphans), then the child.
        for pid in crate::liveness::proc::descendants_of(child.id()) {
            kill_pid_unix(pid);
        }
        let _ = child.kill();
        Vec::new()
    }
}

/// `kill -9 <pid>` through the `kill` binary (no libc dependency); best effort.
#[cfg(not(windows))]
fn kill_pid_unix(pid: u32) {
    let _ = Command::new("kill")
        .args(["-9", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Kill the process tree rooted at `pid` (a tree WE started, by pid — never by name). On Windows
/// `taskkill /F /T /PID <pid>`; on Unix a best-effort `kill`. Used by the registration-failure path
/// where only the child pid is known (the `Child` handle is owned by the running turn).
pub fn kill_tree_by_pid(pid: u32) {
    if pid == 0 {
        return;
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(windows))]
    {
        for d in crate::liveness::proc::descendants_of(pid) {
            kill_pid_unix(d);
        }
        kill_pid_unix(pid);
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
        let flight = |line: &str| crate::engines::codex::codex_tool_flight(line);
        let td: Option<&dyn Fn(&str) -> ToolFlight> = Some(&flight);

        let mut offset = 0u64;
        let mut line_buf = String::new();
        let mut open = ToolCalls::default();
        let mut oversized = 0u64;
        let mut discarding = false;
        let grow = |offset: &mut u64,
                    line_buf: &mut String,
                    open: &mut ToolCalls,
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
        assert_eq!(open.count(), 0);

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
        assert_eq!(open.count(), 1);

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
        assert_eq!(open.count(), 0);

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
        let flight = |line: &str| crate::engines::codex::codex_tool_flight(line);
        let td: Option<&dyn Fn(&str) -> ToolFlight> = Some(&flight);

        let mut offset = 0u64;
        let mut line_buf = String::new();
        let mut open = ToolCalls::default();
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
            open.count(),
            1,
            "the tool-call line after the discard is still parsed"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // (wave 2b) the launcher quoting: a plain argument (the plugin's bare-token set, `=` included)
    // reaches a batch launcher as it is; anything else keeps std's batch escaping.
    #[test]
    fn batch_args_keep_plain_arguments_plain() {
        let argv: Vec<String> = [
            "exec",
            "-c",
            "model_context_window=256000",
            "-p=",
            "-",
            r"C:\dir\file.md",
            "a b",
            r#"model_provider="ZAI""#,
            "%PATH%",
            "x+y",
            "",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let got = batch_args(&argv);
        let raw = |v: &str| BatchArg::Raw(v.to_string());
        let std_ = |v: &str| BatchArg::Std(v.to_string());
        assert_eq!(
            got,
            vec![
                raw("exec"),
                raw("-c"),
                raw("model_context_window=256000"),
                raw("-p="),
                raw("-"),
                raw(r"C:\dir\file.md"),
                std_("a b"),
                std_(r#"model_provider="ZAI""#),
                std_("%PATH%"),
                std_("x+y"),
                std_(""),
            ]
        );
        assert!(is_batch_launcher(r"C:\x\codex.CMD"));
        assert!(is_batch_launcher("run.bat"));
        assert!(!is_batch_launcher(r"C:\x\codex.exe"));
    }

    // (wave 2b) what a batch launcher's `%*` really sees (Windows): a plain `k=v` unquoted, a
    // value with spaces and quotes quoted with every `"` doubled.
    #[cfg(windows)]
    #[test]
    fn a_batch_launcher_sees_the_plugin_argv() {
        let dir = std::env::temp_dir().join(format!("c3-batch-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let bat = dir.join("echo-args.cmd");
        let out = dir.join("args.txt");
        std::fs::write(
            &bat,
            format!("@echo off\r\n>\"{}\" echo %*\r\n", out.display()),
        )
        .unwrap();
        let argv: Vec<String> = [
            "-c",
            "model_context_window=256000",
            "-p=",
            "a b",
            r#"k="v w""#,
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let launcher = bat.to_string_lossy().to_string();
        let mut cmd = Command::new(&launcher);
        apply_launcher_args(&mut cmd, &launcher, &argv);
        let st = cmd.status().unwrap();
        assert!(st.success());
        let seen = std::fs::read_to_string(&out).unwrap();
        assert_eq!(
            seen.trim_end(),
            r#"-c model_context_window=256000 -p= "a b" "k=""v w""""#
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // (wave 2e, RC3) The argv-dumping helper of the test below: this test binary itself, re-run by
    // a batch launcher with `--exact <this test> -- %*` and `C3_TEST_ARGV_DUMP=<file>`. Rust std
    // parses its command line by the MSVC CRT rules (as node.exe behind `codex.cmd` does); the
    // helper writes the arguments after the first `--` as a JSON array. Without the variable it
    // does nothing.
    #[test]
    fn argv_dump_helper() {
        let Ok(out) = std::env::var("C3_TEST_ARGV_DUMP") else {
            return;
        };
        let args: Vec<String> = std::env::args().collect();
        let rest: Vec<String> = match args.iter().position(|a| a == "--") {
            Some(p) => args[p + 1..].to_vec(),
            None => Vec::new(),
        };
        std::fs::write(out, serde_json::to_string(&rest).unwrap()).unwrap();
    }

    // (wave 2e, RC3) What a NATIVE program behind a batch launcher really receives through
    // `apply_launcher_args` + the launcher's `%*`: every supported argument exactly (the plugin's
    // bare tokens raw, a literal `%TEMP%` never expanded, a backslash before a quote, a spaced path
    // ending in a backslash, embedded quotes, an empty argument); a line break is refused at the
    // spawn (nothing starts). The matrix is in docs/port/wave2b-compat.md section 2.
    #[cfg(windows)]
    #[test]
    fn a_native_program_behind_a_batch_launcher_gets_the_exact_argv() {
        let dir = std::env::temp_dir().join(format!("c3-argv-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let helper = std::env::current_exe().unwrap();
        let bat = dir.join("forward.cmd");
        std::fs::write(
            &bat,
            format!(
                "@echo off\r\n\"{}\" --exact engines::subprocess::tests::argv_dump_helper --quiet -- %*\r\nexit /b %ERRORLEVEL%\r\n",
                helper.display()
            ),
        )
        .unwrap();
        let launcher = bat.to_string_lossy().to_string();
        let mut n = 0;
        let mut run = |argv: &[&str]| -> Result<Vec<String>, String> {
            n += 1;
            let out = dir.join(format!("argv-{n}.json"));
            let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
            let mut cmd = Command::new(&launcher);
            apply_launcher_args(&mut cmd, &launcher, &argv);
            cmd.env("C3_TEST_ARGV_DUMP", &out)
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let st = cmd.status().map_err(|e| e.to_string())?;
            assert!(st.success(), "{argv:?}: {st:?}");
            let text = std::fs::read_to_string(&out).map_err(|e| e.to_string())?;
            Ok(serde_json::from_str(&text).unwrap())
        };
        let supported: &[&[&str]] = &[
            // the plugin's bare tokens (raw), a path without spaces ending in a backslash (raw)
            &[
                "-c",
                "model_context_window=256000",
                "-p=",
                "-",
                r"C:\dir\",
                "next",
            ],
            // a literal %TEMP% (alone and inside a quoted value), never expanded
            &["%TEMP%", r#"foo="%TEMP%""#, "100%", "%%"],
            // a backslash before a quote, inside and at the end
            &[r#"a\"b"#, r#"x\\"y"#, r#"end\""#],
            // a spaced path ending in one and in two backslashes, then another argument
            &[r"C:\Program Files\x\", r"C:\Program Files\y\\", "z"],
            // embedded quotes (the plugin's doubled form), spaces, an empty argument, symbols
            &[r#"k="v w""#, "a b", "", "x+y", "a&b|c<d>e^f"],
        ];
        for argv in supported {
            let got = run(argv).unwrap();
            assert_eq!(got, argv.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        }
        // a line break (CR LF, LF, CR) cannot reach a batch file intact: refused at the spawn
        for nl in ["a\r\nb", "a\nb", "a\rb"] {
            let e = run(&["ok", nl]).unwrap_err();
            assert!(
                e.contains("batch file arguments are invalid"),
                "{nl:?}: {e}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // (wave 28b, D12) an open tool call suspends the stall timer for at most 2 x stall without
    // growth - no 1800 s floor.
    #[test]
    fn tool_suspension_cap_is_bounded() {
        assert_eq!(tool_suspension_cap_secs(2), 4);
        assert_eq!(tool_suspension_cap_secs(10), 20);
        assert_eq!(tool_suspension_cap_secs(1000), 2000);
    }

    // (wave 28b, D12) the tracker keeps the open calls by key: a completion closes its own call (by
    // id, else the first open one of its type), a stall names what is still open.
    #[test]
    fn tool_calls_name_the_open_call() {
        let mut t = ToolCalls::default();
        let start = |id: &str| {
            crate::engines::codex::codex_tool_flight(&format!(
                r#"{{"type":"item.started","item":{{"id":"{id}","type":"command_execution"}}}}"#
            ))
        };
        let done = |id: &str| {
            crate::engines::codex::codex_tool_flight(&format!(
                r#"{{"type":"item.completed","item":{{"id":"{id}","type":"command_execution"}}}}"#
            ))
        };
        t.apply(start("item_9"));
        t.apply(start("item_3"));
        assert_eq!(t.count(), 2);
        assert_eq!(
            t.labels(),
            "codex command_execution item_3, codex command_execution item_9"
        );
        t.apply(done("item_3"));
        assert_eq!(t.labels(), "codex command_execution item_9");
        // a completion of a call that is not open changes nothing
        t.apply(done("item_77"));
        assert_eq!(t.count(), 1);
        // calls without an id: closed by their type
        let anon_start = crate::engines::codex::codex_tool_flight(
            r#"{"type":"item.started","item":{"type":"web_search"}}"#,
        );
        let anon_done = crate::engines::codex::codex_tool_flight(
            r#"{"type":"item.completed","item":{"type":"web_search"}}"#,
        );
        t.apply(anon_start.clone());
        t.apply(anon_start);
        assert_eq!(t.count(), 3);
        t.apply(anon_done);
        assert_eq!(t.count(), 2);
        // a muse tool task: proposed opens it, its completion (any kind field) closes it
        let mut m = ToolCalls::default();
        m.apply(crate::engines::muse::muse_tool_flight(
            r#"{"payload_type":"task.lifecycle.proposed","payload":{"event":{"task_kind":"tool.shell","task_id":"t1"}}}"#,
        ));
        m.apply(crate::engines::muse::muse_tool_flight(
            r#"{"payload_type":"task.lifecycle.started","payload":{"event":{"task_id":"t1"}}}"#,
        ));
        assert_eq!(m.labels(), "muse tool.shell t1");
        m.apply(crate::engines::muse::muse_tool_flight(
            r#"{"payload_type":"task.lifecycle.completed","payload":{"event":{"task_id":"t1"}}}"#,
        ));
        assert_eq!(m.count(), 0);
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
