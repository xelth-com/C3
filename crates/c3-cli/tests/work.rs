//! (`c3 work`, `docs/work-mode.md`) The worker run end to end through the real `c3` binary
//! against a FAKE claude launcher: the full run (the record directory's five files, the
//! preamble in `brief.md`, the report from the result event, `diff.txt` and `files_changed`
//! computed by c3 from git, `turns` and `usage` from the result, and the token value nowhere
//! in the record), the guards that refuse BEFORE any record directory exists (an unreadable
//! brief, `--allow` naming git, a non-worker label, the entry's `env_key` variable unset),
//! `--status` exit codes 0/1/2 with `--id` narrowing over synthetic `run.json` files, and
//! the one-live-run-per-task refusal.
//!
//! GUARD: every child gets a scratch USERPROFILE/HOME and a PATH without any directory that
//! holds a claude launcher, so the real CLI can never start; the fake launcher is a `cmd`
//! stub on Windows, a `sh` stub elsewhere. Nothing reaches a real provider or intake.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

/// The token value of the fake roster entry (`env_key` `WB_FAKE_ZAI_TOKEN`): distinctive, so
/// the "nowhere in the record" grep means something.
const TOKEN: &str = "wb-fake-token-1106-do-not-leak";
const ENV_KEY: &str = "WB_FAKE_ZAI_TOKEN";
/// The roster's one worker entry: engine claude, role worker, the endpoint route.
const ROSTER: &str = r#"{"roster_version":1,"reviewers":[{"provider":"ZAI-claude","engine":"claude","model":"glm-5.3","auth":"endpoint","endpoint":{"base_url":"https://api.z.ai/api/anthropic","env_key":"WB_FAKE_ZAI_TOKEN"},"plan":"zai","roles":["worker"]},{"provider":"anthropic","engine":"claude","model":"claude-opus-5-5[1m]","auth":"subscription"},{"provider":"openai","engine":"codex","model":"gpt-6.1-sol","roles":["worker"]},{"provider":"meta","engine":"muse","model":"muse-spark-1.3-contributor","roles":["worker"]}]}"#;
/// The preamble every work run's prompt starts with (a contract with the worker).
const PREAMBLE: &str = "You are a coding worker inside this repository. Do only what the brief asks, touch only the files it names or clearly implies, never run git, never leave the repository, and never commit. Finish with a final report: what you changed (file list), the exact test commands you ran and their result counts, any deviation from the brief and why, and what you could not do.";
const BRIEF_TEXT: &str = "Create the file worker-touched.txt with the text touched. Run nothing.";
const REPORT_TEXT: &str = "Changed one file; cargo test: 12 passed.";

/// The fake worker launcher: ignores stdin, creates `worker-touched.txt` in the current
/// directory, echoes three stream-json lines (init, an assistant text, a result with
/// `num_turns` and a `usage` object) and exits 0. A `cmd` stub on Windows, a `sh` stub
/// elsewhere (the existing `compat_wave2b` shape).
#[cfg(windows)]
fn write_fake_launcher(work: &Path) -> PathBuf {
    let p = work.join("fake-work-claude.cmd");
    std::fs::write(
        &p,
        concat!(
            "@echo off\r\n",
            "echo touched>worker-touched.txt\r\n",
            "@echo {\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"11111111-1111-1111-1111-111111111111\",\"model\":\"glm-5.3\"}\r\n",
            "@echo {\"type\":\"assistant\",\"message\":{\"model\":\"glm-5.3\",\"content\":[{\"type\":\"text\",\"text\":\"I read the brief and did the work.\"}]}}\r\n",
            "@echo {\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"num_turns\":3,\"result\":\"Changed one file; cargo test: 12 passed.\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5}}\r\n",
            "@exit /b 0\r\n",
        ),
    )
    .unwrap();
    p
}

/// [`write_fake_launcher`] elsewhere: the same three lines, the same file, `sh` instead.
#[cfg(unix)]
fn write_fake_launcher(work: &Path) -> PathBuf {
    let p = work.join("fake-work-claude.sh");
    std::fs::write(
        &p,
        concat!(
            "#!/bin/sh\n",
            "cat >/dev/null\n",
            "echo touched >worker-touched.txt\n",
            "echo '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"11111111-1111-1111-1111-111111111111\",\"model\":\"glm-5.3\"}'\n",
            "echo '{\"type\":\"assistant\",\"message\":{\"model\":\"glm-5.3\",\"content\":[{\"type\":\"text\",\"text\":\"I read the brief and did the work.\"}]}}'\n",
            "echo '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"num_turns\":3,\"result\":\"Changed one file; cargo test: 12 passed.\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5}}'\n",
            "exit 0\n",
        ),
    )
    .unwrap();
    set_executable(&p);
    p
}

/// A quota launcher: the first launch (no `--resume`) emits a rejecting rate_limit_event (with a
/// far-future `resetsAt`) and an is_error "usage limit reached" result, exit 1; a `--resume` launch
/// emits a normal successful result. Used for the foreground quota -> `--resume`/`--kick` -> done
/// flow.
#[cfg(windows)]
fn write_quota_launcher(work: &Path) -> PathBuf {
    let p = work.join("fake-quota-claude.cmd");
    std::fs::write(
        &p,
        concat!(
            "@echo off\r\n",
            "echo %*| findstr /C:\"--resume\" >nul && goto resume\r\n",
            "@echo {\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"22222222-2222-2222-2222-222222222222\",\"model\":\"glm-5.3\"}\r\n",
            "@echo {\"type\":\"rate_limit_event\",\"rate_limit_info\":{\"status\":\"rejected\",\"resetsAt\":4102444800,\"rateLimitType\":\"five_hour\"},\"session_id\":\"22222222-2222-2222-2222-222222222222\"}\r\n",
            "@echo {\"type\":\"result\",\"subtype\":\"error\",\"is_error\":true,\"num_turns\":1,\"result\":\"Claude AI usage limit reached\"}\r\n",
            "@exit /b 1\r\n",
            ":resume\r\n",
            "@echo {\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"22222222-2222-2222-2222-222222222222\",\"model\":\"glm-5.3\"}\r\n",
            "@echo {\"type\":\"assistant\",\"message\":{\"model\":\"glm-5.3\",\"content\":[{\"type\":\"text\",\"text\":\"Resumed.\"}]}}\r\n",
            "@echo {\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"num_turns\":2,\"result\":\"Resumed and finished; cargo test: 12 passed.\",\"usage\":{\"input_tokens\":8,\"output_tokens\":4}}\r\n",
            "@exit /b 0\r\n",
        ),
    )
    .unwrap();
    p
}

#[cfg(unix)]
fn write_quota_launcher(work: &Path) -> PathBuf {
    let p = work.join("fake-quota-claude.sh");
    std::fs::write(
        &p,
        concat!(
            "#!/bin/sh\n",
            "cat >/dev/null\n",
            "if echo \"$@\" | grep -q -- --resume; then\n",
            "  echo '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"22222222-2222-2222-2222-222222222222\",\"model\":\"glm-5.3\"}'\n",
            "  echo '{\"type\":\"assistant\",\"message\":{\"model\":\"glm-5.3\",\"content\":[{\"type\":\"text\",\"text\":\"Resumed.\"}]}}'\n",
            "  echo '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"num_turns\":2,\"result\":\"Resumed and finished; cargo test: 12 passed.\",\"usage\":{\"input_tokens\":8,\"output_tokens\":4}}'\n",
            "  exit 0\n",
            "fi\n",
            "echo '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"22222222-2222-2222-2222-222222222222\",\"model\":\"glm-5.3\"}'\n",
            "echo '{\"type\":\"rate_limit_event\",\"rate_limit_info\":{\"status\":\"rejected\",\"resetsAt\":4102444800,\"rateLimitType\":\"five_hour\"},\"session_id\":\"22222222-2222-2222-2222-222222222222\"}'\n",
            "echo '{\"type\":\"result\",\"subtype\":\"error\",\"is_error\":true,\"num_turns\":1,\"result\":\"Claude AI usage limit reached\"}'\n",
            "exit 1\n",
        ),
    )
    .unwrap();
    set_executable(&p);
    p
}

/// An always-quota launcher: every launch (resume or not) hits the limit. Used for the resume cap.
#[cfg(windows)]
fn write_always_quota_launcher(work: &Path) -> PathBuf {
    let p = work.join("fake-always-quota-claude.cmd");
    std::fs::write(
        &p,
        concat!(
            "@echo off\r\n",
            "@echo {\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"33333333-3333-3333-3333-333333333333\",\"model\":\"glm-5.3\"}\r\n",
            "@echo {\"type\":\"rate_limit_event\",\"rate_limit_info\":{\"status\":\"rejected\",\"resetsAt\":4102444800,\"rateLimitType\":\"five_hour\"},\"session_id\":\"33333333-3333-3333-3333-333333333333\"}\r\n",
            "@echo {\"type\":\"result\",\"subtype\":\"error\",\"is_error\":true,\"num_turns\":1,\"result\":\"Claude AI usage limit reached\"}\r\n",
            "@exit /b 1\r\n",
        ),
    )
    .unwrap();
    p
}

#[cfg(unix)]
fn write_always_quota_launcher(work: &Path) -> PathBuf {
    let p = work.join("fake-always-quota-claude.sh");
    std::fs::write(
        &p,
        concat!(
            "#!/bin/sh\n",
            "cat >/dev/null\n",
            "echo '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"33333333-3333-3333-3333-333333333333\",\"model\":\"glm-5.3\"}'\n",
            "echo '{\"type\":\"rate_limit_event\",\"rate_limit_info\":{\"status\":\"rejected\",\"resetsAt\":4102444800,\"rateLimitType\":\"five_hour\"},\"session_id\":\"33333333-3333-3333-3333-333333333333\"}'\n",
            "echo '{\"type\":\"result\",\"subtype\":\"error\",\"is_error\":true,\"num_turns\":1,\"result\":\"Claude AI usage limit reached\"}'\n",
            "exit 1\n",
        ),
    )
    .unwrap();
    set_executable(&p);
    p
}

