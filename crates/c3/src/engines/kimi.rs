//! The `kimi` engine adapter: run one Moonshot Kimi CLI turn
//! (`kimi -p <prompt> --output-format stream-json -m <model>`) and turn its stream-json records
//! into an [`AttemptOutcome`].
//!
//! The argv is the plugin's (built by [`c3_core::engine::SubprocessEngine::plan_kimi`]); the prompt
//! rides in argv (`-p <prompt>`) and stdin stays empty (delivery [`PromptDelivery::PromptFile`],
//! like muse/grok, so the runner writes nothing to stdin). The stream-json records
//! ([`super::subprocess`] captured them to `.events.jsonl`) are parsed by [`read_kimi_events`] and
//! judged by [`kimi_turn_outcome`].
//!
//! The stream-json shape (kimi 0.27.0) is ROLE-based, not type-based:
//! - `{"role":"assistant","content":"<text>"}` - a final-answer chunk (concatenated into the reply);
//! - `{"role":"assistant","tool_calls":[{"function":{"name":"Read",...}},...]}` - tool call(s) the
//!   model issued (the evidence it read the code); a line may carry BOTH `content` (narration) and
//!   `tool_calls` - then the content is narration and is NOT part of the reply;
//! - `{"role":"tool","tool_call_id":"...","content":"..."}` - a tool result (ignored);
//! - `{"role":"meta","type":"session.resume_hint","session_id":"session_<uuid>",...}` - the LAST
//!   line of a clean run; the session id is READ from it (never minted) and resumed with `-S <id>`.
//!
//! kimi reports no token usage and has no stream-level error record: a start failure (bad model id,
//! not signed in, quota) comes on STDERR (`error: failed to run prompt: ...`) with a non-zero exit;
//! a benign libuv `Assertion failed: ...` shutdown line on stderr is ignored. kimi bills the Kimi
//! Code plan through a managed oauth sign-in; there is no per-token API-key billing route to guard
//! (unlike grok/muse), so the child inherits this process's environment (minus the host markers,
//! scrubbed by [`run_turn`]). kimi has no read-only sandbox flag in `-p` mode, so read-only is
//! requested in the prompt and enforced by the orchestrator's [`super::tree_check`] (any tree
//! change fails the run).

use std::path::{Path, PathBuf};

use c3_core::engine::{
    AttemptOutcome, Capabilities, ConversationId, ConversationTrust, Engine, EngineError,
    EngineKind, LaunchPlan, Mode, RawReply, Reply, Request, StructuredReply, SubprocessEngine,
    TurnKind, TurnRequest, KIMI_MAX_ARGV_PROMPT,
};
use c3_core::health::{convert_from_provider_error_text, provider_failure_class};
use serde_json::Value;

use super::codex::TurnFiles;
use super::subprocess::{run_turn, SpawnRequest, ToolFlight};

/// The runtime `kimi` engine: the resolved launcher, the working directory (repo root) and the
/// per-turn stream files. `plan`/`capabilities` delegate to the core [`SubprocessEngine`].
#[derive(Clone)]
pub struct KimiEngine {
    pub launcher: String,
    pub cwd: PathBuf,
    /// The primary turn's streams.
    pub primary: TurnFiles,
    /// The secondary turn's streams (timeout continuation).
    pub secondary: TurnFiles,
    /// The stall cut in seconds for the PRIMARY turn (`0` = off).
    pub stall_sec: i64,
    /// The consultation purpose (`acceptance`, `framing`, ...). The zero-tool-call review guard
    /// ([`kimi_review_tool_guard`]) reads it on the PRIMARY turn; empty / irrelevant for a secondary
    /// (timeout-continuation) turn.
    pub purpose: String,
    /// The operator's kick file for the PRIMARY turn (`None` = no kick watch).
    pub kick_path: Option<PathBuf>,
    /// Called once per turn, right after the child spawns and before it is waited on.
    pub on_running: Option<std::sync::Arc<dyn Fn(u32, String) + Send + Sync>>,
    /// Told around every tree kill of a turn (`SpawnRequest::on_kill`).
    pub on_kill: Option<super::subprocess::KillHook>,
}

