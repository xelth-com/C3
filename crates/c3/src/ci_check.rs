//! The red-CI check (0.2.5): `c3 finish`, after its commit, and the SessionStart hook name a failed
//! latest run of the default branch, read through the GitHub CLI.
//!
//! CI sat red for a week once and nobody saw it. The check asks `gh run list --branch <default>
//! --limit 5 --json status,conclusion,displayTitle,databaseId,createdAt` in the repository (cwd =
//! the repository root) and, when the LATEST COMPLETED run - a cancelled or skipped one decides
//! nothing - failed, answers one line:
//!
//! ```text
//! CI: the latest run on main failed - <title> (run <id>)
//! ```
//!
//! It is a note, never a refusal, and it is silent whenever it cannot know: no `gh` on PATH, `gh`
//! not logged in, no GitHub remote, an unknown default branch, a timeout, a reply that does not
//! parse, a run that is green. `gh` runs under a deadline (10 s for `c3 finish`, 3 s in the hook)
//! and is killed past it. Default ON; `C3_CI_CHECK=off` (the environment wins) or `"ci_check":
//! false` in the C3 config turns it off.
//!
//! The hook asks at most once a day per repository: the time of the last attempt is kept in
//! `<codex home>/c3/ci-check.json` (a temp file and a rename), written BEFORE the call so a slow or
//! failing `gh` backs off for a day too, and the line is printed only by the session that asked.
//! Nothing runs inside a C3 child.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde_json::Value;

/// How long `c3 finish` waits for `gh`.
pub const FINISH_DEADLINE: Duration = Duration::from_secs(10);

/// How long the SessionStart hook waits for `gh`.
pub const HOOK_DEADLINE: Duration = Duration::from_secs(3);

/// The hook asks again for the same repository only after this long.
const HOOK_INTERVAL_HOURS: i64 = 24;

/// How many runs `gh run list` is asked for.
const RUN_LIMIT: &str = "5";

/// The longest title the line carries (characters).
const TITLE_MAX: usize = 100;

/// Whether the check is ON. Default ON; `C3_CI_CHECK=off` (environment wins) or `"ci_check": false`
/// in the C3 config turns it off.
pub fn enabled() -> bool {
    if let Ok(v) = std::env::var("C3_CI_CHECK") {
        return !v.trim().eq_ignore_ascii_case("off");
    }
    match crate::c3config::load() {
        Ok(Some(cfg)) => cfg.ci_check,
        _ => true,
    }
}

/// The line for a failed latest completed run, from the JSON `gh run list` printed (newest first,
/// but ordered by `createdAt` here to be sure); `None` for a green run, no completed run, or JSON
/// that does not parse.
pub fn failure_line(json: &str, branch: &str) -> Option<String> {
    let v: Value = serde_json::from_str(json).ok()?;
    let mut runs: Vec<&Value> = v
        .as_array()?
        .iter()
        .filter(|r| r.get("status").and_then(Value::as_str) == Some("completed"))
        // a cancelled or skipped run says nothing about the code
        .filter(|r| {
            !matches!(
                r.get("conclusion").and_then(Value::as_str),
                Some("cancelled" | "skipped")
            )
        })
        .collect();
    // RFC 3339 timestamps in one zone sort as text; newest first, a stable sort keeps gh's order
    runs.sort_by(|a, b| {
        let ka = a.get("createdAt").and_then(Value::as_str).unwrap_or("");
        let kb = b.get("createdAt").and_then(Value::as_str).unwrap_or("");
        kb.cmp(ka)
    });
    let latest = runs.first()?;
    let conclusion = latest.get("conclusion").and_then(Value::as_str)?;
    if !matches!(conclusion, "failure" | "timed_out" | "startup_failure") {
        return None;
    }
    let title = c3_core::one_line(
        latest
            .get("displayTitle")
            .and_then(Value::as_str)
            .unwrap_or("(untitled run)"),
    );
    let title: String = if title.chars().count() > TITLE_MAX {
        let cut: String = title.chars().take(TITLE_MAX - 3).collect();
        format!("{cut}...")
    } else {
        title
    };
    let id = latest
        .get("databaseId")
        .and_then(Value::as_u64)
        .map(|n| n.to_string())
        .unwrap_or_else(|| "?".to_string());
    Some(format!(
        "CI: the latest run on {} failed - {title} (run {id})",
        c3_core::one_line(branch)
    ))
}

