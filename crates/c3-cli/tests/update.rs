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
        .env("CODEX_CONSULT_TELEMETRY", "off")
        // (11b F99-2) the test-mode fetcher appends one line per ACTUAL fetch attempt here, so a
        // test can observe the throttle independent of whole-second timestamp resolution.
        .env(
            "C3_UPDATE_TEST_FETCH_COUNT_FILE",
            home.join("fetch-count.log"),
        );
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
    // (11b F97-2) the cargo step pins the API commit, never the tag name.
    assert!(
        t.contains("cargo install --git https://github.com/xelth-com/C3.git --rev abc123 --locked"),
        "{t}"
    );
    assert!(!t.contains("--tag v99.0.0"), "{t}");
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

// ---------------------------------------------------------------- 11b F97-3: attempt state / offer

/// Seed `<home>/c3/update-check.json` with a success `age_days` old advertising v99.0.0.
fn seed_cache(home: &std::path::Path, age_days: i64) -> String {
    let stamp = (chrono::Utc::now() - chrono::Duration::days(age_days))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let dir = home.join("c3");
    std::fs::create_dir_all(&dir).unwrap();
    let body = format!(
        r#"{{"checked_at":"{stamp}","attempted_at":"{stamp}","latest":"99.0.0","latest_sha":"abc123","installed":"0.0.1","source":"github-tags"}}"#
    );
    std::fs::write(dir.join("update-check.json"), body).unwrap();
    stamp
}

fn cache_field(home: &std::path::Path, key: &str) -> String {
    let s = std::fs::read_to_string(home.join("c3").join("update-check.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}

#[test]
fn hook_suppresses_a_stale_offer_on_fetch_failure() {
    // a success 8 days old (> the 7-day offer window) + an offline refresh: no line, and the
    // failed attempt is recorded (attempted_at advances) while the stale success is kept.
    let home = scratch("stale-offer");
    let seeded = seed_cache(&home, 8);
    let o = run(&home, None, &["hook"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(
        !stdout(&o)
            .lines()
            .any(|l| l.starts_with("c3: update available ")),
        "a > 7-day-old success must not be offered:\n{}",
        stdout(&o)
    );
    assert_eq!(cache_field(&home, "checked_at"), seeded, "success kept");
    assert_ne!(
        cache_field(&home, "attempted_at"),
        seeded,
        "attempt recorded"
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn hook_still_offers_from_a_fresh_success_on_fetch_failure() {
    // a success 2 days old (< 7 days) + an offline refresh: the cached offer still prints.
    let home = scratch("fresh-offer");
    seed_cache(&home, 2);
    let o = run(&home, None, &["hook"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(
        stdout(&o)
            .lines()
            .any(|l| l.starts_with("c3: update available v99.0.0 (installed ")),
        "a fresh cached success must still be offered:\n{}",
        stdout(&o)
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn hook_backs_off_after_a_failed_attempt() {
    // no cache, offline: the first hook attempts (and records the failure); the immediate second
    // call is within the 1 h failure backoff, so it must NOT fetch again. Observe the fetch itself
    // via the counter file (not whole-second timestamps), so two runs inside one UTC second cannot
    // slip through.
    let home = scratch("backoff");
    assert_eq!(run(&home, None, &["hook"]).status.code(), Some(0));
    assert!(
        !cache_field(&home, "attempted_at").is_empty(),
        "the first failed attempt is recorded"
    );
    assert_eq!(run(&home, None, &["hook"]).status.code(), Some(0));
    let fetches = std::fs::read_to_string(home.join("fetch-count.log"))
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count();
    assert_eq!(
        fetches, 1,
        "exactly one fetch; the second call must be throttled"
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn update_prints_cached_data_when_a_forced_refresh_fails() {
    // `c3 update` offline WITH a prior cache: prints the cached data annotated "refresh failed",
    // exit 1.
    let home = scratch("forced-fail");
    seed_cache(&home, 2);
    let o = run(&home, None, &["update"]);
    assert_eq!(o.status.code(), Some(1), "{}", stdout(&o));
    let t = stdout(&o);
    assert!(t.contains("refresh failed: offline"), "{t}");
    assert!(t.contains("v99.0.0"), "the cached latest is shown: {t}");
    let _ = std::fs::remove_dir_all(&home);
}
