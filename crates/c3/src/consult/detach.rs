//! The detached-run lifecycle behind `--detach`, `--status`, `--wait`, `--prune`, `--id`
//! (wave 25, R12): a port of the `-Detach`/`-DetachId`/`-Status`/`-Wait` paths of
//! `codex-consult.ps1` (`760-1305`). The pure readers/judgement/budget live in
//! [`super::detached`]; this module wires them into `c3 consult`:
//!
//! * the read-only query surface (`--status`/`--wait`/`--prune`), its refusals and exit codes
//!   (0/1/2/3/4), reading only the status files;
//! * the foreground of `--detach` ([`start_detached_run`]): pick a free detach id, write the
//!   `starting` record once, spawn `current_exe() consult ... --detach-id <id>` with the console
//!   redirected to the `.log`, print the three lines, exit 0;
//! * the background of `--detach-id` ([`background`]): the self-report `running` {pid, start_time,
//!   host}, the consultation itself in this process (with the [sink](SINK) set so the run keeps its
//!   members' states and its summary in the status file), the final `done` {exit, summary} on every
//!   exit path (exit 6 when that terminal write fails).
//!
//! The `args` field of the `starting` record is the port's JSON wire (not the plugin's base64
//! CLIXML): a JSON object of the background's options, from which [`options_from_wire`] rebuilds
//! the run.

use std::path::{Path, PathBuf};
#[cfg(not(windows))]
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::args::Options;
use super::detached::{
    detached_paths, judgement, member_guard, read_detached_status, DetachedMember, DetachedPaths,
    DetachedRecord, Judgement, PRUNE_DAYS,
};
use crate::providers;

const TOOL: &str = "codex-consult";

/// Print a refusal (`Stop-WithError`) and return exit 1; remember it as this run's final line.
fn refuse(msg: &str) -> i32 {
    let line = format!("{TOOL}: {msg}");
    eprintln!("{line}");
    note_line(&line);
    1
}

// ------------------------------------------------------------------ small shared helpers

/// This host's name, matching the record's `host` and the elsewhere check (`[Environment]::MachineName`).
pub fn machine_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_default()
}

fn iso_now() -> String {
    chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string()
}

/// Write a status record (atomic replace), stamping `updated` — `Write-DetachedStatus`.
fn write_status(path: &Path, rec: &mut DetachedRecord) -> Result<(), String> {
    rec.updated = Some(iso_now());
    let bytes = super::detached::record_to_bytes(rec).map_err(|e| e.to_string())?;
    c3_core::store::write_text_atomic(path, &bytes).map_err(|e| e.to_string())
}

/// Best-effort remove of a file; returns the error text on failure (else empty).
fn remove_file(path: &Path) -> String {
    if !path.exists() {
        return String::new();
    }
    match std::fs::remove_file(path) {
        Ok(_) => String::new(),
        Err(e) => e.to_string(),
    }
}

// ------------------------------------------------------------------ the run's status-file sink

/// The status file this process (the background) keeps, and the record it maintains as the run
/// progresses. Set once by [`background`]; the orchestrator and the panel scheduler report into it.
struct Sink {
    path: PathBuf,
    record: DetachedRecord,
    /// The last refusal / error line the run printed (the summary of a run that never reached its
    /// own summary block — a refusal after the lock, D3).
    last_line: String,
}

fn sink() -> &'static Mutex<Option<Sink>> {
    static SINK: OnceLock<Mutex<Option<Sink>>> = OnceLock::new();
    SINK.get_or_init(|| Mutex::new(None))
}

/// Whether this process is the background of a detached run (the sink is active).
pub fn is_active() -> bool {
    sink().lock().map(|s| s.is_some()).unwrap_or(false)
}

/// Remember the last refusal / error line (`refuse()` calls this so a run refused after the lock
/// records the refusal as its final summary — D3).
pub fn note_line(line: &str) {
    if let Ok(mut g) = sink().lock() {
        if let Some(s) = g.as_mut() {
            s.last_line = line.to_string();
        }
    }
}

/// Replace the members of the detached run (a panel once its seats are known), and save.
pub fn set_members(members: Vec<DetachedMember>) {
    if let Ok(mut g) = sink().lock() {
        if let Some(s) = g.as_mut() {
            s.record.members = members;
            let _ = write_status(&s.path, &mut s.record);
        }
    }
}

/// Update the member at `position` (mutating it), and save. No-op when the sink is inactive.
pub fn update_member<F: FnOnce(&mut DetachedMember)>(position: i64, f: F) {
    if let Ok(mut g) = sink().lock() {
        if let Some(s) = g.as_mut() {
            if let Some(m) = s.record.members.iter_mut().find(|m| m.position == position) {
                f(m);
                let _ = write_status(&s.path, &mut s.record);
            }
        }
    }
}

/// Set the summary block the run printed (a single run: its outcome lines up to `events file:`; a
/// panel: its `Panel <id8>: ...` block), and save.
pub fn note_summary(lines: &[String]) {
    if let Ok(mut g) = sink().lock() {
        if let Some(s) = g.as_mut() {
            s.record.summary = lines.join("\n");
            let _ = write_status(&s.path, &mut s.record);
        }
    }
}

/// A single run's summary is its outcome lines up to (and including) `events file:` — the same
/// slice the harness's `RunSummary` takes. Truncate the full render there.
pub fn note_single_run_summary(rendered: &[String]) {
    let end = rendered
        .iter()
        .position(|l| l.starts_with("events file:"))
        .map(|i| i + 1)
        .unwrap_or(rendered.len());
    note_summary(&rendered[..end]);
}

// ------------------------------------------------------------------ the JSON args wire

