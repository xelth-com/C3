//! (wave 3a of the 0.6.1 parity: the plugin's 0.6.0 wave 28e, E1 / E19 / E23) The recovery paths
//! that need a test hook or a real process tree: the start-time and command-line hooks of the
//! fail-closed re-check (`Test-UnverifiedProcess`), and the CONFIRMED tree kill
//! (`Stop-ProcessTreeChecked`) - a clean kill, a descendant whose start time cannot be read (left
//! alone, unverified) and a kill whose enumeration and taskkill are denied (the root stopped, its
//! child left as an orphan, the kill not confirmed and naming no pid).
//!
//! Every check that sets a `CODEX_CONSULT_TEST_*` variable runs inside ONE test function: the
//! environment is process-wide and the test harness runs tests on parallel threads.

use c3::liveness::{pending, proc};

const NOWHERE: &str = r"C:\nowhere\codex.cmd";

fn set(k: &str, v: &str) {
    std::env::set_var(k, v);
}

fn unset(k: &str) {
    std::env::remove_var(k);
}

#[test]
fn hooked_recovery_rules_and_the_confirmed_tree_kill() {
    let me = std::process::id();
    let past = chrono::DateTime::parse_from_rfc3339("2020-01-01T00:00:00+00:00")
        .ok()
        .map(|d| d.with_timezone(&chrono::Utc));
    let future = Some(chrono::Utc::now() + chrono::Duration::hours(1));
    let name = proc::process_info(me).expect("this process").name;

    // ---- CODEX_CONSULT_TEST_START_UNREADABLE (wave 28c D8): honoured in test mode only
    unset("CODEX_CONSULT_TEST_MODE");
    set("CODEX_CONSULT_TEST_START_UNREADABLE", &me.to_string());
    let real = proc::process_start_iso(me).expect("this process");
    set("CODEX_CONSULT_TEST_MODE", "1");
    assert_eq!(proc::process_start_iso(me).as_deref(), Some(""));
    assert_eq!(proc::pid_identity(me, &real), proc::PidIdentity::Unknown);
    // (E1) its start time still unreadable: running, whatever else (fail-closed) - even with a
    // record that started after it
    assert_eq!(
        pending::test_unverified_process(me, future, "", &[]),
        pending::Verdict {
            alive: true,
            how: "its start time still cannot be read - counted as running (fail-closed)".into()
        }
    );
    unset("CODEX_CONSULT_TEST_START_UNREADABLE");
    if !real.is_empty() {
        assert_eq!(proc::pid_identity(me, &real), proc::PidIdentity::Alive);
    }

    // ---- CODEX_CONSULT_TEST_CMDLINE_UNREADABLE (E19): an unreadable command line counts as running
    set("CODEX_CONSULT_TEST_CMDLINE_UNREADABLE", &me.to_string());
    assert_eq!(
        pending::test_unverified_process(me, past, NOWHERE, &[]),
        pending::Verdict {
            alive: true,
            how: format!(
                "start time readable now; pid {me} runs {name}; command line not readable - counted as running (fail-closed)"
            )
        }
    );
    // ... unless it started before that run (dropped)
    let v = pending::test_unverified_process(me, future, NOWHERE, &[]);
    assert!(
        !v.alive && v.how.ends_with(", before that run: not its process"),
        "{v:?}"
    );
    // ... and the hook is ignored without test mode (the command line read: not codex)
    unset("CODEX_CONSULT_TEST_MODE");
    let v = pending::test_unverified_process(me, past, NOWHERE, &[]);
    assert!(!v.alive && v.how.ends_with(", not codex"), "{v:?}");
    unset("CODEX_CONSULT_TEST_CMDLINE_UNREADABLE");

    #[cfg(windows)]
    tree_kills::run();

    unset("CODEX_CONSULT_TEST_MODE");
}

#[cfg(windows)]
mod tree_kills {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{Duration, Instant};

    use c3::engines::subprocess::{run_turn, SpawnRequest, TurnResult, TurnStop};
    use c3::liveness::proc;
    use c3_core::engine::PromptDelivery;

