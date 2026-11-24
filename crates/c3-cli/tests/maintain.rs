//! `c3 maintain` end to end through the real `c3` binary against FAKE engine/npm CLIs
//! (`tests/fixtures/fake-mnt.ps1` + the three `.cmd` wrappers). Covers an update + smoke pass, a
//! post-update smoke failure with a rollback, the supply-chain cool-down hold, the off switch, and
//! the SessionStart hook (report once, no spawn under test mode). The two-turn consult smoke itself
//! is scripted with `C3_MAINTAIN_TEST_SMOKE` so no real model is consulted; the update/rollback and
//! the real CLI invocations are exercised for real.
//!
//! GUARD: grok/codex launchers are the fakes (via `CODEX_CONSULT_GROK_EXE` / `CODEX_CONSULT_EXE`),
//! `npm` is a fake prepended to PATH, and the fakes carry distinctive names so the in-use process
//! scan never matches a real grok/codex. Nothing reaches a real provider. Windows only.
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FAKE_PS1: &str = include_str!("fixtures/fake-mnt.ps1");
const FAKE_GROK: &str = include_str!("fixtures/fake-mnt-grok.cmd");
const FAKE_CODEX: &str = include_str!("fixtures/fake-mnt-codex.cmd");
const FAKE_NPM: &str = include_str!("fixtures/fake-mnt-npm.cmd");

fn c3_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_c3"))
}

struct Env {
    work: PathBuf,
    home: PathBuf,
    bin: PathBuf,
    grok_exe: PathBuf,
    codex_exe: PathBuf,
    grok_verfile: PathBuf,
    codex_verfile: PathBuf,
}

fn setup(tag: &str) -> Env {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let work = std::env::temp_dir().join(format!("c3-mnt-{tag}-{}-{nanos}", std::process::id()));
    let bin = work.join("bin");
    let home = work.join("codexhome");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    // the fakes share one brain; the cmd wrappers set the role. npm must be literally "npm.cmd".
    std::fs::write(bin.join("fake-mnt.ps1"), FAKE_PS1).unwrap();
    let grok_exe = bin.join("fake-mnt-grok.cmd");
    let codex_exe = bin.join("fake-mnt-codex.cmd");
    std::fs::write(&grok_exe, crlf(FAKE_GROK)).unwrap();
    std::fs::write(&codex_exe, crlf(FAKE_CODEX)).unwrap();
    std::fs::write(bin.join("npm.cmd"), crlf(FAKE_NPM)).unwrap();
    Env {
        work: work.clone(),
        home,
        bin,
        grok_exe,
        codex_exe,
        grok_verfile: work.join("grok.ver"),
        codex_verfile: work.join("codex.ver"),
    }
}

fn crlf(s: &str) -> Vec<u8> {
    s.replace("\r\n", "\n").replace('\n', "\r\n").into_bytes()
}

impl Env {
    fn cmd(&self, args: &[&str], env: &[(&str, &str)]) -> Command {
        let mut cmd = Command::new(c3_bin());
        for (k, _) in std::env::vars() {
            if k.starts_with("CODEX_CONSULT_") || k.starts_with("FAKE_") || k.starts_with("C3_") {
                cmd.env_remove(&k);
            }
        }
        let path = format!(
            "{};{}",
            self.bin.to_string_lossy(),
            std::env::var("PATH").unwrap_or_default()
        );
        cmd.current_dir(&self.work)
            .env("PATH", path)
            .env("CODEX_HOME", &self.home)
            .env("CODEX_CONSULT_TEST_MODE", "1")
            .env("CODEX_CONSULT_ROSTER", "none")
            .env("CODEX_CONSULT_HEALTH", "none")
            .env("CODEX_CONSULT_TELEMETRY", "off")
            .env("C3_PRIORS", "off")
            .env("C3_MAINTAIN_MODELS", "off");
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

    fn run(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        self.cmd(args, env).output().expect("run c3")
    }

    /// The common env to maintain grok through the fake, smoke scripted by `smoke`.
    fn grok_env<'a>(
        &'a self,
        ver: &'a str,
        latest: &'a str,
        smoke: &'a str,
    ) -> Vec<(&'a str, &'a str)> {
        std::fs::write(&self.grok_verfile, ver).unwrap();
        vec![
            ("CODEX_CONSULT_GROK_EXE", self.grok_exe_s()),
            ("FAKE_GROK_VERFILE", self.grok_verfile_s()),
            ("FAKE_GROK_VER", ver),
            ("FAKE_GROK_LATEST", latest),
            ("FAKE_MNT_LOG", self.mnt_log_s()),
            ("C3_MAINTAIN_SMOKE_GROK", "grok::grok-4.7"),
            ("C3_MAINTAIN_TEST_SMOKE", smoke),
        ]
    }

    fn grok_exe_s(&self) -> &str {
        Box::leak(
            self.grok_exe
                .to_string_lossy()
                .into_owned()
                .into_boxed_str(),
        )
    }
    fn grok_verfile_s(&self) -> &str {
        Box::leak(
            self.grok_verfile
                .to_string_lossy()
                .into_owned()
                .into_boxed_str(),
        )
    }
    fn mnt_log_s(&self) -> &str {
        Box::leak(
            self.work
                .join("fake.log")
                .to_string_lossy()
                .into_owned()
                .into_boxed_str(),
        )
    }
    fn mnt_log(&self) -> String {
        std::fs::read_to_string(self.work.join("fake.log")).unwrap_or_default()
    }

    fn state_path(&self) -> PathBuf {
        self.home.join("c3").join("maintain").join("state.json")
    }

    fn write_state(&self, json: &str) {
        let dir = self.home.join("c3").join("maintain");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("state.json"), json).unwrap();
    }

    fn last_run_log(&self) -> PathBuf {
        self.home.join("c3").join("maintain").join("last-run.log")
    }

    fn pending_path(&self) -> PathBuf {
        self.home.join("c3").join("maintain").join("pending.json")
    }

    fn write_pending(&self, json: &str) {
        let dir = self.home.join("c3").join("maintain");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("pending.json"), json).unwrap();
    }

    fn pending_json(&self) -> String {
        std::fs::read_to_string(self.pending_path()).unwrap_or_default()
    }

    fn dumpenv_path(&self) -> PathBuf {
        self.work.join("envdump.txt")
    }

    fn dumpenv(&self) -> String {
        std::fs::read_to_string(self.dumpenv_path()).unwrap_or_default()
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

fn verfile(p: &Path) -> String {
    std::fs::read_to_string(p)
        .unwrap_or_default()
        .trim()
        .to_string()
}

// ------------------------------------------------------------------ update + smoke ok

#[test]
fn an_update_passes_its_smoke_and_is_installed() {
    let e = setup("update-ok");
    let env = e.grok_env("1.0.77", "1.0.80", "grok::grok-4.7=pass");
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("grok 1.0.77 -> 1.0.80 ok"),
        "the pass reports the update: {t}"
    );
    assert_eq!(
        verfile(&e.grok_verfile),
        "1.0.80",
        "the new version is installed"
    );
}

// ------------------------------------------------------------------ the in-use probe under test mode

/// A process that stays alive while a pass runs; killed when dropped (a failed assert too).
struct Running(std::process::Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// (0.2.5 D3) An engine CLI running on the machine - here a process whose command line names the
/// fake grok launcher, which the real process scan reads as that engine in use - does not make a
/// test pass skip the engine: under test mode the probe never reads the machine's process table
/// (another session's real `grok.exe` used to fail six to nine of these tests).
#[test]
fn a_running_engine_process_does_not_make_a_test_pass_skip_the_engine() {
    let e = setup("in-use-machine");
    let _running = Running(
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &format!("Start-Sleep -Seconds 90 # {}", e.grok_exe.display()),
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("start the stand-in engine process"),
    );
    let env = e.grok_env("1.0.77", "1.0.80", "grok::grok-4.7=pass");
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(!t.contains("in use"), "the machine was not read: {t}");
    assert!(t.contains("grok 1.0.77 -> 1.0.80 ok"), "{t}");
    assert_eq!(verfile(&e.grok_verfile), "1.0.80");
}

/// (0.2.5 D3) The "in use - skipped" path, driven through the test hook: the engine it names is
/// skipped (nothing installed, due again tomorrow); a hook naming other engines leaves it alone.
#[test]
fn an_engine_the_test_hook_reports_in_use_is_skipped_and_due_tomorrow() {
    let e = setup("in-use-hook");
    let mut env = e.grok_env("1.0.77", "1.0.80", "grok::grok-4.7=pass");
    env.push(("CODEX_CONSULT_TEST_MAINTAIN_IN_USE", "codex,Grok"));
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(t.contains("c3 maintain: grok skipped: in use"), "{t}");
    assert!(
        t.contains("c3: maintenance") && t.contains(" - grok skipped: in use"),
        "the report line names it: {t}"
    );
    assert_eq!(verfile(&e.grok_verfile), "1.0.77", "nothing was installed");
    assert!(!e.mnt_log().contains("update --version"), "{}", e.mnt_log());
    let state: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(e.state_path()).unwrap()).unwrap();
    assert_eq!(state["engines"][0]["action"], "skipped: in use", "{state}");
    let last = chrono::DateTime::parse_from_rfc3339(state["last_pass"].as_str().unwrap()).unwrap();
    let next = chrono::DateTime::parse_from_rfc3339(state["next_due"].as_str().unwrap()).unwrap();
    assert_eq!(next - last, chrono::Duration::days(1), "{state}");

    // a hook that names other engines only: grok is updated
    let mut env = e.grok_env("1.0.77", "1.0.80", "grok::grok-4.7=pass");
    env.push(("CODEX_CONSULT_TEST_MAINTAIN_IN_USE", "codex,kimi"));
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert!(t.contains("grok 1.0.77 -> 1.0.80 ok"), "{t}");
}

// ------------------------------------------------------------------ post-update smoke fails -> rollback

#[test]
fn a_post_update_smoke_failure_rolls_back() {
    let e = setup("rollback");
    // the installed-version smoke passes, the post-update smoke fails.
    let env = e.grok_env("1.0.77", "1.0.80", "grok::grok-4.7=pass-then-fail");
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("smoke failed, rolled back"),
        "the run rolled back: {t}"
    );
    assert_eq!(
        verfile(&e.grok_verfile),
        "1.0.77",
        "rolled back to the old version\n--- fake invocations ---\n{}",
        e.mnt_log()
    );
}

// (F11-7) a post-update smoke that could not PROVE what it must (the sub-agent switch on both
// turns) holds the update exactly like a failed one: rolled back, never "ok".
#[test]
fn an_unproven_post_update_smoke_rolls_back_like_a_failure() {
    let e = setup("unproven-rb");
    let env = e.grok_env("1.0.77", "1.0.80", "grok::grok-4.7=pass-then-unproven");
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("smoke unproven, rolled back"),
        "the unproven smoke rolled the update back: {t}"
    );
    assert!(!t.contains("1.0.80 ok"), "never reported as ok: {t}");
    assert_eq!(
        verfile(&e.grok_verfile),
        "1.0.77",
        "rolled back to the old version\n--- fake invocations ---\n{}",
        e.mnt_log()
    );
}

