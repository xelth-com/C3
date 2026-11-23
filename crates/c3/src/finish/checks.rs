//! (0.2.5) Pre-finish checks: the repository's own checks `c3 finish` runs BEFORE it writes or
//! stages anything (CI sat red for a week once on a formatting failure nobody ran locally).
//!
//! The list is per repository: `"finish_checks"` in `<repo>/.c3/config.json` (C3's own
//! configuration, `<codex home>/c3/config.json`, is per machine), an array of command lines. The
//! file is only READ - through [`crate::state_dir::read_path`], so a legacy `.eck/` that was not
//! moved yet is read too - and never created. No file or no key: no checks, as before.
//!
//! Each command line runs in the repository root through the platform's shell (`sh -c`, `cmd /D /S
//! /C`), in order, with `C3_CHILD=1` (a `c3` it runs knows it is a child) and under its own
//! [`CHECK_DEADLINE`] through the shared deadline-bound runner ([`crate::bounded`]): past it the
//! check and every process it started are stopped and the check counts as failed. The first check
//! that fails ends the run; `c3 finish` then prints its name, its exit code and the last
//! [`TAIL_LINES`] lines of its output, and nothing is staged or committed. `--skip-checks` goes on
//! without them and says so, in the output and in the journal entry.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::Value;

/// The per-repository configuration file inside C3's project folder.
pub const CONFIG_FILE: &str = "config.json";

/// The key of the list in [`CONFIG_FILE`].
pub const CHECKS_KEY: &str = "finish_checks";

/// The deadline of one check.
pub const CHECK_DEADLINE: Duration = Duration::from_secs(120);

/// How many of a failed check's last output lines are shown.
pub const TAIL_LINES: usize = 20;

/// The configuration file a repository's checks are read from (`.c3/config.json`, or the legacy
/// folder's until it was moved), `None` when there is no project folder.
pub fn config_path(repo_root: &Path) -> Option<PathBuf> {
    crate::state_dir::read_path(repo_root).map(|d| d.join(CONFIG_FILE))
}

/// The repository's pre-finish checks: `Ok(empty)` when there is no project folder, no
/// [`CONFIG_FILE`] or no [`CHECKS_KEY`]; `Err` (why) when the file cannot be read or parsed, or the
/// key is not an array of non-empty strings - the coordinator meant checks to run, so `c3 finish`
/// refuses rather than skipping them silently.
pub fn configured(repo_root: &Path) -> Result<Vec<String>, String> {
    let Some(path) = config_path(repo_root) else {
        return Ok(Vec::new());
    };
    let label = crate::state_dir::label(path.parent().unwrap_or(repo_root), CONFIG_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{label} cannot be read ({e})")),
    };
    parse_checks(&text).map_err(|e| format!("{label}: {e}"))
}

/// [`configured`] over the file's text (pure).
pub fn parse_checks(text: &str) -> Result<Vec<String>, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("not valid JSON ({e})"))?;
    let obj = v
        .as_object()
        .ok_or_else(|| "the top level must be a JSON object".to_string())?;
    let Some(list) = obj.get(CHECKS_KEY) else {
        return Ok(Vec::new());
    };
    let items = list
        .as_array()
        .ok_or_else(|| format!("{CHECKS_KEY} must be an array of command lines"))?;
    items
        .iter()
        .map(|i| {
            i.as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty() && !s.contains(['\r', '\n']))
                .map(str::to_string)
                .ok_or_else(|| {
                    format!("each {CHECKS_KEY} entry must be a non-empty one-line command line")
                })
        })
        .collect()
}

/// A check that did not pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckFailure {
    /// The command line, as configured.
    pub name: String,
    /// Its exit code; `None` when it did not exit by itself (stopped at the deadline, killed by a
    /// signal) or could not be started.
    pub code: Option<i32>,
    /// It was stopped at [`CHECK_DEADLINE`].
    pub timed_out: bool,
    /// Why it could not be started (empty when it ran).
    pub start_error: String,
    /// The last [`TAIL_LINES`] lines of its output (stdout, then stderr).
    pub tail: Vec<String>,
}

impl std::fmt::Display for CheckFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let how = if self.timed_out {
            format!(
                "was stopped after {} s (no exit code)",
                CHECK_DEADLINE.as_secs()
            )
        } else if !self.start_error.is_empty() {
            format!("could not be started ({})", self.start_error)
        } else {
            match self.code {
                Some(c) => format!("failed with exit code {c}"),
                None => "failed without an exit code".to_string(),
            }
        };
        writeln!(
            f,
            "refusing to finish: the pre-finish check `{}` {how}; nothing was staged or committed",
            self.name
        )?;
        if self.tail.is_empty() {
            write!(f, "(it printed nothing)")?;
        } else {
            writeln!(f, "its last {} line(s) of output:", self.tail.len())?;
            for l in &self.tail {
                writeln!(f, "  {l}")?;
            }
        }
        write!(
            f,
            "fix it and run c3 finish again, or pass --skip-checks to finish without the checks"
        )
    }
}

/// The last [`TAIL_LINES`] non-empty lines of a check's stdout and stderr (lossy UTF-8, trailing
/// white space trimmed).
pub fn output_tail(stdout: &[u8], stderr: &[u8]) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for bytes in [stdout, stderr] {
        let text = String::from_utf8_lossy(bytes);
        lines.extend(
            text.lines()
                .map(|l| l.trim_end().to_string())
                .filter(|l| !l.is_empty()),
        );
    }
    let skip = lines.len().saturating_sub(TAIL_LINES);
    lines.split_off(skip)
}