/// A fake codex worker launcher: writes `worker-codex.txt` in the cwd, echoes a codex JSON stream
/// (thread started, one command_execution tool call, an agent_message, turn completed) and exits 0.
#[cfg(windows)]
fn write_fake_codex(work: &Path) -> PathBuf {
    let p = work.join("fake-work-codex.cmd");
    std::fs::write(
        &p,
        concat!(
            "@echo off\r\n",
            "echo touched>worker-codex.txt\r\n",
            "@echo {\"type\":\"thread.started\",\"thread_id\":\"aaaaaaaa-1111-4111-8111-111111111111\"}\r\n",
            "@echo {\"type\":\"item.started\",\"item\":{\"id\":\"c1\",\"type\":\"command_execution\",\"command\":\"cargo test\"}}\r\n",
            "@echo {\"type\":\"item.completed\",\"item\":{\"id\":\"a1\",\"type\":\"agent_message\",\"text\":\"codex worker done; cargo test: 7 passed.\"}}\r\n",
            "@echo {\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":20,\"output_tokens\":6}}\r\n",
            "@exit /b 0\r\n",
        ),
    )
    .unwrap();
    p
}

#[cfg(unix)]
fn write_fake_codex(work: &Path) -> PathBuf {
    let p = work.join("fake-work-codex.sh");
    std::fs::write(
        &p,
        concat!(
            "#!/bin/sh\n",
            "cat >/dev/null\n",
            "echo touched >worker-codex.txt\n",
            "echo '{\"type\":\"thread.started\",\"thread_id\":\"aaaaaaaa-1111-4111-8111-111111111111\"}'\n",
            "echo '{\"type\":\"item.started\",\"item\":{\"id\":\"c1\",\"type\":\"command_execution\",\"command\":\"cargo test\"}}'\n",
            "echo '{\"type\":\"item.completed\",\"item\":{\"id\":\"a1\",\"type\":\"agent_message\",\"text\":\"codex worker done; cargo test: 7 passed.\"}}'\n",
            "echo '{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":20,\"output_tokens\":6}}'\n",
            "exit 0\n",
        ),
    )
    .unwrap();
    set_executable(&p);
    p
}

/// A fake muse worker launcher: writes `worker-muse.txt` in the cwd, echoes a provenance-valid MSP
/// stream (one session, one linked run, the model and the completed terminal bound to it, plus one
/// `tool.*` task) and exits 0.
#[cfg(windows)]
fn write_fake_muse(work: &Path) -> PathBuf {
    let p = work.join("fake-work-muse.cmd");
    std::fs::write(
        &p,
        concat!(
            "@echo off\r\n",
            "echo touched>worker-muse.txt\r\n",
            "@echo {\"schema_version\":1,\"stream\":{\"kind\":\"session\",\"id\":\"bbbbbbbb-2222-4222-8222-222222222222\"},\"payload_type\":\"session.run.linked\",\"payload\":{\"run_stream\":{\"kind\":\"run\",\"id\":\"cccccccc-3333-4333-8333-333333333333\"}}}\r\n",
            "@echo {\"schema_version\":1,\"stream\":{\"kind\":\"session\",\"id\":\"bbbbbbbb-2222-4222-8222-222222222222\"},\"payload_type\":\"run.model.configured\",\"payload\":{\"run_stream\":{\"kind\":\"run\",\"id\":\"cccccccc-3333-4333-8333-333333333333\"},\"model_id\":\"muse-spark-1.3-contributor\"}}\r\n",
            "@echo {\"schema_version\":1,\"stream\":{\"kind\":\"session\",\"id\":\"bbbbbbbb-2222-4222-8222-222222222222\"},\"payload_type\":\"task.lifecycle.proposed\",\"payload\":{\"event\":{\"task_kind\":\"tool.write_file\"}}}\r\n",
            "@echo {\"schema_version\":1,\"stream\":{\"kind\":\"session\",\"id\":\"bbbbbbbb-2222-4222-8222-222222222222\"},\"payload_type\":\"run.terminal.completed\",\"payload\":{\"run_stream\":{\"kind\":\"run\",\"id\":\"cccccccc-3333-4333-8333-333333333333\"},\"terminal\":\"completed\",\"text\":\"muse worker done; wrote worker-muse.txt\"}}\r\n",
            "@exit /b 0\r\n",
        ),
    )
    .unwrap();
    p
}

#[cfg(unix)]
fn write_fake_muse(work: &Path) -> PathBuf {
    let p = work.join("fake-work-muse.sh");
    std::fs::write(
        &p,
        concat!(
            "#!/bin/sh\n",
            "cat >/dev/null\n",
            "echo touched >worker-muse.txt\n",
            "echo '{\"schema_version\":1,\"stream\":{\"kind\":\"session\",\"id\":\"bbbbbbbb-2222-4222-8222-222222222222\"},\"payload_type\":\"session.run.linked\",\"payload\":{\"run_stream\":{\"kind\":\"run\",\"id\":\"cccccccc-3333-4333-8333-333333333333\"}}}'\n",
            "echo '{\"schema_version\":1,\"stream\":{\"kind\":\"session\",\"id\":\"bbbbbbbb-2222-4222-8222-222222222222\"},\"payload_type\":\"run.model.configured\",\"payload\":{\"run_stream\":{\"kind\":\"run\",\"id\":\"cccccccc-3333-4333-8333-333333333333\"},\"model_id\":\"muse-spark-1.3-contributor\"}}'\n",
            "echo '{\"schema_version\":1,\"stream\":{\"kind\":\"session\",\"id\":\"bbbbbbbb-2222-4222-8222-222222222222\"},\"payload_type\":\"task.lifecycle.proposed\",\"payload\":{\"event\":{\"task_kind\":\"tool.write_file\"}}}'\n",
            "echo '{\"schema_version\":1,\"stream\":{\"kind\":\"session\",\"id\":\"bbbbbbbb-2222-4222-8222-222222222222\"},\"payload_type\":\"run.terminal.completed\",\"payload\":{\"run_stream\":{\"kind\":\"run\",\"id\":\"cccccccc-3333-4333-8333-333333333333\"},\"terminal\":\"completed\",\"text\":\"muse worker done; wrote worker-muse.txt\"}}'\n",
            "exit 0\n",
        ),
    )
    .unwrap();
    set_executable(&p);
    p
}

/// (0.2.4, F89-1/F89-2) A blocking launcher: it waits for `release.txt` to appear in the cwd
/// (the repo root), up to a bounded number of slices, then emits a successful result and exits 0.
/// While it blocks, the owning c3 process stays alive, so the record stays `running`.
#[cfg(windows)]
fn write_blocking_launcher(work: &Path) -> PathBuf {
    let p = work.join("fake-blocking-claude.cmd");
    std::fs::write(
        &p,
        concat!(
            "@echo off\r\n",
            "setlocal enabledelayedexpansion\r\n",
            "set /a n=0\r\n",
            ":wait\r\n",
            "if exist \"release.txt\" goto go\r\n",
            "if !n! geq 120 goto go\r\n",
            "ping -n 2 127.0.0.1 >nul\r\n",
            "set /a n+=1\r\n",
            "goto wait\r\n",
            ":go\r\n",
            "@echo {\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"44444444-4444-4444-4444-444444444444\",\"model\":\"glm-5.3\"}\r\n",
            "@echo {\"type\":\"assistant\",\"message\":{\"model\":\"glm-5.3\",\"content\":[{\"type\":\"text\",\"text\":\"blocked then done\"}]}}\r\n",
            "@echo {\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"num_turns\":1,\"result\":\"blocking worker done.\"}\r\n",
            "@exit /b 0\r\n",
        ),
    )
    .unwrap();
    p
}

#[cfg(unix)]
fn write_blocking_launcher(work: &Path) -> PathBuf {
    let p = work.join("fake-blocking-claude.sh");
    std::fs::write(
        &p,
        concat!(
            "#!/bin/sh\n",
            "cat >/dev/null\n",
            "n=0\n",
            "while [ ! -f release.txt ] && [ $n -lt 120 ]; do sleep 0.5; n=$((n+1)); done\n",
            "echo '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"44444444-4444-4444-4444-444444444444\",\"model\":\"glm-5.3\"}'\n",
            "echo '{\"type\":\"assistant\",\"message\":{\"model\":\"glm-5.3\",\"content\":[{\"type\":\"text\",\"text\":\"blocked then done\"}]}}'\n",
            "echo '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"num_turns\":1,\"result\":\"blocking worker done.\"}'\n",
            "exit 0\n",
        ),
    )
    .unwrap();
    set_executable(&p);
    p
}

/// (0.2.4, F89-5) A launcher whose result carries a sentinel in an EXTRA usage field; the
/// allow-list rebuild must keep the numeric counters and drop the sentinel from run.json.
#[cfg(windows)]
fn write_sentinel_usage_launcher(work: &Path) -> PathBuf {
    let p = work.join("fake-sentinel-claude.cmd");
    std::fs::write(
        &p,
        concat!(
            "@echo off\r\n",
            "echo touched>worker-touched.txt\r\n",
            "@echo {\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"55555555-5555-5555-5555-555555555555\",\"model\":\"glm-5.3\"}\r\n",
            "@echo {\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"num_turns\":2,\"result\":\"done.\",\"total_cost_usd\":0.03,\"usage\":{\"input_tokens\":11,\"output_tokens\":7,\"secret\":\"sentinel-usage-do-not-leak\"}}\r\n",
            "@exit /b 0\r\n",
        ),
    )
    .unwrap();
    p
}

#[cfg(unix)]
fn write_sentinel_usage_launcher(work: &Path) -> PathBuf {
    let p = work.join("fake-sentinel-claude.sh");
    std::fs::write(
        &p,
        concat!(
            "#!/bin/sh\n",
            "cat >/dev/null\n",
            "echo touched >worker-touched.txt\n",
            "echo '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"55555555-5555-5555-5555-555555555555\",\"model\":\"glm-5.3\"}'\n",
            "echo '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"num_turns\":2,\"result\":\"done.\",\"total_cost_usd\":0.03,\"usage\":{\"input_tokens\":11,\"output_tokens\":7,\"secret\":\"sentinel-usage-do-not-leak\"}}'\n",
            "exit 0\n",
        ),
    )
    .unwrap();
    set_executable(&p);
    p
}

