//! `c3 maintain` — C3 keeps the engine CLIs and the seated model ids fresh by itself.
//!
//! The operator's wish, verbatim: "встроить в сам C3 чтобы он не спрашивал меня а время от времени
//! обновлял движки и модели" — periodic, automatic, no questions, default ON with an off switch.
//!
//! A pass (`c3 maintain --run`, or the SessionStart hook's detached spawn when one is due) walks the
//! engines (codex, grok, kimi, agy, muse, claude): resolves each launcher, learns the installed and
//! the latest version, and — when a newer one exists and the installed binary smokes clean — updates
//! it, re-smokes, and rolls back on a failure. Then it collects the model ids per source and, in
//! `swap` mode, seats a seated id's successor after the same smoke. Everything external goes through
//! the injectable [`env::Env`] so the whole decision logic is unit-tested with no CLI, network or
//! model turn. State lives under `<codex home>/c3/maintain/`.
//!
//! The off switch mirrors `update`: `C3_MAINTAIN=off` (environment wins) or `"maintain": false` in
//! the C3 config. The hook never spawns under `CODEX_CONSULT_TEST_MODE` unless a test opts in with
//! `C3_MAINTAIN_TEST_ALLOW_SPAWN=1`.

pub mod engine;
pub mod env;
pub mod models;
pub mod smoke;
pub mod version;
pub mod web;

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use env::{Env, SystemEnv};

/// The default interval between passes (days), overridable with `C3_MAINTAIN_DAYS`.
const DEFAULT_INTERVAL_DAYS: i64 = 7;
/// The default supply-chain cool-down (days), overridable with `C3_MAINTAIN_MIN_AGE_DAYS`.
const DEFAULT_MIN_AGE_DAYS: i64 = 3;

// --------------------------------------------------------------------------- state

/// What the last maintenance pass recorded, under `<codex home>/c3/maintain/state.json`. Written
/// atomically after every engine, so a crash mid-pass leaves a consistent file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct State {
    /// RFC3339 completion of the last finished pass (empty: never run).
    #[serde(default)]
    pub last_pass: String,
    /// RFC3339 instant the next pass is due (empty: due now).
    #[serde(default)]
    pub next_due: String,
    /// Whether the hook has already printed the last finished pass's report line.
    #[serde(default)]
    pub reported: bool,
    /// The one-line report of the last finished pass (what the hook prints).
    #[serde(default)]
    pub report: String,
    /// Per engine: the installed version and the last action.
    #[serde(default)]
    pub engines: Vec<EngineRec>,
    /// The last-seen model ids per source.
    #[serde(default)]
    pub models: models::ModelsState,
    /// A short summary of the last model findings (swaps / new ids).
    #[serde(default)]
    pub model_findings: String,
    /// The full model report of the last model step (successors seated or not and why, "new in the
    /// family" ids per seated entry, new list ids) - printed by `c3 maintain`; the hook line is short.
    #[serde(default)]
    pub model_notes: Vec<String>,
}

/// The maintenance state of one engine.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EngineRec {
    pub name: String,
    pub installed: String,
    pub action: String,
    pub when: String,
}

/// `<codex home>/c3/maintain/`. `None` when no codex home resolves.
pub fn state_dir() -> Option<PathBuf> {
    let home = crate::providers::get_codex_home();
    if home.is_empty() {
        return None;
    }
    Some(Path::new(&home).join("c3").join("maintain"))
}

fn state_path(dir: &Path) -> PathBuf {
    dir.join("state.json")
}

fn read_state(dir: &Path) -> State {
    std::fs::read(state_path(dir))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// (0.2.5 R2, F13-6) The model part of the maintain state on disk (the catalogs' ids and dates, the
/// recorded older pairs) - empty when there is no state or it cannot be read. The repository
/// `AGENTS.md` repair reads its date evidence here.
pub fn models_state() -> models::ModelsState {
    state_dir()
        .map(|d| read_state(&d).models)
        .unwrap_or_default()
}

/// Write the state via a temp file and rename, so a reader never sees a half-written file.
fn write_state(dir: &Path, s: &State) {
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let Ok(bytes) = serde_json::to_vec_pretty(s) else {
        return;
    };
    let path = state_path(dir);
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, &bytes).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

fn rfc3339(now: DateTime<Utc>) -> String {
    now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

// --------------------------------------------------------------------------- the pending-update journal

/// An in-progress update recorded (atomically) just before the install command runs and cleared
/// once the post-update smoke (and any rollback) is resolved. A pass that finds one for an engine
/// handles it first, so a crash between a successful update and its post-update smoke never leaves an
/// unverified new CLI that the next pass calls "up to date".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PendingUpdate {
    pub engine: String,
    pub previous: String,
    pub target: String,
    pub when: String,
}

fn pending_path(dir: &Path) -> PathBuf {
    dir.join("pending.json")
}

fn read_pending(dir: &Path) -> Vec<PendingUpdate> {
    std::fs::read(pending_path(dir))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Write the pending-update journal atomically (temp + rename), so a crash mid-write never corrupts
/// it. (R2) Every persistence failure is PROPAGATED - the caller must not run the installer when the
/// record it relies on for crash recovery was never written to disk.
fn write_pending(dir: &Path, list: &[PendingUpdate]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let bytes = serde_json::to_vec(list)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let path = pending_path(dir);
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, &path)
}

/// Record (or replace) the pending update for one engine. (R2) Returns the persistence error so the
/// caller can fail closed before the install.
fn set_pending(dir: &Path, rec: PendingUpdate) -> std::io::Result<()> {
    let mut list = read_pending(dir);
    list.retain(|p| p.engine != rec.engine);
    list.push(rec);
    write_pending(dir, &list)
}

/// Clear the pending update for one engine (a no-op when there is none).
fn clear_pending(dir: &Path, engine: &str) {
    let mut list = read_pending(dir);
    let before = list.len();
    list.retain(|p| p.engine != engine);
    if list.len() != before {
        // A clear-failure is safe (a stale record is re-smoked and resolved on the next pass), so it
        // is best-effort - unlike the write-before-install, whose failure must fail the pass closed.
        let _ = write_pending(dir, &list);
    }
}

// --------------------------------------------------------------------------- switches

/// The feature is ON by default; `C3_MAINTAIN=off` (environment wins) or `"maintain": false` in the
/// C3 config turns it off. Mirrors `update::check_enabled`.
pub fn enabled() -> bool {
    if let Ok(v) = std::env::var("C3_MAINTAIN") {
        if v.trim().eq_ignore_ascii_case("off") {
            return false;
        }
        return true;
    }
    match crate::c3config::load() {
        Ok(Some(cfg)) => cfg.maintain,
        _ => true,
    }
}

fn interval_days() -> i64 {
    env_i64("C3_MAINTAIN_DAYS", DEFAULT_INTERVAL_DAYS).max(0)
}

fn min_age_days() -> i64 {
    env_i64("C3_MAINTAIN_MIN_AGE_DAYS", DEFAULT_MIN_AGE_DAYS).max(0)
}

fn env_i64(name: &str, default: i64) -> i64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok())
        .unwrap_or(default)
}

/// The engines never touched (`C3_MAINTAIN_SKIP=kimi,grok`).
fn skip_set() -> Vec<String> {
    std::env::var("C3_MAINTAIN_SKIP")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

/// The model step mode (`swap` | `announce` | `off`, default `swap`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelsMode {
    Swap,
    Announce,
    Off,
}

fn models_mode() -> ModelsMode {
    match std::env::var("C3_MAINTAIN_MODELS")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "announce" => ModelsMode::Announce,
        "off" => ModelsMode::Off,
        _ => ModelsMode::Swap,
    }
}

/// The explicit smoke target for an engine (`C3_MAINTAIN_SMOKE_<ENGINE>=provider::model`).
fn smoke_override(name: &str) -> String {
    std::env::var(format!("C3_MAINTAIN_SMOKE_{}", name.to_ascii_uppercase())).unwrap_or_default()
}

// --------------------------------------------------------------------------- due logic

/// Whether a pass is due now: never run (`next_due` empty and `last_pass` empty) or `now >=
/// next_due`. `force` ignores the interval entirely (handled by the caller).
pub fn is_due(s: &State, now: DateTime<Utc>) -> bool {
    if s.next_due.trim().is_empty() {
        return true; // no schedule recorded → due
    }
    match DateTime::parse_from_rfc3339(&s.next_due) {
        Ok(t) => now >= t.with_timezone(&Utc),
        Err(_) => true,
    }
}

/// The next-due instant after a pass: `+1 day` when an engine was skipped in use (re-try soon),
/// else `+interval`.
fn next_due(now: DateTime<Utc>, interval: i64, in_use_seen: bool) -> DateTime<Utc> {
    let days = if in_use_seen { 1 } else { interval.max(1) };
    now + chrono::Duration::days(days)
}

// --------------------------------------------------------------------------- the lock

