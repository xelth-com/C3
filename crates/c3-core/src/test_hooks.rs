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
    fn unset_hook_is_none_and_does_not_warn() {
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var(MODE_VAR);
        std::env::remove_var("CODEX_CONSULT_TEST_NOT_SET_ANYWHERE");
        assert_eq!(hook("CODEX_CONSULT_TEST_NOT_SET_ANYWHERE"), None);
    }
}