/// (0.2.4, F89-8) A launcher that emits an is_error result and exits 0: exit 0 alone must NOT be
/// treated as success.
#[cfg(windows)]
fn write_error_result_launcher(work: &Path) -> PathBuf {
    let p = work.join("fake-errres-claude.cmd");
    std::fs::write(
        &p,
        concat!(
            "@echo off\r\n",
            "@echo {\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"66666666-6666-6666-6666-666666666666\",\"model\":\"glm-5.3\"}\r\n",
            "@echo {\"type\":\"result\",\"subtype\":\"error\",\"is_error\":true,\"num_turns\":1,\"result\":\"the model refused the task\"}\r\n",
            "@exit /b 0\r\n",
        ),
    )
    .unwrap();
    p
}

#[cfg(unix)]
fn write_error_result_launcher(work: &Path) -> PathBuf {
    let p = work.join("fake-errres-claude.sh");
    std::fs::write(
        &p,
        concat!(
            "#!/bin/sh\n",
            "cat >/dev/null\n",
            "echo '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"66666666-6666-6666-6666-666666666666\",\"model\":\"glm-5.3\"}'\n",
            "echo '{\"type\":\"result\",\"subtype\":\"error\",\"is_error\":true,\"num_turns\":1,\"result\":\"the model refused the task\"}'\n",
            "exit 0\n",
        ),
    )
    .unwrap();
    set_executable(&p);
    p
}

/// (0.2.4, F89-8) A launcher that emits only an init line (no terminal result) and exits 0:
/// an empty stream with exit 0 must fail with reason `no terminal event`.
#[cfg(windows)]
fn write_empty_stream_launcher(work: &Path) -> PathBuf {
    let p = work.join("fake-empty-claude.cmd");
    std::fs::write(
        &p,
        concat!(
            "@echo off\r\n",
            "@echo {\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"77777777-7777-7777-7777-777777777777\",\"model\":\"glm-5.3\"}\r\n",
            "@exit /b 0\r\n",
        ),
    )
    .unwrap();
    p
}

#[cfg(unix)]
fn write_empty_stream_launcher(work: &Path) -> PathBuf {
    let p = work.join("fake-empty-claude.sh");
    std::fs::write(
        &p,
        concat!(
            "#!/bin/sh\n",
            "cat >/dev/null\n",
            "echo '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"77777777-7777-7777-7777-777777777777\",\"model\":\"glm-5.3\"}'\n",
            "exit 0\n",
        ),
    )
    .unwrap();
    set_executable(&p);
    p
}

/// (0.2.4, F89-7) A launcher that further edits the already-tracked `app.txt` (which the test
/// dirties before the run), to prove a pre-dirty file the worker edits is listed in files_changed.
#[cfg(windows)]
fn write_edit_launcher(work: &Path) -> PathBuf {
    let p = work.join("fake-edit-claude.cmd");
    std::fs::write(
        &p,
        concat!(
            "@echo off\r\n",
            "echo worker-edit>app.txt\r\n",
            "@echo {\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"88888888-8888-8888-8888-888888888888\",\"model\":\"glm-5.3\"}\r\n",
            "@echo {\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"num_turns\":1,\"result\":\"edited app.txt\"}\r\n",
            "@exit /b 0\r\n",
        ),
    )
    .unwrap();
    p
}

#[cfg(unix)]
fn write_edit_launcher(work: &Path) -> PathBuf {
    let p = work.join("fake-edit-claude.sh");
    std::fs::write(
        &p,
        concat!(
            "#!/bin/sh\n",
            "cat >/dev/null\n",
            "echo worker-edit >app.txt\n",
            "echo '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"88888888-8888-8888-8888-888888888888\",\"model\":\"glm-5.3\"}'\n",
            "echo '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"num_turns\":1,\"result\":\"edited app.txt\"}'\n",
            "exit 0\n",
        ),
    )
    .unwrap();
    set_executable(&p);
    p
}

#[cfg(unix)]
fn set_executable(p: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(p).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(p, perm).unwrap();
}

fn c3_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_c3"))
}

fn git(repo: &Path, args: &[&str]) {
    let st = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("git on PATH");
    assert!(st.status.success(), "git {args:?}");
}

/// A PATH without any directory that holds a claude launcher (GUARD).
fn safe_path() -> String {
    let sep = if cfg!(windows) { ";" } else { ":" };
    let path = std::env::var("PATH").unwrap_or_default();
    path.split(sep)
        .filter(|d| {
            !d.is_empty()
                && !["claude", "claude.exe", "claude.cmd", "claude.ps1"]
                    .iter()
                    .any(|n| Path::new(d).join(n).exists())
        })
        .collect::<Vec<_>>()
        .join(sep)
}

struct Env {
    work: PathBuf,
    repo: PathBuf,
    brief: PathBuf,
    fake: PathBuf,
}

fn setup(tag: &str) -> Env {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let work = std::env::temp_dir().join(format!("c3-wb-{tag}-{}-{nanos}", std::process::id()));
    let repo = work.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.email", "t@e.com"]);
    git(&repo, &["config", "user.name", "T"]);
    std::fs::write(repo.join("app.txt"), "one\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    let home = work.join("codexhome");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("config.toml"), "model = \"gpt-5.1\"\n").unwrap();
    std::fs::create_dir_all(work.join("userhome").join("AppData").join("Local")).unwrap();
    std::fs::create_dir_all(work.join("claude-config")).unwrap();
    let brief = work.join("brief.md");
    std::fs::write(&brief, BRIEF_TEXT).unwrap();
    let roster = work.join("roster.json");
    std::fs::write(&roster, ROSTER).unwrap();
    let fake = write_fake_launcher(&work);
    Env {
        work,
        repo,
        brief,
        fake,
    }
}

impl Env {
    /// The task's work-run root `.collab/t/work` (which no refusal may have created).
    fn work_root(&self) -> PathBuf {
        self.repo.join(".collab").join("t").join("work")
    }

    fn cmd(&self, args: &[&str], env: &[(&str, &str)]) -> Command {
        let mut cmd = Command::new(c3_bin());
        for (k, _) in std::env::vars() {
            if k.starts_with("CODEX_CONSULT_")
                || k.starts_with("FAKE_")
                || k.starts_with("C3_")
                || k.starts_with("ANTHROPIC_")
                || k.starts_with("CLAUDE")
            {
                cmd.env_remove(&k);
            }
        }
        let uh = self.work.join("userhome");
        cmd.current_dir(&self.repo)
            .env("PATH", safe_path())
            .env("CODEX_HOME", self.work.join("codexhome"))
            .env("HOME", &uh)
            .env("USERPROFILE", &uh)
            .env("LOCALAPPDATA", uh.join("AppData").join("Local"))
            .env("CLAUDE_CONFIG_DIR", self.work.join("claude-config"))
            .env("CODEX_CONSULT_ROSTER", self.work.join("roster.json"))
            .env("CODEX_CONSULT_HEALTH", "none")
            .env("CODEX_CONSULT_TELEMETRY", "off")
            .env("C3_TELEMETRY_HUB", "http://127.0.0.1:9/T")
            .env("C3_PRIORS", "off")
            .env("CODEX_CONSULT_CLAUDE_EXE", &self.fake)
            .env(ENV_KEY, TOKEN);
        for (k, v) in env {
            if v.is_empty() {
                cmd.env_remove(k);
            } else {
                cmd.env(k, v);
            }
        }
        cmd.args(args);
        cmd
    }

    fn c3(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        self.cmd(args, env).output().expect("run c3")
    }

    /// `c3 work --task t --brief <brief> --provider <label>` with the extras appended.
    fn work(&self, provider: &str, extra: &[&str], env: &[(&str, &str)]) -> Output {
        let mut v = vec![
            "work".to_string(),
            "--task".to_string(),
            "t".to_string(),
            "--brief".to_string(),
            self.brief.to_string_lossy().to_string(),
            "--provider".to_string(),
            provider.to_string(),
        ];
        v.extend(extra.iter().map(|s| s.to_string()));
        let args: Vec<&str> = v.iter().map(|s| s.as_str()).collect();
        self.c3(&args, env)
    }

    /// One synthetic `run.json` of `<work>/<id>/`: the status given, a live pid when
    /// `running` (this process's, so `is_live` holds).
    fn synthetic_run(&self, id: &str, status: &str) {
        let dir = self.work_root().join(id);
        std::fs::create_dir_all(&dir).unwrap();
        let seq: i64 = id.split('-').next().unwrap().parse().unwrap();
        let mut json = format!(
            concat!(
                "{{\"id\":\"{id}\",\"task\":\"t\",\"seq\":{seq},\"provider\":\"ZAI-claude\",",
                "\"model\":\"glm-5.3\",\"engine\":\"claude\",\"argv\":[\"-p\",\"--model\",\"glm-5.3\"],",
                "\"child_env_scrubbed\":[],\"started\":\"2026-10-21T10:00:00+00:00\",",
                "\"ended\":\"2026-10-21T10:01:00+00:00\",\"exit_code\":0,\"status\":\"{status}\",",
                "\"turns\":3,\"tool_calls\":{{}},\"files_changed\":[\"worker-touched.txt\"],",
                "\"timeout_sec\":3600"
            ),
            id = id,
            seq = seq,
            status = status,
        );
        if status == "running" {
            json.push_str(&format!(",\"pid\":{}", std::process::id()));
        }
        json.push('}');
        std::fs::write(dir.join("run.json"), json).unwrap();
    }

