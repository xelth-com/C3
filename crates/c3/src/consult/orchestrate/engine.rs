//! The **per-engine adapters** of a `c3 consult` run: everything that differs between the
//! codex / agy / claude / muse / grok engines once a turn is built and run. It resolves the
//! engine kind and exe binding ([`engine_kind_of`], [`resolve_engine_exe_binding`],
//! [`entry_engine`]), builds the non-codex live request ([`make_live_request_engine`], the
//! claude-only [`ClaudeTurnFields`] / [`fix_claude_lineage`]) and the turn request
//! ([`engine_turn`]), and normalizes each engine's run into the shared [`EngineDetail`]
//! ([`agy_detail`], [`claude_detail`], [`muse_detail`], [`grok_detail`], plus the claude
//! launcher [`claude_engine`] and its init records [`ClaudeRunDetail`] /
//! [`claude_engine_run_extra`]). The pure text work stays in the sibling engine adapters
//! under [`crate::engines`].

use super::*;

/// (0.6.0, wave 29) One claude turn's parsed events and outcome.
#[derive(Debug, Clone, Default)]
pub(crate) struct ClaudeRunDetail {
    pub(super) events: crate::engines::claude::ClaudeEvents,
    pub(super) turn: crate::engines::claude::ClaudeTurn,
    pub(super) killed: bool,
}

/// Map the engine name to its [`EngineKind`] (unknown → codex; unknown is refused earlier).
pub(super) fn engine_kind_of(engine: &str) -> EngineKind {
    match engine {
        "agy" => EngineKind::Agy,
        "muse" => EngineKind::Muse,
        "claude" => EngineKind::Claude,
        "grok" => EngineKind::Grok,
        "http" => EngineKind::Http,
        _ => EngineKind::Codex,
    }
}

/// The consult-path `-EngineExe` binding (`Resolve-EngineExeBinding`, D3): the engine it names —
/// `-Engine`'s, else the `-Provider` roster entry's, else the roster's only non-codex engine.
pub(super) fn resolve_engine_exe_binding(
    engine: &str,
    roster: &c3_core::roster::Roster,
    provider: &str,
    model: &str,
) -> Result<String, String> {
    let others: Vec<&str> = c3_core::lineage::ENGINE_NAMES
        .iter()
        .copied()
        .filter(|e| *e != "codex")
        .collect();
    if !engine.is_empty() {
        if engine == "codex" {
            return Err(format!(
                "-EngineExe names the launcher of an engine other than codex ({}); codex takes -CodexExe.",
                others.join(", ")
            ));
        }
        return Ok(engine.to_string());
    }
    if !provider.is_empty() {
        if let Some(pe) = providers::find_roster_entry(roster, provider, model) {
            let pe_engine = entry_engine(pe);
            if pe_engine != "codex" {
                return Ok(pe_engine);
            }
            return Err(format!(
                "-EngineExe: the roster entry {} for -Provider {provider} is engine codex, which takes -CodexExe.",
                pe.position
            ));
        }
    }
    let mut used: Vec<String> = Vec::new();
    if roster.exists {
        for e in &roster.entries {
            let ee = entry_engine(e);
            if ee != "codex" && !used.contains(&ee) {
                used.push(ee);
            }
        }
    }
    if used.len() == 1 {
        return Ok(used[0].clone());
    }
    if used.len() > 1 {
        Err(format!(
            "-EngineExe is ambiguous: the reviewer roster has entries of the engines {}; pass -Engine <{}> to name the one it launches.",
            used.join(" and "),
            others.join("|")
        ))
    } else {
        Err(format!(
            "-EngineExe names the launcher of an engine other than codex: pass -Engine <{}> with it.",
            others.join("|")
        ))
    }
}

/// A roster entry's engine, defaulting an absent one to `codex`.
pub(super) fn entry_engine(e: &c3_core::roster::RosterEntry) -> String {
    if e.engine.is_empty() {
        "codex".to_string()
    } else {
        e.engine.clone()
    }
}