/// A git command in `repo` that takes no optional lock.
fn git_out(repo: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Whether the repository has a remote whose URL names github.com.
pub fn has_github_remote(repo_root: &Path) -> bool {
    git_out(repo_root, &["remote", "-v"])
        .map(|s| remotes_name_github(&s))
        .unwrap_or(false)
}

/// Whether `git remote -v` output names a github.com remote (https, ssh or scp-like form).
fn remotes_name_github(out: &str) -> bool {
    out.lines().any(|l| {
        let l = l.to_ascii_lowercase();
        l.contains("github.com/") || l.contains("github.com:")
    })
}

/// `gh` with its arguments, run in `cwd` under `deadline`: its stdout on a clean exit, `None` on any
/// failure - a spawn error, a non-zero exit (not logged in, no such repository), a timeout (the
/// process is killed), a reply that does not arrive in time.
fn run_gh(gh: &str, args: &[&str], cwd: &Path, deadline: Duration) -> Option<String> {
    let mut child = Command::new(gh)
        .args(args)
        .current_dir(cwd)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    // the reader runs apart so a full pipe never stalls the wait, and a grandchild that keeps the
    // pipe open cannot hold this call past its deadline
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        let _ = tx.send(buf);
    });
    let end = Instant::now() + deadline;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let left = end
                    .saturating_duration_since(Instant::now())
                    .max(Duration::from_millis(200));
                return rx
                    .recv_timeout(left)
                    .ok()
                    .map(|b| String::from_utf8_lossy(&b).into_owned());
            }
            Ok(None) => {
                if Instant::now() >= end {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => return None,
        }
    }
}

/// The check proper, for a repository: `gh` on PATH, a GitHub remote, a known default branch, then
/// the latest completed run. `None` whenever any of it is missing or the run is not red.
pub fn check(repo_root: &Path, deadline: Duration) -> Option<String> {
    let gh = crate::providers::find_on_path("gh")?;
    if !has_github_remote(repo_root) {
        return None;
    }
    let branch = crate::setup::branches::default_branch(repo_root).ok()?;
    check_with(&gh, repo_root, &branch, deadline)
}

/// [`check`] once `gh` and the branch are known.
fn check_with(gh: &str, repo_root: &Path, branch: &str, deadline: Duration) -> Option<String> {
    let json = run_gh(
        gh,
        &[
            "run",
            "list",
            "--branch",
            branch,
            "--limit",
            RUN_LIMIT,
            "--json",
            "status,conclusion,displayTitle,databaseId,createdAt",
        ],
        repo_root,
        deadline,
    )?;
    failure_line(&json, branch)
}

/// The line for `c3 finish` (after its commit): `None` when the check is off or has nothing to say.
pub fn finish_line(repo_root: &Path) -> Option<String> {
    if !enabled() {
        return None;
    }
    check(repo_root, FINISH_DEADLINE)
}

// --------------------------------------------------------------------------- the hook's daily gate

/// `<codex home>/c3/ci-check.json`.
fn state_path() -> Option<PathBuf> {
    let home = crate::update::codex_home();
    if home.as_os_str().is_empty() {
        return None;
    }
    Some(home.join("c3").join("ci-check.json"))
}

fn rfc3339(now: DateTime<Utc>) -> String {
    now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// The repository's key in the state file.
fn state_key(repo_root: &Path) -> String {
    let k = repo_root.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        k.to_lowercase()
    } else {
        k
    }
}

/// Whether the repository `key` is due: no attempt on record, one older than a day, or one in the
/// future (a clock that moved back must not silence the check for good).
fn due(state: &Value, key: &str, now: DateTime<Utc>) -> bool {
    let last = state
        .get("repos")
        .and_then(|r| r.get(key))
        .and_then(|e| e.get("attempted_at"))
        .and_then(Value::as_str)
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc));
    match last {
        None => true,
        Some(t) => {
            let age = now.signed_duration_since(t);
            age < chrono::Duration::zero() || age >= chrono::Duration::hours(HOOK_INTERVAL_HOURS)
        }
    }
}

/// `state` with the attempt of `key` set to `now`; entries older than 30 days are dropped.
fn with_attempt(mut state: Value, key: &str, now: DateTime<Utc>) -> Value {
    if !state.is_object() {
        state = serde_json::json!({});
    }
    let obj = state.as_object_mut().expect("an object");
    let repos = obj
        .entry("repos")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut();
    let Some(repos) = repos else {
        return serde_json::json!({ "repos": { key: { "attempted_at": rfc3339(now) } } });
    };
    repos.retain(|_, e| {
        e.get("attempted_at")
            .and_then(Value::as_str)
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .is_some_and(|t| now.signed_duration_since(t) < chrono::Duration::days(30))
    });
    repos.insert(
        key.to_string(),
        serde_json::json!({ "attempted_at": rfc3339(now) }),
    );
    state
}

