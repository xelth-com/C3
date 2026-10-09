//! Process liveness, ported from the plugin's `Get-ProcessStartIso` / `Test-PidAlive` /
//! `Test-SameStartTime` (`codex-consult-common.ps1`). A lock or recovery record names a
//! pid and (0.3.0+) that process's start time; a status change or rating is refused while
//! that process is still the one that wrote the record. The start time distinguishes a live
//! owner from a reused pid.
//!
//! On Windows the start time is read from the kernel's process creation `FILETIME`
//! (`OpenProcess` + `GetProcessTimes`), formatted as .NET's round-trip `o` string in UTC —
//! the same value `Process.StartTime.ToUniversalTime().ToString('o')` produces, so a record
//! written by the PowerShell bridge and one written by C3 compare equal. Elsewhere liveness
//! is a best-effort existence check (`/proc` on Linux) and the start time is left blank,
//! matching the plugin's sub-second tolerance for non-Windows.

/// The process's start time as .NET's `o` string in UTC, `Some("")` when the process exists
/// but its start time is not available, `None` when there is no such process. Mirrors
/// `Get-ProcessStartIso` (which returns `$null` when `Get-Process` fails, `''` when
/// `StartTime` throws). (wave 28e, E1/E19) On Windows a process that exists but denies the query
/// (a protected or another session's process: `OpenProcess` fails with access denied) reads `""`,
/// never "gone" - its identity cannot be confirmed, so it is never taken for a dead one.
/// TEST HOOK (test mode only, wave 28c D8): `CODEX_CONSULT_TEST_START_UNREADABLE=<pid>[,<pid>]` -
/// these pids, when they exist, read `""` (as access denied makes it). The machine-wide scan's
/// process table ([`enumerate_processes`]) does not go through the hook (the plugin's scan reads
/// `Win32_Process.CreationDate`, not `Get-ProcessStartIso`).
pub fn process_start_iso(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    let live = imp::process_start_iso(pid)?;
    if start_unreadable_hook().contains(&pid) {
        return Some(String::new());
    }
    Some(live)
}

/// The pids of `CODEX_CONSULT_TEST_START_UNREADABLE` (test mode only; empty otherwise).
fn start_unreadable_hook() -> Vec<u32> {
    pid_list_hook("CODEX_CONSULT_TEST_START_UNREADABLE")
}

/// A `CODEX_CONSULT_TEST_*` hook that names pids (`<pid>[,<pid>]`), honoured in test mode only.
pub fn pid_list_hook(name: &str) -> Vec<u32> {
    c3_core::test_hooks::hook(name)
        .map(|v| {
            v.split(',')
                .filter_map(|t| t.trim().parse::<u32>().ok())
                .filter(|p| *p > 0)
                .collect()
        })
        .unwrap_or_default()
}

/// (wave 28c, D8) `Get-PidIdentity`: is `pid` still the process that had `start_time`?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PidIdentity {
    /// A process with that pid runs with that start time.
    Alive,
    /// No such process, or the pid now belongs to another one (another start time).
    Gone,
    /// The process exists but its identity cannot be confirmed: no start time was recorded, or
    /// its start time cannot be read now.
    Unknown,
}

/// `Get-PidIdentity`.
pub fn pid_identity(pid: u32, start_time: &str) -> PidIdentity {
    if pid == 0 {
        return PidIdentity::Gone;
    }
    let Some(live) = process_start_iso(pid) else {
        return PidIdentity::Gone;
    };
    if start_time.is_empty() || live.is_empty() {
        return PidIdentity::Unknown;
    }
    if same_start_time(&live, start_time) {
        PidIdentity::Alive
    } else {
        PidIdentity::Gone
    }
}

/// (wave 28e, E19) `Get-ProcessInfo`: one process - its name (.NET's `ProcessName`: the image name
/// without `.exe`), its command line (`""` when it cannot be read - access denied), its start time
/// (`""` when it cannot be read) and its parent's pid (`0` when unknown) - or `None` when no
/// process has that pid. TEST HOOK (test mode only): `CODEX_CONSULT_TEST_CMDLINE_UNREADABLE=<pid>
/// [,<pid>]` - these pids read with the command line `""`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cmd: String,
    pub start: String,
    pub ppid: u32,
    /// (wave 3c, F23-1) `false` when the process EXISTS (its start time reads, or reads `""`) but
    /// its name and parent could not be read: the process-table lookup failed or did not list it
    /// while the pid still answers. `name` is `""` and `ppid` `0` then. Its identity is unknown,
    /// never "gone": a re-check counts such a process as running (fail-closed).
    pub inspected: bool,
}

/// `Get-ProcessInfo` (see [`ProcessInfo`]): `None` only when no process has that pid - a process
/// whose name and parent cannot be read is returned with `inspected: false` (wave 3c, F23-1; the
/// plugin's `Get-Process` reads the name of any process that exists, its CIM read of the command
/// line and the parent failing leaves `''` and `0`). TEST HOOK (test mode only):
/// `CODEX_CONSULT_TEST_INFO_UNREADABLE=<pid>[,<pid>]` - the name-and-parent lookup of these pids
/// fails (as a failed or incomplete process-table snapshot makes it).
pub fn process_info(pid: u32) -> Option<ProcessInfo> {
    let start = process_start_iso(pid)?;
    let looked_up = if pid_list_hook("CODEX_CONSULT_TEST_INFO_UNREADABLE").contains(&pid) {
        None
    } else {
        imp::name_and_parent(pid)
    };
    let (name, ppid, inspected) = match looked_up {
        Some((image, ppid)) => (strip_exe(&image).to_string(), ppid, true),
        None => {
            // gone between the two reads: no such process; still there: not inspectable - the
            // lookup's failure is never taken for the process's absence
            process_start_iso(pid)?;
            (String::new(), 0, false)
        }
    };
    let cmd = if pid_list_hook("CODEX_CONSULT_TEST_CMDLINE_UNREADABLE").contains(&pid) {
        String::new()
    } else {
        process_command_line(pid)
    };
    Some(ProcessInfo {
        pid,
        name,
        cmd,
        start,
        ppid,
        inspected,
    })
}

/// Terminate one process by its pid (best effort). Callers check its identity first
/// ([`pid_identity`]): a pid is never killed on a guess.
pub fn terminate_pid(pid: u32) {
    if pid == 0 || pid == std::process::id() {
        return;
    }
    imp::terminate_pid(pid)
}

/// `Test-PidAlive`: a pid is alive when a process with that id exists and — if a start time
/// was recorded — still has that start time (otherwise the pid was reused).
pub fn pid_alive(pid: u32, start_time: &str) -> bool {
    if pid == 0 {
        return false;
    }
    let live = match process_start_iso(pid) {
        Some(s) => s,
        None => return false,
    };
    if !start_time.is_empty() && !live.is_empty() && !same_start_time(&live, start_time) {
        return false;
    }
    true
}

/// (Test harness) The "bridge" process that recovery and lock records name and whose death ends
/// this run. In production c3 IS the bridge, so this is c3's own pid + start time. Under the
/// plugin's PowerShell harnesses c3 runs as a child of a thin shim: the harness monitors and kills
/// the SHIM (the process it launched), so the shim exports `CODEX_CONSULT_TEST_BRIDGE_PID` = its
/// own pid, c3 records that pid (matching the plugin, where the launched process IS the bridge),
/// and [`watch_bridge`] shares the shim's fate. A panel run clears the var when spawning members,
/// so each member records its own pid.
///
/// SECURITY: the hook is honoured only when it names a LIVE ANCESTOR of this process (the shim is
/// c3's parent, so it is always an ancestor). An invalid, dead or non-ancestor value is ignored
/// silently and c3 falls back to its own pid — a stray environment variable can never make c3
/// record, monitor or share the fate of an unrelated process. The value is validated exactly once
/// at process start and cached.
static VALIDATED_BRIDGE: std::sync::OnceLock<Option<(u32, String)>> = std::sync::OnceLock::new();

/// Resolve and validate the bridge hook once: a live ancestor named by
/// `CODEX_CONSULT_TEST_BRIDGE_PID`, else `None`.
fn validated_bridge() -> &'static Option<(u32, String)> {
    VALIDATED_BRIDGE.get_or_init(|| {
        let pid = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_BRIDGE_PID")
            .and_then(|s| s.trim().parse::<u32>().ok())
            .filter(|p| *p > 0 && *p != std::process::id())?;
        if !pid_alive(pid, "") {
            return None;
        }
        if !is_ancestor_of_self(pid) {
            return None;
        }
        Some((pid, process_start_iso(pid).unwrap_or_default()))
    })
}

pub fn bridge_identity() -> (u32, String) {
    if let Some((pid, start)) = validated_bridge() {
        return (*pid, start.clone());
    }
    let me = std::process::id();
    (me, process_start_iso(me).unwrap_or_default())
}

/// Walk the parent chain from this process upward: is `target` one of this process's ancestors?
/// Bounded to 64 hops and stops at the system/idle pids so a reused or stale parent pid never
/// loops. A non-ancestor (e.g. pid 4, or an unrelated process) returns `false`.
fn is_ancestor_of_self(target: u32) -> bool {
    let mut cur = std::process::id();
    for _ in 0..64 {
        let parent = match imp::parent_pid(cur) {
            Some(p) if p > 0 && p != cur => p,
            _ => return false,
        };
        if parent == target {
            return true;
        }
        if parent <= 4 {
            return false;
        }
        cur = parent;
    }
    false
}