// (F11-7) an unproven recovery smoke of an interrupted update is never "verified": the update is
// rolled back to the recorded previous version.
#[test]
fn an_unproven_recovery_smoke_is_never_recorded_verified() {
    let e = setup("pend-unproven");
    e.write_pending(
        r#"[{"engine":"grok","previous":"1.0.77","target":"1.0.80","when":"2026-11-15T00:00:00Z"}]"#,
    );
    let env = e.grok_env("1.0.80", "1.0.80", "grok::grok-4.7=unproven");
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(!t.contains("verified"), "never recorded as verified: {t}");
    assert!(
        t.contains("rolled back to 1.0.77") && t.contains("scripted unproven smoke"),
        "the unproven update is rolled back: {t}"
    );
    assert_eq!(verfile(&e.grok_verfile), "1.0.77");
}

// ------------------------------------------------------------------ held when too fresh (codex / npm)

#[test]
fn a_too_fresh_version_is_held() {
    let e = setup("held");
    // min age raised far above any real release age, so the known npm release date always holds.
    let codex_exe = e.codex_exe.to_string_lossy().into_owned();
    let cverfile = e.codex_verfile.to_string_lossy().into_owned();
    let env = vec![
        ("CODEX_CONSULT_EXE", codex_exe.as_str()),
        ("FAKE_CODEX_VERFILE", cverfile.as_str()),
        ("FAKE_CODEX_VER", "0.169.0"),
        ("FAKE_NPM_LATEST", "0.170.0"),
        ("FAKE_NPM_DATE", "2020-01-01T00:00:00Z"),
        ("C3_MAINTAIN_MIN_AGE_DAYS", "100000"),
    ];
    let o = e.run(&["maintain", "--run", "--engine", "codex"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("held: released"),
        "the fresh version is held: {t}"
    );
    assert!(
        !e.codex_verfile.exists(),
        "nothing was installed (no install call wrote the version file)"
    );
}

// ------------------------------------------------------------------ --dry-run changes nothing

#[test]
fn a_dry_run_changes_nothing() {
    let e = setup("dry");
    let env = e.grok_env("1.0.77", "1.0.80", "grok::grok-4.7=pass");
    let o = e.run(
        &["maintain", "--run", "--dry-run", "--engine", "grok"],
        &env,
    );
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("would update 1.0.77 -> 1.0.80"),
        "dry-run says what it would do: {t}"
    );
    assert_eq!(
        verfile(&e.grok_verfile),
        "1.0.77",
        "the dry run installed nothing"
    );
    // a dry run leaves the state untouched - no state file for a later hook to mistake for a pass.
    assert!(
        !e.state_path().exists(),
        "a dry run writes no state file: {}",
        e.state_path().display()
    );
}

// ------------------------------------------------------------------ the off switch

#[test]
fn off_silences_the_hook_but_a_manual_run_still_works() {
    let e = setup("off");
    // a finished pass is on record but unreported; with C3_MAINTAIN=off the hook must not print it.
    e.write_state(r#"{"report":"c3: maintenance 2026-11-15 - grok up to date","reported":false,"next_due":"2099-01-01T00:00:00Z"}"#);
    let o = e.run(&["hook"], &[("C3_MAINTAIN", "off")]);
    let t = text(&o);
    assert!(
        !t.contains("c3: maintenance"),
        "off: the hook prints no maintenance line: {t}"
    );

    // a manual --run still works even with the feature off.
    let mut env = e.grok_env("1.0.77", "1.0.80", "grok::grok-4.7=pass");
    env.push(("C3_MAINTAIN", "off"));
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert!(
        t.contains("grok 1.0.77 -> 1.0.80 ok"),
        "manual --run works when off: {t}"
    );
}

// ------------------------------------------------------------------ the hook reports once

#[test]
fn the_hook_prints_the_report_line_exactly_once() {
    let e = setup("hook-once");
    e.write_state(r#"{"report":"c3: maintenance 2026-11-15 - grok up to date","reported":false,"next_due":"2099-01-01T00:00:00Z"}"#);
    let first = text(&e.run(&["hook"], &[]));
    assert!(
        first.contains("c3: maintenance 2026-11-15 - grok up to date"),
        "first hook prints it: {first}"
    );
    let second = text(&e.run(&["hook"], &[]));
    assert!(
        !second.contains("c3: maintenance"),
        "second hook does not repeat it: {second}"
    );
}

// ------------------------------------------------------------------ an unparsable installed version is held

#[test]
fn an_unparsable_installed_version_is_held() {
    let e = setup("unknown-ver");
    // the fake prints "grok nightly" -> no semver token -> installed version unknown
    let env = e.grok_env("nightly", "1.0.80", "grok::grok-4.7=pass");
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("held: installed version unknown"),
        "unknown version held: {t}"
    );
    assert_eq!(verfile(&e.grok_verfile), "nightly", "nothing was installed");
}

// ------------------------------------------------------------------ update did not take

#[test]
fn an_update_that_does_not_take_is_reported_not_smoked() {
    let e = setup("not-take");
    let mut env = e.grok_env("1.0.77", "1.0.80", "grok::grok-4.7=pass");
    env.push(("FAKE_GROK_UPDATE_NOOP", "1"));
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("update did not take"),
        "a no-op update is caught: {t}"
    );
    assert_eq!(verfile(&e.grok_verfile), "1.0.77", "still the old version");
}

// ------------------------------------------------------------------ rollback is believed only when re-read confirms it

#[test]
fn a_rollback_that_does_not_restore_is_reported_failed() {
    let e = setup("rb-fail");
    // forward update takes (1.0.80), the post-update smoke fails, but the rollback is a no-op
    let mut env = e.grok_env("1.0.77", "1.0.80", "grok::grok-4.7=pass-then-fail");
    env.push(("FAKE_GROK_ROLLBACK_NOOP", "1"));
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("rollback failed, still 1.0.80"),
        "an unverified rollback is caught: {t}"
    );
    assert_eq!(
        verfile(&e.grok_verfile),
        "1.0.80",
        "the rollback did not restore"
    );
}

// ------------------------------------------------------------------ an update is pinned to the checked version

#[test]
fn the_grok_update_is_pinned_to_the_checked_version() {
    let e = setup("pin");
    let env = e.grok_env("1.0.77", "1.0.80", "grok::grok-4.7=pass");
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    // the recorded invocation pins --version to the checked latest, not a bare `update`
    let log = e.mnt_log();
    assert!(
        log.contains("update --version 1.0.80"),
        "the update is pinned to the checked version:\n{log}"
    );
}

// ------------------------------------------------------------------ provider secrets are scrubbed from update children

#[test]
fn update_children_do_not_see_provider_secrets() {
    let e = setup("scrub");
    let dump = e.dumpenv_path().to_string_lossy().into_owned();
    let mut env = e.grok_env("1.0.77", "1.0.80", "grok::grok-4.7=pass");
    env.push(("ZAI_API_KEY", "super-secret-should-not-leak"));
    env.push(("MY_AUTH_TOKEN", "also-secret"));
    env.push(("FAKE_MNT_DUMPENV", dump.as_str()));
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let dumped = e.dumpenv();
    assert!(!dumped.is_empty(), "the fake recorded its environment");
    assert!(
        !dumped.contains("ZAI_API_KEY"),
        "the API key was scrubbed:\n{dumped}"
    );
    assert!(
        !dumped.contains("MY_AUTH_TOKEN"),
        "the token was scrubbed:\n{dumped}"
    );
    assert!(
        dumped.to_uppercase().contains("PATH"),
        "PATH is kept so the child can still run:\n{dumped}"
    );
}

// ------------------------------------------------------------------ pending-update recovery (interrupted pass)

#[test]
fn a_pending_update_whose_install_never_took_is_cleared() {
    let e = setup("pend-clear");
    // the installed version is still the previous one -> the interrupted update never took
    e.write_pending(
        r#"[{"engine":"grok","previous":"1.0.77","target":"1.0.80","when":"2026-11-15T00:00:00Z"}]"#,
    );
    let env = e.grok_env("1.0.77", "1.0.80", "grok::grok-4.7=pass");
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("pending update cleared"),
        "the stale pending record is cleared: {t}"
    );
    assert_eq!(e.pending_json().trim(), "[]", "the journal is emptied");
    assert_eq!(verfile(&e.grok_verfile), "1.0.77", "nothing re-installed");
}

#[test]
fn a_pending_update_whose_new_binary_smokes_clean_is_recorded_verified() {
    let e = setup("pend-ok");
    // the new binary is installed (1.0.80) and smokes clean -> recorded as verified
    e.write_pending(
        r#"[{"engine":"grok","previous":"1.0.77","target":"1.0.80","when":"2026-11-15T00:00:00Z"}]"#,
    );
    let env = e.grok_env("1.0.80", "1.0.80", "grok::grok-4.7=pass");
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("verified after an interrupted pass"),
        "the new binary is verified: {t}"
    );
    assert_eq!(e.pending_json().trim(), "[]", "the journal is emptied");
    assert_eq!(verfile(&e.grok_verfile), "1.0.80", "the new version stands");
}

#[test]
fn a_pending_update_whose_new_binary_fails_its_smoke_is_rolled_back() {
    let e = setup("pend-rb");
    // the new binary is installed (1.0.80) but its smoke fails -> roll back to the recorded previous
    e.write_pending(
        r#"[{"engine":"grok","previous":"1.0.77","target":"1.0.80","when":"2026-11-15T00:00:00Z"}]"#,
    );
    let env = e.grok_env("1.0.80", "1.0.80", "grok::grok-4.7=fail");
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("rolled back to 1.0.77"),
        "the interrupted update is rolled back: {t}"
    );
    assert_eq!(e.pending_json().trim(), "[]", "the journal is emptied");
    assert_eq!(
        verfile(&e.grok_verfile),
        "1.0.77",
        "rolled back to the previous version"
    );
}

// ------------------------------------------------------------------ (R1) a failed installer keeps the pending record

#[test]
fn a_failed_installer_that_mutated_keeps_the_pending_record() {
    // (R1) the installer writes the new binary (mutates the install) but then exits non-zero: the
    // update is reported failed, yet 1.0.80 is now on disk. The pending record must NOT be cleared -
    // the next pass needs it to smoke / roll back the half-installed version instead of reading it
    // as "up to date".
    let e = setup("mutate-fail");
    let mut env = e.grok_env("1.0.77", "1.0.80", "grok::grok-4.7=pass");
    env.push(("FAKE_GROK_UPDATE_MUTATE_FAIL", "1"));
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("update failed"),
        "the failed installer is reported: {t}"
    );
    assert_eq!(
        verfile(&e.grok_verfile),
        "1.0.80",
        "the installer mutated the install"
    );
    assert_ne!(
        e.pending_json().trim(),
        "[]",
        "the pending record survives a failed installer: {}",
        e.pending_json()
    );
    assert!(
        e.pending_json().contains("grok"),
        "the grok pending record is retained: {}",
        e.pending_json()
    );
}

#[test]
fn a_failed_rollback_keeps_the_pending_record() {
    // (R1) the post-update smoke fails and the rollback is a no-op (does not restore): the engine is
    // left on the new, unverified 1.0.80. The pending record must survive so the next pass retries
    // recovery, rather than being cleared while the broken version stands.
    let e = setup("rb-fail-pending");
    let mut env = e.grok_env("1.0.77", "1.0.80", "grok::grok-4.7=pass-then-fail");
    env.push(("FAKE_GROK_ROLLBACK_NOOP", "1"));
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("rollback failed, still 1.0.80"),
        "the failed rollback is reported: {t}"
    );
    assert_eq!(
        verfile(&e.grok_verfile),
        "1.0.80",
        "the rollback did not restore"
    );
    assert_ne!(
        e.pending_json().trim(),
        "[]",
        "the pending record survives a failed rollback: {}",
        e.pending_json()
    );
}

