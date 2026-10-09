//! The `agy` engine adapter: run one Google Antigravity CLI turn
//! (`agy -p= --input-format stream-json --output-format stream-json ...`) and turn its NDJSON
//! event stream into an [`AttemptOutcome`].
//!
//! The argv is the plugin's (built by [`c3_core::engine::SubprocessEngine`]); the prompt goes
//! on stdin as ONE NDJSON `{"event":"user","message":{"content":...}}` line
//! ([`convert_to_agy_stdin`], `ConvertTo-AgyStdin`). The `--output-format stream-json` stream
//! ([`super::subprocess`] captured it to `.events.jsonl`) is parsed by [`read_agy_events`]
//! (`Read-AgyEvents`) and judged by [`agy_turn_outcome`] (`Get-AgyTurnOutcome`).
//!
//! `agy`'s print mode auto-denies a tool it cannot grant and then ends the turn with no
//! output: [`AgyTurn::denied_empty`] flags that F11 case so the orchestrator can run one
//! denial-retry turn (resume the conversation, tell the model not to call the tool). Read-only
//! is NOT enforced by `agy` itself — the orchestrator's [`super::tree_check`] enforces it.

use std::path::PathBuf;

use c3_core::engine::{
    AttemptOutcome, Capabilities, ConversationId, ConversationTrust, Engine, EngineError,
    EngineKind, LaunchPlan, Mode, RawReply, Reply, Request, StructuredReply, SubprocessEngine,
    TurnKind, TurnRequest,
};
use c3_core::health::{convert_from_provider_error_text, provider_failure_class};
use c3_core::ledger::{ProviderFailure, Usage};
use serde_json::Value;

use super::codex::TurnFiles;
use super::subprocess::{run_turn, SpawnRequest, ToolFlight};

/// The runtime `agy` engine: the resolved launcher, the working directory (repo root) and the
/// per-turn stream files. `plan`/`capabilities` delegate to the core [`SubprocessEngine`] so the
/// argv stays byte-identical; `precheck` adds the sign-in guard.
#[derive(Clone)]
pub struct AgyEngine {
    pub launcher: String,
    pub cwd: PathBuf,
    /// The primary turn's streams.
    pub primary: TurnFiles,
    /// The secondary turn's streams (denial retry / format repair / timeout continuation).
    pub secondary: TurnFiles,
    /// When true, `precheck` does NOT run `agy models` (the `--no-network` preflight skip); the
    /// launcher-present check still applies.
    pub no_network: bool,
    /// The `agy models` timeout, seconds (`$script:AgyModelsTimeoutSec`, 45).
    pub models_timeout_sec: u64,
    /// (wave 26b, D12) The stall cut in seconds for the PRIMARY turn (`0` = off).
    pub stall_sec: i64,
    /// (wave 26b, D10) The operator's kick file for the PRIMARY turn (`None` = no kick watch).
    pub kick_path: Option<PathBuf>,
    /// Called once per turn, right after the child spawns and before it is waited on
    /// (recovery record `launching` -> `running`). `None` leaves the record `launching`.
    pub on_running: Option<std::sync::Arc<dyn Fn(u32, String) + Send + Sync>>,
}