/// A held pass lock (`<state dir>/lock`). Removed on drop if it still holds this process's pid.
pub struct PassLock {
    path: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct LockRec {
    pid: u32,
    start_time: String,
    host: String,
    /// RFC3339 instant the lock was created, so a lock whose owner died without clearing it (and
    /// whose pid cannot be proven dead) is taken over once it is older than a generous bound.
    #[serde(default)]
    created: String,
}

/// A lock older than this with no provably-live owner is treated as abandoned and taken over. A real
/// pass finishes in minutes; this bound is far above any legitimate run.
const LOCK_STALE_HOURS: i64 = 6;

impl Drop for PassLock {
    fn drop(&mut self) {
        // Only remove the lock if it is still ours (a taken-over stale lock stays ours).
        if let Ok(bytes) = std::fs::read(&self.path) {
            if let Ok(rec) = serde_json::from_slice::<LockRec>(&bytes) {
                if rec.pid != std::process::id() {
                    return;
                }
            }
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

enum LockErr {
    /// The lock file already exists (another racer won the atomic create).
    Exists,
    Io(std::io::Error),
}

/// Create the lock by atomic exclusive creation (`create_new`), so exactly one of two racing passes
/// wins. `Err(Exists)` when the file already exists.
fn try_create_lock(path: &Path) -> Result<PassLock, LockErr> {
    use std::io::Write as _;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut f) => {
            let me = std::process::id();
            let rec = LockRec {
                pid: me,
                start_time: crate::liveness::proc::process_start_iso(me).unwrap_or_default(),
                host: c3_core::host::machine_name(),
                created: rfc3339(Utc::now()),
            };
            let bytes = serde_json::to_vec(&rec).unwrap_or_default();
            f.write_all(&bytes).map_err(LockErr::Io)?;
            Ok(PassLock {
                path: path.to_path_buf(),
            })
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(LockErr::Exists),
        Err(e) => Err(LockErr::Io(e)),
    }
}

/// Whether the existing lock is stale (its owner is provably dead, or it is unreadable garbage, or
/// it is older than [`LOCK_STALE_HOURS`]); if so, remove it so the caller can retry the atomic
/// create. Returns `true` when it took the lock away.
fn take_over_if_stale(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false; // gone already: the caller's retry will create it
    };
    let stale = match serde_json::from_slice::<LockRec>(&bytes) {
        Ok(rec) => {
            let dead = rec.pid != std::process::id()
                && !crate::liveness::proc::pid_alive(rec.pid, &rec.start_time);
            let too_old = DateTime::parse_from_rfc3339(&rec.created)
                .map(|t| (Utc::now() - t.with_timezone(&Utc)).num_hours() >= LOCK_STALE_HOURS)
                .unwrap_or(false);
            dead || too_old
        }
        Err(_) => true, // unparsable garbage: never a live owner
    };
    if stale {
        std::fs::remove_file(path).is_ok()
    } else {
        false
    }
}

/// The holder message of a live lock (best-effort).
fn lock_holder_msg(path: &Path) -> String {
    match std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<LockRec>(&b).ok())
    {
        Some(rec) => format!(
            "a maintenance pass is already running (pid {} on {})",
            rec.pid, rec.host
        ),
        None => "a maintenance pass is already running".to_string(),
    }
}

/// Acquire the pass lock by atomic creation. `Err` when another live pass holds it; a stale lock
/// (dead pid, unreadable, or older than a generous bound) is taken over and the create retried once,
/// so exactly one of two racers ever wins.
fn acquire_lock(dir: &Path) -> Result<PassLock, String> {
    if std::fs::create_dir_all(dir).is_err() {
        return Err("could not create the maintain state directory".to_string());
    }
    let path = dir.join("lock");
    for attempt in 0..2 {
        match try_create_lock(&path) {
            Ok(lock) => return Ok(lock),
            Err(LockErr::Io(e)) => return Err(format!("could not write the lock ({e})")),
            Err(LockErr::Exists) => {
                // Someone else created it first. Take it over only if it is provably stale, then
                // retry the atomic create exactly once; otherwise the holder wins.
                if attempt == 0 && take_over_if_stale(&path) {
                    continue;
                }
                return Err(lock_holder_msg(&path));
            }
        }
    }
    Err(lock_holder_msg(&path))
}

// --------------------------------------------------------------------------- the in-use guard

/// The generic runtimes a CLI may run behind (an npm shim is `cmd` -> `node`; a PowerShell shim is
/// `powershell`): such a process names what it runs only through its command line.
const GENERIC_RUNTIMES: &[&str] = &[
    "node",
    "nodejs",
    "bun",
    "deno",
    "cmd",
    "powershell",
    "pwsh",
    "sh",
    "bash",
    "dash",
    "zsh",
    "python",
    "python3",
];

/// An image name without a trailing `.exe` (any case), lowercased.
fn strip_exe_lower(name: &str) -> String {
    let cut = name.len().saturating_sub(4);
    match name.get(cut..) {
        Some(tail) if tail.eq_ignore_ascii_case(".exe") => name[..cut].to_ascii_lowercase(),
        _ => name.to_ascii_lowercase(),
    }
}

/// The command-line markers that identify this engine behind a generic runtime: a path segment
/// containing the launcher stem (`\grok\`, `/grok.`), and the npm package for codex. The broad codex
/// package marker is dropped under test mode, where the fake codex launcher has a distinct stem and a
/// real codex on the machine must not be mistaken for the fake.
fn cmdline_markers(name: &str, stem: &str, test_mode: bool) -> Vec<String> {
    let mut v = vec![
        format!("\\{stem}\\"),
        format!("/{stem}/"),
        format!("\\{stem}."),
        format!("/{stem}."),
    ];
    if name == "codex" && !test_mode {
        v.push("@openai/codex".to_string());
        v.push("@openai\\codex".to_string());
    }
    v
}

/// Whether a CLI of this engine is running other than this pass's own process and children — a live
/// consultation or `c3 work` run on the engine shows up as such a process. Matches by image name AND
/// by command line (an npm/PowerShell-shimmed CLI runs as `node`/`powershell` whose image name is
/// not the launcher stem). A FAILED process snapshot reads as "in use" (fail-closed): the pass must
/// not update while it cannot prove the engine idle.
fn engine_in_use(name: &str, launcher: &str) -> bool {
    let me = std::process::id();
    let mut mine: std::collections::HashSet<u32> = crate::liveness::proc::descendants_of(me)
        .into_iter()
        .collect();
    mine.insert(me);
    // Gather (pid, image-name, command-line); a command line is fetched only for the generic
    // runtimes that need it. A snapshot failure is Err.
    let rows: Result<Vec<(u32, String, String)>, ()> =
        match crate::liveness::proc::enumerate_processes_checked() {
            Ok(procs) => Ok(procs
                .iter()
                .map(|p| {
                    let cmd = if GENERIC_RUNTIMES.contains(&strip_exe_lower(&p.name).as_str()) {
                        crate::liveness::proc::process_command_line(p.pid)
                    } else {
                        String::new()
                    };
                    (p.pid, p.name.clone(), cmd)
                })
                .collect()),
            Err(_) => Err(()),
        };
    in_use_from(
        name,
        launcher,
        c3_core::test_hooks::mode_on(),
        &mine,
        rows.as_ref().map(|v| v.as_slice()).map_err(|_| ()),
    )
}

/// The pure in-use decision over a gathered snapshot (so it is unit-tested without the machine): a
/// failed snapshot (`Err`) is in use; otherwise an image-name match of the launcher stem, or a
/// generic-runtime process whose command line names the launcher path or an engine marker.
fn in_use_from(
    name: &str,
    launcher: &str,
    test_mode: bool,
    mine: &std::collections::HashSet<u32>,
    snapshot: Result<&[(u32, String, String)], ()>,
) -> bool {
    let snapshot = match snapshot {
        Ok(s) => s,
        Err(()) => return true, // a failed process snapshot reads as "in use" (fail-closed)
    };
    let stem = Path::new(launcher)
        .file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if stem.is_empty() {
        return false;
    }
    let launcher_l = launcher.to_ascii_lowercase();
    let markers = cmdline_markers(name, &stem, test_mode);
    for (pid, pname, cmd) in snapshot {
        if mine.contains(pid) {
            continue;
        }
        let pstem = strip_exe_lower(pname);
        if pstem == stem {
            return true;
        }
        if GENERIC_RUNTIMES.contains(&pstem.as_str()) && !cmd.is_empty() {
            let cl = cmd.to_ascii_lowercase();
            if cl.contains(&launcher_l) || markers.iter().any(|m| cl.contains(m)) {
                return true;
            }
        }
    }
    false
}

// --------------------------------------------------------------------------- the report line

/// Compose the one-line pass report (the hook line and `state.report`). Bounded: the new-ids list is
/// capped at 5 (then `+N more`) and the whole line at 400 chars.
pub fn format_report_line(
    date: &str,
    engine_clauses: &[String],
    model_clauses: &[String],
    new_ids: &[String],
) -> String {
    // Every dynamic piece (versions, ids, failure reasons) is one-lined, stripped of control
    // characters and length-capped, so a hostile model id or a multi-line CLI error can neither
    // smuggle a second `c3:` line into the coordinator session nor blow the line up.
    let mut parts: Vec<String> = Vec::new();
    parts.extend(
        engine_clauses
            .iter()
            .filter(|c| !c.is_empty())
            .map(|c| one_line_capped(c, 160)),
    );
    parts.extend(
        model_clauses
            .iter()
            .filter(|c| !c.is_empty())
            .map(|c| one_line_capped(c, 160)),
    );
    if !new_ids.is_empty() {
        let shown: Vec<String> = new_ids
            .iter()
            .take(5)
            .map(|i| one_line_capped(i, 120))
            .collect();
        let more = if new_ids.len() > 5 {
            format!(" (+{} more)", new_ids.len() - 5)
        } else {
            String::new()
        };
        parts.push(format!("new models: {}{more}", shown.join(", ")));
    }
    let body = if parts.is_empty() {
        "nothing to do".to_string()
    } else {
        parts.join("; ")
    };
    let mut line = format!("c3: maintenance {date} - {body}");
    if line.chars().count() > 400 {
        line = format!("{}...", line.chars().take(397).collect::<String>());
    }
    line
}

/// One-line a dynamic report piece: replace every control character (newlines included) with a
/// space, collapse runs of whitespace, and cap the result at `cap` characters.
fn one_line_capped(s: &str, cap: usize) -> String {
    let swapped: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let collapsed = swapped.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > cap {
        format!(
            "{}...",
            collapsed
                .chars()
                .take(cap.saturating_sub(3))
                .collect::<String>()
        )
    } else {
        collapsed
    }
}

// --------------------------------------------------------------------------- the pass

/// Options for a pass.
#[derive(Debug, Clone, Default)]
pub struct PassOpts {
    /// The engines to touch (empty = all).
    pub engines: Vec<String>,
    pub dry_run: bool,
    pub force: bool,
    /// `--models`: run ONLY the model step (no CLI is checked or updated) and leave the schedule
    /// (`next_due`, `last_pass`) and the engine records as they are.
    pub models_only: bool,
}

/// `c3 maintain --run`: run one pass in the foreground. Exit 0 always (a pass never fails the
/// caller); 1 only when the state directory cannot be resolved or another pass is running.
pub fn run_pass(opts: PassOpts) -> i32 {
    let now = Utc::now();
    let Some(dir) = state_dir() else {
        eprintln!("c3 maintain: no codex home - set CODEX_HOME");
        return 1;
    };
    // (F15-6) `C3_MAINTAIN_MODELS=off` with `--models`: one line and no file touched - read before
    // the pass lock is taken and before the state directory is created.
    if opts.models_only && models_mode() == ModelsMode::Off {
        println!("{MODELS_OFF_LINE}");
        return 0;
    }
    // A dry run is read-only: no lock, no state directory, no state file - it must leave nothing
    // behind that a later hook could mistake for a completed pass.
    let lock = if opts.dry_run {
        None
    } else {
        match acquire_lock(&dir) {
            Ok(l) => Some(l),
            Err(e) => {
                println!("c3 maintain: {e}");
                return 1;
            }
        }
    };
    let env = SystemEnv;
    let code = pass_inner(&env, &dir, &opts, now);
    drop(lock);
    code
}

/// The body of a pass, over an injected [`Env`]. Separated so unit tests drive it with a fake.
fn pass_inner(env: &dyn Env, dir: &Path, opts: &PassOpts, now: DateTime<Utc>) -> i32 {
    if opts.models_only {
        return models_pass(env, dir, opts, now);
    }
    let mut state = read_state(dir);
    let roster = crate::providers::read_reviewer_roster().unwrap_or_default();
    let skip = skip_set();
    let only: Vec<String> = opts
        .engines
        .iter()
        .map(|e| e.to_ascii_lowercase())
        .collect();
    let self_exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "c3".to_string());