// --------------------------------------------- (F11-1) pending recovery stays unresolved on failed rollback

#[test]
fn a_pending_recovery_whose_rollback_keeps_failing_stays_unresolved() {
    // (F11-1) A retained pending update reaches a later recovery pass: the installed 1.0.80 still
    // fails its smoke and the rollback is a no-op (does not restore 1.0.77). Recovery must report
    // "unresolved" (never "recovered") and KEEP the pending record across repeated forced passes, so
    // the broken version is re-smoked next time instead of being cleared and read as up to date.
    let e = setup("pend-rb-unresolved");
    e.write_pending(
        r#"[{"engine":"grok","previous":"1.0.77","target":"1.0.80","when":"2026-11-15T00:00:00Z"}]"#,
    );
    // installed is the new 1.0.80 (so the interrupted update "took"); the recovery smoke fails and
    // the rollback is a no-op, leaving 1.0.80 on disk.
    let mut env = e.grok_env("1.0.80", "1.0.80", "grok::grok-4.7=fail");
    env.push(("FAKE_GROK_ROLLBACK_NOOP", "1"));

    // pass 1: unresolved, record retained.
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("unresolved"),
        "recovery is reported unresolved: {t}"
    );
    assert!(
        !t.contains("recovered"),
        "a failed rollback is never reported 'recovered': {t}"
    );
    assert!(
        e.pending_json().contains("grok"),
        "the pending record is retained after pass 1: {}",
        e.pending_json()
    );

    // pass 2: still failing, the record must survive again.
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("unresolved") && !t.contains("recovered"),
        "pass 2 is still unresolved: {t}"
    );
    assert!(
        e.pending_json().contains("grok"),
        "the pending record is retained after pass 2: {}",
        e.pending_json()
    );

    // pass 3: confirm it still stays until a smoke actually passes.
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(
        e.pending_json().contains("grok"),
        "the pending record is retained after pass 3: {}",
        e.pending_json()
    );
}

#[test]
fn a_pending_recovery_whose_smoke_passes_clears_the_record() {
    // (F11-1) The positive counterpart: once the installed binary smokes clean, the retained pending
    // record is cleared (reported recovered/verified).
    let e = setup("pend-recover-ok");
    e.write_pending(
        r#"[{"engine":"grok","previous":"1.0.77","target":"1.0.80","when":"2026-11-15T00:00:00Z"}]"#,
    );
    // installed is 1.0.80 and now smokes clean.
    let env = e.grok_env("1.0.80", "1.0.80", "grok::grok-4.7=pass");
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("verified after an interrupted pass"),
        "a passing recovery smoke clears the record: {t}"
    );
    assert_eq!(
        e.pending_json().trim(),
        "[]",
        "the record is cleared on a passing recovery smoke"
    );
}

// --------------------------------------- (F57-3) a recovery never credits a smoke to a version that did not run

fn grok_state(e: &Env) -> serde_json::Value {
    let state: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(e.state_path()).unwrap()).unwrap();
    state["engines"][0].clone()
}

/// (F57-3, the reviewer's RC2 second half) previous, target and installed are three distinct
/// versions (1.0.77 -> 1.0.80 was pending, 1.0.79 is installed): a passing smoke is recorded for
/// 1.0.79 - the action and the state name it - and the pending target is NOT verified; a failing
/// smoke of the third version names it too and rolls back to the recorded previous version.
#[test]
fn a_pending_recovery_never_credits_a_third_installed_version_to_the_target() {
    let e = setup("pend-third");
    e.write_pending(
        r#"[{"engine":"grok","previous":"1.0.77","target":"1.0.80","when":"2026-11-15T00:00:00Z"}]"#,
    );
    let env = e.grok_env("1.0.79", "1.0.80", "grok::grok-4.7=pass");
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("grok 1.0.79 - pending update to 1.0.80 not verified: 1.0.79 is installed instead (changed since the update was recorded); the smoke passed on 1.0.79"),
        "{t}"
    );
    assert!(!t.contains("update to 1.0.80 verified"), "{t}");
    assert!(!t.contains("recovered"), "{t}");
    let rec = grok_state(&e);
    assert_eq!(rec["installed"], "1.0.79", "{rec}");
    let action = rec["action"].as_str().unwrap_or_default();
    assert!(
        action.starts_with("pending update to 1.0.80 not verified: 1.0.79 is installed instead")
            && action.ends_with("the smoke passed on 1.0.79"),
        "{rec}"
    );
    // the record is cleared, the truth stays in the state; nothing was installed or rolled back
    assert_eq!(e.pending_json().trim(), "[]", "{}", e.pending_json());
    assert_eq!(verfile(&e.grok_verfile), "1.0.79");
    assert!(!e.mnt_log().contains("update --version"), "{}", e.mnt_log());

    // the third version FAILS its smoke: named in the clause, rolled back to the previous version
    let e = setup("pend-third-fail");
    e.write_pending(
        r#"[{"engine":"grok","previous":"1.0.77","target":"1.0.80","when":"2026-11-15T00:00:00Z"}]"#,
    );
    let env = e.grok_env("1.0.79", "1.0.80", "grok::grok-4.7=fail");
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("grok 1.0.79 - recovered (1.0.79 is installed, not the pending target 1.0.80): smoke failed, rolled back to 1.0.77"),
        "{t}"
    );
    assert!(!t.contains("verified"), "{t}");
    assert_eq!(verfile(&e.grok_verfile), "1.0.77");
    assert_eq!(e.pending_json().trim(), "[]", "{}", e.pending_json());
}

/// (F62-1, the reviewer's RC1) The third-version fixture with an installed PRE-RELEASE of the
/// target's numbers: 1.0.77 -> 1.0.80 was pending, 1.0.80-rc.1 is installed. The pre-release is a
/// third version, not the target: a passing smoke is recorded for 1.0.80-rc.1 - the action and the
/// state name it exactly, never the bare 1.0.80 - the target 1.0.80 is NOT verified, and the record
/// is cleared as the third-version case clears it; a failing smoke names the pre-release and rolls
/// back to the recorded previous version.
#[test]
fn a_pending_recovery_never_credits_a_prerelease_smoke_to_the_final_release() {
    let e = setup("pend-rc");
    e.write_pending(
        r#"[{"engine":"grok","previous":"1.0.77","target":"1.0.80","when":"2026-11-15T00:00:00Z"}]"#,
    );
    let env = e.grok_env("1.0.80-rc.1", "1.0.80", "grok::grok-4.7=pass");
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("grok 1.0.80-rc.1 - pending update to 1.0.80 not verified: 1.0.80-rc.1 is installed instead (changed since the update was recorded); the smoke passed on 1.0.80-rc.1"),
        "{t}"
    );
    assert!(!t.contains("update to 1.0.80 verified"), "{t}");
    assert!(!t.contains("recovered"), "{t}");
    let rec = grok_state(&e);
    assert_eq!(rec["installed"], "1.0.80-rc.1", "{rec}");
    let action = rec["action"].as_str().unwrap_or_default();
    assert!(
        action
            .starts_with("pending update to 1.0.80 not verified: 1.0.80-rc.1 is installed instead")
            && action.ends_with("the smoke passed on 1.0.80-rc.1"),
        "{rec}"
    );
    // the record is cleared as in the third-version case; nothing was installed or rolled back
    assert_eq!(e.pending_json().trim(), "[]", "{}", e.pending_json());
    assert_eq!(verfile(&e.grok_verfile), "1.0.80-rc.1");
    assert!(!e.mnt_log().contains("update --version"), "{}", e.mnt_log());

    // the pre-release FAILS its smoke: named exactly, rolled back to the previous version
    let e = setup("pend-rc-fail");
    e.write_pending(
        r#"[{"engine":"grok","previous":"1.0.77","target":"1.0.80","when":"2026-11-15T00:00:00Z"}]"#,
    );
    let env = e.grok_env("1.0.80-rc.1", "1.0.80", "grok::grok-4.7=fail");
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("grok 1.0.80-rc.1 - recovered (1.0.80-rc.1 is installed, not the pending target 1.0.80): smoke failed, rolled back to 1.0.77"),
        "{t}"
    );
    assert!(!t.contains("verified"), "{t}");
    assert_eq!(verfile(&e.grok_verfile), "1.0.77");
    assert_eq!(e.pending_json().trim(), "[]", "{}", e.pending_json());
}

/// (F62-1) The mirror of RC1: a PRE-RELEASE target (1.0.77 -> 1.0.80-rc.1 was pending) and the final
/// release 1.0.80 installed. The final release is not the pending target either: the smoke passes
/// on 1.0.80, which the action and the state name, and the target 1.0.80-rc.1 is NOT verified.
#[test]
fn a_pending_recovery_never_credits_a_final_release_smoke_to_a_prerelease_target() {
    let e = setup("pend-rc-target");
    e.write_pending(
        r#"[{"engine":"grok","previous":"1.0.77","target":"1.0.80-rc.1","when":"2026-11-15T00:00:00Z"}]"#,
    );
    let env = e.grok_env("1.0.80", "1.0.80-rc.1", "grok::grok-4.7=pass");
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("grok 1.0.80 - pending update to 1.0.80-rc.1 not verified: 1.0.80 is installed instead (changed since the update was recorded); the smoke passed on 1.0.80"),
        "{t}"
    );
    assert!(!t.contains("update to 1.0.80-rc.1 verified"), "{t}");
    assert!(!t.contains("recovered"), "{t}");
    let rec = grok_state(&e);
    assert_eq!(rec["installed"], "1.0.80", "{rec}");
    let action = rec["action"].as_str().unwrap_or_default();
    assert!(
        action
            .starts_with("pending update to 1.0.80-rc.1 not verified: 1.0.80 is installed instead")
            && action.ends_with("the smoke passed on 1.0.80"),
        "{rec}"
    );
    assert_eq!(e.pending_json().trim(), "[]", "{}", e.pending_json());
    assert_eq!(verfile(&e.grok_verfile), "1.0.80");
    assert!(!e.mnt_log().contains("update --version"), "{}", e.mnt_log());
}

/// (F62-1, outside the recovery) The update decision orders a pre-release BEFORE its final release
/// and pins the exact release: an installed 1.0.80-rc.1 is not "up to date" with the vendor's
/// 1.0.80 (the numbers alone called it so), the update pins exactly 1.0.80, and the report and the
/// state name 1.0.80-rc.1 as the release it came from.
#[test]
fn an_installed_prerelease_is_updated_to_its_final_release() {
    let e = setup("rc-update");
    let env = e.grok_env("1.0.80-rc.1", "1.0.80", "grok::grok-4.7=pass");
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(!t.contains("up to date"), "{t}");
    assert!(t.contains("grok 1.0.80-rc.1 -> 1.0.80 ok"), "{t}");
    assert!(
        e.mnt_log().contains("update --version 1.0.80"),
        "{}",
        e.mnt_log()
    );
    assert_eq!(verfile(&e.grok_verfile), "1.0.80");
    let rec = grok_state(&e);
    assert_eq!(rec["installed"], "1.0.80-rc.1", "{rec}");
    assert_eq!(rec["action"], "1.0.80-rc.1 -> 1.0.80 ok", "{rec}");
    assert_eq!(e.pending_json().trim(), "[]", "{}", e.pending_json());
}