/// (Test harness) When `CODEX_CONSULT_TEST_BRIDGE_PID` names a live process other than this one,
/// spawn a watchdog thread that force-exits this process shortly after that bridge dies — so a c3
/// launched under the plugin's shim shares the shim's fate. Without it an orphaned c3 would outlive
/// the shim the harness killed, keeping the task lock held and finishing a commit the test means to
/// interrupt (ORPHAN, PARENT). No effect in production (the var is unset). Requires three
/// consecutive dead reads (~300 ms) before acting, so a transient `OpenProcess` failure never kills
/// a live run.
pub fn watch_bridge() {
    // Only a validated live ancestor is watched (see [`bridge_identity`]).
    if let Some((pid, _)) = validated_bridge() {
        imp::spawn_bridge_watchdog(*pid);
    }
}

/// `Test-SameStartTime`: exact match on Windows; elsewhere within one clock tick of the `/proc`
/// start time (10 ms, `USER_HZ` 100 — two processes cannot share a tick on one pid, F09-3). The
/// plugin's wider non-Windows tolerance is not needed now that both sides of the comparison come
/// from the same `/proc` reading.
pub fn same_start_time(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if cfg!(windows) {
        return false;
    }
    match (
        chrono::DateTime::parse_from_rfc3339(a),
        chrono::DateTime::parse_from_rfc3339(b),
    ) {
        (Ok(ta), Ok(tb)) => (ta.timestamp_millis() - tb.timestamp_millis()).abs() < 10,
        _ => false,
    }
}

// ------------------------------------------------------------- machine-wide codex scan (rows (b))
// Ported from `Find-CodexProcesses` / `Get-CodexRule` (`codex-consult-common.ps1:8022`, `:7943`).
// A `launching` recovery record whose process is not registered yet — or a record whose every
// recorded pid is gone — is judged by a READ-ONLY scan of the process table: it enumerates and
// compares parents/start-times/names, it never stops anything. The rule logic is a pure function
// over a process list so it is proven by unit tests without touching the machine.

/// One process-table row for the scan: pid, parent pid, image name (with extension, as
/// `Win32_Process.Name`), creation time (`None` when it could not be read), and command line
/// (empty unless fetched for a candidate that passed the name+time rules).
#[derive(Debug, Clone)]
pub struct ScanProc {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub created: Option<chrono::DateTime<chrono::Utc>>,
    pub command_line: String,
}

/// A process the scan attributes to an interrupted run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundProc {
    pub pid: u32,
    pub name: String,
    pub rule: String,
}

/// A codex-named process the machine-wide rule left out (wave 29, E27): one of the Codex desktop
/// app's (or an IDE extension's) servers or helpers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExcludedProc {
    pub pid: u32,
    pub name: String,
    /// What it was recognised as: `codex app-server`, `codex-computer-use-swift helper`, ...
    pub why: String,
}

/// The outcome of one scan pass (`Find-CodexProcesses`'s return object).
#[derive(Debug, Clone)]
pub struct ScanOutcome {
    pub found: Vec<FoundProc>,
    pub check: String,
    pub failed: bool,
    /// (wave 29, E27) the Codex app's servers and helpers the name rule left out (also named in
    /// `check`: "excluded: pid N codex.exe [codex app-server]").
    pub excluded: Vec<ExcludedProc>,
}

/// `Get-CodexMatch`'s result: the rule (`""` or the reason, as [`codex_rule`] returns it) and what
/// the rule left out (`""` or what a codex-named process was recognised as - `codex app-server`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodexMatch {
    pub rule: String,
    pub excluded: String,
}

/// `Get-CodexRule`: why a process looks like codex (the rule label), or `""` when it does not -
/// name codex / codex.exe (the native binary), the file name of the recorded launcher when that is
/// a binary, or a command line containing the recorded launcher path or `@openai/codex`. It cannot
/// tell WHICH task's consultation a process belongs to. (wave 29, E27) A codex-named process whose
/// command line shows it is one of the Codex desktop app's servers or helpers never matches
/// ([`codex_server_exclusion`]; [`codex_match`] says what was left out); one whose command line
/// cannot be read still does (fail-closed), and so (E28 / F37-1) does one whose command line holds
/// the word exec anywhere or whose quoting is ambiguous.
/// `name` is the image name (with or without `.exe`), `cmd` its command line, `launcher` the
/// recorded launcher path (empty when none).
pub fn codex_rule(name: &str, cmd: &str, launcher: &str) -> String {
    codex_match(name, cmd, launcher).rule
}

/// (wave 29, E27) `Get-CodexMatch`: the "looks like codex" rule with what it left out. The
/// exclusion is decided first: a codex-named server is left out even when its command line
/// carries the recorded launcher (the app's own codex.exe given as -CodexExe). (E28 / F37-1) A
/// codex-named process whose command line cannot be split with certainty (unbalanced quoting) is
/// NOT excluded and counts as codex: rule "command line ambiguous - counted as codex". Pure.
pub fn codex_match(name: &str, cmd: &str, launcher: &str) -> CodexMatch {
    let verdict = codex_server_exclusion(name, cmd);
    if !verdict.excluded.is_empty() {
        return CodexMatch {
            rule: String::new(),
            excluded: verdict.excluded,
        };
    }
    let rule = if !verdict.ambiguous.is_empty() {
        "command line ambiguous - counted as codex".to_string()
    } else {
        codex_name_rule(name, cmd, launcher)
    };
    CodexMatch {
        rule,
        excluded: String::new(),
    }
}

/// The rule itself, before the E27 exclusion (the pre-wave-29 `Get-CodexRule`).
fn codex_name_rule(name: &str, cmd: &str, launcher: &str) -> String {
    let name_l = name.to_ascii_lowercase();
    if name_l == "codex" || name_l == "codex.exe" {
        return "name codex".to_string();
    }
    if !launcher.is_empty() && !name.is_empty() {
        let path = std::path::Path::new(launcher);
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let base = path
            .file_stem()
            .map(|b| b.to_string_lossy().to_string())
            .unwrap_or_default();
        let name_base = strip_exe(name);
        if !base.is_empty()
            && (ext.is_empty() || ext == "exe")
            && name_base.eq_ignore_ascii_case(&base)
        {
            return format!("name {base} (the recorded launcher)");
        }
    }
    if !launcher.is_empty()
        && !cmd.is_empty()
        && cmd
            .to_ascii_lowercase()
            .contains(&launcher.to_ascii_lowercase())
    {
        return "launcher in command line".to_string();
    }
    let cmd_l = cmd.to_ascii_lowercase();
    if cmd_l.contains("@openai/codex") || cmd_l.contains("@openai\\codex") {
        return "@openai/codex in command line".to_string();
    }
    String::new()
}

/// An image name without a trailing `.exe` (any case). (F04-1) Boundary-safe: the candidate
/// suffix is taken with `str::get`, so a name whose last four bytes start inside a character
/// (a Linux `/proc/<pid>/comm` such as `日本`) is returned unchanged instead of panicking.
fn strip_exe(name: &str) -> &str {
    let cut = name.len().saturating_sub(4);
    match name.get(cut..) {
        Some(tail) if tail.eq_ignore_ascii_case(".exe") => &name[..cut],
        _ => name,
    }
}

/// (wave 29, E27) The Codex CLI subcommands that are never a reviewer run (a reviewer run of the
/// bridge is always `codex exec ...`, or the launcher shim that starts it): the long-running
/// servers of the Codex desktop app and of the IDE extensions (app-server, exec-server,
/// mcp-server), the sign-in (login) and the app launcher (app).
const CODEX_SERVER_SUBCOMMANDS: &[&str] =
    &["app-server", "exec-server", "mcp-server", "login", "app"];

/// The Codex CLI's global options that take their value as the next token (`-c key=value`):
/// skipped with that value while the first non-option token (the subcommand) is looked for.
/// Case-sensitive.
const CODEX_VALUE_OPTIONS: &[&str] = &[
    "-c",
    "--config",
    "-m",
    "--model",
    "-p",
    "--profile",
    "-C",
    "--cd",
    "-s",
    "--sandbox",
    "-a",
    "--ask-for-approval",
    "-i",
    "--image",
    "--enable",
    "--disable",
    "--add-dir",
    "--local-provider",
];

/// (wave 28e, E19 / F27-2) The generic runtimes a reviewer may run under (the codex npm shim is
/// cmd -> node -> codex): such a process tells what it runs only through its arguments.
const GENERIC_RUNTIME_NAMES: &[&str] = &[
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

/// (wave 28e, E19 / F27-2) `Get-CommandLineGap`: whether a command line read for a process says
/// what it runs: `""` when it does; `unreadable` when it could not be read (`""` - access denied -
/// or ps's `[name]`); `no arguments` when the process is a generic runtime and its command line
/// holds nothing beyond the executable. Pure.
pub fn command_line_gap(name: &str, cmd: &str) -> &'static str {
    let c = cmd.trim();
    let bracketed =
        c.len() >= 2 && c.starts_with('[') && c.ends_with(']') && !c[1..c.len() - 1].contains(']');
    if c.is_empty() || bracketed {
        return "unreadable";
    }
    let base = strip_exe(name).to_lowercase();
    if GENERIC_RUNTIME_NAMES.contains(&base.as_str()) {
        let rest = if let Some(after) = c.strip_prefix('"') {
            match after.find('"') {
                Some(close) => &after[close + 1..],
                None => "",
            }
        } else {
            match c.find([' ', '\t']) {
                Some(sp) => &c[sp..],
                None => "",
            }
        };
        if rest.trim().is_empty() {
            return "no arguments";
        }
    }
    ""
}