    let mut engine_clauses: Vec<String> = Vec::new();
    let mut new_engines: Vec<EngineRec> = Vec::new();
    let mut in_use_seen = false;

    for &name in engine::ENGINES {
        if !only.is_empty() && !only.contains(&name.to_string()) {
            continue;
        }
        if skip.contains(&name.to_string()) {
            continue;
        }
        let Some(launcher) = engine::resolve_launcher(name) else {
            // not installed → skipped silently
            continue;
        };
        let installed = engine::installed_version(env, &launcher);
        let installed_text = installed
            .as_ref()
            .map(|v| v.text().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let caps = engine::caps(name);

        // record-only engines (muse, claude): record the version, never update.
        if !caps.updatable {
            new_engines.push(EngineRec {
                name: name.to_string(),
                installed: installed_text.clone(),
                action: "self-updates; version recorded".to_string(),
                when: rfc3339(now),
            });
            println!("c3 maintain: {name} {installed_text} (self-updates; recorded)");
            write_state_engines(dir, &mut state, &new_engines, opts.dry_run);
            continue;
        }

        // in-use guard: a live consultation / work run on this engine → skip, due again tomorrow.
        if engine_in_use(name, &launcher) {
            in_use_seen = true;
            let action = "skipped: in use".to_string();
            println!("c3 maintain: {name} {action}");
            engine_clauses.push(format!("{name} skipped: in use"));
            new_engines.push(EngineRec {
                name: name.to_string(),
                installed: installed_text.clone(),
                action,
                when: rfc3339(now),
            });
            write_state_engines(dir, &mut state, &new_engines, opts.dry_run);
            continue;
        }

        // A pending update from an interrupted pass is resolved before anything else (a real pass
        // only; a dry run touches nothing).
        if !opts.dry_run {
            if let Some(p) = read_pending(dir).into_iter().find(|p| p.engine == name) {
                let (clause, action, resolved) = handle_pending(
                    env,
                    name,
                    &launcher,
                    &installed_text,
                    installed.as_ref(),
                    &caps,
                    &roster,
                    dir,
                    &self_exe,
                    opts.force,
                    now,
                    &p,
                );
                if !clause.is_empty() {
                    engine_clauses.push(format!("{name} {clause}"));
                }
                new_engines.push(EngineRec {
                    name: name.to_string(),
                    installed: installed_text.clone(),
                    action,
                    when: rfc3339(now),
                });
                if resolved {
                    clear_pending(dir, name);
                }
                write_state_engines(dir, &mut state, &new_engines, opts.dry_run);
                continue;
            }
        }

        let (clause, action) = maintain_one_engine(
            env,
            name,
            &launcher,
            &installed_text,
            installed.as_ref(),
            &caps,
            &roster,
            dir,
            &self_exe,
            opts,
            now,
        );
        // An in-use re-check right before the update (inside maintain_one_engine) can also trip.
        if action == "skipped: in use" {
            in_use_seen = true;
        }
        if !clause.is_empty() {
            engine_clauses.push(format!("{name} {clause}"));
        }
        new_engines.push(EngineRec {
            name: name.to_string(),
            installed: installed_text.clone(),
            action,
            when: rfc3339(now),
        });
        write_state_engines(dir, &mut state, &new_engines, opts.dry_run);
    }

    // The model step.
    let outcome = run_models(env, &roster, &mut state, &self_exe, dir, opts);

    // Compose and store the report.
    let date = now.format("%Y-%m-%d").to_string();
    let report = format_report_line(&date, &engine_clauses, &outcome.clauses, &outcome.new_ids);
    println!("{report}");

    // A dry run leaves the state untouched (no file to mislead a later hook); a real pass records
    // the engines, the report and the schedule.
    if !opts.dry_run {
        state.engines = new_engines;
        state.model_findings = outcome.findings;
        state.model_notes = outcome.notes;
        state.report = report;
        state.reported = false;
        state.last_pass = rfc3339(now);
        state.next_due = rfc3339(next_due(now, interval_days(), in_use_seen));
        write_state(dir, &state);
    }
    0
}

/// The one line `c3 maintain --models` prints under `C3_MAINTAIN_MODELS=off`.
const MODELS_OFF_LINE: &str = "c3 maintain: the model step is off (C3_MAINTAIN_MODELS=off) - --models fetched nothing; set C3_MAINTAIN_MODELS=announce or swap to run it";

/// `c3 maintain --models`: the model step alone, on request. No CLI is checked or updated; the
/// schedule (`next_due`, `last_pass`) and the engine records stay as they are. A real run stores the
/// collected ids, the report (printed once by the next SessionStart hook) and the notes; a dry run
/// stores nothing.
fn models_pass(env: &dyn Env, dir: &Path, opts: &PassOpts, now: DateTime<Utc>) -> i32 {
    // (F02-18) `C3_MAINTAIN_MODELS=off` is authoritative: an explicit `--models` fetches nothing
    // (no engine, endpoint or public list) and writes nothing, and says why. (F15-6) `run_pass`
    // answers it before the lock and the state directory; this is the same answer for a direct call.
    if models_mode() == ModelsMode::Off {
        println!("{MODELS_OFF_LINE}");
        return 0;
    }
    let mut state = read_state(dir);
    let roster = crate::providers::read_reviewer_roster().unwrap_or_default();
    let self_exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "c3".to_string());
    let outcome = run_models(env, &roster, &mut state, &self_exe, dir, opts);
    let date = now.format("%Y-%m-%d").to_string();
    let report = format_report_line(&date, &[], &outcome.clauses, &outcome.new_ids);
    println!("{report}");
    if !opts.dry_run {
        state.model_findings = outcome.findings;
        state.model_notes = outcome.notes;
        state.report = report;
        state.reported = false;
        write_state(dir, &state);
    }
    0
}

/// Persist the engine records built so far (atomic per-engine write, so a crash is consistent). A
/// dry run writes nothing - it must leave the state untouched.
fn write_state_engines(dir: &Path, state: &mut State, engines: &[EngineRec], dry_run: bool) {
    state.engines = engines.to_vec();
    if !dry_run {
        write_state(dir, state);
    }
}

/// Maintain one updatable, in-use-clear engine. Returns (report clause without the name, the state
/// action text). Prints one line per step.
#[allow(clippy::too_many_arguments)]
fn maintain_one_engine(
    env: &dyn Env,
    name: &str,
    launcher: &str,
    installed_text: &str,
    installed: Option<&version::CliVersion>,
    caps: &engine::Caps,
    roster: &c3_core::roster::Roster,
    dir: &Path,
    self_exe: &str,
    opts: &PassOpts,
    now: DateTime<Utc>,
) -> (String, String) {
    if !caps.has_check {
        let msg = "report only (no check / rollback path)".to_string();
        println!("c3 maintain: {name} {installed_text} - {msg}");
        return (String::new(), msg);
    }
    let latest = engine::check_latest(env, name, launcher);
    let Some(latest_v) = latest.version.clone() else {
        let msg = if latest.note.is_empty() {
            "latest unknown".to_string()
        } else {
            latest.note.clone()
        };
        println!("c3 maintain: {name} {installed_text} - {msg}");
        return (String::new(), msg);
    };
    // An unparsable installed version has no safe update/rollback target (an "unknown" pin cannot
    // install and would roll back to "unknown"): hold, install nothing.
    let Some(inst) = installed else {
        let msg = "held: installed version unknown".to_string();
        println!("c3 maintain: {name} {installed_text} {msg}");
        return (format!("{installed_text} {msg}"), msg);
    };
    // up to date
    if &latest_v <= inst {
        println!("c3 maintain: {name} up to date ({installed_text})");
        return ("up to date".to_string(), "up to date".to_string());
    }
    let new_text = latest_v.text().to_string();

    // supply-chain cool-down: a version younger than the floor is held (--force ignores it).
    if let Some(released) = latest.released {
        let age = (now - released).num_days();
        if !opts.force && age < min_age_days() {
            let msg = format!("held: released {age} days ago");
            println!("c3 maintain: {name} {installed_text} -> {new_text} {msg}");
            return (format!("{installed_text} -> {new_text} {msg}"), msg);
        }
    }

    // dry-run: say what a real pass would do, install nothing.
    if opts.dry_run {
        let msg = format!("would update {installed_text} -> {new_text}");
        println!("c3 maintain: {name} {msg} (dry-run)");
        return (format!("{installed_text} -> {new_text} (dry-run)"), msg);
    }

    // A smoke target is required before touching anything.
    let target = smoke::pick_target(&smoke_override(name), roster, name, now);
    let Some(target) = target else {
        let msg = "held: no quota for a smoke".to_string();
        println!("c3 maintain: {name} {installed_text} -> {new_text} {msg}");
        return (format!("{installed_text} -> {new_text} {msg}"), msg);
    };

    // 1. smoke the installed version first. (F11-7) An unproven smoke holds exactly like a
    // failed one.
    let roster_text = roster_text_of(roster);
    if let Some((word, why)) =
        run_one_smoke(env, self_exe, &target, name, now, roster_text.as_deref()).hold()
    {
        let verb = if word == "failed" {
            "fails"
        } else {
            "is unproven"
        };
        let msg = format!("held: smoke {verb} on the installed version ({why})");
        println!("c3 maintain: {name} {installed_text} {msg}");
        return (
            format!("{installed_text} held: smoke {verb} on the installed version"),
            msg,
        );
    }

    // An in-use re-check immediately before the update: a consultation may have started during the
    // (minutes-long) installed-version smoke.
    if engine_in_use(name, launcher) {
        let msg = "skipped: in use".to_string();
        println!("c3 maintain: {name} {msg} (a consultation started during the smoke)");
        return ("skipped: in use".to_string(), msg);
    }

    // Record the in-progress update BEFORE the install runs, so a crash before the post-update smoke
    // finishes is recovered on the next pass instead of being read as "up to date". (R2) If the record
    // cannot be persisted, the recovery guarantee is void: hold and run no installer (fail closed).
    if let Err(e) = set_pending(
        dir,
        PendingUpdate {
            engine: name.to_string(),
            previous: installed_text.to_string(),
            target: new_text.clone(),
            when: rfc3339(now),
        },
    ) {
        let msg = format!("held: could not record the pending update ({e})");
        println!("c3 maintain: {name} {installed_text} -> {new_text} {msg}");
        return (format!("{installed_text} -> {new_text} {msg}"), msg);
    }
    let (clause, action, resolved) = update_and_verify(
        env,
        name,
        launcher,
        installed_text,
        inst,
        &latest_v,
        &new_text,
        caps,
        &target,
        roster,
        self_exe,
        dir,
        now,
    );
    // (R1) Clear the pending record ONLY when the update is genuinely resolved - the target passed
    // its smoke, or the previous version is verified restored. A failed installer (which may have
    // mutated the install), an update that did not take, or a failed rollback KEEPS the record so the
    // next pass re-probes and recovers instead of calling a half-installed CLI "up to date".
    if resolved {
        clear_pending(dir, name);
    }
    (clause, action)
}