    /// One synthetic quota/waiting_quota `run.json` with a quota block (a live pid when
    /// `waiting_quota`, so its background reads as alive).
    fn synthetic_quota(&self, id: &str, status: &str, exhausted: bool) {
        let dir = self.work_root().join(id);
        std::fs::create_dir_all(&dir).unwrap();
        let seq: i64 = id.split('-').next().unwrap().parse().unwrap();
        let mut json = format!(
            concat!(
                "{{\"id\":\"{id}\",\"task\":\"t\",\"seq\":{seq},\"provider\":\"ZAI-claude\",",
                "\"model\":\"glm-5.3\",\"engine\":\"claude\",\"argv\":[\"-p\",\"--session-id\",\"s\"],",
                "\"child_env_scrubbed\":[],\"started\":\"2026-10-21T10:00:00+00:00\",",
                "\"status\":\"{status}\",\"session_id\":\"s\",",
                "\"quota\":{{\"text\":\"usage limit reached\",\"reset_at\":null,",
                "\"resume_at\":\"2100-01-01T00:01:00+00:00\",\"resumes\":1,\"exhausted\":{exhausted}}},",
                "\"timeout_sec\":3600"
            ),
            id = id,
            seq = seq,
            status = status,
            exhausted = exhausted,
        );
        if status == "waiting_quota" {
            json.push_str(&format!(",\"pid\":{}", std::process::id()));
        }
        json.push('}');
        std::fs::write(dir.join("run.json"), json).unwrap();
    }
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
    .replace("\r\n", "\n")
}

/// Every file under `dir` (any depth).
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(files_under(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

fn read(p: &Path) -> String {
    String::from_utf8_lossy(&std::fs::read(p).unwrap())
        .to_string()
        .replace("\r\n", "\n")
}

// ------------------------------------------------------------------ THE RUN

/// One work run through the fake launcher: exit 0; the record directory holds all five
/// files; `brief.md` is the prompt as sent (the preamble, then the brief); `report.md` is
/// the result event's text; `diff.txt` and `files_changed` come from git; `turns` and
/// `usage` from the result; and the token value is nowhere in the record.
#[test]
fn a_work_run_against_the_fake_launcher_lands_its_whole_record() {
    let e = setup("run");
    let out = e.work("ZAI-claude", &[], &[]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(t.contains("work run done (ZAI-claude :: glm-5.3)"), "{t}");
    assert!(t.contains("files changed: worker-touched.txt"), "{t}");

    // the fake ran in the repo root and touched the tree
    assert!(e.repo.join("worker-touched.txt").is_file());

    // the record directory: <collab>/<task>/work/NN-<provider>-<model>
    let rec = e.work_root().join("01-ZAI-claude-glm-5.3");
    assert!(rec.is_dir(), "{}", rec.display());
    for f in [
        "brief.md",
        "events.jsonl",
        "report.md",
        "diff.txt",
        "run.json",
    ] {
        assert!(rec.join(f).is_file(), "{} missing", rec.join(f).display());
    }

    // brief.md: the preamble first, the brief text after it
    let brief = read(&rec.join("brief.md"));
    assert!(brief.starts_with(PREAMBLE), "{brief}");
    assert!(brief.contains(BRIEF_TEXT), "{brief}");

    // events.jsonl: the fake's three lines, verbatim
    let events = read(&rec.join("events.jsonl"));
    let lines: Vec<&str> = events.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 3, "{events}");
    assert!(lines[0].contains("\"subtype\":\"init\""), "{events}");
    assert!(lines[1].contains("\"type\":\"assistant\""), "{events}");
    assert!(lines[2].contains("\"type\":\"result\""), "{events}");

    // report.md: the result event's text
    assert_eq!(read(&rec.join("report.md")), REPORT_TEXT);

    // diff.txt: the baseline and the after state, both computed by c3 — the touched
    // file is new in the AFTER block only
    let diff = read(&rec.join("diff.txt"));
    let before_at = diff.find("git status --short (before the run):").unwrap();
    let after_at = diff.find("git status --short (after the run):").unwrap();
    assert!(before_at < after_at, "{diff}");
    assert!(
        !diff[before_at..after_at].contains("worker-touched.txt"),
        "{diff}"
    );
    assert!(diff[after_at..].contains("?? worker-touched.txt"), "{diff}");

    // run.json: the summary of the stream and the git state
    let run: Value = serde_json::from_str(&read(&rec.join("run.json"))).unwrap();
    assert_eq!(run["status"], "done");
    assert_eq!(run["exit_code"], 0);
    assert_eq!(run["provider"], "ZAI-claude");
    assert_eq!(run["model"], "glm-5.3");
    assert_eq!(run["engine"], "claude");
    assert_eq!(run["turns"], 3);
    assert_eq!(run["usage"]["input_tokens"], 10);
    // (0.2.4, F89-1) the owning pid + start time were written (the liveness owner)
    assert!(run["pid"].as_i64().unwrap_or(0) > 0, "{run}");
    assert!(run["pid_start"].is_string(), "{run}");
    // (0.2.4, F89-4) the permission boundary is recorded; this run was restricted
    assert_eq!(run["unrestricted"], false, "{run}");
    assert!(
        run["permission"]
            .as_str()
            .unwrap_or("")
            .contains("allow list"),
        "{run}"
    );
    assert_eq!(
        run["files_changed"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>(),
        vec!["worker-touched.txt"]
    );
    // the worker argv, no secrets in it
    let argv = run["argv"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(argv.contains("--permission-mode acceptEdits"), "{argv}");
    assert!(
        argv.contains("--tools Read,Edit,Write,Grep,Glob,Bash"),
        "{argv}"
    );
    assert!(argv.contains("--allowedTools Bash(cargo test:*),Bash(cargo check:*),Bash(cargo clippy:*),Bash(cargo fmt:*)"), "{argv}");

    // the token value is nowhere in the record (its variable name is, its value never)
    for p in files_under(&rec) {
        assert!(!read(&p).contains(TOKEN), "{} leaks the token", p.display());
    }
}

// ------------------------------------------------------------------ THE REFUSALS

/// The design's guards refuse BEFORE any record directory exists: an unreadable brief,
/// `--allow` naming git, a non-worker label (the message lists the workers), and the
/// entry's `env_key` variable unset. Each exits 1 and leaves no `.collab/t/work`.
#[test]
fn the_guards_refuse_before_any_record_exists() {
    let e = setup("refuse");

    // a --brief that does not exist
    let out = e.c3(
        &[
            "work",
            "--task",
            "t",
            "--brief",
            "no-such-brief.md",
            "--provider",
            "ZAI-claude",
        ],
        &[],
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("could not be read"), "{t}");
    assert!(!e.work_root().exists(), "no record directory may exist");

    // --allow naming git: a worker never commits
    let out = e.work("ZAI-claude", &["--allow", "Bash(git:*)"], &[]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("--allow pattern #1 names git"), "{t}");
    assert!(!e.work_root().exists(), "no record directory may exist");

    // a label that is not a worker: the message lists the roster's worker entries
    let out = e.work("anthropic", &[], &[]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains("the roster entry 'anthropic' is not a worker"),
        "{t}"
    );
    assert!(
        t.contains("the roster's worker entries: ZAI-claude :: glm-5.3"),
        "{t}"
    );
    assert!(!e.work_root().exists(), "no record directory may exist");

    // the entry's env_key variable unset: refused before any record exists
    let out = e.work("ZAI-claude", &[], &[(ENV_KEY, "")]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains(&format!(
            "the entry's endpoint is not usable: env {ENV_KEY} not set"
        )),
        "{t}"
    );
    assert!(!e.work_root().exists(), "no record directory may exist");
}

// ------------------------------------------------------------------ --STATUS

/// `--status` over synthetic `run.json` files: 0 all done, 1 one died, 2 one running, and
/// `--id` narrows to the runs whose id starts with it.
#[test]
fn status_reads_synthetic_records_and_narrows_by_id() {
    let e = setup("status");
    // an empty task is 0 too; --status needs no roster or launcher, only the records
    let out = e.c3(&["work", "--task", "t", "--status"], &[]);
    let t = text(&out);
    assert!(t.contains("no work run in task 't'"), "{t}");
    assert_eq!(out.status.code(), Some(0), "{t}");

    // one done run: 0
    e.synthetic_run("01-a-m", "done");
    let out = e.c3(&["work", "--task", "t", "--status"], &[]);
    let t = text(&out);
    assert!(
        t.contains("work 01-a-m (ZAI-claude/glm-5.3 [claude]): done"),
        "{t}"
    );
    assert_eq!(out.status.code(), Some(0), "{t}");

    // two done runs: still 0
    e.synthetic_run("02-b-m", "done");
    let out = e.c3(&["work", "--task", "t", "--status"], &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));

    // one died: 1
    e.synthetic_run("03-c-m", "died");
    let out = e.c3(&["work", "--task", "t", "--status"], &[]);
    let t = text(&out);
    assert!(
        t.contains("work 03-c-m (ZAI-claude/glm-5.3 [claude]): died"),
        "{t}"
    );
    assert_eq!(out.status.code(), Some(1), "{t}");

    // one running (a live pid): 2, and --id narrows away from it
    e.synthetic_run("04-d-m", "running");
    let out = e.c3(&["work", "--task", "t", "--status"], &[]);
    let t = text(&out);
    assert!(
        t.contains("work 04-d-m (ZAI-claude/glm-5.3 [claude]): running"),
        "{t}"
    );
    assert_eq!(out.status.code(), Some(2), "{t}");
    let out = e.c3(&["work", "--task", "t", "--status", "--id", "01"], &[]);
    let t = text(&out);
    assert!(t.contains("work 01-a-m"), "{t}");
    assert!(!t.contains("work 04-d-m"), "{t}");
    assert_eq!(out.status.code(), Some(0), "{t}");
    let out = e.c3(&["work", "--task", "t", "--status", "--id", "04"], &[]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));

    // an --id no run starts with: refused with the known ids
    let out = e.c3(&["work", "--task", "t", "--status", "--id", "zz"], &[]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains("no work run of task 't' has an id starting with 'zz'"),
        "{t}"
    );
}

// ------------------------------------------------------------------ ONE LIVE RUN

/// A second `c3 work` on a task that has a `running` record is refused (exit 1), and no
/// second record directory appears.
#[test]
fn a_second_work_on_a_task_with_a_live_run_is_refused() {
    let e = setup("second");
    e.synthetic_run("01-ZAI-claude-glm-5.3", "running");
    let out = e.work("ZAI-claude", &[], &[]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains("task 't' already has a live work run (01-ZAI-claude-glm-5.3)"),
        "{t}"
    );
    assert!(!e.work_root().join("02-ZAI-claude-glm-5.3").exists());
}

// ------------------------------------------------------------------ QUOTA / RESUME / KICK

/// A foreground run that hits the limit parks on `quota` (exit 3) with `resume_at` = reset + 60 s;
/// `--resume` then continues the SAME session, the record goes `done`, events.jsonl gains the
/// resume separator, and `quota.resumes` is 1.
#[test]
fn a_quota_hit_parks_the_run_and_resume_finishes_it() {
    let e = setup("quota");
    let fake = write_quota_launcher(&e.work);
    let fakep = fake.to_string_lossy().to_string();
    let qenv: Vec<(&str, &str)> = vec![("CODEX_CONSULT_CLAUDE_EXE", fakep.as_str())];

    // the foreground run hits the limit: exit 3, status quota, resume_at = reset + 60 s
    let out = e.work("ZAI-claude", &[], &qenv);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(3), "{t}");
    assert!(t.contains("hit a usage limit"), "{t}");
    assert!(t.contains("--resume 01-ZAI-claude-glm-5.3"), "{t}");

    let rec = e.work_root().join("01-ZAI-claude-glm-5.3");
    let run: Value = serde_json::from_str(&read(&rec.join("run.json"))).unwrap();
    assert_eq!(run["status"], "quota", "{run}");
    let reset = run["quota"]["reset_at"].as_str().unwrap();
    let resume_at = run["quota"]["resume_at"].as_str().unwrap();
    let reset_t = chrono::DateTime::parse_from_rfc3339(reset).unwrap();
    let resume_t = chrono::DateTime::parse_from_rfc3339(resume_at).unwrap();
    assert_eq!(
        (resume_t - reset_t).num_seconds(),
        60,
        "resume_at = reset + 60 s"
    );
    assert_eq!(run["quota"]["resumes"], 0);

    // --resume continues the same session (argv carries --resume): status done, resumes 1
    let out = e.c3(&["work", "--task", "t", "--resume", "01"], &qenv);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    let run: Value = serde_json::from_str(&read(&rec.join("run.json"))).unwrap();
    assert_eq!(run["status"], "done", "{run}");
    assert_eq!(run["quota"]["resumes"], 1, "{run}");
    assert_eq!(
        read(&rec.join("report.md")),
        "Resumed and finished; cargo test: 12 passed."
    );
    let events = read(&rec.join("events.jsonl"));
    assert!(
        events.contains("\"type\":\"c3\",\"subtype\":\"resume\",\"n\":1"),
        "the resume separator is in events.jsonl: {events}"
    );
}

/// `--kick` on a foreground quota run resumes it directly (no sleeper); `--kick` on a done run
/// refuses with its state.
#[test]
fn kick_resumes_a_quota_run_and_refuses_a_done_run() {
    let e = setup("kick");
    let fake = write_quota_launcher(&e.work);
    let fakep = fake.to_string_lossy().to_string();
    let qenv: Vec<(&str, &str)> = vec![("CODEX_CONSULT_CLAUDE_EXE", fakep.as_str())];

    let out = e.work("ZAI-claude", &[], &qenv);
    assert_eq!(out.status.code(), Some(3), "{}", text(&out));

    // --kick resumes the parked quota run (there is no sleeper): status done
    let out = e.c3(&["work", "--task", "t", "--kick", "--id", "01"], &qenv);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    let rec = e.work_root().join("01-ZAI-claude-glm-5.3");
    let run: Value = serde_json::from_str(&read(&rec.join("run.json"))).unwrap();
    assert_eq!(run["status"], "done", "{run}");

    // --kick on the now-done run refuses with the state
    let out = e.c3(&["work", "--task", "t", "--kick", "--id", "01"], &qenv);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("is in state done"), "{t}");
}

