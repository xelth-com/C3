//! The codex engine adapter: run one `codex exec` turn and turn its event stream into an
//! [`AttemptOutcome`].
//!
//! The argv is the plugin's (built by [`c3_core::engine::SubprocessEngine`]); the prompt goes
//! on stdin; the last agent message is read from the `-o` file. The JSONL event stream
//! ([`super::subprocess`] captured it to `.events.jsonl`) yields the thread id
//! (`Get-ThreadIdFromEvents`), any failure text (`Get-ErrorFromEvents`) and the token usage
//! (`Get-UsageFromEvents`). A non-zero exit or an `error`/`turn.failed` event is a classified
//! [`ProviderFailure`]; a clean exit with a reply is a [`Reply`] (structured when the reply
//! parses and validates); a timeout is [`AttemptOutcome::TimedOut`] with the salvaged partial.

use std::path::{Path, PathBuf};

use c3_core::engine::{
    AttemptOutcome, Capabilities, ConversationId, ConversationTrust, Engine, EngineError,
    EngineKind, LaunchPlan, RawReply, Reply, Request, StructuredReply, SubprocessEngine, TurnKind,
    TurnRequest,
};
use c3_core::health::{convert_from_provider_error_text, provider_failure_class};
use c3_core::ledger::{ProviderFailure, Usage};

use super::subprocess::{run_turn, SpawnRequest};

/// Where a turn's captured streams go. The orchestrator names the files (they live next to
/// the handoff); the adapter picks the pair for the turn being run.
#[derive(Debug, Clone, Default)]
pub struct TurnFiles {
    pub events: PathBuf,
    pub stderr: PathBuf,
}

/// The runtime codex engine: the resolved launcher, the working directory (repo root) and
/// the per-turn stream files. `plan`/`precheck`/`capabilities` delegate to the core
/// [`SubprocessEngine`] so the argv stays byte-identical.
#[derive(Clone)]
pub struct CodexEngine {
    pub launcher: String,
    pub cwd: PathBuf,
    /// The primary turn's streams.
    pub primary: TurnFiles,
    /// The secondary turn's streams (format repair / timeout continuation), when one runs.
    pub secondary: TurnFiles,
    /// (wave 26b, D12) The stall cut in seconds for the PRIMARY turn (`0` = off).
    pub stall_sec: i64,
    /// (wave 26b, D10) The operator's kick file for the PRIMARY turn (`None` = no kick watch).
    pub kick_path: Option<PathBuf>,
    /// Called once per turn, right after the child spawns and before it is waited on, with the
    /// child pid and its start time — the orchestrator flips the recovery record `launching`
    /// -> `running` (`codex-consult.ps1:3330`). `None` leaves the record `launching`.
    pub on_running: Option<std::sync::Arc<dyn Fn(u32, String) + Send + Sync>>,
}

