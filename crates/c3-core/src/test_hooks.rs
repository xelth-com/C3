//! (wave 27c, D14) Test hooks cannot change production behaviour by accident.
//!
//! The bridge reads several `CODEX_CONSULT_TEST_*` environment variables to steer its tests
//! (shorter waits, seeded panel draws, forced survivors, a forced unconfirmed kill, …). A hook is
//! honoured ONLY when `CODEX_CONSULT_TEST_MODE` is set too; without the mode every hook is ignored
//! and the run warns ONCE that it saw one. Every read of a `CODEX_CONSULT_TEST_*` variable in the
//! runtime routes through [`hook`] (the harnesses and the cargo tests set the mode themselves; the
//! shims set it when the caller did not). `CODEX_CONSULT_TEST_MODE` itself is the gate and is read
//! only by [`mode_on`].

use std::sync::atomic::{AtomicBool, Ordering};

/// The gate variable name.
pub const MODE_VAR: &str = "CODEX_CONSULT_TEST_MODE";

/// (wave 28b, D10) The line every run in test mode says - on the console and once in the ledger's
/// `warnings[]` (`$script:TestModeWarning`), so test mode never goes unnoticed.
pub const TEST_MODE_WARNING: &str = "test mode is ON: test hooks are honoured";

/// (wave 28b, D10) `Test-TestVarName`: a test-mode variable (`CODEX_CONSULT_TEST_*`, the mode gate
/// included; any case). An engine child or a launcher probe never inherits one - only the bridge's
/// own processes (a panel member, the detached background) keep them.
pub fn is_test_var(name: &str) -> bool {
    name.to_ascii_uppercase().starts_with("CODEX_CONSULT_TEST_")
}

/// The test-mode warning when test mode is on (`None` otherwise).
pub fn test_mode_warning() -> Option<&'static str> {
    if mode_on() {
        Some(TEST_MODE_WARNING)
    } else {
        None
    }
}

/// One warning per run when a hook was seen without the mode (`Warn-TestHookIgnored`).
static WARNED_IGNORED: AtomicBool = AtomicBool::new(false);

/// Whether test mode is on: `CODEX_CONSULT_TEST_MODE` set to a truthy value (`1`/`true`/`yes`/`on`,
/// case-insensitively). An empty or `0`/`false` value is off (PowerShell-style truthiness).
pub fn mode_on() -> bool {
    match std::env::var(MODE_VAR) {
        Ok(v) => {
            let t = v.trim().to_ascii_lowercase();
            matches!(t.as_str(), "1" | "true" | "yes" | "on")
        }
        Err(_) => false,
    }
}

/// Read a `CODEX_CONSULT_TEST_*` hook variable, honoured only when [`mode_on`]. Returns `None` when
/// the variable is not set. When it IS set but the mode is off, the hook is ignored (returns
/// `None`) and the run warns once on stderr that it saw a test hook.
pub fn hook(name: &str) -> Option<String> {
    debug_assert!(
        name.starts_with("CODEX_CONSULT_TEST_") && name != MODE_VAR,
        "hook() is for CODEX_CONSULT_TEST_* variables other than the mode gate"
    );
    let raw = std::env::var(name).ok()?;
    if mode_on() {
        return Some(raw);
    }
    if !WARNED_IGNORED.swap(true, Ordering::SeqCst) {
        eprintln!(
            "codex-consult: a CODEX_CONSULT_TEST_* hook is set but {MODE_VAR} is not on; test hooks are ignored this run"
        );
    }
    None
}