/// A run that keeps hitting the limit exhausts its resume cap: after `--max-resumes` resumes the
/// record has `quota.exhausted = true` and the resume count equals the cap.
#[test]
fn the_resume_cap_exhausts() {
    let e = setup("cap");
    let fake = write_always_quota_launcher(&e.work);
    let fakep = fake.to_string_lossy().to_string();
    let qenv: Vec<(&str, &str)> = vec![("CODEX_CONSULT_CLAUDE_EXE", fakep.as_str())];

    // initial run -> quota (exit 3)
    let out = e.work("ZAI-claude", &["--max-resumes", "3"], &qenv);
    assert_eq!(out.status.code(), Some(3), "{}", text(&out));

    let rec = e.work_root().join("01-ZAI-claude-glm-5.3");
    // resume 1 and 2 stay quota (exit 3), resume 3 exhausts the cap (exit 1)
    for n in 1..=3 {
        let out = e.c3(
            &[
                "work",
                "--task",
                "t",
                "--resume",
                "01",
                "--max-resumes",
                "3",
            ],
            &qenv,
        );
        let code = out.status.code();
        let run: Value = serde_json::from_str(&read(&rec.join("run.json"))).unwrap();
        assert_eq!(run["quota"]["resumes"], n, "resume {n}: {run}");
        if n < 3 {
            assert_eq!(code, Some(3), "resume {n} stays quota: {}", text(&out));
            assert_eq!(run["quota"]["exhausted"], false);
        } else {
            assert_eq!(code, Some(1), "resume {n} exhausts: {}", text(&out));
            assert_eq!(run["quota"]["exhausted"], true, "{run}");
        }
    }

    // a further --resume is refused: the cap is spent
    let out = e.c3(
        &[
            "work",
            "--task",
            "t",
            "--resume",
            "01",
            "--max-resumes",
            "3",
        ],
        &qenv,
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("spent its resume cap"), "{t}");
}

/// `--status` exit codes over synthetic quota records: `waiting_quota` reads as running (2);
/// `quota` and quota-exhausted need a hand (1).
#[test]
fn status_exit_codes_for_quota_states() {
    let e = setup("quota-status");

    // waiting_quota (a live pid): the background is alive -> exit 2
    e.synthetic_quota("01-ZAI-claude-glm-5.3", "waiting_quota", false);
    let out = e.c3(&["work", "--task", "t", "--status", "--id", "01"], &[]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(2), "{t}");
    assert!(t.contains("waiting_quota until"), "{t}");

    // quota, not exhausted -> exit 1 (needs a --resume/--kick)
    e.synthetic_quota("02-ZAI-claude-glm-5.3", "quota", false);
    let out = e.c3(&["work", "--task", "t", "--status", "--id", "02"], &[]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));

    // quota exhausted -> exit 1
    e.synthetic_quota("03-ZAI-claude-glm-5.3", "quota", true);
    let out = e.c3(&["work", "--task", "t", "--status", "--id", "03"], &[]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("exhausted"), "{t}");
}

// ------------------------------------------------------------------ CODEX / MUSE ENGINES