/// (F65-2, F65-4, the review's RC3) The updater, asked for exactly 1.0.80, leaves 1.0.80-rc.1 - a
/// release of the target's numbers, not the target. It is smoked in the SAME pass and credited to
/// itself (the recovery's third-version path): the action names 1.0.80-rc.1 and says the target is
/// not verified, the record is cleared with that truth in the state, nothing is rolled back.
#[test]
fn an_update_that_leaves_a_prerelease_of_the_target_smokes_it_for_itself() {
    let e = setup("upd-left-rc");
    let mut env = e.grok_env("1.0.77", "1.0.80", "grok::grok-4.7=pass");
    env.push(("FAKE_GROK_UPDATE_WRITES", "1.0.80-rc.1"));
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("grok 1.0.77 -> 1.0.80 left 1.0.80-rc.1 installed instead of the target; 1.0.80-rc.1 is smoked for itself"),
        "{t}"
    );
    assert!(
        t.contains("grok 1.0.80-rc.1 - pending update to 1.0.80 not verified: 1.0.80-rc.1 is installed instead (changed since the update was recorded); the smoke passed on 1.0.80-rc.1"),
        "{t}"
    );
    assert!(!t.contains("did not take"), "{t}");
    assert!(!t.contains("-> 1.0.80 ok"), "{t}");
    assert!(!t.contains("-> 1.0.80-rc.1 ok"), "{t}");
    assert!(!t.contains("update to 1.0.80 verified"), "{t}");
    let rec = grok_state(&e);
    // `installed` is the version measured at the start of the engine's turn, as after every update
    // outcome; the action names the release the updater left
    assert_eq!(rec["installed"], "1.0.77", "{rec}");
    let action = rec["action"].as_str().unwrap_or_default();
    assert!(
        action
            .starts_with("pending update to 1.0.80 not verified: 1.0.80-rc.1 is installed instead")
            && action.ends_with("the smoke passed on 1.0.80-rc.1"),
        "{rec}"
    );
    assert_eq!(e.pending_json().trim(), "[]", "{}", e.pending_json());
    assert_eq!(verfile(&e.grok_verfile), "1.0.80-rc.1");
    let log = e.mnt_log();
    assert!(log.contains("update --version 1.0.80"), "{log}");
    assert!(
        !log.contains("update --version 1.0.77"),
        "no rollback: {log}"
    );
}

/// (F65-2, F65-4, RC3) The same updater, and the release it left FAILS its smoke: rolled back to
/// the previous release in the same pass, and the line says so, naming the release that failed.
#[test]
fn an_update_that_leaves_a_failing_prerelease_rolls_it_back() {
    let e = setup("upd-left-rc-fail");
    let mut env = e.grok_env("1.0.77", "1.0.80", "grok::grok-4.7=pass-then-fail");
    env.push(("FAKE_GROK_UPDATE_WRITES", "1.0.80-rc.1"));
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("grok 1.0.80-rc.1 - recovered (1.0.80-rc.1 is installed, not the pending target 1.0.80): smoke failed, rolled back to 1.0.77"),
        "{t}"
    );
    assert!(!t.contains("verified"), "{t}");
    assert!(!t.contains("did not take"), "{t}");
    let rec = grok_state(&e);
    assert!(
        rec["action"].as_str().unwrap_or_default().starts_with(
            "recovered (1.0.80-rc.1 is installed, not the pending target 1.0.80): smoke failed, rolled back to 1.0.77"
        ),
        "{rec}"
    );
    assert_eq!(verfile(&e.grok_verfile), "1.0.77");
    assert_eq!(e.pending_json().trim(), "[]", "{}", e.pending_json());
    assert!(
        e.mnt_log().contains("update --version 1.0.77"),
        "{}",
        e.mnt_log()
    );
}

/// (F65-4) A rollback that restores a release of the previous version's numbers (1.0.77-rc.1 for
/// 1.0.77, the command exiting 0) has not restored the previous release: reported failed, the
/// pending record kept.
#[test]
fn a_rollback_to_a_same_numbers_release_is_not_a_restore() {
    let e = setup("rb-same-numbers");
    let mut env = e.grok_env("1.0.77", "1.0.80", "grok::grok-4.7=pass-then-fail");
    env.push(("FAKE_GROK_ROLLBACK_WRITES", "1.0.77-rc.1"));
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("grok 1.0.77 -> 1.0.80 smoke failed, rollback failed, still 1.0.77-rc.1"),
        "{t}"
    );
    assert!(!t.contains("rolled back"), "{t}");
    let rec = grok_state(&e);
    assert_eq!(
        rec["action"], "smoke failed, rollback failed, still 1.0.77-rc.1",
        "{rec}"
    );
    assert_eq!(verfile(&e.grok_verfile), "1.0.77-rc.1");
    let pending = e.pending_json();
    assert!(
        pending.contains("\"engine\":\"grok\"") && pending.contains("\"previous\":\"1.0.77\""),
        "the record is kept: {pending}"
    );
}

/// (F65-2) An update after which the installed version cannot be read smokes nothing (its result
/// could not be attributed to a release, the recovery's rule too): "did not take (still unknown)",
/// no rollback, the record kept for the next pass.
#[test]
fn an_update_that_leaves_an_unreadable_version_smokes_nothing_and_keeps_the_record() {
    let e = setup("upd-left-unknown");
    // a second smoke would fail (and roll back) - it must not run
    let mut env = e.grok_env("1.0.77", "1.0.80", "grok::grok-4.7=pass-then-fail");
    env.push(("FAKE_GROK_UPDATE_WRITES", "nightly"));
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("grok 1.0.77 -> 1.0.80 update did not take (still unknown)"),
        "{t}"
    );
    assert!(!t.contains("smoked for itself"), "{t}");
    assert_eq!(verfile(&e.grok_verfile), "nightly");
    assert!(
        !e.mnt_log().contains("update --version 1.0.77"),
        "no rollback: {}",
        e.mnt_log()
    );
    let pending = e.pending_json();
    assert!(
        pending.contains("\"engine\":\"grok\"") && pending.contains("\"target\":\"1.0.80\""),
        "the record is kept: {pending}"
    );
}

/// (F65-4) codex's release date - the key of the cool-down - is read under the EXACT release npm
/// announced: a fresh `0.170.0-alpha.1` is held by its own date (the bare numbers `0.170.0` have no
/// entry, so a lookup under them found no date and held nothing).
#[test]
fn a_prerelease_cool_down_reads_the_prereleases_own_date() {
    let e = setup("held-rc");
    let codex_exe = e.codex_exe.to_string_lossy().into_owned();
    let cverfile = e.codex_verfile.to_string_lossy().into_owned();
    let env = vec![
        ("CODEX_CONSULT_EXE", codex_exe.as_str()),
        ("FAKE_CODEX_VERFILE", cverfile.as_str()),
        ("FAKE_CODEX_VER", "0.169.0"),
        ("FAKE_NPM_LATEST", "0.170.0-alpha.1"),
        ("FAKE_NPM_DATE", "2020-01-01T00:00:00Z"),
        ("C3_MAINTAIN_MIN_AGE_DAYS", "100000"),
    ];
    let o = e.run(&["maintain", "--run", "--engine", "codex"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("codex 0.169.0 -> 0.170.0-alpha.1 held: released"),
        "the pre-release is held by its own date: {t}"
    );
    assert!(
        !e.codex_verfile.exists(),
        "nothing was installed (no install call wrote the version file)"
    );
}

/// (F57-3) An installed version that cannot be read verifies nothing: no smoke is run, the action
/// and the state say the pending update to the target is not verified, and the record is kept.
#[test]
fn a_pending_recovery_with_an_unreadable_installed_version_verifies_nothing() {
    let e = setup("pend-unknown");
    e.write_pending(
        r#"[{"engine":"grok","previous":"1.0.77","target":"1.0.80","when":"2026-11-15T00:00:00Z"}]"#,
    );
    // the fake prints "grok nightly" -> no version token -> the installed version is unknown
    let env = e.grok_env("nightly", "1.0.80", "grok::grok-4.7=pass");
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("grok unknown - pending update to 1.0.80 unresolved: the installed version cannot be read, so the update is not verified (no smoke run)"),
        "{t}"
    );
    assert!(!t.contains("verified after"), "{t}");
    assert!(!t.contains("recovered"), "{t}");
    let rec = grok_state(&e);
    assert_eq!(rec["installed"], "unknown", "{rec}");
    assert!(
        rec["action"].as_str().unwrap_or_default().starts_with(
            "pending update to 1.0.80 unresolved: the installed version cannot be read"
        ),
        "{rec}"
    );
    let pending = e.pending_json();
    assert!(
        pending.contains("\"engine\":\"grok\"") && pending.contains("\"target\":\"1.0.80\""),
        "the record is kept: {pending}"
    );
    assert_eq!(verfile(&e.grok_verfile), "nightly", "nothing was installed");
    assert!(!e.mnt_log().contains("update --version"), "{}", e.mnt_log());
}

// ------------------------------------------------------------------ (R2) a pending record that cannot be persisted holds the install

#[test]
fn a_pending_record_that_cannot_be_persisted_holds_the_install() {
    // (R2) if the pending journal cannot be written (here pending.json.tmp is pre-created as a
    // DIRECTORY so the atomic write fails), the write-before-install guarantee is void: the pass must
    // hold and run no installer (fail closed), not install with no recovery record on disk.
    let e = setup("pend-unwritable");
    let maintain_dir = e.home.join("c3").join("maintain");
    std::fs::create_dir_all(&maintain_dir).unwrap();
    std::fs::create_dir_all(maintain_dir.join("pending.json.tmp")).unwrap();
    let env = e.grok_env("1.0.77", "1.0.80", "grok::grok-4.7=pass");
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("could not record the pending update"),
        "the install is held when the journal cannot be written: {t}"
    );
    assert_eq!(
        verfile(&e.grok_verfile),
        "1.0.77",
        "no installer ran (the old version stands)"
    );
}

// ------------------------------------------------------------------ the model step honours --engine