/// (wave 26c, D3) One codex `--json` line's effect on the tool-call-in-flight count: `+1` when a
/// `command_execution` / `mcp_tool_call` / `web_search` item STARTS, `-1` when one COMPLETES,
/// `0` otherwise. Used to suspend the stall timer while a tool call runs.
pub fn codex_tool_delta(line: &str) -> i64 {
    let v: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return 0,
    };
    let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let item_type = v
        .get("item")
        .and_then(|i| i.get("type"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    if !matches!(
        item_type,
        "command_execution" | "mcp_tool_call" | "web_search"
    ) {
        return 0;
    }
    match ty {
        "item.started" => 1,
        "item.completed" => -1,
        _ => 0,
    }
}

impl CodexEngine {
    fn inner(&self) -> SubprocessEngine {
        SubprocessEngine::new(EngineKind::Codex)
    }

    fn files_for(&self, kind: TurnKind) -> &TurnFiles {
        match kind {
            TurnKind::Primary => &self.primary,
            _ => &self.secondary,
        }
    }

    fn run_argv(&self, turn: &TurnRequest) -> Result<Vec<String>, EngineError> {
        match self.inner().plan(&turn.request)? {
            LaunchPlan::Subprocess(a) => Ok(a.args),
            LaunchPlan::Http(_) => Err(EngineError::MissingField("subprocess argv")),
        }
    }

    fn execute(&self, turn: &TurnRequest) -> Result<AttemptOutcome, EngineError> {
        let args = self.run_argv(turn)?;
        let files = self.files_for(turn.kind);
        let timeout = std::time::Duration::from_secs_f64(turn.request.timeout_sec.max(0.0));
        // The stall cut and the operator's kick watch only the primary turn (a continuation /
        // format-repair turn takes neither).
        let is_primary = matches!(turn.kind, TurnKind::Primary);
        let td = |l: &str| codex_tool_delta(l);
        let spawn = SpawnRequest {
            launcher: &self.launcher,
            argv: &args,
            cwd: &self.cwd,
            prompt_delivery: self.inner().capabilities().prompt_delivery,
            stdin_text: &turn.request.prompt,
            events_path: &files.events,
            stderr_path: &files.stderr,
            timeout,
            stall_sec: if is_primary { self.stall_sec } else { 0 },
            // (wave 26c, D1) the caller sets `kick_path` only where it should be watched: the
            // primary turn always, and the FORMAT REPAIR turn (a kicked repair leaves the first
            // reply standing). A continuation/denial retry passes `None`.
            kick_path: self.kick_path.as_deref(),
            tool_delta: Some(&td),
            on_running: self
                .on_running
                .as_ref()
                .map(|a| a.as_ref() as &dyn Fn(u32, String)),
        };
        let result = run_turn(&spawn);

        if !result.started {
            return Ok(AttemptOutcome::LaunchFailed {
                child_exists: false,
                message: result
                    .error
                    .unwrap_or_else(|| "the codex process did not start".into()),
            });
        }

        let events_text = read_text(&files.events);
        let thread = parse_thread_id(&events_text);
        let conversation = |trust_verified: bool| -> ConversationTrust {
            if thread.is_empty() {
                ConversationTrust::None
            } else if trust_verified {
                ConversationTrust::Verified(ConversationId(thread.clone()))
            } else {
                ConversationTrust::Candidate(ConversationId(thread.clone()))
            }
        };

        use super::subprocess::TurnStop;
        use c3_core::engine::StopKind;
        match result.stop {
            TurnStop::Timeout => {
                return Ok(AttemptOutcome::TimedOut {
                    partial: salvage_partial(&events_text),
                    survivors: result.survivors,
                    conversation: conversation(false),
                    wall_seconds: result.wall_seconds,
                });
            }
            TurnStop::Stall => {
                return Ok(AttemptOutcome::Stopped {
                    kind: StopKind::Stall {
                        silent_seconds: result.silent_seconds,
                        tool_open_seconds: result.tool_open_seconds,
                        last_event: result.last_event.clone(),
                    },
                    partial: salvage_partial(&events_text),
                    survivors: result.survivors,
                    conversation: conversation(false),
                    wall_seconds: result.wall_seconds,
                });
            }
            TurnStop::Kick => {
                return Ok(AttemptOutcome::Stopped {
                    kind: StopKind::Kick,
                    partial: salvage_partial(&events_text),
                    survivors: result.survivors,
                    conversation: conversation(false),
                    wall_seconds: result.wall_seconds,
                });
            }
            TurnStop::Exited => {}
        }

        let usage = parse_usage(&events_text);
        let event_error = parse_error(&events_text);
        let clean_exit = result.exit_code == Some(0);

        if !clean_exit || !event_error.is_empty() {
            // The failure text: the event stream's error first (codex reports failures there),
            // else its stderr.
            let source = if !event_error.is_empty() {
                event_error
            } else {
                result.stderr.clone()
            };
            let (code, message) = convert_from_provider_error_text(&source);
            let class = provider_failure_class(&message);
            return Ok(AttemptOutcome::ProviderFailure {
                failure: ProviderFailure {
                    class,
                    code,
                    message,
                    ..Default::default()
                },
                exit_code: result.exit_code,
            });
        }

        let raw_text = read_text(&files.events_reply(turn));
        let structured = parse_structured(&raw_text);
        Ok(AttemptOutcome::Completed(Reply {
            raw_text,
            structured,
            events_path: files.events.clone(),
            usage,
            wall_seconds: result.wall_seconds,
            conversation: conversation(true),
        }))
    }
}

impl TurnFiles {
    // The reply text is the `-o` file named in the request; a tiny helper keeps `execute`
    // readable when reading it per turn.
    fn events_reply(&self, turn: &TurnRequest) -> PathBuf {
        turn.request
            .output_last_message
            .clone()
            .unwrap_or_else(|| self.events.clone())
    }
}

impl Engine for CodexEngine {
    fn capabilities(&self) -> Capabilities {
        self.inner().capabilities()
    }

    fn plan(&self, request: &Request) -> Result<LaunchPlan, EngineError> {
        self.inner().plan(request)
    }

    fn precheck(&self, turn: &TurnRequest) -> Result<(), EngineError> {
        self.inner().precheck(turn)
    }

    fn run(&self, turn: &TurnRequest) -> Result<AttemptOutcome, EngineError> {
        self.execute(turn)
    }

    fn continue_turn(&self, turn: &TurnRequest) -> Result<AttemptOutcome, EngineError> {
        self.execute(turn)
    }
}

// --------------------------------------------------------------------------- event parsing

const SESSION_START_EVENTS: [&str; 2] = ["session_configured", "session.started"];
const UUID_RE: &str =
    r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$";

fn is_uuid(s: &str) -> bool {
    regex::Regex::new(UUID_RE).unwrap().is_match(s)
}

/// The thread id from the event stream (`Get-ThreadIdFromEvents`): the `thread.started`
/// event's `thread_id`, else a top-level session-start event's id, else the older
/// `{"msg":{...}}`-wrapped shape.
pub fn parse_thread_id(events_text: &str) -> String {
    for line in events_text.split(['\r', '\n']) {
        let trimmed = line.trim();
        if !trimmed.starts_with('{') {
            continue;
        }
        let obj: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ty = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if ty == "thread.started" {
            if let Some(t) = obj.get("thread_id").and_then(|v| v.as_str()) {
                if is_uuid(t) {
                    return t.to_string();
                }
            }
        }
        if SESSION_START_EVENTS.contains(&ty) {
            for field in ["thread_id", "session_id", "conversation_id"] {
                if let Some(v) = obj.get(field).and_then(|v| v.as_str()) {
                    if is_uuid(v) {
                        return v.to_string();
                    }
                }
            }
        }
        if let Some(msg) = obj.get("msg") {
            let mty = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if SESSION_START_EVENTS.contains(&mty) {
                for field in ["session_id", "thread_id", "conversation_id"] {
                    if let Some(v) = msg.get(field).and_then(|v| v.as_str()) {
                        if is_uuid(v) {
                            return v.to_string();
                        }
                    }
                }
            }
        }
    }
    String::new()
}

/// Verify (or nominate) a thread from codex's rollout files when the event stream named none
/// (`Find-ThreadInRollouts`, `codex-consult.ps1:1289`). Rollouts live at
/// `<codex home>/sessions/<yyyy>/<mm>/<dd>/rollout-*.jsonl`; consider the ones written since
/// the run started (a 5 s slack), newest first. A file whose name carries a uuid AND whose
/// content contains this run's consultation id verifies that uuid as the thread; otherwise the
/// newest uuid becomes an (unverified) candidate. Returns `(thread, candidate)` - at most one
/// is non-empty.
pub fn find_thread_in_rollouts(
    codex_home: &str,
    started_at: chrono::DateTime<chrono::Utc>,
    consult_id: &str,
) -> (String, String) {
    use chrono::Datelike;
    if codex_home.is_empty() {
        return (String::new(), String::new());
    }
    let root = Path::new(codex_home).join("sessions");
    if !root.is_dir() {
        return (String::new(), String::new());
    }
    // The day directories for the run's start and now (the run may cross midnight). Codex (and the
    // fake, and the plugin's `Get-Date`) name the `sessions/YYYY/MM/DD` dirs by LOCAL date, so the
    // day-dir names must be built in local time — a UTC date would miss the files whenever the run
    // straddles the local-vs-UTC midnight boundary. The mtime `cutoff` below stays an instant
    // comparison and is unaffected.
    let now = chrono::Local::now();
    let start_local = started_at.with_timezone(&chrono::Local);
    let mut days: Vec<PathBuf> = Vec::new();
    for d in [start_local, now] {
        let p = root
            .join(format!("{:04}", d.year()))
            .join(format!("{:02}", d.month()))
            .join(format!("{:02}", d.day()));
        if !days.contains(&p) {
            days.push(p);
        }
    }
    let uuid_in_name = regex::Regex::new(
        r"([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})",
    )
    .unwrap();
    let cutoff = started_at - chrono::Duration::seconds(5);
    // Collect rollout files with their modified time, then sort newest-first.
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for day in &days {
        let rd = match std::fs::read_dir(day) {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !(name.starts_with("rollout-") && name.ends_with(".jsonl")) {
                continue;
            }
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
            let mtime_utc: chrono::DateTime<chrono::Utc> = mtime.into();
            if mtime_utc < cutoff {
                continue;
            }
            files.push((mtime, entry.path()));
        }
    }
    files.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
    let mut candidate = String::new();
    for (_, path) in files {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let uuid = match uuid_in_name.captures(&name) {
            Some(c) => c.get(1).unwrap().as_str().to_string(),
            None => continue,
        };
        if candidate.is_empty() {
            candidate = uuid.clone();
        }
        if !consult_id.is_empty() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if text.to_lowercase().contains(&consult_id.to_lowercase()) {
                    return (uuid, String::new());
                }
            }
        }
    }
    (String::new(), candidate)
}

