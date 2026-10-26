//! The `grok` engine adapter: run one Grok Build CLI turn (`grok --prompt-file ...
//! --output-format streaming-json`) and turn its streaming-json event stream into an
//! [`AttemptOutcome`].
//!
//! The argv is the plugin's (built by [`c3_core::engine::SubprocessEngine`]); the prompt is the
//! `--prompt-file` named in the argv and stdin stays empty (like muse). The streaming-json records
//! ([`super::subprocess`] captured them to `.events.jsonl`) are parsed by [`read_grok_events`]
//! (the reply is the concatenated `text` events, the thread id is READ from the `end` event -
//! grok ignores a caller-supplied session id for a new conversation) and judged by
//! [`grok_turn_outcome`].
//!
//! Grok bills the SuperGrok subscription through the grok.com oauth sign-in; an xAI API key the
//! child would inherit (`XAI_API_KEY` / `GROK_CODE_XAI_API_KEY`) bills per token. The adapter's
//! BILLING GUARD removes those variables from the child's environment ([`grok_child_env`]) unless
//! the operator opts in with `C3_GROK_ALLOW_API_KEY=1`. Grok has no denial retry (its write/shell
//! tools are disallowed); read-only is enforced by the orchestrator's [`super::tree_check`].

use std::path::{Path, PathBuf};

use c3_core::engine::{
    AttemptOutcome, Capabilities, ConversationId, ConversationTrust, Engine, EngineError,
    EngineKind, LaunchPlan, Mode, RawReply, Reply, Request, StructuredReply, SubprocessEngine,
    TurnKind, TurnRequest, GROK_ALLOW_API_KEY_VAR, GROK_API_KEY_VARS,
};
use c3_core::health::{convert_from_provider_error_text, provider_failure_class};
use c3_core::ledger::Usage;
use serde_json::Value;

use super::codex::TurnFiles;
use super::subprocess::{run_turn, SpawnRequest, ToolFlight};

/// The runtime `grok` engine: the resolved launcher, the working directory (repo root) and the
/// per-turn stream files. `plan`/`capabilities` delegate to the core [`SubprocessEngine`].
#[derive(Clone)]
pub struct GrokEngine {
    pub launcher: String,
    pub cwd: PathBuf,
    /// The primary turn's streams.
    pub primary: TurnFiles,
    /// The secondary turn's streams (format repair / timeout continuation).
    pub secondary: TurnFiles,
    /// The stall cut in seconds for the PRIMARY turn (`0` = off).
    pub stall_sec: i64,
    /// The operator's kick file for the PRIMARY turn (`None` = no kick watch).
    pub kick_path: Option<PathBuf>,
    /// Called once per turn, right after the child spawns and before it is waited on.
    pub on_running: Option<std::sync::Arc<dyn Fn(u32, String) + Send + Sync>>,
    /// Told around every tree kill of a turn (`SpawnRequest::on_kill`).
    pub on_kill: Option<super::subprocess::KillHook>,
}

/// Whether the operator opted a grok child's xAI API key in (`C3_GROK_ALLOW_API_KEY=1`): then the
/// key route is allowed and the API key is left in the child's environment.
pub fn grok_allow_api_key() -> bool {
    std::env::var(GROK_ALLOW_API_KEY_VAR)
        .map(|v| v.trim() == "1")
        .unwrap_or(false)
}

/// (BILLING GUARD) The child environment for a grok turn: this process's environment, with the
/// xAI API-key variables ([`GROK_API_KEY_VARS`]) removed unless the operator opted in. The host
/// markers are scrubbed separately by [`run_turn`]. A non-UTF-8 variable (vanishingly rare on
/// Windows, and none grok needs) is dropped.
pub fn grok_child_env() -> Vec<(String, String)> {
    let strip = !grok_allow_api_key();
    std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
        .filter(|(k, _)| !(strip && GROK_API_KEY_VARS.iter().any(|g| k.eq_ignore_ascii_case(g))))
        .collect()
}

