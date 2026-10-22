//! The worker-engine seam (work mode v1.1, `docs/work-mode.md`): a work run may drive the
//! `claude`, `codex` or `muse` engine. The record, the detach/status/wait lifecycle and the
//! quota alarm in [`super`] are engine-agnostic; this module owns the three things that differ
//! per engine — the child argv (first launch and quota resume), how the prompt reaches the child
//! (claude/codex on stdin, muse through a `--prompt-file`), the summary read out of the child's
//! event stream (`report.md`, turns, tool calls, usage), the usage-limit classification, and the
//! child session id a resume continues.
//!
//! Each engine reuses the review adapter's own parsers so a worker and a reviewer read one event
//! stream the same way: claude [`crate::engines::claude`], codex [`crate::engines::codex`], muse
//! [`crate::engines::muse`].

use std::collections::BTreeMap;

use c3_core::engine::PromptDelivery;
use chrono::{DateTime, Utc};
use serde_json::Value;

use super::record::summarize_events;

/// The engines a work run may drive (the roster entry's `engine`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerEngine {
    Claude,
    Codex,
    Muse,
}

/// The engines a worker entry may run on, for the refusal wording.
pub const WORKER_ENGINE_LIST: &str = "claude, codex or muse";

impl WorkerEngine {
    /// The engine of a roster entry's `engine` string, or `None` when it is not a worker engine.
    pub fn parse(engine: &str) -> Option<WorkerEngine> {
        match engine {
            "claude" => Some(WorkerEngine::Claude),
            "codex" => Some(WorkerEngine::Codex),
            "muse" => Some(WorkerEngine::Muse),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            WorkerEngine::Claude => "claude",
            WorkerEngine::Codex => "codex",
            WorkerEngine::Muse => "muse",
        }
    }

    /// How the prompt reaches the child: claude/codex on stdin, muse through a `--prompt-file`.
    pub fn prompt_delivery(&self) -> PromptDelivery {
        match self {
            WorkerEngine::Claude | WorkerEngine::Codex => PromptDelivery::Stdin,
            WorkerEngine::Muse => PromptDelivery::PromptFile,
        }
    }
}

// --------------------------------------------------------------------------- the argv