/// The background's options as a JSON object (`ConvertTo-DetachArgs` / `ConvertFrom-DetachArgs`,
/// but a JSON wire rather than base64 CLIXML). Every field a run needs; the paths are absolute and
/// an inline prompt has moved to `prompt_file`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct DetachArgs {
    #[serde(default)]
    collab_dir: String,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    thread: String,
    #[serde(default)]
    brief: String,
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    prompt_file: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    purpose: String,
    #[serde(default)]
    effort: String,
    #[serde(default)]
    sandbox: String,
    #[serde(default)]
    max_words: i64,
    #[serde(default)]
    timeout_sec: i64,
    #[serde(default = "minus_one")]
    continue_sec: i64,
    #[serde(default)]
    range: String,
    #[serde(default)]
    reply_name: String,
    #[serde(default)]
    artifacts: Vec<String>,
    #[serde(default)]
    raw: bool,
    #[serde(default)]
    codex_exe: String,
    #[serde(default)]
    provider: String,
    #[serde(default)]
    key_env: String,
    #[serde(default)]
    base_url: String,
    #[serde(default = "minus_one")]
    pack_budget: i64,
    #[serde(default)]
    native_effort: String,
    #[serde(default)]
    off_peak_only: bool,
    #[serde(default)]
    skip_preflight: bool,
    #[serde(default)]
    codex_config: Vec<String>,
    #[serde(default)]
    schema_transport: String,
    #[serde(default = "one")]
    format_retry: i64,
    #[serde(default)]
    engine: String,
    #[serde(default)]
    engine_exe: String,
    #[serde(default = "one")]
    denial_retry: i64,
    #[serde(default)]
    max_model_steps: i64,
    #[serde(default)]
    panel: bool,
    #[serde(default)]
    panel_all: bool,
    #[serde(default)]
    panel_concurrency: i64,
    #[serde(default)]
    panel_concurrency_given: bool,
    #[serde(default)]
    panel_size: i64,
    #[serde(default)]
    panel_size_given: bool,
    #[serde(default)]
    panel_order: String,
    #[serde(default)]
    panel_seed: String,
    #[serde(default)]
    require: Vec<String>,
    #[serde(default)]
    role: String,
    #[serde(default)]
    roles: Vec<String>,
    #[serde(default)]
    topic: Vec<String>,
}

fn minus_one() -> i64 {
    -1
}
fn one() -> i64 {
    1
}

impl DetachArgs {
    /// The background's options from this call (minus `-Detach`, paths absolute, prompt→file).
    fn from_options(o: &Options, collab_abs: &str, prompt_file: &str) -> DetachArgs {
        // (D8) the background runs in the caller's directory but is handed ABSOLUTE artifact paths,
        // resolved here in the foreground's cwd, so a relative `-Artifact art.bin` from a
        // subdirectory binds to `<cwd>/art.bin` (its raw arg, which the ledger records) rather than
        // a bare relative name the background could misresolve.
        let cwd = std::env::current_dir().unwrap_or_default();
        let artifacts_abs: Vec<String> = o
            .artifacts
            .iter()
            .map(|a| {
                let p = std::path::Path::new(a);
                if a.is_empty() || p.is_absolute() {
                    a.clone()
                } else {
                    cwd.join(p).to_string_lossy().to_string()
                }
            })
            .collect();
        DetachArgs {
            collab_dir: collab_abs.to_string(),
            mode: o.mode.clone(),
            thread: o.thread.clone(),
            brief: o.brief.clone(),
            prompt: if prompt_file.is_empty() {
                o.prompt.clone()
            } else {
                String::new()
            },
            prompt_file: prompt_file.to_string(),
            model: o.model.clone(),
            purpose: o.purpose.clone(),
            effort: o.effort.clone(),
            sandbox: o.sandbox.clone(),
            max_words: o.max_words,
            timeout_sec: o.timeout_sec,
            continue_sec: o.continue_sec,
            range: o.range.clone(),
            reply_name: o.reply_name.clone(),
            artifacts: artifacts_abs,
            raw: o.raw,
            codex_exe: o.codex_exe.clone(),
            provider: o.provider.clone(),
            key_env: o.key_env.clone(),
            base_url: o.base_url.clone(),
            pack_budget: o.pack_budget,
            native_effort: o.native_effort.clone(),
            off_peak_only: o.off_peak_only,
            skip_preflight: o.skip_preflight,
            codex_config: o.codex_config.clone(),
            schema_transport: o.schema_transport.clone(),
            format_retry: o.format_retry,
            engine: o.engine.clone(),
            engine_exe: o.engine_exe.clone(),
            denial_retry: o.denial_retry,
            max_model_steps: o.max_model_steps,
            panel: o.panel,
            panel_all: o.panel_all,
            panel_concurrency: o.panel_concurrency,
            panel_concurrency_given: o.panel_concurrency_given,
            panel_size: o.panel_size,
            panel_size_given: o.panel_size_given,
            panel_order: o.panel_order.clone(),
            panel_seed: o.panel_seed.clone(),
            require: o.require.clone(),
            role: o.role.clone(),
            roles: o.roles.clone(),
            topic: o.topic.clone(),
        }
    }

    fn into_options(self, task: &str) -> Options {
        Options {
            task: task.to_string(),
            collab_dir: if self.collab_dir.is_empty() {
                ".collab".into()
            } else {
                self.collab_dir
            },
            mode: self.mode,
            thread: self.thread,
            brief: self.brief,
            prompt: self.prompt,
            model: self.model,
            purpose: self.purpose,
            effort: self.effort,
            sandbox: self.sandbox,
            max_words: self.max_words,
            timeout_sec: self.timeout_sec,
            continue_sec: self.continue_sec,
            continue_sec_given: self.continue_sec != -1,
            // A detached member re-derives its stall cut from its own roster entry (the panel's
            // explicit --stall-sec is not preserved across detach); -Kick acts by the kick file.
            stall_sec: -1,
            stall_sec_given: false,
            range: self.range,
            reply_name: self.reply_name,
            artifacts: self.artifacts,
            raw: self.raw,
            codex_exe: self.codex_exe,
            provider: self.provider,
            key_env: self.key_env,
            base_url: self.base_url,
            pack_budget: self.pack_budget,
            native_effort: self.native_effort,
            off_peak_only: self.off_peak_only,
            skip_preflight: self.skip_preflight,
            codex_config: self.codex_config,
            schema_transport: self.schema_transport,
            telemetry: Some(false),
            format_retry: self.format_retry,
            dry_run: false,
            engine: self.engine,
            engine_exe: self.engine_exe,
            denial_retry: self.denial_retry,
            max_model_steps: self.max_model_steps,
            panel: self.panel,
            panel_all: self.panel_all,
            panel_concurrency: self.panel_concurrency,
            panel_concurrency_given: self.panel_concurrency_given,
            panel_size: self.panel_size,
            panel_size_given: self.panel_size_given,
            panel_order: self.panel_order,
            panel_seed: self.panel_seed,
            require: self.require,
            role: self.role,
            roles: self.roles,
            topic: self.topic,
            panel_spec: String::new(),
            detach: false,
            status: false,
            id: String::new(),
            id_given: false,
            detach_id: String::new(),
            list: false,
            wait: false,
            wait_timeout_sec: 0,
            wait_timeout_sec_given: false,
            prune: false,
            kick: false,
            member: String::new(),
        }
    }

    /// The prompt file this args wire carries (read by the background before the run).
    fn prompt_file(&self) -> &str {
        &self.prompt_file
    }
}

// ------------------------------------------------------------------ the foreground: Start-DetachedRun

/// A planned member for the `starting` record (its lineage; state `pending`).
pub struct PlannedMember {
    pub position: i64,
    pub lineage: String,
    /// `pending` normally; `skipped` with `not started: ...` for a roster-skipped seat.
    pub state: String,
    pub outcome: String,
}