impl AgyEngine {
    fn inner(&self) -> SubprocessEngine {
        SubprocessEngine::new(EngineKind::Agy)
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

    /// The conversation id this turn resumes (`--conversation`), when it is a resume/continue.
    fn expect_thread(turn: &TurnRequest) -> String {
        match &turn.request.mode {
            Mode::Resume { thread, .. } => thread.clone(),
            _ => String::new(),
        }
    }

    /// Run one turn and return the full parsed detail (the orchestrator uses
    /// [`AgyTurn::denied_empty`] and [`AgyTurn::thread`] to decide the denial retry). The trait's
    /// [`Engine::run`]/[`Engine::continue_turn`] return only the [`AttemptOutcome`].
    pub fn run_detailed(&self, turn: &TurnRequest) -> Result<AgyRun, EngineError> {
        let args = self.run_argv(turn)?;
        let files = self.files_for(turn.kind);
        let timeout = std::time::Duration::from_secs_f64(turn.request.timeout_sec.max(0.0));
        let stdin = convert_to_agy_stdin(&turn.request.prompt);
        let is_primary = matches!(turn.kind, TurnKind::Primary);
        let spawn = SpawnRequest {
            launcher: &self.launcher,
            argv: &args,
            cwd: &self.cwd,
            prompt_delivery: self.inner().capabilities().prompt_delivery,
            stdin_text: &stdin,
            events_path: &files.events,
            stderr_path: &files.stderr,
            timeout,
            stall_sec: if is_primary { self.stall_sec } else { 0 },
            // (wave 26c, D1) the caller scopes `kick_path` (primary always; the format-repair turn
            // so a kicked repair leaves the first reply standing; a continuation passes None).
            kick_path: self.kick_path.as_deref(),
            // (wave 2e, F11-5) an ACTIVE tool step suspends the stall timer (up to 2 x stall
            // without growth) and a stall cut names it ("agy tool step <n>").
            tool_flight: Some(&agy_tool_flight),
            on_running: self
                .on_running
                .as_ref()
                .map(|a| a.as_ref() as &dyn Fn(u32, String)),
        };
        let result = run_turn(&spawn);

        if !result.started {
            return Ok(AgyRun {
                outcome: AttemptOutcome::LaunchFailed {
                    child_exists: false,
                    message: result
                        .error
                        .unwrap_or_else(|| "the agy process did not start".into()),
                },
                turn: AgyTurn::default(),
            });
        }

        let events_text = read_text(&files.events);
        // The last line may be a truncated NDJSON line only when the process was killed or
        // exited non-zero (F10-2); on a clean exit trailing garbage makes the stream malformed.
        use super::subprocess::TurnStop;
        let killed = !matches!(result.stop, TurnStop::Exited);
        let allow_partial = killed || result.exit_code != Some(0);
        let events = read_agy_events(&events_text, allow_partial);

        if killed {
            let candidate = first_uuid(&[&events.thread, &events.init_thread]);
            let conversation = if candidate.is_empty() {
                ConversationTrust::None
            } else {
                ConversationTrust::Candidate(ConversationId(candidate))
            };
            let partial = agy_salvage(&events_text);
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
            return Ok(AgyRun {
                outcome,
                turn: AgyTurn::default(),
            });
        }

        let exit_code = result.exit_code.unwrap_or(-1);
        let expect = Self::expect_thread(turn);
        let turn_outcome = agy_turn_outcome(&events, exit_code, &result.stderr, "", &expect);
        let outcome = self.to_outcome(
            &turn_outcome,
            &events,
            files,
            result.wall_seconds,
            exit_code,
        );
        Ok(AgyRun {
            outcome,
            turn: turn_outcome,
        })
    }

    fn to_outcome(
        &self,
        t: &AgyTurn,
        events: &AgyEvents,
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
        // A failure: the forced class (transport/permission/unknown/capability), else classify
        // the failure texts. The message is the best evidence, one-lined.
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
            failure: ProviderFailure {
                class,
                code,
                message,
                ..Default::default()
            },
            exit_code: Some(exit_code),
        }
    }
}

/// One turn's outcome plus the full parsed detail (for the denial-retry decision).
#[derive(Debug, Clone)]
pub struct AgyRun {
    pub outcome: AttemptOutcome,
    pub turn: AgyTurn,
}

impl Engine for AgyEngine {
    fn capabilities(&self) -> Capabilities {
        self.inner().capabilities()
    }

    fn plan(&self, request: &Request) -> Result<LaunchPlan, EngineError> {
        self.inner().plan(request)
    }

    fn precheck(&self, _turn: &TurnRequest) -> Result<(), EngineError> {
        if self.launcher.trim().is_empty() {
            return Err(EngineError::Precheck("agy CLI not found on PATH".into()));
        }
        if self.no_network {
            return Ok(());
        }
        // The sign-in check lives once, in `providers` (`Get-AgyModelsStatus`): a model listing =
        // signed in; an auth-wording line = not signed in; anything else unknown.
        let cred = crate::providers::get_agy_models_status(&self.launcher, self.models_timeout_sec);
        match cred.state {
            c3_core::credential::State::Ok => Ok(()),
            _ => Err(EngineError::Precheck(cred.reason)),
        }
    }