/// `Get-IgnoredTestHooks`: the `CODEX_CONSULT_TEST_*` (other than the mode gate) and
/// `CODEX_CONSULT_NOW` variables that are SET in this process but IGNORED because the mode is off,
/// sorted ordinally. Empty when the mode is on (nothing is ignored). A run names them once in a
/// warning so a hook that quietly did nothing is visible.
pub fn ignored_hooks() -> Vec<String> {
    if mode_on() {
        return Vec::new();
    }
    let mut names: Vec<String> = Vec::new();
    for (k, v) in std::env::vars() {
        let u = k.to_ascii_uppercase();
        let is_hook =
            (u.starts_with("CODEX_CONSULT_TEST_") && u != MODE_VAR) || u == "CODEX_CONSULT_NOW";
        if is_hook && !v.trim().is_empty() && !names.contains(&k) {
            names.push(k);
        }
    }
    names.sort();
    names
}

/// The one-line warning naming the ignored hooks (`""` when none), as a run records it once:
/// `test hook[s] ignored - CODEX_CONSULT_TEST_MODE=1 is not set: <names>`.
pub fn ignored_hooks_warning() -> String {
    let names = ignored_hooks();
    if names.is_empty() {
        return String::new();
    }
    let plural = if names.len() != 1 { "s" } else { "" };
    format!(
        "test hook{plural} ignored - {MODE_VAR}=1 is not set: {}",
        names.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests mutate one process-wide gate; they take turns.
    static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn hook_honoured_only_with_mode() {
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CODEX_CONSULT_TEST_KICK_WAIT_MS", "42");
        // Without the mode: ignored.
        std::env::remove_var(MODE_VAR);
        assert_eq!(hook("CODEX_CONSULT_TEST_KICK_WAIT_MS"), None);
        // With the mode: honoured.
        std::env::set_var(MODE_VAR, "1");
        assert_eq!(
            hook("CODEX_CONSULT_TEST_KICK_WAIT_MS").as_deref(),
            Some("42")
        );
        // A falsey mode is off.
        std::env::set_var(MODE_VAR, "0");
        assert_eq!(hook("CODEX_CONSULT_TEST_KICK_WAIT_MS"), None);
        std::env::remove_var("CODEX_CONSULT_TEST_KICK_WAIT_MS");
        std::env::remove_var(MODE_VAR);
    }

    #[test]
    fn ignored_hooks_named_only_when_mode_off() {
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var(MODE_VAR);
        std::env::set_var("CODEX_CONSULT_TEST_PANEL_SEED", "seed-27c");
        // Mode off: the hook is named as ignored, with the singular wording.
        assert!(ignored_hooks().contains(&"CODEX_CONSULT_TEST_PANEL_SEED".to_string()));
        assert_eq!(
            ignored_hooks_warning(),
            "test hook ignored - CODEX_CONSULT_TEST_MODE=1 is not set: CODEX_CONSULT_TEST_PANEL_SEED"
        );
        // Mode on: nothing is ignored.
        std::env::set_var(MODE_VAR, "1");
        assert!(ignored_hooks().is_empty());
        assert_eq!(ignored_hooks_warning(), "");
        std::env::remove_var("CODEX_CONSULT_TEST_PANEL_SEED");
        std::env::remove_var(MODE_VAR);
    }

    #[test]
    fn test_vars_and_the_mode_line() {
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
        assert!(is_test_var("CODEX_CONSULT_TEST_MODE"));
        assert!(is_test_var("codex_consult_test_xyz"));
        assert!(!is_test_var("CODEX_CONSULT_NOW"));
        assert!(!is_test_var("CODEX_CONSULT_TELEMETRY"));
        std::env::set_var(MODE_VAR, "1");
        assert_eq!(test_mode_warning(), Some(TEST_MODE_WARNING));
        std::env::remove_var(MODE_VAR);
        assert_eq!(test_mode_warning(), None);
    }

    #[test]
    fn unset_hook_is_none_and_does_not_warn() {
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var(MODE_VAR);
        std::env::remove_var("CODEX_CONSULT_TEST_NOT_SET_ANYWHERE");
        assert_eq!(hook("CODEX_CONSULT_TEST_NOT_SET_ANYWHERE"), None);
    }
}