/// The foreground of `-Detach` once every pre-lock check passed (D1, D5, D8): picks a free detach
/// id, writes the `starting` record once, spawns the background, prints three lines and returns 0
/// — or refuses (exit 1) with nothing left behind.
#[allow(clippy::too_many_arguments)]
pub fn start_detached_run(
    o: &Options,
    kind: &str,
    members: &[PlannedMember],
    budget: i64,
    plan: &str,
    brief_full: &str,
    warnings: &[String],
) -> i32 {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = providers::resolve_repo_root(&cwd);
    let collab_root = providers::resolve_collab_root(&repo_root, &o.collab_dir);
    let collab_abs = collab_root.to_string_lossy().to_string();
    let task_dir = collab_root.join(&o.task);

    // The detach id: a guid whose id8 names no status/log/prompt file of the task yet (D7). TEST
    // HOOK CODEX_CONSULT_TEST_DETACH_GUIDS=<guid>[,<guid>...] is tried first, in that order.
    let mut tries: Vec<String> = Vec::new();
    if let Some(guids) = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_DETACH_GUIDS") {
        for g in guids.split(',') {
            let g = g.trim();
            if is_guid(g) {
                tries.push(g.to_lowercase());
            }
        }
    }
    for _ in 0..16 {
        tries.push(uuid::Uuid::new_v4().to_string());
    }
    if let Err(e) = std::fs::create_dir_all(&task_dir) {
        return refuse(&format!(
            "could not create the task directory '{}' ({e}); nothing was started.",
            task_dir.display()
        ));
    }
    let mut chosen: Option<(String, DetachedPaths)> = None;
    for cand in &tries {
        let p = detached_paths(&task_dir, cand);
        if p.status.exists() || p.log.exists() || p.prompt.exists() {
            continue;
        }
        chosen = Some((cand.clone(), p));
        break;
    }
    let (new_id, paths) = match chosen {
        Some(v) => v,
        None => {
            return refuse(&format!(
                "no free detach id for task '{}' (the status or log file of every candidate exists); nothing was started.",
                o.task
            ))
        }
    };

    // (F11-2) an inline -Prompt moves to <task>/.consult.detached-<id8>.prompt.txt; the record's
    // args name only that file, so a run that never starts leaves no prompt text in its status.
    let mut prompt_file = String::new();
    if !o.prompt.is_empty() {
        prompt_file = paths.prompt.to_string_lossy().to_string();
        if let Err(e) = c3_core::store::write_text_atomic(&paths.prompt, o.prompt.as_bytes()) {
            return refuse(&format!(
                "could not write the prompt file '{prompt_file}' ({e}); nothing was started."
            ));
        }
    }

    let args = DetachArgs::from_options(o, &collab_abs, &prompt_file);
    let args_wire = serde_json::to_string(&args).unwrap_or_default();

    let mut rec = DetachedRecord {
        id: new_id.clone(),
        id8: paths.id8.clone(),
        task: o.task.clone(),
        kind: kind.to_string(),
        state: "starting".into(),
        started: Some(iso_now()),
        host: machine_name(),
        budget_sec: budget,
        purpose: o.purpose.clone(),
        reply_name: o.reply_name.clone(),
        brief: brief_full.to_string(),
        members: members
            .iter()
            .map(|m| DetachedMember {
                position: m.position,
                lineage: m.lineage.clone(),
                state: if m.state.is_empty() {
                    "pending".into()
                } else {
                    m.state.clone()
                },
                outcome: m.outcome.clone(),
                ..Default::default()
            })
            .collect(),
        log: paths.log.to_string_lossy().to_string(),
        args: Some(args_wire),
        ..Default::default()
    };
    if let Err(e) = write_status(&paths.status, &mut rec) {
        if !prompt_file.is_empty() {
            let _ = std::fs::remove_file(&paths.prompt);
        }
        return refuse(&format!(
            "could not write the status file '{}' ({e}); nothing was started.",
            paths.status.display()
        ));
    }

    // Spawn the background: current_exe() consult --task <t> --collab-dir <abs> --detach-id <id>,
    // stdin null, stdout+stderr to the log, detached so the caller's handles close on exit.
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            let _ = std::fs::remove_file(&paths.status);
            if !prompt_file.is_empty() {
                let _ = std::fs::remove_file(&paths.prompt);
            }
            return refuse(&format!(
                "could not find this executable to start the background ({e}); nothing was started."
            ));
        }
    };
    if let Err(e) = spawn_background(&exe, &o.task, &collab_abs, &new_id, &paths.log, &cwd) {
        let _ = std::fs::remove_file(&paths.status);
        if !prompt_file.is_empty() {
            let _ = std::fs::remove_file(&paths.prompt);
        }
        return refuse(&format!("{e}; nothing was started."));
    }

    for w in warnings {
        if !w.is_empty() {
            println!("WARNING: {w}");
        }
    }
    let collab_opt = if o.collab_dir != ".collab" {
        format!(" -CollabDir \"{collab_abs}\"")
    } else {
        String::new()
    };
    println!(
        "Detached {}: {plan} - it runs in the background (detach id {new_id}; budget {budget} s).",
        paths.id8
    );
    println!(
        "status file: {} (console output: {})",
        paths.status.display(),
        paths.log.display()
    );
    println!(
        "come back  : codex-consult.ps1 -Task {}{collab_opt} -Status -Id {} (exit 0 done and usable, 1 a failure, 2 still running); -Wait -Id {} waits until it is done (default: its budget, {budget} s)",
        o.task, paths.id8, paths.id8
    );
    0
}

/// Spawn the detached background so it holds NONE of the caller's handles (the foreground must
/// return at once even when the caller captured it through a pipe).
///
/// On Windows this mirrors the plugin exactly: `cmd.exe /d /v:off /s /c "<exe> consult ... <NUL
/// 1>log 2>&1"` launched with no inherited handles (`bInheritHandles = FALSE`, `DETACHED_PROCESS`)
/// — cmd opens the log itself, so no file handle and no console pipe reaches the background.
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
    // cmd.exe would expand a %NAME% in these paths: refuse rather than corrupt (matches the plugin).
    for p in [&exe_s, collab_abs, &log_s] {
        if p.contains('%') {
            return Err(format!(
                "-Detach starts its background through cmd.exe, which would expand the '%' in '{p}'; run without -Detach"
            ));
        }
    }
    // A trailing backslash before the closing quote would escape it.
    let collab_arg = if collab_abs.ends_with('\\') {
        format!("{collab_abs}.")
    } else {
        collab_abs.to_string()
    };
    let inner = format!(
        "\"{exe_s}\" consult --task {task} --collab-dir \"{collab_arg}\" --detach-id {id} <NUL 1>\"{log_s}\" 2>&1"
    );
    let raw = format!("/d /v:off /s /c \"{inner}\"");
    let comspec = std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".into());
    // SAFETY: a straight CreateProcessW with bInheritHandles=FALSE and DETACHED_PROCESS; every
    // pointer is to a local, NUL-terminated buffer that outlives the call.
    unsafe { create_process_no_inherit(&comspec, &raw, cwd) }
}