/// (wave 29, E27; E28 / F37-1) `Split-CommandLineTokens`: the arguments of a Windows command line
/// as the program itself sees them - the rules of the Rust standard library (the Codex CLI is a
/// Rust program) and of the Microsoft C runtime: (tokens, the program name first; `""` or why the
/// split is not certain).
///   the program name: a quote toggles quoting (no escapes in it), a blank outside quotes ends it;
///   an argument: blanks outside quotes separate arguments; n backslashes followed by a quote
///     become n/2 backslashes, and an odd n makes that quote a literal one (\" is a quote inside a
///     value, it does NOT toggle quoting); backslashes not followed by a quote are literal; inside
///     quotes "" is one literal quote; any other quote toggles quoting; "" outside quotes is an
///     empty argument.
/// Ambiguous: the command line ends inside quotes (unbalanced quoting) - the program could see
/// other arguments than the ones split here. Pure.
pub fn split_command_line_tokens(cmd: &str) -> (Vec<String>, String) {
    let s: Vec<char> = cmd.chars().collect();
    let n = s.len();
    let mut tokens: Vec<String> = Vec::new();
    let mut sb = String::new();
    let mut i = 0;
    while i < n && (s[i] == ' ' || s[i] == '\t') {
        i += 1;
    }
    // the program name
    let mut quoted = false;
    let mut has = false;
    while i < n {
        let ch = s[i];
        if ch == '"' {
            quoted = !quoted;
            has = true;
            i += 1;
            continue;
        }
        if !quoted && (ch == ' ' || ch == '\t') {
            break;
        }
        sb.push(ch);
        has = true;
        i += 1;
    }
    if quoted {
        if has {
            tokens.push(sb);
        }
        return (
            tokens,
            "the quote of the program name is not closed".to_string(),
        );
    }
    if has {
        tokens.push(std::mem::take(&mut sb));
    }
    sb.clear();
    has = false;
    // the arguments
    while i < n {
        let ch = s[i];
        if !quoted && (ch == ' ' || ch == '\t') {
            if has {
                tokens.push(std::mem::take(&mut sb));
                has = false;
            }
            i += 1;
            continue;
        }
        if ch == '\\' {
            let mut k = i;
            while k < n && s[k] == '\\' {
                k += 1;
            }
            let count = k - i;
            if k < n && s[k] == '"' {
                sb.push_str(&"\\".repeat(count / 2));
                if count % 2 == 1 {
                    sb.push('"');
                    k += 1;
                }
            } else {
                sb.push_str(&"\\".repeat(count));
            }
            has = true;
            i = k;
            continue;
        }
        if ch == '"' {
            if quoted {
                if i + 1 < n && s[i + 1] == '"' {
                    sb.push('"');
                    i += 2;
                    continue;
                }
                quoted = false;
            } else {
                quoted = true;
                has = true;
            }
            i += 1;
            continue;
        }
        sb.push(ch);
        has = true;
        i += 1;
    }
    if has {
        tokens.push(sb);
    }
    let why = if quoted {
        "unbalanced quoting (the command line ends inside quotes)".to_string()
    } else {
        String::new()
    };
    (tokens, why)
}

/// (wave 29, E28 / F37-1) `Test-ExecWord`: whether a text holds the word exec - a whole word in
/// the sense of the command line, where a hyphen belongs to the word (exec-server and --exec are
/// not it; exec, "exec", =exec and \exec\ are), case-insensitive. Pure.
pub fn is_exec_word(text: &str) -> bool {
    let word = |c: char| c.is_alphanumeric() || c == '_' || c == '-';
    let lower: Vec<char> = text.chars().flat_map(|c| c.to_lowercase()).collect();
    let target = ['e', 'x', 'e', 'c'];
    if lower.len() < 4 {
        return false;
    }
    (0..=lower.len() - 4).any(|i| {
        lower[i..i + 4] == target
            && (i == 0 || !word(lower[i - 1]))
            && (i + 4 == lower.len() || !word(lower[i + 4]))
    })
}

/// (wave 29, E27; E28 / F37-1) `Get-CodexServerExclusion`'s result.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerExclusion {
    /// `""` or what the process is (`codex app-server`).
    pub excluded: String,
    /// `""` or why its command line cannot be split with certainty.
    pub ambiguous: String,
}

/// (wave 29, E27; E28 / F37-1) `Get-CodexServerExclusion`: whether a codex-named process (its
/// name, without .exe, starts with codex) is one of the Codex desktop app's servers or helpers -
/// never a reviewer run. In this order:
///   (E28) a command line READ that holds the word exec anywhere ([`is_exec_word`] on the raw
///     text, quotes ignored, and on every argument as the program sees it) is NEVER excluded - a
///     reviewer run always carries exec, the app's servers never do;
///   an executable named codex-computer-use* (the app's computer-use helper): "<name> helper"
///     (also when its command line cannot be read);
///   a command line that cannot be read ([`command_line_gap`]): not excluded - unknown stays
///     suspicious (fail-closed);
///   (E28) a command line whose split is ambiguous ([`split_command_line_tokens`]: unbalanced
///     quoting): not excluded, `ambiguous` set - the caller counts it as codex;
///   the first non-option token after the executable (Windows quoting; the value of a global
///     option such as -c key=value skipped with it) is app-server, exec-server, mcp-server, login
///     or app: "codex <subcommand>";
///   --parent-pid (and no exec, above): "codex helper (--parent-pid, no exec)".
/// Anything else is neither, and the rule decides as before. Pure.
pub fn codex_server_exclusion(name: &str, cmd: &str) -> ServerExclusion {
    let none = ServerExclusion::default();
    let base = strip_exe(name);
    if !base.to_ascii_lowercase().starts_with("codex") {
        return none;
    }
    let gap = command_line_gap(name, cmd);
    let mut split: Option<(Vec<String>, String)> = None;
    if gap.is_empty() {
        if is_exec_word(cmd) {
            return none;
        }
        let s = split_command_line_tokens(cmd);
        if s.0.iter().any(|t| is_exec_word(t)) {
            return none;
        }
        split = Some(s);
    }
    if base.to_ascii_lowercase().starts_with("codex-computer-use") {
        return ServerExclusion {
            excluded: format!("{} helper", base.to_lowercase()),
            ambiguous: String::new(),
        };
    }
    let Some((tokens, ambiguous)) = split else {
        return none;
    };
    if !ambiguous.is_empty() {
        return ServerExclusion {
            excluded: String::new(),
            ambiguous,
        };
    }
    let mut sub = "";
    let mut i = 1;
    while i < tokens.len() {
        let t = tokens[i].as_str();
        if t == "--" {
            if i + 1 < tokens.len() {
                sub = &tokens[i + 1];
            }
            break;
        }
        if t.len() > 1 && t.starts_with('-') {
            if !t.contains('=') && CODEX_VALUE_OPTIONS.contains(&t) {
                i += 1;
            }
            i += 1;
            continue;
        }
        sub = t;
        break;
    }
    if !sub.is_empty()
        && CODEX_SERVER_SUBCOMMANDS
            .iter()
            .any(|c| c.eq_ignore_ascii_case(sub))
    {
        return ServerExclusion {
            excluded: format!("codex {}", sub.to_lowercase()),
            ambiguous: String::new(),
        };
    }
    // exec is nowhere on this command line (above)
    let parent_pid = tokens.iter().skip(1).any(|t| {
        t.eq_ignore_ascii_case("--parent-pid")
            || t.get(..13)
                .is_some_and(|head| head.eq_ignore_ascii_case("--parent-pid="))
    });
    if parent_pid {
        return ServerExclusion {
            excluded: "codex helper (--parent-pid, no exec)".to_string(),
            ambiguous: String::new(),
        };
    }
    none
}

/// (wave 3c, F23-2) What a scan says of a row whose start time could not be read: it is counted.
pub const START_UNREADABLE_COUNTED: &str = "its start time cannot be read - counted (fail-closed)";