/// One kimi stream-json line's effect on the tool calls in flight (`Update-ToolFlight`): an
/// `assistant` line carrying `tool_calls` OPENS the first call (key `kimi:<id>`, label
/// `kimi <name> <id>`); a `tool` line CLOSES the call with that `tool_call_id`. A single assistant
/// line may open several calls - only the first is tracked (best-effort; a kimi tool call is a fast
/// read and the stall timer also resets on any stream growth). Suspends the stall timer while a
/// kimi tool call runs.
pub fn kimi_tool_flight(line: &str) -> ToolFlight {
    let v: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return ToolFlight::None,
    };
    let role = v.get("role").and_then(|r| r.as_str()).unwrap_or("");
    match role {
        "assistant" => {
            let first = v
                .get("tool_calls")
                .and_then(|t| t.as_array())
                .and_then(|a| a.first());
            if let Some(tc) = first {
                let id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("");
                let name = tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("tool");
                if id.is_empty() {
                    ToolFlight::Open {
                        key: format!("kimi-tool:{name}#"),
                        label: format!("kimi {name}"),
                    }
                } else {
                    ToolFlight::Open {
                        key: format!("kimi:{id}"),
                        label: format!("kimi {name} {id}"),
                    }
                }
            } else {
                ToolFlight::None
            }
        }
        "tool" => {
            let id = v
                .get("tool_call_id")
                .and_then(|i| i.as_str())
                .unwrap_or("");
            ToolFlight::Close {
                key: if id.is_empty() {
                    String::new()
                } else {
                    format!("kimi:{id}")
                },
                fallback_prefix: "kimi-tool:".into(),
            }
        }
        _ => ToolFlight::None,
    }
}

/// Run a git subcommand in `repo`, returning trimmed stdout or an error.
fn git_capture(repo: &Path, args: &[&str]) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// (C3) The throwaway worktree a kimi turn runs in (WRITE ISOLATION): kimi `-p` has no
/// tool-restriction flag and WILL execute writes, so the reviewer never runs in the real checkout.
/// A fresh detached worktree of HEAD under the OS temp is created on [`create`], the kimi child runs
/// with its `cwd`, and the worktree is removed when this guard drops (every success / failure /
/// timeout path), with `git worktree prune` after. The orchestrator's strict tree check on the MAIN
/// tree stays as the second guard.
struct KimiWorktree {
    repo: PathBuf,
    path: PathBuf,
    head: String,
    main_dirty: bool,
    removed: bool,
}

impl KimiWorktree {
    fn create(repo: &Path) -> Result<KimiWorktree, String> {
        let head = git_capture(repo, &["rev-parse", "HEAD"])?;
        let main_dirty = !git_capture(repo, &["status", "--porcelain"])?.is_empty();
        let run_id = uuid::Uuid::new_v4().simple().to_string();
        let path = std::env::temp_dir().join(format!("c3-kimi-wt-{run_id}"));
        let path_s = path.to_string_lossy().to_string();
        git_capture(repo, &["worktree", "add", "--detach", &path_s, "HEAD"])?;
        Ok(KimiWorktree {
            repo: repo.to_path_buf(),
            path,
            head,
            main_dirty,
            removed: false,
        })
    }

    /// The handoff `Isolation:` header line (the worktree path, the short HEAD sha, and - when the
    /// main tree was dirty at launch - a note that the reviewer saw the committed HEAD only).
    fn isolation_line(&self) -> String {
        let short: String = self.head.chars().take(12).collect();
        let mut l = format!(
            "Isolation: the reviewer ran in a throwaway git worktree `{}` of HEAD {short} (removed after the run).",
            self.path.display()
        );
        if self.main_dirty {
            l.push_str(
                " The main working tree was dirty at launch; the reviewer saw the committed HEAD only.",
            );
        }
        l
    }