/// The update + post-update verify/smoke/rollback. `inst` is the parsed previous version; `latest_v`
/// the checked target. Returns (report clause without the name, state action, RESOLVED) - (R1)
/// `resolved` is true only when the engine is left in a known-good state (the target smoked clean, or
/// the previous version is verified restored); every failure path returns false so the caller keeps
/// the pending-update record for the next pass to recover.
#[allow(clippy::too_many_arguments)]
fn update_and_verify(
    env: &dyn Env,
    name: &str,
    launcher: &str,
    installed_text: &str,
    inst: &version::CliVersion,
    latest_v: &version::CliVersion,
    new_text: &str,
    caps: &engine::Caps,
    target: &smoke::Target,
    roster: &c3_core::roster::Roster,
    self_exe: &str,
    dir: &Path,
    now: DateTime<Utc>,
) -> (String, String, bool) {
    // 2. update.
    match engine::do_update(env, name, launcher, new_text, dir) {
        Ok(r) if r.ok() => {}
        Ok(r) => {
            // (R1) The installer exited nonzero but may have mutated the install: keep the pending
            // record (resolved = false) so the next pass re-probes and smokes/rolls back the result.
            let msg = format!("update failed ({})", r.fail_reason());
            println!("c3 maintain: {name} {installed_text} -> {new_text} {msg}");
            return (format!("{installed_text} -> {new_text} {msg}"), msg, false);
        }
        Err(e) => {
            let msg = format!("update failed ({e})");
            println!("c3 maintain: {name} {installed_text} -> {new_text} {msg}");
            return (format!("{installed_text} -> {new_text} {msg}"), msg, false);
        }
    }

    // Require the update to have actually taken: re-read `--version` and demand the checked target
    // (a `grok update` that installed something else, or a silent no-op, is a failed update).
    let after = engine::installed_version(env, launcher);
    if !matches!(&after, Some(v) if v >= latest_v) {
        // (R1) Some other version may now be on disk: keep the pending record for recovery.
        let now_t = after
            .as_ref()
            .map(|v| v.text().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let msg = format!("update did not take (still {now_t})");
        println!("c3 maintain: {name} {installed_text} -> {new_text} {msg}");
        return (format!("{installed_text} -> {new_text} {msg}"), msg, false);
    }
    let after_text = after
        .as_ref()
        .map(|v| v.text().to_string())
        .unwrap_or_else(|| new_text.to_string());

    // 3. smoke the new version. (F11-7) An unproven smoke (the sub-agent switch not proven on
    // both turns) holds the update exactly like a failed one: rolled back where supported, the
    // pending record kept unless the previous version is verified back - never "ok".
    let roster_text = roster_text_of(roster);
    if let Some((word, why)) =
        run_one_smoke(env, self_exe, target, name, now, roster_text.as_deref()).hold()
    {
        if caps.has_rollback {
            if let Some(res) = engine::do_rollback(env, name, launcher, installed_text, dir) {
                let cmd_ok = matches!(res, Ok(ref r) if r.ok());
                // Believe a rollback only when `--version` actually shows the previous version.
                let back = engine::installed_version(env, launcher);
                let restored = back
                    .as_ref()
                    .map(|v| v.parts == inst.parts)
                    .unwrap_or(false);
                // (R1) Resolved only when the previous version is verified back; a failed rollback
                // leaves the broken new version on disk, so keep the pending record.
                let (msg, resolved) = if cmd_ok && restored {
                    (format!("smoke {word}, rolled back"), true)
                } else {
                    let now_t = back
                        .as_ref()
                        .map(|v| v.text().to_string())
                        .unwrap_or_else(|| "unknown".to_string());
                    (
                        format!("smoke {word}, rollback failed, still {now_t}"),
                        false,
                    )
                };
                println!("c3 maintain: {name} {installed_text} -> {new_text} {msg} ({why})");
                return (
                    format!("{installed_text} -> {new_text} {msg}"),
                    msg,
                    resolved,
                );
            }
        }
        // (R1) No rollback path: the engine is stuck on a failed new version with no safe target.
        // Keep the record so the next pass re-smokes it once more before giving up.
        let msg = format!("smoke {word}, no rollback path");
        println!("c3 maintain: {name} {installed_text} -> {new_text} {msg} ({why})");
        return (format!("{installed_text} -> {new_text} {msg}"), msg, false);
    }

    let msg = "ok".to_string();
    println!("c3 maintain: {name} {installed_text} -> {after_text} {msg}");
    (
        format!("{installed_text} -> {after_text} ok"),
        format!("{installed_text} -> {after_text} ok"),
        true,
    )
}

/// Resolve a pending update left by an interrupted pass. Returns (report clause, state action,
/// whether the pending record may now be cleared). The installed version still being the previous
/// one means the update never took (clear). Otherwise the installed version is smoked: a pass clears
/// the record (verified); a fail is rolled back to the recorded previous version and the record is
/// cleared only when the rollback command succeeds AND the previous version is actually restored. A
/// failed rollback, a rollback that does not restore the previous version, and the absence of any
/// rollback path all leave the record in place (resolved = false) so a later due pass smokes it
/// again - the installed engine still fails its smoke, so the record must not be cleared. Then,
/// instead of sitting on the failing version until the operator steps in, a vendor release NEWER
/// than it is tried ([`try_newer_release`]), with the record's original `previous` kept as the
/// rollback target.
#[allow(clippy::too_many_arguments)]
fn handle_pending(
    env: &dyn Env,
    name: &str,
    launcher: &str,
    installed_text: &str,
    installed: Option<&version::CliVersion>,
    caps: &engine::Caps,
    roster: &c3_core::roster::Roster,
    dir: &Path,
    self_exe: &str,
    force: bool,
    now: DateTime<Utc>,
    p: &PendingUpdate,
) -> (String, String, bool) {
    let outcome = handle_pending_smoke(
        env,
        name,
        launcher,
        installed_text,
        installed,
        caps,
        roster,
        dir,
        self_exe,
        now,
        p,
    );
    match outcome {
        PendingOutcome::Done(clause, action, resolved) => (clause, action, resolved),
        PendingOutcome::StillFailing(clause, action, target) => {
            match try_newer_release(
                env, name, launcher, installed, caps, roster, dir, self_exe, force, now, p, &target,
            ) {
                Some(next) => next,
                None => (clause, action, false),
            }
        }
    }
}

/// What the recovery smoke of a pending update decided: resolved or not (and why), or the installed
/// version still FAILS its smoke with no working rollback - the case a newer release may fix.
enum PendingOutcome {
    Done(String, String, bool),
    StillFailing(String, String, smoke::Target),
}

/// The recovery smoke of [`handle_pending`] (the behaviour before the newer-release step).
#[allow(clippy::too_many_arguments)]
fn handle_pending_smoke(
    env: &dyn Env,
    name: &str,
    launcher: &str,
    installed_text: &str,
    installed: Option<&version::CliVersion>,
    caps: &engine::Caps,
    roster: &c3_core::roster::Roster,
    _dir: &Path,
    self_exe: &str,
    now: DateTime<Utc>,
    p: &PendingUpdate,
) -> PendingOutcome {
    use PendingOutcome::{Done, StillFailing};
    let prev = version::parse_cli_version(&p.previous);
    // The installed version is still the previous one: the interrupted update never took. Clear.
    if let (Some(inst), Some(prev)) = (installed, prev.as_ref()) {
        if inst.parts == prev.parts {
            let msg = "pending update cleared (installed version unchanged)".to_string();
            println!("c3 maintain: {name} {installed_text} - {msg}");
            return Done(format!("{installed_text} {msg}"), msg, true);
        }
    }
    // Otherwise verify the installed version by a smoke.
    let Some(target) = smoke::pick_target(&smoke_override(name), roster, name, now) else {
        let msg = "pending update unresolved: no quota for a smoke".to_string();
        println!("c3 maintain: {name} {installed_text} - {msg}");
        return Done(format!("{installed_text} {msg}"), msg, false); // keep the pending record
    };
    let roster_text = roster_text_of(roster);
    let outcome = run_one_smoke(env, self_exe, &target, name, now, roster_text.as_deref());
    let word = outcome.hold().map(|(w, _)| w).unwrap_or("failed");
    match outcome {
        smoke::Smoke::Pass => {
            let msg = format!(
                "recovered: update to {} verified after an interrupted pass",
                p.target
            );
            println!("c3 maintain: {name} {installed_text} - {msg}");
            Done(format!("{installed_text} {msg}"), msg, true)
        }
        // (F11-7) an unproven smoke is never "verified": it is handled exactly like a failure
        smoke::Smoke::Fail(why) | smoke::Smoke::Unproven(why) => {
            if caps.has_rollback {
                if let Some(res) = engine::do_rollback(env, name, launcher, &p.previous, _dir) {
                    let cmd_ok = matches!(res, Ok(ref r) if r.ok());
                    let back = engine::installed_version(env, launcher);
                    let restored = back
                        .as_ref()
                        .zip(prev.as_ref())
                        .map(|(b, pv)| b.parts == pv.parts)
                        .unwrap_or(false);
                    // Rollback succeeded and the previous version is back: the failed update is
                    // undone, so the pending record may be cleared.
                    if cmd_ok && restored {
                        let msg = format!(
                            "recovered: smoke {word}, rolled back to {} ({why})",
                            p.previous
                        );
                        println!("c3 maintain: {name} {installed_text} - {msg}");
                        return Done(format!("{installed_text} {msg}"), msg, true);
                    }
                    // Rollback command failed, or it ran but the previous version is not back: the
                    // installed engine still fails its smoke. Keep the record so a later pass tries
                    // again; never report this as "recovered".
                    let now_t = back
                        .as_ref()
                        .map(|v| v.text().to_string())
                        .unwrap_or_else(|| "unknown".to_string());
                    let msg = format!(
                        "pending update unresolved: rollback failed, still {now_t} ({why})"
                    );
                    println!("c3 maintain: {name} {installed_text} - {msg}");
                    return StillFailing(format!("{installed_text} {msg}"), msg, target);
                }
            }
            // No rollback path (unsupported, or do_rollback returned None): the engine is stuck on a
            // failing version. Keep the record for another pass; this is not a recovery.
            let msg = format!("pending update unresolved: smoke {word}, no rollback path ({why})");
            println!("c3 maintain: {name} {installed_text} - {msg}");
            StillFailing(format!("{installed_text} {msg}"), msg, target)
        }
    }
}

/// (TECH_DEBT 2026-11-16) An unresolved pending record no longer freezes its engine on the failing
/// version: when the vendor offers a release NEWER than the installed (failing) one, that release
/// is tried - the same cool-down (unless `--force`), in-use re-check, write-before-install record
/// and post-update smoke as a normal update, with the record's ORIGINAL `previous` kept as the
/// rollback target (and in the record). `None` when there is nothing to try (no check command, no
/// newer release, an unparsable previous or installed version, a cool-down hold, the engine in
/// use, or the record could not be written) - the caller then keeps the unresolved record as it is.
/// (F15-5) "Newer" is judged against the version installed NOW: `--version` is read again here,
/// after the recovery smoke and any rollback (a failed rollback or a concurrent manual update can
/// have left another version than the pass started with), and a release that is not newer than it
/// is never installed.
#[allow(clippy::too_many_arguments)]
fn try_newer_release(
    env: &dyn Env,
    name: &str,
    launcher: &str,
    installed_at_start: Option<&version::CliVersion>,
    caps: &engine::Caps,
    roster: &c3_core::roster::Roster,
    dir: &Path,
    self_exe: &str,
    force: bool,
    now: DateTime<Utc>,
    p: &PendingUpdate,
    target: &smoke::Target,
) -> Option<(String, String, bool)> {
    if !caps.has_check {
        return None;
    }
    let prev = version::parse_cli_version(&p.previous)?;
    let latest = engine::check_latest(env, name, launcher);
    let latest_v = latest.version.clone()?;
    // (F15-5) the version on disk now, not the one read before the recovery smoke and rollback
    let failing = engine::installed_version(env, launcher)?;
    let new_text = latest_v.text().to_string();
    let failing_text = failing.text().to_string();
    if latest_v <= failing {
        // say so when only the fresh read stopped it (the pass-start version would have let it in)
        if installed_at_start.is_some_and(|start| latest_v > *start) {
            println!(
                "c3 maintain: {name} {new_text} is not tried: {failing_text} is installed now (not older than it)"
            );
        }
        return None;
    }
    if let Some(released) = latest.released {
        let age = (now - released).num_days();
        if !force && age < min_age_days() {
            println!(
                "c3 maintain: {name} the newer {new_text} is held: released {age} days ago (the pending update on {failing_text} stays unresolved)"
            );
            return None;
        }
    }
    if engine_in_use(name, launcher) {
        println!("c3 maintain: {name} the newer {new_text} is not tried: in use");
        return None;
    }
    // (R2) the record first, keeping the ORIGINAL previous as the rollback target.
    if let Err(e) = set_pending(
        dir,
        PendingUpdate {
            engine: name.to_string(),
            previous: p.previous.clone(),
            target: new_text.clone(),
            when: rfc3339(now),
        },
    ) {
        println!(
            "c3 maintain: {name} the newer {new_text} is not tried: could not record the pending update ({e})"
        );
        return None;
    }
    println!(
        "c3 maintain: {name} {failing_text} still fails its smoke; trying the newer {new_text} (rollback target {})",
        p.previous
    );
    let (clause, action, resolved) = update_and_verify(
        env,
        name,
        launcher,
        &p.previous,
        &prev,
        &latest_v,
        &new_text,
        caps,
        target,
        roster,
        self_exe,
        dir,
        now,
    );
    Some((
        format!("{clause} (after {failing_text} failed)"),
        action,
        resolved,
    ))
}

/// Run one smoke in a fresh scratch repo (under the OS temp dir, random name) with a random token.
/// The smoke leaves no state behind: the scratch repo is removed afterwards and never lives under
/// the maintain state directory. `roster_text` is the roster a seat-bound target
/// ([`smoke::Bind::Entry`]) is taken from.
fn run_one_smoke(
    env: &dyn Env,
    self_exe: &str,
    target: &smoke::Target,
    engine: &str,
    now: DateTime<Utc>,
    roster_text: Option<&str>,
) -> smoke::Smoke {
    let nonce = now
        .timestamp_nanos_opt()
        .unwrap_or_else(|| now.timestamp_millis())
        .wrapping_mul(2_654_435_761)
        .wrapping_add(std::process::id() as i64);
    let token = format!("c3tok-{}-{}", now.timestamp(), nonce as u64);
    let work = std::env::temp_dir().join(format!(
        "c3-maintain-smoke-{engine}-{}-{}",
        std::process::id(),
        nonce as u64
    ));
    // (F14-3) the smoke creates `work` itself (exclusively) and removes it when it ends; a path that
    // already existed is never removed here
    smoke::run_smoke(env, self_exe, target, engine, &work, &token, roster_text)
}

/// The roster text an engine smoke's seat is bound to, read at smoke time (`None` when there is no
/// roster file: a seat-bound smoke then fails instead of re-resolving the label).
fn roster_text_of(roster: &c3_core::roster::Roster) -> Option<String> {
    if roster.path.is_empty() {
        return None;
    }
    std::fs::read_to_string(&roster.path).ok()
}

/// Whether an engine is within a pass's scope: inside the `--engine` filter (empty = all) and not in
/// the `C3_MAINTAIN_SKIP` set. Mirrors the engine step's own filter so the model step never touches a
/// seat the operator excluded.
fn engine_in_scope(engine: &str, only: &[String], skip: &[String]) -> bool {
    let eng = engine.to_ascii_lowercase();
    if !only.is_empty() && !only.contains(&eng) {
        return false;
    }
    !skip.contains(&eng)
}

/// What the model step found: the short report clauses (the hook line), the new ids of the engine
/// and endpoint lists, the findings summary, and the full notes `c3 maintain` prints.
struct ModelOutcome {
    clauses: Vec<String>,
    new_ids: Vec<String>,
    findings: String,
    notes: Vec<String>,
}

/// The most "new in the family" ids the short report line names (the rest: `+N more`; the full
/// list is in the stored notes).
const FAMILY_IDS_IN_LINE: usize = 3;

/// The model step: collect ids (the engines' lists, the endpoints' lists and - unless switched off -
/// the public catalogs and pages), record them, announce what is new for the roster, and in `swap`
/// mode seat successors after a smoke on the entry's own plan.
fn run_models(
    env: &dyn Env,
    roster: &c3_core::roster::Roster,
    state: &mut State,
    self_exe: &str,
    _dir: &Path,
    opts: &PassOpts,
) -> ModelOutcome {
    // (F02-18) `off` is authoritative for every pass, an explicit `--models` included (which returns
    // before it gets here): nothing is collected or fetched.
    let mode = models_mode();
    if mode == ModelsMode::Off {
        return ModelOutcome {
            clauses: Vec::new(),
            new_ids: Vec::new(),
            findings: "models: off".to_string(),
            notes: Vec::new(),
        };
    }
    let mut current = models::collect(env, roster);
    if web::enabled() {
        let (catalogs, pages) = web::configured_sources();
        let public = web::collect(env, &catalogs, &pages);
        for s in &public {
            // a catalog's name is shown without its query, which may hold a token
            println!(
                "c3 maintain: {} - {} model id(s) collected",
                web::catalog_source_display(&s.source),
                s.ids.len()
            );
        }
        current.extend(public);
    } else {
        println!("c3 maintain: public model sources off (C3_MODEL_WEB / model_web)");
    }
    let new_ids = models::new_ids(&state.models, &current);
    let news = models::family_news(roster, &state.models, &current, Utc::now().timestamp());
    let mut clauses: Vec<String> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut seated = 0usize;
    // The model step honours the same `--engine` filter and C3_MAINTAIN_SKIP as the engine step, so
    // a scoped pass never edits the roster or spends a smoke turn on a seat outside its scope.
    let only: Vec<String> = opts
        .engines
        .iter()
        .map(|e| e.to_ascii_lowercase())
        .collect();
    let skip = skip_set();

    let older_found: Vec<models::OlderPair>;
    {
        let plan = models::plan_swaps(
            roster,
            &current,
            &state.models.older,
            Utc::now().timestamp(),
        );
        older_found = plan.older;
        let (candidates, rejected) = (plan.candidates, plan.rejected);
        // A public id the version rule alone would take, without the date evidence that it is newer
        // than the seated model: reported in the notes (never smoked, never in the short line).
        for r in &rejected {
            if !engine_in_scope(&r.engine, &only, &skip) {
                continue;
            }
            let msg = format!(
                "model {} :: {} -> {} not a candidate ({})",
                r.provider, r.old, r.new, r.why
            );
            println!("c3 maintain: {msg}");
            notes.push(msg);
        }
        for c in &candidates {
            if !engine_in_scope(&c.engine, &only, &skip) {
                continue; // a seat of an engine outside this pass's scope is never touched
            }
            let label = format!("model {} :: {} -> {}", c.provider, c.old, c.new);
            println!(
                "c3 maintain: {label} (candidate{})",
                if c.from_public {
                    ", from a public catalog"
                } else {
                    ""
                }
            );
            // Announced only: a protected or on-request entry is never smoked or edited, and mode
            // `announce` edits nothing.
            let hold = if c.protected {
                Some("protected entry")
            } else if c.on_request {
                Some("on-request entry")
            } else if c.engine == "http" {
                // an `ext.c3.reviewers` http reviewer is not a `reviewers[]` element: the swap
                // edits only `reviewers[<position>].model` (F02-17), so it is never smoked either
                Some("an ext.c3.reviewers http entry is announced only")
            } else if mode == ModelsMode::Announce {
                Some("mode announce")
            } else {
                None
            };
            if let Some(why) = hold {
                let msg = format!("{label} not seated ({why})");
                println!("c3 maintain: {msg}");
                clauses.push(msg);
                continue;
            }
            if opts.dry_run {
                clauses.push(format!("{label} (dry-run: would smoke and seat)"));
                continue;
            }
            let now = Utc::now();
            // (R3) Re-read the roster fresh right before the smoke: the protection / saturation gate
            // and the swap's comparison basis come from the SAME on-disk snapshot, so a concurrent
            // edit during (or just before) the minutes-long smoke - protecting the entry, or
            // re-pointing its old model to another provider - is judged and aborted instead of being
            // swapped over the stale pass-start roster.
            let Ok(basis) = std::fs::read_to_string(&roster.path) else {
                let msg = format!("{label} not seated (roster unreadable)");
                println!("c3 maintain: {msg}");
                clauses.push(msg);
                continue;
            };
            let home = std::env::var("USERPROFILE")
                .ok()
                .or_else(|| std::env::var("HOME").ok());
            let fresh = c3_core::roster::validate_roster(&roster.path, &basis, home.as_deref());
            // The candidate's smoke obeys the saturation rule pick_target applies: a swap on a plan
            // whose governing window is at 90 %+ is held, the roster untouched. (0.2.4) a protected
            // or on-request seat is never smoked either - its quota is kept / it runs only when named.
            // (F02-16) The entry is the one at the candidate's POSITION, and it must still be this
            // exact provider + old model and active (else the roster changed under us and the swap
            // is abandoned) - never another entry that happens to share the label or the model.
            let entry = fresh.entries.iter().find(|e| {
                e.position == c.position
                    && e.provider == c.provider
                    && e.model == c.old
                    && !e.inactive
            });
            let blocked = match entry {
                None => Some("roster changed"),
                Some(e) if e.reserve_percent > 0 => Some("protected entry"),
                Some(e) if e.on_request => Some("on-request entry"),
                Some(e) if smoke::seat_saturated(&fresh, e, now) => {
                    Some("held: no quota for a smoke")
                }
                Some(_) => None,
            };
            if let Some(why) = blocked {
                let msg = format!("{label} not seated ({why})");
                println!("c3 maintain: {msg}");
                clauses.push(msg);
                continue;
            }
            // (F02-16) Both smoke turns run through exactly that entry - its endpoint, auth, plan
            // and quota policy, taken from the same snapshot (`basis`) - with only the model
            // overridden; the swap below edits the same position.
            let target = smoke::Target {
                provider: c.provider.clone(),
                model: c.new.clone(),
                bind: smoke::Bind::Entry(c.position),
                seated: c.old.clone(),
            };
            // One smoke per entry and pass: the highest successor only. A failed candidate is tried
            // again on a later pass.
            match run_one_smoke(env, self_exe, &target, &c.engine, now, Some(&basis)) {
                smoke::Smoke::Pass => {
                    let today = now.format("%Y%m%d").to_string();
                    match models::apply_swap(
                        &roster.path,
                        &basis,
                        c.position,
                        &c.provider,
                        &c.old,
                        &c.new,
                        &today,
                    ) {
                        Ok(()) => {
                            seated += 1;
                            let msg = format!("{label}, smoke passed");
                            println!("c3 maintain: {msg} (roster edited, backup taken)");
                            clauses.push(msg);
                        }
                        Err(e) => {
                            println!("c3 maintain: {label} smoke passed, not seated ({e})");
                            clauses.push(format!("{label} smoke passed, not seated"));
                            notes.push(format!("{label}: smoke passed, not seated ({e})"));
                        }
                    }
                }
                smoke::Smoke::Fail(why) => {
                    let msg = if c.from_public {
                        format!(
                            "{label} not seated (smoke failed: seen in the catalog, not served by this plan)"
                        )
                    } else {
                        format!("{label} not seated (smoke failed)")
                    };
                    println!("c3 maintain: {msg} ({why})");
                    clauses.push(msg);
                }
                // (F11-7) an unproven smoke never seats a model
                smoke::Smoke::Unproven(why) => {
                    let msg = format!("{label} not seated (smoke unproven)");
                    println!("c3 maintain: {msg} ({why})");
                    clauses.push(msg);
                }
            }
        }
    }

    // "New in the family": the full list goes into the notes, the line names a few.
    let mut family_ids: Vec<String> = Vec::new();
    for n in &news {
        let note = format!(
            "new in the family of {} ({}): {}",
            n.seated,
            n.provider,
            n.ids.join(", ")
        );
        println!("c3 maintain: {note}");
        notes.push(note);
        for id in &n.ids {
            if !family_ids.contains(id) {
                family_ids.push(id.clone());
            }
        }
    }
    if !family_ids.is_empty() {
        let more = family_ids.len().saturating_sub(FAMILY_IDS_IN_LINE);
        let shown = family_ids
            .iter()
            .take(FAMILY_IDS_IN_LINE)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        clauses.push(if more > 0 {
            format!("new in the family: {shown} (+{more} more)")
        } else {
            format!("new in the family: {shown}")
        });
    }
    // Every model clause is a note too (the stored full report).
    let mut all_notes: Vec<String> = clauses
        .iter()
        .filter(|c| !c.starts_with("new in the family: "))
        .cloned()
        .collect();
    all_notes.extend(notes);
    if !new_ids.is_empty() {
        all_notes.push(format!("new model ids: {}", new_ids.join(", ")));
    }

    // Record the collected ids as the new baseline. A public source that was not collected this
    // time (switched off, or the fetch failed) keeps its previous record, so a later pass does not
    // take it for a first collection and its names stay published. (F16-1, F17-5) A legacy
    // host-keyed catalog record (`web:<host>`, never trusted) is kept only until a catalog of that
    // host is recorded under its URL: then the fetch has rewritten it.
    let mut sources = current;
    for prev in &state.models.sources {
        if !web::is_public_source(&prev.source) || sources.iter().any(|s| s.source == prev.source) {
            continue;
        }
        if web::is_legacy_catalog_source(&prev.source) {
            let host = web::catalog_source_host(&prev.source);
            let rewritten = sources.iter().any(|s| {
                !web::is_legacy_catalog_source(&s.source)
                    && s.source.starts_with("web:")
                    && web::catalog_source_host(&s.source) == host
            });
            if rewritten {
                continue;
            }
        }
        sources.push(prev.clone());
    }
    // (F04-3) a candidate a public source dated as not newer stays rejected in later passes;
    // (F15-2, F14-2) the refusals of a seated id still in the roster survive the cap
    let seated_ids: Vec<String> = roster
        .active_entries()
        .filter(|e| !e.model.is_empty())
        .map(|e| e.model.to_ascii_lowercase())
        .collect();
    let older = models::merge_older(&state.models.older, &older_found, &seated_ids);
    state.models = models::ModelsState { sources, older };
    let findings = format!(
        "{seated} swap(s), {} new id(s), {} new in the family",
        new_ids.len(),
        family_ids.len()
    );
    ModelOutcome {
        clauses,
        new_ids,
        findings,
        notes: all_notes,
    }
}

// --------------------------------------------------------------------------- the smoke subcommand

/// `c3 maintain --smoke [--engine <name>]...`: run ONLY the two-turn smoke on each selected engine's
/// chosen target (no version check, no update, no state change). `--dry-run` prints the chosen
/// target per engine without running a model turn. Exit 0.
pub fn run_smoke_only(engines: Vec<String>, dry_run: bool) -> i32 {
    let now = Utc::now();
    // A codex home is still required to resolve the roster/config the targets come from; the smoke
    // scratch repo itself lives under the OS temp dir (no state is left behind).
    let Some(_dir) = state_dir() else {
        println!("c3 maintain smoke: no codex home - set CODEX_HOME");
        return 1;
    };
    let env = SystemEnv;
    let roster = crate::providers::read_reviewer_roster().unwrap_or_default();
    let only: Vec<String> = engines.iter().map(|e| e.to_ascii_lowercase()).collect();
    let self_exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "c3".to_string());

    for &name in engine::ENGINES {
        if !only.is_empty() && !only.contains(&name.to_string()) {
            continue;
        }
        if engine::resolve_launcher(name).is_none() {
            if !only.is_empty() {
                println!("c3 maintain smoke: {name} - not installed");
            }
            continue;
        }
        match smoke::pick_target(&smoke_override(name), &roster, name, now) {
            None => println!("c3 maintain smoke: {name} - no smoke target"),
            Some(t) => {
                if dry_run {
                    println!(
                        "c3 maintain smoke: {name} -> target {}::{} (target only, no model turn)",
                        t.provider, t.model
                    );
                } else {
                    let seat = format!("{}::{}", t.provider, t.model);
                    let roster_text = roster_text_of(&roster);
                    match run_one_smoke(&env, &self_exe, &t, name, now, roster_text.as_deref()) {
                        smoke::Smoke::Pass => {
                            println!("c3 maintain smoke: {name} -> {seat} pass")
                        }
                        smoke::Smoke::Fail(why) => {
                            println!("c3 maintain smoke: {name} -> {seat} fail ({why})")
                        }
                        smoke::Smoke::Unproven(why) => {
                            println!("c3 maintain smoke: {name} -> {seat} unproven ({why})")
                        }
                    }
                }
            }
        }
    }
    0
}