/// `Find-CodexProcesses` over a given process list (pure). `since_text` is the pre-formatted
/// "started at or after" stamp for the message; `since` is the same instant for the comparison.
/// With a bridge pid on Windows, a process counts when its parent is that pid and it started at or
/// after `since` (and, if the pid was reused by a live process, before that reuse). Otherwise the
/// machine-wide "looks like codex" rule applies (labelled "task not verifiable"); (wave 29, E27) the
/// Codex desktop app's servers and helpers ([`codex_server_exclusion`]) are left out and named in
/// `check` ("excluded: pid N codex.exe [codex app-server]", at most 6, then "(+k more)") and in
/// `excluded`. This process and its ancestors are never counted. (wave 3c, F23-2) A row whose start
/// time could not be read (`created: None`) is never skipped: it counts as started at or after
/// `since` (and before a reuse of the bridge's pid), its rule says so ([`START_UNREADABLE_COUNTED`]).
/// The plugin's `Win32_Process.CreationDate` is always read; C3's table (`GetProcessTimes`) cannot
/// read a protected or another user's process, which must never release a record by being skipped.
#[allow(clippy::too_many_arguments)]
pub fn find_codex_processes(
    procs: &[ScanProc],
    since: chrono::DateTime<chrono::Utc>,
    since_text: &str,
    launcher: &str,
    bridge_pid: u32,
    self_pid: u32,
    on_windows: bool,
) -> ScanOutcome {
    let by_parent = on_windows && bridge_pid > 0;
    let rules = if by_parent {
        format!("children of the interrupted bridge pid {bridge_pid}")
    } else {
        "name codex*, or a command line containing the recorded launcher or @openai/codex; not the Codex app's servers and helpers"
            .to_string()
    };
    let scanner = if on_windows {
        "Win32_Process scan"
    } else {
        "ps scan"
    };
    let mut check = format!("{scanner} ({rules}; started at or after {since_text})");

    use std::collections::{HashMap, HashSet};
    let by_id: HashMap<u32, &ScanProc> = procs.iter().map(|p| (p.pid, p)).collect();
    // Never count ourselves or our ancestors (a shell that started this run may carry the launcher
    // path on its command line).
    let mut excluded: HashSet<u32> = HashSet::new();
    let mut cur = self_pid;
    let mut guard = 0;
    while guard < 64 && cur > 0 && !excluded.contains(&cur) {
        excluded.insert(cur);
        match by_id.get(&cur) {
            Some(p) => cur = p.ppid,
            None => break,
        }
        guard += 1;
    }

    let mut found = Vec::new();
    let mut app_servers: Vec<ExcludedProc> = Vec::new();
    if by_parent {
        // A live process holding the bridge's pid now is a reuse; only children created before it
        // started are ours.
        let reused_at = by_id.get(&bridge_pid).and_then(|p| p.created);
        for p in procs {
            if excluded.contains(&p.pid) || p.ppid != bridge_pid {
                continue;
            }
            let rule = match p.created {
                // (wave 3c, F23-2) a child whose start time cannot be read is never filtered out:
                // it may have started at or after `since` and before any reuse (fail-closed)
                None => format!(
                    "child of the interrupted bridge (ppid {bridge_pid}); {START_UNREADABLE_COUNTED}"
                ),
                Some(c) if c < since => continue,
                Some(c) => {
                    if let Some(r) = reused_at {
                        if c >= r {
                            continue;
                        }
                    }
                    format!("child of the interrupted bridge (ppid {bridge_pid})")
                }
            };
            found.push(FoundProc {
                pid: p.pid,
                name: p.name.clone(),
                rule,
            });
        }
    } else {
        for p in procs {
            if excluded.contains(&p.pid) {
                continue;
            }
            // (wave 3c, F23-2) a start time that cannot be read counts as "at or after `since`":
            // such a process is judged by the rule like any recent one (fail-closed)
            let start_unknown = match p.created {
                None => true,
                Some(c) if c < since => continue,
                Some(_) => false,
            };
            let m = codex_match(&p.name, &p.command_line, launcher);
            if !m.rule.is_empty() {
                let unknown = if start_unknown {
                    format!("; {START_UNREADABLE_COUNTED}")
                } else {
                    String::new()
                };
                found.push(FoundProc {
                    pid: p.pid,
                    name: p.name.clone(),
                    rule: format!("{}, task not verifiable{unknown}", m.rule),
                });
            } else if !m.excluded.is_empty() {
                app_servers.push(ExcludedProc {
                    pid: p.pid,
                    name: p.name.clone(),
                    why: m.excluded,
                });
            }
        }
        // (wave 29, E27) what was left out, said where the scan is reported (at most 6 named)
        if !app_servers.is_empty() {
            let shown = app_servers
                .iter()
                .take(6)
                .map(|e| format!("pid {} {} [{}]", e.pid, e.name, e.why))
                .collect::<Vec<_>>()
                .join(", ");
            let more = if app_servers.len() > 6 {
                format!(" (+{} more)", app_servers.len() - 6)
            } else {
                String::new()
            };
            check.pop();
            check.push_str(&format!("; excluded: {shown}{more})"));
        }
    }
    ScanOutcome {
        found,
        check,
        failed: false,
        excluded: app_servers,
    }
}

/// The whole process table for the scan (Toolhelp names/parents + `GetProcessTimes` start times on
/// Windows; command lines are left empty and fetched per-candidate by [`process_command_line`]).
pub fn enumerate_processes() -> Vec<ScanProc> {
    imp::enumerate_processes().unwrap_or_default()
}

/// [`enumerate_processes`], saying when the table could not be read (`Find-CodexProcesses`'
/// `Failed`; `Get-DescendantTree`'s `Denied`): the snapshot failed, or it came back empty.
pub fn enumerate_processes_checked() -> Result<Vec<ScanProc>, String> {
    let t = imp::enumerate_processes()?;
    if t.is_empty() {
        return Err("the process table came back empty".to_string());
    }
    Ok(t)
}

/// A process's command line, best effort (empty when it cannot be read — another user's process, a
/// protected process, or access denied). Windows: `NtQueryInformationProcess`
/// `ProcessCommandLineInformation`.
pub fn process_command_line(pid: u32) -> String {
    imp::process_command_line(pid)
}

/// The live descendants of `pid` (children first, then their children, ...), from one read of the
/// process table. Used by the non-Windows tree kill, where no `taskkill /T` exists: a reviewer
/// launcher's own children would otherwise outlive the kill as orphans.
pub fn descendants_of(pid: u32) -> Vec<u32> {
    descendants_in(&enumerate_processes(), pid)
}

/// [`descendants_of`] over a given process table (pure): every row is visited at most once, so
/// the walk is bounded by the table itself and never truncates (F09-4).
pub fn descendants_in(procs: &[ScanProc], pid: u32) -> Vec<u32> {
    let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
    seen.insert(pid);
    let mut out = Vec::new();
    let mut frontier = vec![pid];
    while let Some(parent) = frontier.pop() {
        for p in procs.iter().filter(|p| p.ppid == parent) {
            if seen.insert(p.pid) {
                out.push(p.pid);
                frontier.push(p.pid);
            }
        }
    }
    out
}

#[cfg(windows)]
#[allow(clippy::upper_case_acronyms)]
mod imp {
    type DWORD = u32;
    type BOOL = i32;
    type HANDLE = isize;

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct FILETIME {
        low: DWORD,
        high: DWORD,
    }

    const PROCESS_QUERY_LIMITED_INFORMATION: DWORD = 0x1000;
    const PROCESS_TERMINATE: DWORD = 0x0001;
    const ERROR_ACCESS_DENIED: DWORD = 5;
    const SYNCHRONIZE: DWORD = 0x0010_0000;
    const INFINITE: DWORD = 0xFFFF_FFFF;
    const TH32CS_SNAPPROCESS: DWORD = 0x0000_0002;
    const INVALID_HANDLE_VALUE: HANDLE = -1;

    #[repr(C)]
    struct PROCESSENTRY32W {
        dw_size: DWORD,
        cnt_usage: DWORD,
        th32_process_id: DWORD,
        th32_default_heap_id: usize,
        th32_module_id: DWORD,
        cnt_threads: DWORD,
        th32_parent_process_id: DWORD,
        pc_pri_class_base: i32,
        dw_flags: DWORD,
        sz_exe_file: [u16; 260],
    }

    extern "system" {
        fn OpenProcess(access: DWORD, inherit: BOOL, pid: DWORD) -> HANDLE;
        fn CloseHandle(h: HANDLE) -> BOOL;
        fn GetProcessTimes(
            h: HANDLE,
            creation: *mut FILETIME,
            exit: *mut FILETIME,
            kernel: *mut FILETIME,
            user: *mut FILETIME,
        ) -> BOOL;
        fn WaitForSingleObject(h: HANDLE, ms: DWORD) -> DWORD;
        fn GetLastError() -> DWORD;
        fn TerminateProcess(h: HANDLE, code: u32) -> BOOL;
        fn CreateToolhelp32Snapshot(flags: DWORD, pid: DWORD) -> HANDLE;
        fn Process32FirstW(snap: HANDLE, entry: *mut PROCESSENTRY32W) -> BOOL;
        fn Process32NextW(snap: HANDLE, entry: *mut PROCESSENTRY32W) -> BOOL;
    }

    extern "system" {
        // ntdll: NTSTATUS NtQueryInformationProcess(HANDLE, PROCESSINFOCLASS, PVOID, ULONG, PULONG)
        fn NtQueryInformationProcess(
            handle: HANDLE,
            class: u32,
            info: *mut core::ffi::c_void,
            info_len: u32,
            ret_len: *mut u32,
        ) -> i32;
    }

