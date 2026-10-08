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
/// `StartTime` throws).
pub fn process_start_iso(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    imp::process_start_iso(pid)
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

/// `Test-SameStartTime`: exact match on Windows; within one second elsewhere (a pid is not
/// reused that fast, and non-Windows clocks derive the value slightly differently).
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
        (Ok(ta), Ok(tb)) => (ta.timestamp_millis() - tb.timestamp_millis()).abs() < 1000,
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

/// The outcome of one scan pass (`Find-CodexProcesses`'s return object).
#[derive(Debug, Clone)]
pub struct ScanOutcome {
    pub found: Vec<FoundProc>,
    pub check: String,
    pub failed: bool,
}

/// `Get-CodexRule`: why a process looks like codex (the rule label), or `""` when it does not.
/// `name` is the image name (with or without `.exe`), `cmd` its command line, `launcher` the
/// recorded launcher path (empty when none).
pub fn codex_rule(name: &str, cmd: &str, launcher: &str) -> String {
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
        let name_base = if name_l.ends_with(".exe") {
            &name[..name.len() - 4]
        } else {
            name
        };
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

/// `Find-CodexProcesses` over a given process list (pure). `since_text` is the pre-formatted
/// "started at or after" stamp for the message; `since` is the same instant for the comparison.
/// With a bridge pid on Windows, a process counts when its parent is that pid and it started at or
/// after `since` (and, if the pid was reused by a live process, before that reuse). Otherwise the
/// machine-wide "looks like codex" rule applies (labelled "task not verifiable"). This process and
/// its ancestors are never counted.
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
        "name codex*, or a command line containing the recorded launcher or @openai/codex"
            .to_string()
    };
    let scanner = if on_windows {
        "Win32_Process scan"
    } else {
        "ps scan"
    };
    let check = format!("{scanner} ({rules}; started at or after {since_text})");

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
    if by_parent {
        // A live process holding the bridge's pid now is a reuse; only children created before it
        // started are ours.
        let reused_at = by_id.get(&bridge_pid).and_then(|p| p.created);
        for p in procs {
            if excluded.contains(&p.pid) || p.ppid != bridge_pid {
                continue;
            }
            match p.created {
                None => continue,
                Some(c) if c < since => continue,
                Some(c) => {
                    if let Some(r) = reused_at {
                        if c >= r {
                            continue;
                        }
                    }
                }
            }
            found.push(FoundProc {
                pid: p.pid,
                name: p.name.clone(),
                rule: format!("child of the interrupted bridge (ppid {bridge_pid})"),
            });
        }
    } else {
        for p in procs {
            if excluded.contains(&p.pid) {
                continue;
            }
            match p.created {
                None => continue,
                Some(c) if c < since => continue,
                _ => {}
            }
            let rule = codex_rule(&p.name, &p.command_line, launcher);
            if !rule.is_empty() {
                found.push(FoundProc {
                    pid: p.pid,
                    name: p.name.clone(),
                    rule: format!("{rule}, task not verifiable"),
                });
            }
        }
    }
    ScanOutcome {
        found,
        check,
        failed: false,
    }
}

/// The whole process table for the scan (Toolhelp names/parents + `GetProcessTimes` start times on
/// Windows; command lines are left empty and fetched per-candidate by [`process_command_line`]).
pub fn enumerate_processes() -> Vec<ScanProc> {
    imp::enumerate_processes()
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
    let procs = enumerate_processes();
    let mut out = Vec::new();
    let mut frontier = vec![pid];
    while let Some(parent) = frontier.pop() {
        for p in procs.iter().filter(|p| p.ppid == parent && p.pid != parent) {
            if !out.contains(&p.pid) && out.len() < 4096 {
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
    /// and fetched per-candidate later (`process_command_line`).
    pub fn enumerate_processes() -> Vec<super::ScanProc> {
        let mut out = Vec::new();
        // SAFETY: the snapshot handle is checked and closed; the PROCESSENTRY32W is owned and its
        // dw_size is set as the API requires.
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE || snap == 0 {
                return out;
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
                    let created = super::process_start_iso(pid).and_then(|s| {
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
        out
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
            let units: Vec<u16> = buf[header..header + length]
                .chunks_exact(2)
                .map(|c| u16::from_ne_bytes([c[0], c[1]]))
                .collect();
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
                return None;
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
    pub fn enumerate_processes() -> Vec<super::ScanProc> {
        let mut out = Vec::new();
        let rd = match std::fs::read_dir("/proc") {
            Ok(r) => r,
            Err(_) => return out,
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
            let created = super::process_start_iso(pid).and_then(|s| {
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
        out
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
    fn scan_by_name_finds_a_launcher_on_the_command_line() {
        let since = at(0).unwrap();
        let procs = vec![p(3000, 1, "node.exe", 10, r"node C:\tools\zcode.cmd exec")];
        let out = find_codex_processes(&procs, since, "t", r"C:\tools\zcode.cmd", 0, 1, true);
        assert_eq!(out.found.len(), 1);
        assert!(out.found[0].rule.starts_with("launcher in command line"));
    }
}