/// (codex) `exec --sandbox workspace-write --color never --json -C <root> -m <model>
/// [-c model_reasoning_effort="<e>"] [resume <thread>] -`. Mirrors [`c3_core::engine`]'s review
/// `plan_codex` (the effort is the same `-c model_reasoning_effort` TOML string) minus the
/// reviewer-only `-o`/`--output-schema`/`model_provider`, plus the worker's write sandbox and
/// `-C <repo root>`. The prompt travels on stdin (`-`); on a resume `thread` is the first launch's
/// codex thread id.
pub fn codex_work_args(
    model: &str,
    effort: Option<&str>,
    repo_root: &str,
    resume_thread: Option<&str>,
) -> Vec<String> {
    let mut a: Vec<String> = [
        "exec",
        "--sandbox",
        "workspace-write",
        "--color",
        "never",
        "--json",
        "-C",
        repo_root,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if !model.trim().is_empty() {
        a.push("-m".into());
        a.push(model.to_string());
    }
    if let Some(e) = effort.filter(|e| !e.trim().is_empty()) {
        a.push("-c".into());
        a.push(format!(
            "model_reasoning_effort={}",
            c3_core::engine::toml_basic_string(e)
        ));
    }
    if let Some(thread) = resume_thread.filter(|t| !t.trim().is_empty()) {
        a.push("resume".into());
        a.push(thread.to_string());
    }
    a.push("-".into());
    a
}

/// (muse) `exec --json --prompt-file <file> --model <id> [--reasoning-effort <e>]
/// --workspace <root> --approval-mode never --trust-workspace --disable-sandbox
/// --session-id <uuid>`. The review `plan_muse` disables write/shell (a reviewer never edits);
/// a worker does the opposite — writes and the shell are left ON, approvals are withheld
/// (`--approval-mode never`), the workspace's rules are trusted, and the shell sandbox is
/// disabled (its Windows enforcement needs an elevated setup that is not present, so an enabled
/// sandbox fails every `cargo`/`dir`). The session id is minted up front (as for claude), so a
/// resume continues it with the SAME `--session-id` and only a new `--prompt-file`.
pub fn muse_work_args(
    model: &str,
    effort: Option<&str>,
    repo_root: &str,
    prompt_file: &str,
    session_id: &str,
) -> Vec<String> {
    let mut a: Vec<String> = [
        "exec",
        "--json",
        "--prompt-file",
        prompt_file,
        "--model",
        model,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if let Some(e) = effort.filter(|e| !e.trim().is_empty()) {
        a.push("--reasoning-effort".into());
        a.push(e.to_string());
    }
    a.push("--workspace".into());
    a.push(repo_root.to_string());
    a.push("--approval-mode".into());
    a.push("never".into());
    a.push("--trust-workspace".into());
    a.push("--disable-sandbox".into());
    a.push("--session-id".into());
    a.push(session_id.to_string());
    a
}

/// Turn a stored first-launch argv into its quota-resume argv for `engine`, given the run's child
/// `session_id` (claude: the minted id; codex: the parsed thread; muse: the minted id) and the
/// resume prompt file (muse only). Returns `None` when the record cannot be resumed (a codex run
/// that never named a thread). The flags of the first launch are kept verbatim; only the session
/// handoff changes.
pub fn resume_args(
    engine: WorkerEngine,
    argv: &[String],
    session_id: &str,
    resume_prompt_file: &str,
) -> Option<Vec<String>> {
    match engine {
        // claude: `--session-id <x>` -> `--resume <x>`.
        WorkerEngine::Claude => {
            let mut out = Vec::with_capacity(argv.len());
            let mut i = 0;
            let mut swapped = false;
            while i < argv.len() {
                if argv[i] == "--session-id" && i + 1 < argv.len() {
                    out.push("--resume".to_string());
                    out.push(session_id.to_string());
                    i += 2;
                    swapped = true;
                } else {
                    out.push(argv[i].clone());
                    i += 1;
                }
            }
            swapped.then_some(out)
        }
        // codex: append `resume <thread>` before the trailing `-`.
        WorkerEngine::Codex => {
            if session_id.trim().is_empty() {
                return None;
            }
            // Drop a trailing stdin `-`, re-append it after the resume subcommand.
            let mut base: Vec<String> = argv.to_vec();
            let had_dash = base.last().map(|s| s == "-").unwrap_or(false);
            if had_dash {
                base.pop();
            }
            // A record that already carries a resume subcommand is reused as is.
            if base.iter().any(|a| a == "resume") {
                base.push("-".into());
                return Some(base);
            }
            base.push("resume".into());
            base.push(session_id.to_string());
            base.push("-".into());
            Some(base)
        }
        // muse: keep `--session-id <x>`, point `--prompt-file` at the resume prompt.
        WorkerEngine::Muse => {
            if !argv.iter().any(|a| a == "--session-id") {
                return None;
            }
            let mut out = Vec::with_capacity(argv.len());
            let mut i = 0;
            while i < argv.len() {
                if argv[i] == "--prompt-file" && i + 1 < argv.len() {
                    out.push("--prompt-file".to_string());
                    out.push(resume_prompt_file.to_string());
                    i += 2;
                } else {
                    out.push(argv[i].clone());
                    i += 1;
                }
            }
            Some(out)
        }
    }
}

// --------------------------------------------------------------------------- the summary

/// What a work run reads out of its child's event stream (engine-agnostic shape; the git diff
/// and `files_changed` are computed by [`super`] from git, not here).
#[derive(Debug, Clone, Default)]
pub struct WorkSummary {
    /// The final report text (`report.md`).
    pub report: String,
    pub turns: Option<i64>,
    pub tool_calls: BTreeMap<String, i64>,
    pub usage: Option<Value>,
}

/// Summarize the whole event stream of `engine` into a [`WorkSummary`].
pub fn summarize(engine: WorkerEngine, events_text: &str) -> WorkSummary {
    match engine {
        WorkerEngine::Claude => {
            let s = summarize_events(events_text);
            let report = if !s.result_text.trim().is_empty() {
                s.result_text
            } else {
                s.last_assistant_text
            };
            WorkSummary {
                report,
                turns: s
                    .num_turns
                    .or((s.assistant_count > 0).then_some(s.assistant_count)),
                tool_calls: s.tool_calls,
                usage: s.usage,
            }
        }
        WorkerEngine::Codex => summarize_codex(events_text),
        WorkerEngine::Muse => summarize_muse(events_text),
    }
}

fn summarize_codex(events_text: &str) -> WorkSummary {
    let mut report = String::new();
    let mut turns = 0i64;
    let mut tool_calls: BTreeMap<String, i64> = BTreeMap::new();
    for line in events_text.split(['\r', '\n']) {
        let t = line.trim();
        if !t.starts_with('{') {
            continue;
        }
        let Ok(obj) = serde_json::from_str::<Value>(t) else {
            continue;
        };
        let ty = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if ty == "turn.completed" {
            turns += 1;
        }
        if ty == "item.started" {
            if let Some(item) = obj.get("item") {
                let it = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if matches!(it, "command_execution" | "mcp_tool_call" | "web_search") {
                    *tool_calls.entry(it.to_string()).or_insert(0) += 1;
                }
            }
        }
        if ty == "item.completed" {
            if let Some(item) = obj.get("item") {
                if item.get("type").and_then(|v| v.as_str()) == Some("agent_message") {
                    if let Some(tx) = item.get("text").and_then(|v| v.as_str()) {
                        if !tx.trim().is_empty() {
                            report = tx.to_string();
                        }
                    }
                }
            }
        }
    }
    if report.trim().is_empty() {
        report = crate::engines::codex::salvage_partial(events_text).unwrap_or_default();
    }
    WorkSummary {
        report,
        turns: (turns > 0).then_some(turns),
        tool_calls,
        usage: crate::engines::codex::parse_usage(events_text).map(usage_to_value),
    }
}

fn summarize_muse(events_text: &str) -> WorkSummary {
    let ev = crate::engines::muse::read_muse_events(events_text, true);
    let report = if ev.terminal == "completed" && !ev.text.trim().is_empty() {
        ev.text.clone()
    } else {
        crate::engines::muse::muse_salvage(events_text).unwrap_or_else(|| ev.text.clone())
    };
    // tool calls: each `task.lifecycle.proposed` of a `tool.*` kind, by kind; `run.usage` carries
    // the model-call count (the nearest thing to a turn count) and the token usage.
    let mut tool_calls: BTreeMap<String, i64> = BTreeMap::new();
    let mut turns: Option<i64> = None;
    let mut usage: Option<Value> = None;
    for line in events_text.split(['\r', '\n']) {
        let t = line.trim();
        if !t.starts_with('{') {
            continue;
        }
        let Ok(obj) = serde_json::from_str::<Value>(t) else {
            continue;
        };
        let p_type = obj
            .get("payload_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let payload = obj.get("payload");
        if p_type == "task.lifecycle.proposed" {
            let kind = payload
                .and_then(|p| p.get("event"))
                .and_then(|e| e.get("task_kind"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if let Some(name) = kind.strip_prefix("tool.") {
                *tool_calls.entry(name.to_string()).or_insert(0) += 1;
            }
        } else if p_type == "run.usage" {
            if let Some(p) = payload {
                turns = p.get("calls").and_then(|v| v.as_i64()).or(turns);
                usage = muse_usage_to_value(p);
            }
        }
    }
    WorkSummary {
        report,
        turns,
        tool_calls,
        usage,
    }
}

fn usage_to_value(u: c3_core::ledger::Usage) -> Value {
    let mut m = serde_json::Map::new();
    m.insert("input_tokens".into(), Value::from(u.input_tokens));
    m.insert(
        "cached_input_tokens".into(),
        Value::from(u.cached_input_tokens),
    );
    m.insert("output_tokens".into(), Value::from(u.output_tokens));
    m.insert(
        "reasoning_output_tokens".into(),
        Value::from(u.reasoning_output_tokens),
    );
    Value::Object(m)
}

/// Sum the token columns of a muse `run.usage` payload's `models` array into a usage object.
fn muse_usage_to_value(payload: &Value) -> Option<Value> {
    let models = payload.get("models").and_then(|v| v.as_array())?;
    let mut input = 0i64;
    let mut cached = 0i64;
    let mut output = 0i64;
    let mut reasoning = 0i64;
    for m in models {
        input += m.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
        cached += m.get("cached_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
        output += m.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
        reasoning += m
            .get("reasoning_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
    }
    let mut map = serde_json::Map::new();
    map.insert("input_tokens".into(), Value::from(input));
    map.insert("cached_tokens".into(), Value::from(cached));
    map.insert("output_tokens".into(), Value::from(output));
    map.insert("reasoning_tokens".into(), Value::from(reasoning));
    Some(Value::Object(map))
}

// --------------------------------------------------------------------------- the quota evidence

/// The usage-limit wording codex/muse report in their event stream or stderr (the review engines
/// classify the same text through [`c3_core::health::provider_failure_class`]).
const WORK_QUOTA_RE: &str = r"(?i)usage limit|rate limit|rate-limit|quota|too many requests|429";

/// The usage-limit evidence of a finished attempt: the limit wording (`""` when none) and the
/// reset instant when the engine named one. claude reuses the review engine's `quota_evidence`
/// (one wording, one regex for a worker and a reviewer); codex/muse match [`WORK_QUOTA_RE`] on
/// the stream's error text and stderr (reset time: a 10–13 digit epoch in the text, else unknown
/// so [`super`] falls back to the quota window).
pub fn quota_evidence(
    engine: WorkerEngine,
    events_text: &str,
    stderr_text: &str,
) -> (String, Option<DateTime<Utc>>) {
    let stderr_lines: Vec<String> = stderr_text
        .split('\n')
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    match engine {
        WorkerEngine::Claude => {
            let ev = crate::engines::claude::read_claude_events(events_text, true);
            crate::engines::claude::quota_evidence(&ev, &stderr_lines)
        }
        WorkerEngine::Codex => {
            let err = crate::engines::codex::parse_error(events_text);
            text_quota(&err, &stderr_lines)
        }
        WorkerEngine::Muse => {
            let ev = crate::engines::muse::read_muse_events(events_text, true);
            let err = if ev.error.is_empty() {
                ev.reason
            } else {
                ev.error
            };
            text_quota(&err, &stderr_lines)
        }
    }
}

fn text_quota(err_text: &str, stderr_lines: &[String]) -> (String, Option<DateTime<Utc>>) {
    let re = regex::Regex::new(WORK_QUOTA_RE).expect("valid regex");
    let hit = if !err_text.is_empty() && re.is_match(err_text) {
        Some(err_text.to_string())
    } else {
        stderr_lines.iter().find(|l| re.is_match(l)).cloned()
    };
    let text = match hit {
        Some(t) => format!("usage limit: {}", c3_core::one_line(&t)),
        None => return (String::new(), None),
    };
    // A reset time, when the wording carries a 10–13 digit epoch (seconds or millis).
    let reset = regex::Regex::new(r"\b([0-9]{10,13})\b")
        .ok()
        .and_then(|r| r.captures(&text))
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
        .and_then(|digits| {
            let n: i64 = digits.parse().ok()?;
            let secs = if digits.len() >= 13 { n / 1000 } else { n };
            DateTime::<Utc>::from_timestamp(secs, 0)
        });
    (text, reset)
}

// --------------------------------------------------------------------------- the terminal outcome

/// (0.2.4, F89-8) The engine's terminal verdict read from its finished stream, independent of the
/// process exit code: a run is `done` only when the engine emitted its SUCCESSFUL terminal event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminal {
    /// The engine's successful terminal event (claude `result` with `is_error:false`; codex a
    /// `turn.completed`/`agent_message` with no failure; muse a `completed` terminal).
    Success,
    /// The engine's error terminal (claude `result` with `is_error:true`; codex a `turn.failed`
    /// or a parsed error; muse a non-`completed` terminal or an error).
    Error,
    /// No terminal event at all (an empty/truncated stream): completion evidence is missing.
    None,
}

/// The terminal verdict of `engine`'s event stream (F89-8): success requires the engine's own
/// successful terminal event, an error terminal is a failure, and an empty stream is `None`.
pub fn terminal_outcome(engine: WorkerEngine, events_text: &str) -> Terminal {
    match engine {
        WorkerEngine::Claude => {
            let s = summarize_events(events_text);
            if !s.has_result {
                Terminal::None
            } else if s.result_ok {
                // (0.2.4, F91-4) affirmative: is_error present and false, subtype success/absent.
                Terminal::Success
            } else {
                Terminal::Error
            }
        }
        WorkerEngine::Codex => {
            if !crate::engines::codex::parse_error(events_text).is_empty() {
                return Terminal::Error;
            }
            let mut saw_completed = false;
            for line in events_text.split(['\r', '\n']) {
                let t = line.trim();
                if !t.starts_with('{') {
                    continue;
                }
                let Ok(obj) = serde_json::from_str::<Value>(t) else {
                    continue;
                };
                // (0.2.4, F91-4) Only the successful turn terminal (`turn.completed`) counts as
                // success; an `agent_message` item alone is NOT a completion (it is used for report
                // extraction in `summarize_codex`, kept separate from classification here).
                match obj.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                    "turn.failed" => return Terminal::Error,
                    "turn.completed" => saw_completed = true,
                    _ => {}
                }
            }
            if saw_completed {
                Terminal::Success
            } else {
                Terminal::None
            }
        }
        WorkerEngine::Muse => {
            let ev = crate::engines::muse::read_muse_events(events_text, true);
            if ev.terminal == "completed" {
                Terminal::Success
            } else if !ev.error.is_empty() || !ev.terminal.is_empty() {
                Terminal::Error
            } else {
                Terminal::None
            }
        }
    }
}

// --------------------------------------------------------------------------- the session id

/// The child session id a resume continues, read from the finished event stream: codex names a
/// thread in its stream (parsed here); claude and muse mint theirs before the launch, so the
/// record already holds it and this returns `""`.
pub fn session_from_events(engine: WorkerEngine, events_text: &str) -> String {
    match engine {
        WorkerEngine::Codex => crate::engines::codex::parse_thread_id(events_text),
        WorkerEngine::Claude | WorkerEngine::Muse => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_first_and_resume_argv() {
        let a = codex_work_args("gpt-6.1-sol", Some("high"), "C:\\repo", None);
        assert_eq!(
            a.join(" "),
            "exec --sandbox workspace-write --color never --json -C C:\\repo -m gpt-6.1-sol -c model_reasoning_effort=\"high\" -"
        );
        // no effort -> no -c; the prompt still on stdin (`-`)
        let b = codex_work_args("gpt-6.1-sol", None, "/r", None);
        assert_eq!(
            b.join(" "),
            "exec --sandbox workspace-write --color never --json -C /r -m gpt-6.1-sol -"
        );
        // resume: `resume <thread>` before the trailing `-`, every other flag kept
        let r = resume_args(WorkerEngine::Codex, &a, "th-9", "").unwrap();
        assert!(
            r.join(" ")
                .ends_with("model_reasoning_effort=\"high\" resume th-9 -"),
            "{r:?}"
        );
        // a codex run that never named a thread cannot resume
        assert!(resume_args(WorkerEngine::Codex, &a, "", "").is_none());
    }

    #[test]
    fn muse_first_and_resume_argv() {
        let a = muse_work_args(
            "muse-spark-1.3-contributor",
            Some("medium"),
            "/r",
            "/r/.collab/t/work/01/brief.md",
            "sess-1",
        );
        assert_eq!(
            a.join(" "),
            "exec --json --prompt-file /r/.collab/t/work/01/brief.md --model muse-spark-1.3-contributor --reasoning-effort medium --workspace /r --approval-mode never --trust-workspace --disable-sandbox --session-id sess-1"
        );
        // resume keeps --session-id, points --prompt-file at the resume prompt
        let r = resume_args(
            WorkerEngine::Muse,
            &a,
            "sess-1",
            "/r/.collab/t/work/01/.resume-prompt.md",
        )
        .unwrap();
        assert!(r.iter().any(|x| x == "--session-id"), "{r:?}");
        assert!(
            r.windows(2).any(
                |w| w[0] == "--prompt-file" && w[1] == "/r/.collab/t/work/01/.resume-prompt.md"
            ),
            "{r:?}"
        );
        assert!(!r.iter().any(|x| x.ends_with("brief.md")), "{r:?}");
    }

    #[test]
    fn claude_resume_swaps_the_session_flag() {
        let argv: Vec<String> = ["-p", "--model", "glm-5.3", "--session-id", "s1"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let r = resume_args(WorkerEngine::Claude, &argv, "s1", "").unwrap();
        assert_eq!(r.join(" "), "-p --model glm-5.3 --resume s1");
    }

    #[test]
    fn codex_summary_reads_report_tools_and_turns() {
        let s = concat!(
            r#"{"type":"thread.started","thread_id":"11111111-2222-3333-4444-555555555555"}"#,
            "\n",
            r#"{"type":"item.started","item":{"id":"c1","type":"command_execution","command":"cargo test"}}"#,
            "\n",
            r#"{"type":"item.completed","item":{"id":"a1","type":"agent_message","text":"done; 3 passed"}}"#,
            "\n",
            r#"{"type":"turn.completed","usage":{"input_tokens":10,"output_tokens":5}}"#,
            "\n"
        );
        let sum = summarize(WorkerEngine::Codex, s);
        assert_eq!(sum.report, "done; 3 passed");
        assert_eq!(sum.turns, Some(1));
        assert_eq!(sum.tool_calls.get("command_execution"), Some(&1));
        assert_eq!(sum.usage.unwrap()["input_tokens"], 10);
        assert_eq!(
            session_from_events(WorkerEngine::Codex, s),
            "11111111-2222-3333-4444-555555555555"
        );
    }

    /// (0.2.4, F89-8) The terminal verdict is read from the stream, not the exit code: a successful
    /// result is `Success`, an error result is `Error`, an empty stream is `None`.
    #[test]
    fn terminal_outcome_reads_the_stream() {
        let ok = r#"{"type":"result","subtype":"success","is_error":false,"result":"done"}"#;
        assert_eq!(
            terminal_outcome(WorkerEngine::Claude, ok),
            Terminal::Success
        );
        let err = r#"{"type":"result","subtype":"error","is_error":true,"result":"boom"}"#;
        assert_eq!(terminal_outcome(WorkerEngine::Claude, err), Terminal::Error);
        // (F91-4) a result MISSING is_error is not success
        let bare = r#"{"type":"result","result":"done"}"#;
        assert_eq!(
            terminal_outcome(WorkerEngine::Claude, bare),
            Terminal::Error
        );
        assert_eq!(
            terminal_outcome(WorkerEngine::Claude, "{\"type\":\"system\"}"),
            Terminal::None
        );
        // (F91-4) codex: an agent_message item alone is NOT success; turn.completed is
        let msg = r#"{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}"#;
        assert_eq!(terminal_outcome(WorkerEngine::Codex, msg), Terminal::None);
        let done = r#"{"type":"turn.completed","usage":{}}"#;
        assert_eq!(
            terminal_outcome(WorkerEngine::Codex, done),
            Terminal::Success
        );
        assert_eq!(terminal_outcome(WorkerEngine::Codex, ""), Terminal::None);
    }

    #[test]
    fn codex_and_muse_quota_wording() {
        let codex = r#"{"type":"turn.failed","error":{"message":"You've hit your usage limit."}}"#;
        let (text, _) = quota_evidence(WorkerEngine::Codex, codex, "");
        assert!(text.contains("usage limit"), "{text}");
        // no quota wording -> empty
        let clean = r#"{"type":"turn.completed","usage":{}}"#;
        assert!(quota_evidence(WorkerEngine::Codex, clean, "").0.is_empty());
        // reset epoch in the text is parsed
        let (_, reset) = text_quota("usage limit; resets at 4102444800", &[]);
        assert!(reset.is_some());
    }
}
