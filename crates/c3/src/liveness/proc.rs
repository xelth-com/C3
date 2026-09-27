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
