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
const ROSTER: &str = r#"{"roster_version":1,"reviewers":[{"provider":"ZAI-claude","engine":"claude","model":"glm-5.3","auth":"endpoint","endpoint":{"base_url":"https://api.z.ai/api/anthropic","env_key":"WB_FAKE_ZAI_TOKEN"},"plan":"zai","roles":["worker"]},{"provider":"anthropic","engine":"claude","model":"claude-opus-5-5[1m]","auth":"subscription"}]}"#;
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
    assert!(t.contains("--allow 'Bash(git:*)' names git"), "{t}");
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
    assert!(t.contains("work 01-a-m (ZAI-claude/glm-5.3): done"), "{t}");
    assert_eq!(out.status.code(), Some(0), "{t}");

    // two done runs: still 0
    e.synthetic_run("02-b-m", "done");
    let out = e.c3(&["work", "--task", "t", "--status"], &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));

    // one died: 1
    e.synthetic_run("03-c-m", "died");
    let out = e.c3(&["work", "--task", "t", "--status"], &[]);
    let t = text(&out);
    assert!(t.contains("work 03-c-m (ZAI-claude/glm-5.3): died"), "{t}");
    assert_eq!(out.status.code(), Some(1), "{t}");

    // one running (a live pid): 2, and --id narrows away from it
    e.synthetic_run("04-d-m", "running");
    let out = e.c3(&["work", "--task", "t", "--status"], &[]);
    let t = text(&out);
    assert!(
        t.contains("work 04-d-m (ZAI-claude/glm-5.3): running"),
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