    /// The whole process table (pid, ppid, image name, creation time). Command lines are left empty
    /// and fetched per-candidate later (`process_command_line`). `Err` when the snapshot fails.
    pub fn enumerate_processes() -> Result<Vec<super::ScanProc>, String> {
        let mut out = Vec::new();
        // SAFETY: the snapshot handle is checked and closed; the PROCESSENTRY32W is owned and its
        // dw_size is set as the API requires.
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE || snap == 0 {
                return Err(format!(
                    "the process table could not be read (CreateToolhelp32Snapshot failed, error {})",
                    GetLastError()
                ));
            }
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dw_size = std::mem::size_of::<PROCESSENTRY32W>() as DWORD;
            if Process32FirstW(snap, &mut entry) != 0 {
                loop {
                    let pid = entry.th32_process_id;
                    let ppid = entry.th32_parent_process_id;
                    let n = entry
                        .sz_exe_file
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(entry.sz_exe_file.len());
                    let name = String::from_utf16_lossy(&entry.sz_exe_file[..n]);
                    // the start time as the kernel reports it (no test hook: the scan reads the
                    // creation date, as the plugin's Win32_Process scan does)
                    let created = process_start_iso(pid).and_then(|s| {
                        if s.is_empty() {
                            None
                        } else {
                            chrono::DateTime::parse_from_rfc3339(&s)
                                .ok()
                                .map(|d| d.with_timezone(&chrono::Utc))
                        }
                    });
                    out.push(super::ScanProc {
                        pid,
                        ppid,
                        name,
                        created,
                        command_line: String::new(),
                    });
                    if Process32NextW(snap, &mut entry) == 0 {
                        break;
                    }
                }
            }
            CloseHandle(snap);
        }
        Ok(out)
    }

    /// The image name and the parent pid of `pid` from a Toolhelp snapshot, `None` when no such
    /// process is listed.
    pub fn name_and_parent(pid: u32) -> Option<(String, u32)> {
        // SAFETY: as `parent_pid`.
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE || snap == 0 {
                return None;
            }
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dw_size = std::mem::size_of::<PROCESSENTRY32W>() as DWORD;
            let mut found = None;
            if Process32FirstW(snap, &mut entry) != 0 {
                loop {
                    if entry.th32_process_id == pid {
                        let n = entry
                            .sz_exe_file
                            .iter()
                            .position(|&c| c == 0)
                            .unwrap_or(entry.sz_exe_file.len());
                        found = Some((
                            String::from_utf16_lossy(&entry.sz_exe_file[..n]),
                            entry.th32_parent_process_id,
                        ));
                        break;
                    }
                    if Process32NextW(snap, &mut entry) == 0 {
                        break;
                    }
                }
            }
            CloseHandle(snap);
            found
        }
    }

    /// `TerminateProcess` on `pid` (best effort).
    pub fn terminate_pid(pid: u32) {
        // SAFETY: OpenProcess returns 0 on failure; the handle is closed after use.
        unsafe {
            let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if h == 0 {
                return;
            }
            TerminateProcess(h, 1);
            CloseHandle(h);
        }
    }

    /// The command line of `pid` via `NtQueryInformationProcess(ProcessCommandLineInformation)`
    /// (class 60, Windows 8.1+). Best effort: `""` on any failure — a process of another user, a
    /// protected process, or a denied query. The returned buffer is a `UNICODE_STRING` (16 bytes on
    /// x64) whose string data follows it in the same allocation.
    pub fn process_command_line(pid: u32) -> String {
        const PROCESS_COMMAND_LINE_INFORMATION: u32 = 60;
        const STATUS_INFO_LENGTH_MISMATCH: i32 = i32::from_ne_bytes(0xC000_0004u32.to_ne_bytes());
        // SAFETY: the handle is closed on every path; NtQueryInformationProcess writes at most
        // `info_len` bytes into a buffer we own, and reports the needed length via `ret_len`.
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h == 0 {
                return String::new();
            }
            let mut ret_len: u32 = 0;
            let status = NtQueryInformationProcess(
                h,
                PROCESS_COMMAND_LINE_INFORMATION,
                std::ptr::null_mut(),
                0,
                &mut ret_len,
            );
            if (status != STATUS_INFO_LENGTH_MISMATCH && status != 0) || ret_len == 0 {
                CloseHandle(h);
                return String::new();
            }
            let mut buf = vec![0u8; ret_len as usize];
            let status = NtQueryInformationProcess(
                h,
                PROCESS_COMMAND_LINE_INFORMATION,
                buf.as_mut_ptr() as *mut core::ffi::c_void,
                ret_len,
                &mut ret_len,
            );
            CloseHandle(h);
            if status != 0 {
                return String::new();
            }
            // UNICODE_STRING { USHORT Length; USHORT MaximumLength; [pad] PWSTR Buffer }: 16 bytes
            // on x64, 8 on x86 (= 2 * pointer size); the string bytes follow it in the same buffer.
            let header = 2 * std::mem::size_of::<usize>();
            if buf.len() < 2 {
                return String::new();
            }
            let length = u16::from_ne_bytes([buf[0], buf[1]]) as usize; // bytes
            if length == 0 || header + length > buf.len() {
                return String::new();
            }
            let (pairs, _) = buf[header..header + length].as_chunks::<2>();
            let units: Vec<u16> = pairs.iter().map(|c| u16::from_ne_bytes(*c)).collect();
            String::from_utf16_lossy(&units)
        }
    }

    /// The parent pid of `pid` from a Toolhelp process snapshot, or `None`.
    pub fn parent_pid(pid: u32) -> Option<u32> {
        // SAFETY: CreateToolhelp32Snapshot returns INVALID_HANDLE_VALUE on failure; the handle is
        // closed before returning. Process32FirstW/NextW write into a PROCESSENTRY32W we own whose
        // dw_size we set as the API requires.
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE || snap == 0 {
                return None;
            }
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dw_size = std::mem::size_of::<PROCESSENTRY32W>() as DWORD;
            let mut found = None;
            if Process32FirstW(snap, &mut entry) != 0 {
                loop {
                    if entry.th32_process_id == pid {
                        found = Some(entry.th32_parent_process_id);
                        break;
                    }
                    if Process32NextW(snap, &mut entry) == 0 {
                        break;
                    }
                }
            }
            CloseHandle(snap);
            found
        }
    }

    /// Hold a handle to the bridge process and force-exit this process the moment it terminates.
    /// A held handle stays bound to the ORIGINAL process object, so — unlike `OpenProcess` +
    /// `GetProcessTimes`, which keeps succeeding for a terminated process whose handle another
    /// process (the harness) still holds — `WaitForSingleObject` signals exactly at termination and
    /// is immune to pid reuse. If the bridge cannot be opened (already gone), no watchdog is armed.
    pub fn spawn_bridge_watchdog(pid: u32) {
        // SAFETY: OpenProcess returns 0 on failure; the handle is waited on and closed in the
        // spawned thread. WaitForSingleObject blocks until the process object is signaled.
        let h = unsafe { OpenProcess(SYNCHRONIZE, 0, pid) };
        if h == 0 {
            return;
        }
        std::thread::spawn(move || {
            unsafe {
                WaitForSingleObject(h, INFINITE);
                CloseHandle(h);
            }
            std::process::exit(1);
        });
    }

    pub fn process_start_iso(pid: u32) -> Option<String> {
        // SAFETY: OpenProcess with a query access right returns 0 on failure; the handle is
        // closed before returning. GetProcessTimes writes into stack FILETIMEs we own.
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h == 0 {
                // (wave 28e, E19) a process that exists but denies the query: its start time
                // cannot be read (`Get-ProcessStartIso`'s ''); any other failure (no such pid): gone
                return (GetLastError() == ERROR_ACCESS_DENIED).then(String::new);
            }
            let mut creation = FILETIME::default();
            let mut exit = FILETIME::default();
            let mut kernel = FILETIME::default();
            let mut user = FILETIME::default();
            let ok = GetProcessTimes(h, &mut creation, &mut exit, &mut kernel, &mut user);
            CloseHandle(h);
            if ok == 0 {
                return Some(String::new());
            }
            // A terminated process whose handle another process still holds (e.g. a harness that
            // launched it and keeps its Process object) stays openable, but its exit FILETIME is
            // set. Treat it as gone — matching the plugin's `Get-Process`, which never returns a
            // dead process — so a recovery record its (now dead) writer left reads inactive.
            if exit.low != 0 || exit.high != 0 {
                return None;
            }
            let ft = ((creation.high as u64) << 32) | (creation.low as u64);
            Some(filetime_to_iso(ft))
        }
    }

    /// A creation `FILETIME` (100 ns ticks since 1601-01-01 UTC) as .NET's `o` string in
    /// UTC: `yyyy-MM-ddTHH:mm:ss.fffffffZ`.
    fn filetime_to_iso(ft: u64) -> String {
        // Ticks since the Unix epoch (1601 -> 1970 is 11 644 473 600 seconds).
        const UNIX_OFFSET_100NS: u64 = 116_444_736_000_000_000;
        if ft < UNIX_OFFSET_100NS {
            return String::new();
        }
        let ticks = ft - UNIX_OFFSET_100NS; // 100 ns since Unix epoch
        let secs = (ticks / 10_000_000) as i64;
        let sub_100ns = (ticks % 10_000_000) as u32; // 0..=9_999_999
        match chrono::DateTime::from_timestamp(secs, sub_100ns * 100) {
            Some(dt) => {
                let base = dt.format("%Y-%m-%dT%H:%M:%S").to_string();
                format!("{base}.{sub_100ns:07}Z")
            }
            None => String::new(),
        }
    }
}

