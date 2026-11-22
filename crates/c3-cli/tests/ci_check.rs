//! End-to-end tests of the red-CI check (0.2.5): `c3 finish` and the SessionStart hook name a failed
//! latest run of the default branch, read through the GitHub CLI. A fake `gh` (a `gh.cmd` on
//! Windows, a `gh` shell script elsewhere) stands first on PATH: it appends its arguments to
//! `GH_FAKE_LOG`, then prints the file `GH_FAKE_JSON` (or exits with `GH_FAKE_EXIT`, or sleeps for
//! `GH_FAKE_SLEEP` seconds first).
//!
//! Each test builds a throwaway repository in a temp dir and sets its identity LOCALLY - never
//! `git config --global`. The `c3` binary cargo built for this test is used (`CARGO_BIN_EXE_c3`).

use std::path::{Path, PathBuf};
use std::process::Command;

fn c3_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_c3"))
}

fn git(repo: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("git on PATH");
    assert!(
        out.status.success(),
        "git {args:?} failed in {} (exit {:?}): {}",
        repo.display(),
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).trim()
    );
}

fn scratch_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "c3-ci-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A repository on `main` with a LOCAL identity, one commit and (when `github`) a GitHub remote.
fn scratch_repo(name: &str, github: bool) -> PathBuf {
    let d = scratch_dir(name);
    git(&d, &["init", "-q", "-b", "main"]);
    git(&d, &["config", "user.email", "t@example.com"]);
    git(&d, &["config", "user.name", "Test"]);
    git(&d, &["config", "commit.gpgsign", "false"]);
    std::fs::write(d.join(".gitignore"), ".eck/\n").unwrap();
    std::fs::write(d.join("README.md"), "# scratch\n").unwrap();
    git(&d, &["add", "-A"]);
    git(&d, &["commit", "-qm", "init"]);
    if github {
        git(
            &d,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/example/c3-ci-test.git",
            ],
        );
    }
    d
}

/// The environment of one test: the fake `gh`, its log and reply files, an isolated codex home.
struct Env {
    root: PathBuf,
    bin: PathBuf,
    home: PathBuf,
    log: PathBuf,
    json: PathBuf,
}

impl Env {
    fn new(name: &str) -> Env {
        let root = scratch_dir(name);
        let bin = root.join("bin");
        let home = root.join("home");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        #[cfg(windows)]
        std::fs::write(
            bin.join("gh.cmd"),
            "@echo off\r\n>>\"%GH_FAKE_LOG%\" echo %*\r\nif defined GH_FAKE_SLEEP ping -n %GH_FAKE_SLEEP% 127.0.0.1 >nul\r\nif defined GH_FAKE_EXIT exit /b %GH_FAKE_EXIT%\r\ntype \"%GH_FAKE_JSON%\"\r\n",
        )
        .unwrap();
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            let p = bin.join("gh");
            std::fs::write(
                &p,
                "#!/bin/sh\necho \"$@\" >> \"$GH_FAKE_LOG\"\n[ -n \"$GH_FAKE_SLEEP\" ] && sleep \"$GH_FAKE_SLEEP\"\n[ -n \"$GH_FAKE_EXIT\" ] && exit \"$GH_FAKE_EXIT\"\ncat \"$GH_FAKE_JSON\"\n",
            )
            .unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        Env {
            log: root.join("gh.log"),
            json: root.join("gh.json"),
            root,
            bin,
            home,
        }
    }

    fn reply(&self, json: &str) {
        std::fs::write(&self.json, json).unwrap();
    }

