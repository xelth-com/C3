//! (wave 11) `c3 update` and the `c3 hook` self-update line, end to end through the real `c3`
//! binary. No test reaches the network: every run sets `CODEX_CONSULT_TEST_MODE=1`, which replaces
//! the GitHub fetch with a reader of `C3_UPDATE_TEST_TAGS_FILE` (the tags-API JSON shape) - absent
//! means "offline". `CODEX_HOME` is a scratch directory so the cache (`<codex home>/c3/
//! update-check.json`) never touches a real home, and `CLAUDE_PLUGIN_ROOT`/`C3_SOURCE_CHECKOUT`/
//! `C3_CONFIG` are cleared so no source checkout is ever detected.

use std::path::PathBuf;
use std::process::{Command, Output};

fn c3_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_c3"))
}

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let p = std::env::temp_dir().join(format!("c3-upd-it-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// A `c3` invocation with a clean, offline, scratch-home environment. `tags` is the tags-API JSON
/// body to serve (none → the fetcher reports "offline").
fn run(home: &PathBuf, tags: Option<&str>, args: &[&str]) -> Output {
    let mut cmd = Command::new(c3_bin());
    for (k, _) in std::env::vars() {
        if k.starts_with("CODEX_CONSULT_") || k.starts_with("C3_") || k == "CLAUDE_PLUGIN_ROOT" {
            cmd.env_remove(&k);
        }
    }
    cmd.env("CODEX_HOME", home)
        .env("HOME", home.join("userhome"))
        .env("USERPROFILE", home.join("userhome"))
        .env("CODEX_CONSULT_TEST_MODE", "1")
        .env("CODEX_CONSULT_ROSTER", "none")
        .env("CODEX_CONSULT_HEALTH", "none")
        .env("CODEX_CONSULT_TELEMETRY", "off");
    match tags {
        Some(body) => {
            let f = home.join("tags.json");
            std::fs::write(&f, body).unwrap();
            cmd.env("C3_UPDATE_TEST_TAGS_FILE", &f);
        }
        None => {
            cmd.env_remove("C3_UPDATE_TEST_TAGS_FILE");
        }
    }
    cmd.args(args);
    cmd.output().expect("run c3")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).replace("\r\n", "\n")
}

/// A newer release (and a non-semver tag that must be ignored).
const NEWER: &str =
    r#"[{"name":"nightly","commit":{}},{"name":"v99.0.0","commit":{"sha":"abc123"}}]"#;
/// A release older than anything we ship: the installed binary is up to date.
const OLDER: &str = r#"[{"name":"v0.0.1","commit":{"sha":"old"}}]"#;

#[test]
fn hook_prints_the_update_line_only_when_newer() {
    let home = scratch("hook-newer");
    let o = run(&home, Some(NEWER), &["hook"]);
    assert_eq!(o.status.code(), Some(0));
    let line = stdout(&o)
        .lines()
        .find(|l| l.starts_with("c3: update available "))
        .map(str::to_string)
        .unwrap_or_else(|| panic!("no update line in:\n{}", stdout(&o)));
    assert!(
        line.starts_with("c3: update available v99.0.0 (installed "),
        "{line}"
    );
    assert!(
        line.contains("review: https://github.com/xelth-com/C3/blob/v99.0.0/CHANGELOG.md ; apply: c3 update --apply"),
        "{line}"
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn hook_prints_no_update_line_when_up_to_date_or_offline() {
    // up to date
    let home = scratch("hook-utd");
    let o = run(&home, Some(OLDER), &["hook"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(
        !stdout(&o)
            .lines()
            .any(|l| l.starts_with("c3: update available ")),
        "unexpected update line:\n{}",
        stdout(&o)
    );
    let _ = std::fs::remove_dir_all(&home);

    // offline (no tags file): still exit 0, no line, silent
    let home = scratch("hook-offline");
    let o = run(&home, None, &["hook"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(
        !stdout(&o)
            .lines()
            .any(|l| l.starts_with("c3: update available ")),
        "unexpected update line:\n{}",
        stdout(&o)
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn update_exit_3_when_newer() {
    let home = scratch("upd-3");
    let o = run(&home, Some(NEWER), &["update"]);
    assert_eq!(o.status.code(), Some(3), "{}", stdout(&o));
    let t = stdout(&o);
    assert!(t.contains("an update is available"), "{t}");
    assert!(
        t.contains("https://github.com/xelth-com/C3/blob/v99.0.0/CHANGELOG.md"),
        "{t}"
    );
    assert!(
        t.contains(
            "cargo install --git https://github.com/xelth-com/C3.git --tag v99.0.0 --locked"
        ),
        "{t}"
    );
    assert!(t.contains("nothing was changed."), "{t}");
    // the cache was written
    assert!(home.join("c3").join("update-check.json").exists());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn update_exit_0_when_up_to_date() {
    let home = scratch("upd-0");
    let o = run(&home, Some(OLDER), &["update"]);
    assert_eq!(o.status.code(), Some(0), "{}", stdout(&o));
    assert!(stdout(&o).contains("up to date"), "{}", stdout(&o));
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn update_exit_1_on_fetch_failure() {
    let home = scratch("upd-1");
    let o = run(&home, None, &["update"]);
    assert_eq!(o.status.code(), Some(1), "{}", stdout(&o));
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("offline"), "{err}");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn update_check_flag_is_the_same_as_no_flags() {
    let home = scratch("upd-check");
    let o = run(&home, Some(NEWER), &["update", "--check"]);
    assert_eq!(o.status.code(), Some(3), "{}", stdout(&o));
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn apply_refused_when_up_to_date() {
    let home = scratch("apply-utd");
    let o = run(&home, Some(OLDER), &["update", "--apply"]);
    assert_eq!(o.status.code(), Some(0), "{}", stdout(&o));
    assert!(stdout(&o).contains("already at v"), "{}", stdout(&o));
    assert!(
        !stdout(&o).contains("cargo install"),
        "apply must not run anything when up to date:\n{}",
        stdout(&o)
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn apply_refused_on_fetch_failure() {
    let home = scratch("apply-offline");
    let o = run(&home, None, &["update", "--apply"]);
    assert_eq!(o.status.code(), Some(1), "{}", stdout(&o));
    let _ = std::fs::remove_dir_all(&home);
}
