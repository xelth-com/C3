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
//! [`TAIL_LINES`] lines of its output - (F45-5) of a stopped check too, what it printed before it
//! was stopped - and nothing is staged or committed. (F45-9) The whole phase has a ceiling,
//! [`CHECKS_CEILING`]: a check never runs past it, and past it the run refuses like a failed check
//! and names the checks that did not run. `--skip-checks` goes on without them and says so, in the
//! output and in the journal entry.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

/// The per-repository configuration file inside C3's project folder.
pub const CONFIG_FILE: &str = "config.json";

/// The key of the list in [`CONFIG_FILE`].
pub const CHECKS_KEY: &str = "finish_checks";

/// The deadline of one check.
pub const CHECK_DEADLINE: Duration = Duration::from_secs(120);

/// (F45-9) The ceiling of the whole checks phase.
pub const CHECKS_CEILING: Duration = Duration::from_secs(600);

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
    /// It was stopped at its deadline (`deadline`).
    pub timed_out: bool,
    /// The deadline it ran under: [`CHECK_DEADLINE`], or what was left of the phase's ceiling.
    pub deadline: Duration,
    /// (F45-9) Set when the phase's ceiling ended the run: the check was stopped at it
    /// (`timed_out`) or did not start at all.
    pub ceiling: Option<Box<CeilingReached>>,
    /// Why it could not be started (empty when it ran).
    pub start_error: String,
    /// The last [`TAIL_LINES`] lines of its output (stdout, then stderr) - of a stopped check, what
    /// it printed before it was stopped.
    pub tail: Vec<String>,
}

/// (F45-9) The checks phase reached its ceiling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CeilingReached {
    /// The ceiling of the phase.
    pub ceiling: Duration,
    /// The checks that did not run because of it.
    pub not_run: Vec<String>,
}

impl CheckFailure {
    /// A failure of the check `name` (no ceiling reached).
    fn of(name: &str, deadline: Duration) -> Self {
        CheckFailure {
            name: name.to_string(),
            code: None,
            timed_out: false,
            deadline,
            ceiling: None,
            start_error: String::new(),
            tail: Vec::new(),
        }
    }

    /// The ceiling was reached before this check could start.
    fn did_not_start(&self) -> bool {
        self.ceiling.is_some() && !self.timed_out && self.start_error.is_empty()
    }
}