#[test]
fn the_model_step_honours_the_engine_filter() {
    let e = setup("model-scope");
    // a roster with a swappable grok seat (grok-4.6 -> grok-4.7 from the fake `grok models`)
    let roster = e.work.join("roster.json");
    std::fs::write(
        &roster,
        r#"{"roster_version":1,"reviewers":[{"provider":"grok","engine":"grok","model":"grok-4.6","plan":"supergrok"}]}"#,
    )
    .unwrap();
    let roster_s = roster.to_string_lossy().into_owned();
    // grok is up to date so the engine step is quiet; the model step is what we are checking. The
    // codex launcher is pinned to the fake so a codex-scoped pass never reaches a real codex on PATH.
    let grok_exe = e.grok_exe.to_string_lossy().into_owned();
    let codex_exe = e.codex_exe.to_string_lossy().into_owned();
    std::fs::write(&e.grok_verfile, "1.0.77").unwrap();
    let grok_vf = e.grok_verfile.to_string_lossy().into_owned();
    let cvf = e.codex_verfile.to_string_lossy().into_owned();
    std::fs::write(&e.codex_verfile, "0.1.0").unwrap();
    let common: Vec<(&str, &str)> = vec![
        ("CODEX_CONSULT_GROK_EXE", grok_exe.as_str()),
        ("CODEX_CONSULT_EXE", codex_exe.as_str()),
        ("CODEX_CONSULT_ROSTER", roster_s.as_str()),
        ("FAKE_GROK_VERFILE", grok_vf.as_str()),
        ("FAKE_GROK_VER", "1.0.77"),
        ("FAKE_GROK_LATEST", "1.0.77"),
        ("FAKE_CODEX_VERFILE", cvf.as_str()),
        ("FAKE_CODEX_VER", "0.1.0"),
        ("C3_MAINTAIN_MODELS", "swap"),
    ];

    // scoped to codex: the grok seat's candidate must NOT appear
    let o = e.run(
        &["maintain", "--run", "--dry-run", "--engine", "codex"],
        &common,
    );
    let t = text(&o);
    assert!(
        !t.contains("grok-4.6 -> grok-4.7"),
        "a codex-scoped pass does not touch the grok seat: {t}"
    );

    // scoped to grok: the candidate appears
    let o = e.run(
        &["maintain", "--run", "--dry-run", "--engine", "grok"],
        &common,
    );
    let t = text(&o);
    assert!(
        t.contains("grok-4.6 -> grok-4.7"),
        "a grok-scoped pass surfaces the grok seat's candidate: {t}"
    );
}

// ------------------------------------------------------------------ the public catalog source (--models)

const CATALOG_URL: &str = "https://openrouter.ai/api/v1/models";

/// An Anthropic-compatible endpoint entry whose endpoint gives no `/v1/models` list (its key
/// variable is unset, so the list is never even requested) - the Alibaba case of 2026-11-18.
fn endpoint_roster(protected: bool) -> String {
    let ext = if protected {
        r#","ext":{"reserve_percent":15}"#
    } else {
        ""
    };
    format!(
        r#"{{"roster_version":1,"reviewers":[{{"provider":"alibaba-claude","engine":"claude","model":"qwen3.8-max","auth":"endpoint","endpoint":{{"base_url":"https://alibaba.example/apps/anthropic","env_key":"C3_TEST_NO_SUCH_KEY"}}{ext}}}]}}"#
    )
}

/// A catalog offering the successor `qwen4-max` (plus a `:free` variant, which is skipped) and a new
/// family member `qwen4-coder`, both created a day ago.
fn catalog_json() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let c = now - 86_400;
    format!(
        r#"{{"data":[{{"id":"qwen/qwen4-max","created":{c}}},{{"id":"qwen/qwen4-max:free","created":{c}}},{{"id":"qwen/qwen4-coder","created":{c}}},{{"id":"openai/gpt-oss-9","created":{c}}}]}}"#
    )
}

struct ModelsCase {
    roster: PathBuf,
    http_log: PathBuf,
    owned: Vec<(String, String)>,
}

impl ModelsCase {
    fn env(&self) -> Vec<(&str, &str)> {
        self.owned
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect()
    }
    fn roster_text(&self) -> String {
        std::fs::read_to_string(&self.roster).unwrap()
    }
    fn backups(&self) -> Vec<String> {
        std::fs::read_dir(self.roster.parent().unwrap())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("roster.json.bak-maintain-"))
            .collect()
    }
}

/// A `--models` case: the roster, the catalog served from a file in place of the network, the fetch
/// log, a scripted smoke, and the fake grok/codex wired so any engine probe would be recorded.
fn models_case(e: &Env, roster_json: &str, smoke: &str) -> ModelsCase {
    models_case_with(e, roster_json, smoke, &catalog_json())
}

fn models_case_with(e: &Env, roster_json: &str, smoke: &str, catalog_body: &str) -> ModelsCase {
    let roster = e.work.join("roster.json");
    std::fs::write(&roster, roster_json).unwrap();
    let catalog = e.work.join("catalog.json");
    std::fs::write(&catalog, catalog_body).unwrap();
    let http_log = e.work.join("http.log");
    std::fs::write(&e.grok_verfile, "1.0.77").unwrap();
    let s = |p: &Path| p.to_string_lossy().into_owned();
    let owned: Vec<(String, String)> = vec![
        ("CODEX_CONSULT_ROSTER".into(), s(&roster)),
        ("C3_MAINTAIN_MODELS".into(), "swap".into()),
        (
            "C3_MAINTAIN_TEST_HTTP".into(),
            format!("{CATALOG_URL}|{}", s(&catalog)),
        ),
        ("C3_MAINTAIN_TEST_HTTP_LOG".into(), s(&http_log)),
        ("C3_MAINTAIN_TEST_SMOKE".into(), smoke.into()),
        ("CODEX_CONSULT_GROK_EXE".into(), s(&e.grok_exe)),
        ("CODEX_CONSULT_EXE".into(), s(&e.codex_exe)),
        ("FAKE_GROK_VERFILE".into(), s(&e.grok_verfile)),
        ("FAKE_GROK_VER".into(), "1.0.77".into()),
        ("FAKE_GROK_LATEST".into(), "1.0.80".into()),
        ("FAKE_MNT_LOG".into(), s(&e.work.join("fake.log"))),
    ];
    ModelsCase {
        roster,
        http_log,
        owned,
    }
}

const SCHEDULED: &str = r#"{"last_pass":"2026-11-10T00:00:00Z","next_due":"2026-11-17T00:00:00Z","reported":true,"engines":[{"name":"grok","installed":"1.0.77","action":"up to date","when":"2026-11-10T00:00:00Z"}]}"#;

/// The default catalog configured with a query that holds a token (the GET is keyless, but an
/// operator may write one): the source key keeps it, nothing a person or an agent reads shows it.
const QUERY_CATALOG_URL: &str = "https://openrouter.ai/api/v1/models?key=sk-test-QUERYTOKEN";

#[test]
fn a_catalog_successor_that_passes_its_smoke_is_seated_without_moving_the_schedule() {
    let e = setup("web-seat");
    e.write_state(SCHEDULED);
    let mut m = models_case(
        &e,
        &endpoint_roster(false),
        "alibaba-claude::qwen4-max=pass",
    );
    // the catalog is configured WITH a query, served from the same file
    std::fs::write(
        e.home.join("c3").join("config.json"),
        format!(r#"{{"config_version":1,"model_catalogs":["{QUERY_CATALOG_URL}"]}}"#),
    )
    .unwrap();
    for (k, v) in m.owned.iter_mut() {
        if k == "C3_MAINTAIN_TEST_HTTP" {
            *v = format!(
                "{QUERY_CATALOG_URL}|{}",
                e.work.join("catalog.json").display()
            );
        }
    }
    let o = e.run(&["maintain", "--models"], &m.env());
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    // the collected line names the catalog without its query
    assert!(
        t.contains(
            "c3 maintain: web:https://openrouter.ai/api/v1/models (query not shown) - 3 model id(s) collected"
        ),
        "{t}"
    );
    assert!(
        !t.contains("QUERYTOKEN") && !t.contains("key="),
        "no query text is printed: {t}"
    );
    assert!(
        t.contains("model alibaba-claude :: qwen3.8-max -> qwen4-max, smoke passed"),
        "the successor from the catalog is seated: {t}"
    );
    assert!(
        t.contains("new in the family: qwen4-coder"),
        "a new family member is announced: {t}"
    );
    assert!(!t.contains("qwen4-max:free"), "a :variant is skipped: {t}");
    // the TEST roster: only that entry's model edited, a backup taken first
    let roster = m.roster_text();
    assert!(roster.contains("\"model\":\"qwen4-max\""), "{roster}");
    assert_eq!(
        roster,
        endpoint_roster(false).replace("qwen3.8-max", "qwen4-max")
    );
    let backups = m.backups();
    assert_eq!(backups.len(), 1, "{backups:?}");
    let backup = std::fs::read_to_string(e.work.join(&backups[0])).unwrap();
    assert_eq!(
        backup,
        endpoint_roster(false),
        "the backup holds the old roster"
    );
    // --models touched no CLI and did not move the schedule or the engine records
    assert!(
        e.mnt_log().is_empty(),
        "no engine CLI was run: {}",
        e.mnt_log()
    );
    let state = std::fs::read_to_string(e.state_path()).unwrap();
    assert!(
        state.contains("\"next_due\": \"2026-11-17T00:00:00Z\""),
        "{state}"
    );
    assert!(
        state.contains("\"last_pass\": \"2026-11-10T00:00:00Z\""),
        "{state}"
    );
    assert!(state.contains("\"action\": \"up to date\""), "{state}");
    assert!(
        state.contains("\"web:https://openrouter.ai:443/api/v1/models?key=sk-test-QUERYTOKEN\""),
        "the catalog is recorded under its FULL URL (the key keeps the query): {state}"
    );
    // the stored report names it, and the full notes are printed by `c3 maintain`
    let r = text(&e.run(&["maintain"], &m.env()));
    assert!(r.contains("qwen3.8-max -> qwen4-max, smoke passed"), "{r}");
    assert!(
        r.contains("new in the family of qwen3.8-max (alibaba-claude): qwen4-coder"),
        "{r}"
    );
    // the next SessionStart hook prints the line once
    let h = text(&e.run(&["hook"], &m.env()));
    assert!(h.contains("qwen3.8-max -> qwen4-max, smoke passed"), "{h}");
    for (what, out) in [("the report", &r), ("the hook line", &h)] {
        assert!(
            !out.contains("QUERYTOKEN") && !out.contains("key="),
            "{what} shows no query text: {out}"
        );
    }
}

#[test]
fn a_catalog_successor_whose_smoke_fails_is_reported_not_seated() {
    let e = setup("web-fail");
    let m = models_case(
        &e,
        &endpoint_roster(false),
        "alibaba-claude::qwen4-max=fail",
    );
    let o = e.run(&["maintain", "--models"], &m.env());
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("qwen3.8-max -> qwen4-max not seated (smoke failed: seen in the catalog, not served by this plan)"),
        "the failed candidate is reported: {t}"
    );
    assert_eq!(
        m.roster_text(),
        endpoint_roster(false),
        "the roster is untouched"
    );
    assert!(m.backups().is_empty(), "no backup without a swap");
}

#[test]
fn a_catalog_id_older_than_the_seated_model_by_date_is_not_smoked_or_seated() {
    // the live grok-4.20 case: numerically above the seated id, but the catalog dates it before
    // the seated one - the smoke would pass on a plan that still serves it, so it must never run
    let e = setup("web-older");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let catalog = format!(
        r#"{{"data":[{{"id":"qwen/qwen3.8-max","created":{}}},{{"id":"qwen/qwen4-max","created":{}}}]}}"#,
        now - 10 * 86_400,
        now - 300 * 86_400
    );
    let m = models_case_with(
        &e,
        &endpoint_roster(false),
        "alibaba-claude::qwen4-max=pass",
        &catalog,
    );
    let o = e.run(&["maintain", "--models"], &m.env());
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("qwen3.8-max -> qwen4-max not a candidate (older than the seated model by the catalog's date)"),
        "the older id is reported with the reason: {t}"
    );
    assert!(!t.contains("smoke passed"), "no smoke ran: {t}");
    assert!(!t.contains("would smoke"), "{t}");
    let summary = t
        .lines()
        .find(|l| l.starts_with("c3: maintenance"))
        .unwrap_or_default();
    assert!(
        !summary.contains("qwen4-max"),
        "the rejected id stays out of the one-line summary: {summary}"
    );
    assert_eq!(
        m.roster_text(),
        endpoint_roster(false),
        "the roster is untouched"
    );
    assert!(m.backups().is_empty());
}