/// The failure text from the event stream (`Get-ErrorFromEvents`): the last `error`
/// message or `turn.failed` error message, whitespace-collapsed.
pub fn parse_error(events_text: &str) -> String {
    let mut found = String::new();
    for line in events_text.split(['\r', '\n']) {
        let trimmed = line.trim();
        if !trimmed.starts_with('{') {
            continue;
        }
        let obj: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ty = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if ty == "error" {
            if let Some(m) = obj.get("message").and_then(|v| v.as_str()) {
                if !m.is_empty() {
                    found = m.to_string();
                }
            }
        } else if ty == "turn.failed" {
            if let Some(m) = obj
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|v| v.as_str())
            {
                if !m.is_empty() {
                    found = m.to_string();
                }
            }
        }
    }
    c3_core::one_line(&found)
}

/// Token usage from the last `turn.completed` event (`Get-UsageFromEvents`).
pub fn parse_usage(events_text: &str) -> Option<Usage> {
    let mut usage = None;
    for line in events_text.split(['\r', '\n']) {
        if !line.contains("turn.completed") {
            continue;
        }
        let trimmed = line.trim();
        if !trimmed.starts_with('{') {
            continue;
        }
        let obj: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if obj.get("type").and_then(|v| v.as_str()) != Some("turn.completed") {
            continue;
        }
        let u = obj.get("usage");
        let get = |name: &str| -> i64 {
            u.and_then(|u| u.get(name))
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
        };
        usage = Some(Usage {
            input_tokens: get("input_tokens"),
            cached_input_tokens: get("cached_input_tokens"),
            output_tokens: get("output_tokens"),
            reasoning_output_tokens: get("reasoning_output_tokens"),
            total_tokens: None,
            extra: Default::default(),
        });
    }
    usage
}

