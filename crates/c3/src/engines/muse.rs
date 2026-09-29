//! The `muse` engine adapter: run one Meta Muse Code CLI turn (`muse exec --json
//! --prompt-file ...`) and turn its MSP (Muse Structured Protocol) JSONL stream into an
//! [`AttemptOutcome`].
//!
//! The argv is the plugin's (built by [`c3_core::engine::SubprocessEngine`]); the prompt is the
//! `--prompt-file` named in the argv and stdin stays empty. The MSP records
//! ([`super::subprocess`] captured them to `.events.jsonl`) are parsed by [`read_muse_events`]
//! (`Read-MuseEvents`, fail-closed provenance: exactly one session stream, one linked run, the
//! model and reply bound to it) and judged by [`muse_turn_outcome`] (`Get-MuseTurnOutcome`).
//!
//! Muse bills the subscription only through the browser oauth sign-in; an API key the child
//! would inherit bills per token. [`Engine::precheck`] re-runs the launch guard
//! ([`muse_launch_block`], `Get-MuseLaunchBlock`) before EVERY turn, reading `auth.json` afresh.
//! Muse has no denial retry (its write/shell/web tools are off); read-only is enforced by the
//! orchestrator's [`super::tree_check`], not by muse.

use std::path::{Path, PathBuf};

use c3_core::engine::{
    AttemptOutcome, Capabilities, ConversationId, ConversationTrust, Engine, EngineError,
    EngineKind, LaunchPlan, Mode, RawReply, Reply, Request, StructuredReply, SubprocessEngine,
    TurnKind, TurnRequest,
};
use c3_core::health::{convert_from_provider_error_text, provider_failure_class};
use serde_json::Value;

use super::codex::TurnFiles;
use super::subprocess::{run_turn, SpawnRequest};

/// The runtime `muse` engine: the resolved launcher, the working directory (repo root) and the
/// per-turn stream files. `plan`/`capabilities` delegate to the core [`SubprocessEngine`];
/// `precheck` runs the billing/oauth launch guard.
#[derive(Clone)]
pub struct MuseEngine {
    pub launcher: String,
    pub cwd: PathBuf,
    /// The primary turn's streams.
    pub primary: TurnFiles,
    /// The secondary turn's streams (format repair / timeout continuation).
    pub secondary: TurnFiles,
    /// (wave 26b, D12) The stall cut in seconds for the PRIMARY turn (`0` = off).
    pub stall_sec: i64,
    /// (wave 26b, D10) The operator's kick file for the PRIMARY turn (`None` = no kick watch).
    pub kick_path: Option<PathBuf>,
    /// Called once per turn, right after the child spawns and before it is waited on
    /// (recovery record `launching` -> `running`). `None` leaves the record `launching`.
    pub on_running: Option<std::sync::Arc<dyn Fn(u32, String) + Send + Sync>>,
}