#[test]
fn a_protected_entry_is_announced_only() {
    let e = setup("web-protected");
    // the smoke would pass - but a protected entry is never smoked or edited
    let m = models_case(&e, &endpoint_roster(true), "alibaba-claude::qwen4-max=pass");
    let o = e.run(&["maintain", "--models"], &m.env());
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("qwen3.8-max -> qwen4-max not seated (protected entry)"),
        "the protected entry's successor is announced: {t}"
    );
    assert!(!t.contains("smoke passed"), "{t}");
    assert_eq!(
        m.roster_text(),
        endpoint_roster(true),
        "the roster is untouched"
    );
    assert!(m.backups().is_empty());
}

#[test]
fn model_web_off_fetches_nothing() {
    let e = setup("web-off");
    let m = models_case(
        &e,
        &endpoint_roster(false),
        "alibaba-claude::qwen4-max=pass",
    );
    let mut env = m.env();
    env.push(("C3_MODEL_WEB", "off"));
    let o = e.run(&["maintain", "--models"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(!m.http_log.exists(), "no public source was requested");
    assert!(!t.contains("qwen4-max"), "no catalog candidate: {t}");
    assert_eq!(m.roster_text(), endpoint_roster(false));
    // and C3_MAINTAIN_MODELS=off on a full pass fetches nothing either
    let mut env = m.env();
    env.retain(|(k, _)| *k != "C3_MAINTAIN_MODELS");
    env.push(("C3_MAINTAIN_MODELS", "off"));
    env.push(("C3_MAINTAIN_SKIP", "codex,grok,kimi,agy,muse,claude"));
    let o = e.run(&["maintain", "--run", "--dry-run"], &env);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(!m.http_log.exists(), "the model step off: no fetch");
}

#[test]
fn models_dry_run_edits_nothing() {
    let e = setup("web-dry");
    let m = models_case(
        &e,
        &endpoint_roster(false),
        "alibaba-claude::qwen4-max=pass",
    );
    let o = e.run(&["maintain", "--models", "--dry-run"], &m.env());
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("qwen3.8-max -> qwen4-max (dry-run: would smoke and seat)"),
        "the dry run says what it would do: {t}"
    );
    assert!(t.contains("new in the family: qwen4-coder"), "{t}");
    assert!(m.http_log.exists(), "the dry run still collects");
    assert_eq!(
        m.roster_text(),
        endpoint_roster(false),
        "the roster is untouched"
    );
    assert!(m.backups().is_empty());
    assert!(!e.state_path().exists(), "a dry run writes no state");
    assert!(e.mnt_log().is_empty(), "no engine CLI was run");
}

/// (F02-18) `C3_MAINTAIN_MODELS=off` is authoritative: an explicit `--models` (dry run or not)
/// fetches nothing - no public source, no endpoint list, no engine list - writes no state, edits
/// nothing, and says why.
#[test]
fn models_off_is_authoritative_for_an_explicit_models_run() {
    let e = setup("web-models-off");
    let m = models_case(
        &e,
        &endpoint_roster(false),
        "alibaba-claude::qwen4-max=pass",
    );
    let mut env = m.env();
    env.retain(|(k, _)| *k != "C3_MAINTAIN_MODELS");
    env.push(("C3_MAINTAIN_MODELS", "off"));
    for args in [
        vec!["maintain", "--models"],
        vec!["maintain", "--models", "--dry-run"],
    ] {
        let o = e.run(&args, &env);
        let t = text(&o);
        assert_eq!(o.status.code(), Some(0), "{t}");
        assert!(
            t.contains("the model step is off (C3_MAINTAIN_MODELS=off) - --models fetched nothing"),
            "{t}"
        );
        assert!(!m.http_log.exists(), "no public source was requested: {t}");
        assert!(e.mnt_log().is_empty(), "no engine list was read: {t}");
        assert!(!t.contains("qwen4-max"), "{t}");
    }
    assert!(!e.state_path().exists(), "nothing was written");
    // (F15-6) not even the pass lock or the state directory: the switch is read before both
    let maintain_dir = e.home.join("c3").join("maintain");
    assert!(
        !maintain_dir.exists(),
        "the maintain state directory was not created"
    );
    // an existing directory with a lock another pass left (unreadable, so a lock taker would
    // remove it as stale) is not touched either
    std::fs::create_dir_all(&maintain_dir).unwrap();
    std::fs::write(maintain_dir.join("lock"), "not a lock record").unwrap();
    let o = e.run(&["maintain", "--models"], &env);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert_eq!(
        std::fs::read_to_string(maintain_dir.join("lock")).unwrap(),
        "not a lock record",
        "the lock was left as it was"
    );
    let names: Vec<String> = std::fs::read_dir(&maintain_dir)
        .unwrap()
        .flatten()
        .map(|d| d.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["lock".to_string()], "no other file appeared");
    assert_eq!(m.roster_text(), endpoint_roster(false));
}

/// (F04-3) A candidate a catalog once dated older than the seated id is recorded in the state and
/// stays rejected in a later pass, even when the catalog then re-dates it after the seated id.
#[test]
fn a_candidate_dated_older_once_stays_rejected_in_a_later_pass() {
    let e = setup("web-older-recorded");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let older = format!(
        r#"{{"data":[{{"id":"qwen/qwen3.8-max","created":{}}},{{"id":"qwen/qwen4-max","created":{}}}]}}"#,
        now - 10 * 86_400,
        now - 300 * 86_400
    );
    let m = models_case_with(
        &e,
        &endpoint_roster(false),
        "alibaba-claude::qwen4-max=pass",
        &older,
    );
    let t = text(&e.run(&["maintain", "--models"], &m.env()));
    assert!(
        t.contains("not a candidate (older than the seated model by the catalog's date)"),
        "{t}"
    );
    let state = std::fs::read_to_string(e.state_path()).unwrap();
    assert!(state.contains("\"older\""), "{state}");
    assert!(state.contains("\"candidate\": \"qwen4-max\""), "{state}");
    // the catalog now claims qwen4-max is newer than the seated model
    let restamped = format!(
        r#"{{"data":[{{"id":"qwen/qwen3.8-max","created":{}}},{{"id":"qwen/qwen4-max","created":{}}}]}}"#,
        now - 10 * 86_400,
        now - 86_400
    );
    std::fs::write(e.work.join("catalog.json"), restamped).unwrap();
    let t = text(&e.run(&["maintain", "--models"], &m.env()));
    assert!(
        t.contains("qwen3.8-max -> qwen4-max not a candidate (recorded as older than the seated model in an earlier pass)"),
        "{t}"
    );
    assert!(!t.contains("smoke passed"), "{t}");
    assert_eq!(
        m.roster_text(),
        endpoint_roster(false),
        "the roster is untouched"
    );
}

const FAKE_CLAUDE_PS1: &str = include_str!("fixtures/fake-claude.ps1");
const FAKE_CLAUDE_CMD: &str = include_str!("fixtures/fake-claude.cmd");

/// (F02-16, RC2) The swap smoke runs through the EXACT roster entry being edited, not whatever its
/// provider label resolves to. Two entries share the label `alibaba` on two stub endpoints; the
/// first is on-request (and would be the label's first match). The real two-turn smoke (no script)
/// runs through the fake claude, which records the endpoint each turn reached: both turns reach the
/// candidate entry's endpoint, the on-request entry's endpoint receives zero calls, and the swap
/// edits that same position only.
#[test]
fn a_swap_smoke_runs_through_the_exact_roster_entry_not_its_label() {
    let e = setup("bind-entry");
    std::fs::write(e.bin.join("fake-claude.ps1"), FAKE_CLAUDE_PS1).unwrap();
    let reply = e.work.join("reply.txt");
    std::fs::write(&reply, "OK").unwrap();
    let argv_log = e.work.join("claude-argv.jsonl");
    let claude_cfg = e.work.join("claude-config");
    std::fs::create_dir_all(&claude_cfg).unwrap();
    // The smoke's `c3 consult` children never see this process's CODEX_CONSULT_TEST_* variables
    // (scrubbed like host markers), so the fake's drivers cannot travel through the test hook
    // CODEX_CONSULT_TEST_CHILD_ENV_PASS: the wrapper sets them itself.
    let claude = e.bin.join("fake-claude.cmd");
    let wrapper = FAKE_CLAUDE_CMD.replace(
        "set \"FAKE_CLAUDE_ARGS=%*\"",
        &format!(
            "set \"FAKE_CLAUDE_ARGS=%*\"\nset \"FAKE_CLAUDE_ARGV_LOG={}\"\nset \"FAKE_CLAUDE_REPLY={}\"\nset \"FAKE_CLAUDE_REPLY_FROM_CWD=token.txt\"",
            argv_log.display(),
            reply.display()
        ),
    );
    assert!(wrapper.contains("FAKE_CLAUDE_REPLY_FROM_CWD"), "{wrapper}");
    std::fs::write(&claude, crlf(&wrapper)).unwrap();
    // Two stub endpoints on a closed local port: the endpoint list fetch (the keys are set, so it
    // is attempted) is refused at once and never leaves the machine.
    let roster_json = r#"{"roster_version":1,"reviewers":[{"provider":"alibaba","engine":"claude","model":"qwen3.7-max","auth":"endpoint","endpoint":{"base_url":"https://127.0.0.1:9/onrequest/anthropic","env_key":"C3T_KEY_ONREQ"},"ext":{"on_request":true}},{"provider":"alibaba","engine":"claude","model":"qwen3.8-max","auth":"endpoint","endpoint":{"base_url":"https://127.0.0.1:9/seat/anthropic","env_key":"C3T_KEY_SEAT"}}]}"#;
    // no C3_MAINTAIN_TEST_SMOKE: the real two-turn smoke runs
    let m = models_case(&e, roster_json, "");
    let s = |p: &Path| p.to_string_lossy().into_owned();
    let (claude_s, cfg_s) = (s(&claude), s(&claude_cfg));
    let mut env = m.env();
    env.extend([
        ("CODEX_CONSULT_CLAUDE_EXE", claude_s.as_str()),
        ("CLAUDE_CONFIG_DIR", cfg_s.as_str()),
        ("C3T_KEY_ONREQ", "tok-onreq"),
        ("C3T_KEY_SEAT", "tok-seat"),
    ]);
    let o = e.run(&["maintain", "--models"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("model alibaba :: qwen3.7-max -> qwen4-max not seated (on-request entry)"),
        "{t}"
    );
    // every claude turn the smoke started, by the endpoint it reached
    let turns: Vec<serde_json::Value> = std::fs::read_to_string(&argv_log)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v["kind"] == "turn")
        .collect();
    let urls: Vec<String> = turns
        .iter()
        .map(|v| v["base_url"].as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(
        urls,
        vec![
            "https://127.0.0.1:9/seat/anthropic".to_string(),
            "https://127.0.0.1:9/seat/anthropic".to_string()
        ],
        "both smoke turns reached the candidate entry's endpoint: {t}"
    );
    assert!(
        !urls.iter().any(|u| u.contains("onrequest")),
        "the on-request entry's endpoint received no call"
    );
    assert!(
        turns.iter().all(|v| v["argv"]
            .as_array()
            .is_some_and(|a| a.iter().any(|x| x == "qwen4-max"))),
        "only the model was overridden: {turns:?}"
    );
    // the smoke passed (the turn read token.txt) and the swap edited that position only
    assert!(
        t.contains("model alibaba :: qwen3.8-max -> qwen4-max, smoke passed"),
        "{t}"
    );
    assert_eq!(
        m.roster_text(),
        roster_json.replace("\"qwen3.8-max\"", "\"qwen4-max\""),
        "only reviewers[2].model changed"
    );
    // (F14-3) the smoke's one-entry roster lived inside the smoke's own scratch directory and is
    // gone with it; nothing was written at a sibling path in the shared temp directory
    let leftovers: Vec<String> = std::fs::read_dir(std::env::temp_dir())
        .unwrap()
        .flatten()
        .filter(|d| {
            let n = d.file_name().to_string_lossy().into_owned();
            n.starts_with("c3-maintain-smoke-claude-")
                && (n.ends_with(".roster.json") || d.path().join("roster.json").exists())
        })
        .map(|d| d.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

/// (F15-3) An engine-update smoke bound to a roster seat is not run when that position no longer
/// holds the provider AND the model the target was chosen with: here the seat's model is edited
/// (same provider) after the pass read the roster and before the smoke. The update is held, and
/// the fake's scripted smoke - which would pass for either model - is never asked. Without the
/// edit the same pass updates, so the binding check passes an unchanged seat.
#[test]
fn an_engine_smoke_is_not_run_when_its_roster_seat_changed() {
    let e = setup("seat-changed");
    let roster = e.work.join("roster.json");
    let roster_json = r#"{"roster_version":1,"reviewers":[{"provider":"grok","engine":"grok","model":"grok-4.7","plan":"supergrok"}]}"#;
    let roster_s = roster.to_string_lossy().into_owned();
    let run = |edit: bool| {
        std::fs::write(&roster, roster_json).unwrap();
        std::fs::write(&e.grok_verfile, "1.0.77").unwrap();
        let mut env: Vec<(&str, &str)> = vec![
            ("CODEX_CONSULT_GROK_EXE", e.grok_exe_s()),
            ("FAKE_GROK_VERFILE", e.grok_verfile_s()),
            ("FAKE_GROK_VER", "1.0.77"),
            ("FAKE_GROK_LATEST", "1.0.80"),
            ("FAKE_MNT_LOG", e.mnt_log_s()),
            ("CODEX_CONSULT_ROSTER", roster_s.as_str()),
            (
                "C3_MAINTAIN_TEST_SMOKE",
                "grok::grok-4.7=pass,grok::grok-4.6=pass",
            ),
        ];
        if edit {
            env.extend([
                ("FAKE_GROK_EDIT_ROSTER", roster_s.as_str()),
                ("FAKE_GROK_EDIT_FROM", "grok-4.7"),
                ("FAKE_GROK_EDIT_TO", "grok-4.6"),
            ]);
        }
        let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
        let t = text(&o);
        assert_eq!(o.status.code(), Some(0), "{t}");
        t
    };
    let t = run(true);
    assert!(
        t.contains("held: smoke fails on the installed version (not run: the roster entry changed: reviewers entry 1 no longer holds grok :: grok-4.7)"),
        "{t}"
    );
    assert_eq!(verfile(&e.grok_verfile), "1.0.77", "nothing was installed");
    assert!(
        !e.mnt_log().contains("update --version 1.0.80"),
        "{}",
        e.mnt_log()
    );
    assert!(
        std::fs::read_to_string(&roster)
            .unwrap()
            .contains("grok-4.6"),
        "the edit happened during the pass"
    );
    // the control: the seat unchanged, the bound smoke runs (scripted) and the update lands
    let t = run(false);
    assert!(t.contains("grok 1.0.77 -> 1.0.80 ok"), "{t}");
    assert_eq!(verfile(&e.grok_verfile), "1.0.80");
}

/// (0.2.5 D3, F15-3) The entry-bound smoke refuses ANY edit of the chosen entry, not only of its
/// provider and model: the seat's plan is edited between the two smokes of an engine update (the
/// forward update does it), so the post-update smoke - which the script would pass - is not run,
/// and the update is rolled back like a failed one.
#[test]
fn an_entry_edited_in_another_field_during_the_pass_holds_the_post_update_smoke() {
    let e = setup("seat-edited");
    let roster = e.work.join("roster.json");
    let roster_json = r#"{"roster_version":1,"reviewers":[{"provider":"grok","engine":"grok","model":"grok-4.7","plan":"supergrok"}]}"#;
    std::fs::write(&roster, roster_json).unwrap();
    let roster_s = roster.to_string_lossy().into_owned();
    let mut env = e.grok_env("1.0.77", "1.0.80", "grok::grok-4.7=pass");
    env.extend([
        ("CODEX_CONSULT_ROSTER", roster_s.as_str()),
        ("FAKE_GROK_EDIT_ROSTER", roster_s.as_str()),
        ("FAKE_GROK_EDIT_AT", "update"),
        ("FAKE_GROK_EDIT_FROM", "\"supergrok\""),
        ("FAKE_GROK_EDIT_TO", "\"supergrok-team\""),
        ("C3_MAINTAIN_SMOKE_GROK", ""),
    ]);
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("smoke failed, rolled back (not run: the roster entry changed: reviewers entry 1 is not the entry the smoke was chosen with (edited since))"),
        "{t}"
    );
    assert!(!t.contains("1.0.80 ok"), "{t}");
    assert_eq!(verfile(&e.grok_verfile), "1.0.77", "rolled back");
    assert!(
        std::fs::read_to_string(&roster)
            .unwrap()
            .contains("supergrok-team"),
        "the edit happened during the pass"
    );
}

// ------------------------------------------------------------------ the price of a swap across a major

/// (0.2.5 D3) A successor of a higher major that the catalog lists at more than twice the seated
/// id's list price is announced only - "list price N times higher - seat it by hand" - never
/// smoked or seated (the same plan may charge more for it); the prices are not stored.
#[test]
fn a_catalog_successor_across_a_major_priced_over_twice_is_announced_only() {
    let e = setup("web-price");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let catalog = format!(
        r#"{{"data":[{{"id":"qwen/qwen3.8-max","created":{},"pricing":{{"prompt":"0.000001","completion":"0.000004"}}}},{{"id":"qwen/qwen4-max","created":{},"pricing":{{"prompt":"0.000003","completion":"0.000006"}}}}]}}"#,
        now - 30 * 86_400,
        now - 86_400
    );
    let m = models_case_with(
        &e,
        &endpoint_roster(false),
        "alibaba-claude::qwen4-max=pass",
        &catalog,
    );
    let o = e.run(&["maintain", "--models"], &m.env());
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("model alibaba-claude :: qwen3.8-max -> qwen4-max not seated (list price 3 times higher - seat it by hand)"),
        "{t}"
    );
    assert!(!t.contains("smoke passed"), "no smoke ran: {t}");
    assert_eq!(
        m.roster_text(),
        endpoint_roster(false),
        "the roster is untouched"
    );
    assert!(m.backups().is_empty());
    let state = std::fs::read_to_string(e.state_path()).unwrap();
    assert!(
        !state.contains("0.000003") && !state.contains("pricing") && !state.contains("prices"),
        "no price is stored: {state}"
    );
}

// ------------------------------------------------------------------ a roster id spelled with its lab token

/// (0.2.5 D3) A roster id the plan spells with a token naming its maker (`kimi-k3.1-pro`), which
/// the catalog leaves out (`moonshotai/k3.1-pro`), is matched through the catalog's spelling: its
/// successor is dated, smoked and seated exactly like a direct match, in the plan's own spelling.
#[test]
fn a_roster_id_spelled_with_its_lab_token_is_seated_in_the_plans_spelling() {
    let e = setup("web-alias");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let roster_json = r#"{"roster_version":1,"reviewers":[{"provider":"kimi-claude","engine":"claude","model":"kimi-k3.1-pro","auth":"endpoint","endpoint":{"base_url":"https://kimi.example/anthropic","env_key":"C3_TEST_NO_SUCH_KEY"},"plan":"kimi"}]}"#;
    let catalog = format!(
        r#"{{"data":[{{"id":"moonshotai/k3.1-pro","created":{}}},{{"id":"moonshotai/k3.2-pro","created":{}}}]}}"#,
        now - 10 * 86_400,
        now - 86_400
    );
    let m = models_case_with(&e, roster_json, "kimi-claude::kimi-k3.2-pro=pass", &catalog);
    let o = e.run(&["maintain", "--models"], &m.env());
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("model kimi-claude :: kimi-k3.1-pro -> kimi-k3.2-pro, smoke passed"),
        "{t}"
    );
    assert_eq!(
        m.roster_text(),
        roster_json.replace("\"kimi-k3.1-pro\"", "\"kimi-k3.2-pro\""),
        "the plan's spelling, the prefix put back"
    );
    assert_eq!(m.backups().len(), 1);
}

// ------------------------------------------------------------------ no proxy from the environment

/// (0.2.5 D3, F29-1) A `c3 maintain` GET never goes through a proxy the environment names: with
/// `HTTPS_PROXY` / `HTTP_PROXY` / `ALL_PROXY` pointing at a dead local port, the request still
/// reaches its origin directly. Driven through an endpoint's model list (the same per-hop client a
/// public catalog uses; a public catalog itself is never fetched from a loopback address, so the
/// test cannot serve one locally).
#[test]
fn a_maintain_fetch_never_goes_through_the_environments_proxy() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    let e = setup("no-proxy");
    let origin = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = origin.local_addr().unwrap().port();
    let dead = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    origin.set_nonblocking(true).unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (acc, halt) = (accepted.clone(), stop.clone());
    let server = std::thread::spawn(move || {
        while !halt.load(Ordering::SeqCst) {
            match origin.accept() {
                Ok((sock, _)) => {
                    acc.fetch_add(1, Ordering::SeqCst);
                    drop(sock); // no TLS: the client fails after it has connected
                }
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(20)),
            }
        }
    });
    let roster = e.work.join("roster.json");
    std::fs::write(
        &roster,
        format!(
            r#"{{"roster_version":1,"reviewers":[{{"provider":"probe-claude","engine":"claude","model":"glm-5.3","auth":"endpoint","endpoint":{{"base_url":"https://127.0.0.1:{port}/anthropic","env_key":"C3T_PROBE_KEY"}}}}]}}"#
        ),
    )
    .unwrap();
    let roster_s = roster.to_string_lossy().into_owned();
    let proxy = format!("http://127.0.0.1:{dead}");
    let env: Vec<(&str, &str)> = vec![
        ("CODEX_CONSULT_ROSTER", roster_s.as_str()),
        ("C3_MAINTAIN_MODELS", "announce"),
        ("C3_MODEL_WEB", "off"),
        ("C3T_PROBE_KEY", "tok-probe"),
        ("HTTPS_PROXY", proxy.as_str()),
        ("HTTP_PROXY", proxy.as_str()),
        ("ALL_PROXY", proxy.as_str()),
    ];
    let o = e.run(&["maintain", "--models", "--dry-run"], &env);
    stop.store(true, Ordering::SeqCst);
    let _ = server.join();
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(
        accepted.load(Ordering::SeqCst) >= 1,
        "the model-list GET connected to its origin directly, not to the proxy: {}",
        text(&o)
    );
}