/// One grok streaming-json line's effect on the tool calls in flight (`Update-ToolFlight`): a
/// `tool_call` opens a call (key `grok:<toolCallId>`, label `grok <toolName> <id>`); a
/// `tool_call_update` whose `status` is `completed`/`failed` closes it (a `null` status is a
/// progress update, neutral). Suspends the stall timer while a grok tool call runs.
pub fn grok_tool_flight(line: &str) -> ToolFlight {
    let v: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return ToolFlight::None,
    };
    let t = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let id = v
        .get("toolCallId")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();
    match t {
        "tool_call" => {
            let name = v.get("toolName").and_then(|n| n.as_str()).unwrap_or("tool");
            if id.is_empty() {
                ToolFlight::Open {
                    key: format!("grok-tool:{name}#"),
                    label: format!("grok {name}"),
                }
            } else {
                ToolFlight::Open {
                    key: format!("grok:{id}"),
                    label: format!("grok {name} {id}"),
                }
            }
        }
        "tool_call_update" => {
            let status = v.get("status").and_then(|s| s.as_str()).unwrap_or("");
            if status == "completed" || status == "failed" {
                ToolFlight::Close {
                    key: if id.is_empty() {
                        String::new()
                    } else {
                        format!("grok:{id}")
                    },
                    fallback_prefix: String::new(),
                }
            } else {
                ToolFlight::None
            }
        }
        _ => ToolFlight::None,
    }
}