/// (wave 26c, D3) One muse MSP line's effect on the tool-call-in-flight count: `+1` when a
/// `tool.*` task is proposed/started, `-1` when it completes/fails, `0` otherwise. Suspends the
/// stall timer while a muse tool task runs.
pub fn muse_tool_delta(line: &str) -> i64 {
    let v: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return 0,
    };
    let p_type = v.get("payload_type").and_then(|t| t.as_str()).unwrap_or("");
    let kind = v
        .get("payload")
        .and_then(|p| p.get("event"))
        .and_then(|e| e.get("task_kind"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    if !kind.starts_with("tool.") {
        return 0;
    }
    match p_type {
        "task.lifecycle.proposed" | "task.lifecycle.started" => 1,
        "task.lifecycle.completed" | "task.lifecycle.failed" => -1,
        _ => 0,
    }
}

impl MuseEngine {
    fn inner(&self) -> SubprocessEngine {
        SubprocessEngine::new(EngineKind::Muse)
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

    /// Run one turn and return the outcome plus the parsed detail (the thread id for a timeout
    /// continuation, warnings). The trait methods return only the [`AttemptOutcome`].
    pub fn run_detailed(&self, turn: &TurnRequest) -> Result<MuseRun, EngineError> {
        let args = self.run_argv(turn)?;
        let files = self.files_for(turn.kind);
        let timeout = std::time::Duration::from_secs_f64(turn.request.timeout_sec.max(0.0));
        let is_primary = matches!(turn.kind, TurnKind::Primary);
        let td = |l: &str| muse_tool_delta(l);
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
            // (wave 26c, D1) the caller scopes `kick_path` (primary always; the format-repair turn
            // so a kicked repair leaves the first reply standing; a continuation passes None).
            kick_path: self.kick_path.as_deref(),
            tool_delta: Some(&td),
            on_running: self
                .on_running
                .as_ref()
                .map(|a| a.as_ref() as &dyn Fn(u32, String)),
        };
        let result = run_turn(&spawn);

        if !result.started {
            return Ok(MuseRun {
                outcome: AttemptOutcome::LaunchFailed {
                    child_exists: false,
                    message: result
                        .error
                        .unwrap_or_else(|| "the muse process did not start".into()),
                },
                turn: MuseTurn::default(),
            });
        }

        let events_text = read_text(&files.events);
        use super::subprocess::TurnStop;
        let killed = !matches!(result.stop, TurnStop::Exited);
        let allow_partial = killed || result.exit_code != Some(0);
        let events = read_muse_events(&events_text, allow_partial);

        if killed {
            let conversation = if is_uuid(&events.session) {
                ConversationTrust::Candidate(ConversationId(events.session.clone()))
            } else {
                ConversationTrust::None
            };
            let partial = muse_salvage(&events_text);
            let survivors = result.survivors.clone();
            let wall_seconds = result.wall_seconds;
            let outcome = match result.stop {
                TurnStop::Stall => AttemptOutcome::Stopped {
                    kind: c3_core::engine::StopKind::Stall {
                        silent_seconds: result.silent_seconds,
                        last_event: result.last_event.clone(),
                    },
                    partial,
                    survivors,
                    conversation,
                    wall_seconds,
                },
                TurnStop::Kick => AttemptOutcome::Stopped {
                    kind: c3_core::engine::StopKind::Kick,
                    partial,
                    survivors,
                    conversation,
                    wall_seconds,
                },
                _ => AttemptOutcome::TimedOut {
                    partial,
                    survivors,
                    conversation,
                    wall_seconds,
                },
            };
            return Ok(MuseRun {
                outcome,
                turn: MuseTurn::default(),
            });
        }

        let exit_code = result.exit_code.unwrap_or(-1);
        let expect_thread = Self::expect_thread(turn);
        let t = muse_turn_outcome(
            &events,
            exit_code,
            &result.stderr,
            "",
            &expect_thread,
            &turn.request.model,
        );
        let outcome = to_outcome(&t, files, result.wall_seconds, exit_code);
        Ok(MuseRun { outcome, turn: t })
    }
}

/// One turn's outcome plus its parsed detail.
#[derive(Debug, Clone)]
pub struct MuseRun {
    pub outcome: AttemptOutcome,
    pub turn: MuseTurn,
}

fn to_outcome(
    t: &MuseTurn,
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
            usage: None, // MSP records carry no token usage.
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

impl Engine for MuseEngine {
    fn capabilities(&self) -> Capabilities {
        self.inner().capabilities()
    }

    fn plan(&self, request: &Request) -> Result<LaunchPlan, EngineError> {
        self.inner().plan(request)
    }

    fn precheck(&self, _turn: &TurnRequest) -> Result<(), EngineError> {
        // The billing/oauth launch guard lives once, in `providers` (`Get-MuseLaunchBlock`): it
        // re-reads `auth.json` afresh, so this is safe to call before EVERY turn. Empty = ok.
        let block = crate::providers::get_muse_launch_block();
        if block.is_empty() {
            Ok(())
        } else {
            Err(EngineError::Precheck(block))
        }
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

/// The normalized record of one `muse exec --json` MSP stream (`Read-MuseEvents`).
#[derive(Debug, Clone, Default)]
pub struct MuseEvents {
    pub records: usize,
    /// Why the stream is malformed (`''` when well-formed).
    pub malformed: String,
    pub schema_version: Option<i64>,
    /// The distinct session stream ids (exactly one expected).
    pub sessions: Vec<String>,
    /// The session id when exactly one, `''` otherwise.
    pub session: String,
    /// The run the session links (`"run <id>"`, `''` when none/malformed).
    pub run_stream: String,
    pub terminal_count: usize,
    pub has_terminal: bool,
    /// `payload.terminal`: `completed | failed | cancelled | ...`.
    pub terminal: String,
    pub text: String,
    pub reason: String,
    /// The `model_id` of every `run.model.configured` record, in order.
    pub models: Vec<String>,
    /// The reason of a terminal other than `completed`.
    pub error: String,
}

struct At {
    kind: String,
    id: String,
    line: usize,
    run: String,
}

/// Parse one muse MSP stream, fail-closed on ambiguous provenance. `allow_partial_last` lets the
/// last non-empty line be truncated (a killed / non-zero-exit turn).
pub fn read_muse_events(events_text: &str, allow_partial_last: bool) -> MuseEvents {
    let mut r = MuseEvents::default();
    let lines: Vec<&str> = events_text.split(['\r', '\n']).collect();
    let last_idx = lines.iter().rposition(|l| !l.trim().is_empty());
    let mut sessions: Vec<String> = Vec::new();
    let mut links: Vec<At> = Vec::new();
    let mut model_at: Vec<At> = Vec::new();
    let mut terminal: Option<Value> = None;
    let mut terminal_stream: Option<At> = None;

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
        match obj.get("schema_version").and_then(|v| v.as_i64()) {
            None => {
                if r.malformed.is_empty() {
                    r.malformed =
                        format!("the record at line {} has no integer schema_version", i + 1);
                }
            }
            Some(sv) => {
                if r.schema_version.is_none() {
                    r.schema_version = Some(sv);
                }
                if sv != 1 && r.malformed.is_empty() {
                    r.malformed = format!("unsupported MSP version {sv} (the bridge reads MSP 1)");
                }
            }
        }
        let (s_kind, s_id) = match obj.get("stream") {
            Some(s) if s.is_object() => (
                s.get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                s.get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            ),
            _ => (String::new(), String::new()),
        };
        if s_kind == "session" && !s_id.is_empty() && !sessions.contains(&s_id) {
            sessions.push(s_id.clone());
        }
        let p_type = obj
            .get("payload_type")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let payload = obj.get("payload");
        let mut p_kind = String::new();
        let mut p_run = String::new();
        if let Some(p) = payload {
            if p.is_object() {
                p_kind = p
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if let Some(rs) = p.get("run_stream") {
                    if rs.is_object() {
                        p_run = format!(
                            "{} {}",
                            rs.get("kind").and_then(|v| v.as_str()).unwrap_or(""),
                            rs.get("id").and_then(|v| v.as_str()).unwrap_or("")
                        )
                        .trim()
                        .to_string();
                    }
                }
            }
        }
        let at = At {
            kind: s_kind,
            id: s_id,
            line: i + 1,
            run: p_run,
        };
        if p_type == "session.run.linked" || p_kind == "session_run_linked" {
            links.push(at_clone(&at));
        }
        if p_type == "run.model.configured" || p_kind == "run_model_configured" {
            if let Some(p) = payload {
                if let Some(mid) = p.get("model_id").and_then(|v| v.as_str()) {
                    if !mid.is_empty() {
                        r.models.push(mid.to_string());
                    }
                }
            }
            model_at.push(at_clone(&at));
        }
        if p_kind == "run_terminal" || p_type.starts_with("run.terminal.") {
            r.terminal_count += 1;
            terminal = payload.cloned();
            terminal_stream = Some(at);
        }
    }

    r.sessions = sessions.clone();
    if sessions.len() > 1 && r.malformed.is_empty() {
        r.malformed = format!(
            "{} session streams (exactly one expected): {}",
            sessions.len(),
            sessions.join(", ")
        );
    }
    if r.terminal_count > 1 && r.malformed.is_empty() {
        r.malformed = format!(
            "{} run_terminal records (exactly one expected)",
            r.terminal_count
        );
    }
    if sessions.len() == 1 {
        r.session = sessions[0].clone();
    }
    if let (Some(term), Some(ts)) = (&terminal, &terminal_stream) {
        r.has_terminal = true;
        if term.is_object() {
            r.terminal = term
                .get("terminal")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            r.text = match term.get("text") {
                Some(Value::String(s)) => s.clone(),
                Some(v) if !v.is_null() => serde_json::to_string(v).unwrap_or_default(),
                _ => String::new(),
            };
            r.reason = match term.get("reason") {
                Some(Value::String(s)) => s.clone(),
                Some(v) if !v.is_null() => serde_json::to_string(v).unwrap_or_default(),
                _ => String::new(),
            };
        } else if r.malformed.is_empty() {
            r.malformed = format!(
                "the run_terminal record at line {} has no payload object",
                ts.line
            );
        }
        if r.malformed.is_empty()
            && (ts.kind != "session" || (!r.session.is_empty() && ts.id != r.session))
        {
            let k = if ts.kind.is_empty() {
                "(none)".to_string()
            } else {
                ts.kind.clone()
            };
            r.malformed = format!(
                "the run_terminal record at line {} is on stream {} {}, not on the session stream",
                ts.line, k, ts.id
            );
        }
        if r.terminal != "completed" {
            r.error = r.reason.clone();
        }
    }

    // Fail-closed provenance (F09-3): one session, one linked run, the model + reply bound to it.
    if r.malformed.is_empty() && sessions.len() == 1 {
        let sid = &sessions[0];
        let mut runs: Vec<String> = Vec::new();
        for l in &links {
            if l.kind != "session" || &l.id != sid {
                r.malformed = format!(
                    "ambiguous provenance: the session.run.linked record at line {} is on stream {}, not on the session stream",
                    l.line, stream_ref(&l.kind, &l.id)
                );
                break;
            }
            if !re(r"^run \S+$").is_match(&l.run) {
                r.malformed = format!(
                    "ambiguous provenance: the session.run.linked record at line {} names no run stream",
                    l.line
                );
                break;
            }
            if !runs.contains(&l.run) {
                runs.push(l.run.clone());
            }
        }
        if r.malformed.is_empty() && runs.len() > 1 {
            r.malformed = format!(
                "ambiguous provenance: {} run streams linked to the session (exactly one expected): {}",
                runs.len(),
                runs.join(", ")
            );
        }
        let linked_run = if runs.len() == 1 {
            runs[0].clone()
        } else {
            String::new()
        };
        if r.malformed.is_empty() {
            for m in &model_at {
                let named = if m.run.is_empty() {
                    "no run stream".to_string()
                } else {
                    format!("stream {}", m.run)
                };
                if m.kind != "session" || &m.id != sid {
                    r.malformed = format!(
                        "ambiguous provenance: the run.model.configured record at line {} is on stream {}, not on the session stream",
                        m.line, stream_ref(&m.kind, &m.id)
                    );
                    break;
                }
                if linked_run.is_empty() {
                    r.malformed = format!(
                        "ambiguous provenance: the run.model.configured record at line {} names {}, but no session.run.linked record links a run to the session",
                        m.line, named
                    );
                    break;
                }
                if m.run != linked_run {
                    r.malformed = format!(
                        "ambiguous provenance: the run.model.configured record at line {} names {}, not the run linked to the session ({})",
                        m.line, named, linked_run
                    );
                    break;
                }
            }
        }
        if r.malformed.is_empty() && !linked_run.is_empty() && r.terminal == "completed" {
            if let Some(ts) = &terminal_stream {
                if ts.run != linked_run {
                    let named = if ts.run.is_empty() {
                        "no run stream".to_string()
                    } else {
                        format!("stream {}", ts.run)
                    };
                    r.malformed = format!(
                        "ambiguous provenance: the run_terminal record at line {} names {}, not the run linked to the session ({})",
                        ts.line, named, linked_run
                    );
                }
            }
        }
        if r.malformed.is_empty() {
            r.run_stream = linked_run;
        }
    }
    r
}

fn at_clone(a: &At) -> At {
    At {
        kind: a.kind.clone(),
        id: a.id.clone(),
        line: a.line,
        run: a.run.clone(),
    }
}

fn stream_ref(kind: &str, id: &str) -> String {
    let s = format!("{kind} {id}").trim().to_string();
    if s.is_empty() {
        "(none)".to_string()
    } else {
        s
    }
}

// --------------------------------------------------------------------------- outcome rules

/// The failure rules of one muse turn (`Get-MuseTurnOutcome`). `denied_empty` is always false
/// (muse has no denial retry).
#[derive(Debug, Clone, Default)]
pub struct MuseTurn {
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

const STEP_CAP_RE: &str =
    r"(?i)\bmax(?:imum)?[ _-]?(?:model[ _-]?)?steps?\b|\bstep (?:cap|limit|budget)\b";
const INFO_STDERR_RE: &str = r"(?i)^muse:\s*(workspace root:|agent delegation:)";

/// Judge one muse turn. `expect_thread` is the session a resume must land on; `expect_model` is
/// the model that `run.model.configured` must name (drift is a capability failure).
pub fn muse_turn_outcome(
    events: &MuseEvents,
    exit_code: i32,
    stderr_text: &str,
    pre: &str,
    expect_thread: &str,
    expect_model: &str,
) -> MuseTurn {
    let mut o = MuseTurn::default();
    let info_re = re(INFO_STDERR_RE);
    let lines: Vec<String> = stderr_text
        .split(['\r', '\n'])
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !info_re.is_match(l))
        .collect();
    let warn_re = re(r"(?i)^(muse:\s*)?warning:");
    let warn_lines: Vec<String> = lines
        .iter()
        .filter(|l| warn_re.is_match(l))
        .cloned()
        .collect();
    let err_re = re(r"(?i)^(muse:\s*)?error\b");
    let err_line = lines.iter().find(|l| err_re.is_match(l)).cloned();
    let stderr_tail = err_line
        .clone()
        .or_else(|| lines.last().cloned())
        .unwrap_or_default();

    let session = &events.session;
    let is_uuid_session = is_uuid(session);
    if events.terminal == "completed" {
        o.reply = events.text.clone();
    }
    if o.reply.trim().starts_with('{') {
        o.structured = serde_json::from_str::<Value>(o.reply.trim())
            .map(|v| v.is_object())
            .unwrap_or(false);
    }
    let reason = &events.reason;
    let detail = if !reason.is_empty() {
        reason.clone()
    } else {
        stderr_tail.clone()
    };
    let step_cap_re = re(STEP_CAP_RE);
    let step_cap = !detail.is_empty() && step_cap_re.is_match(&detail);

    if !pre.is_empty() {
        o.outcome = pre.to_string();
        o.texts = keep(&[reason.clone(), detail.clone(), strip_failed(pre)]);
        if is_uuid_session {
            o.thread_candidate = session.clone();
        }
        return o;
    }
    if exit_code != 0 {
        let tail = if detail.is_empty() {
            String::new()
        } else {
            format!(" - {}", one_line(&detail))
        };
        if exit_code == 2 {
            fail(
                &mut o,
                &format!("muse exit 2 (usage error){tail}"),
                "capability",
                std::slice::from_ref(&detail),
            );
        } else if exit_code == 130 || exit_code == 143 {
            fail(
                &mut o,
                &format!("muse exit {exit_code} (stopped by a signal){tail}"),
                "transport",
                std::slice::from_ref(&detail),
            );
        } else if step_cap {
            let extra = if detail.is_empty() {
                String::new()
            } else {
                format!(" ({})", one_line(&detail))
            };
            fail(
                &mut o,
                &format!("muse exit {exit_code} - max model steps reached{extra}"),
                "capability",
                std::slice::from_ref(&detail),
            );
        } else {
            fail(
                &mut o,
                &format!("muse exit {exit_code}{tail}"),
                "",
                &[reason.clone(), stderr_tail.clone()],
            );
        }
        if is_uuid_session && (expect_thread.is_empty() || session == expect_thread) {
            o.thread = session.clone();
        } else if is_uuid_session {
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
    if !events.has_terminal {
        let tail = if stderr_tail.is_empty() {
            String::new()
        } else {
            format!(" - {}", one_line(&stderr_tail))
        };
        fail(
            &mut o,
            &format!("no run_terminal record in the muse event stream{tail}"),
            "",
            std::slice::from_ref(&stderr_tail),
        );
        if is_uuid_session {
            o.thread_candidate = session.clone();
        }
        return o;
    }
    if session.is_empty() {
        fail(
            &mut o,
            "the muse event stream names no session (no stream of kind session)",
            "unknown",
            &[],
        );
        return o;
    }
    if !expect_thread.is_empty() && session != expect_thread {
        fail(
            &mut o,
            &format!("parent session {expect_thread} not found, muse started {session}"),
            "unknown",
            &[],
        );
        if is_uuid_session {
            o.thread_candidate = session.clone();
        }
        return o;
    }
    if events.terminal != "completed" {
        let t = if events.terminal.is_empty() {
            "(none)".to_string()
        } else {
            events.terminal.clone()
        };
        if step_cap {
            let extra = if detail.is_empty() {
                String::new()
            } else {
                format!(" ({})", one_line(&detail))
            };
            fail(
                &mut o,
                &format!("muse terminal {t} - max model steps reached{extra}"),
                "capability",
                std::slice::from_ref(&detail),
            );
        } else {
            let tail = if detail.is_empty() {
                String::new()
            } else {
                format!(" - {}", one_line(&detail))
            };
            fail(
                &mut o,
                &format!("muse terminal {t}{tail}"),
                "",
                &[reason.clone(), stderr_tail.clone()],
            );
        }
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
    if !expect_model.is_empty() {
        if events.models.is_empty() {
            fail(&mut o, &format!("the muse event stream names no configured model (run.model.configured; asked {expect_model})"), "unknown", &[]);
            o.thread_candidate = session.clone();
            return o;
        }
        if let Some(other) = events.models.iter().find(|m| m.as_str() != expect_model) {
            fail(
                &mut o,
                &format!("model drift: asked {expect_model}, served {other}"),
                "capability",
                &[],
            );
            o.thread_candidate = session.clone();
            return o;
        }
    }
    o.thread = session.clone();
    if o.reply.trim().is_empty() {
        fail(&mut o, "empty reply", "", &[]);
        return o;
    }
    o.ok = true;
    o.outcome = "usable reply".into();
    o.warnings = warn_lines.iter().map(|w| one_line(w)).collect();
    o
}

fn fail(o: &mut MuseTurn, why: &str, class: &str, texts: &[String]) {
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

// --------------------------------------------------------------------------- salvage

/// Salvage the reply text of a killed turn (`Read-MuseSalvage`): the `run.output.delta` texts,
/// joined, split at each tool call (`task.lifecycle.proposed` with a `tool.*` kind). `None` when
/// nothing was produced.
pub fn muse_salvage(events_text: &str) -> Option<String> {
    let mut items: Vec<String> = Vec::new();
    let mut cur = String::new();
    for line in events_text.split(['\r', '\n']) {
        let t = line.trim();
        if !t.starts_with('{') {
            continue;
        }
        let obj: Value = match serde_json::from_str(t) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let p_type = obj
            .get("payload_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let payload = match obj.get("payload") {
            Some(p) if p.is_object() => p,
            _ => continue,
        };
        if p_type == "run.output.delta" {
            if let Some(d) = payload.get("text").and_then(|v| v.as_str()) {
                cur.push_str(d);
            }
        } else if p_type == "task.lifecycle.proposed" {
            let kind = payload
                .get("event")
                .and_then(|e| e.get("task_kind"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if kind.starts_with("tool.") && !cur.trim().is_empty() {
                items.push(std::mem::take(&mut cur));
            }
        }
    }
    if !cur.trim().is_empty() {
        items.push(cur);
    }
    if items.is_empty() {
        None
    } else {
        Some(items.join("\n\n"))
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
    use c3_core::engine::MUSE_API_KEY_VARS;

    #[test]
    fn tool_delta_tracks_tool_lifecycle() {
        let proposed = r#"{"payload_type":"task.lifecycle.proposed","payload":{"event":{"task_kind":"tool.shell"}}}"#;
        let completed = r#"{"payload_type":"task.lifecycle.completed","payload":{"event":{"task_kind":"tool.shell"}}}"#;
        assert_eq!(muse_tool_delta(proposed), 1);
        assert_eq!(muse_tool_delta(completed), -1);
        // A non-tool task and a plain output delta are neutral.
        let plan = r#"{"payload_type":"task.lifecycle.proposed","payload":{"event":{"task_kind":"plan.step"}}}"#;
        assert_eq!(muse_tool_delta(plan), 0);
        assert_eq!(
            muse_tool_delta(r#"{"payload_type":"run.output.delta","payload":{"text":"x"}}"#),
            0
        );
        assert_eq!(muse_tool_delta("garbage"), 0);
    }

    const OK: &str = include_str!("fixtures/muse_ok.events.jsonl");
    const FAILED: &str = include_str!("fixtures/muse_failed.events.jsonl");
    const OK_STDERR: &str = "muse: workspace root: C:\\x (cwd default)\nmuse: Agent delegation: auto unavailable: workspace is untrusted.";

    #[test]
    fn ok_stream_is_a_usable_structured_reply() {
        let e = read_muse_events(OK, false);
        assert!(e.malformed.is_empty(), "malformed: {}", e.malformed);
        assert!(e.has_terminal);
        assert_eq!(e.terminal, "completed");
        assert!(is_uuid(&e.session));
        assert!(e.run_stream.starts_with("run "));
        assert_eq!(e.models, vec!["muse-1".to_string()]);
        let t = muse_turn_outcome(&e, 0, OK_STDERR, "", "", "muse-1");
        assert!(t.ok, "outcome: {}", t.outcome);
        assert!(t.structured);
        assert!(parse_structured(&t.reply).is_some());
    }

    #[test]
    fn failed_terminal_is_a_provider_failure_with_reason() {
        let e = read_muse_events(FAILED, false);
        assert!(e.has_terminal);
        assert_eq!(e.terminal, "failed");
        assert_eq!(e.error, "the model hit its usage quota");
        let t = muse_turn_outcome(&e, 1, "", "", "", "muse-1");
        assert!(!t.ok);
        assert!(t.outcome.contains("muse exit 1"));
        // exit != 0 forces no class (Get-MuseTurnOutcome leaves it empty); the failure text is
        // classified downstream. Its first text is the terminal reason, which reads as quota.
        assert!(t.class.is_empty(), "exit-1 leaves the class unforced");
        assert_eq!(t.texts.first().unwrap(), "the model hit its usage quota");
        assert_eq!(
            c3_core::health::provider_failure_class(t.texts.first().unwrap()),
            "quota"
        );
    }

    #[test]
    fn model_drift_is_a_capability_failure() {
        let e = read_muse_events(OK, false);
        let t = muse_turn_outcome(&e, 0, OK_STDERR, "", "", "some-other-model");
        assert!(!t.ok);
        assert_eq!(t.class, "capability");
        assert!(t.outcome.contains("model drift"));
    }

    #[test]
    fn info_stderr_lines_are_not_a_failure_detail() {
        // A no-terminal stream whose only stderr is the two info lines must not read as transport.
        let e = read_muse_events("", false);
        let t = muse_turn_outcome(&e, 0, OK_STDERR, "", "", "");
        assert!(!t.ok);
        assert!(t.outcome.contains("no run_terminal record"));
        // detail from info lines was filtered out.
        assert!(!t.outcome.contains("workspace root"));
    }

    // One test for both launch-guard refusals: env mutation is global, so keep it in a single
    // sequential test rather than two that could race under the parallel test runner.
    #[test]
    fn launch_guard_refusals() {
        // (1) an API key set in this process blocks the launch (fail-closed).
        let key = MUSE_API_KEY_VARS[0];
        let prev_key = std::env::var(key).ok();
        std::env::set_var(key, "sk-xxx");
        let block = crate::providers::get_muse_launch_block();
        assert!(
            !block.is_empty(),
            "the guard must refuse while an API key is set"
        );
        assert!(block.contains(key));
        assert!(block.contains("bill per token"));
        match prev_key {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }

        // (2) a temp home with an auth.json whose mechanism is not oauth → refusal.
        let dir = std::env::temp_dir().join(format!("c3-muse-guard-{}", std::process::id()));
        let cfg = dir.join(".config").join("muse");
        std::fs::create_dir_all(&cfg).unwrap();
        std::fs::write(
            cfg.join("auth.json"),
            r#"{"providers":{"meta":{"mechanism":"apikey"}}}"#,
        )
        .unwrap();
        // Isolate env for the check.
        let prev_backend = std::env::var("TBH_CREDENTIAL_BACKEND").ok();
        let prev_profile = std::env::var("USERPROFILE").ok();
        let prev_home = std::env::var("HOME").ok();
        for k in MUSE_API_KEY_VARS {
            std::env::remove_var(k);
        }
        std::env::set_var("TBH_CREDENTIAL_BACKEND", "file");
        std::env::set_var("USERPROFILE", &dir);
        std::env::set_var("HOME", &dir);

        let block = crate::providers::get_muse_launch_block();

        // Restore.
        match prev_backend {
            Some(v) => std::env::set_var("TBH_CREDENTIAL_BACKEND", v),
            None => std::env::remove_var("TBH_CREDENTIAL_BACKEND"),
        }
        match prev_profile {
            Some(v) => std::env::set_var("USERPROFILE", v),
            None => std::env::remove_var("USERPROFILE"),
        }
        match prev_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            !block.is_empty(),
            "a non-oauth mechanism must refuse the launch"
        );
        assert!(block.contains("apikey"), "block: {block}");
        assert!(block.contains("not oauth"));
    }

    #[test]
    fn salvage_joins_output_deltas() {
        let s = concat!(
            r#"{"payload_type":"run.output.delta","payload":{"text":"half "}}"#,
            "\n",
            r#"{"payload_type":"run.output.delta","payload":{"text":"reply"}}"#,
        );
        assert_eq!(muse_salvage(s).as_deref(), Some("half reply"));
        assert!(muse_salvage("").is_none());
    }
}