/// Engine-turn detail carried out of the primary turn (empty/false for codex). The agy/muse
/// finish path uses it for the denial-retry gate, the bridge-outcome text, the forced failure
/// class, the engine warnings and the MSP schema version.
#[derive(Default, Clone)]
pub(super) struct EngineDetail {
    /// The turn's own outcome string (`AgyTurn`/`MuseTurn` `.outcome`), the engine bridge-outcome.
    pub(super) turn_outcome: String,
    /// The forced `provider_failure` class (`''` = classify the texts).
    pub(super) forced_class: String,
    /// A tool was auto-denied and the turn produced nothing (run the agy denial retry).
    pub(super) denied_empty: bool,
    pub(super) denial_line: String,
    pub(super) permission: String,
    pub(super) tool_name: String,
    pub(super) denied_action: String,
    /// (agy only) `tool_name` of every `step_update` tool step of the primary turn, in order. The
    /// tree check classifies these to decide whether an agy tree change FAILS or only WARNS.
    pub(super) agy_tool_names: Vec<String>,
    /// A conversation id observed but not verified (never a thread/parent).
    pub(super) thread_candidate: String,
    /// A verified thread the turn named even on a failure (e.g. agy status ERROR).
    pub(super) thread: String,
    /// The reply text the turn produced, even on a failure (kept as `.reply.json`).
    pub(super) reply: String,
    /// The turn's warnings (denial notices, engine warnings).
    pub(super) warnings: Vec<String>,
    /// The MSP schema version of a muse stream (`None` for agy).
    pub(super) msp_schema_version: Option<i64>,
    /// (M7b-b) The http seat's `reviewer.provider_config` (`{engine, base_url, model, pack,
    /// pack_sha256}`), built by the adapter and applied to `ctx.identity.provider_config` in
    /// `finish` so it lands on the ledger. `None` for every other engine.
    pub(super) http_provider_config: Option<serde_json::Value>,
    /// (STEP 2) The http seat's secondary turn (a format-repair replay or a timeout retry) when one
    /// ran, so the caller records `engine_turns: 2` and the `format_repair` fields. `None` otherwise.
    pub(super) http_secondary: Option<crate::consult::http::HttpSecondary>,
    /// (0.6.0, wave 29) a claude turn's events and outcome (its proof, its resolved model).
    pub(super) claude: Option<ClaudeRunDetail>,
}

/// Build the request for a live primary/secondary turn of the selected engine.
pub(super) fn make_live_request_engine(ctx: &Context, mode: Mode) -> Request {
    let sandbox = sandbox_label(&ctx.o);
    let engine = engine_kind_of(&ctx.engine);
    // The schema flag rides a non-prompt-only, non-raw transport: codex `output-schema`, agy/muse
    // `native`.
    let schema_arg = if !ctx.r.raw
        && (ctx.transport.transport == "output-schema" || ctx.transport.transport == "native")
    {
        ctx.schema_path.clone()
    } else {
        None
    };
    Request {
        prompt: ctx.prompt_text.clone(),
        brief_path: ctx.brief_path.clone(),
        model: if ctx.identity.model_source == "unknown" {
            String::new()
        } else {
            ctx.identity.model.clone()
        },
        provider: if ctx.identity.provider_source.is_empty() {
            String::new()
        } else {
            ctx.identity.provider.clone()
        },
        engine,
        effort: ctx.effort.sent.clone(),
        timeout_sec: ctx.r.timeout_sec as f64,
        mode,
        sandbox,
        schema_path: schema_arg,
        extra_config: with_context_config(&ctx.r.extra_config, &ctx.context_config),
        agent_config: agent_config_for(engine, &ctx.r.extra_config),
        output_last_message: if engine == EngineKind::Codex {
            Some(ctx.last_msg_path.clone())
        } else {
            None
        },
        prompt_file: ctx.prompt_file.clone(),
        max_model_steps: if ctx.o.max_model_steps > 0 {
            Some(ctx.o.max_model_steps as u32)
        } else {
            None
        },
        new_thread: None,
        add_dirs: Vec::new(),
    }
    .claude_turn(ctx)
}

/// (0.6.0, wave 29) The claude turn's own fields on a request: the model every turn sends (D4:
/// the resolved id once known), the minted `--session-id` of a NEW thread, the `--add-dir`s.
pub(super) trait ClaudeTurnFields {
    fn claude_turn(self, ctx: &Context) -> Self;
}

impl ClaudeTurnFields for Request {
    fn claude_turn(mut self, ctx: &Context) -> Self {
        if ctx.engine == "claude" {
            if !ctx.engine_model.is_empty() {
                self.model = ctx.engine_model.clone();
            }
            self.new_thread = if matches!(self.mode, Mode::New) && !ctx.new_thread.is_empty() {
                Some(ctx.new_thread.clone())
            } else {
                None
            };
            self.add_dirs = ctx.add_dirs.clone();
            fix_claude_lineage(&mut self);
        }
        self
    }
}

/// A claude turn may send another model than the identity's (D4: the resolved id): the mode's
/// lineage follows the request's, so the core's cross-lineage guard compares like with like (the
/// lineage of record stays the identity's).
pub(super) fn fix_claude_lineage(r: &mut Request) {
    let l = r.lineage();
    match &mut r.mode {
        Mode::Resume { lineage, .. } | Mode::Fork { lineage, .. } => *lineage = l,
        Mode::New => {}
    }
}