#[cfg(not(windows))]
mod imp {
    /// The process table via `/proc` (best effort): pid, ppid, comm, start time. Command lines are
    /// left empty and fetched per-candidate. Non-Windows is a best-effort fallback (the harnesses
    /// run on Windows).
    pub fn enumerate_processes() -> Result<Vec<super::ScanProc>, String> {
        let mut out = Vec::new();
        let rd = match std::fs::read_dir("/proc") {
            Ok(r) => r,
            Err(e) => return Err(format!("the process table could not be read (/proc: {e})")),
        };
        for ent in rd.flatten() {
            let name = ent.file_name();
            let pid: u32 = match name.to_string_lossy().parse() {
                Ok(p) => p,
                Err(_) => continue,
            };
            let ppid = super::imp::parent_pid(pid).unwrap_or(0);
            let comm = std::fs::read_to_string(format!("/proc/{pid}/comm"))
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            let created = process_start_iso(pid).and_then(|s| {
                if s.is_empty() {
                    None
                } else {
                    chrono::DateTime::parse_from_rfc3339(&s)
                        .ok()
                        .map(|d| d.with_timezone(&chrono::Utc))
                }
            });
            out.push(super::ScanProc {
                pid,
                ppid,
                name: comm,
                created,
                command_line: String::new(),
            });
        }
        Ok(out)
    }

    /// The name (`/proc/<pid>/comm`) and the parent pid of `pid`, `None` when it does not exist.
    pub fn name_and_parent(pid: u32) -> Option<(String, u32)> {
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
        Some((comm.trim().to_string(), parent_pid(pid).unwrap_or(0)))
    }

    /// `kill -9 <pid>` (best effort).
    pub fn terminate_pid(pid: u32) {
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }

    /// The command line of `pid` from `/proc/<pid>/cmdline` (NUL-separated), best effort.
    pub fn process_command_line(pid: u32) -> String {
        std::fs::read(format!("/proc/{pid}/cmdline"))
            .map(|b| {
                b.split(|&c| c == 0)
                    .map(|s| String::from_utf8_lossy(s).to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default()
    }

    /// Poll the bridge pid (best effort) and force-exit when it disappears. No held-handle wait on
    /// Unix; the start-time check guards against pid reuse.
    pub fn spawn_bridge_watchdog(pid: u32) {
        let start = super::process_start_iso(pid).unwrap_or_default();
        if !super::pid_alive(pid, &start) {
            return;
        }
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if !super::pid_alive(pid, &start) {
                std::process::exit(1);
            }
        });
    }

    /// The parent pid of `pid` from `/proc/<pid>/stat` (field 4), or `None`.
    pub fn parent_pid(pid: u32) -> Option<u32> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // `pid (comm) state ppid ...` — comm may contain spaces/parens, so split after the last ')'.
        let rest = stat.rsplit_once(')').map(|(_, r)| r).unwrap_or(&stat);
        let mut it = rest.split_whitespace();
        let _state = it.next()?;
        it.next()?.parse::<u32>().ok()
    }