    /// How many times the fake `gh` ran.
    fn calls(&self) -> usize {
        std::fs::read_to_string(&self.log)
            .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count())
            .unwrap_or(0)
    }

    fn last_args(&self) -> String {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .rfind(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim()
            .to_string()
    }

    fn run(&self, repo: &Path, args: &[&str], extra: &[(&str, &str)]) -> (i32, String, String) {
        let mut paths = vec![self.bin.clone()];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let mut cmd = Command::new(c3_bin());
        cmd.current_dir(repo)
            .args(args)
            .env_remove("C3_CHILD")
            .env_remove("C3_CI_CHECK")
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("CODEX_HOME", &self.home)
            .env("C3_CONFIG", "none")
            .env("C3_MAINTAIN", "off")
            .env("C3_UPDATE_CHECK", "off")
            .env("C3_REPO_MODELS", "off")
            .env("C3_BRANCH_CHECK", "off")
            .env("GH_FAKE_LOG", &self.log)
            .env("GH_FAKE_JSON", &self.json)
            .env_remove("GH_FAKE_EXIT")
            .env_remove("GH_FAKE_SLEEP");
        for (k, v) in extra {
            cmd.env(k, v);
        }
        let out = cmd.output().expect("spawn c3");
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

const RED: &str = r#"[{"status":"completed","conclusion":"failure","displayTitle":"Fix the Format step","databaseId":4242,"createdAt":"2026-11-20T10:00:00Z"},{"status":"completed","conclusion":"success","displayTitle":"older","databaseId":4100,"createdAt":"2026-11-19T10:00:00Z"}]"#;
const GREEN: &str = r#"[{"status":"completed","conclusion":"success","displayTitle":"ok","databaseId":4243,"createdAt":"2026-11-21T10:00:00Z"},{"status":"completed","conclusion":"failure","displayTitle":"old red","databaseId":4200,"createdAt":"2026-11-20T10:00:00Z"}]"#;

const RED_LINE: &str = "CI: the latest run on main failed - Fix the Format step (run 4242)";

fn finish(e: &Env, repo: &Path, extra: &[(&str, &str)], json: bool) -> (i32, String, String) {
    let mut args = vec![
        "finish",
        "--message",
        "chore: ci check test",
        "--status",
        "done",
        "--allow-empty",
        "--no-snapshot",
    ];
    if json {
        args.push("--json");
    }
    e.run(repo, &args, extra)
}

#[test]
fn finish_names_a_red_latest_ci_run_in_text_and_json() {
    let e = Env::new("finish-red");
    let repo = scratch_repo("finish-red", true);
    e.reply(RED);

    let (code, out, err) = finish(&e, &repo, &[], false);
    assert_eq!(code, 0, "finish must not refuse over CI: {out}\n{err}");
    assert!(out.lines().any(|l| l == RED_LINE), "{out}");
    // (a Windows batch launcher gets the comma list quoted by the runtime)
    let args = e.last_args().replace('"', "");
    assert!(
        args.contains("run list --branch main --limit 5 --json status,conclusion,displayTitle,databaseId,createdAt"),
        "gh was asked: {args}"
    );

    let (code, out, err) = finish(&e, &repo, &[], true);
    assert_eq!(code, 0, "{out}\n{err}");
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(v["ci"], RED_LINE, "{v}");

    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn finish_is_silent_for_green_ci_a_failing_gh_no_github_remote_and_the_off_switch() {
    let e = Env::new("finish-quiet");
    let repo = scratch_repo("finish-quiet", true);

    // the latest completed run is green (an older red one does not count)
    e.reply(GREEN);
    let (code, out, _err) = finish(&e, &repo, &[], false);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("CI:"), "{out}");
    assert_eq!(e.calls(), 1);
    let (_c, out, _e) = finish(&e, &repo, &[], true);
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert!(v["ci"].is_null(), "{v}");

    // gh not logged in / failing: silent, finish still succeeds
    e.reply(RED);
    let (code, out, _err) = finish(&e, &repo, &[("GH_FAKE_EXIT", "1")], false);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("CI:"), "{out}");

    // the off switch: gh is not even started
    let before = e.calls();
    let (code, out, _err) = finish(&e, &repo, &[("C3_CI_CHECK", "off")], false);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("CI:"), "{out}");
    assert_eq!(e.calls(), before, "gh ran with the check off");

    // no GitHub remote: gh is not started either
    let plain = scratch_repo("finish-quiet-plain", false);
    let before = e.calls();
    let (code, out, _err) = finish(&e, &plain, &[], false);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("CI:"), "{out}");
    assert_eq!(e.calls(), before, "gh ran without a GitHub remote");

    let _ = std::fs::remove_dir_all(&repo);
    let _ = std::fs::remove_dir_all(&plain);
}