/// Assemble a `TurnRequest` for an engine turn (agy/muse). The continuation is the native thread
/// resume when the request's mode names one.
pub(super) fn engine_turn(ctx: &Context, request: Request, kind: TurnKind) -> TurnRequest {
    let continuation = match &request.mode {
        Mode::Resume { thread, .. } | Mode::Fork { thread, .. } => Some(
            c3_core::engine::Continuation::Native(c3_core::engine::ConversationId(thread.clone())),
        ),
        _ => None,
    };
    TurnRequest {
        request,
        consultation: ConsultationId(ctx.consult_id.clone()),
        attempt: c3_core::engine::AttemptId(ctx.consult_id.clone()),
        kind,
        continuation,
    }
}

pub(super) fn agy_detail(t: &crate::engines::agy::AgyTurn) -> EngineDetail {
    EngineDetail {
        turn_outcome: t.outcome.clone(),
        forced_class: t.class.clone(),
        denied_empty: t.denied_empty,
        denial_line: t.denial_line.clone(),
        permission: t.permission.clone(),
        tool_name: String::new(),
        denied_action: String::new(),
        agy_tool_names: Vec::new(),
        thread_candidate: t.thread_candidate.clone(),
        thread: t.thread.clone(),
        reply: t.reply.clone(),
        warnings: t.warnings.clone(),
        msp_schema_version: None,
        http_provider_config: None,
        http_secondary: None,
        claude: None,
    }
}

/// (0.6.0, wave 29) A claude turn's engine detail.
pub(super) fn claude_detail(r: &crate::engines::claude::ClaudeRun) -> EngineDetail {
    let t = &r.turn;
    EngineDetail {
        turn_outcome: t.outcome.clone(),
        forced_class: t.class.clone(),
        denied_empty: t.denied_empty,
        denial_line: t.denial_line.clone(),
        permission: String::new(),
        tool_name: r.events.denied_action.clone(),
        denied_action: r.events.denied_action.clone(),
        agy_tool_names: Vec::new(),
        thread_candidate: t.thread_candidate.clone(),
        thread: t.thread.clone(),
        reply: t.reply.clone(),
        warnings: t.warnings.clone(),
        msp_schema_version: None,
        http_provider_config: None,
        http_secondary: None,
        claude: Some(ClaudeRunDetail {
            events: r.events.clone(),
            turn: r.turn.clone(),
            killed: r.killed,
        }),
    }
}

/// (0.6.0, wave 29) The claude engine of a run (its launcher, auth and endpoint) with the given
/// streams; the primary turn watches the stall and the kick.
pub(super) fn claude_engine(
    ctx: &Context,
    primary: TurnFiles,
    secondary: TurnFiles,
    stall_sec: i64,
    kick_path: Option<PathBuf>,
    on_running: Option<std::sync::Arc<dyn Fn(u32, String) + Send + Sync>>,
    on_kill: Option<crate::engines::subprocess::KillHook>,
) -> crate::engines::claude::ClaudeEngine {
    crate::engines::claude::ClaudeEngine {
        launcher: ctx.engine_launcher.clone(),
        cwd: ctx.repo_root.clone(),
        primary,
        secondary,
        stall_sec,
        kick_path,
        on_running,
        on_kill,
        auth: ctx.engine_auth.clone(),
        endpoint: ctx.engine_endpoint.clone(),
    }
}

pub(super) fn muse_detail(t: &crate::engines::muse::MuseTurn) -> EngineDetail {
    EngineDetail {
        turn_outcome: t.outcome.clone(),
        forced_class: t.class.clone(),
        denied_empty: false,
        denial_line: String::new(),
        permission: String::new(),
        tool_name: String::new(),
        denied_action: String::new(),
        agy_tool_names: Vec::new(),
        thread_candidate: t.thread_candidate.clone(),
        thread: t.thread.clone(),
        reply: t.reply.clone(),
        warnings: t.warnings.clone(),
        msp_schema_version: None,
        http_provider_config: None,
        http_secondary: None,
        claude: None,
    }
}

pub(super) fn grok_detail(t: &crate::engines::grok::GrokTurn) -> EngineDetail {
    EngineDetail {
        turn_outcome: t.outcome.clone(),
        forced_class: t.class.clone(),
        denied_empty: false,
        denial_line: String::new(),
        permission: String::new(),
        tool_name: String::new(),
        denied_action: String::new(),
        agy_tool_names: Vec::new(),
        thread_candidate: t.thread_candidate.clone(),
        thread: t.thread.clone(),
        reply: t.reply.clone(),
        warnings: t.warnings.clone(),
        msp_schema_version: None,
        http_provider_config: None,
        http_secondary: None,
        claude: None,
    }
}

/// The MSP `schema_version` of a muse event stream (`None` when unreadable).
pub(super) fn muse_msp_version(events_path: &Path) -> Option<i64> {
    let text = std::fs::read_to_string(events_path).unwrap_or_default();
    crate::engines::muse::read_muse_events(&text, true).schema_version
}

