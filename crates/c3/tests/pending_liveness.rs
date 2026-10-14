//! Live integration coverage for the pending-recovery / liveness path (M2d-2, item 1): the
//! subprocess turn registers the running child through the `on_running` callback before it is
//! waited on, a timeout kills the tree, the `CODEX_CONSULT_TEST_SURVIVORS` hook adds survivors
//! to that kill, and a recovery record naming a live process is judged active (so the next run
//! is refused). These exercise the real spawn/kill path, not just the pure record logic.

// The spawn/kill tests below run a `.cmd` launcher through cmd.exe, so they and their helpers
// are Windows-only; the start-time test at the end runs everywhere.
#[cfg(windows)]
use std::sync::atomic::{AtomicU32, Ordering};
#[cfg(windows)]
use std::sync::Arc;
#[cfg(windows)]
use std::time::Duration;

#[cfg(windows)]
use c3_core::engine::PromptDelivery;

#[cfg(windows)]
use c3::engines::subprocess::{run_turn, SpawnRequest};
use c3::liveness::proc;

/// A `.cmd` that hangs for a while, so the turn is killed on its timeout.
#[cfg(windows)] // a cmd.exe batch file
fn hanging_launcher(dir: &std::path::Path) -> std::path::PathBuf {
    let p = dir.join("hang.cmd");
    // `ping -n 30` waits ~29 s; well past the 1 s test timeout.
    std::fs::write(&p, "@echo off\r\nping 127.0.0.1 -n 30 >nul\r\n").unwrap();
    p
}

#[cfg(windows)] // used by the Windows-only spawn tests
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
        tool_flight: None,
        on_running: Some(&cb),
        on_kill: None,
        env: None,
        crt_quote: false,
        budget: None,
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
        tool_flight: None,
        on_running: None,
        on_kill: None,
        env: None,
        crt_quote: false,
        budget: None,
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

/// (wave 8 = plugin wave 30, T15) The quota budget watch on the real spawn/kill path: the child
/// names its thread on the event stream, the thread's rollout under the codex home reports the
/// 5-hour window at 95 % (observed during the turn, its reset ahead) and the watch (polling every
/// 200 ms) stops the turn at a budget of 90 - `TurnStop::Budget` with the window, long before the
/// child's own 30 s and the 20 s timeout; a budget of 96 lets the same turn run to its timeout.
#[test]
#[cfg(windows)]
fn the_budget_watch_stops_the_turn_at_the_quota_budget() {
    use c3::engines::subprocess::{BudgetWatch, TurnStop};
    let dir = scratch_dir("subproc-budget");
    let home = dir.join("codexhome");
    let now = chrono::Utc::now();
    let ms = format!("{:012x}", now.timestamp_millis());
    let thread = format!("{}-{}-7abc-8def-0123456789ab", &ms[0..8], &ms[8..12]);
    let local = now.with_timezone(&chrono::Local);
    let day = home
        .join("sessions")
        .join(local.format("%Y").to_string())
        .join(local.format("%m").to_string())
        .join(local.format("%d").to_string());
    std::fs::create_dir_all(&day).unwrap();
    let at = (now + chrono::Duration::seconds(1)).format("%Y-%m-%dT%H:%M:%S%.3fZ");
    let reset = (now + chrono::Duration::minutes(90)).timestamp();
    let line = format!(
        "{{\"timestamp\":\"{at}\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"total_token_usage\":{{\"input_tokens\":10,\"cached_input_tokens\":0,\"output_tokens\":1}},\"last_token_usage\":{{\"input_tokens\":10}},\"model_context_window\":258400}},\"rate_limits\":{{\"limit_id\":\"codex\",\"primary\":{{\"used_percent\":95.0,\"window_minutes\":300,\"resets_at\":{reset}}},\"secondary\":null}}}}}}\n"
    );
    std::fs::write(
        day.join(format!(
            "rollout-{}-{thread}.jsonl",
            local.format("%Y-%m-%dT%H-%M-%S")
        )),
        line,
    )
    .unwrap();
    let launcher = dir.join("thread.cmd");
    std::fs::write(
        &launcher,
        format!("@echo off\r\necho {{\"type\":\"thread.started\",\"thread_id\":\"{thread}\"}}\r\nping 127.0.0.1 -n 30 >nul\r\n"),
    )
    .unwrap();
    let run = |budget: i64, timeout: u64| {
        let events = dir.join(format!("out-{budget}.jsonl"));
        let stderr = dir.join(format!("err-{budget}.txt"));
        let watch = BudgetWatch {
            percent: budget,
            codex_home: home.to_string_lossy().to_string(),
            poll_ms: 200,
        };
        let req = SpawnRequest {
            launcher: launcher.to_str().unwrap(),
            argv: &[],
            cwd: &dir,
            prompt_delivery: PromptDelivery::Stdin,
            stdin_text: "",
            events_path: &events,
            stderr_path: &stderr,
            timeout: Duration::from_secs(timeout),
            stall_sec: 0,
            kick_path: None,
            tool_flight: None,
            on_running: None,
            on_kill: None,
            env: None,
            crt_quote: false,
            budget: Some(&watch),
        };
        run_turn(&req)
    };
    let t0 = std::time::Instant::now();
    let r = run(90, 20);
    assert!(r.started);
    assert_eq!(r.stop, TurnStop::Budget, "{r:?}");
    assert!(t0.elapsed() < Duration::from_secs(15), "{:?}", t0.elapsed());
    let w = r.budget.expect("the window that stopped the turn");
    assert_eq!(w.used, Some(95.0));
    assert_eq!(w.name, "5-hour window");
    let r2 = run(96, 3);
    assert_eq!(r2.stop, TurnStop::Timeout, "{r2:?}");
    assert!(r2.budget.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