    fn remove(&mut self) {
        if self.removed {
            return;
        }
        self.removed = true;
        let path_s = self.path.to_string_lossy().to_string();
        let _ = git_capture(&self.repo, &["worktree", "remove", "--force", &path_s]);
        let _ = git_capture(&self.repo, &["worktree", "prune"]);
        // A belt-and-suspenders directory removal in case `worktree remove` left anything.
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

impl Drop for KimiWorktree {
    fn drop(&mut self) {
        self.remove();
    }
}

/// The value of the `-p` argument in a built kimi argv (the pointer prompt), for the length guard.
fn argv_prompt_value(args: &[String]) -> Option<&str> {
    args.iter()
        .position(|a| a == "-p")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
}

impl KimiEngine {
    fn inner(&self) -> SubprocessEngine {
        SubprocessEngine::new(EngineKind::Kimi)
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
    pub fn run_detailed(&self, turn: &TurnRequest) -> Result<KimiRun, EngineError> {
        let args = self.run_argv(turn)?;
        // (B) The argv-length guard: the pointer is short, so this only fires on a pathological
        // composed value - a clear capability refusal before anything launches, never a silent cut.
        if let Some(p) = argv_prompt_value(&args) {
            if p.len() > KIMI_MAX_ARGV_PROMPT {
                return Ok(KimiRun {
                    outcome: AttemptOutcome::ProviderFailure {
                        failure: c3_core::ledger::ProviderFailure {
                            class: "capability".into(),
                            code: String::new(),
                            message: format!(
                                "the kimi argv prompt is {} chars (> {KIMI_MAX_ARGV_PROMPT}); refusing to launch",
                                p.len()
                            ),
                            ..Default::default()
                        },
                        exit_code: None,
                    },
                    turn: KimiTurn::default(),
                    isolation: None,
                });
            }
        }
        // (A) WRITE ISOLATION: run the kimi child in a throwaway worktree of HEAD, never the real
        // checkout. The guard removes it on every return path (Drop). A failure to create it refuses
        // the run (class capability) rather than silently running in the main tree.
        let worktree = match KimiWorktree::create(&self.cwd) {
            Ok(w) => w,
            Err(e) => {
                return Ok(KimiRun {
                    outcome: AttemptOutcome::ProviderFailure {
                        failure: c3_core::ledger::ProviderFailure {
                            class: "capability".into(),
                            code: String::new(),
                            message: format!("kimi isolation worktree could not be created: {e}"),
                            ..Default::default()
                        },
                        exit_code: None,
                    },
                    turn: KimiTurn::default(),
                    isolation: None,
                });
            }
        };
        let isolation = Some(worktree.isolation_line());
        let files = self.files_for(turn.kind);
        let timeout = std::time::Duration::from_secs_f64(turn.request.timeout_sec.max(0.0));
        let is_primary = matches!(turn.kind, TurnKind::Primary);
        let td = |l: &str| kimi_tool_flight(l);
        let spawn = SpawnRequest {
            launcher: &self.launcher,
            argv: &args,
            // (A) the kimi child runs in the throwaway worktree, not the real checkout.
            cwd: &worktree.path,
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
            // No per-token API-key billing route to guard: the child inherits this process's
            // environment (minus the host markers, scrubbed by `run_turn`).
            env: None,
            // The prompt TEXT travels in argv (`-p`): quote it by the C runtime rules on a batch
            // launcher, like the claude/grok engines (harmless on an `.exe` launcher).
            crt_quote: true,
            budget: None,
        };
        let result = run_turn(&spawn);

        if !result.started {
            return Ok(KimiRun {
                outcome: AttemptOutcome::LaunchFailed {
                    child_exists: false,
                    message: result
                        .error
                        .unwrap_or_else(|| "the kimi process did not start".into()),
                },
                turn: KimiTurn::default(),
                isolation,
            });
        }

        let events_text = read_text(&files.events);
        use super::subprocess::TurnStop;
        let killed = !matches!(result.stop, TurnStop::Exited);
        let allow_partial = killed || result.exit_code != Some(0);
        let events = read_kimi_events(&events_text, allow_partial);

        if killed {
            let conversation = if is_kimi_session(&events.session) {
                ConversationTrust::Candidate(ConversationId(events.session.clone()))
            } else {
                ConversationTrust::None
            };
            let partial = kimi_salvage(&events_text);
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
            return Ok(KimiRun {
                outcome,
                turn: KimiTurn::default(),
                isolation,
            });
        }

        let exit_code = result.exit_code.unwrap_or(-1);
        let expect_thread = Self::expect_thread(turn);
        let mut t = kimi_turn_outcome(&events, exit_code, &result.stderr, &expect_thread);
        // The zero-tool-call review guard (primary turn, a reviewing purpose): see grok.
        if is_primary && t.ok {
            if let Some((why, class)) = kimi_review_tool_guard(&self.purpose, events.tool_calls) {
                fail(&mut t, &why, &class, &[]);
            }
        }
        let outcome = to_outcome(&t, files, result.wall_seconds, exit_code);
        Ok(KimiRun {
            outcome,
            turn: t,
            isolation,
        })
    }
}

/// One turn's outcome plus its parsed detail and the `Isolation:` handoff line (the throwaway
/// worktree the turn ran in; `None` only when no worktree was created — a length-guard or
/// worktree-creation refusal).
#[derive(Debug, Clone)]
pub struct KimiRun {
    pub outcome: AttemptOutcome,
    pub turn: KimiTurn,
    pub isolation: Option<String>,
}

fn to_outcome(t: &KimiTurn, files: &TurnFiles, wall_seconds: f64, exit_code: i32) -> AttemptOutcome {
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
            // kimi reports no token usage.
            usage: None,
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

impl Engine for KimiEngine {
    fn capabilities(&self) -> Capabilities {
        self.inner().capabilities()
    }

    fn plan(&self, request: &Request) -> Result<LaunchPlan, EngineError> {
        self.inner().plan(request)
    }

    fn precheck(&self, _turn: &TurnRequest) -> Result<(), EngineError> {
        // kimi has no launch refusal: the billing route is a managed oauth sign-in (no per-token
        // API key to strip). The sign-in is reported by `providers`.
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

const KIMI_SESSION_RE: &str =
    r"^session_[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$";

/// Whether a string is a kimi session id (`session_<uuid>`).
pub fn is_kimi_session(s: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(KIMI_SESSION_RE).unwrap())
        .is_match(s)
}

fn one_line(s: &str) -> String {
    c3_core::one_line(s)
}

/// The normalized record of one `kimi ... --output-format stream-json` stream.
#[derive(Debug, Clone, Default)]
pub struct KimiEvents {
    pub records: usize,
    /// The number of tool calls (across all `assistant` lines carrying `tool_calls`) - the evidence
    /// the model read the code.
    pub tool_calls: usize,
    /// Why the stream is malformed (`''` when well-formed).
    pub malformed: String,
    /// The reply text: the concatenated `content` of `assistant` lines WITHOUT `tool_calls`
    /// (content that accompanies tool calls is narration and is dropped).
    pub text: String,
    /// The session id of the `session.resume_hint` meta line (`''` when none).
    pub session: String,
    /// A `session.resume_hint` meta line was seen (the end-of-run marker of a clean turn).
    pub has_end: bool,
    /// The message of an in-stream error record (rare; kimi errors come on stderr).
    pub error: String,
}

/// Parse one kimi stream-json stream. `allow_partial_last` lets the last non-empty line be
/// truncated (a killed / non-zero-exit turn).
pub fn read_kimi_events(events_text: &str, allow_partial_last: bool) -> KimiEvents {
    let mut r = KimiEvents::default();
    let lines: Vec<&str> = events_text.split(['\r', '\n']).collect();
    let last_idx = lines.iter().rposition(|l| !l.trim().is_empty());

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
        let role = obj.get("role").and_then(|v| v.as_str()).unwrap_or("");
        match role {
            "assistant" => {
                if let Some(tcs) = obj.get("tool_calls").and_then(|v| v.as_array()) {
                    // A tool-call line: count the calls; its `content` (if any) is narration.
                    r.tool_calls += tcs.len();
                } else if let Some(c) = obj.get("content").and_then(|v| v.as_str()) {
                    r.text.push_str(c);
                }
            }
            "meta" => {
                let ty = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if ty == "session.resume_hint" {
                    r.has_end = true;
                    r.session = obj
                        .get("session_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                }
            }
            "error" => {
                if r.error.is_empty() {
                    r.error = obj
                        .get("content")
                        .or_else(|| obj.get("message"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                }
            }
            // "tool" results and any other role are ignored.
            _ => {
                // A defensive catch for a top-level `{"error":"..."}` without a role.
                if role.is_empty() && r.error.is_empty() {
                    if let Some(e) = obj.get("error").and_then(|v| v.as_str()) {
                        r.error = e.to_string();
                    }
                }
            }
        }
    }
    r
}

/// The error line kimi prints on STDERR for a start failure (`error: failed to run prompt: ...`),
/// one-lined; the benign libuv `Assertion failed: ...` shutdown line is skipped. Falls back to the
/// last meaningful stderr line, else `''`.
pub fn kimi_stderr_error(stderr_text: &str) -> String {
    let lines: Vec<&str> = stderr_text
        .split(['\r', '\n'])
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with("Assertion failed"))
        .collect();
    if let Some(l) = lines.iter().find(|l| l.starts_with("error:")) {
        return one_line(l.trim_start_matches("error:").trim());
    }
    lines.last().map(|l| one_line(l)).unwrap_or_default()
}

// --------------------------------------------------------------------------- outcome rules

/// The failure rules of one kimi turn.
#[derive(Debug, Clone, Default)]
pub struct KimiTurn {
    pub ok: bool,
    pub outcome: String,
    pub class: String,
    pub texts: Vec<String>,
    pub thread: String,
    pub thread_candidate: String,
    pub reply: String,
    pub structured: bool,
    /// The number of tool calls over the stream (the review guard's input).
    pub tool_calls: usize,
    pub warnings: Vec<String>,
}

/// The consultation purposes whose kimi reply must be backed by reading the code: a zero-tool-call
/// answer to one of these cannot have checked anything, so it is not a usable review. (Same set as
/// grok's.)
pub const KIMI_TOOL_REQUIRED_PURPOSES: [&str; 4] =
    ["acceptance", "core-contract", "diff-review", "checkpoint"];

/// The zero-tool-call review guard decision (pure): a turn of one of
/// [`KIMI_TOOL_REQUIRED_PURPOSES`] that made zero tool calls cannot have read the code -> a forced
/// `capability` failure (keeps the reply/thread for the record). `None` otherwise.
pub fn kimi_review_tool_guard(purpose: &str, tool_calls: usize) -> Option<(String, String)> {
    if tool_calls == 0 && KIMI_TOOL_REQUIRED_PURPOSES.contains(&purpose) {
        Some((
            format!(
                "kimi answered without reading any file (0 tool calls) - a {purpose} review must read the code; ask again"
            ),
            "capability".into(),
        ))
    } else {
        None
    }
}

/// Judge one kimi turn. `expect_thread` is the session a resume must land on.
pub fn kimi_turn_outcome(
    events: &KimiEvents,
    exit_code: i32,
    stderr_text: &str,
    expect_thread: &str,
) -> KimiTurn {
    let mut o = KimiTurn {
        tool_calls: events.tool_calls,
        ..Default::default()
    };
    let session = &events.session;
    let is_session = is_kimi_session(session);

    // A non-zero exit is a start/run failure: kimi prints the reason on stderr (bad model id, not
    // signed in, quota). An in-stream error record (rare) is preferred when present.
    if exit_code != 0 {
        let msg = if !events.error.is_empty() {
            one_line(&events.error)
        } else {
            let e = kimi_stderr_error(stderr_text);
            if e.is_empty() {
                format!("kimi exit {exit_code}")
            } else {
                e
            }
        };
        fail(&mut o, &format!("kimi failed - {msg}"), "", &[msg.clone()]);
        if is_session {
            o.thread_candidate = session.clone();
        }
        return o;
    }
    if !events.error.is_empty() {
        fail(
            &mut o,
            &format!("kimi error - {}", one_line(&events.error)),
            "",
            std::slice::from_ref(&events.error),
        );
        return o;
    }
    if !events.malformed.is_empty() {
        fail(
            &mut o,
            &format!("malformed event stream: {}", events.malformed),
            "transport",
            &[],
        );
        if is_session {
            o.thread_candidate = session.clone();
        }
        return o;
    }
    if !events.has_end {
        fail(
            &mut o,
            "no session.resume_hint in the kimi stream (the turn did not finish cleanly)",
            "",
            &[],
        );
        return o;
    }
    if session.is_empty() {
        fail(
            &mut o,
            "the kimi event stream names no session (the resume hint has no session_id)",
            "unknown",
            &[],
        );
        return o;
    }
    if !is_session {
        fail(
            &mut o,
            &format!("the session id '{session}' is not a kimi session id"),
            "unknown",
            &[],
        );
        return o;
    }
    if !expect_thread.is_empty() && session != expect_thread {
        fail(
            &mut o,
            &format!("parent session {expect_thread} not found, kimi used {session}"),
            "unknown",
            &[],
        );
        o.thread_candidate = session.clone();
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
    o
}

fn fail(o: &mut KimiTurn, why: &str, class: &str, texts: &[String]) {
    o.ok = false;
    o.outcome = format!("failed: {why}");
    o.class = class.to_string();
    let mut all: Vec<String> = texts.iter().filter(|t| !t.is_empty()).cloned().collect();
    all.push(why.to_string());
    o.texts = all;
}

// --------------------------------------------------------------------------- salvage

/// Salvage the reply text of a killed turn: the `content` of `assistant` lines without
/// `tool_calls`, concatenated. `None` when nothing was produced.
pub fn kimi_salvage(events_text: &str) -> Option<String> {
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
        if obj.get("role").and_then(|v| v.as_str()) == Some("assistant")
            && obj.get("tool_calls").is_none()
        {
            if let Some(c) = obj.get("content").and_then(|v| v.as_str()) {
                out.push_str(c);
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

    const OK: &str = include_str!("fixtures/kimi_ok.events.jsonl");
    const TOOLS: &str = include_str!("fixtures/kimi_tools.events.jsonl");
    const RESUMED: &str = include_str!("fixtures/kimi_resumed.events.jsonl");

    #[test]
    fn ok_stream_is_a_usable_reply_with_a_session() {
        let e = read_kimi_events(OK, false);
        assert!(e.malformed.is_empty(), "malformed: {}", e.malformed);
        assert!(e.has_end);
        assert!(is_kimi_session(&e.session), "session: {}", e.session);
        assert_eq!(e.text, "OK");
        assert_eq!(e.tool_calls, 0);
        let t = kimi_turn_outcome(&e, 0, "", "");
        assert!(t.ok, "outcome: {}", t.outcome);
        assert_eq!(t.reply, "OK");
        assert_eq!(t.thread, e.session);
    }

    #[test]
    fn tool_calls_are_counted_and_narration_is_dropped_from_the_reply() {
        let e = read_kimi_events(TOOLS, false);
        assert!(e.malformed.is_empty(), "malformed: {}", e.malformed);
        // Two tool calls in one assistant line, plus a later single one = 3.
        assert_eq!(e.tool_calls, 3, "tool calls across all assistant lines");
        // The reply is the FINAL assistant content only (no "I'll grep..." narration).
        assert!(e.text.starts_with("Found it."), "reply: {:?}", e.text);
        assert!(!e.text.contains("I'll grep"), "narration leaked: {:?}", e.text);
        let t = kimi_turn_outcome(&e, 0, "", "");
        assert!(t.ok, "outcome: {}", t.outcome);
        assert_eq!(t.tool_calls, 3);
    }

    #[test]
    fn resume_keeps_the_same_session_id() {
        let e = read_kimi_events(RESUMED, false);
        let sid = e.session.clone();
        assert!(is_kimi_session(&sid));
        // A resume that lands on the expected session is usable.
        let t = kimi_turn_outcome(&e, 0, "", &sid);
        assert!(t.ok, "outcome: {}", t.outcome);
        assert_eq!(t.thread, sid);
        // A resume that lands on a DIFFERENT session fails (lineage/thread mismatch).
        let miss = kimi_turn_outcome(
            &e,
            0,
            "",
            "session_00000000-0000-0000-0000-000000000000",
        );
        assert!(!miss.ok);
        assert!(miss.outcome.contains("not found"));
        assert_eq!(miss.thread_candidate, sid);
    }

    #[test]
    fn nonzero_exit_reads_the_error_off_stderr() {
        // A bad-model-id run: empty stdout, the reason on stderr (plus a benign libuv line).
        let stderr = "error: failed to run prompt: config.invalid: Model \"kimi-code/x\" is not configured in config.toml.\nAssertion failed: !(handle->flags & UV_HANDLE_CLOSING), file src\\win\\async.c, line 76";
        let e = read_kimi_events("", true);
        let t = kimi_turn_outcome(&e, 127, stderr, "");
        assert!(!t.ok);
        assert!(t.outcome.contains("is not configured"), "outcome: {}", t.outcome);
        assert!(!t.outcome.contains("Assertion failed"), "libuv noise leaked");
        assert!(t.thread.is_empty());
    }

    #[test]
    fn no_resume_hint_is_a_failure() {
        // A clean exit but no session.resume_hint (the turn did not finish).
        let stream = r#"{"role":"assistant","content":"partial"}"#;
        let e = read_kimi_events(stream, false);
        assert!(!e.has_end);
        let t = kimi_turn_outcome(&e, 0, "", "");
        assert!(!t.ok);
        assert!(t.outcome.contains("no session.resume_hint"));
    }

    #[test]
    fn review_tool_guard_fires_only_on_a_reviewing_purpose_with_zero_tool_calls() {
        let (why, class) = kimi_review_tool_guard("acceptance", 0).expect("acceptance/0 fires");
        assert!(why.contains("0 tool calls"), "message: {why}");
        assert_eq!(class, "capability");
        assert!(kimi_review_tool_guard("acceptance", 3).is_none());
        assert!(kimi_review_tool_guard("framing", 0).is_none());
        for p in KIMI_TOOL_REQUIRED_PURPOSES {
            assert!(kimi_review_tool_guard(p, 0).is_some(), "{p}/0 must fire");
            assert!(kimi_review_tool_guard(p, 1).is_none(), "{p}/1 must not fire");
        }
        assert!(kimi_review_tool_guard("", 0).is_none());
    }

    #[test]
    fn tool_flight_opens_on_assistant_tool_calls_and_closes_on_tool_result() {
        let open = r#"{"role":"assistant","content":"reading","tool_calls":[{"type":"function","id":"tool_a","function":{"name":"Read","arguments":"{}"}}]}"#;
        let result = r#"{"role":"tool","tool_call_id":"tool_a","content":"1\thi"}"#;
        let answer = r#"{"role":"assistant","content":"done"}"#;
        assert_eq!(kimi_tool_flight(open).delta(), 1);
        assert_eq!(kimi_tool_flight(result).delta(), -1);
        assert_eq!(kimi_tool_flight(answer).delta(), 0);
        assert_eq!(kimi_tool_flight("garbage").delta(), 0);
    }

    #[test]
    fn salvage_keeps_only_final_answer_content() {
        let s = concat!(
            r#"{"role":"assistant","content":"narration","tool_calls":[{"id":"t","function":{"name":"Read"}}]}"#,
            "\n",
            r#"{"role":"tool","tool_call_id":"t","content":"x"}"#,
            "\n",
            r#"{"role":"assistant","content":"the answer"}"#,
        );
        assert_eq!(kimi_salvage(s).as_deref(), Some("the answer"));
        assert!(kimi_salvage("").is_none());
    }
}
