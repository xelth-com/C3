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