    fn run(&self, turn: &TurnRequest) -> Result<AttemptOutcome, EngineError> {
        self.run_detailed(turn).map(|r| r.outcome)
    }

    fn continue_turn(&self, turn: &TurnRequest) -> Result<AttemptOutcome, EngineError> {
        self.run_detailed(turn).map(|r| r.outcome)
    }
}

// --------------------------------------------------------------------------- tool calls in flight

/// (wave 2e, F11-5) One agy stream-json line's effect on the tool calls in flight
/// (`Update-ToolFlight`, engine agy): a `step_update` whose `step_type` is `tool` OPENS a call
/// while its `state` is `ACTIVE` (key `agy:<step_index>`, label `agy tool step <step_index>`) and
/// CLOSES it in any other state. Any other line changes nothing.
pub fn agy_tool_flight(line: &str) -> ToolFlight {
    let t = line.trim();
    // a cheap test first: most lines are text deltas
    if !t.starts_with('{') || !t.contains("\"tool\"") {
        return ToolFlight::None;
    }
    let v: Value = match serde_json::from_str(t) {
        Ok(v) => v,
        Err(_) => return ToolFlight::None,
    };
    if v.get("event").and_then(|e| e.as_str()) != Some("step_update") {
        return ToolFlight::None;
    }
    let su = match v.get("step_update") {
        Some(s) if s.is_object() => s,
        _ => return ToolFlight::None,
    };
    if su.get("step_type").and_then(|s| s.as_str()) != Some("tool") {
        return ToolFlight::None;
    }
    let index = match su.get("step_index") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    };
    let key = format!("agy:{index}");
    if su.get("state").and_then(|s| s.as_str()) == Some("ACTIVE") {
        ToolFlight::Open {
            label: format!(
                "agy tool step {}",
                if index.is_empty() { "?" } else { &index }
            ),
            key,
        }
    } else {
        ToolFlight::Close {
            key,
            fallback_prefix: String::new(),
        }
    }
}

// --------------------------------------------------------------------------- stdin transport

/// The stdin of one agy turn: ONE NDJSON `{"event":"user","message":{"content":<prompt>}}` line
/// plus a trailing LF (`ConvertTo-AgyStdin`). `serde_json` escapes exactly as the plugin's
/// `ConvertTo-Json -Compress` does for the wire.
pub fn convert_to_agy_stdin(prompt: &str) -> String {
    let obj = serde_json::json!({ "event": "user", "message": { "content": prompt } });
    format!("{}\n", serde_json::to_string(&obj).unwrap_or_default())
}

// --------------------------------------------------------------------------- event parsing

const UUID_RE: &str =
    r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$";

fn is_uuid(s: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(UUID_RE).unwrap())
        .is_match(s)
}

fn first_uuid(candidates: &[&String]) -> String {
    for c in candidates {
        if is_uuid(c) {
            return (*c).clone();
        }
    }
    String::new()
}

/// The normalized record of one agy `--output-format stream-json` stream (`Read-AgyEvents`).
#[derive(Debug, Clone, Default)]
pub struct AgyEvents {
    /// `init.conversation_id` (`''` when none).
    pub init_thread: String,
    /// Number of `result` events (exactly one is well-formed).
    pub result_count: usize,
    /// Why the stream is malformed (`''` when well-formed).
    pub malformed: String,
    /// A `result` event was seen.
    pub has_result: bool,
    /// `result.conversation_id`.
    pub thread: String,
    pub status: String,
    pub response: String,
    /// `result.error` (a string, or a compact JSON serialization of an object).
    pub error: String,
    /// `result.structured_output` is a JSON object.
    pub has_structured: bool,
    /// That object, serialized compactly.
    pub structured_json: String,
    pub usage: Option<Usage>,
    /// `tool_name` of the LAST `step_update` with `step_type` `"tool"`.
    pub tool_name: String,
    /// The first `result.denied_actions[].display_name` (else `.action`).
    pub denied_action: String,
}