    /// The start time of `pid` as .NET's `o` string in UTC, from `/proc/<pid>/stat` field 22
    /// (clock ticks since boot, `USER_HZ` = 100 on Linux) plus `btime` of `/proc/stat`; a zombie
    /// (state `Z`) counts as gone, like a Windows process with an exit time. `Some("")` when the
    /// process exists but the start time cannot be read (no `/proc`: a `kill -0` probe), so the
    /// comparison falls back to the plugin's sub-second tolerance.
    pub fn process_start_iso(pid: u32) -> Option<String> {
        if std::path::Path::new("/proc/self").exists() {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
            return Some(start_iso_from_stat(&stat, boot_time_secs()).unwrap_or_default());
        }
        // Fallback: kill -0 exits 0 when the process exists.
        let ok = std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            Some(String::new())
        } else {
            None
        }
    }

    /// `btime` (seconds since the epoch at boot) from `/proc/stat`, when readable.
    fn boot_time_secs() -> Option<i64> {
        let stat = std::fs::read_to_string("/proc/stat").ok()?;
        stat.lines()
            .find_map(|l| l.strip_prefix("btime "))
            .and_then(|v| v.trim().parse::<i64>().ok())
    }

    /// The `o`-string start time from one `/proc/<pid>/stat` line and the boot time: `None` for
    /// a zombie or an unreadable field (the caller then reports a blank start time).
    pub(super) fn start_iso_from_stat(stat: &str, btime: Option<i64>) -> Option<String> {
        // `pid (comm) state ppid ...` — comm may contain spaces/parens, so split after the last ')'.
        let rest = stat.rsplit_once(')').map(|(_, r)| r).unwrap_or(stat);
        let fields: Vec<&str> = rest.split_whitespace().collect();
        // After the split, index 0 is the state (field 3); starttime is field 22, index 19.
        if fields.first().copied() == Some("Z") {
            return None;
        }
        let ticks: u64 = fields.get(19)?.parse().ok()?;
        let btime = btime?;
        const USER_HZ: u64 = 100;
        let secs = btime.checked_add((ticks / USER_HZ) as i64)?;
        let sub_100ns = ((ticks % USER_HZ) * (10_000_000 / USER_HZ)) as u32;
        let dt = chrono::DateTime::from_timestamp(secs, sub_100ns * 100)?;
        Some(format!(
            "{}.{sub_100ns:07}Z",
            dt.format("%Y-%m-%dT%H:%M:%S")
        ))
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn start_time_from_a_stat_line_with_a_boot_time() {
            // comm with spaces and parens; starttime (field 22) = 12 345 ticks = 123.45 s.
            let stat = "4242 (my (odd) comm) S 1 4242 4242 0 -1 4194560 100 0 0 0 5 3 0 0 20 0 1 0 12345 1000 200 18446744073709551615 1 1 0 0 0 0 0 0 0 0 0 0 17 0 0 0 0 0 0";
            let iso = super::start_iso_from_stat(stat, Some(1_700_000_000)).unwrap();
            assert_eq!(iso, "2023-11-14T22:15:23.4500000Z");
            // A zombie reads as gone; a missing boot time or field yields no start time.
            let zombie = stat.replacen(" S ", " Z ", 1);
            assert!(super::start_iso_from_stat(&zombie, Some(1_700_000_000)).is_none());
            assert!(super::start_iso_from_stat(stat, None).is_none());
            assert!(super::start_iso_from_stat("1 (x) S 0", Some(1)).is_none());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ancestor_accepts_own_parent_and_rejects_non_ancestors() {
        // This test process's own parent (the test runner) is an ancestor.
        let parent = imp::parent_pid(std::process::id())
            .expect("this process must have a discoverable parent");
        assert!(parent > 0);
        assert!(
            is_ancestor_of_self(parent),
            "the direct parent pid {parent} must count as an ancestor"
        );
        // pid 4 (Windows System) / a low system pid is never this test's ancestor.
        assert!(!is_ancestor_of_self(4));
        // This process's own pid is not its own ancestor.
        assert!(!is_ancestor_of_self(std::process::id()));
    }

    // ---- the machine-wide codex scan (rows (b)), proven over synthetic process lists ----

    fn at(secs: i64) -> Option<chrono::DateTime<chrono::Utc>> {
        chrono::DateTime::from_timestamp(1_700_000_000 + secs, 0)
    }

    fn p(pid: u32, ppid: u32, name: &str, created: i64, cmd: &str) -> ScanProc {
        ScanProc {
            pid,
            ppid,
            name: name.to_string(),
            created: at(created),
            command_line: cmd.to_string(),
        }
    }

    #[test]
    #[cfg(windows)] // the recorded launcher paths are Windows paths; std::path splits `\` only there
    fn codex_rule_matches_the_plugin() {
        assert_eq!(codex_rule("codex.exe", "", ""), "name codex");
        assert_eq!(codex_rule("CODEX", "", ""), "name codex");
        // recorded launcher basename: only an .exe/extensionless launcher matches by name (a .cmd
        // launcher matches by command line instead, below).
        assert_eq!(
            codex_rule("zcode.exe", "", r"C:\tools\zcode.exe"),
            "name zcode (the recorded launcher)"
        );
        assert_eq!(
            codex_rule("zcode.exe", "", r"C:\tools\zcode.cmd"),
            "",
            "a .cmd launcher does not match by process name"
        );
        // launcher path on the command line
        assert_eq!(
            codex_rule(
                "node.exe",
                r"node C:\tools\zcode.cmd run",
                r"C:\tools\zcode.cmd"
            ),
            "launcher in command line"
        );
        // the npm shim
        assert_eq!(
            codex_rule("node.exe", r"node C:\n\@openai\codex\bin\codex.js", ""),
            "@openai/codex in command line"
        );
        // unrelated
        assert_eq!(
            codex_rule("powershell.exe", "powershell -File x.ps1", ""),
            ""
        );
    }

    #[test]
    fn scan_by_parent_finds_children_since_and_honours_reuse_and_exclusion() {
        let since = at(100).unwrap();
        // bridge pid 1000 is gone (not in the list); its child 1200 started after `since`.
        // 1300 is an unrelated child of another pid. self is 42 with parent 7 — both excluded.
        let procs = vec![
            p(1200, 1000, "codex.exe", 150, ""),
            p(1300, 9, "codex.exe", 150, ""),
            p(42, 7, "c3.exe", 150, ""),
            p(7, 1, "shim.exe", 90, ""),
        ];
        let out = find_codex_processes(&procs, since, "2023-11-14T22:13:20", "", 1000, 42, true);
        assert!(out
            .check
            .contains("children of the interrupted bridge pid 1000"));
        assert_eq!(out.found.len(), 1);
        assert_eq!(out.found[0].pid, 1200);
        assert!(out.found[0]
            .rule
            .contains("child of the interrupted bridge (ppid 1000)"));

        // A live process now HOLDS the reused bridge pid (started at 140): only children created
        // before 140 count — 1200 (started 150) is now excluded.
        let mut procs2 = procs.clone();
        procs2.push(p(1000, 1, "other.exe", 140, ""));
        let out2 = find_codex_processes(&procs2, since, "t", "", 1000, 42, true);
        assert!(out2.found.is_empty(), "child after the reuse is not ours");
    }

    /// A row whose creation time could not be read (`created: None`).
    fn unknown_start(pid: u32, ppid: u32, name: &str, cmd: &str) -> ScanProc {
        ScanProc {
            created: None,
            ..p(pid, ppid, name, 0, cmd)
        }
    }

    #[test]
    fn f23_2_a_row_whose_start_cannot_be_read_is_never_skipped() {
        let since = at(100).unwrap();
        // by parent: the dead bridge 1000's children - one unreadable (counted, said so), one
        // started before the run (left out), one after it (counted)
        let procs = vec![
            unknown_start(1200, 1000, "node.exe", ""),
            p(1201, 1000, "ping.exe", 50, ""),
            p(1202, 1000, "codex.exe", 150, ""),
            unknown_start(1300, 9, "codex-like.exe", ""),
        ];
        let out = find_codex_processes(&procs, since, "t", "", 1000, 42, true);
        let found: Vec<(u32, &str)> = out.found.iter().map(|f| (f.pid, f.rule.as_str())).collect();
        assert_eq!(
            found,
            vec![
                (
                    1200,
                    "child of the interrupted bridge (ppid 1000); its start time cannot be read - counted (fail-closed)"
                ),
                (1202, "child of the interrupted bridge (ppid 1000)"),
            ]
        );
        // a live process holds the reused bridge pid: a child whose start cannot be read may predate
        // the reuse - still counted; a readable one after the reuse is not ours
        let mut reused = procs.clone();
        reused.push(p(1000, 1, "other.exe", 140, ""));
        let out = find_codex_processes(&reused, since, "t", "", 1000, 42, true);
        assert_eq!(
            out.found.iter().map(|f| f.pid).collect::<Vec<_>>(),
            vec![1200]
        );

        // machine-wide: an unreadable codex-like row is counted (and says why), an unreadable row
        // that is not codex-like is not (no blanket refusal), a codex row before the run is not; an
        // unreadable app server stays left out (named)
        let procs = vec![
            unknown_start(2000, 1, "codex.exe", ""),
            unknown_start(2001, 1, "csrss.exe", ""),
            p(2002, 1, "codex.exe", 50, ""),
            unknown_start(
                2003,
                1,
                "node.exe",
                r"node C:\x\node_modules\@openai\codex\bin\codex.js exec",
            ),
            unknown_start(2004, 1, "codex.exe", "codex.exe app-server"),
        ];
        let out = find_codex_processes(&procs, since, "t", "", 0, 42, true);
        let found: Vec<(u32, &str)> = out.found.iter().map(|f| (f.pid, f.rule.as_str())).collect();
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(found[0].0, 2000);
        assert_eq!(
            found[0].1,
            "name codex, task not verifiable; its start time cannot be read - counted (fail-closed)"
        );
        assert_eq!(found[1].0, 2003);
        assert!(
            found[1].1.ends_with(
                ", task not verifiable; its start time cannot be read - counted (fail-closed)"
            ),
            "{found:?}"
        );
        assert_eq!(
            out.excluded.iter().map(|e| e.pid).collect::<Vec<_>>(),
            vec![2004]
        );
    }

    #[test]
    fn scan_by_name_respects_since_and_self_exclusion() {
        let since = at(100).unwrap();
        let procs = vec![
            p(2000, 1, "codex.exe", 150, ""),                // matches, recent
            p(2001, 1, "codex.exe", 50, ""),                 // too old
            p(2002, 1, "powershell.exe", 150, "powershell"), // not codex
            p(42, 7, "codex.exe", 150, ""),                  // us — excluded
        ];
        let out = find_codex_processes(&procs, since, "t", "", 0, 42, true);
        assert!(out.check.contains(
            "name codex*, or a command line containing the recorded launcher or @openai/codex"
        ));
        assert_eq!(out.found.len(), 1);
        assert_eq!(out.found[0].pid, 2000);
        assert!(out.found[0].rule.ends_with(", task not verifiable"));
    }

    #[test]
    fn descendants_walk_the_whole_table_without_a_cap_or_a_loop() {
        // (F09-4) A wide tree (more than the old 4096 cap) is returned whole; a self-parented row
        // and a cycle never loop; the root itself is not a descendant.
        let mut procs: Vec<ScanProc> = (1..=5000).map(|i| p(100 + i, 100, "w", 1, "")).collect();
        procs.push(p(100, 1, "root", 0, ""));
        procs.push(p(9000, 5100, "grandchild", 2, ""));
        procs.push(p(9001, 9000, "great", 3, ""));
        procs.push(p(9001, 9001, "self-parented twin", 3, ""));
        let d = descendants_in(&procs, 100);
        assert_eq!(d.len(), 5002, "{}", d.len());
        assert!(d.contains(&9001) && d.contains(&9000) && !d.contains(&100));
        assert!(descendants_in(&procs, 9001).is_empty());
    }

    #[test]
    #[cfg(not(windows))] // the tick tolerance is the non-Windows rule; Windows compares exactly
    fn start_times_match_within_one_tick_only() {
        // (F09-3) Two `/proc` readings of one process agree to the tick; a reused pid differs by
        // at least one tick (10 ms) and must not compare equal.
        assert!(same_start_time(
            "2026-10-08T07:00:00.0000000Z",
            "2026-10-08T07:00:00.0050000Z"
        ));
        assert!(!same_start_time(
            "2026-10-08T07:00:00.0000000Z",
            "2026-10-08T07:00:00.0100000Z"
        ));
        assert!(!same_start_time(
            "2026-10-08T07:00:00.0000000Z",
            "2026-10-08T07:00:00.5000000Z"
        ));
        assert!(!same_start_time("2026-10-08T07:00:00.0000000Z", ""));
    }

    #[test]
    fn scan_by_name_finds_a_launcher_on_the_command_line() {
        let since = at(0).unwrap();
        let procs = vec![p(3000, 1, "node.exe", 10, r"node C:\tools\zcode.cmd exec")];
        let out = find_codex_processes(&procs, since, "t", r"C:\tools\zcode.cmd", 0, 1, true);
        assert_eq!(out.found.len(), 1);
        assert!(out.found[0].rule.starts_with("launcher in command line"));
    }

    // ---- (wave 29, E27 / E28) the Codex app's servers and helpers are no reviewer run ----

    /// The plugin's `harness-fixes.ps1` E27 UNIT samples (v0.6.1): (name, command line, recorded
    /// launcher, expected rule, expected exclusion) - the app's real lines.
    const E27_SAMPLES: &[(&str, &str, &str, &str, &str)] = &[
        (
            "codex.exe",
            r"C:\Users\u\AppData\Local\OpenAI\Codex\bin\5ea2\codex.exe -c features.code_mode_host=true app-server --analytics-default-enabled -c plugins.x=1",
            "",
            "",
            "codex app-server",
        ),
        (
            "codex.exe",
            r"C:\Users\u\AppData\Local\OpenAI\Codex\bin\5ea2\codex.exe exec-server --remote https://codex-cloud-environments.chatgpt.com/api --environment-id e1",
            "",
            "",
            "codex exec-server",
        ),
        (
            "codex-computer-use-swift.exe",
            r"C:\Users\u\AppData\Local\OpenAI\Codex\runtimes\cua_node\x\codex-computer-use-swift.exe --parent-pid 22140",
            "",
            "",
            "codex-computer-use-swift helper",
        ),
        ("codex", "codex mcp-server", "", "", "codex mcp-server"),
        ("codex", "codex login status", "", "", "codex login"),
        (
            "codex.exe",
            r#""C:\x y\codex.exe" app"#,
            "",
            "",
            "codex app",
        ),
        (
            "codex.exe",
            r#""C:\x\codex.exe" --parent-pid 7"#,
            "",
            "",
            "codex helper (--parent-pid, no exec)",
        ),
        (
            "codex.exe",
            r#""C:\t\codex.exe" app-server"#,
            r"C:\t\codex.exe",
            "",
            "codex app-server",
        ),
        ("codex.exe", "codex.exe exec --json -", "", "name codex", ""),
        (
            "codex",
            r#"codex exec --sandbox read-only --color never --json -m m1 -c model_reasoning_effort="high" -o C:\t\last.txt -"#,
            "",
            "name codex",
            "",
        ),
        (
            "codex.exe",
            r#""C:\x\codex.exe" exec --parent-pid 7"#,
            "",
            "name codex",
            "",
        ),
        (
            "codex.exe",
            "codex.exe -c app-server=1 exec -",
            "",
            "name codex",
            "",
        ),
        ("codex.exe", "codex.exe", "", "name codex", ""),
        ("codex.exe", "", "", "name codex", ""),
        ("codex", "[codex]", "", "name codex", ""),
        (
            "node.exe",
            r"node C:\npm\node_modules\@openai\codex\bin\codex.js exec -",
            "",
            "@openai/codex in command line",
            "",
        ),
        (
            "cmd.exe",
            r#"cmd /c "C:\t\fake-codex.cmd" exec -"#,
            r"C:\t\fake-codex.cmd",
            "launcher in command line",
            "",
        ),
    ];

    /// The plugin's `harness-fixes.ps1` E28 / F37-1 UNIT samples (v0.6.1): escaped quotes around
    /// app-server in a reviewer's -c value, exec hidden by quoting, the word exec inside a server's
    /// value, Windows quoting inside a real server's value, unbalanced quoting.
    const E28_SAMPLES: &[(&str, &str, &str, &str, &str)] = &[
        (
            "codex.exe",
            r#"codex.exe -c "developer_instructions=\"please app-server check\"" exec --json -"#,
            "",
            "name codex",
            "",
        ),
        (
            "codex.exe",
            r#"codex.exe -c developer_instructions="please \"app-server\" check" exec --json -"#,
            "",
            "name codex",
            "",
        ),
        (
            "codex.exe",
            r#"codex.exe -c "a=\"app-server\"" e"x"ec -"#,
            "",
            "name codex",
            "",
        ),
        (
            "codex.exe",
            r#"codex.exe -c "developer_instructions=never exec here" app-server"#,
            "",
            "name codex",
            "",
        ),
        (
            "codex.exe",
            r#"codex.exe -c "x=\"y\"" app-server --analytics-default-enabled"#,
            "",
            "",
            "codex app-server",
        ),
        (
            "codex.exe",
            r#"codex.exe -c "x=""y"" z" app-server"#,
            "",
            "",
            "codex app-server",
        ),
        (
            "codex.exe",
            r#"codex.exe -c "x=\"y app-server"#,
            "",
            "command line ambiguous - counted as codex",
            "",
        ),
        (
            "codex.exe",
            r#""C:\x\codex.exe app-server"#,
            "",
            "command line ambiguous - counted as codex",
            "",
        ),
        (
            "codex-command-runner.exe",
            r#"codex-command-runner.exe "x"#,
            "",
            "command line ambiguous - counted as codex",
            "",
        ),
    ];

    #[test]
    fn e27_codex_match_leaves_the_app_servers_out() {
        let mut bad: Vec<String> = Vec::new();
        for (name, cmd, launcher, rule, excluded) in E27_SAMPLES {
            let m = codex_match(name, cmd, launcher);
            let rr = codex_rule(name, cmd, launcher);
            if m.rule != *rule || m.excluded != *excluded || rr != *rule {
                bad.push(format!(
                    "{name} '{cmd}' -> rule '{}' / '{rr}' excluded '{}'",
                    m.rule, m.excluded
                ));
            }
        }
        assert!(bad.is_empty(), "{}", bad.join("\n"));
    }

    #[test]
    fn e28_exec_is_never_excluded_and_ambiguous_quoting_counts() {
        let mut bad: Vec<String> = Vec::new();
        for (name, cmd, launcher, rule, excluded) in E28_SAMPLES {
            let m = codex_match(name, cmd, launcher);
            if m.rule != *rule || m.excluded != *excluded {
                bad.push(format!(
                    "{name} '{cmd}' -> rule '{}' excluded '{}'",
                    m.rule, m.excluded
                ));
            }
        }
        assert!(bad.is_empty(), "{}", bad.join("\n"));
        let (tokens, ambiguous) = split_command_line_tokens(
            r#"codex.exe -c "developer_instructions=\"please app-server check\"" exec --json -"#,
        );
        assert_eq!(
            tokens.join("|"),
            r#"codex.exe|-c|developer_instructions="please app-server check"|exec|--json|-"#
        );
        assert_eq!(ambiguous, "");
        let (_, ambiguous) = split_command_line_tokens(r#"codex.exe -c "x=\"y app-server"#);
        assert_eq!(
            ambiguous,
            "unbalanced quoting (the command line ends inside quotes)"
        );
    }

    #[test]
    fn exec_word_is_a_whole_word_with_the_hyphen_inside() {
        for yes in [
            "exec",
            "codex exec -",
            r#""exec""#,
            "x=exec",
            r"\exec\",
            "EXEC",
        ] {
            assert!(is_exec_word(yes), "{yes}");
        }
        for no in ["exec-server", "--exec", "execute", "codexec", "my_exec", ""] {
            assert!(!is_exec_word(no), "{no}");
        }
    }

    #[test]
    fn command_line_gap_matches_the_plugin() {
        assert_eq!(command_line_gap("codex.exe", ""), "unreadable");
        assert_eq!(command_line_gap("codex", "[codex]"), "unreadable");
        assert_eq!(
            command_line_gap("node.exe", r#""C:\n\node.exe""#),
            "no arguments"
        );
        assert_eq!(command_line_gap("node.exe", "node"), "no arguments");
        assert_eq!(command_line_gap("node.exe", "node x.js"), "");
        assert_eq!(command_line_gap("codex.exe", "codex.exe"), "");
    }

    #[test]
    fn scan_by_name_leaves_the_app_servers_out_and_names_them() {
        let since = at(100).unwrap();
        let procs = vec![
            p(5000, 1, "codex.exe", 150, "codex.exe -c x=1 app-server"),
            p(
                5001,
                1,
                "codex.exe",
                150,
                r"C:\x\codex.exe exec-server --remote u",
            ),
            p(5002, 1, "codex.exe", 150, "codex.exe exec --json -"),
            p(5003, 1, "codex.exe", 150, ""),
        ];
        let out = find_codex_processes(&procs, since, "t", "", 0, 42, true);
        let found: Vec<u32> = out.found.iter().map(|f| f.pid).collect();
        assert_eq!(found, vec![5002, 5003], "{:?}", out.found);
        assert_eq!(
            out.check,
            "Win32_Process scan (name codex*, or a command line containing the recorded launcher or @openai/codex; not the Codex app's servers and helpers; started at or after t; excluded: pid 5000 codex.exe [codex app-server], pid 5001 codex.exe [codex exec-server])"
        );
        assert_eq!(out.excluded.len(), 2);
        assert_eq!(out.excluded[0].why, "codex app-server");
        // more than six left out: six named, then the count
        let many: Vec<ScanProc> = (0..8)
            .map(|i| p(6000 + i, 1, "codex.exe", 150, "codex.exe app-server"))
            .collect();
        let out = find_codex_processes(&many, since, "t", "", 0, 42, true);
        assert!(out.found.is_empty());
        assert!(
            out.check
                .ends_with("pid 6005 codex.exe [codex app-server] (+2 more))"),
            "{}",
            out.check
        );
    }

    // ---- (F04-1, RC1) a Unicode process name never panics the classifier or the scan ----

    #[test]
    fn strip_exe_is_boundary_safe() {
        assert_eq!(strip_exe("日本"), "日本");
        assert_eq!(strip_exe("日本.exe"), "日本");
        assert_eq!(strip_exe("日本.EXE"), "日本");
        assert_eq!(strip_exe("aé.ex"), "aé.ex");
        assert_eq!(strip_exe("codex.Exe"), "codex");
        assert_eq!(strip_exe(".exe"), "");
        assert_eq!(strip_exe("exe"), "exe");
        assert_eq!(strip_exe(""), "");
    }

    #[test]
    fn unicode_name_codex_match_completes_without_a_match() {
        let m = codex_match("日本", "unrelated", "");
        assert_eq!(m.rule, "");
        assert_eq!(m.excluded, "");
        assert_eq!(
            codex_server_exclusion("日本", "unrelated"),
            ServerExclusion::default()
        );
        assert_eq!(command_line_gap("日本", "unrelated"), "");
        // with a recorded launcher too (the launcher-basename branch strips the name)
        let m = codex_match("日本", "unrelated", r"C:\x\codex.exe");
        assert_eq!((m.rule.as_str(), m.excluded.as_str()), ("", ""));
    }

    #[test]
    fn unicode_name_scan_completes_without_a_match_or_an_exclusion() {
        let since = at(100).unwrap();
        let procs = vec![
            p(7000, 1, "日本", 150, "unrelated"),
            p(7001, 1, "日本", 150, "日本"),
            p(7002, 1, "codex.exe", 150, "codex.exe exec --json -"),
        ];
        for on_windows in [true, false] {
            let out = find_codex_processes(&procs, since, "t", "", 0, 42, on_windows);
            let found: Vec<u32> = out.found.iter().map(|f| f.pid).collect();
            assert_eq!(found, vec![7002], "{:?}", out.found);
            assert!(out.excluded.is_empty(), "{:?}", out.excluded);
            assert!(!out.check.contains("excluded:"), "{}", out.check);
        }
    }
}