// ------------------------------------------------------------------ an unresolved pending update and a newer release

/// (TECH_DEBT 2026-11-16) The no-rollback-path branch: agy has no rollback (and no check), so a
/// pending record whose installed version fails its smoke stays unresolved, pass after pass, and
/// nothing is installed.
#[test]
fn a_pending_update_without_a_rollback_path_stays_unresolved() {
    let e = setup("pend-agy");
    let agy = e.bin.join("fake-mnt-agy.cmd");
    std::fs::write(&agy, crlf(&FAKE_GROK.replace("=grok", "=agy"))).unwrap();
    let agy_s = agy.to_string_lossy().into_owned();
    e.write_pending(
        r#"[{"engine":"agy","previous":"1.1.0","target":"1.2.0","when":"2026-11-15T00:00:00Z"}]"#,
    );
    let env: Vec<(&str, &str)> = vec![
        ("CODEX_CONSULT_AGY_EXE", agy_s.as_str()),
        ("FAKE_AGY_VER", "1.2.0"),
        ("FAKE_MNT_LOG", e.mnt_log_s()),
        ("C3_MAINTAIN_SMOKE_AGY", "gemini::gemini-3.8-flash-high"),
        (
            "C3_MAINTAIN_TEST_SMOKE",
            "gemini::gemini-3.8-flash-high=fail",
        ),
    ];
    for _ in 0..2 {
        let o = e.run(&["maintain", "--run", "--engine", "agy"], &env);
        let t = text(&o);
        assert_eq!(o.status.code(), Some(0), "{t}");
        assert!(
            t.contains("pending update unresolved: smoke failed, no rollback path"),
            "{t}"
        );
        assert!(!t.contains("recovered"), "{t}");
        assert!(!t.contains("trying the newer"), "agy has no check: {t}");
        let pending = e.pending_json();
        assert!(
            pending.contains("\"engine\":\"agy\"") && pending.contains("\"previous\":\"1.1.0\""),
            "the record stays as it was: {pending}"
        );
    }
    assert!(
        !e.mnt_log().contains("agy :: update"),
        "nothing was installed: {}",
        e.mnt_log()
    );
}