impl std::fmt::Display for CheckFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ceiling = self.ceiling.as_deref();
        let how = if let (Some(c), true) = (ceiling, self.did_not_start()) {
            format!(
                "did not run: the checks phase reached its ceiling of {} s",
                c.ceiling.as_secs()
            )
        } else if let (Some(c), true) = (ceiling, self.timed_out) {
            format!(
                "was stopped after {} s, at the checks phase's ceiling of {} s (no exit code)",
                self.deadline.as_secs(),
                c.ceiling.as_secs()
            )
        } else if self.timed_out {
            format!(
                "was stopped after {} s (no exit code)",
                self.deadline.as_secs()
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
        if let Some(c) = ceiling.filter(|c| !c.not_run.is_empty()) {
            let names: Vec<String> = c.not_run.iter().map(|n| format!("`{n}`")).collect();
            writeln!(f, "not run: {}", names.join("; "))?;
        }
        let before = if self.timed_out {
            " before it was stopped"
        } else {
            ""
        };
        if self.did_not_start() {
            // nothing ran, nothing was printed
        } else if self.tail.is_empty() {
            writeln!(f, "(it printed nothing{before})")?;
        } else {
            writeln!(f, "its last {} line(s) of output{before}:", self.tail.len())?;
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

/// Run one check under `deadline`: `Ok(())` when it exits 0, else why it failed. (F45-5) Its output
/// is copied as it is read, so a check stopped at the deadline still shows what it printed.
pub fn run_check(line: &str, repo_root: &Path, deadline: Duration) -> Result<(), CheckFailure> {
    let cmd = shell_command(line, repo_root);
    let sink: crate::bounded::OutputSink = Arc::default();
    let copy = Arc::clone(&sink);
    let outcome = crate::bounded::within(deadline, move || {
        Some(crate::bounded::run_registered_into(cmd, copy).map_err(|e| e.to_string()))
    });
    let failure = CheckFailure::of(line, deadline);
    match outcome {
        None => {
            let printed = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
            Err(CheckFailure {
                timed_out: true,
                tail: output_tail(&printed.0, &printed.1),
                ..failure
            })
        }
        Some(Err(e)) => Err(CheckFailure {
            start_error: e,
            ..failure
        }),
        Some(Ok(out)) if out.status.success() => Ok(()),
        Some(Ok(out)) => Err(CheckFailure {
            code: out.status.code(),
            tail: output_tail(&out.stdout, &out.stderr),
            ..failure
        }),
    }
}

/// Run `checks` in order (each under [`CHECK_DEADLINE`], all under [`CHECKS_CEILING`]); the first
/// that fails ends the run.
pub fn run_all(checks: &[String], repo_root: &Path) -> Result<(), CheckFailure> {
    run_all_within(checks, repo_root, CHECK_DEADLINE, CHECKS_CEILING)
}

/// [`run_all`] with the deadline of one check and the ceiling of the phase given: a check runs
/// under `per_check` or what is left of `ceiling`, whichever is shorter; a check stopped at the
/// ceiling, or one the ceiling leaves no time to start, ends the run and names the checks that did
/// not run.
pub fn run_all_within(
    checks: &[String],
    repo_root: &Path,
    per_check: Duration,
    ceiling: Duration,
) -> Result<(), CheckFailure> {
    let started = std::time::Instant::now();
    for (i, line) in checks.iter().enumerate() {
        let left = ceiling.saturating_sub(started.elapsed());
        let reached = |not_run: &[String]| {
            Some(Box::new(CeilingReached {
                ceiling,
                not_run: not_run.to_vec(),
            }))
        };
        if left.is_zero() {
            return Err(CheckFailure {
                ceiling: reached(&checks[i..]),
                ..CheckFailure::of(line, Duration::ZERO)
            });
        }
        let cut = left < per_check;
        run_check(line, repo_root, left.min(per_check)).map_err(|f| {
            if f.timed_out && cut {
                CheckFailure {
                    ceiling: reached(&checks[i + 1..]),
                    ..f
                }
            } else {
                f
            }
        })?;
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
            code: Some(1),
            tail: vec!["Diff in src/a.rs".into()],
            ..CheckFailure::of("cargo fmt --all -- --check", CHECK_DEADLINE)
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

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "c3-finish-checks-{name}-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A check that prints two lines and then hangs.
    fn printing_then_hanging() -> &'static str {
        if cfg!(windows) {
            "echo started& echo still going& ping -n 30 127.0.0.1 >nul"
        } else {
            "echo started; echo still going; sleep 30"
        }
    }

    // (F45-5) A check stopped at its deadline reports the lines it printed before it was stopped,
    // not "it printed nothing".
    #[test]
    fn a_stopped_check_shows_what_it_printed_before_it_was_stopped() {
        let dir = scratch("tail");
        let f = run_check(printing_then_hanging(), &dir, Duration::from_millis(2500)).unwrap_err();
        assert!(f.timed_out, "{f:?}");
        assert_eq!(
            f.tail,
            vec!["started".to_string(), "still going".to_string()]
        );
        let s = f.to_string();
        assert!(s.contains("was stopped after 2 s (no exit code)"), "{s}");
        assert!(
            s.contains("line(s) of output before it was stopped:\n  started"),
            "{s}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // (F45-9) The checks phase has a ceiling: a check is stopped at it and the run refuses naming
    // the checks that did not run; with no time left a check does not start at all.
    #[test]
    fn the_checks_phase_refuses_at_its_ceiling_and_names_what_did_not_run() {
        let dir = scratch("ceiling");
        let checks = vec![
            printing_then_hanging().to_string(),
            "git --version".to_string(),
            "git version".to_string(),
        ];
        let t = std::time::Instant::now();
        let f = run_all_within(
            &checks,
            &dir,
            Duration::from_secs(60),
            Duration::from_millis(2500),
        )
        .unwrap_err();
        assert!(t.elapsed() < Duration::from_secs(20), "{:?}", t.elapsed());
        assert!(f.timed_out, "{f:?}");
        let c = f.ceiling.as_deref().expect("the ceiling ended the run");
        assert_eq!(c.not_run, checks[1..].to_vec());
        let s = f.to_string();
        assert!(s.contains("at the checks phase's ceiling of 2 s"), "{s}");
        assert!(s.contains("not run: `git --version`; `git version`"), "{s}");
        assert!(s.contains("  started"), "{s}");
        // no time left: nothing starts
        let f = run_all_within(&checks[1..], &dir, Duration::from_secs(60), Duration::ZERO)
            .unwrap_err();
        assert!(!f.timed_out && f.did_not_start(), "{f:?}");
        assert_eq!(f.name, "git --version");
        assert_eq!(
            f.ceiling.as_deref().map(|c| c.not_run.clone()),
            Some(checks[1..].to_vec())
        );
        let s = f.to_string();
        assert!(
            s.contains("did not run: the checks phase reached its ceiling"),
            "{s}"
        );
        assert!(!s.contains("printed nothing"), "{s}");
        // under the ceiling, a passing list passes
        assert_eq!(
            run_all_within(&checks[1..], &dir, Duration::from_secs(60), CHECKS_CEILING),
            Ok(())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