    use super::{set, unset};

    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("c3-w3a-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The first descendant of `root` (the batch file's `ping`), waited for up to 5 s.
    fn first_descendant(root: u32) -> u32 {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(d) = proc::descendants_of(root).first() {
                return *d;
            }
            if Instant::now() >= until {
                return 0;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// One turn of a `.cmd` that hangs (`cmd.exe` -> `PING.EXE`), killed on a 1 s timeout;
    /// `on_child(root, child)` runs once the child exists, before the wait begins.
    fn hanging_turn(tag: &str, on_child: &dyn Fn(u32, u32)) -> (TurnResult, u32, u32) {
        let dir = scratch_dir(tag);
        let launcher = dir.join("hang.cmd");
        std::fs::write(&launcher, "@echo off\r\nping 127.0.0.1 -n 30 >nul\r\n").unwrap();
        let events = dir.join("out.jsonl");
        let stderr = dir.join("err.txt");
        let root = AtomicU32::new(0);
        let child = AtomicU32::new(0);
        let cb = |pid: u32, _start: String| {
            root.store(pid, Ordering::SeqCst);
            let c = first_descendant(pid);
            child.store(c, Ordering::SeqCst);
            on_child(pid, c);
        };
        let req = SpawnRequest {
            launcher: launcher.to_str().unwrap(),
            argv: &[],
            cwd: &dir,
            prompt_delivery: PromptDelivery::Stdin,
            stdin_text: "",
            events_path: &events,
            stderr_path: &stderr,
            timeout: Duration::from_secs(1),
            stall_sec: 0,
            kick_path: None,
            tool_flight: None,
            on_running: Some(&cb),
        };
        let result = run_turn(&req);
        assert!(matches!(result.stop, TurnStop::Timeout), "{tag}: timed out");
        let _ = std::fs::remove_dir_all(dir);
        (
            result,
            root.load(Ordering::SeqCst),
            child.load(Ordering::SeqCst),
        )
    }

    fn stop(pid: u32) {
        if pid > 0 && proc::process_start_iso(pid).is_some() {
            proc::terminate_pid(pid);
        }
    }

    pub fn run() {
        set("CODEX_CONSULT_TEST_MODE", "1");

        // (a) a clean tree kill: the root and its child gone - confirmed, nothing named
        let (r, root, child) = hanging_turn("clean", &|_, _| {});
        let k = r.kill.clone().expect("a kill check");
        assert!(child > 0, "the batch file started its child");
        assert!(
            k.confirmed && k.why.is_empty() && k.unverified.is_empty() && r.survivors.is_empty(),
            "{k:?} survivors {:?}",
            r.survivors
        );
        assert_eq!(k.root_pid, root);
        assert!(
            proc::process_start_iso(child).is_none(),
            "the child was killed"
        );

        // (b) (wave 28c D8, 28e E1) a descendant whose start time cannot be read: never killed by
        // its pid, never counted as gone - unverified, the kill not confirmed. taskkill is denied
        // too (`=taskkill`), so nothing else stops it.
        set("CODEX_CONSULT_TEST_KILL_DENIED", "taskkill");
        let (r, _root, child) = hanging_turn("unverified", &|_, c| {
            set("CODEX_CONSULT_TEST_START_UNREADABLE", &c.to_string());
        });
        unset("CODEX_CONSULT_TEST_START_UNREADABLE");
        unset("CODEX_CONSULT_TEST_KILL_DENIED");
        let k = r.kill.clone().expect("a kill check");
        let alive = proc::process_start_iso(child).is_some();
        stop(child);
        assert!(
            !k.confirmed
                && k.unverified == vec![child]
                && k.why == format!("start time of pid {child} unreadable")
                && r.survivors.is_empty(),
            "{k:?} survivors {:?}",
            r.survivors
        );
        assert!(alive, "the unverified descendant was left alone");

        // (c) (wave 27c D16, 28e E23) the enumeration and taskkill denied: the root is stopped by
        // its handle, its child left running as an orphan; the kill is not confirmed and names no
        // pid (no survivor, no unverified descendant)
        set("CODEX_CONSULT_TEST_KILL_DENIED", "1");
        let (r, root, child) = hanging_turn("denied", &|_, _| {});
        unset("CODEX_CONSULT_TEST_KILL_DENIED");
        let k = r.kill.clone().expect("a kill check");
        let orphan_alive = proc::process_start_iso(child).is_some();
        let root_gone = proc::process_start_iso(root).is_none();
        stop(child);
        assert_eq!(
            k.why,
            "the children could not be enumerated (process inspection denied (test hook CODEX_CONSULT_TEST_KILL_DENIED)) and taskkill /T /F failed (exit 1) - the root exited, its children may not have"
        );
        assert!(
            !k.confirmed && k.unverified.is_empty() && r.survivors.is_empty(),
            "{k:?} survivors {:?}",
            r.survivors
        );
        assert!(root_gone, "the root was stopped");
        assert!(orphan_alive, "its child outlived the kill (an orphan)");
    }
}