// --------------------------------------------------------------------------- the report subcommand

/// `c3 maintain` (no `--run`): report from the stored state. Exit 0.
pub fn run_report() -> i32 {
    let Some(dir) = state_dir() else {
        println!("c3 maintain: no codex home - set CODEX_HOME");
        return 0;
    };
    let s = read_state(dir.as_path());
    let on = enabled();
    println!("c3 maintain");
    println!("  feature:   {}", if on { "on" } else { "off" });
    println!(
        "  last pass: {}",
        if s.last_pass.is_empty() {
            "never"
        } else {
            &s.last_pass
        }
    );
    println!(
        "  next due:  {}",
        if s.next_due.is_empty() {
            "now"
        } else {
            &s.next_due
        }
    );
    if s.engines.is_empty() {
        println!("  engines:   none recorded yet");
    } else {
        for e in &s.engines {
            println!("  {:<8} {} - {}", e.name, e.installed, e.action);
        }
    }
    if !s.model_findings.is_empty() {
        println!("  models:    {}", s.model_findings);
    }
    for n in &s.model_notes {
        println!("    - {}", one_line_capped(n, 400));
    }
    if !s.report.is_empty() {
        println!("  report:    {}", s.report);
    }
    0
}

// --------------------------------------------------------------------------- the hook