/// Parse the reply text into a validated [`StructuredReply`]; `None` when it is not one bare
/// JSON object or fails validation (the orchestrator then decides prose/repair).
pub fn parse_structured(raw_text: &str) -> Option<StructuredReply> {
    // Strip a single ```lang ... ``` fence first, exactly like the plugin's
    // `ConvertFrom-StructuredReply` (a fenced JSON object is a valid structured reply).
    let body = strip_reply_fence(raw_text.trim());
    let trimmed = body.trim();
    if !trimmed.starts_with('{') {
        return None;
    }
    let raw: RawReply = serde_json::from_str(trimmed).ok()?;
    StructuredReply::try_from(raw).ok()
}

/// Strip a single leading/trailing ```lang fence, returning the body; the input unchanged when
/// it is not one fenced block. Mirrors `ConvertFrom-StructuredReply`'s fence net.
fn strip_reply_fence(t: &str) -> String {
    static FENCE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = FENCE.get_or_init(|| {
        regex::Regex::new(r"(?s)^```[A-Za-z0-9_-]*[ \t]*\r?\n(.*?)\r?\n[ \t]*```$").unwrap()
    });
    match re.captures(t) {
        Some(c) => c
            .get(1)
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_else(|| t.to_string()),
        None => t.to_string(),
    }
}