/// A work run on the CODEX engine against a fake codex launcher: the record lands, status done,
/// the engine is codex, the codex thread id is recorded as the session, `report.md` is the
/// agent_message, `files_changed` and the tool call come from the codex stream.
#[test]
fn a_codex_work_run_lands_its_record() {
    let e = setup("codex");
    let fake = write_fake_codex(&e.work);
    let fakep = fake.to_string_lossy().to_string();
    let out = e.work("openai", &[], &[("CODEX_CONSULT_EXE", fakep.as_str())]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(e.repo.join("worker-codex.txt").is_file(), "{t}");

    let rec = e.work_root().join("01-openai-gpt-6.1-sol");
    for f in [
        "brief.md",
        "events.jsonl",
        "report.md",
        "diff.txt",
        "run.json",
    ] {
        assert!(rec.join(f).is_file(), "{} missing", rec.join(f).display());
    }
    let run: Value = serde_json::from_str(&read(&rec.join("run.json"))).unwrap();
    assert_eq!(run["status"], "done", "{run}");
    assert_eq!(run["engine"], "codex");
    assert_eq!(run["session_id"], "aaaaaaaa-1111-4111-8111-111111111111");
    assert_eq!(
        run["files_changed"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>(),
        vec!["worker-codex.txt"]
    );
    assert_eq!(run["tool_calls"]["command_execution"], 1);
    assert_eq!(
        read(&rec.join("report.md")),
        "codex worker done; cargo test: 7 passed."
    );
    // the codex worker argv: workspace-write sandbox, -C the repo root, the model, prompt on `-`
    let argv = run["argv"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(argv.contains("exec --sandbox workspace-write"), "{argv}");
    assert!(argv.contains("-m gpt-6.1-sol"), "{argv}");
    assert!(argv.ends_with(" -"), "{argv}");
}

/// A work run on the MUSE engine against a fake muse launcher (a planted oauth `auth.json` so the
/// per-token-billing guard passes): the record lands, status done, the engine is muse, the minted
/// session id is recorded, `report.md` is the terminal text, the tool task is counted.
#[test]
fn a_muse_work_run_lands_its_record() {
    let e = setup("muse");
    let fake = write_fake_muse(&e.work);
    let fakep = fake.to_string_lossy().to_string();
    // plant an oauth auth.json under the scratch home so get_muse_launch_block passes
    let auth = e
        .work
        .join("userhome")
        .join(".config")
        .join("muse")
        .join("auth.json");
    std::fs::create_dir_all(auth.parent().unwrap()).unwrap();
    std::fs::write(&auth, r#"{"providers":{"meta":{"mechanism":"oauth"}}}"#).unwrap();
    let env: Vec<(&str, &str)> = vec![
        ("CODEX_CONSULT_MUSE_EXE", fakep.as_str()),
        ("TBH_CREDENTIAL_BACKEND", "file"),
        ("META_API_KEY", ""),
        ("MODEL_API_KEY", ""),
    ];
    // (0.2.4, F89-4) the muse worker disables its shell sandbox: it needs the explicit opt-in.
    let out = e.work("meta", &["--unrestricted"], &env);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(e.repo.join("worker-muse.txt").is_file(), "{t}");

    let rec = e.work_root().join("01-meta-muse-spark-1.3-contributor");
    for f in [
        "brief.md",
        "events.jsonl",
        "report.md",
        "diff.txt",
        "run.json",
    ] {
        assert!(rec.join(f).is_file(), "{} missing", rec.join(f).display());
    }
    let run: Value = serde_json::from_str(&read(&rec.join("run.json"))).unwrap();
    assert_eq!(run["status"], "done", "{run}");
    assert_eq!(run["engine"], "muse");
    assert_eq!(run["unrestricted"], true, "{run}");
    // the minted session id is a uuid (muse mints it up front, so a resume continues it)
    let sid = run["session_id"].as_str().unwrap();
    assert_eq!(sid.len(), 36, "minted uuid: {sid}");
    assert_eq!(
        run["files_changed"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>(),
        vec!["worker-muse.txt"]
    );
    assert_eq!(run["tool_calls"]["write_file"], 1);
    assert_eq!(
        read(&rec.join("report.md")),
        "muse worker done; wrote worker-muse.txt"
    );
    // the muse worker argv: writes/shell ON, approvals withheld, sandbox off, the minted session
    let argv = run["argv"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(argv.contains("exec --json --prompt-file"), "{argv}");
    assert!(argv.contains("--approval-mode never"), "{argv}");
    assert!(argv.contains("--disable-sandbox"), "{argv}");
    assert!(argv.contains(&format!("--session-id {sid}")), "{argv}");
}

// ------------------------------------------------------------------ WAVE 10a (F89-1..F89-8)

/// (F89-4) The allow-list grammar refuses broad patterns (naming the position, not the pattern),
/// and the muse worker needs the explicit --unrestricted opt-in. Each refuses before any record.
#[test]
fn the_allow_grammar_and_muse_opt_in_refuse_before_any_record() {
    let e = setup("grammar");

    // Grammar misses: the refusal is generic and NEVER echoes the pattern (it could carry a
    // secret). A path separator (C:\x\node.exe) is a grammar miss too.
    for pat in [
        "Bash(*)",
        "*",
        "Bash(python -c:*)",
        "Bash(cargo test && git push:*)",
        "Bash(cargo  test:*)",
        "Bash(npm run \"lint\":*)",
        "Bash(C:\\x\\node.exe:*)",
    ] {
        let out = e.work("ZAI-claude", &["--allow", pat], &[]);
        let t = text(&out);
        assert_eq!(out.status.code(), Some(1), "{pat}: {t}");
        assert!(
            t.contains("not a valid restricted tool pattern") || t.contains("names git"),
            "{pat}: {t}"
        );
        // A distinctive (>2 char) pattern is never echoed wholesale (a bare `*` appears in the
        // help text itself, so it is exempt from this check).
        if pat.len() > 2 {
            assert!(
                !t.contains(pat),
                "{pat}: the refusal must never echo the pattern: {t}"
            );
        }
        assert!(!e.work_root().exists(), "{pat}: no record may exist");
    }

    // (F89-4 remainder) A bare shell/exec tool, and shells/interpreters/git by exe basename (even
    // with a .exe suffix), are refused — naming the safe keyword is allowed.
    for pat in [
        "Bash",
        "Bash(sh:*)",
        "Bash(python:*)",
        "Bash(powershell.exe:*)",
    ] {
        let out = e.work("ZAI-claude", &["--allow", pat], &[]);
        let t = text(&out);
        assert_eq!(out.status.code(), Some(1), "{pat}: {t}");
        assert!(
            t.contains("not allowed in restricted mode") || t.contains("shell, interpreter or git"),
            "{pat}: {t}"
        );
        assert!(!e.work_root().exists(), "{pat}: no record may exist");
    }

    // a valid specific pattern is accepted (the run lands); subcommand words are plain words
    let out = e.work("ZAI-claude", &["--allow", "Bash(cargo)"], &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let e1 = setup("grammar1");
    let out = e1.work("ZAI-claude", &["--allow", "Bash(npm run lint:*)"], &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));

    // --unrestricted lifts the grammar entirely (even a bare wildcard), recorded in run.json
    let e2 = setup("grammar2");
    let out = e2.work("ZAI-claude", &["--allow", "Bash(*)", "--unrestricted"], &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let rec = e2.work_root().join("01-ZAI-claude-glm-5.3");
    let run: Value = serde_json::from_str(&read(&rec.join("run.json"))).unwrap();
    assert_eq!(run["unrestricted"], true, "{run}");

    // the muse worker is refused without the opt-in (before any record exists)
    let e3 = setup("muse-refuse");
    let auth = e3
        .work
        .join("userhome")
        .join(".config")
        .join("muse")
        .join("auth.json");
    std::fs::create_dir_all(auth.parent().unwrap()).unwrap();
    std::fs::write(&auth, r#"{"providers":{"meta":{"mechanism":"oauth"}}}"#).unwrap();
    let out = e3.work(
        "meta",
        &[],
        &[
            ("TBH_CREDENTIAL_BACKEND", "file"),
            ("META_API_KEY", ""),
            ("MODEL_API_KEY", ""),
        ],
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("--disable-sandbox"), "{t}");
    assert!(!e3.work_root().exists(), "no record may exist");
}

/// (F89-5) usage in run.json is rebuilt from an allow list of numeric counters: a sentinel in an
/// extra usage field never reaches the record, anywhere.
#[test]
fn usage_in_run_json_is_an_allow_list() {
    let e = setup("usage");
    let fake = write_sentinel_usage_launcher(&e.work);
    let fakep = fake.to_string_lossy().to_string();
    let out = e.work(
        "ZAI-claude",
        &[],
        &[("CODEX_CONSULT_CLAUDE_EXE", fakep.as_str())],
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let rec = e.work_root().join("01-ZAI-claude-glm-5.3");
    let run: Value = serde_json::from_str(&read(&rec.join("run.json"))).unwrap();
    assert_eq!(run["usage"]["input_tokens"], 11, "{run}");
    assert_eq!(run["usage"]["output_tokens"], 7, "{run}");
    assert_eq!(run["usage"]["total_cost_usd"], 0.03, "{run}");
    assert!(run["usage"].get("secret").is_none(), "{run}");
    // run.json, report.md and diff.txt carry no secret VALUES. events.jsonl is exempt: it is the
    // child's raw stream retained verbatim (F89-5 record contract), like a consult's events file.
    for f in ["run.json", "report.md", "diff.txt"] {
        assert!(
            !read(&rec.join(f)).contains("sentinel-usage"),
            "{f} leaks the sentinel"
        );
    }
}

/// (F89-6) A persisted --max-resumes is reused by a plain --resume (no flag): a cap of 1 exhausts
/// after the first quota-hitting resume.
#[test]
fn a_persisted_max_resumes_is_reused_without_the_flag() {
    let e = setup("persist-cap");
    let fake = write_always_quota_launcher(&e.work);
    let fakep = fake.to_string_lossy().to_string();
    let qenv: Vec<(&str, &str)> = vec![("CODEX_CONSULT_CLAUDE_EXE", fakep.as_str())];

    let out = e.work("ZAI-claude", &["--max-resumes", "1"], &qenv);
    assert_eq!(out.status.code(), Some(3), "{}", text(&out));
    let rec = e.work_root().join("01-ZAI-claude-glm-5.3");
    let run: Value = serde_json::from_str(&read(&rec.join("run.json"))).unwrap();
    assert_eq!(run["max_resumes"], 1, "{run}");

    // a PLAIN --resume (no --max-resumes): the stored cap of 1 is honoured -> exhausted
    let out = e.c3(&["work", "--task", "t", "--resume", "01"], &qenv);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    let run: Value = serde_json::from_str(&read(&rec.join("run.json"))).unwrap();
    assert_eq!(run["quota"]["resumes"], 1, "{run}");
    assert_eq!(run["quota"]["exhausted"], true, "{run}");
}

/// (F89-7) files_changed compares content identity: a file already dirty before the run that the
/// worker edits further is listed.
#[test]
fn files_changed_lists_a_pre_dirty_file_the_worker_edited() {
    let e = setup("predirty");
    std::fs::write(e.repo.join("app.txt"), "pre-dirty\n").unwrap();
    let fake = write_edit_launcher(&e.work);
    let fakep = fake.to_string_lossy().to_string();
    let out = e.work(
        "ZAI-claude",
        &[],
        &[("CODEX_CONSULT_CLAUDE_EXE", fakep.as_str())],
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let rec = e.work_root().join("01-ZAI-claude-glm-5.3");
    let run: Value = serde_json::from_str(&read(&rec.join("run.json"))).unwrap();
    let changed: Vec<&str> = run["files_changed"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        changed.contains(&"app.txt"),
        "pre-dirty edited file listed: {run}"
    );
}

/// (F89-8) exit 0 alone is not success: an is_error result -> died; an empty stream -> died with
/// reason `no terminal event`. Both exit non-zero.
#[test]
fn exit_zero_without_a_success_terminal_is_not_done() {
    let e = setup("errres");
    let fake = write_error_result_launcher(&e.work);
    let fakep = fake.to_string_lossy().to_string();
    let out = e.work(
        "ZAI-claude",
        &[],
        &[("CODEX_CONSULT_CLAUDE_EXE", fakep.as_str())],
    );
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    let rec = e.work_root().join("01-ZAI-claude-glm-5.3");
    let run: Value = serde_json::from_str(&read(&rec.join("run.json"))).unwrap();
    assert_eq!(run["status"], "died", "error result -> died: {run}");
    assert_eq!(run["exit_code"], 0, "{run}");

    let e2 = setup("empty");
    let fake = write_empty_stream_launcher(&e2.work);
    let fakep = fake.to_string_lossy().to_string();
    let out = e2.work(
        "ZAI-claude",
        &[],
        &[("CODEX_CONSULT_CLAUDE_EXE", fakep.as_str())],
    );
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    let rec = e2.work_root().join("01-ZAI-claude-glm-5.3");
    let run: Value = serde_json::from_str(&read(&rec.join("run.json"))).unwrap();
    assert_eq!(run["status"], "died", "empty stream -> died: {run}");
    assert_eq!(run["error"], "no terminal event", "{run}");
}

/// (F89-3) An artifact write failure makes a resumed run `artifact_error`, never `done`: with
/// report.md replaced by a directory the write fails; the attempt stream is still appended first.
#[test]
fn an_artifact_write_failure_is_not_done() {
    let e = setup("artifact");
    let fake = write_quota_launcher(&e.work);
    let fakep = fake.to_string_lossy().to_string();
    let qenv: Vec<(&str, &str)> = vec![("CODEX_CONSULT_CLAUDE_EXE", fakep.as_str())];

    let out = e.work("ZAI-claude", &[], &qenv);
    assert_eq!(out.status.code(), Some(3), "{}", text(&out));
    let rec = e.work_root().join("01-ZAI-claude-glm-5.3");
    let report = rec.join("report.md");
    let _ = std::fs::remove_file(&report);
    std::fs::create_dir(&report).unwrap();

    let out = e.c3(&["work", "--task", "t", "--resume", "01"], &qenv);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    let run: Value = serde_json::from_str(&read(&rec.join("run.json"))).unwrap();
    assert_eq!(run["status"], "artifact_error", "{run}");
    assert!(
        run["error"].as_str().unwrap_or("").contains("report.md"),
        "{run}"
    );
    let events = read(&rec.join("events.jsonl"));
    assert!(
        events.contains("\"subtype\":\"resume\",\"n\":1"),
        "{events}"
    );
}

/// (F89-1/F89-2) A foreground run stays `running` with a live owning pid while its child blocks,
/// and a second start on the same task is refused before any second record.
#[test]
fn a_live_foreground_run_refuses_a_second_launch() {
    let e = setup("live");
    let fake = write_blocking_launcher(&e.work);
    let fakep = fake.to_string_lossy().to_string();
    let env: Vec<(&str, &str)> = vec![("CODEX_CONSULT_CLAUDE_EXE", fakep.as_str())];

    let brief = e.brief.to_string_lossy().to_string();
    let child = e
        .cmd(
            &[
                "work",
                "--task",
                "t",
                "--brief",
                &brief,
                "--provider",
                "ZAI-claude",
            ],
            &env,
        )
        .spawn()
        .expect("spawn c3");

    let run_json = e.work_root().join("01-ZAI-claude-glm-5.3").join("run.json");
    let mut ok = false;
    for _ in 0..200 {
        if run_json.is_file() {
            if let Ok(v) = serde_json::from_str::<Value>(&read(&run_json)) {
                if v["status"] == "running" && v["pid"].as_i64().unwrap_or(0) > 0 {
                    ok = true;
                    break;
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(ok, "the run never reported running with a pid");

    // --status reads it as running (exit 2) — the pid owner is alive past any start grace
    let st = e.c3(&["work", "--task", "t", "--status"], &env);
    assert_eq!(st.status.code(), Some(2), "{}", text(&st));

    // a second start on the same task refuses before spawning a second record
    let second = e.work("ZAI-claude", &[], &env);
    let t = text(&second);
    assert_eq!(second.status.code(), Some(1), "{t}");
    assert!(t.contains("already has a live work run"), "{t}");
    assert!(!e.work_root().join("02-ZAI-claude-glm-5.3").exists());

    std::fs::write(e.repo.join("release.txt"), b"1").unwrap();
    let out = child.wait_with_output().expect("wait c3");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let run: Value = serde_json::from_str(&read(&run_json)).unwrap();
    assert_eq!(run["status"], "done", "{run}");
}

// ------------------------------------------------------------------ WAVE 10b (F91-1..F91-4, F89-4/5)

/// (F91-4) A claude launcher whose result MISSES is_error, exit 0: not affirmative success -> died.
#[cfg(windows)]
fn write_result_no_iserror_launcher(work: &Path) -> PathBuf {
    let p = work.join("fake-noiserr-claude.cmd");
    std::fs::write(
        &p,
        concat!(
            "@echo off\r\n",
            "@echo {\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"99999999-9999-9999-9999-999999999999\",\"model\":\"glm-5.3\"}\r\n",
            "@echo {\"type\":\"result\",\"result\":\"no is_error field here\"}\r\n",
            "@exit /b 0\r\n",
        ),
    )
    .unwrap();
    p
}
#[cfg(unix)]
fn write_result_no_iserror_launcher(work: &Path) -> PathBuf {
    let p = work.join("fake-noiserr-claude.sh");
    std::fs::write(
        &p,
        concat!(
            "#!/bin/sh\n",
            "cat >/dev/null\n",
            "echo '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"99999999-9999-9999-9999-999999999999\",\"model\":\"glm-5.3\"}'\n",
            "echo '{\"type\":\"result\",\"result\":\"no is_error field here\"}'\n",
            "exit 0\n",
        ),
    )
    .unwrap();
    set_executable(&p);
    p
}

/// (F91-4) A codex launcher that emits an agent_message item but NO turn.completed, exit 0 -> died.
#[cfg(windows)]
fn write_codex_no_turn_launcher(work: &Path) -> PathBuf {
    let p = work.join("fake-noturn-codex.cmd");
    std::fs::write(
        &p,
        concat!(
            "@echo off\r\n",
            "@echo {\"type\":\"thread.started\",\"thread_id\":\"abababab-1111-4111-8111-111111111111\"}\r\n",
            "@echo {\"type\":\"item.completed\",\"item\":{\"id\":\"a1\",\"type\":\"agent_message\",\"text\":\"message but no turn terminal\"}}\r\n",
            "@exit /b 0\r\n",
        ),
    )
    .unwrap();
    p
}
#[cfg(unix)]
fn write_codex_no_turn_launcher(work: &Path) -> PathBuf {
    let p = work.join("fake-noturn-codex.sh");
    std::fs::write(
        &p,
        concat!(
            "#!/bin/sh\n",
            "cat >/dev/null\n",
            "echo '{\"type\":\"thread.started\",\"thread_id\":\"abababab-1111-4111-8111-111111111111\"}'\n",
            "echo '{\"type\":\"item.completed\",\"item\":{\"id\":\"a1\",\"type\":\"agent_message\",\"text\":\"message but no turn terminal\"}}'\n",
            "exit 0\n",
        ),
    )
    .unwrap();
    set_executable(&p);
    p
}

/// (F89-5) A launcher whose RESULT TEXT embeds a sanitizer-fixture AWS key sentinel.
#[cfg(windows)]
fn write_secret_report_launcher(work: &Path) -> PathBuf {
    let p = work.join("fake-secret-claude.cmd");
    std::fs::write(
        &p,
        concat!(
            "@echo off\r\n",
            "echo touched>worker-touched.txt\r\n",
            "@echo {\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"abcabcab-0000-0000-0000-00000000abcd\",\"model\":\"glm-5.3\"}\r\n",
            "@echo {\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"num_turns\":1,\"result\":\"Done. The key was AKIAIOSFODNN7EXAMPLE in the log.\"}\r\n",
            "@exit /b 0\r\n",
        ),
    )
    .unwrap();
    p
}
#[cfg(unix)]
fn write_secret_report_launcher(work: &Path) -> PathBuf {
    let p = work.join("fake-secret-claude.sh");
    std::fs::write(
        &p,
        concat!(
            "#!/bin/sh\n",
            "cat >/dev/null\n",
            "echo touched >worker-touched.txt\n",
            "echo '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"abcabcab-0000-0000-0000-00000000abcd\",\"model\":\"glm-5.3\"}'\n",
            "echo '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"num_turns\":1,\"result\":\"Done. The key was AKIAIOSFODNN7EXAMPLE in the log.\"}'\n",
            "exit 0\n",
        ),
    )
    .unwrap();
    set_executable(&p);
    p
}

/// (F91-4) exit 0 with an INCOMPLETE terminal stream is not success: a claude result missing
/// is_error, and a codex agent_message with no turn.completed -> died, naming the missing terminal.
#[test]
fn exit_zero_with_incomplete_terminal_is_not_done() {
    let e = setup("noiserr");
    let fake = write_result_no_iserror_launcher(&e.work);
    let fakep = fake.to_string_lossy().to_string();
    let out = e.work(
        "ZAI-claude",
        &[],
        &[("CODEX_CONSULT_CLAUDE_EXE", fakep.as_str())],
    );
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    let rec = e.work_root().join("01-ZAI-claude-glm-5.3");
    let run: Value = serde_json::from_str(&read(&rec.join("run.json"))).unwrap();
    assert_eq!(
        run["status"], "died",
        "claude result without is_error -> died: {run}"
    );

    let e2 = setup("noturn");
    let fake = write_codex_no_turn_launcher(&e2.work);
    let fakep = fake.to_string_lossy().to_string();
    let out = e2.work("openai", &[], &[("CODEX_CONSULT_EXE", fakep.as_str())]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    let rec = e2.work_root().join("01-openai-gpt-6.1-sol");
    let run: Value = serde_json::from_str(&read(&rec.join("run.json"))).unwrap();
    assert_eq!(
        run["status"], "died",
        "codex agent_message without turn.completed -> died: {run}"
    );
    assert_eq!(run["error"], "no terminal event", "{run}");
}

/// (F89-5) report.md passes the sanitizer: a secret sentinel in the worker's report text is
/// redacted in report.md, absent from run.json, and the raw stream in events.jsonl keeps it.
#[test]
fn report_md_and_run_json_pass_the_sanitizer() {
    let e = setup("sanitize");
    let fake = write_secret_report_launcher(&e.work);
    let fakep = fake.to_string_lossy().to_string();
    let out = e.work(
        "ZAI-claude",
        &[],
        &[("CODEX_CONSULT_CLAUDE_EXE", fakep.as_str())],
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let rec = e.work_root().join("01-ZAI-claude-glm-5.3");
    let report = read(&rec.join("report.md"));
    assert!(
        report.contains("[REDACTED:aws-key]"),
        "report redacted: {report}"
    );
    assert!(
        !report.contains("AKIAIOSFODNN7EXAMPLE"),
        "secret survived in report: {report}"
    );
    let run = read(&rec.join("run.json"));
    assert!(
        !run.contains("AKIAIOSFODNN7EXAMPLE"),
        "secret survived in run.json"
    );
    // events.jsonl is the raw stream, retained verbatim (keeps the secret by contract)
    let events = read(&rec.join("events.jsonl"));
    assert!(
        events.contains("AKIAIOSFODNN7EXAMPLE"),
        "events.jsonl is raw: {events}"
    );
}

/// (F91-2) A live worker whose run.json cannot be inspected makes a second same-task command FAIL
/// CLOSED: admission refuses before any launch, and no second record appears.
#[test]
fn an_unreadable_live_record_refuses_a_second_launch() {
    let e = setup("unreadable");
    let fake = write_blocking_launcher(&e.work);
    let fakep = fake.to_string_lossy().to_string();
    let env: Vec<(&str, &str)> = vec![("CODEX_CONSULT_CLAUDE_EXE", fakep.as_str())];

    let brief = e.brief.to_string_lossy().to_string();
    let child = e
        .cmd(
            &[
                "work",
                "--task",
                "t",
                "--brief",
                &brief,
                "--provider",
                "ZAI-claude",
            ],
            &env,
        )
        .spawn()
        .expect("spawn c3");

    let rec = e.work_root().join("01-ZAI-claude-glm-5.3");
    let run_json = rec.join("run.json");
    let mut ok = false;
    for _ in 0..200 {
        if run_json.is_file() {
            if let Ok(v) = serde_json::from_str::<Value>(&read(&run_json)) {
                if v["status"] == "running" && v["pid"].as_i64().unwrap_or(0) > 0 {
                    ok = true;
                    break;
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(ok, "the run never reported running");

    // make the live run's record UNREADABLE: replace run.json with a directory
    std::fs::remove_file(&run_json).unwrap();
    std::fs::create_dir(&run_json).unwrap();

    // a second start must refuse (fail closed), naming no contents, with no second record
    let second = e.work("ZAI-claude", &[], &env);
    let t = text(&second);
    assert_eq!(second.status.code(), Some(1), "{t}");
    assert!(
        t.contains("cannot admit") || t.contains("could not be read"),
        "{t}"
    );
    assert!(!e.work_root().join("02-ZAI-claude-glm-5.3").exists());

    std::fs::write(e.repo.join("release.txt"), b"1").unwrap();
    let _ = child.wait_with_output();
}

/// (F91-1) The locked claim rejects a STALE resume: a delayed resume that passed its unlocked
/// checks refuses after a competing resume completes the run, with no launch marker.
#[test]
fn a_stale_resume_refuses_under_the_lock() {
    let e = setup("stale");
    let fake = write_quota_launcher(&e.work);
    let fakep = fake.to_string_lossy().to_string();
    let qenv: Vec<(&str, &str)> = vec![("CODEX_CONSULT_CLAUDE_EXE", fakep.as_str())];

    // first run -> quota (resumes 0)
    let out = e.work("ZAI-claude", &[], &qenv);
    assert_eq!(out.status.code(), Some(3), "{}", text(&out));
    let rec = e.work_root().join("01-ZAI-claude-glm-5.3");

    // start the DELAYED resume B, paused at the claim barrier (test mode + a gate file)
    let gate = e.work.join("claim-gate.txt");
    let gates = gate.to_string_lossy().to_string();
    let benv: Vec<(&str, &str)> = vec![
        ("CODEX_CONSULT_CLAUDE_EXE", fakep.as_str()),
        ("CODEX_CONSULT_TEST_MODE", "1"),
        ("CODEX_CONSULT_TEST_WORK_CLAIM_BARRIER", gates.as_str()),
    ];
    let b = e
        .cmd(&["work", "--task", "t", "--resume", "01"], &benv)
        .spawn()
        .expect("spawn B");

    // let B reach the barrier (it read the quota record, resumes=0)
    std::thread::sleep(std::time::Duration::from_millis(1500));

    // meanwhile resume A finishes the run (done, resumes 1)
    let a = e.c3(&["work", "--task", "t", "--resume", "01"], &qenv);
    assert_eq!(a.status.code(), Some(0), "A should finish: {}", text(&a));
    let run: Value = serde_json::from_str(&read(&rec.join("run.json"))).unwrap();
    assert_eq!(run["status"], "done", "{run}");

    // release B: its locked re-read now sees a terminal record -> refuse, no launch marker
    std::fs::write(&gate, b"1").unwrap();
    let out = b.wait_with_output().expect("wait B");
    let t = text(&out);
    assert_ne!(out.status.code(), Some(0), "B must refuse: {t}");
    assert!(
        !t.contains("resuming work run"),
        "B must not launch a resume: {t}"
    );
    // the record is still A's done result, resumes unchanged at 1
    let run: Value = serde_json::from_str(&read(&rec.join("run.json"))).unwrap();
    assert_eq!(run["status"], "done", "{run}");
    assert_eq!(run["quota"]["resumes"], 1, "{run}");
}

// ------------------------------------------------------------------ WAVE 10c (F93-1)

/// Write a raw run.json for `01` with the given status fields (F93-1 regression helper).
fn write_raw_run(e: &Env, status: &str, pid: Option<i64>, attempt: i64, quota: Option<&str>) {
    let dir = e.work_root().join("01-ZAI-claude-glm-5.3");
    std::fs::create_dir_all(&dir).unwrap();
    let pid_field = pid.map(|p| format!(",\"pid\":{p}")).unwrap_or_default();
    let quota_field = quota.map(|q| format!(",\"quota\":{q}")).unwrap_or_default();
    let json = format!(
        concat!(
            "{{\"id\":\"01-ZAI-claude-glm-5.3\",\"task\":\"t\",\"seq\":1,\"provider\":\"ZAI-claude\",",
            "\"model\":\"glm-5.3\",\"engine\":\"claude\",\"argv\":[\"-p\",\"--model\",\"glm-5.3\",\"--session-id\",\"s\"],",
            "\"child_env_scrubbed\":[],\"started\":\"2026-10-21T10:00:00+00:00\",\"status\":\"{status}\",",
            "\"session_id\":\"s\",\"attempt\":{attempt},\"max_resumes\":3,\"quota_window_sec\":18000,",
            "\"pid_start\":\"\",\"timeout_sec\":3600{pid_field}{quota_field}}}"
        ),
        status = status,
        attempt = attempt,
        pid_field = pid_field,
        quota_field = quota_field,
    );
    std::fs::write(dir.join("run.json"), json).unwrap();
}

/// (F93-1) A delayed status-recovery write must NOT overwrite a newly claimed live resume. The
/// status reader snapshots a running record whose owner is dead, pauses at the recovery barrier,
/// a resume claims the record under the lock and launches a (blocking) live child; the delayed
/// recovery re-reads under the lock, sees the generation/owner changed, and does NOT write `died`.
/// A fresh same-task start then refuses before spawning.
#[test]
fn a_delayed_status_recovery_cannot_overwrite_a_live_resume_claim() {
    let e = setup("recovery");
    let blocking = write_blocking_launcher(&e.work);
    let bp = blocking.to_string_lossy().to_string();
    let benv: Vec<(&str, &str)> = vec![("CODEX_CONSULT_CLAUDE_EXE", bp.as_str())];

    // 01 is `running` with a DEAD owner (pid 999999 does not exist): the "A running" snapshot.
    write_raw_run(&e, "running", Some(999999), 0, None);

    // start --status, paused at the recovery barrier (gate absent) in test mode
    let gate = e.work.join("recovery-gate.txt");
    let gates = gate.to_string_lossy().to_string();
    let senv: Vec<(&str, &str)> = vec![
        ("CODEX_CONSULT_TEST_MODE", "1"),
        ("CODEX_CONSULT_TEST_WORK_RECOVERY_BARRIER", gates.as_str()),
    ];
    let status = e
        .cmd(&["work", "--task", "t", "--status"], &senv)
        .spawn()
        .expect("spawn status");
    std::thread::sleep(std::time::Duration::from_millis(1500)); // let status reach the barrier

    // A finished as quota (owner gone): rewrite 01 to a resumable quota record
    let quota = "{\"text\":\"usage limit\",\"reset_at\":null,\"resume_at\":\"2100-01-01T00:01:00+00:00\",\"resumes\":0,\"exhausted\":false}";
    write_raw_run(&e, "quota", None, 0, Some(quota));

    // resume B claims the quota record under the lock and launches a BLOCKING live child
    let b = e
        .cmd(&["work", "--task", "t", "--resume", "01"], &benv)
        .spawn()
        .expect("spawn B");
    let run_json = e.work_root().join("01-ZAI-claude-glm-5.3").join("run.json");
    let mut bpid = 0i64;
    for _ in 0..200 {
        if let Ok(v) = serde_json::from_str::<Value>(&read(&run_json)) {
            if v["status"] == "running" && v["pid"].as_i64().unwrap_or(0) > 0 && v["pid"] != 999999
            {
                bpid = v["pid"].as_i64().unwrap();
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(bpid > 0, "resume B never claimed the record as running");

    // release the status barrier: its delayed recovery must NOT clobber B's live claim
    std::fs::write(&gate, b"1").unwrap();
    let st = status.wait_with_output().expect("wait status");
    // the record on disk is still B's running claim, never a stale `died`
    let run: Value = serde_json::from_str(&read(&run_json)).unwrap();
    assert_eq!(
        run["status"],
        "running",
        "status recovery clobbered B: {}",
        text(&st)
    );
    assert_eq!(run["pid"].as_i64().unwrap(), bpid, "{run}");
    assert_eq!(run["attempt"], 1, "{run}");

    // a fresh same-task start refuses before spawning (B is live); no second record appears
    let fresh = e.work("ZAI-claude", &[], &benv);
    let t = text(&fresh);
    assert_eq!(fresh.status.code(), Some(1), "{t}");
    assert!(t.contains("already has a live work run"), "{t}");
    assert!(!e.work_root().join("02-ZAI-claude-glm-5.3").exists());

    // release B and reap
    std::fs::write(e.repo.join("release.txt"), b"1").unwrap();
    let _ = b.wait_with_output();
}