/// The SessionStart hook's maintenance line (called after the update line in `hook::run`). It:
/// (a) returns the stored report line ONCE when a finished pass has not been reported yet (marking
/// it reported); (b) when the feature is on, a pass is due and no lock is held, starts a detached
/// `c3 maintain --run` child (output to `<state dir>/last-run.log`, no console window) and returns
/// `None`. Errors are silent; nothing to say prints nothing. Never spawns under
/// `CODEX_CONSULT_TEST_MODE` unless `C3_MAINTAIN_TEST_ALLOW_SPAWN=1`.
pub fn hook_line(now: DateTime<Utc>) -> Option<String> {
    if !enabled() {
        return None;
    }
    let dir = state_dir()?;
    let mut state = read_state(&dir);

    // (a) a finished pass not yet reported → return its line once.
    if !state.reported && !state.report.is_empty() {
        state.reported = true;
        write_state(&dir, &state);
        return Some(state.report);
    }

    // (b) spawn a detached pass when one is due and none is running. A machine with no reviewer
    // roster has nothing to smoke against (every engine would be held), so a pass is pointless -
    // the spawn is skipped, which also keeps the hook side-effect-free where no roster is configured.
    if is_due(&state, now) && !lock_held(&dir) && roster_present() && may_spawn() {
        let _ = spawn_detached_pass(&dir);
    }
    None
}