/// (0.6.0, wave 29) The claude engine's evidence per run, over the turns that ran
/// (`engine_run` after `msp_schema_version`): the auth, the init events' tools (the union), MCP
/// servers (the most any turn listed), the first init's permission mode and apiKeySource, the
/// resolved model (D4) and the other models a turn named, the permission denials (count and
/// tools), the most severe rate_limit_event as the CLI wrote it (D6), the quota mark (E15), the
/// notional cost, the names of the child environment (D2 - never a value) and what R22 switched
/// off.
pub(super) fn claude_engine_run_extra(
    ctx: &Context,
    sec: &Secondary,
) -> serde_json::Map<String, serde_json::Value> {
    use serde_json::Value;
    let inits: Vec<&ClaudeRunDetail> = sec
        .claude_turns
        .iter()
        .filter(|c| c.events.init_count > 0)
        .collect();
    let resolved = sec
        .claude_turns
        .iter()
        .map(|c| c.turn.model_resolved.clone())
        .rfind(|m| !m.is_empty())
        .unwrap_or_default();
    let mut tools: Vec<String> = Vec::new();
    for c in &inits {
        for t in &c.events.init_tools {
            if !t.is_empty() && !tools.contains(t) {
                tools.push(t.clone());
            }
        }
    }
    let mut other: Vec<String> = Vec::new();
    for c in &sec.claude_turns {
        for m in &c.turn.other_models {
            if !m.is_empty() && !other.contains(m) {
                other.push(m.clone());
            }
        }
    }
    let denials: Vec<&(String, String)> = sec
        .claude_turns
        .iter()
        .flat_map(|c| c.events.denials.iter())
        .collect();
    let mut denied_tools: Vec<String> = Vec::new();
    for (t, _) in &denials {
        if !t.is_empty() && !denied_tools.contains(t) {
            denied_tools.push(t.clone());
        }
    }
    let mut rate: Option<Value> = None;
    for c in sec
        .claude_turns
        .iter()
        .filter(|c| c.events.rate_limit.is_some())
    {
        if rate.is_none() || c.events.rate_limit_rejected {
            rate = c.events.rate_limit.clone();
        }
    }
    let mut cost: Option<f64> = None;
    for c in &sec.claude_turns {
        if let Some(v) = c.events.cost_usd {
            cost = Some(cost.unwrap_or(0.0) + v);
        }
    }
    let first = inits.first();
    let tok = |s: &str| c3_core::claude::token(Some(&Value::String(s.to_string())));
    let names =
        crate::engines::claude_auth::child_env(&ctx.engine_auth, ctx.engine_endpoint.as_ref())
            .names;
    let strs = |v: &[String]| Value::Array(v.iter().map(|s| Value::String(s.clone())).collect());
    let mut m = serde_json::Map::new();
    m.insert("auth".into(), Value::String(ctx.engine_auth.clone()));
    m.insert("init_tools".into(), strs(&tools));
    m.insert(
        "mcp_servers".into(),
        if inits.is_empty() {
            Value::Null
        } else {
            Value::from(
                inits
                    .iter()
                    .map(|c| c.events.init_mcp.len())
                    .max()
                    .unwrap_or(0) as i64,
            )
        },
    );
    m.insert(
        "permission_mode".into(),
        first
            .and_then(|c| c.events.init_modes.first())
            .map(|s| Value::String(tok(s)))
            .unwrap_or(Value::Null),
    );
    m.insert(
        "api_key_source".into(),
        first
            .and_then(|c| c.events.init_key_sources.first())
            .map(|s| Value::String(tok(s)))
            .unwrap_or(Value::Null),
    );
    m.insert(
        "model_resolved".into(),
        if resolved.is_empty() {
            Value::Null
        } else {
            Value::String(resolved)
        },
    );
    m.insert("other_models".into(), strs(&other));
    m.insert(
        "permission_denials".into(),
        Value::from(denials.len() as i64),
    );
    m.insert("denied_tools".into(), strs(&denied_tools));
    m.insert("rate_limit".into(), rate.unwrap_or(Value::Null));
    m.insert(
        "quota_mark".into(),
        sec.claude_quota_mark
            .as_ref()
            .and_then(|q| serde_json::to_value(q).ok())
            .unwrap_or(Value::Null),
    );
    m.insert(
        "cost_usd".into(),
        cost.and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or(Value::Null),
    );
    m.insert("child_env_allowed".into(), strs(&names));
    m.insert(
        "switched_off".into(),
        Value::Array(
            c3_core::claude::CLAUDE_SWITCHED_OFF
                .iter()
                .map(|s| Value::String(s.to_string()))
                .collect(),
        ),
    );
    m
}