#[cfg(windows)]
unsafe fn create_process_no_inherit(
    program: &str,
    raw_args: &str,
    cwd: &Path,
) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    #[allow(non_snake_case)]
    #[repr(C)]
    struct StartupInfoW {
        cb: u32,
        lpReserved: *mut u16,
        lpDesktop: *mut u16,
        lpTitle: *mut u16,
        dwX: u32,
        dwY: u32,
        dwXSize: u32,
        dwYSize: u32,
        dwXCountChars: u32,
        dwYCountChars: u32,
        dwFillAttribute: u32,
        dwFlags: u32,
        wShowWindow: u16,
        cbReserved2: u16,
        lpReserved2: *mut u8,
        hStdInput: isize,
        hStdOutput: isize,
        hStdError: isize,
    }
    #[allow(non_snake_case)]
    #[repr(C)]
    struct ProcessInformation {
        hProcess: isize,
        hThread: isize,
        dwProcessId: u32,
        dwThreadId: u32,
    }
    #[allow(non_snake_case)]
    extern "system" {
        fn CreateProcessW(
            lpApplicationName: *const u16,
            lpCommandLine: *mut u16,
            lpProcessAttributes: *mut u8,
            lpThreadAttributes: *mut u8,
            bInheritHandles: i32,
            dwCreationFlags: u32,
            lpEnvironment: *mut u8,
            lpCurrentDirectory: *const u16,
            lpStartupInfo: *mut StartupInfoW,
            lpProcessInformation: *mut ProcessInformation,
        ) -> i32;
        fn CloseHandle(h: isize) -> i32;
    }
    // CREATE_NO_WINDOW gives cmd its own hidden console (so its `>log` redirection works) without
    // a visible window; CREATE_NEW_PROCESS_GROUP detaches it from the caller's Ctrl-C. With
    // bInheritHandles=FALSE the background still holds none of the caller's handles.
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let mut app: Vec<u16> = std::path::Path::new(program)
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect();
    // The command line: "program" raw_args (program quoted, then the /d /v:off /s /c "...").
    let mut cmdline: Vec<u16> = format!("\"{program}\" {raw_args}")
        .encode_utf16()
        .chain([0])
        .collect();
    let dir: Vec<u16> = cwd.as_os_str().encode_wide().chain([0]).collect();
    let mut si: StartupInfoW = std::mem::zeroed();
    si.cb = std::mem::size_of::<StartupInfoW>() as u32;
    let mut pi: ProcessInformation = std::mem::zeroed();
    let ok = CreateProcessW(
        app.as_mut_ptr(),
        cmdline.as_mut_ptr(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        0, // bInheritHandles = FALSE: the background holds none of our handles
        CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP,
        std::ptr::null_mut(),
        dir.as_ptr(),
        &mut si,
        &mut pi,
    );
    if ok == 0 {
        let code = std::io::Error::last_os_error();
        return Err(format!("could not start the background process ({code})"));
    }
    CloseHandle(pi.hProcess);
    CloseHandle(pi.hThread);
    Ok(())
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
    Command::new(exe)
        .arg("consult")
        .arg("--task")
        .arg(task)
        .arg("--collab-dir")
        .arg(collab_abs)
        .arg("--detach-id")
        .arg(id)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_err))
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not start the background process ({e})"))
}

