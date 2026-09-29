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
        let pid = std::env::var("CODEX_CONSULT_TEST_BRIDGE_PID")
            .ok()
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

    pub fn process_start_iso(pid: u32) -> Option<String> {
        if std::path::Path::new(&format!("/proc/{pid}")).exists() {
            return Some(String::new());
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
}