/// The shell command that runs one command line in `repo_root`, marked as a C3 child.
fn shell_command(line: &str, repo_root: &Path) -> Command {
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        let mut c = Command::new("cmd");
        // /S: cmd strips exactly the outer quotes and runs the rest as written
        c.args(["/D", "/S", "/C"]).raw_arg(format!("\"{line}\""));
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new("sh");
        c.args(["-c", line]);
        c
    };
    cmd.current_dir(repo_root)
        .env(crate::engines::C3_CHILD_ENV, "1");
    cmd
}

/// Run one check under `deadline`: `Ok(())` when it exits 0, else why it failed.
pub fn run_check(line: &str, repo_root: &Path, deadline: Duration) -> Result<(), CheckFailure> {
    let cmd = shell_command(line, repo_root);
    let outcome = crate::bounded::within(deadline, move || {
        Some(crate::bounded::run_registered(cmd).map_err(|e| e.to_string()))
    });
    let failure = |code, timed_out, start_error: String, tail| CheckFailure {
        name: line.to_string(),
        code,
        timed_out,
        start_error,
        tail,
    };
    match outcome {
        None => Err(failure(None, true, String::new(), Vec::new())),
        Some(Err(e)) => Err(failure(None, false, e, Vec::new())),
        Some(Ok(out)) if out.status.success() => Ok(()),
        Some(Ok(out)) => Err(failure(
            out.status.code(),
            false,
            String::new(),
            output_tail(&out.stdout, &out.stderr),
        )),
    }
}

/// Run `checks` in order (each under [`CHECK_DEADLINE`]); the first that fails ends the run.
pub fn run_all(checks: &[String], repo_root: &Path) -> Result<(), CheckFailure> {
    for line in checks {
        run_check(line, repo_root, CHECK_DEADLINE)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_read_from_the_finish_checks_key() {
        assert_eq!(parse_checks("{}").unwrap(), Vec::<String>::new());
        assert_eq!(
            parse_checks(r#"{"other":1,"finish_checks":["cargo fmt --all -- --check"," x "]}"#)
                .unwrap(),
            vec!["cargo fmt --all -- --check".to_string(), "x".to_string()]
        );
        assert_eq!(
            parse_checks(r#"{"finish_checks":[]}"#).unwrap(),
            Vec::<String>::new()
        );
        for bad in [
            "not json",
            "[]",
            r#"{"finish_checks":"cargo fmt"}"#,
            r#"{"finish_checks":[""]}"#,
            r#"{"finish_checks":[3]}"#,
            r#"{"finish_checks":["a\nb"]}"#,
        ] {
            assert!(parse_checks(bad).is_err(), "{bad} was accepted");
        }
    }

    #[test]
    fn the_tail_is_the_last_twenty_lines_of_stdout_then_stderr() {
        let out: String = (1..=15).map(|i| format!("out {i}\n")).collect();
        let err: String = (1..=10).map(|i| format!("err {i}  \n\n")).collect();
        let tail = output_tail(out.as_bytes(), err.as_bytes());
        assert_eq!(tail.len(), TAIL_LINES);
        assert_eq!(tail.first().map(String::as_str), Some("out 6"));
        assert_eq!(tail.last().map(String::as_str), Some("err 10"));
        assert_eq!(output_tail(b"", b""), Vec::<String>::new());
    }

    #[test]
    fn a_failure_names_the_check_its_exit_code_and_its_output() {
        let f = CheckFailure {
            name: "cargo fmt --all -- --check".into(),
            code: Some(1),
            timed_out: false,
            start_error: String::new(),
            tail: vec!["Diff in src/a.rs".into()],
        };
        let s = f.to_string();
        assert!(
            s.contains("`cargo fmt --all -- --check` failed with exit code 1"),
            "{s}"
        );
        assert!(s.contains("nothing was staged or committed"), "{s}");
        assert!(s.contains("  Diff in src/a.rs"), "{s}");
        assert!(s.contains("--skip-checks"), "{s}");
        let t = CheckFailure {
            timed_out: true,
            code: None,
            tail: vec![],
            ..f
        };
        assert!(t.to_string().contains("was stopped after 120 s"), "{t}");
    }

    // A check runs in the given directory through the shell, marked as a C3 child; a non-zero exit
    // is a failure with its code and output; a check past its deadline is stopped.
    #[test]
    fn checks_run_in_the_repository_marked_as_a_child() {
        let dir = std::env::temp_dir().join(format!(
            "c3-finish-checks-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let (mark, fail, slow) = if cfg!(windows) {
            (
                ">mark.txt echo child=%C3_CHILD%",
                "echo first& echo second& exit /b 3",
                "ping -n 30 127.0.0.1 >nul",
            )
        } else {
            (
                "echo child=$C3_CHILD > mark.txt",
                "echo first; echo second; exit 3",
                "sleep 30",
            )
        };
        assert_eq!(run_check(mark, &dir, Duration::from_secs(60)), Ok(()));
        let seen = std::fs::read_to_string(dir.join("mark.txt")).unwrap();
        assert_eq!(seen.trim(), "child=1");
        let f = run_check(fail, &dir, Duration::from_secs(60)).unwrap_err();
        assert_eq!(f.code, Some(3));
        assert_eq!(f.tail, vec!["first".to_string(), "second".to_string()]);
        let t = std::time::Instant::now();
        let f = run_check(slow, &dir, Duration::from_millis(1500)).unwrap_err();
        assert!(f.timed_out, "{f:?}");
        assert!(t.elapsed() < Duration::from_secs(20), "{:?}", t.elapsed());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