/// Whether a reviewer roster is configured (a pass needs one to pick a smoke target).
fn roster_present() -> bool {
    crate::providers::read_reviewer_roster()
        .map(|r| r.exists)
        .unwrap_or(false)
}

/// Whether the hook may spawn: always in production; under test mode only with the opt-in.
fn may_spawn() -> bool {
    if !c3_core::test_hooks::mode_on() {
        return true;
    }
    std::env::var("C3_MAINTAIN_TEST_ALLOW_SPAWN")
        .map(|v| v.trim() == "1")
        .unwrap_or(false)
}

/// Whether a live pass holds the lock (a dead pid's lock does not count).
fn lock_held(dir: &Path) -> bool {
    let path = dir.join("lock");
    match std::fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice::<LockRec>(&bytes) {
            Ok(rec) => crate::liveness::proc::pid_alive(rec.pid, &rec.start_time),
            Err(_) => false,
        },
        Err(_) => false,
    }
}

/// Start a detached `c3 maintain --run` whose console goes to `<state dir>/last-run.log` and which
/// holds none of the caller's handles (no visible window). Best-effort.
fn spawn_detached_pass(dir: &Path) -> Result<(), String> {
    if std::fs::create_dir_all(dir).is_err() {
        return Err("no state dir".into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let log = dir.join("last-run.log");
    spawn_detached(&exe, &["maintain", "--run"], &log, dir)
}

/// (0.2.5, F02-5 / F04-6) The environment of the detached pass: `vars` (this process's own) with
/// [`crate::engines::C3_CHILD_ENV`] set to `1`, replacing any value it had (the name compared
/// case-insensitively on Windows, where variable names are). The pass is a process C3 started, so
/// it is marked like a reviewer or worker child even when the session that spawned it is not.
fn detached_environment(
    vars: impl Iterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    let mark = crate::engines::C3_CHILD_ENV;
    let is_mark = |k: &std::ffi::OsStr| {
        if cfg!(windows) {
            k.to_string_lossy().eq_ignore_ascii_case(mark)
        } else {
            k == mark
        }
    };
    let mut out: Vec<_> = vars.filter(|(k, _)| !is_mark(k)).collect();
    out.push((mark.into(), "1".into()));
    out
}

/// A `CreateProcessW` environment block (`CREATE_UNICODE_ENVIRONMENT`): every `NAME=VALUE` as UTF-16
/// with its NUL, sorted by name case-insensitively as Windows requires, the block closed by one more
/// NUL (an empty block is two NULs).
#[cfg(windows)]
fn environment_block(vars: &[(std::ffi::OsString, std::ffi::OsString)]) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    let mut sorted: Vec<&(std::ffi::OsString, std::ffi::OsString)> = vars.iter().collect();
    sorted.sort_by_key(|(k, _)| k.to_string_lossy().to_uppercase());
    let mut block: Vec<u16> = Vec::new();
    for (k, v) in sorted {
        block.extend(k.encode_wide());
        block.push(u16::from(b'='));
        block.extend(v.encode_wide());
        block.push(0);
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    block
}

/// Start `<exe> <args>` (`c3 maintain --run`) detached, spawning the executable DIRECTLY (not
/// through `cmd.exe /c`): the log is opened by this process and handed to the child as
/// stdout/stderr, and stdin is NUL. Because nothing the child runs is built from a command line, a
/// `CODEX_HOME` or exe path containing a quote, `&`, `|` or `%` is inert. `args` are fixed tokens
/// (no white space, no quote). The child gets this process's environment plus `C3_CHILD=1`
/// ([`detached_environment`]).
#[cfg(windows)]
fn spawn_detached(exe: &Path, args: &[&str], log: &Path, cwd: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::AsRawHandle;

    const STARTF_USESTDHANDLES: u32 = 0x0000_0100;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    const CREATE_UNICODE_ENVIRONMENT: u32 = 0x0000_0400;
    const HANDLE_FLAG_INHERIT: u32 = 0x0000_0001;
    const ERROR_ACCESS_DENIED: i32 = 5;

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
            app: *const u16,
            cmd: *mut u16,
            pa: *mut u8,
            ta: *mut u8,
            inherit: i32,
            flags: u32,
            env: *mut u8,
            dir: *const u16,
            si: *mut StartupInfoW,
            pi: *mut ProcessInformation,
        ) -> i32;
        fn CloseHandle(h: isize) -> i32;
        fn SetHandleInformation(h: isize, mask: u32, flags: u32) -> i32;
        fn GetStdHandle(n: u32) -> isize;
        fn GetLastError() -> u32;
    }
    const STD_HANDLES: [u32; 3] = [-10i32 as u32, -11i32 as u32, -12i32 as u32]; // in, out, err

    // The parent opens the log (truncated for this pass) and NUL, and marks these two handles
    // inheritable (std File handles are non-inheritable by default).
    let log_file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(log)
        .map_err(|e| format!("could not open the maintain log '{}' ({e})", log.display()))?;
    let nul = std::fs::OpenOptions::new()
        .read(true)
        .open("NUL")
        .map_err(|e| format!("could not open NUL ({e})"))?;
    let log_h = log_file.as_raw_handle() as isize;
    let nul_h = nul.as_raw_handle() as isize;

    let app: Vec<u16> = exe.as_os_str().encode_wide().chain([0]).collect();
    let dir: Vec<u16> = cwd.as_os_str().encode_wide().chain([0]).collect();
    // The command line: "<exe>" maintain --run (only the exe is variable; it is quoted). The child
    // re-parses this, but the only variable token is the exe path inside quotes, so metacharacters
    // in it are inert.
    let mut cmdline_src = format!("\"{}\"", exe.to_string_lossy());
    for a in args {
        debug_assert!(!a.is_empty() && !a.contains(|c: char| c.is_whitespace() || c == '"'));
        cmdline_src.push(' ');
        cmdline_src.push_str(a);
    }
    // The child's environment: this process's, marked C3_CHILD=1 (an explicit block - a null one
    // would only inherit, and the spawning session is usually not a C3 child itself).
    let mut env_block = environment_block(&detached_environment(std::env::vars_os()));

    // SAFETY: the two handles live until after CreateProcessW; `app`, `dir`, the environment block
    // and the per-attempt command-line buffer are local and NUL-terminated.
    unsafe {
        if SetHandleInformation(log_h, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) == 0
            || SetHandleInformation(nul_h, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) == 0
        {
            return Err(format!(
                "could not mark the log handle inheritable (error {})",
                GetLastError()
            ));
        }
        // `bInheritHandles = TRUE` hands the child EVERY inheritable handle of this process, and the
        // hook's own stdin/stdout/stderr are inheritable pipes of whoever runs the hook. A pass
        // holding them keeps those pipes open, so a reader that waits for end-of-stream (Claude
        // Code collecting the hook's output) would wait for the whole pass - seen live: the hook
        // "returned" after 30 s, when the pass ended. So this process's own standard handles are
        // made non-inheritable first; it goes on writing to them as before.
        for n in STD_HANDLES {
            let h = GetStdHandle(n);
            if h != 0 && h != -1 {
                SetHandleInformation(h, HANDLE_FLAG_INHERIT, 0);
            }
        }
        let mut si: StartupInfoW = std::mem::zeroed();
        si.cb = std::mem::size_of::<StartupInfoW>() as u32;
        si.dwFlags = STARTF_USESTDHANDLES;
        si.hStdInput = nul_h;
        si.hStdOutput = log_h;
        si.hStdError = log_h;
        let base = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_UNICODE_ENVIRONMENT;
        // Attempt 0 lifts the child out of the launcher's Job (so a closing console cannot kill it);
        // on ERROR_ACCESS_DENIED (a Job forbidding breakaway) retry once without the flag.
        let attempts = [base | CREATE_BREAKAWAY_FROM_JOB, base];
        let mut last_err = 0u32;
        let mut started = false;
        for (i, &flags) in attempts.iter().enumerate() {
            let mut cmdline: Vec<u16> = cmdline_src.encode_utf16().chain([0]).collect();
            let mut pi: ProcessInformation = std::mem::zeroed();
            let ok = CreateProcessW(
                app.as_ptr(),
                cmdline.as_mut_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                1, // bInheritHandles = TRUE: the child inherits the two handles marked above
                flags,
                env_block.as_mut_ptr().cast::<u8>(),
                dir.as_ptr(),
                &mut si,
                &mut pi,
            );
            if ok != 0 {
                CloseHandle(pi.hProcess);
                CloseHandle(pi.hThread);
                started = true;
                break;
            }
            last_err = GetLastError();
            if !(i == 0 && last_err as i32 == ERROR_ACCESS_DENIED) {
                break;
            }
        }
        if !started {
            return Err(format!(
                "could not start the detached maintenance pass (error {last_err})"
            ));
        }
    }
    // log_file and nul drop here, closing the parent's copies; the child holds its own.
    Ok(())
}