/// Parse one agy event stream tolerantly. `allow_partial_last` lets the LAST non-empty line be a
/// truncated NDJSON line (a killed or non-zero-exit turn) without marking the stream malformed.
pub fn read_agy_events(events_text: &str, allow_partial_last: bool) -> AgyEvents {
    let mut r = AgyEvents::default();
    let lines: Vec<&str> = events_text.split(['\r', '\n']).collect();
    let last_idx = lines.iter().rposition(|l| !l.trim().is_empty());
    let mut result: Option<Value> = None;
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
        let ev = obj.get("event").and_then(|v| v.as_str()).unwrap_or("");
        match ev {
            "init" => {
                if r.init_thread.is_empty() {
                    r.init_thread = str_field(&obj, "conversation_id");
                }
            }
            "step_update" => {
                if let Some(su) = obj.get("step_update") {
                    if su.get("step_type").and_then(|v| v.as_str()) == Some("tool") {
                        let tn = str_field(su, "tool_name");
                        if !tn.is_empty() {
                            r.tool_name = tn;
                        }
                    }
                }
            }
            "result" => {
                r.result_count += 1;
                match obj.get("result") {
                    Some(res) if res.is_object() => result = Some(res.clone()),
                    _ => {
                        if r.malformed.is_empty() {
                            r.malformed =
                                format!("result event at line {} is not an object", i + 1);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if r.result_count > 1 && r.malformed.is_empty() {
        r.malformed = format!("{} result events (exactly one expected)", r.result_count);
    }
    if let Some(res) = result {
        r.has_result = true;
        r.thread = str_field(&res, "conversation_id");
        r.status = str_field(&res, "status");
        r.response = str_field(&res, "response");
        r.error = match res.get("error") {
            Some(Value::String(s)) => s.clone(),
            Some(v) if !v.is_null() => serde_json::to_string(v).unwrap_or_default(),
            _ => String::new(),
        };
        if let Some(so) = res.get("structured_output") {
            if so.is_object() {
                r.has_structured = true;
                r.structured_json = serde_json::to_string(so).unwrap_or_default();
            }
        }
        r.usage = parse_agy_usage(res.get("usage"));
        r.denied_action = parse_denied_action(res.get("denied_actions"));
    }
    r
}

fn str_field(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}

fn parse_agy_usage(u: Option<&Value>) -> Option<Usage> {
    let u = u?;
    if !u.is_object() {
        return None;
    }
    let get = |name: &str| -> i64 { u.get(name).and_then(|v| v.as_i64()).unwrap_or(0) };
    Some(Usage {
        input_tokens: get("input_tokens"),
        cached_input_tokens: get("cache_read_tokens"),
        output_tokens: get("output_tokens"),
        reasoning_output_tokens: get("thinking_tokens"),
        total_tokens: u.get("total_tokens").and_then(|v| v.as_i64()),
        extra: Default::default(),
    })
}

/// The first `denied_actions` entry's `display_name` (else `action`). `denied_actions` may be an
/// array or a single object (the real CLI emits an object for one denial).
fn parse_denied_action(v: Option<&Value>) -> String {
    let one = |da: &Value| -> String {
        let name = str_field(da, "display_name");
        if !name.is_empty() {
            name
        } else {
            str_field(da, "action")
        }
    };
    match v {
        Some(Value::Array(arr)) => {
            for da in arr {
                let n = one(da);
                if !n.is_empty() {
                    return n;
                }
            }
            String::new()
        }
        Some(v) if v.is_object() => one(v),
        _ => String::new(),
    }
}

// --------------------------------------------------------------------------- outcome rules

/// The failure rules of one agy turn (`Get-AgyTurnOutcome`). The same shape as the plugin's
/// result object; [`denied_empty`](AgyTurn::denied_empty) is the F11 denial-retry trigger.
#[derive(Debug, Clone, Default)]
pub struct AgyTurn {
    pub ok: bool,
    /// `"usable reply"` or `"failed: <why>"`.
    pub outcome: String,
    /// The forced `provider_failure` class (`''` = classify the texts).
    pub class: String,
    /// The failure evidence, best first.
    pub texts: Vec<String>,
    /// A verified conversation id (safe to resume), `''` otherwise.
    pub thread: String,
    /// An id observed but never trusted as a parent.
    pub thread_candidate: String,
    /// The reply text: the structured object serialized, else the response.
    pub reply: String,
    pub structured: bool,
    /// A tool was auto-denied and the turn produced nothing (run the denial retry).
    pub denied_empty: bool,
    pub denial_line: String,
    /// The permission the denial notice names (e.g. `command`).
    pub permission: String,
    /// The resume "not found" warning line.
    pub not_found: String,
    pub warnings: Vec<String>,
}

/// Judge one agy turn. `pre` is a failure the caller already knows (a timeout/launch failure);
/// `expect_thread` is the conversation id a resume/continue must land on (`''` for a fresh turn).
pub fn agy_turn_outcome(
    events: &AgyEvents,
    exit_code: i32,
    stderr_text: &str,
    pre: &str,
    expect_thread: &str,
) -> AgyTurn {
    let mut o = AgyTurn::default();
    let lines: Vec<String> = stderr_text
        .split(['\r', '\n'])
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    let denial = find(&lines, r"(?i)no output produced|auto-denied");
    let not_found = find(&lines, r"(?i)^warning:\s*conversation\s.*not found");
    let partial = find(&lines, r"(?i)returning partial output|print timeout");
    let warn_lines: Vec<String> = lines
        .iter()
        .filter(|l| re(r"(?i)^warning:").is_match(l))
        .cloned()
        .collect();
    let stderr_tail = lines.last().cloned().unwrap_or_default();

    if !denial.is_empty() {
        o.denial_line = denial.clone();
        if let Some(c) = re(r#"the "(?P<perm>[^"]+)" permission"#).captures(&denial) {
            o.permission = c.name("perm").unwrap().as_str().to_string();
        }
    }
    if !not_found.is_empty() {
        o.not_found = not_found.clone();
    }
    o.reply = if events.has_structured {
        events.structured_json.clone()
    } else {
        events.response.clone()
    };
    o.structured = events.has_structured;
    let res_id = &events.thread;
    let init_id = &events.init_thread;
    let best_err = &events.error;
    let detail = if !best_err.is_empty() {
        best_err.clone()
    } else if !denial.is_empty() {
        denial.clone()
    } else {
        stderr_tail.clone()
    };

    let candidate_from = |o: &mut AgyTurn| {
        if is_uuid(res_id) {
            o.thread_candidate = res_id.clone();
        } else if is_uuid(init_id) {
            o.thread_candidate = init_id.clone();
        }
    };

    if !pre.is_empty() {
        o.outcome = pre.to_string();
        o.texts = keep(&[best_err.clone(), detail.clone(), strip_failed(pre)]);
        candidate_from(&mut o);
        return o;
    }
    if exit_code != 0 {
        let tail = if detail.is_empty() {
            String::new()
        } else {
            format!(" - {}", one_line(&detail))
        };
        fail(
            &mut o,
            &format!("agy exit {exit_code}{tail}"),
            "",
            &[best_err.clone(), detail.clone()],
        );
        if is_uuid(res_id)
            && (expect_thread.is_empty() || res_id == expect_thread)
            && not_found.is_empty()
        {
            o.thread = res_id.clone();
        } else if is_uuid(res_id) {
            o.thread_candidate = res_id.clone();
        } else if is_uuid(init_id) {
            o.thread_candidate = init_id.clone();
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
        if is_uuid(init_id) {
            o.thread_candidate = init_id.clone();
        }
        return o;
    }
    if !events.has_result {
        let tail = if stderr_tail.is_empty() {
            String::new()
        } else {
            format!(" - {}", one_line(&stderr_tail))
        };
        fail(
            &mut o,
            &format!("no result event in the agy event stream{tail}"),
            "",
            std::slice::from_ref(&stderr_tail),
        );
        if is_uuid(init_id) {
            o.thread_candidate = init_id.clone();
        }
        return o;
    }
    if !init_id.is_empty() && !res_id.is_empty() && init_id != res_id {
        fail(
            &mut o,
            &format!("conversation id mismatch: init {init_id}, result {res_id}"),
            "unknown",
            &[],
        );
        if is_uuid(res_id) {
            o.thread_candidate = res_id.clone();
        }
        return o;
    }
    if !expect_thread.is_empty() {
        if !not_found.is_empty() {
            let started = if res_id.is_empty() {
                "another conversation".to_string()
            } else {
                res_id.clone()
            };
            fail(&mut o, &format!("parent conversation {expect_thread} not found, agy started {started} ({not_found})"), "unknown", std::slice::from_ref(&not_found));
            if is_uuid(res_id) {
                o.thread_candidate = res_id.clone();
            }
            return o;
        }
        if res_id.is_empty() {
            fail(
                &mut o,
                &format!("the result names no conversation id (resume of {expect_thread})"),
                "unknown",
                &[],
            );
            return o;
        }
        if res_id != expect_thread {
            fail(
                &mut o,
                &format!("parent conversation {expect_thread} not found, agy started {res_id}"),
                "unknown",
                &[],
            );
            if is_uuid(res_id) {
                o.thread_candidate = res_id.clone();
            }
            return o;
        }
    }
    if events.status != "SUCCESS" {
        let s = if events.status.is_empty() {
            "(none)".to_string()
        } else {
            events.status.clone()
        };
        let tail = if detail.is_empty() {
            String::new()
        } else {
            format!(" - {}", one_line(&detail))
        };
        fail(
            &mut o,
            &format!("agy status {s}{tail}"),
            "",
            &[best_err.clone(), detail.clone()],
        );
        if is_uuid(res_id) {
            o.thread = res_id.clone();
        }
        return o;
    }
    if !is_uuid(res_id) {
        fail(
            &mut o,
            &format!("the result's conversation_id '{res_id}' is not a uuid"),
            "unknown",
            &[],
        );
        return o;
    }
    o.thread = res_id.clone();
    if !partial.is_empty() {
        fail(
            &mut o,
            &format!("partial output - {}", one_line(&partial)),
            "",
            std::slice::from_ref(&partial),
        );
        return o;
    }
    if !events.has_structured && events.response.trim().is_empty() {
        if !denial.is_empty() {
            fail(
                &mut o,
                &one_line(&denial),
                "permission",
                std::slice::from_ref(&denial),
            );
            o.denied_empty = true;
        } else {
            let tail = if best_err.is_empty() {
                String::new()
            } else {
                format!(" - {}", one_line(best_err))
            };
            fail(
                &mut o,
                &format!("empty reply{tail}"),
                "",
                std::slice::from_ref(best_err),
            );
        }
        return o;
    }
    o.ok = true;
    o.outcome = "usable reply".into();
    let mut w = Vec::new();
    if !denial.is_empty() {
        w.push(format!("denial notice: {}", one_line(&denial)));
    }
    for wl in &warn_lines {
        w.push(one_line(wl));
    }
    o.warnings = w;
    o
}

fn fail(o: &mut AgyTurn, why: &str, class: &str, texts: &[String]) {
    o.ok = false;
    o.outcome = format!("failed: {why}");
    o.class = class.to_string();
    let mut all: Vec<String> = texts.iter().filter(|t| !t.is_empty()).cloned().collect();
    all.push(why.to_string());
    o.texts = all;
}

fn strip_failed(s: &str) -> String {
    re(r"^failed:\s*").replace(s, "").to_string()
}

fn keep(items: &[String]) -> Vec<String> {
    items.iter().filter(|s| !s.is_empty()).cloned().collect()
}

fn re(pat: &str) -> regex::Regex {
    regex::Regex::new(pat).unwrap()
}

fn find(lines: &[String], pat: &str) -> String {
    let r = re(pat);
    lines
        .iter()
        .find(|l| r.is_match(l))
        .cloned()
        .unwrap_or_default()
}

fn one_line(s: &str) -> String {
    c3_core::one_line(s)
}

// --------------------------------------------------------------------------- salvage

/// Salvage the agent-response text of a killed turn for the `.partial.md` file (`Read-AgySalvage`):
/// the `text_delta` chunks of each `agent_response` step, joined; the `result.response` as a
/// fallback when no deltas were emitted. `None` when nothing was produced.
pub fn agy_salvage(events_text: &str) -> Option<String> {
    use std::collections::BTreeMap;
    let mut msgs: BTreeMap<i64, String> = BTreeMap::new();
    let mut response = String::new();
    for line in events_text.split(['\r', '\n']) {
        let t = line.trim();
        if !t.starts_with('{') {
            continue;
        }
        let obj: Value = match serde_json::from_str(t) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ev = obj.get("event").and_then(|v| v.as_str()).unwrap_or("");
        if ev == "result" {
            if let Some(res) = obj.get("result") {
                response = str_field(res, "response");
            }
            continue;
        }
        if ev != "step_update" {
            continue;
        }
        let su = match obj.get("step_update") {
            Some(v) if v.is_object() => v,
            _ => continue,
        };
        if su.get("step_type").and_then(|v| v.as_str()) == Some("agent_response") {
            if let Some(d) = su.get("text_delta").and_then(|v| v.as_str()) {
                if !d.is_empty() {
                    let idx = su.get("step_index").and_then(|v| v.as_i64()).unwrap_or(-1);
                    msgs.entry(idx).or_default().push_str(d);
                }
            }
        }
    }
    let mut parts: Vec<String> = msgs
        .into_values()
        .filter(|s| !s.trim().is_empty())
        .collect();
    if parts.is_empty() && !response.trim().is_empty() {
        parts.push(response);
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

// --------------------------------------------------------------------------- helpers

fn read_text(path: &std::path::Path) -> String {
    std::fs::read(path)
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .unwrap_or_default()
}

fn parse_structured(raw_text: &str) -> Option<StructuredReply> {
    let trimmed = raw_text.trim();
    if !trimmed.starts_with('{') {
        return None;
    }
    let raw: RawReply = serde_json::from_str(trimmed).ok()?;
    StructuredReply::try_from(raw).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("fixtures/agy_ok.events.jsonl");
    const DENIED: &str = include_str!("fixtures/agy_denied.events.jsonl");
    const DENIED_STDERR: &str = include_str!("fixtures/agy_denied.stderr.txt");

    // (wave 2e, F11-5) the agy tool-call classifier: an ACTIVE tool step opens a call keyed by its
    // step index, any other state of that step closes it; other steps and events change nothing.
    #[test]
    fn agy_tool_steps_open_and_close_calls() {
        use super::super::subprocess::ToolCalls;
        let step = |idx: &str, state: &str, ty: &str| {
            format!(
                r#"{{"event":"step_update","step_update":{{"conversation_id":"c","step_index":{idx},"state":"{state}","step_type":"{ty}","tool_name":"run_command"}}}}"#
            )
        };
        let mut calls = ToolCalls::default();
        calls.apply(agy_tool_flight(&step("2", "ACTIVE", "tool")));
        calls.apply(agy_tool_flight(&step("2", "ACTIVE", "tool")));
        assert_eq!(calls.count(), 1);
        assert_eq!(calls.labels(), "agy tool step 2");
        // a text step, a user step, another event: nothing
        assert_eq!(
            agy_tool_flight(&step("1", "ACTIVE", "agent_response")),
            ToolFlight::None
        );
        assert_eq!(
            agy_tool_flight(r#"{"event":"init","conversation_id":"c","init":{"tools":["tool"]}}"#),
            ToolFlight::None
        );
        assert_eq!(agy_tool_flight("not json \"tool\""), ToolFlight::None);
        calls.apply(agy_tool_flight(&step("4", "ACTIVE", "tool")));
        assert_eq!(calls.labels(), "agy tool step 2, agy tool step 4");
        // DONE (or any other state) of step 2 closes it; a close of a step never opened is a no-op
        calls.apply(agy_tool_flight(&step("2", "DONE", "tool")));
        calls.apply(agy_tool_flight(&step("9", "ERROR", "tool")));
        assert_eq!(calls.labels(), "agy tool step 4");
        calls.apply(agy_tool_flight(&step("\"4\"", "CANCELED", "tool")));
        assert_eq!(calls.count(), 0);
        // without a step index: key "agy:", label "agy tool step ?"
        assert_eq!(
            agy_tool_flight(
                r#"{"event":"step_update","step_update":{"state":"ACTIVE","step_type":"tool"}}"#
            ),
            ToolFlight::Open {
                key: "agy:".into(),
                label: "agy tool step ?".into()
            }
        );
    }

    #[test]
    fn stdin_is_one_ndjson_user_line() {
        let s = convert_to_agy_stdin("hello \"world\"");
        assert!(s.ends_with('\n'));
        let v: Value = serde_json::from_str(s.trim()).unwrap();
        assert_eq!(v["event"], "user");
        assert_eq!(v["message"]["content"], "hello \"world\"");
    }

    #[test]
    fn ok_stream_yields_structured_reply_and_usage() {
        let e = read_agy_events(OK, false);
        assert!(e.malformed.is_empty());
        assert!(e.has_result);
        assert_eq!(e.status, "SUCCESS");
        assert!(e.has_structured);
        assert!(is_uuid(&e.thread));
        let u = e.usage.clone().unwrap();
        assert_eq!(u.input_tokens, 13000);
        assert_eq!(u.cached_input_tokens, 4000); // cache_read_tokens
        assert_eq!(u.reasoning_output_tokens, 120); // thinking_tokens
        assert_eq!(u.total_tokens, Some(13500));
        let t = agy_turn_outcome(&e, 0, "", "", "");
        assert!(t.ok, "outcome: {}", t.outcome);
        assert!(t.structured);
        assert!(!t.denied_empty);
        assert!(parse_structured(&t.reply).is_some());
    }

    #[test]
    fn denied_and_empty_flags_the_denial_retry() {
        let e = read_agy_events(DENIED, false);
        assert!(e.has_result);
        assert_eq!(e.status, "SUCCESS");
        assert!(!e.has_structured);
        assert!(e.response.trim().is_empty());
        assert_eq!(e.denied_action, "RunCommand");
        let t = agy_turn_outcome(&e, 0, DENIED_STDERR, "", "");
        assert!(!t.ok);
        assert!(t.denied_empty, "expected the F11 denied-empty case");
        assert_eq!(t.class, "permission");
        assert_eq!(t.permission, "command");
        assert!(
            is_uuid(&t.thread),
            "the thread must be verified for the retry resume"
        );
    }

    #[test]
    fn nonzero_exit_is_a_provider_failure() {
        let e = read_agy_events(OK, true);
        let t = agy_turn_outcome(&e, 1, "stream error: provider refused the request", "", "");
        assert!(!t.ok);
        assert!(t.outcome.starts_with("failed: agy exit 1"));
    }

    #[test]
    fn resume_to_a_different_conversation_fails() {
        let e = read_agy_events(OK, false);
        // OK stream's conversation id is not this expected parent.
        let t = agy_turn_outcome(&e, 0, "", "", "00000000-0000-0000-0000-000000000000");
        assert!(!t.ok);
        assert_eq!(t.class, "unknown");
        assert!(t.outcome.contains("not found"));
    }

    #[test]
    fn malformed_trailing_garbage_on_clean_exit() {
        let s = format!("{OK}\nthis is not json");
        let e = read_agy_events(&s, false);
        assert!(!e.malformed.is_empty());
        let t = agy_turn_outcome(&e, 0, "", "", "");
        assert_eq!(t.class, "transport");
    }

    #[test]
    fn salvage_prefers_agent_response_deltas() {
        // A killed turn: two text_delta chunks on the same agent_response step.
        let s = concat!(
            r#"{"event":"init","conversation_id":"x"}"#,
            "\n",
            r#"{"event":"step_update","step_update":{"step_index":3,"step_type":"agent_response","text_delta":"partial "}}"#,
            "\n",
            r#"{"event":"step_update","step_update":{"step_index":3,"step_type":"agent_response","text_delta":"answer"}}"#,
        );
        assert_eq!(agy_salvage(s).as_deref(), Some("partial answer"));
        assert!(agy_salvage("").is_none());
    }
}