impl GrokEngine {
    fn inner(&self) -> SubprocessEngine {
        SubprocessEngine::new(EngineKind::Grok)
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

    fn expect_thread(turn: &TurnRequest) -> String {
        match &turn.request.mode {
            Mode::Resume { thread, .. } => thread.clone(),
            _ => String::new(),
        }
    }

    /// Run one turn and return the outcome plus the parsed detail.
    pub fn run_detailed(&self, turn: &TurnRequest) -> Result<GrokRun, EngineError> {
        let args = self.run_argv(turn)?;
        let files = self.files_for(turn.kind);
        let timeout = std::time::Duration::from_secs_f64(turn.request.timeout_sec.max(0.0));
        let is_primary = matches!(turn.kind, TurnKind::Primary);
        let td = |l: &str| grok_tool_flight(l);
        // (BILLING GUARD) the child env with the xAI API keys stripped unless opted in.
        let child_env = grok_child_env();
        let spawn = SpawnRequest {
            launcher: &self.launcher,
            argv: &args,
            cwd: &self.cwd,
            prompt_delivery: self.inner().capabilities().prompt_delivery, // PromptFile: empty stdin
            stdin_text: "",
            events_path: &files.events,
            stderr_path: &files.stderr,
            timeout,
            stall_sec: if is_primary { self.stall_sec } else { 0 },
            kick_path: self.kick_path.as_deref(),
            tool_flight: Some(&td),
            on_running: self
                .on_running
                .as_ref()
                .map(|a| a.as_ref() as &dyn Fn(u32, String)),
            on_kill: self.on_kill.as_ref().map(|a| {
                a.as_ref()
                    as &dyn Fn(super::subprocess::KillEvent<'_>) -> super::subprocess::KillRecord
            }),
            env: Some(&child_env),
            // The schema TEXT travels in argv (`--json-schema`): quote it by the C runtime rules on
            // a batch launcher, as the claude engine does.
            crt_quote: true,
            budget: None,
        };
        let result = run_turn(&spawn);

        if !result.started {
            return Ok(GrokRun {
                outcome: AttemptOutcome::LaunchFailed {
                    child_exists: false,
                    message: result
                        .error
                        .unwrap_or_else(|| "the grok process did not start".into()),
                },
                turn: GrokTurn::default(),
            });
        }

        let events_text = read_text(&files.events);
        use super::subprocess::TurnStop;
        let killed = !matches!(result.stop, TurnStop::Exited);
        let allow_partial = killed || result.exit_code != Some(0);
        let events = read_grok_events(&events_text, allow_partial);

        if killed {
            let conversation = if is_uuid(&events.session) {
                ConversationTrust::Candidate(ConversationId(events.session.clone()))
            } else {
                ConversationTrust::None
            };
            let partial = grok_salvage(&events_text);
            let survivors = result.survivors.clone();
            let kill = result.kill.clone();
            let wall_seconds = result.wall_seconds;
            let outcome = match result.stop {
                TurnStop::Stall => AttemptOutcome::Stopped {
                    kind: c3_core::engine::StopKind::Stall {
                        silent_seconds: result.silent_seconds,
                        tool_open_seconds: result.tool_open_seconds,
                        open_tools: result.open_tools.clone(),
                        last_event: result.last_event.clone(),
                    },
                    partial,
                    survivors,
                    conversation,
                    kill: kill.clone(),
                    wall_seconds,
                },
                TurnStop::Kick => AttemptOutcome::Stopped {
                    kind: c3_core::engine::StopKind::Kick,
                    partial,
                    survivors,
                    conversation,
                    kill: kill.clone(),
                    wall_seconds,
                },
                _ => AttemptOutcome::TimedOut {
                    partial,
                    survivors,
                    conversation,
                    kill: kill.clone(),
                    wall_seconds,
                },
            };
            return Ok(GrokRun {
                outcome,
                turn: GrokTurn::default(),
            });
        }

        let exit_code = result.exit_code.unwrap_or(-1);
        let expect_thread = Self::expect_thread(turn);
        let t = grok_turn_outcome(&events, exit_code, &result.stderr, &expect_thread);
        let outcome = to_outcome(&t, &events, files, result.wall_seconds, exit_code);
        Ok(GrokRun { outcome, turn: t })
    }
}

/// One turn's outcome plus its parsed detail.
#[derive(Debug, Clone)]
pub struct GrokRun {
    pub outcome: AttemptOutcome,
    pub turn: GrokTurn,
}

fn to_outcome(
    t: &GrokTurn,
    events: &GrokEvents,
    files: &TurnFiles,
    wall_seconds: f64,
    exit_code: i32,
) -> AttemptOutcome {
    if t.ok {
        let conversation = if t.thread.is_empty() {
            ConversationTrust::None
        } else {
            ConversationTrust::Verified(ConversationId(t.thread.clone()))
        };
        return AttemptOutcome::Completed(Reply {
            raw_text: t.reply.clone(),
            structured: parse_structured(&t.reply),
            events_path: files.events.clone(),
            usage: events.usage.clone(),
            wall_seconds,
            conversation,
        });
    }
    let primary = t
        .texts
        .first()
        .cloned()
        .unwrap_or_else(|| t.outcome.clone());
    let (code, message) = convert_from_provider_error_text(&primary);
    let class = if !t.class.is_empty() {
        t.class.clone()
    } else {
        provider_failure_class(&message)
    };
    AttemptOutcome::ProviderFailure {
        failure: c3_core::ledger::ProviderFailure {
            class,
            code,
            message,
            ..Default::default()
        },
        exit_code: Some(exit_code),
    }
}

impl Engine for GrokEngine {
    fn capabilities(&self) -> Capabilities {
        self.inner().capabilities()
    }

    fn plan(&self, request: &Request) -> Result<LaunchPlan, EngineError> {
        self.inner().plan(request)
    }

    fn precheck(&self, _turn: &TurnRequest) -> Result<(), EngineError> {
        // Grok has no launch refusal: the billing guard is the child-env key strip
        // ([`grok_child_env`]), not a block. The sign-in is reported by `providers`.
        Ok(())
    }

    fn run(&self, turn: &TurnRequest) -> Result<AttemptOutcome, EngineError> {
        self.run_detailed(turn).map(|r| r.outcome)
    }

    fn continue_turn(&self, turn: &TurnRequest) -> Result<AttemptOutcome, EngineError> {
        self.run_detailed(turn).map(|r| r.outcome)
    }
}

// --------------------------------------------------------------------------- event parsing

const UUID_RE: &str =
    r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$";

fn is_uuid(s: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(UUID_RE).unwrap())
        .is_match(s)
}

fn re(pat: &str) -> regex::Regex {
    regex::Regex::new(pat).unwrap()
}

fn one_line(s: &str) -> String {
    c3_core::one_line(s)
}

/// The normalized record of one `grok ... --output-format streaming-json` stream.
#[derive(Debug, Clone, Default)]
pub struct GrokEvents {
    pub records: usize,
    /// Why the stream is malformed (`''` when well-formed).
    pub malformed: String,
    /// The reply text: the concatenated `text` events, in order.
    pub text: String,
    /// The session id of the `end` event (`''` when none).
    pub session: String,
    /// The `stopReason` of the `end` event (`end_turn`, `max_turn_requests`, ...).
    pub stop_reason: String,
    /// An `end` event was seen.
    pub has_end: bool,
    /// The message of an `error` event (a fatal/startup error; no `end` follows).
    pub error: String,
    /// The token usage: the `end` event's cumulative usage, else the last `usage` event.
    pub usage: Option<Usage>,
}

/// Map a grok `usage` object to the ledger [`Usage`]. `total` carries the `end` event's
/// `total_tokens` (a per-call `usage` event has none).
fn grok_usage(u: &Value) -> Option<Usage> {
    let o = u.as_object()?;
    let get = |name: &str| -> i64 { o.get(name).and_then(|v| v.as_i64()).unwrap_or(0) };
    Some(Usage {
        input_tokens: get("input_tokens"),
        cached_input_tokens: get("cache_read_input_tokens"),
        output_tokens: get("output_tokens"),
        reasoning_output_tokens: get("reasoning_tokens"),
        total_tokens: o.get("total_tokens").and_then(|v| v.as_i64()),
        extra: Default::default(),
    })
}

/// Parse one grok streaming-json stream. `allow_partial_last` lets the last non-empty line be
/// truncated (a killed / non-zero-exit turn).
pub fn read_grok_events(events_text: &str, allow_partial_last: bool) -> GrokEvents {
    let mut r = GrokEvents::default();
    let lines: Vec<&str> = events_text.split(['\r', '\n']).collect();
    let last_idx = lines.iter().rposition(|l| !l.trim().is_empty());
    let mut end_count = 0usize;
    let mut last_usage: Option<Usage> = None;

    for (i, raw) in lines.iter().enumerate() {
        let t = raw.trim();
        if t.is_empty() {
            continue;
        }
        let obj: Value = match serde_json::from_str::<Value>(t) {
            Ok(v) if v.is_object() => v,
            _ => {
                let is_last = Some(i) == last_idx;
                if (!is_last || !allow_partial_last) && r.malformed.is_empty() {
                    r.malformed = format!("line {} is not a JSON object", i + 1);
                }
                continue;
            }
        };
        r.records += 1;
        let kind = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match kind {
            "available_commands" | "thought" | "tool_call" | "tool_call_update"
            | "max_turns_reached" => {}
            "text" => {
                if let Some(d) = obj.get("data").and_then(|v| v.as_str()) {
                    r.text.push_str(d);
                }
            }
            "error" => {
                if r.error.is_empty() {
                    r.error = obj
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                }
            }
            "usage" => {
                if let Some(u) = obj.get("usage") {
                    last_usage = grok_usage(u);
                }
            }
            "end" => {
                end_count += 1;
                r.has_end = true;
                r.session = obj
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                r.stop_reason = obj
                    .get("stopReason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if let Some(u) = obj.get("usage") {
                    r.usage = grok_usage(u);
                }
            }
            _ => {}
        }
    }

    if end_count > 1 && r.malformed.is_empty() {
        r.malformed = format!("{end_count} end events (exactly one expected)");
    }
    if r.usage.is_none() {
        r.usage = last_usage;
    }
    r
}

// --------------------------------------------------------------------------- outcome rules

/// The failure rules of one grok turn. `denied_empty` is always false (grok has no denial retry).
#[derive(Debug, Clone, Default)]
pub struct GrokTurn {
    pub ok: bool,
    pub outcome: String,
    pub class: String,
    pub texts: Vec<String>,
    pub thread: String,
    pub thread_candidate: String,
    pub reply: String,
    pub structured: bool,
    pub warnings: Vec<String>,
}

/// Judge one grok turn. `expect_thread` is the session a resume must land on.
pub fn grok_turn_outcome(
    events: &GrokEvents,
    exit_code: i32,
    stderr_text: &str,
    expect_thread: &str,
) -> GrokTurn {
    let mut o = GrokTurn::default();
    let lines: Vec<String> = stderr_text
        .split(['\r', '\n'])
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    let warn_re = re(r"(?i)^(grok:\s*)?warning:");
    let warn_lines: Vec<String> = lines
        .iter()
        .filter(|l| warn_re.is_match(l))
        .cloned()
        .collect();
    let stderr_tail = lines.last().cloned().unwrap_or_default();

    let session = &events.session;
    let is_uuid_session = is_uuid(session);

    // An `error` event (bad model id, not signed in, ...) is a fatal start failure: no `end`, no
    // session.
    if !events.error.is_empty() {
        fail(
            &mut o,
            &format!("grok error - {}", one_line(&events.error)),
            "",
            std::slice::from_ref(&events.error),
        );
        return o;
    }

    if exit_code != 0 {
        // The commonest non-zero exit is the model running out of turns (`max_turn_requests`),
        // which still carries the session and the partial reply.
        if events.stop_reason == "max_turn_requests" {
            fail(
                &mut o,
                "grok reached its --max-turns before finishing (max_turn_requests)",
                "capability",
                &[],
            );
            o.reply = events.text.clone();
            if is_uuid_session && (expect_thread.is_empty() || session == expect_thread) {
                o.thread = session.clone();
            } else if is_uuid_session {
                o.thread_candidate = session.clone();
            }
            return o;
        }
        let tail = if stderr_tail.is_empty() {
            String::new()
        } else {
            format!(" - {}", one_line(&stderr_tail))
        };
        fail(
            &mut o,
            &format!("grok exit {exit_code}{tail}"),
            "",
            std::slice::from_ref(&stderr_tail),
        );
        if is_uuid_session {
            o.thread_candidate = session.clone();
        }
        return o;
    }
    if !events.malformed.is_empty() {
        fail(
            &mut o,
            &format!("malformed event stream: {}", events.malformed),
            "transport",
            &[],
        );
        if is_uuid_session {
            o.thread_candidate = session.clone();
        }
        return o;
    }
    if !events.has_end {
        let tail = if stderr_tail.is_empty() {
            String::new()
        } else {
            format!(" - {}", one_line(&stderr_tail))
        };
        fail(
            &mut o,
            &format!("no end event in the grok stream{tail}"),
            "",
            std::slice::from_ref(&stderr_tail),
        );
        return o;
    }
    if session.is_empty() {
        fail(
            &mut o,
            "the grok event stream names no session (the end event has no sessionId)",
            "unknown",
            &[],
        );
        return o;
    }
    if !expect_thread.is_empty() && session != expect_thread {
        fail(
            &mut o,
            &format!("parent session {expect_thread} not found, grok started {session}"),
            "unknown",
            &[],
        );
        if is_uuid_session {
            o.thread_candidate = session.clone();
        }
        return o;
    }
    if events.stop_reason != "end_turn" {
        let sr = if events.stop_reason.is_empty() {
            "(none)".to_string()
        } else {
            events.stop_reason.clone()
        };
        fail(&mut o, &format!("grok stopped: {sr}"), "", &[]);
        if is_uuid_session {
            o.thread = session.clone();
        }
        return o;
    }
    if !is_uuid_session {
        fail(
            &mut o,
            &format!("the session id '{session}' is not a uuid"),
            "unknown",
            &[],
        );
        return o;
    }
    o.thread = session.clone();
    o.reply = events.text.clone();
    if o.reply.trim().is_empty() {
        fail(&mut o, "empty reply", "", &[]);
        return o;
    }
    if o.reply.trim().starts_with('{') {
        o.structured = serde_json::from_str::<Value>(o.reply.trim())
            .map(|v| v.is_object())
            .unwrap_or(false);
    }
    o.ok = true;
    o.outcome = "usable reply".into();
    o.warnings = warn_lines.iter().map(|w| one_line(w)).collect();
    o
}

fn fail(o: &mut GrokTurn, why: &str, class: &str, texts: &[String]) {
    o.ok = false;
    o.outcome = format!("failed: {why}");
    o.class = class.to_string();
    let mut all: Vec<String> = texts.iter().filter(|t| !t.is_empty()).cloned().collect();
    all.push(why.to_string());
    o.texts = all;
}

// --------------------------------------------------------------------------- salvage

/// Salvage the reply text of a killed turn: the `text` events, concatenated. `None` when nothing
/// was produced.
pub fn grok_salvage(events_text: &str) -> Option<String> {
    let mut out = String::new();
    for line in events_text.split(['\r', '\n']) {
        let t = line.trim();
        if !t.starts_with('{') {
            continue;
        }
        let obj: Value = match serde_json::from_str(t) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if obj.get("type").and_then(|v| v.as_str()) == Some("text") {
            if let Some(d) = obj.get("data").and_then(|v| v.as_str()) {
                out.push_str(d);
            }
        }
    }
    if out.trim().is_empty() {
        None
    } else {
        Some(out)
    }
}

// --------------------------------------------------------------------------- helpers

fn parse_structured(raw_text: &str) -> Option<StructuredReply> {
    let trimmed = raw_text.trim();
    if !trimmed.starts_with('{') {
        return None;
    }
    let raw: RawReply = serde_json::from_str(trimmed).ok()?;
    StructuredReply::try_from(raw).ok()
}

fn read_text(path: &Path) -> String {
    std::fs::read(path)
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("fixtures/grok_ok.events.jsonl");
    const SCHEMA: &str = include_str!("fixtures/grok_schema.events.jsonl");
    const TOOLS_MAXTURNS: &str = include_str!("fixtures/grok_tools_maxturns.events.jsonl");
    const ERROR: &str = include_str!("fixtures/grok_error.events.jsonl");

    #[test]
    fn ok_stream_is_a_usable_reply_with_usage_and_session() {
        let e = read_grok_events(OK, false);
        assert!(e.malformed.is_empty(), "malformed: {}", e.malformed);
        assert!(e.has_end);
        assert_eq!(e.stop_reason, "end_turn");
        assert!(is_uuid(&e.session));
        assert_eq!(e.text, "OK");
        let u = e.usage.clone().expect("usage from the end event");
        assert_eq!(u.input_tokens, 17701);
        assert_eq!(u.output_tokens, 127);
        assert_eq!(u.cached_input_tokens, 1664);
        assert_eq!(u.reasoning_output_tokens, 126);
        assert_eq!(u.total_tokens, Some(19492));
        let t = grok_turn_outcome(&e, 0, "", "");
        assert!(t.ok, "outcome: {}", t.outcome);
        assert_eq!(t.reply, "OK");
        assert_eq!(t.thread, e.session);
    }

    #[test]
    fn schema_stream_concatenates_the_text_chunks_into_one_object() {
        let e = read_grok_events(SCHEMA, false);
        assert!(e.malformed.is_empty());
        assert_eq!(e.text, r#"{"answer":"forty-two","n":42}"#);
        let t = grok_turn_outcome(&e, 0, "", "");
        assert!(t.ok, "outcome: {}", t.outcome);
        assert!(t.structured, "the reply is a JSON object");
    }

    #[test]
    fn max_turns_is_a_capability_failure_that_keeps_the_session_and_the_partial() {
        let e = read_grok_events(TOOLS_MAXTURNS, false);
        assert!(e.has_end);
        assert_eq!(e.stop_reason, "max_turn_requests");
        assert!(is_uuid(&e.session));
        // The tool-call chatter is concatenated into the (non-empty) partial reply.
        assert!(!e.text.trim().is_empty());
        let t = grok_turn_outcome(&e, 1, "", "");
        assert!(!t.ok);
        assert_eq!(t.class, "capability");
        assert!(t.outcome.contains("max_turn_requests"));
        assert_eq!(t.thread, e.session);
        assert!(!t.reply.trim().is_empty(), "the partial reply is kept");
    }

    #[test]
    fn error_event_is_a_failure_with_the_message_and_no_thread() {
        let e = read_grok_events(ERROR, true);
        assert!(!e.error.is_empty());
        assert!(e.error.contains("unknown model id"));
        assert!(!e.has_end);
        let t = grok_turn_outcome(&e, 1, "", "");
        assert!(!t.ok);
        assert!(t.outcome.contains("grok error"));
        assert!(t.thread.is_empty());
        assert!(t.thread_candidate.is_empty());
    }

    #[test]
    fn tool_flight_opens_on_tool_call_and_closes_on_completion() {
        let open = r#"{"type":"tool_call","toolCallId":"call-1","toolName":"read_file","status":"pending"}"#;
        let progress = r#"{"type":"tool_call_update","toolCallId":"call-1","status":null}"#;
        let done = r#"{"type":"tool_call_update","toolCallId":"call-1","status":"completed"}"#;
        assert_eq!(grok_tool_flight(open).delta(), 1);
        assert_eq!(grok_tool_flight(progress).delta(), 0);
        assert_eq!(grok_tool_flight(done).delta(), -1);
        assert_eq!(grok_tool_flight(r#"{"type":"text","data":"x"}"#).delta(), 0);
        assert_eq!(grok_tool_flight("garbage").delta(), 0);
    }

    #[test]
    fn salvage_concatenates_text_events() {
        let s = concat!(
            r#"{"type":"text","data":"half "}"#,
            "\n",
            r#"{"type":"thought","data":"ignored"}"#,
            "\n",
            r#"{"type":"text","data":"reply"}"#,
        );
        assert_eq!(grok_salvage(s).as_deref(), Some("half reply"));
        assert!(grok_salvage("").is_none());
    }

    #[test]
    fn child_env_strips_the_api_keys_unless_opted_in() {
        // Env mutation is global, so one sequential test covers both arms.
        let key = GROK_API_KEY_VARS[0];
        let prev_key = std::env::var(key).ok();
        let prev_allow = std::env::var(GROK_ALLOW_API_KEY_VAR).ok();

        std::env::set_var(key, "xai-xxx");
        std::env::remove_var(GROK_ALLOW_API_KEY_VAR);
        let env = grok_child_env();
        assert!(
            !env.iter().any(|(k, _)| k.eq_ignore_ascii_case(key)),
            "the API key must be stripped by default"
        );

        std::env::set_var(GROK_ALLOW_API_KEY_VAR, "1");
        let env = grok_child_env();
        assert!(
            env.iter()
                .any(|(k, v)| k.eq_ignore_ascii_case(key) && v == "xai-xxx"),
            "with the opt-in the API key is kept"
        );

        match prev_key {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
        match prev_allow {
            Some(v) => std::env::set_var(GROK_ALLOW_API_KEY_VAR, v),
            None => std::env::remove_var(GROK_ALLOW_API_KEY_VAR),
        }
    }
}