/// (TECH_DEBT 2026-11-16) An unresolved pending update (the installed 1.0.80 fails its smoke and the
/// rollback to 1.0.77 does not restore it) no longer freezes the engine: the vendor's newer 1.0.82
/// is tried, smokes clean, and the record is cleared.
#[test]
fn an_unresolved_pending_update_tries_a_newer_release() {
    let e = setup("pend-newer-ok");
    e.write_pending(
        r#"[{"engine":"grok","previous":"1.0.77","target":"1.0.80","when":"2026-11-15T00:00:00Z"}]"#,
    );
    let mut env = e.grok_env("1.0.80", "1.0.82", "grok::grok-4.7=fail-then-pass");
    env.push(("FAKE_GROK_ROLLBACK_NOOP", "1"));
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(t.contains("rollback failed, still 1.0.80"), "{t}");
    assert!(
        t.contains(
            "1.0.80 still fails its smoke; trying the newer 1.0.82 (rollback target 1.0.77)"
        ),
        "{t}"
    );
    assert!(t.contains("1.0.77 -> 1.0.82 ok"), "{t}");
    assert_eq!(verfile(&e.grok_verfile), "1.0.82");
    assert_eq!(
        e.pending_json().trim(),
        "[]",
        "resolved: {}",
        e.pending_json()
    );
}

/// (TECH_DEBT 2026-11-16) The newer release fails its smoke too and its rollback does not restore:
/// the record stays unresolved with the ORIGINAL previous (1.0.77) as the rollback target and the
/// newer release as the target.
#[test]
fn a_newer_release_that_fails_too_keeps_the_original_previous() {
    let e = setup("pend-newer-fail");
    e.write_pending(
        r#"[{"engine":"grok","previous":"1.0.77","target":"1.0.80","when":"2026-11-15T00:00:00Z"}]"#,
    );
    let mut env = e.grok_env("1.0.80", "1.0.82", "grok::grok-4.7=fail");
    env.push(("FAKE_GROK_ROLLBACK_NOOP", "1"));
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("trying the newer 1.0.82 (rollback target 1.0.77)"),
        "{t}"
    );
    assert!(
        t.contains("smoke failed, rollback failed, still 1.0.82"),
        "{t}"
    );
    let pending = e.pending_json();
    assert!(
        pending.contains("\"previous\":\"1.0.77\"") && pending.contains("\"target\":\"1.0.82\""),
        "{pending}"
    );
    assert!(
        e.mnt_log().contains("update --version 1.0.77"),
        "{}",
        e.mnt_log()
    );
}

/// (F15-5, the reviewer's RC5) Pending recovery never installs an older release than the one
/// installed: the recovery smoke of 1.0.80 fails, the rollback to 1.0.77 leaves 1.0.90 on disk and
/// exits non-zero, and the vendor offers 1.0.82 - newer than the 1.0.80 the pass started with, but
/// not newer than what is installed NOW. It is not installed; the record stays unresolved.
#[test]
fn a_pending_recovery_never_installs_a_release_older_than_the_installed_one() {
    let e = setup("pend-no-downgrade");
    e.write_pending(
        r#"[{"engine":"grok","previous":"1.0.77","target":"1.0.80","when":"2026-11-15T00:00:00Z"}]"#,
    );
    let mut env = e.grok_env("1.0.80", "1.0.82", "grok::grok-4.7=fail");
    env.push(("FAKE_GROK_ROLLBACK_TO", "1.0.90"));
    let o = e.run(&["maintain", "--run", "--engine", "grok"], &env);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(t.contains("rollback failed, still 1.0.90"), "{t}");
    assert!(
        t.contains("grok 1.0.82 is not tried: 1.0.90 is installed now"),
        "{t}"
    );
    assert!(!t.contains("trying the newer"), "{t}");
    assert_eq!(verfile(&e.grok_verfile), "1.0.90", "no downgrade");
    assert!(
        !e.mnt_log().contains("update --version 1.0.82"),
        "the older release was never installed: {}",
        e.mnt_log()
    );
    let pending = e.pending_json();
    assert!(
        pending.contains("\"previous\":\"1.0.77\"") && pending.contains("\"target\":\"1.0.80\""),
        "the record stays as it was: {pending}"
    );
}

// ------------------------------------------------------------------ no spawn under test mode

#[test]
fn the_hook_does_not_spawn_a_pass_under_test_mode() {
    let e = setup("no-spawn");
    // a real roster (so the roster guard does not suppress the spawn) + no state → due; test mode
    // without the opt-in must still not spawn (no last-run.log appears).
    let roster = e.work.join("roster.json");
    std::fs::write(
        &roster,
        r#"{"roster_version":1,"reviewers":[{"provider":"grok","engine":"grok","model":"grok-4.7","plan":"supergrok"}]}"#,
    )
    .unwrap();
    let roster_s = roster.to_string_lossy().into_owned();
    let o = e.run(&["hook"], &[("CODEX_CONSULT_ROSTER", roster_s.as_str())]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(
        !e.last_run_log().exists(),
        "no detached pass was spawned under test mode"
    );
}

// ------------------------------------------------------------------ the hook returns without waiting for the pass it spawns

/// Regression guard for the Windows detached spawn. `spawn_detached` runs the pass with
/// `bInheritHandles = TRUE`; if it does not first make the hook's own stdin/stdout/stderr
/// non-inheritable, the spawned pass inherits the hook's output pipes and holds them open, so a
/// reader that collects the hook's output to end-of-stream (here `Command::output()`, and live
/// Claude Code) blocks until the whole pass ends. The pass's first grok probe is made to sleep ~6 s;
/// this test asserts the hook comes back well before that, and that the pass still ran afterwards.
#[test]
fn the_hook_returns_without_waiting_for_the_spawned_pass() {
    let e = setup("hook-nowait");
    let roster = e.work.join("roster.json");
    std::fs::write(
        &roster,
        r#"{"roster_version":1,"reviewers":[{"provider":"grok","engine":"grok","model":"grok-4.7","plan":"supergrok"}]}"#,
    )
    .unwrap();
    let roster_s = roster.to_string_lossy().into_owned();
    let grok_exe = e.grok_exe.to_string_lossy().into_owned();
    let grok_vf = e.grok_verfile.to_string_lossy().into_owned();
    std::fs::write(&e.grok_verfile, "1.0.77").unwrap();
    let fake_log = e.work.join("fake.log").to_string_lossy().into_owned();

    // Baseline: a hook with nothing due (future next_due, already reported) does not spawn, so this
    // measures the bare `c3 hook` overhead on this machine right now.
    e.write_state(r#"{"next_due":"2099-01-01T00:00:00Z","reported":true}"#);
    let t0 = std::time::Instant::now();
    let o = e.run(&["hook"], &[]);
    let base = t0.elapsed();
    assert_eq!(o.status.code(), Some(0), "baseline hook: {}", text(&o));
    assert!(
        !e.last_run_log().exists(),
        "the baseline hook did not spawn a pass"
    );

    // Due: no state -> due; the hook is allowed to spawn under test mode; the pass is scoped to grok
    // (every other engine skipped) and the grok version probe sleeps ~6 s, so the pass runs for
    // several seconds. The hook's stdout/stderr are read to end-of-stream by `Command::output()`.
    std::fs::remove_file(e.state_path()).ok();
    let env: Vec<(&str, &str)> = vec![
        ("CODEX_CONSULT_ROSTER", roster_s.as_str()),
        ("CODEX_CONSULT_GROK_EXE", grok_exe.as_str()),
        ("FAKE_GROK_VERFILE", grok_vf.as_str()),
        ("FAKE_GROK_VER", "1.0.77"),
        ("FAKE_GROK_LATEST", "1.0.77"),
        ("FAKE_GROK_SLEEP", "6"),
        ("FAKE_MNT_LOG", fake_log.as_str()),
        ("C3_MAINTAIN_SKIP", "codex,kimi,agy,muse,claude"),
        ("C3_MAINTAIN_TEST_ALLOW_SPAWN", "1"),
    ];
    let t1 = std::time::Instant::now();
    let o = e.run(&["hook"], &env);
    let due = t1.elapsed();
    assert_eq!(o.status.code(), Some(0), "due hook: {}", text(&o));

    // The hook must come back well before the ~6 s pass can finish: at most 4 s on top of the
    // baseline. With the inherited-handle bug it would instead block for the whole pass (~6 s).
    assert!(
        due < base + std::time::Duration::from_secs(4),
        "the hook waited for the spawned pass: baseline {base:?}, due {due:?} (expected due < baseline + 4 s)"
    );

    // The pass really ran: wait (bounded) for its final report line in the detached log.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut ran = false;
    while std::time::Instant::now() < deadline {
        if std::fs::read_to_string(e.last_run_log())
            .map(|s| s.contains("c3: maintenance"))
            .unwrap_or(false)
        {
            ran = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    assert!(
        ran,
        "the spawned pass did not finish; last-run.log: {:?}",
        std::fs::read_to_string(e.last_run_log()).unwrap_or_default()
    );

    // The pass has finished (it only ever talked to the fakes); clean up the temp codex home.
    let _ = std::fs::remove_dir_all(&e.work);
}
