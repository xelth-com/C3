//! Live integration coverage for the pending-recovery / liveness path (M2d-2, item 1): the
//! subprocess turn registers the running child through the `on_running` callback before it is
//! waited on, a timeout kills the tree, the `CODEX_CONSULT_TEST_SURVIVORS` hook adds survivors
//! to that kill, and a recovery record naming a live process is judged active (so the next run
//! is refused). These exercise the real spawn/kill path, not just the pure record logic.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use c3_core::engine::PromptDelivery;

use c3::engines::subprocess::{run_turn, SpawnRequest};
use c3::liveness::proc;

/// A `.cmd` that hangs for a while, so the turn is killed on its timeout.
fn hanging_launcher(dir: &std::path::Path) -> std::path::PathBuf {
    let p = dir.join("hang.cmd");
    // `ping -n 30` waits ~29 s; well past the 1 s test timeout.
    std::fs::write(&p, "@echo off\r\nping 127.0.0.1 -n 30 >nul\r\n").unwrap();
    p
}

fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("c3-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
#[cfg(windows)]
fn on_running_fires_with_child_pid_and_timeout_kills() {
    let dir = scratch_dir("subproc-run");
    let launcher = hanging_launcher(&dir);
    let events = dir.join("out.jsonl");
    let stderr = dir.join("err.txt");

    let seen_pid = Arc::new(AtomicU32::new(0));
    let cb_pid = Arc::clone(&seen_pid);
    let cb = move |pid: u32, _start: String| {
        cb_pid.store(pid, Ordering::SeqCst);
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
        tool_delta: None,
        on_running: Some(&cb),
    };
    let result = run_turn(&req);

    assert!(result.started, "the child started");
    assert!(
        matches!(result.stop, c3::engines::subprocess::TurnStop::Timeout),
        "the 1 s timeout killed the ~29 s hang"
    );
    let pid = seen_pid.load(Ordering::SeqCst);
    assert!(pid > 0, "on_running fired with a real child pid ({pid})");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
#[cfg(windows)]
fn survivor_hook_adds_a_live_pid_to_the_kill() {
    let dir = scratch_dir("subproc-surv");
    let launcher = hanging_launcher(&dir);
    let events = dir.join("out.jsonl");
    let stderr = dir.join("err.txt");

    // A long-lived helper process whose pid the survivor hook names: it must outlive the kill,
    // so the timeout outcome reports it as a survivor.
    let mut helper = std::process::Command::new("cmd")
        .args(["/c", "ping 127.0.0.1 -n 30 >nul"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let helper_pid = helper.id();
    // (wave 27c, D14) a test hook is honoured only with the mode set.
    std::env::set_var("CODEX_CONSULT_TEST_MODE", "1");
    std::env::set_var("CODEX_CONSULT_TEST_SURVIVORS", helper_pid.to_string());

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
        tool_delta: None,
        on_running: None,
    };
    let result = run_turn(&req);

    std::env::remove_var("CODEX_CONSULT_TEST_SURVIVORS");
    std::env::remove_var("CODEX_CONSULT_TEST_MODE");
    let _ = helper.kill();
    let _ = helper.wait();

    assert!(matches!(
        result.stop,
        c3::engines::subprocess::TurnStop::Timeout
    ));
    assert!(
        result.survivors.contains(&helper_pid),
        "the survivor hook added the live helper pid {helper_pid}; got {:?}",
        result.survivors
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn recorded_start_time_reads_back_for_this_process() {
    // The writer-pid / recorded-pid liveness rules compare a recorded start time to the live
    // one; this proves the start time this build records for a pid is readable and matches.
    let me = std::process::id();
    let start = proc::process_start_iso(me).unwrap_or_default();
    assert!(
        proc::pid_alive(me, &start),
        "this process is alive with its own start time"
    );
    assert!(
        !proc::pid_alive(me, "1999-01-01T00:00:00.0000000Z"),
        "a wrong start time is not alive"
    );
}