/// Salvage the reasoning / agent-message / command text of a killed turn's items, for the
/// `.partial.md` file. `None` when nothing was emitted.
pub fn salvage_partial(events_text: &str) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for line in events_text.split(['\r', '\n']) {
        let trimmed = line.trim();
        if !trimmed.starts_with('{') {
            continue;
        }
        let obj: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if obj.get("type").and_then(|v| v.as_str()) != Some("item.completed") {
            continue;
        }
        let item = match obj.get("item") {
            Some(i) => i,
            None => continue,
        };
        let ity = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match ity {
            "reasoning" | "agent_message" => {
                if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                    if !t.is_empty() {
                        parts.push(t.to_string());
                    }
                }
            }
            "command_execution" => {
                if let Some(c) = item.get("command").and_then(|v| v.as_str()) {
                    parts.push(format!("$ {c}"));
                }
            }
            _ => {}
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

fn read_text(path: &Path) -> String {
    std::fs::read(path)
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_delta_tracks_command_and_search_and_mcp() {
        // A tool call STARTS (+1) and COMPLETES (-1); other items are neutral.
        for kind in ["command_execution", "mcp_tool_call", "web_search"] {
            let start = format!(r#"{{"type":"item.started","item":{{"type":"{kind}"}}}}"#);
            let done = format!(r#"{{"type":"item.completed","item":{{"type":"{kind}"}}}}"#);
            assert_eq!(codex_tool_delta(&start), 1, "{kind} start");
            assert_eq!(codex_tool_delta(&done), -1, "{kind} completed");
        }
        // An agent message / reasoning item is not a tool call.
        assert_eq!(
            codex_tool_delta(
                r#"{"type":"item.completed","item":{"type":"agent_message","text":"hi"}}"#
            ),
            0
        );
        assert_eq!(codex_tool_delta("not json"), 0);
    }

    const STREAM: &str = concat!(
        r#"{"type":"thread.started","thread_id":"11111111-2222-3333-4444-555555555555"}"#,
        "\n",
        r#"{"type":"turn.started"}"#,
        "\n",
        r#"{"type":"turn.completed","usage":{"input_tokens":1000,"cached_input_tokens":200,"cache_write_input_tokens":0,"output_tokens":300,"reasoning_output_tokens":40}}"#,
        "\n"
    );

    #[test]
    fn thread_id_from_thread_started() {
        assert_eq!(
            parse_thread_id(STREAM),
            "11111111-2222-3333-4444-555555555555"
        );
    }

    #[test]
    fn thread_id_ignores_foreign_non_uuid() {
        let s = r#"{"type":"turn.started","session_id":"not-a-uuid"}"#;
        assert_eq!(parse_thread_id(s), "");
    }

    #[test]
    fn drift_net_session_configured() {
        let s =
            r#"{"type":"session_configured","session_id":"11111111-2222-3333-4444-555555555555"}"#;
        assert_eq!(parse_thread_id(s), "11111111-2222-3333-4444-555555555555");
    }

    #[test]
    fn usage_from_turn_completed() {
        let u = parse_usage(STREAM).unwrap();
        assert_eq!(u.input_tokens, 1000);
        assert_eq!(u.cached_input_tokens, 200);
        assert_eq!(u.output_tokens, 300);
        assert_eq!(u.reasoning_output_tokens, 40);
        assert_eq!(u.total_tokens, None);
    }

    #[test]
    fn error_from_events_last_wins() {
        let s = concat!(
            r#"{"type":"error","message":"first"}"#,
            "\n",
            r#"{"type":"turn.failed","error":{"message":"You've hit your usage   limit."}}"#,
        );
        assert_eq!(parse_error(s), "You've hit your usage limit.");
    }

    #[test]
    fn structured_parse_valid_and_prose() {
        let json = r#"{"schema_version":"1","verdict":"ACCEPT","verdict_reason":"ok","reply_markdown":"body","findings":[],"prior_findings":[],"unproven":[],"first_run_checklist":[]}"#;
        assert!(parse_structured(json).is_some());
        assert!(parse_structured("This is prose, not JSON.").is_none());
        // valid JSON object but not a valid reply (bad verdict) → None
        assert!(parse_structured(r#"{"schema_version":"1","verdict":"MAYBE"}"#).is_none());
    }

    #[test]
    fn salvage_collects_items() {
        let s = concat!(
            r#"{"type":"item.completed","item":{"id":"i0","type":"reasoning","text":"thinking"}}"#,
            "\n",
            r#"{"type":"item.completed","item":{"id":"i2","type":"agent_message","text":"partial answer"}}"#,
        );
        let salvage = salvage_partial(s).unwrap();
        assert!(salvage.contains("thinking"));
        assert!(salvage.contains("partial answer"));
    }
}