/// Best-effort write of the state file through a temp file and a rename.
fn write_state(path: &Path, state: &Value) {
    if let Some(dir) = path.parent() {
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
    }
    let Ok(bytes) = serde_json::to_vec_pretty(state) else {
        return;
    };
    let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    if std::fs::write(&tmp, &bytes).is_ok() && std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// The SessionStart hook's line: at most one `gh` call per repository and day, the line only when
/// the latest completed run is red. Nothing when the check is off, inside a C3 child, outside a git
/// repository, without `gh` or a GitHub remote (no state is written then: the answer is free).
pub fn hook_line() -> Option<String> {
    if !enabled() || crate::engines::in_c3_child() {
        return None;
    }
    let cwd = std::env::current_dir().ok()?;
    let repo_root = crate::providers::resolve_repo_root(&cwd);
    if !repo_root.join(".git").exists() {
        return None;
    }
    let gh = crate::providers::find_on_path("gh")?;
    if !has_github_remote(&repo_root) {
        return None;
    }
    let path = state_path()?;
    let key = state_key(&repo_root);
    let now = Utc::now();
    let state: Value = std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if !due(&state, &key, now) {
        return None;
    }
    // the attempt is recorded first: a `gh` that stalls, fails or is not logged in costs one try a day
    write_state(&path, &with_attempt(state, &key, now));
    let branch = crate::setup::branches::default_branch(&repo_root).ok()?;
    check_with(&gh, &repo_root, &branch, HOOK_DEADLINE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(status: &str, conclusion: &str, id: u64, at: &str, title: &str) -> String {
        format!(
            r#"{{"status":"{status}","conclusion":"{conclusion}","displayTitle":"{title}","databaseId":{id},"createdAt":"{at}"}}"#
        )
    }

    #[test]
    fn a_failed_latest_completed_run_is_named() {
        let json = format!(
            "[{},{}]",
            run(
                "completed",
                "failure",
                77,
                "2026-11-20T10:00:00Z",
                "Fix the thing"
            ),
            run("completed", "success", 70, "2026-11-19T10:00:00Z", "Older")
        );
        assert_eq!(
            failure_line(&json, "main").as_deref(),
            Some("CI: the latest run on main failed - Fix the thing (run 77)")
        );
    }

    #[test]
    fn green_cancelled_running_and_broken_replies_are_silent() {
        // the latest completed run is green
        let green = format!(
            "[{},{}]",
            run("completed", "success", 2, "2026-11-20T10:00:00Z", "ok"),
            run("completed", "failure", 1, "2026-11-19T10:00:00Z", "old red")
        );
        assert_eq!(failure_line(&green, "main"), None);
        // a newer run still in progress does not hide the completed red one, a cancelled run decides nothing
        let mixed = format!(
            "[{},{},{}]",
            run("in_progress", "", 4, "2026-11-21T10:00:00Z", "running"),
            run(
                "completed",
                "cancelled",
                3,
                "2026-11-20T12:00:00Z",
                "cancelled"
            ),
            run("completed", "failure", 2, "2026-11-20T10:00:00Z", "red")
        );
        assert_eq!(
            failure_line(&mixed, "main").as_deref(),
            Some("CI: the latest run on main failed - red (run 2)")
        );
        // nothing completed, no runs, not JSON, not an array
        let running = format!(
            "[{}]",
            run("queued", "", 5, "2026-11-21T10:00:00Z", "queued")
        );
        assert_eq!(failure_line(&running, "main"), None);
        assert_eq!(failure_line("[]", "main"), None);
        assert_eq!(failure_line("not json", "main"), None);
        assert_eq!(failure_line("{}", "main"), None);
    }

    #[test]
    fn a_long_multiline_title_is_one_short_line() {
        let long = "x ".repeat(80);
        let json = format!(
            "[{}]",
            run("completed", "timed_out", 9, "2026-11-20T10:00:00Z", &long)
        );
        let l = failure_line(&json, "main").unwrap();
        assert!(!l.contains('\n'), "{l}");
        assert!(l.contains("...") && l.ends_with("(run 9)"), "{l}");
    }

    #[test]
    fn github_remotes_are_recognised() {
        assert!(remotes_name_github(
            "origin\thttps://github.com/a/b.git (fetch)\n"
        ));
        assert!(remotes_name_github(
            "origin\tgit@github.com:a/b.git (push)\n"
        ));
        assert!(!remotes_name_github(
            "origin\thttps://gitlab.com/a/b.git (fetch)\n"
        ));
        assert!(!remotes_name_github(""));
    }

    #[test]
    fn the_daily_gate_asks_once_a_day() {
        let now = DateTime::parse_from_rfc3339("2026-11-22T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let key = "c:/repo";
        let empty = serde_json::json!({});
        assert!(due(&empty, key, now));
        let after = with_attempt(empty, key, now);
        assert!(!due(&after, key, now + chrono::Duration::hours(23)));
        assert!(due(&after, key, now + chrono::Duration::hours(24)));
        // a clock that moved back is not silenced for good
        assert!(due(&after, key, now - chrono::Duration::hours(2)));
        // another repository is its own entry
        assert!(due(&after, "c:/other", now));
        // old entries are dropped on the next write
        let later = now + chrono::Duration::days(40);
        let pruned = with_attempt(after, "c:/other", later);
        assert!(pruned["repos"].get(key).is_none());
        assert!(pruned["repos"].get("c:/other").is_some());
    }
}