fn is_guid(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 36 {
        return false;
    }
    let dashes = [8usize, 13, 18, 23];
    for (i, &c) in b.iter().enumerate() {
        if dashes.contains(&i) {
            if c != b'-' {
                return false;
            }
        } else if !c.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

/// The `member_guard`-based budget of a single detached run (one group of one member, D4).
pub fn single_run_budget(timeout_sec: i64, continue_sec: i64, repair: bool, denial: bool) -> i64 {
    let guard = member_guard(timeout_sec, continue_sec, repair, denial);
    super::detached::detached_budget(&[(1, vec![1])], &|_| guard, 0, 120)
}

// ------------------------------------------------------------------ the background: --detach-id

/// The background process of `-Detach` (`--detach-id <guid>`): self-report `running`, run the
/// consultation in this process (sink set), write the final `done` status. Returns the exit code
/// (6 when the terminal write fails).
pub fn background(o: Options, run_flow: impl FnOnce(Options) -> i32) -> i32 {
    if o.detach || o.dry_run || !o.panel_spec.is_empty() {
        return refuse("-DetachId is internal to -Detach; never pass it yourself.");
    }
    if !is_guid(&o.detach_id) {
        return refuse(&format!(
            "-DetachId is internal to -Detach (a detach id is a lowercase guid; got '{}').",
            o.detach_id
        ));
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = providers::resolve_repo_root(&cwd);
    let collab_root = providers::resolve_collab_root(&repo_root, &o.collab_dir);
    let task_dir = collab_root.join(&o.task);
    let paths = detached_paths(&task_dir, &o.detach_id);
    let (mut rec, err) = match read_detached_status(&paths.status) {
        Ok(Some(r)) => (r, String::new()),
        Ok(None) => {
            return refuse(&format!(
                "-DetachId is internal to -Detach: there is no status file {}.",
                paths.status.display()
            ))
        }
        Err(e) => return refuse(&format!("-DetachId is internal to -Detach: {e}")),
    };
    let _ = err;
    if rec.id != o.detach_id {
        return refuse(&format!(
            "-DetachId is internal to -Detach: the status file {} belongs to detach id {}.",
            paths.status.display(),
            rec.id
        ));
    }
    if !(rec.state == "starting" && rec.pid.is_none()) {
        return refuse(&format!(
            "-DetachId is internal to -Detach: the status file {} is in state {}{}, not this process's.",
            paths.status.display(),
            rec.state,
            rec.pid.map(|p| format!(" (pid {p})")).unwrap_or_default()
        ));
    }

    // (D5) the self-report first: running, this process, this host.
    let pid = std::process::id();
    let args_text = rec.args.take().unwrap_or_default();
    rec.state = "running".into();
    rec.pid = Some(pid as i64);
    rec.start_time = crate::liveness::proc::process_start_iso(pid);
    rec.host = machine_name();
    if let Err(e) = write_status(&paths.status, &mut rec) {
        eprintln!(
            "{TOOL}: the detached run could not report to its status file {} ({e}); nothing was started.",
            paths.status.display()
        );
        return 1;
    }
    println!(
        "{TOOL}: detached run {} (detach id {}) - pid {pid} on {}, started {}; status file {}",
        paths.id8,
        o.detach_id,
        machine_name(),
        iso_now(),
        paths.status.display()
    );

    // Install the sink so the run keeps its members and summary in the status file, then run.
    if let Ok(mut g) = sink().lock() {
        *g = Some(Sink {
            path: paths.status.clone(),
            record: rec.clone(),
            last_line: String::new(),
        });
    }

    let mut line = String::new();
    let code = match decode_and_run(&o.task, &args_text, run_flow) {
        Ok(c) => c,
        Err(e) => {
            line = format!(
                "{TOOL}: the detached run stopped on an error: {}",
                c3_core::one_line(&e)
            );
            eprintln!("{line}");
            1
        }
    };

    // The final status (D3): the record as the run left it (in the sink), made final.
    let mut final_rec = sink()
        .lock()
        .ok()
        .and_then(|mut g| g.take())
        .map(|s| {
            if line.is_empty() && !s.last_line.is_empty() {
                line = s.last_line.clone();
            }
            s.record
        })
        .unwrap_or(rec);
    complete_record(&mut final_rec, code, &line);
    match write_status_retry(&paths.status, &mut final_rec) {
        Ok(()) => code,
        Err(_) => {
            eprintln!(
                "{TOOL}: the detached run {} ended with exit {code}, but its status file could not be made final - the result exists only in this log ({}); -Status will judge the run by its background (exit 6).",
                paths.id8,
                paths.log.display()
            );
            6
        }
    }
}

fn decode_and_run(
    task: &str,
    args_text: &str,
    run_flow: impl FnOnce(Options) -> i32,
) -> Result<i32, String> {
    if args_text.trim().is_empty() {
        return Err("the status file carries no arguments for the background".into());
    }
    let mut args: DetachArgs =
        serde_json::from_str(args_text).map_err(|e| format!("its arguments are unusable: {e}"))?;
    // (F11-2) the prompt from its file, then the file goes.
    if !args.prompt_file().is_empty() {
        let pf = args.prompt_file.clone();
        let pfp = Path::new(&pf);
        if !pfp.is_file() {
            return Err(format!("its prompt file '{pf}' is gone"));
        }
        args.prompt = std::fs::read_to_string(pfp).map_err(|e| e.to_string())?;
        args.prompt_file = String::new();
        let _ = std::fs::remove_file(pfp);
    }
    let o = args.into_options(task);
    Ok(run_flow(o))
}

/// `Complete-DetachedRecord` (D3): make a record final and fold its unfinished members.
fn complete_record(rec: &mut DetachedRecord, exit: i32, line: &str) {
    rec.state = "done".into();
    rec.exit = Some(exit as i64);
    if rec.finished.is_none() {
        rec.finished = Some(iso_now());
    }
    if rec.wall_seconds.is_none() {
        if let Some(st) = rec.started.as_deref().and_then(parse_when) {
            let secs = (Utc::now() - st.with_timezone(&Utc)).num_milliseconds() as f64 / 1000.0;
            rec.wall_seconds = Some((secs.max(0.0) * 10.0).round() / 10.0);
        }
    }
    if rec.summary.is_empty() {
        rec.summary = if !line.is_empty() {
            line.to_string()
        } else {
            format!(
                "{TOOL}: the detached run ended with exit {exit} without a summary; its console output is in {}",
                rec.log
            )
        };
    }
    let why = {
        let first = rec.summary.split('\n').next().unwrap_or("");
        let stripped = first.strip_prefix("codex-consult:").unwrap_or(first).trim();
        let mut w = c3_core::one_line(stripped);
        if w.chars().count() > 200 {
            w = format!("{}...", w.chars().take(197).collect::<String>());
        }
        w
    };
    for m in &mut rec.members {
        if m.state == "pending" {
            m.state = "skipped".into();
            m.outcome = format!("not started: {why}");
        } else if m.state == "running" {
            m.state = "failed".into();
            m.outcome = format!("stopped: the run ended (exit {exit}) before this member finished");
        }
    }
}

/// A terminal status write, up to 3 attempts 250 ms apart (`Write-DetachedStatusRetry`, F08-2).
fn write_status_retry(path: &Path, rec: &mut DetachedRecord) -> Result<(), String> {
    let mut last = String::new();
    for attempt in 1..=3 {
        match write_status(path, rec) {
            Ok(()) => return Ok(()),
            Err(e) => last = e,
        }
        if attempt < 3 {
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    }
    Err(last)
}

fn parse_when(s: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .or_else(|| chrono::DateTime::parse_from_rfc3339(&s.replacen(' ', "T", 1)).ok())
}

// ------------------------------------------------------------------ the query surface: -Status / -Wait / -Prune

/// A detached run of a task: its status file, id8, parsed record (or read error), and log path.
struct RunEntry {
    path: PathBuf,
    id8: String,
    record: Option<DetachedRecord>,
    error: String,
    log: PathBuf,
    when: chrono::DateTime<Utc>,
}

/// `Read-DetachedRuns`: every detached run of a task directory, newest first (by `started`, else
/// the file's last-write; then id8 ascending).
fn read_detached_runs(task_dir: &Path) -> Vec<RunEntry> {
    let mut runs: Vec<RunEntry> = Vec::new();
    let rd = match std::fs::read_dir(task_dir) {
        Ok(rd) => rd,
        Err(_) => return runs,
    };
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let id8 = match parse_status_name(&name) {
            Some(v) => v,
            None => continue,
        };
        let path = entry.path();
        let (record, error) = match read_detached_status(&path) {
            Ok(r) => (r, String::new()),
            Err(e) => (None, e),
        };
        let when = record
            .as_ref()
            .and_then(|r| r.started.as_deref())
            .and_then(parse_when)
            .map(|d| d.with_timezone(&Utc))
            .or_else(|| {
                std::fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .ok()
                    .map(chrono::DateTime::<Utc>::from)
            })
            .unwrap_or_else(|| chrono::DateTime::<Utc>::from(std::time::UNIX_EPOCH));
        let log = task_dir.join(format!(".consult.detached-{id8}.log"));
        runs.push(RunEntry {
            path,
            id8,
            record,
            error,
            log,
            when,
        });
    }
    runs.sort_by(|a, b| b.when.cmp(&a.when).then(a.id8.cmp(&b.id8)));
    runs
}

/// The id8 of a `.consult.detached-<id8>.status.json` file name (else `None`).
fn parse_status_name(name: &str) -> Option<String> {
    let rest = name.strip_prefix(".consult.detached-")?;
    let id8 = rest.strip_suffix(".status.json")?;
    if !id8.is_empty() && id8.chars().all(|c| c.is_ascii_alphanumeric()) {
        Some(id8.to_lowercase())
    } else {
        None
    }
}

fn judge(run: &RunEntry, now: chrono::DateTime<Utc>) -> Judgement {
    judgement(run.record.as_ref(), &run.error, now, &machine_name())
}

/// A `-Status` / `-Wait` query. Reads status files only (`-Prune` deletes old ones); no lock, no
/// roster, no launcher. Returns the exit code (0/1/2/3/4).
pub fn query(o: &Options) -> i32 {
    if o.detach {
        return status_query_refusal(
            "-Detach does not go with -Status or -Wait: -Detach starts a consultation, -Status and -Wait look at detached ones.",
        );
    }
    // Only the query options are allowed (`codex-consult.ps1:1103`). Which ones were "given" is
    // known from the *_given flags and the non-empty run fields (the shim forwards only bound
    // parameters for a query).
    let mut extra: Vec<&str> = Vec::new();
    if !o.mode.is_empty() {
        extra.push("Mode");
    }
    if !o.thread.is_empty() {
        extra.push("Thread");
    }
    if !o.brief.is_empty() {
        extra.push("Brief");
    }
    if !o.prompt.is_empty() {
        extra.push("Prompt");
    }
    if !o.provider.is_empty() {
        extra.push("Provider");
    }
    if !o.model.is_empty() {
        extra.push("Model");
    }
    if !o.purpose.is_empty() {
        extra.push("Purpose");
    }
    if o.panel || o.panel_all {
        extra.push("Panel");
    }
    if !o.engine.is_empty() {
        extra.push("Engine");
    }
    if !extra.is_empty() {
        extra.sort_unstable();
        return status_query_refusal(&format!(
            "-Status and -Wait take only -Task, -CollabDir, -Id, -Prune (with -Status) and -WaitTimeoutSec (with -Wait); not -{}.",
            extra.join(", -")
        ));
    }
    if o.prune && o.wait {
        return status_query_refusal(
            "-Prune goes with -Status, not with -Wait (-Status -Prune is the one form that writes: it deletes old files).",
        );
    }
    if o.wait_timeout_sec_given {
        if !o.wait {
            return status_query_refusal("-WaitTimeoutSec goes with -Wait.");
        }
        if o.wait_timeout_sec <= 0 {
            return status_query_refusal(&format!(
                "-WaitTimeoutSec must be greater than 0 (got {}); leave it out for the run's own budget.",
                o.wait_timeout_sec
            ));
        }
    }
    if o.task.is_empty() || !c3_core::task_slug::is_slug(&o.task) {
        return status_query_refusal(
            "-Task must be a slug (letters, digits, dot, dash, underscore).",
        );
    }
    let want = o.id.trim().to_lowercase();
    if o.id_given && !is_id_prefix(&want) {
        return status_query_refusal(&format!(
            "-Id takes a detach id or its beginning (hexadecimal, as -Detach printed it); got '{}'.",
            o.id
        ));
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = providers::resolve_repo_root(&cwd);
    let collab_root = providers::resolve_collab_root(&repo_root, &o.collab_dir);
    // -CollabDir given but missing: the id of a detached run goes with -Id (a bare positional id
    // binds to -CollabDir).
    if o.collab_dir != ".collab" && !collab_root.is_dir() {
        return status_query_refusal(&format!(
            "-CollabDir '{}' does not exist ({}); the id of a detached run goes with -Id (-Status -Id <id8>).",
            o.collab_dir,
            collab_root.display()
        ));
    }
    let task_dir = collab_root.join(&o.task);

    let select = |want: &str| -> Vec<RunEntry> {
        let w8: String = {
            let mut s = want.replace('-', "");
            if s.len() > 8 {
                s.truncate(8);
            }
            s
        };
        read_detached_runs(&task_dir)
            .into_iter()
            .filter(|r| {
                if want.is_empty() {
                    return true;
                }
                if !r.id8.starts_with(&w8) {
                    return false;
                }
                if want.len() > 8 {
                    return r
                        .record
                        .as_ref()
                        .map(|rec| rec.id.to_lowercase().starts_with(want))
                        .unwrap_or(false);
                }
                true
            })
            .collect()
    };

    let mut runs = select(&want);
    if !want.is_empty() {
        if runs.is_empty() {
            let all = read_detached_runs(&task_dir);
            let known: Vec<String> = all.iter().map(|r| r.id8.clone()).collect();
            return status_query_refusal(&format!(
                "no detached run of task '{}' has an id starting with '{want}' ({}).",
                o.task,
                if known.is_empty() {
                    "it has none".to_string()
                } else {
                    format!("its detached runs: {}", known.join(", "))
                }
            ));
        }
        if runs.len() > 1 {
            return status_query_refusal(&format!(
                "-Id '{want}' matches {} detached runs of task '{}': {}; give more of the id.",
                runs.len(),
                o.task,
                runs.iter()
                    .map(|r| r.id8.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }

    if o.prune {
        prune(&runs);
        runs = select(&want);
    }

    let mut timed_out = false;
    let mut wait_limit = 0i64;
    let mut limit_text = String::new();
    if o.wait {
        let is_open = |r: &RunEntry| {
            matches!(
                judge(r, Utc::now()).state.as_str(),
                "running" | "starting" | "elsewhere"
            )
        };
        let waiting_ids: Vec<String> = runs
            .iter()
            .filter(|r| is_open(r))
            .map(|r| r.id8.clone())
            .collect();
        if !waiting_ids.is_empty() {
            if o.wait_timeout_sec > 0 {
                wait_limit = o.wait_timeout_sec;
                limit_text = "-WaitTimeoutSec".into();
            } else {
                wait_limit = runs
                    .iter()
                    .filter(|r| waiting_ids.contains(&r.id8))
                    .map(|r| r.record.as_ref().map(|x| x.budget_sec).unwrap_or(0))
                    .max()
                    .unwrap_or(0);
                if wait_limit <= 0 {
                    wait_limit = 3600;
                }
                limit_text = format!(
                    "the budget of the run{}",
                    if waiting_ids.len() > 1 { "s" } else { "" }
                );
            }
            println!(
                "{TOOL}: waiting for {} detached run{} of task '{}' ({}) - up to {wait_limit} s ({limit_text}), checking every 2 s",
                waiting_ids.len(),
                if waiting_ids.len() != 1 { "s" } else { "" },
                o.task,
                waiting_ids.join(", ")
            );
            let watch = Instant::now();
            loop {
                runs = select(&want);
                let still: Vec<&RunEntry> = runs
                    .iter()
                    .filter(|r| waiting_ids.contains(&r.id8) && is_open(r))
                    .collect();
                if still.is_empty() {
                    break;
                }
                let left = wait_limit as f64 - watch.elapsed().as_secs_f64();
                if left <= 0.0 {
                    timed_out = true;
                    break;
                }
                let ms = (left * 1000.0).clamp(100.0, 2000.0) as u64;
                std::thread::sleep(std::time::Duration::from_millis(ms));
            }
        }
    }

    if runs.is_empty() {
        println!(
            "{TOOL}: no detached consultation in task '{}' ({}).",
            o.task,
            task_dir.display()
        );
        return 0;
    }
    let now = Utc::now();
    write_report(&runs, now);
    if timed_out {
        println!();
        println!(
            "{TOOL}: still running after {wait_limit} s ({limit_text}); the run was not touched - -Wait again, or -Status later."
        );
        return 3;
    }
    // The worst state decides: 2 running > 1 failed/died > 0 all done and usable.
    let mut worst = 0;
    for r in &runs {
        let e = judge(r, now).exit;
        if e > worst {
            worst = e;
        }
    }
    worst
}

/// `-Status -Prune` (D6): delete the files of done/died/never-started/unreadable runs last written
/// more than 7 days ago.
fn prune(runs: &[RunEntry]) {
    let now = Utc::now();
    for run in runs {
        let j = judge(run, now);
        if !["done", "died", "never-started", "unreadable"].contains(&j.state.as_str()) {
            continue;
        }
        let last: Option<chrono::DateTime<Utc>> = if j.state == "unreadable" {
            std::fs::metadata(&run.path)
                .and_then(|m| m.modified())
                .ok()
                .map(chrono::DateTime::<Utc>::from)
        } else {
            let rec = run.record.as_ref();
            ["finished", "updated", "started"].iter().find_map(|f| {
                rec.and_then(|r| match *f {
                    "finished" => r.finished.as_deref(),
                    "updated" => r.updated.as_deref(),
                    _ => r.started.as_deref(),
                })
                .and_then(parse_when)
                .map(|d| d.with_timezone(&Utc))
            })
        };
        let last = match last {
            Some(l) if (now - l).num_seconds() as f64 / 86400.0 >= PRUNE_DAYS as f64 => l,
            _ => continue,
        };
        // (F07-2) a never-started run stays if its log was written within the window (a late
        // background may have started).
        if j.state == "never-started" && run.log.is_file() {
            if let Some(log_last) = std::fs::metadata(&run.log)
                .and_then(|m| m.modified())
                .ok()
                .map(chrono::DateTime::<Utc>::from)
            {
                if (now - log_last).num_seconds() as f64 / 86400.0 < PRUNE_DAYS as f64 {
                    continue;
                }
            }
        }
        let prompt = run
            .path
            .parent()
            .map(|d| d.join(format!(".consult.detached-{}.prompt.txt", run.id8)))
            .unwrap_or_default();
        let mut errs: Vec<String> = Vec::new();
        for f in [&run.path, &run.log, &prompt] {
            let e = remove_file(f);
            if !e.is_empty() {
                errs.push(e);
            }
        }
        if !errs.is_empty() {
            println!(
                "{TOOL}: could not prune detached {}: {}",
                run.id8,
                errs.join("; ")
            );
        } else {
            println!(
                "pruned     : detached {} ({}, last written {}): its status file and log were removed",
                run.id8,
                j.state,
                last.with_timezone(&chrono::Local)
                    .format("%Y-%m-%dT%H:%M:%S%:z")
            );
        }
    }
}

fn status_query_refusal(message: &str) -> i32 {
    eprintln!("{TOOL}: {message}");
    4
}

fn is_id_prefix(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() || b.len() > 36 {
        return false;
    }
    if !b[0].is_ascii_hexdigit() {
        return false;
    }
    b[1..].iter().all(|&c| c.is_ascii_hexdigit() || c == b'-')
}

/// `Write-DetachedReport`: per run its state, one line per member, and (done) the summary block.
fn write_report(runs: &[RunEntry], now: chrono::DateTime<Utc>) {
    let mut first = true;
    for run in runs {
        if !first {
            println!();
        }
        first = false;
        let j = judge(run, now);
        let rec = match &run.record {
            Some(r) => r,
            None => {
                println!("detached {}: {}", run.id8, j.text);
                let file_age = std::fs::metadata(&run.path)
                    .and_then(|m| m.modified())
                    .ok()
                    .map(|t| {
                        (now - chrono::DateTime::<Utc>::from(t)).num_seconds() as f64 / 86400.0
                    });
                if file_age.map(|d| d >= PRUNE_DAYS as f64).unwrap_or(false) {
                    let age = file_age.unwrap() * 86400.0;
                    println!(
                        "  -Status -Prune removes it (last written {} ago)",
                        super::detached::format_span(age)
                    );
                } else {
                    println!(
                        "  -Status -Prune removes it {PRUNE_DAYS} days after its last write; to remove it now: Remove-Item -LiteralPath '{}', '{}' (the log may say what happened)",
                        run.path.display(),
                        run.log.display()
                    );
                }
                continue;
            }
        };
        let mut what = if rec.kind == "panel" {
            "review panel".to_string()
        } else {
            "single run".to_string()
        };
        if !rec.purpose.is_empty() {
            what.push_str(&format!(", purpose {}", rec.purpose));
        }
        if !rec.reply_name.is_empty() {
            what.push_str(&format!(", reply name {}", rec.reply_name));
        }
        println!("detached {} ({what}): {}", run.id8, j.text);
        let pid_text = match rec.pid {
            Some(p) if p > 0 => format!("pid {p} on {}", rec.host),
            _ => format!("no background pid yet (host {})", rec.host),
        };
        let wall_text = if j.state == "done" {
            rec.wall_seconds
                .map(|w| format!("; wall {} s", fmt_num(w)))
                .unwrap_or_default()
        } else {
            String::new()
        };
        println!(
            "  detach id {}, started {}, {pid_text}; budget {} s{wall_text}",
            rec.id,
            rec.started.as_deref().unwrap_or(""),
            rec.budget_sec
        );
        for m in &rec.members {
            let mut outcome = m.outcome.clone();
            let prefix = format!("{}: ", m.state);
            if let Some(rest) = outcome.strip_prefix(&prefix) {
                outcome = rest.to_string();
            }
            if outcome == m.state {
                outcome = String::new();
            }
            let mut details: Vec<String> = Vec::new();
            if let Some(n) = m.n {
                details.push(format!("n={n}"));
            }
            if !m.handoff.is_empty() {
                details.push(format!("handoff {}", m.handoff));
            }
            if let Some(w) = m.wall_seconds {
                details.push(format!("{} s", fmt_num(w)));
            }
            println!(
                "  #{} {} - {}{}{}",
                m.position,
                m.lineage,
                m.state,
                if outcome.is_empty() {
                    String::new()
                } else {
                    format!(": {outcome}")
                },
                if details.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", details.join(", "))
                }
            );
        }
        let log = if rec.log.is_empty() {
            run.log.to_string_lossy().to_string()
        } else {
            rec.log.clone()
        };
        println!("  log: {log}");
        if j.state == "done" && !rec.summary.is_empty() {
            println!();
            for l in rec.summary.split('\n') {
                println!("{l}");
            }
        }
    }
}

/// A number as the plugin prints it (`3.2`, `2`), dropping a trailing `.0`.
fn fmt_num(v: f64) -> String {
    if (v - v.round()).abs() < f64::EPSILON {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{v}");
        s
    }
}

// ------------------------------------------------------------------ the hook phrase (Get-DetachedPhrase)

/// `Get-DetachedPhrase`: the SessionStart hook's one phrase over every task of the collab root.
pub fn detached_phrase(
    collab_root: &Path,
    now: chrono::DateTime<Utc>,
    finished_hours: i64,
) -> String {
    if !collab_root.is_dir() {
        return String::new();
    }
    let mut counts = [0i64; 3]; // running, finished, died
    let mut tasks: [Vec<String>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let dirs = match std::fs::read_dir(collab_root) {
        Ok(d) => d,
        Err(_) => return String::new(),
    };
    for entry in dirs.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let task = entry.file_name().to_string_lossy().to_string();
        for run in read_detached_runs(&entry.path()) {
            let j = judge(&run, now);
            let cat = match j.state.as_str() {
                "running" | "elsewhere" | "starting" => 0,
                "died" | "never-started" | "unreadable" => 2,
                "done" => {
                    let fin = run
                        .record
                        .as_ref()
                        .and_then(|r| r.finished.as_deref())
                        .and_then(parse_when)
                        .map(|d| d.with_timezone(&Utc));
                    match fin {
                        Some(f) if (now - f).num_seconds() < finished_hours * 3600 => 1,
                        _ => continue,
                    }
                }
                _ => continue,
            };
            counts[cat] += 1;
            if !tasks[cat].contains(&task) {
                tasks[cat].push(task.clone());
            }
        }
    }
    let names = ["running", "finished", "died"];
    let mut parts: Vec<(usize, String)> = Vec::new();
    for cat in 0..3 {
        if counts[cat] == 0 {
            continue;
        }
        let t = &tasks[cat];
        let mut shown = t.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
        if t.len() > 3 {
            shown.push_str(", ...");
        }
        let word = if t.len() == 1 { "task" } else { "tasks" };
        parts.push((cat, format!("{} ({word} {shown})", names[cat])));
    }
    if parts.is_empty() {
        return String::new();
    }
    let total: i64 = counts.iter().sum();
    if parts.len() == 1 {
        let (_, p) = &parts[0];
        return format!(
            "; {total} detached consultation{} {p}",
            if total != 1 { "s" } else { "" }
        );
    }
    let with_counts: Vec<String> = parts
        .iter()
        .map(|(cat, p)| format!("{} {p}", counts[*cat]))
        .collect();
    format!("; detached consultations: {}", with_counts.join(", "))
}

/// `Format-DetachedListLine`: the `findings --list` line for a not-done run of a task ('' for a
/// done one). Reads the task's status files.
pub fn list_lines(task_dir: &Path, task: &str, now: chrono::DateTime<Utc>) -> Vec<String> {
    read_detached_runs(task_dir)
        .into_iter()
        .filter_map(|run| {
            let line = super::detached::list_line(
                run.record.as_ref(),
                &run.error,
                &run.id8,
                task,
                now,
                &machine_name(),
            );
            if line.is_empty() {
                None
            } else {
                Some(line)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detach_args_round_trip_json() {
        let mut o = Options {
            task: "t".into(),
            collab_dir: ".collab".into(),
            prompt: "the ask".into(),
            reply_name: "sd".into(),
            format_retry: 1,
            continue_sec: -1,
            ..Default::default()
        };
        o.artifacts = vec!["C:/a b/x.bin".into(), "y\"z".into()];
        let a = DetachArgs::from_options(&o, "C:/repo/.collab", "C:/repo/.collab/t/p.txt");
        // an inline prompt goes to the file; the wire carries the file, not the text.
        assert!(a.prompt.is_empty());
        assert_eq!(a.prompt_file, "C:/repo/.collab/t/p.txt");
        let wire = serde_json::to_string(&a).unwrap();
        assert!(!wire.contains("the ask"));
        let back: DetachArgs = serde_json::from_str(&wire).unwrap();
        let o2 = back.into_options("t");
        assert_eq!(o2.reply_name, "sd");
        assert_eq!(o2.artifacts.len(), 2);
        assert_eq!(o2.collab_dir, "C:/repo/.collab");
    }

    #[test]
    fn single_run_budget_default() {
        // timeout 900, continuation 900, repair on -> guard 2280, budget 2400.
        assert_eq!(single_run_budget(900, 900, true, false), 2400);
    }

    #[test]
    fn guid_and_id_prefix() {
        assert!(is_guid("abcdef12-3456-4000-8000-000000000000"));
        assert!(!is_guid("abcdef12"));
        assert!(is_id_prefix("abcd"));
        assert!(is_id_prefix("abcd1234-0000"));
        assert!(!is_id_prefix("xyz!"));
    }

    #[test]
    fn status_name_parse() {
        assert_eq!(
            parse_status_name(".consult.detached-abcd1234.status.json").as_deref(),
            Some("abcd1234")
        );
        assert!(parse_status_name(".consult.detached-abcd1234.log").is_none());
        assert!(parse_status_name("sessions.json").is_none());
    }

    #[test]
    fn complete_record_folds_members() {
        let mut rec = DetachedRecord {
            id: "x".into(),
            state: "running".into(),
            started: Some(iso_now()),
            members: vec![
                DetachedMember {
                    position: 1,
                    state: "usable".into(),
                    ..Default::default()
                },
                DetachedMember {
                    position: 2,
                    state: "pending".into(),
                    ..Default::default()
                },
                DetachedMember {
                    position: 3,
                    state: "running".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        complete_record(
            &mut rec,
            1,
            "codex-consult: another consultation is running: x",
        );
        assert_eq!(rec.state, "done");
        assert_eq!(rec.exit, Some(1));
        assert!(rec.finished.is_some());
        assert_eq!(rec.members[0].state, "usable");
        assert_eq!(rec.members[1].state, "skipped");
        assert!(rec.members[1]
            .outcome
            .starts_with("not started: another consultation"));
        assert_eq!(rec.members[2].state, "failed");
        assert!(rec.members[2]
            .outcome
            .starts_with("stopped: the run ended (exit 1)"));
        assert!(rec
            .summary
            .starts_with("codex-consult: another consultation"));
    }
}