#[test]
fn the_ci_switch_in_the_config_file_turns_it_off() {
    let e = Env::new("finish-config");
    let repo = scratch_repo("finish-config", true);
    e.reply(RED);
    let cfg = e.root.join("config.json");
    std::fs::write(&cfg, r#"{"config_version":1,"ci_check":false}"#).unwrap();
    let cfg_s = cfg.to_string_lossy().to_string();
    let (code, out, err) = finish(&e, &repo, &[("C3_CONFIG", cfg_s.as_str())], false);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(!out.contains("CI:"), "{out}");
    assert_eq!(e.calls(), 0, "gh ran with ci_check false");
    // and a config without the key keeps it ON
    std::fs::write(&cfg, r#"{"config_version":1}"#).unwrap();
    let (_c, out, _e) = finish(&e, &repo, &[("C3_CONFIG", cfg_s.as_str())], false);
    assert!(out.lines().any(|l| l == RED_LINE), "{out}");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn the_hook_names_a_red_run_once_a_day_and_never_in_a_child() {
    let e = Env::new("hook-red");
    let repo = scratch_repo("hook-red", true);
    e.reply(RED);

    let (code, out, err) = e.run(&repo, &["hook"], &[]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert_eq!(
        out.lines().filter(|l| *l == RED_LINE).count(),
        1,
        "the red line once: {out}"
    );
    assert_eq!(e.calls(), 1);
    assert!(e.home.join("c3").join("ci-check.json").exists());

    // the same day: no new call, no line
    let (code, out, _err) = e.run(&repo, &["hook"], &[]);
    assert_eq!(code, 0);
    assert!(!out.contains("CI:"), "{out}");
    assert_eq!(e.calls(), 1, "a second network call within a day");

    // a different repository is its own entry
    let other = scratch_repo("hook-red-other", true);
    let (_c, out, _e) = e.run(&other, &["hook"], &[]);
    assert!(out.lines().any(|l| l == RED_LINE), "{out}");
    assert_eq!(e.calls(), 2);

    // inside a C3 child and with the off switch: nothing, no call
    let third = scratch_repo("hook-red-third", true);
    let (_c, out, _e) = e.run(&third, &["hook"], &[("C3_CHILD", "1")]);
    assert!(!out.contains("CI:"), "{out}");
    let (_c, out, _e) = e.run(&third, &["hook"], &[("C3_CI_CHECK", "off")]);
    assert!(!out.contains("CI:"), "{out}");
    assert_eq!(e.calls(), 2);

    let _ = std::fs::remove_dir_all(&repo);
    let _ = std::fs::remove_dir_all(&other);
    let _ = std::fs::remove_dir_all(&third);
}

#[test]
fn a_failing_gh_costs_the_hook_one_try_a_day_and_a_green_run_prints_nothing() {
    let e = Env::new("hook-quiet");
    let repo = scratch_repo("hook-quiet", true);

    // not logged in: silent, and the attempt is on record
    e.reply(RED);
    let (code, out, _err) = e.run(&repo, &["hook"], &[("GH_FAKE_EXIT", "1")]);
    assert_eq!(code, 0);
    assert!(!out.contains("CI:"), "{out}");
    assert_eq!(e.calls(), 1);
    let (_c, out, _e) = e.run(&repo, &["hook"], &[]);
    assert!(!out.contains("CI:"), "{out}");
    assert_eq!(e.calls(), 1, "a failed attempt was retried within a day");

    // a green run: nothing
    let green = scratch_repo("hook-green", true);
    e.reply(GREEN);
    let (_c, out, _e) = e.run(&green, &["hook"], &[]);
    assert!(!out.contains("CI:"), "{out}");
    assert_eq!(e.calls(), 2);

    // no GitHub remote: gh is not started and no state is written for it
    let plain = scratch_repo("hook-plain", false);
    let (_c, out, _e) = e.run(&plain, &["hook"], &[]);
    assert!(!out.contains("CI:"), "{out}");
    assert_eq!(e.calls(), 2);

    let _ = std::fs::remove_dir_all(&repo);
    let _ = std::fs::remove_dir_all(&green);
    let _ = std::fs::remove_dir_all(&plain);
}

// A `gh` that stalls cannot hold the SessionStart hook: the hook's own deadline (3 s for gh, 5 s in
// all) ends the wait, the hook exits 0 and prints no CI line.
#[test]
fn a_stalled_gh_does_not_hold_the_hook() {
    let e = Env::new("hook-stall");
    let repo = scratch_repo("hook-stall", true);
    e.reply(RED);
    let start = std::time::Instant::now();
    let (code, out, _err) = e.run(&repo, &["hook"], &[("GH_FAKE_SLEEP", "25")]);
    assert_eq!(code, 0);
    assert!(!out.contains("CI:"), "{out}");
    assert!(
        start.elapsed() < std::time::Duration::from_secs(20),
        "the hook waited for gh: {:?}",
        start.elapsed()
    );
    let _ = std::fs::remove_dir_all(&repo);
}