#[cfg(not(windows))]
fn spawn_detached(exe: &Path, args: &[&str], log: &Path, cwd: &Path) -> Result<(), String> {
    use std::process::{Command, Stdio};
    let log_file = std::fs::File::create(log).map_err(|e| e.to_string())?;
    let log_err = log_file.try_clone().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .env_clear()
        .envs(detached_environment(std::env::vars_os()))
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_err));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-15T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn due_when_never_run_or_past_next_due() {
        let s = State::default();
        assert!(is_due(&s, now()), "never run → due");
        let mut s = State {
            next_due: rfc3339(now() + chrono::Duration::days(3)),
            ..Default::default()
        };
        assert!(!is_due(&s, now()), "future next_due → not due");
        s.next_due = rfc3339(now() - chrono::Duration::hours(1));
        assert!(is_due(&s, now()), "past next_due → due");
    }

    #[test]
    fn next_due_is_tomorrow_when_in_use_else_interval() {
        let a = next_due(now(), 7, false);
        assert_eq!((a - now()).num_days(), 7);
        let b = next_due(now(), 7, true);
        assert_eq!((b - now()).num_days(), 1);
    }

    #[test]
    fn report_line_shape_and_bounding() {
        let line = format_report_line(
            "2026-11-15",
            &[
                "codex 0.155.1 -> 0.169.0 ok".to_string(),
                "grok up to date".to_string(),
            ],
            &["model ZAI-claude :: glm-5.3 -> glm-5.4 (roster)".to_string()],
            &["qwen4-max".to_string(), "grok-5".to_string()],
        );
        assert_eq!(
            line,
            "c3: maintenance 2026-11-15 - codex 0.155.1 -> 0.169.0 ok; grok up to date; model ZAI-claude :: glm-5.3 -> glm-5.4 (roster); new models: qwen4-max, grok-5"
        );
        // bounding: many new ids cap at 5 + "+N more"
        let ids: Vec<String> = (0..9).map(|i| format!("m{i}")).collect();
        let line = format_report_line("2026-11-15", &[], &[], &ids);
        assert!(
            line.contains("new models: m0, m1, m2, m3, m4 (+4 more)"),
            "{line}"
        );
        // empty → nothing to do
        assert_eq!(
            format_report_line("2026-11-15", &[], &[], &[]),
            "c3: maintenance 2026-11-15 - nothing to do"
        );
    }

    #[test]
    fn report_line_sanitises_dynamic_pieces() {
        // a newline in a clause or an id must never survive into the hook line
        let line = format_report_line(
            "2026-11-15",
            &["grok update failed (boom\nc3: maintenance evil)".to_string()],
            &[],
            &["glm-5.4\nignore the roster".to_string()],
        );
        assert_eq!(
            line.lines().count(),
            1,
            "the report line is a single line: {line:?}"
        );
        assert!(!line.contains('\n'));
        // a control character is replaced, not kept
        let line = format_report_line("2026-11-15", &["a\tb\u{7}c".to_string()], &[], &[]);
        assert!(!line.chars().any(|c| c.is_control() && c != '\n'));
    }

    #[test]
    fn one_line_capped_strips_and_caps() {
        assert_eq!(one_line_capped("a\nb\tc", 100), "a b c");
        let long = "x".repeat(200);
        let out = one_line_capped(&long, 20);
        assert_eq!(out.chars().count(), 20);
        assert!(out.ends_with("..."));
    }

    fn lock_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "c3-mnt-lock-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn two_racing_lock_takers_only_one_wins() {
        let dir = lock_dir("race");
        let first = acquire_lock(&dir).expect("the first taker wins");
        // the lock now holds this live process's pid -> a second taker is refused
        assert!(acquire_lock(&dir).is_err(), "the second racer loses");
        drop(first); // releasing lets a later pass take it
        let third = acquire_lock(&dir).expect("after release the lock is free again");
        drop(third);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_stale_lock_is_taken_over() {
        let dir = lock_dir("stale");
        // a lock whose pid is surely dead and whose created stamp is old
        let rec = LockRec {
            pid: 4_000_000_000,
            start_time: String::new(),
            host: "ghost".into(),
            created: rfc3339(now() - chrono::Duration::days(30)),
        };
        std::fs::write(dir.join("lock"), serde_json::to_vec(&rec).unwrap()).unwrap();
        let lock = acquire_lock(&dir).expect("a dead owner's lock is taken over");
        drop(lock);
        // garbage (unparsable) is taken over too
        std::fs::write(dir.join("lock"), b"not json").unwrap();
        let lock = acquire_lock(&dir).expect("an unreadable lock is taken over");
        drop(lock);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn in_use_detects_name_and_command_line_and_fails_closed_on_a_snapshot_error() {
        use std::collections::HashSet;
        let mine: HashSet<u32> = HashSet::new();
        // a failed snapshot reads as in use (fail-closed)
        assert!(in_use_from(
            "codex",
            "/usr/bin/codex",
            false,
            &mine,
            Err(())
        ));
        // an image-name match of the launcher stem
        let rows = vec![(10u32, "grok.exe".to_string(), String::new())];
        assert!(in_use_from(
            "grok",
            "C:/n/grok.cmd",
            false,
            &mine,
            Ok(&rows)
        ));
        // a codex npm shim running as node, caught by the package marker (launcher stem "kodex"
        // does not appear in the path, so only the @openai/codex marker can match it)
        let rows = vec![(
            11u32,
            "node.exe".to_string(),
            "node C:/npm/@openai/codex/cli.js exec".to_string(),
        )];
        assert!(in_use_from(
            "codex",
            "C:/bin/kodex.cmd",
            false,
            &mine,
            Ok(&rows)
        ));
        // under test mode the broad package marker is dropped (the fake has a distinct stem)
        assert!(!in_use_from(
            "codex",
            "C:/bin/kodex.cmd",
            true,
            &mine,
            Ok(&rows)
        ));
        // a generic runtime whose command line carries the launcher stem as a path segment
        let rows = vec![(
            12u32,
            "powershell.exe".to_string(),
            "powershell -File C:/tools/grok/grok.ps1".to_string(),
        )];
        assert!(in_use_from(
            "grok",
            "C:/n/grok.cmd",
            false,
            &mine,
            Ok(&rows)
        ));
        // an unrelated process is not a match
        let rows = vec![
            (13u32, "chrome.exe".to_string(), String::new()),
            (14u32, "node.exe".to_string(), "node server.js".to_string()),
        ];
        assert!(!in_use_from(
            "grok",
            "C:/n/grok.cmd",
            false,
            &mine,
            Ok(&rows)
        ));
        // our own process and children are skipped even on a name match
        let mut mine2: HashSet<u32> = HashSet::new();
        mine2.insert(20);
        let rows = vec![(20u32, "grok.exe".to_string(), String::new())];
        assert!(!in_use_from(
            "grok",
            "C:/n/grok.cmd",
            false,
            &mine2,
            Ok(&rows)
        ));
    }

    #[test]
    fn the_model_step_scope_filter() {
        let none: Vec<String> = vec![];
        // no filter, no skip: every engine is in scope
        assert!(engine_in_scope("grok", &none, &none));
        // an --engine codex filter excludes grok/claude seats
        let only = vec!["codex".to_string()];
        assert!(engine_in_scope("codex", &only, &none));
        assert!(!engine_in_scope("grok", &only, &none));
        // C3_MAINTAIN_SKIP=grok excludes grok even with no --engine filter
        let skip = vec!["grok".to_string()];
        assert!(!engine_in_scope("grok", &none, &skip));
        assert!(engine_in_scope("codex", &none, &skip));
    }

    #[test]
    fn the_pending_journal_round_trips() {
        let dir = lock_dir("pending");
        assert!(read_pending(&dir).is_empty());
        set_pending(
            &dir,
            PendingUpdate {
                engine: "grok".into(),
                previous: "1.0.77".into(),
                target: "1.0.80".into(),
                when: rfc3339(now()),
            },
        )
        .unwrap();
        let got = read_pending(&dir);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].previous, "1.0.77");
        assert_eq!(got[0].target, "1.0.80");
        clear_pending(&dir, "grok");
        assert!(read_pending(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // (0.2.5, F02-5 / F04-6) The detached pass's environment: the parent's, with C3_CHILD=1 replacing
    // whatever the parent had (on Windows also a lower-case spelling of the name).
    #[test]
    fn the_detached_environment_carries_the_child_mark() {
        use std::ffi::OsString;
        let vars: Vec<(OsString, OsString)> = vec![
            ("PATH".into(), "p".into()),
            ("C3_CHILD".into(), "0".into()),
            ("W25_KEEP".into(), "k".into()),
        ];
        let env = detached_environment(vars.into_iter());
        let marks: Vec<_> = env
            .iter()
            .filter(|(k, _)| k.to_string_lossy().eq_ignore_ascii_case("C3_CHILD"))
            .collect();
        assert_eq!(marks.len(), 1, "{env:?}");
        assert_eq!(marks[0].1, "1");
        assert!(env.iter().any(|(k, v)| k == "W25_KEEP" && v == "k"));
        assert!(env.iter().any(|(k, v)| k == "PATH" && v == "p"));
        if cfg!(windows) {
            let lower: Vec<(OsString, OsString)> = vec![("c3_child".into(), "x".into())];
            let env = detached_environment(lower.into_iter());
            assert_eq!(env, vec![(OsString::from("C3_CHILD"), OsString::from("1"))]);
        }
    }

    #[cfg(windows)]
    #[test]
    fn the_environment_block_is_sorted_and_double_nul_terminated() {
        let block = environment_block(&[
            ("b".into(), "2".into()),
            ("C3_CHILD".into(), "1".into()),
            ("A".into(), "1".into()),
        ]);
        let text = String::from_utf16(&block).unwrap();
        assert_eq!(text, "A=1\0b=2\0C3_CHILD=1\0\0");
        assert_eq!(environment_block(&[]), vec![0, 0]);
    }

    /// The probe the detached-spawn test starts: run as `<this test binary> --exact
    /// maintain::tests::detached_env_probe --nocapture`, it prints its own `C3_CHILD` into the
    /// pass's log. Run as an ordinary test it prints into the captured output and passes.
    #[test]
    fn detached_env_probe() {
        println!(
            "detached-env-probe C3_CHILD=[{}]",
            std::env::var(crate::engines::C3_CHILD_ENV).unwrap_or_default()
        );
    }

    // (0.2.5, F02-5 / F04-6) The real detached spawn (`CreateProcessW` with an explicit environment
    // block on Windows, a process group elsewhere) marks the child: this test binary is the fake
    // launcher, started detached with the probe above, and its log line names C3_CHILD=1.
    #[test]
    fn the_detached_spawn_marks_its_child() {
        let dir = lock_dir("detached-env");
        let log = dir.join("last-run.log");
        let exe = std::env::current_exe().unwrap();
        spawn_detached(
            &exe,
            &[
                "--exact",
                "maintain::tests::detached_env_probe",
                "--nocapture",
                "--test-threads=1",
            ],
            &log,
            &dir,
        )
        .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let mut text = String::new();
        while std::time::Instant::now() < deadline {
            text = std::fs::read_to_string(&log).unwrap_or_default();
            if text.contains("test result:") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(
            text.contains("detached-env-probe C3_CHILD=[1]"),
            "the detached child was not marked: {text}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
