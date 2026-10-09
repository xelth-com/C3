//! The `claude` engine adapter (plugin 0.6.0, wave 29 / 29b): run one Claude Code headless turn
//! (`claude -p --output-format stream-json --verbose --restricted ...`) and judge its event stream
//! into an [`AttemptOutcome`].
//!
//! A port of `codex-consult-common.ps1` at v0.6.1: `Read-ClaudeEvents`, `Get-ClaudeInitProblem`,
//! `Get-ClaudeServedModelProblem`, `Get-ClaudeTurnOutcome`, `Read-ClaudeSalvage` and the claude
//! branch of `Update-ToolFlight`. The credential side (the child environment, the probes, the
//! sign-in) is [`super::claude_auth`]; the pure tables (the model table, the endpoint object, the
//! allow list, the argv) live in [`c3_core::claude`].
//!
//! Every claude child - every turn, the sign-in check and the version probe - starts with THE
//! child environment of its auth ([`super::claude_auth::child_env`]: an allow list, never a scrub list; D2/D3): the
//! host markers, the test-mode variables, every other `ANTHROPIC_*` / `CLAUDE_*` variable and the
//! operator's own variables are absent, `DISABLE_AUTOUPDATER=1` is set, and auth `endpoint` adds
//! its route's `ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN` (the value of the variable its
//! `env_key` names, read at the launch, never logged) and `API_TIMEOUT_MS`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;

use c3_core::claude::{
    model_match, ClaudeEndpoint, CLAUDE_AUTH_MODES, CLAUDE_PERMISSION_MODE, CLAUDE_TOOLS,
};
use c3_core::engine::{
    AttemptOutcome, Capabilities, ConversationId, ConversationTrust, Engine, EngineError,
    EngineKind, LaunchPlan, Mode, RawReply, Reply, Request, StructuredReply, SubprocessEngine,
    TurnKind, TurnRequest,
};
use c3_core::ledger::{ProviderFailure, Usage};

use super::claude_auth::child_env;
use super::codex::TurnFiles;
use super::subprocess::{run_turn, SpawnRequest, ToolFlight};
use crate::consult::secondary::{Salvage, SalvageItem};

fn re(pat: &str) -> regex::Regex {
    regex::Regex::new(pat).expect("a valid built-in regex")
}

fn one_line(s: &str) -> String {
    c3_core::one_line(s)
}

fn is_uuid(s: &str) -> bool {
    re(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$").is_match(s)
}

fn str_of(v: Option<&Value>) -> String {
    v.and_then(|x| x.as_str()).unwrap_or("").to_string()
}

// --------------------------------------------------------------------------- the event stream

/// One claude event stream (`Read-ClaudeEvents`), parsed tolerantly into the normalized turn record.
#[derive(Debug, Clone, Default)]
pub struct ClaudeEvents {
    pub init_count: usize,
    /// The FIRST init's session_id, model and cwd.
    pub init_thread: String,
    pub init_model: String,
    pub init_cwd: String,
    /// The distinct values over every init (in order; blanks dropped for threads and models).
    pub init_threads: Vec<String>,
    pub init_models: Vec<String>,
    pub init_modes: Vec<String>,
    pub init_key_sources: Vec<String>,
    /// (A4) any init's apiKeySource absent, null or not a string.
    pub init_key_lacks: bool,
    /// The union of the inits' tools; the names of their MCP servers.
    pub init_tools: Vec<String>,
    pub init_mcp: Vec<String>,
    /// (E14) the capability fields an init lacks, in the order model, permissionMode, tools,
    /// mcp_servers.
    pub init_lacks: Vec<String>,
    pub result_count: usize,
    /// `""` or why the stream is malformed.
    pub malformed: String,
    pub has_result: bool,
    /// The result's session_id.
    pub thread: String,
    pub subtype: String,
    pub is_error: bool,
    /// The result's text (`result.result`).
    pub response: String,
    /// The text of a failed result, else its `errors[]`.
    pub error: String,
    pub has_structured: bool,
    /// `result.structured_output`, serialized compactly.
    pub structured_json: String,
    pub cost_usd: Option<f64>,
    pub num_turns: Option<i64>,
    pub stop_reason: String,
    pub usage: Option<Usage>,
    /// The keys of `result.modelUsage`; the main model is the key with the most output tokens.
    pub model_usage: Vec<String>,
    pub main_model: String,
    /// The distinct `message.model` of the assistant events (`<synthetic>` left out).
    pub assistant_models: Vec<String>,
    /// `result.permission_denials` as (tool, target).
    pub denials: Vec<(String, String)>,
    pub denied_action: String,
    /// The most severe rate_limit_event's info object as the CLI wrote it (D6: recorded raw).
    pub rate_limit: Option<Value>,
    pub rate_limit_status: String,
    pub rate_limit_rejected: bool,
    pub rate_limit_type: String,
    pub rate_limit_reset: Option<DateTime<Utc>>,
    pub compactions: usize,
}

fn add_unique(list: &mut Vec<String>, v: &str) {
    if !list.iter().any(|x| x == v) {
        list.push(v.to_string());
    }
}

/// `ConvertFrom-ClaudeResetTime`: a Unix time (seconds, or milliseconds when it is that large) or
/// an ISO string as a UTC instant, `None` when it is neither.
pub fn reset_time(v: &Value) -> Option<DateTime<Utc>> {
    let from_secs = |n: f64| -> Option<DateTime<Utc>> {
        let n = if n > 1e12 { n / 1000.0 } else { n };
        if !(1e9..=1e10).contains(&n) {
            return None;
        }
        Utc.timestamp_opt(n.floor() as i64, 0).single()
    };
    if let Some(n) = v.as_f64() {
        return from_secs(n);
    }
    let s = v.as_str()?;
    if re(r"^[0-9]{9,14}(\.[0-9]+)?$").is_match(s) {
        return s.parse::<f64>().ok().and_then(from_secs);
    }
    if re(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T").is_match(s) {
        if let Ok(d) = DateTime::parse_from_rfc3339(s) {
            return Some(d.with_timezone(&Utc));
        }
        if let Ok(d) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
            return Some(Utc.from_utc_datetime(&d));
        }
    }
    None
}

/// `Read-ClaudeEvents`: the stream's lines parsed; the LAST non-empty line may be partial only with
/// `allow_partial_last` (a killed turn or a non-zero exit, F10-2).
pub fn read_claude_events(text: &str, allow_partial_last: bool) -> ClaudeEvents {
    let mut r = ClaudeEvents::default();
    if text.is_empty() {
        return r;
    }
    let lines: Vec<&str> = text.split('\n').map(|l| l.trim_end_matches('\r')).collect();
    let last_idx = lines.iter().rposition(|l| !l.trim().is_empty());
    let mut result: Option<Value> = None;
    let mut result_line: usize = 0;
    let mut after_result = false;
    let mut threads: Vec<String> = Vec::new();
    let mut models: Vec<String> = Vec::new();
    let mut lacks: Vec<String> = Vec::new();
    let mut rl_rank: i32 = -1;
    for (i, raw) in lines.iter().enumerate() {
        let t = raw.trim();
        if t.is_empty() {
            continue;
        }
        let obj: Value = match serde_json::from_str::<Value>(t) {
            Ok(v) if v.is_object() => v,
            _ => {
                if (Some(i) != last_idx || !allow_partial_last) && r.malformed.is_empty() {
                    r.malformed = format!("line {} is not a JSON object", i + 1);
                }
                continue;
            }
        };
        if result.is_some() {
            after_result = true;
        }
        let ty = str_of(obj.get("type"));
        let sub = str_of(obj.get("subtype"));
        if ty == "system" && sub == "init" {
            r.init_count += 1;
            let sid = str_of(obj.get("session_id"));
            let mdl = str_of(obj.get("model"));
            if r.init_count == 1 {
                r.init_thread = sid.clone();
                r.init_model = mdl.clone();
                r.init_cwd = str_of(obj.get("cwd"));
            }
            add_unique(&mut threads, &sid);
            add_unique(&mut models, &mdl);
            add_unique(&mut r.init_modes, &str_of(obj.get("permissionMode")));
            // (E14) the capability fields must be THERE
            for sf in ["model", "permissionMode"] {
                let ok = obj
                    .get(sf)
                    .and_then(|v| v.as_str())
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false);
                if !ok {
                    add_unique(&mut lacks, sf);
                }
            }
            for af in ["tools", "mcp_servers"] {
                if !obj.get(af).map(|v| v.is_array()).unwrap_or(false) {
                    add_unique(&mut lacks, af);
                }
            }
            // (A4) apiKeySource: recorded when a string, else the billing proof lacks
            match obj.get("apiKeySource").and_then(|v| v.as_str()) {
                Some(ks) => add_unique(&mut r.init_key_sources, ks),
                None => r.init_key_lacks = true,
            }
            if let Some(arr) = obj.get("tools").and_then(|v| v.as_array()) {
                for tn in arr {
                    if tn.is_null() {
                        continue;
                    }
                    let name = tn
                        .as_str()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| tn.to_string());
                    add_unique(&mut r.init_tools, &name);
                }
            }
            if let Some(arr) = obj.get("mcp_servers").and_then(|v| v.as_array()) {
                for ms in arr {
                    if ms.is_null() {
                        continue;
                    }
                    let mn = if ms.is_object() {
                        str_of(ms.get("name"))
                    } else {
                        ms.as_str()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| ms.to_string())
                    };
                    add_unique(
                        &mut r.init_mcp,
                        if mn.is_empty() { "(unnamed)" } else { &mn },
                    );
                }
            }
        } else if ty == "system" && sub == "compact_boundary" {
            r.compactions += 1;
        } else if ty == "assistant" {
            let am = str_of(obj.get("message").and_then(|m| m.get("model")));
            if !am.is_empty() && am != "<synthetic>" {
                add_unique(&mut r.assistant_models, &am);
            }
        } else if ty == "rate_limit_event" {
            let info = match obj.get("rate_limit_info") {
                Some(v) if v.is_object() => v.clone(),
                _ => obj.clone(),
            };
            let st = str_of(info.get("status"));
            let lower = st.to_lowercase();
            let rank = if lower.contains("reject") {
                2
            } else if lower.contains("warn") {
                1
            } else {
                0
            };
            if rank >= rl_rank {
                rl_rank = rank;
                r.rate_limit_status = c3_core::claude::token(info.get("status"));
                r.rate_limit_type = c3_core::claude::token(
                    info.get("rateLimitType")
                        .or_else(|| info.get("rate_limit_type")),
                );
                let reset = info
                    .get("resetsAt")
                    .or_else(|| info.get("resets_at"))
                    .or_else(|| info.get("reset_at"));
                r.rate_limit_reset = reset.and_then(reset_time);
                r.rate_limit = Some(info);
            }
            if rank == 2 {
                r.rate_limit_rejected = true;
            }
        } else if ty == "result" {
            r.result_count += 1;
            result = Some(obj.clone());
            result_line = i;
            after_result = false;
        }
    }
    r.init_threads = threads.into_iter().filter(|s| !s.is_empty()).collect();
    r.init_models = models.into_iter().filter(|s| !s.is_empty()).collect();
    r.init_lacks = ["model", "permissionMode", "tools", "mcp_servers"]
        .iter()
        .filter(|f| lacks.iter().any(|l| l == *f))
        .map(|f| f.to_string())
        .collect();
    if r.malformed.is_empty() && r.result_count > 1 {
        r.malformed = format!("{} result events (exactly one expected)", r.result_count);
    }
    if r.malformed.is_empty() && after_result {
        r.malformed = format!(
            "an event follows the result event (line {}); the result must be the last",
            result_line + 1
        );
    }
    if r.malformed.is_empty() && r.init_threads.len() > 1 {
        r.malformed = format!(
            "the init events name {} sessions ({})",
            r.init_threads.len(),
            r.init_threads.join(", ")
        );
    }
    if let Some(res) = result {
        r.has_result = true;
        r.thread = str_of(res.get("session_id"));
        r.subtype = str_of(res.get("subtype"));
        r.is_error = res.get("is_error").and_then(|v| v.as_bool()) == Some(true);
        r.response = res
            .get("result")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        r.stop_reason = str_of(res.get("stop_reason"));
        r.num_turns = res.get("num_turns").and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        });
        r.cost_usd = res.get("total_cost_usd").and_then(|v| {
            v.as_f64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        });
        if r.is_error || (!r.subtype.is_empty() && r.subtype != "success") {
            r.error = r.response.clone();
            if r.error.trim().is_empty() {
                let errs: Vec<String> = res
                    .get("errors")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter(|e| !e.is_null())
                            .map(|e| {
                                e.as_str()
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| e.to_string())
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                r.error = errs.join("; ");
            }
        }
        if let Some(so) = res.get("structured_output").filter(|v| v.is_object()) {
            r.has_structured = true;
            r.structured_json = serde_json::to_string(so).unwrap_or_default();
        }
        if let Some(u) = res.get("usage").filter(|v| v.is_object()) {
            let num = |name: &str| -> Option<i64> {
                u.get(name).and_then(|v| {
                    v.as_i64()
                        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                })
            };
            let input = num("input_tokens");
            let cr = num("cache_read_input_tokens");
            let cc = num("cache_creation_input_tokens");
            let out = num("output_tokens");
            // input = input + cache read + cache creation (the whole prompt, as codex counts it)
            let in_all = input.map(|i| i + cr.unwrap_or(0) + cc.unwrap_or(0));
            let mut extra = serde_json::Map::new();
            extra.insert(
                "cache_creation_input_tokens".into(),
                cc.map(Value::from).unwrap_or(Value::Null),
            );
            r.usage = Some(Usage {
                input_tokens: in_all.unwrap_or(0),
                cached_input_tokens: cr.unwrap_or(0),
                output_tokens: out.unwrap_or(0),
                reasoning_output_tokens: 0,
                total_tokens: match (in_all, out) {
                    (Some(a), Some(b)) => Some(a + b),
                    _ => None,
                },
                extra,
            });
        }
        if let Some(mu) = res.get("modelUsage").and_then(|v| v.as_object()) {
            let mut best = -1.0f64;
            for (k, v) in mu {
                r.model_usage.push(k.clone());
                let out = v
                    .get("outputTokens")
                    .and_then(|x| {
                        x.as_f64()
                            .or_else(|| x.as_str().and_then(|s| s.parse().ok()))
                    })
                    .unwrap_or(0.0);
                if out > best {
                    best = out;
                    r.main_model = k.clone();
                }
            }
        }
        if let Some(arr) = res.get("permission_denials").and_then(|v| v.as_array()) {
            for d in arr.iter().filter(|d| d.is_object()) {
                let tin = d.get("tool_input");
                let mut target = String::new();
                for f in [
                    "file_path",
                    "path",
                    "pattern",
                    "notebook_path",
                    "url",
                    "command",
                ] {
                    if let Some(s) = tin.and_then(|t| t.get(f)).and_then(|v| v.as_str()) {
                        if !s.is_empty() {
                            target = one_line(s);
                            break;
                        }
                    }
                }
                if target.chars().count() > 200 {
                    target = target.chars().take(200).collect();
                }
                r.denials.push((str_of(d.get("tool_name")), target));
            }
            if let Some((tool, target)) = r.denials.first() {
                r.denied_action = format!("{tool} {target}").trim().to_string();
            }
        }
    }
    r
}

// --------------------------------------------------------------------------- the turn rules

/// `Get-ClaudeInitProblem`: what the init events of a turn prove (evidence first) - `("", "")`
/// when proven or when there is no init (the caller decides).
pub fn init_problem(e: &ClaudeEvents, auth: &str) -> (String, &'static str) {
    if e.init_count == 0 {
        return (String::new(), "");
    }
    if let Some(lack) = e.init_lacks.first() {
        return (
            format!("init event lacks {lack} - the CLI's schema changed; pin the version"),
            "capability",
        );
    }
    let extra: Vec<&String> = e
        .init_tools
        .iter()
        .filter(|t| !CLAUDE_TOOLS.contains(&t.as_str()))
        .collect();
    if !extra.is_empty() {
        return (
            format!(
                "the init event lists tools outside {}: {} - the turn's read-only capability is not proven",
                CLAUDE_TOOLS.join(", "),
                extra
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "permission",
        );
    }
    if !e.init_mcp.is_empty() {
        return (
            format!(
                "the init event lists MCP server(s) ({}) - a reviewer runs without any",
                e.init_mcp.join(", ")
            ),
            "permission",
        );
    }
    if let Some(bad) = e.init_modes.iter().find(|m| *m != CLAUDE_PERMISSION_MODE) {
        return (
            format!(
                "the init event names the permission mode '{}', not {CLAUDE_PERMISSION_MODE}",
                c3_core::claude::token(Some(&Value::String(bad.clone())))
            ),
            "permission",
        );
    }
    if auth == "endpoint" {
        if e.init_key_sources.iter().any(|k| k == "ANTHROPIC_API_KEY") {
            return ("the init event names apiKeySource ANTHROPIC_API_KEY on an endpoint route - a competing credential reached the child (auth endpoint sends ANTHROPIC_AUTH_TOKEN only)".to_string(), "auth");
        }
        return (String::new(), "");
    }
    if e.init_key_lacks {
        return (
            "init event lacks apiKeySource - the billing proof of this auth mode; pin the CLI version"
                .to_string(),
            "auth",
        );
    }
    let want = if auth == "api-key" {
        "ANTHROPIC_API_KEY"
    } else {
        "none"
    };
    if let Some(bad) = e.init_key_sources.iter().find(|k| *k != want) {
        let shown = if bad.is_empty() {
            "(none named)".to_string()
        } else {
            c3_core::claude::token(Some(&Value::String(bad.clone())))
        };
        return (
            format!(
                "the init event names apiKeySource {shown}, not {want} - the turn did not bill {} the roster names (auth {auth})",
                if auth == "api-key" {
                    "the API key"
                } else {
                    "the claude.ai subscription"
                }
            ),
            "auth",
        );
    }
    (String::new(), "")
}

/// `Get-ClaudeServedModelProblem` (E12, E13): every init model must be the pinned one (an alias
/// takes an id of its family; `exact` - endpoint - equality only), and every assistant message by
/// the init's id. Class capability.
pub fn served_model_problem(e: &ClaudeEvents, pinned: &str, exact: bool) -> (String, &'static str) {
    if e.init_model.is_empty() {
        return (String::new(), "");
    }
    if !pinned.is_empty() {
        if let Some(drift) = e
            .init_models
            .iter()
            .find(|m| !model_match(pinned, m, exact))
        {
            return (
                format!("model drift: asked {pinned}, served {drift}"),
                "capability",
            );
        }
    }
    if let Some(foreign) = e
        .assistant_models
        .iter()
        .find(|m| !model_match(&e.init_model, m, true))
    {
        return (
            format!("a different model authored an assistant message: {foreign}"),
            "capability",
        );
    }
    (String::new(), "")
}

/// (D6) The wording of a usage limit (`$script:ClaudeQuotaRe`).
pub const QUOTA_RE: &str = r"(?i)usage limit|limit reached|hit your (?:usage |session |weekly )?limit|rate[ _]limit|weekly limit|session limit|quota|too many requests|\b429\b|credit balance is too low|overage";
/// (D6, E4) The wording of a missing sign-in or a rejected credential (`$script:ClaudeAuthRe`).
pub const AUTH_RE: &str = r"(?i)not logged in|please run /login|invalid api key|oauth token (?:has )?(?:expired|revoked)|authentication[ _]error|failed to authenticate|api error: 40[13]\b|\b401\b|unauthori[sz]ed";

/// The turn's options as the rules read them (`New-EngineTurnOptions` / `-Turn`).
#[derive(Debug, Clone, Default)]
pub struct ClaudeTurnOpts {
    /// `new` | `resume` | `fork` | `denial-retry` | `format-repair` | `timeout-continue`.
    pub mode: String,
    /// The thread the turn continues (`--resume`); `""` for a new one.
    pub thread: String,
    /// The minted id of a new thread (`--session-id`).
    pub new_thread: String,
    /// The pinned model (the roster's, or the resolved id).
    pub model: String,
    pub auth: String,
}

/// One claude turn's outcome (`Get-ClaudeTurnOutcome`'s result).
#[derive(Debug, Clone, Default)]
pub struct ClaudeTurn {
    pub ok: bool,
    /// `usable reply` or `failed: <why>` (with `pre`: the pre text unless the proof failed).
    pub outcome: String,
    /// The forced class (`''` = classify the texts).
    pub class: String,
    /// The failure evidence, best first.
    pub texts: Vec<String>,
    pub thread: String,
    pub thread_candidate: String,
    pub reply: String,
    pub structured: bool,
    pub denied_empty: bool,
    pub denial_line: String,
    pub warnings: Vec<String>,
    /// (D4) The model the init event resolved (`[1m]` kept when the pin had it).
    pub model_resolved: String,
    pub other_models: Vec<String>,
    /// (E12) The init or model proof failed: no continuation may resume the session.
    pub proof_problem: String,
    /// (E15) A rejecting rate_limit_event beside a successful result: the quota text a failed
    /// turn would carry (the route is marked with it).
    pub quota_mark: String,
}

fn fmt_utc(d: &DateTime<Utc>) -> String {
    d.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// `Get-ClaudeTurnOutcome`: the failure rules of one claude turn. `pre`: a failure the bridge
/// already knows (a kill) - it wins over everything but the proof of the turn's init and model
/// (E12). `expect_thread` / `expect_model` as the plugin's.
pub fn claude_turn_outcome(
    e: &ClaudeEvents,
    exit_code: i32,
    stderr_text: &str,
    pre: &str,
    expect_thread: &str,
    expect_model: &str,
    turn: &ClaudeTurnOpts,
) -> ClaudeTurn {
    let mut o = ClaudeTurn::default();
    let lines: Vec<String> = stderr_text
        .split('\n')
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    let stderr_tail = lines.last().cloned().unwrap_or_default();
    let auth = if CLAUDE_AUTH_MODES.contains(&turn.auth.as_str()) {
        turn.auth.as_str()
    } else {
        "subscription"
    };
    let exact = auth == "endpoint";
    let pinned = if !expect_model.is_empty() {
        expect_model.to_string()
    } else {
        turn.model.clone()
    };
    let fork_of = if turn.mode == "fork" && !turn.thread.is_empty() {
        turn.thread.clone()
    } else {
        String::new()
    };
    let mut expect = expect_thread.to_string();
    if expect.is_empty() && !turn.thread.is_empty() && fork_of.is_empty() {
        expect = turn.thread.clone();
    }
    if expect.is_empty() && turn.thread.is_empty() && !turn.new_thread.is_empty() {
        expect = turn.new_thread.clone();
    }
    let res_id = e.thread.clone();
    let init_id = e.init_thread.clone();
    let ok_id = |id: &str| -> bool {
        is_uuid(id) && (expect.is_empty() || id == expect) && (fork_of.is_empty() || id != fork_of)
    };
    let candidate_of = |id: &str| -> String {
        if is_uuid(id) && (fork_of.is_empty() || id != fork_of) {
            id.to_string()
        } else {
            String::new()
        }
    };
    o.structured = e.has_structured;
    o.reply = if e.has_structured {
        e.structured_json.clone()
    } else if !e.is_error {
        e.response.clone()
    } else {
        String::new()
    };
    // (D6) the quota evidence: a rejecting rate_limit_event, else the limit wording of the
    // result's text or stderr; its reset time (the event's, else a "|<unix time>" in the text)
    let err_text = e.error.clone();
    let quota_re = re(QUOTA_RE);
    let auth_re = re(AUTH_RE);
    let mut quota_text = String::new();
    let mut reset_at = e.rate_limit_reset;
    if e.rate_limit_rejected {
        quota_text = format!(
            "usage limit reached (claude rate_limit_event {}{})",
            e.rate_limit_status,
            if e.rate_limit_type.is_empty() {
                String::new()
            } else {
                format!(", {}", e.rate_limit_type)
            }
        );
    } else if !err_text.is_empty() && quota_re.is_match(&err_text) {
        quota_text = format!("usage limit: {}", one_line(&err_text));
    } else if let Some(ql) = lines.iter().find(|l| quota_re.is_match(l)) {
        quota_text = format!("usage limit: {}", one_line(ql));
    }
    if !quota_text.is_empty() && reset_at.is_none() {
        if let Some(c) = re(r"\|(?P<t>[0-9]{10,13})\b").captures(&err_text) {
            reset_at = reset_time(&Value::String(c["t"].to_string()));
        }
    }
    if !quota_text.is_empty() {
        if let Some(r) = reset_at {
            quota_text.push_str(&format!("; resets at {}", fmt_utc(&r)));
        }
    }
    let mut auth_text = String::new();
    if quota_text.is_empty() {
        if !err_text.is_empty() && auth_re.is_match(&err_text) {
            auth_text = one_line(&err_text);
        } else if let Some(al) = lines.iter().find(|l| auth_re.is_match(l)) {
            auth_text = one_line(al);
        }
    }
    let evidence_class = if !quota_text.is_empty() {
        "quota"
    } else if !auth_text.is_empty() {
        "auth"
    } else {
        ""
    };
    let fail = |o: &mut ClaudeTurn, why: &str, class: &str, texts: &[&str]| {
        o.ok = false;
        o.outcome = format!("failed: {why}");
        o.class = class.to_string();
        let mut all: Vec<String> = texts
            .iter()
            .filter(|t| !t.is_empty())
            .map(|t| t.to_string())
            .collect();
        if !why.is_empty() {
            all.push(why.to_string());
        }
        o.texts = all;
    };
    let detail = if !err_text.trim().is_empty() {
        err_text.clone()
    } else {
        stderr_tail.clone()
    };
    if !pre.is_empty() {
        // (E12) a killed turn's init and model are judged too
        let (mut problem, mut class) = init_problem(e, auth);
        if problem.is_empty() {
            (problem, class) = served_model_problem(e, &pinned, exact);
        }
        if !problem.is_empty() {
            let stopped = one_line(&re(r"^failed:\s*").replace(pre, ""));
            fail(
                &mut o,
                &format!("{problem} (the turn was also stopped: {stopped})"),
                class,
                &[&quota_text, &err_text, &stderr_tail],
            );
            o.proof_problem = problem;
            o.thread_candidate = candidate_of(if res_id.is_empty() { &init_id } else { &res_id });
            return o;
        }
        o.outcome = pre.to_string();
        o.class = if quota_text.is_empty() {
            String::new()
        } else {
            "quota".into()
        };
        let pre_tail = re(r"^failed:\s*").replace(pre, "").to_string();
        o.texts = [
            quota_text.as_str(),
            err_text.as_str(),
            stderr_tail.as_str(),
            pre_tail.as_str(),
        ]
        .iter()
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect();
        o.thread_candidate = candidate_of(if res_id.is_empty() { &init_id } else { &res_id });
        if !o.thread_candidate.is_empty() && !expect.is_empty() && o.thread_candidate != expect {
            o.thread_candidate = String::new();
        }
        // (D4) a killed turn's init already resolved the model: the continuation sends that id
        let served = e.init_model.clone();
        if !served.is_empty() && (pinned.is_empty() || model_match(&pinned, &served, exact)) {
            o.model_resolved = served.clone();
            if re(r"(?i)\[1m\]$").is_match(&pinned) && !re(r"(?i)\[1m\]$").is_match(&served) {
                o.model_resolved.push_str("[1m]");
            }
        }
        return o;
    }
    let (problem, class) = init_problem(e, auth);
    if !problem.is_empty() {
        fail(&mut o, &problem, class, &[]);
        o.proof_problem = problem;
        o.thread_candidate = candidate_of(if res_id.is_empty() { &init_id } else { &res_id });
        return o;
    }
    if exit_code != 0 {
        let why = format!(
            "claude exit {exit_code}{}",
            if detail.is_empty() {
                String::new()
            } else {
                format!(" - {}", one_line(&detail))
            }
        );
        fail(
            &mut o,
            &why,
            evidence_class,
            &[&quota_text, &auth_text, &err_text, &stderr_tail],
        );
        if ok_id(&res_id) {
            o.thread = res_id.clone();
        } else {
            o.thread_candidate = candidate_of(if res_id.is_empty() { &init_id } else { &res_id });
        }
        return o;
    }
    if !e.malformed.is_empty() {
        fail(
            &mut o,
            &format!("malformed event stream: {}", e.malformed),
            "transport",
            &[],
        );
        o.thread_candidate = candidate_of(&init_id);
        return o;
    }
    if e.init_count == 0 {
        fail(&mut o, "the claude event stream has no init event - the turn's tools, MCP servers and permission mode are not proven", "permission", &[]);
        return o;
    }
    if !e.has_result {
        let why = format!(
            "no result event in the claude event stream{}",
            if stderr_tail.is_empty() {
                String::new()
            } else {
                format!(" - {}", one_line(&stderr_tail))
            }
        );
        fail(
            &mut o,
            &why,
            evidence_class,
            &[&quota_text, &auth_text, &stderr_tail],
        );
        o.thread_candidate = candidate_of(&init_id);
        return o;
    }
    if !init_id.is_empty() && !res_id.is_empty() && init_id != res_id {
        fail(
            &mut o,
            &format!("session id mismatch: init {init_id}, result {res_id}"),
            "unknown",
            &[],
        );
        o.thread_candidate = candidate_of(&res_id);
        return o;
    }
    if !expect.is_empty() && res_id != expect {
        let why = if !turn.thread.is_empty() {
            format!(
                "parent session {expect} not found, claude answered on {}",
                if res_id.is_empty() {
                    "no session"
                } else {
                    &res_id
                }
            )
        } else {
            format!(
                "the new session is {}, not the minted {expect}",
                if res_id.is_empty() { "(none)" } else { &res_id }
            )
        };
        fail(&mut o, &why, "unknown", &[]);
        o.thread_candidate = candidate_of(&res_id);
        return o;
    }
    if !fork_of.is_empty() && res_id == fork_of {
        fail(
            &mut o,
            &format!("the fork came back on its parent session {fork_of} (--fork-session started no new session)"),
            "unknown",
            &[],
        );
        return o;
    }
    if !is_uuid(&res_id) {
        fail(
            &mut o,
            &format!("the result's session_id '{res_id}' is not a uuid"),
            "unknown",
            &[],
        );
        return o;
    }
    if e.is_error || e.subtype != "success" {
        let st = if e.subtype.is_empty() {
            "(none)".to_string()
        } else {
            e.subtype.clone()
        };
        let mut cls = evidence_class.to_string();
        if cls.is_empty()
            && (st == "error_max_turns" || st == "error_max_structured_output_retries")
        {
            cls = "capability".into();
        }
        let what = match st.as_str() {
            "error_max_turns" => "max turns reached (--max-turns)",
            "error_max_structured_output_retries" => {
                "no reply satisfied the schema (structured output retries exhausted)"
            }
            _ => "",
        };
        let why = format!(
            "claude {st}{}{}",
            if what.is_empty() {
                String::new()
            } else {
                format!(" - {what}")
            },
            if detail.is_empty() {
                String::new()
            } else {
                format!(" - {}", one_line(&detail))
            }
        );
        fail(
            &mut o,
            &why,
            &cls,
            &[&quota_text, &auth_text, &err_text, &stderr_tail],
        );
        o.thread = res_id.clone();
        return o;
    }
    // (D4) one resolved model per thread, proven per turn
    let served = e.init_model.clone();
    if served.is_empty() {
        fail(
            &mut o,
            &format!("the init event names no model (asked {pinned})"),
            "unknown",
            &[],
        );
        o.thread_candidate = res_id.clone();
        return o;
    }
    let (mp, mc) = served_model_problem(e, &pinned, exact);
    if !mp.is_empty() {
        fail(&mut o, &mp, mc, &[]);
        o.proof_problem = mp;
        o.thread_candidate = res_id.clone();
        return o;
    }
    if !e.model_usage.is_empty()
        && !e.main_model.is_empty()
        && !model_match(&served, &e.main_model, exact)
    {
        let why = format!(
            "model drift: the init event names {served}, the result's modelUsage names {} as the main model",
            e.main_model
        );
        fail(&mut o, &why, "capability", &[]);
        o.proof_problem = why;
        o.thread_candidate = res_id.clone();
        return o;
    }
    let mut resolved = served.clone();
    if re(r"(?i)\[1m\]$").is_match(&pinned) && !re(r"(?i)\[1m\]$").is_match(&resolved) {
        resolved.push_str("[1m]");
    }
    o.model_resolved = resolved;
    for m in e.model_usage.iter().chain(e.assistant_models.iter()) {
        if !m.is_empty() && !model_match(&served, m, exact) && !o.other_models.contains(m) {
            o.other_models.push(m.clone());
        }
    }
    o.thread = res_id.clone();
    let denial_list = e
        .denials
        .iter()
        .take(5)
        .map(|(t, g)| format!("{t} {g}").trim().to_string())
        .collect::<Vec<_>>()
        .join("; ");
    if !e.denials.is_empty() {
        o.denial_line = format!(
            "{} tool call(s) denied under --permission-mode dontAsk: {denial_list}",
            e.denials.len()
        );
    }
    if o.reply.trim().is_empty() {
        if !e.denials.is_empty() {
            let dl = o.denial_line.clone();
            fail(&mut o, &format!("no reply - {dl}"), "permission", &[&dl]);
            o.denied_empty = true;
        } else {
            fail(&mut o, "empty reply", "", &[]);
        }
        return o;
    }
    o.ok = true;
    o.outcome = "usable reply".into();
    let mut w: Vec<String> = Vec::new();
    if !e.denials.is_empty() {
        w.push(format!(
            "permission denials beside the reply: {}",
            o.denial_line
        ));
    }
    // (E15) a rejecting rate_limit_event survives the successful result
    if e.rate_limit_rejected {
        let raw = e
            .rate_limit
            .as_ref()
            .and_then(|v| serde_json::to_string(v).ok())
            .unwrap_or_else(|| e.rate_limit_status.clone());
        w.push(format!(
            "a rate limit rejected a request during the turn: {raw}"
        ));
        o.quota_mark = quota_text.clone();
    }
    if !e.rate_limit_status.is_empty() && e.rate_limit_status.to_lowercase().contains("warn") {
        w.push(format!(
            "claude rate limit status {}{}{}",
            e.rate_limit_status,
            if e.rate_limit_type.is_empty() {
                String::new()
            } else {
                format!(" ({})", e.rate_limit_type)
            },
            e.rate_limit_reset
                .map(|r| format!("; resets at {}", fmt_utc(&r)))
                .unwrap_or_default()
        ));
    }
    if !o.other_models.is_empty() {
        w.push(format!(
            "other models in the turn beside {served}: {} (engine_run.other_models; a helper model of the CLI?)",
            o.other_models.join(", ")
        ));
    }
    for wl in lines.iter().filter(|l| re(r"(?i)^warning:").is_match(l)) {
        w.push(one_line(wl));
    }
    o.warnings = w;
    o
}

/// `New-ProviderFailure`'s choice of evidence: the first text that names a code or carries an
/// `{"error"` payload, else the first text; (code, message) of it.
pub fn failure_evidence(texts: &[String]) -> (String, String) {
    let mut chosen: Option<(String, String)> = None;
    for t in texts.iter().filter(|t| !t.trim().is_empty()) {
        let (code, msg) = c3_core::health::convert_from_provider_error_text(t);
        if !code.is_empty() || t.contains("{\"error\"") {
            return (code, msg);
        }
        if chosen.is_none() {
            chosen = Some((code, msg));
        }
    }
    chosen.unwrap_or_default()
}

// --------------------------------------------------------------------------- salvage + tool flight

/// `Read-ClaudeSalvage` (item 7): the assistant events' text blocks (messages) and thinking blocks
/// (reasoning) in stream order, the result's text when no text block came; a tool_use ->
/// `<tool>: <file_path | path | pattern>`, once per call id.
pub fn claude_salvage(events_text: &str) -> Salvage {
    let mut items: Vec<SalvageItem> = Vec::new();
    let mut tool_keys: Vec<String> = Vec::new();
    let mut tool_map: HashMap<String, String> = HashMap::new();
    let mut response = String::new();
    let mut saw_text = false;
    for line in events_text.split('\n') {
        let t = line.trim();
        if !t.starts_with('{') {
            continue;
        }
        let obj: Value = match serde_json::from_str(t) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ty = str_of(obj.get("type"));
        if ty == "result" {
            if let Some(s) = obj.get("result").and_then(|v| v.as_str()) {
                response = s.to_string();
            }
            continue;
        }
        if ty != "assistant" {
            continue;
        }
        let content = obj
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();
        for b in content.iter().filter(|b| b.is_object()) {
            match str_of(b.get("type")).as_str() {
                "text" => {
                    let tx = str_of(b.get("text"));
                    if !tx.trim().is_empty() {
                        items.push(SalvageItem {
                            kind: "message".into(),
                            text: tx,
                        });
                        saw_text = true;
                    }
                }
                "thinking" => {
                    let tx = str_of(b.get("thinking"));
                    if !tx.trim().is_empty() {
                        items.push(SalvageItem {
                            kind: "reasoning".into(),
                            text: tx,
                        });
                    }
                }
                "tool_use" => {
                    let mut id = str_of(b.get("id"));
                    if id.is_empty() {
                        id = format!("#{}", tool_keys.len());
                    }
                    let name = str_of(b.get("name"));
                    let mut target = String::new();
                    for f in ["file_path", "path", "pattern"] {
                        if let Some(s) = b
                            .get("input")
                            .and_then(|i| i.get(f))
                            .and_then(|v| v.as_str())
                        {
                            if !s.is_empty() {
                                target = one_line(s);
                                break;
                            }
                        }
                    }
                    if let std::collections::hash_map::Entry::Vacant(v) = tool_map.entry(id) {
                        tool_keys.push(v.key().clone());
                        v.insert(if target.is_empty() {
                            name
                        } else {
                            format!("{name}: {target}")
                        });
                    }
                }
                _ => {}
            }
        }
    }
    if !saw_text && !response.trim().is_empty() {
        items.push(SalvageItem {
            kind: "message".into(),
            text: response,
        });
    }
    Salvage {
        items,
        tools: tool_keys
            .iter()
            .filter_map(|k| tool_map.get(k).cloned())
            .collect(),
    }
}

/// The salvage as one text (the [`AttemptOutcome`]'s `partial`), `None` when nothing was produced.
pub fn claude_salvage_text(events_text: &str) -> Option<String> {
    let s = claude_salvage(events_text);
    let parts: Vec<String> = s
        .items
        .iter()
        .filter(|i| i.kind == "message")
        .map(|i| i.text.clone())
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

/// (wave 29) The claude branch of `Update-ToolFlight`: an assistant event's `tool_use` opens
/// `claude:<id>` (label `claude <name> <id>`), a user event's `tool_result` closes it.
pub fn claude_tool_flight(line: &str) -> ToolFlight {
    let t = line.trim();
    if !t.contains("tool_use") && !t.contains("tool_result") {
        return ToolFlight::None;
    }
    let obj: Value = match serde_json::from_str(t) {
        Ok(v) => v,
        Err(_) => return ToolFlight::None,
    };
    let ty = str_of(obj.get("type"));
    if ty != "assistant" && ty != "user" {
        return ToolFlight::None;
    }
    let content = obj
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();
    for b in content.iter().filter(|b| b.is_object()) {
        let bt = str_of(b.get("type"));
        if ty == "assistant" && bt == "tool_use" {
            let id = str_of(b.get("id"));
            if id.is_empty() {
                continue;
            }
            let name = b
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("tool")
                .to_string();
            return ToolFlight::Open {
                key: format!("claude:{id}"),
                label: format!("claude {name} {id}"),
            };
        }
        if ty == "user" && bt == "tool_result" {
            let tid = str_of(b.get("tool_use_id"));
            if !tid.is_empty() {
                return ToolFlight::Close {
                    key: format!("claude:{tid}"),
                    fallback_prefix: String::new(),
                };
            }
        }
    }
    ToolFlight::None
}

// --------------------------------------------------------------------------- the engine

/// The runtime `claude` engine: the resolved launcher, the working directory, the per-turn
/// streams, the auth and its child environment.
#[derive(Clone)]
pub struct ClaudeEngine {
    pub launcher: String,
    pub cwd: PathBuf,
    pub primary: TurnFiles,
    pub secondary: TurnFiles,
    pub stall_sec: i64,
    pub kick_path: Option<PathBuf>,
    pub on_running: Option<std::sync::Arc<dyn Fn(u32, String) + Send + Sync>>,
    /// The roster's auth (`subscription` | `api-key` | `endpoint`).
    pub auth: String,
    /// The roster entry's endpoint (auth `endpoint`).
    pub endpoint: Option<ClaudeEndpoint>,
}

/// One turn's outcome plus the parsed detail.
#[derive(Debug, Clone)]
pub struct ClaudeRun {
    pub outcome: AttemptOutcome,
    pub turn: ClaudeTurn,
    pub events: ClaudeEvents,
    /// The turn was killed (timeout, stall, kick).
    pub killed: bool,
}

impl ClaudeEngine {
    fn inner(&self) -> SubprocessEngine {
        SubprocessEngine::new(EngineKind::Claude)
    }

    fn files_for(&self, kind: TurnKind) -> &TurnFiles {
        match kind {
            TurnKind::Primary => &self.primary,
            _ => &self.secondary,
        }
    }

    /// The turn options the rules read, from the request.
    pub fn opts_of(&self, turn: &TurnRequest) -> ClaudeTurnOpts {
        let (mode, thread) = match &turn.request.mode {
            Mode::New => ("new", String::new()),
            Mode::Resume { thread, .. } => ("resume", thread.clone()),
            Mode::Fork { thread, .. } => ("fork", thread.clone()),
        };
        ClaudeTurnOpts {
            mode: mode.to_string(),
            thread,
            new_thread: turn.request.new_thread.clone().unwrap_or_default(),
            model: turn.request.model.clone(),
            auth: self.auth.clone(),
        }
    }

    /// Run one turn and return the full parsed detail. A child environment that must not start
    /// (auth endpoint without its token) refuses before anything starts (`LaunchFailed`,
    /// `child_exists: false`, the message naming the problem).
    pub fn run_detailed(&self, turn: &TurnRequest) -> Result<ClaudeRun, EngineError> {
        let args = match self.inner().plan(&turn.request)? {
            LaunchPlan::Subprocess(a) => a.args,
            LaunchPlan::Http(_) => return Err(EngineError::MissingField("subprocess argv")),
        };
        let env = child_env(&self.auth, self.endpoint.as_ref());
        if !env.problem.is_empty() {
            return Ok(ClaudeRun {
                outcome: AttemptOutcome::LaunchFailed {
                    child_exists: false,
                    message: format!(
                        "the claude engine's child environment is not usable: {}",
                        env.problem
                    ),
                },
                turn: ClaudeTurn::default(),
                events: ClaudeEvents::default(),
                killed: false,
            });
        }
        let files = self.files_for(turn.kind);
        let timeout = Duration::from_secs_f64(turn.request.timeout_sec.max(0.0));
        let is_primary = matches!(turn.kind, TurnKind::Primary);
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
            kick_path: self.kick_path.as_deref(),
            tool_flight: Some(&claude_tool_flight),
            on_running: self
                .on_running
                .as_ref()
                .map(|a| a.as_ref() as &dyn Fn(u32, String)),
            env: Some(&env.env),
            crt_quote: true,
        };
        let result = run_turn(&spawn);
        if !result.started {
            return Ok(ClaudeRun {
                outcome: AttemptOutcome::LaunchFailed {
                    child_exists: false,
                    message: result
                        .error
                        .unwrap_or_else(|| "the claude process did not start".into()),
                },
                turn: ClaudeTurn::default(),
                events: ClaudeEvents::default(),
                killed: false,
            });
        }
        let text = std::fs::read(&files.events)
            .map(|b| String::from_utf8_lossy(&b).to_string())
            .unwrap_or_default();
        use super::subprocess::TurnStop;
        let killed = !matches!(result.stop, TurnStop::Exited);
        let allow_partial = killed || result.exit_code != Some(0);
        let events = read_claude_events(&text, allow_partial);
        let opts = self.opts_of(turn);
        if killed {
            // (E12) a killed turn's init and model are judged too: the orchestrator reads
            // `turn.proof_problem` (no continuation; the problem is the outcome's reason)
            let t = claude_turn_outcome(
                &events,
                -1,
                &result.stderr,
                "failed: stopped",
                "",
                "",
                &opts,
            );
            let conversation = if t.thread_candidate.is_empty() {
                ConversationTrust::None
            } else {
                ConversationTrust::Candidate(ConversationId(t.thread_candidate.clone()))
            };
            let partial = claude_salvage_text(&text);
            let kill = result.kill.clone();
            let survivors = result.survivors.clone();
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
                    kill,
                    wall_seconds,
                },
                TurnStop::Kick => AttemptOutcome::Stopped {
                    kind: c3_core::engine::StopKind::Kick,
                    partial,
                    survivors,
                    conversation,
                    kill,
                    wall_seconds,
                },
                _ => AttemptOutcome::TimedOut {
                    partial,
                    survivors,
                    conversation,
                    kill,
                    wall_seconds,
                },
            };
            return Ok(ClaudeRun {
                outcome,
                turn: t,
                events,
                killed: true,
            });
        }
        let exit_code = result.exit_code.unwrap_or(-1);
        let t = claude_turn_outcome(&events, exit_code, &result.stderr, "", "", "", &opts);
        let outcome = to_outcome(&t, &events, files, result.wall_seconds, exit_code);
        Ok(ClaudeRun {
            outcome,
            turn: t,
            events,
            killed: false,
        })
    }
}

fn parse_structured(raw_text: &str) -> Option<StructuredReply> {
    let trimmed = raw_text.trim();
    if !trimmed.starts_with('{') {
        return None;
    }
    let raw: RawReply = serde_json::from_str(trimmed).ok()?;
    StructuredReply::try_from(raw).ok()
}

fn to_outcome(
    t: &ClaudeTurn,
    events: &ClaudeEvents,
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
    let (code, message) = failure_evidence(&t.texts);
    let class = if !t.class.is_empty() {
        t.class.clone()
    } else {
        c3_core::health::provider_failure_class(&format!("{code} {message}"))
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

impl Engine for ClaudeEngine {
    fn capabilities(&self) -> Capabilities {
        self.inner().capabilities()
    }

    fn plan(&self, request: &Request) -> Result<LaunchPlan, EngineError> {
        self.inner().plan(request)
    }

    fn precheck(&self, _turn: &TurnRequest) -> Result<(), EngineError> {
        if self.launcher.trim().is_empty() {
            return Err(EngineError::Precheck("claude CLI not found on PATH".into()));
        }
        Ok(())
    }

    fn run(&self, turn: &TurnRequest) -> Result<AttemptOutcome, EngineError> {
        self.run_detailed(turn).map(|r| r.outcome)
    }

    fn continue_turn(&self, turn: &TurnRequest) -> Result<AttemptOutcome, EngineError> {
        self.run_detailed(turn).map(|r| r.outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const U: &str = "11111111-2222-3333-4444-555555555555";

    fn init(extra: &str) -> String {
        let base = format!(
            r#"{{"type":"system","subtype":"init","cwd":"C:\\r","session_id":"{U}","tools":["Glob","Grep","Read","StructuredOutput"],"mcp_servers":[],"model":"claude-sonnet-5-5","permissionMode":"dontAsk","apiKeySource":"none"}}"#
        );
        if extra.is_empty() {
            base
        } else {
            base.replace("\"apiKeySource\":\"none\"", extra)
        }
    }

    fn result(extra: &str) -> String {
        format!(
            r#"{{"type":"result","subtype":"success","is_error":false,"num_turns":2,"result":"done","session_id":"{U}","total_cost_usd":0.012,"usage":{{"input_tokens":10,"cache_creation_input_tokens":100,"cache_read_input_tokens":1000,"output_tokens":50}},"modelUsage":{{"claude-sonnet-5-5":{{"outputTokens":50}}}},"permission_denials":[]{extra},"structured_output":{{"schema_version":"1","verdict":"ADVISE"}}}}"#
        )
    }

    fn opts(model: &str) -> ClaudeTurnOpts {
        ClaudeTurnOpts {
            mode: "new".into(),
            new_thread: U.into(),
            model: model.into(),
            auth: "subscription".into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_well_formed_stream_is_usable() {
        let text = format!("{}\n{}\n", init(""), result(""));
        let e = read_claude_events(&text, false);
        let o = claude_turn_outcome(&e, 0, "", "", "", "", &opts("sonnet"));
        assert!(o.ok, "{}", o.outcome);
        assert_eq!(o.thread, U);
        assert_eq!(o.model_resolved, "claude-sonnet-5-5");
        assert_eq!(o.reply, r#"{"schema_version":"1","verdict":"ADVISE"}"#);
        let u = e.usage.unwrap();
        assert_eq!(u.input_tokens, 1110);
        assert_eq!(u.cached_input_tokens, 1000);
        assert_eq!(u.output_tokens, 50);
    }

    #[test]
    fn the_rules_fail_with_their_class() {
        let cases: Vec<(String, ClaudeTurnOpts, &str, &str)> = vec![
            (
                format!(
                    "{}\n{}",
                    init("").replace("\"StructuredOutput\"", "\"StructuredOutput\",\"Bash\""),
                    result("")
                ),
                opts("sonnet"),
                "permission",
                "tools outside Read, Grep, Glob, StructuredOutput: Bash",
            ),
            (
                format!("{}\n{}", init("").replace("dontAsk", "default"), result("")),
                opts("sonnet"),
                "permission",
                "permission mode 'default', not dontAsk",
            ),
            (
                format!(
                    "{}\n{}",
                    init("\"apiKeySource\":\"ANTHROPIC_API_KEY\""),
                    result("")
                ),
                opts("sonnet"),
                "auth",
                "apiKeySource ANTHROPIC_API_KEY, not none",
            ),
            (result(""), opts("sonnet"), "permission", "no init event"),
            (
                format!("{}\n{}", init(""), result("")),
                opts("claude-opus-5-5"),
                "capability",
                "model drift: asked claude-opus-5-5, served claude-sonnet-5-5",
            ),
            (
                format!("{}\n{}\n{}", init(""), result(""), result("")),
                opts("sonnet"),
                "transport",
                "2 result events",
            ),
        ];
        for (text, o, class, frag) in cases {
            let e = read_claude_events(&text, false);
            let t = claude_turn_outcome(&e, 0, "", "", "", "", &o);
            assert!(!t.ok);
            assert_eq!(t.class, class, "{}", t.outcome);
            assert!(t.outcome.contains(frag), "{}", t.outcome);
        }
    }

    #[test]
    fn a_rejecting_rate_limit_marks_quota_and_survives_a_success() {
        let rl = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","resetsAt":1900000000,"rateLimitType":"five_hour"}}"#;
        let failed = format!(
            "{}\n{rl}\n{{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":true,\"result\":\"Claude AI usage limit reached\",\"session_id\":\"{U}\"}}\n",
            init("")
        );
        let e = read_claude_events(&failed, true);
        let t = claude_turn_outcome(&e, 1, "", "", "", "", &opts("sonnet"));
        assert_eq!(t.class, "quota");
        assert!(
            t.texts[0].ends_with("; resets at 2030-03-17T17:46:40Z"),
            "{:?}",
            t.texts
        );
        let ok = format!("{}\n{rl}\n{}\n", init(""), result(""));
        let e2 = read_claude_events(&ok, false);
        let t2 = claude_turn_outcome(&e2, 0, "", "", "", "", &opts("sonnet"));
        assert!(t2.ok);
        assert!(t2
            .quota_mark
            .starts_with("usage limit reached (claude rate_limit_event rejected, five_hour)"));
        assert!(t2.warnings[0].starts_with("a rate limit rejected a request during the turn: {"));
    }

    #[test]
    fn a_killed_turn_is_judged_by_its_init() {
        let text = format!(
            "{}\n{{\"type\":\"assis",
            init("").replace("\"StructuredOutput\"", "\"StructuredOutput\",\"Bash\"")
        );
        let e = read_claude_events(&text, true);
        let t = claude_turn_outcome(
            &e,
            -1,
            "",
            "failed: timeout after 5 s (process tree killed)",
            "",
            "",
            &opts("sonnet"),
        );
        assert!(!t.proof_problem.is_empty());
        assert!(t
            .outcome
            .ends_with("(the turn was also stopped: timeout after 5 s (process tree killed))"));
        assert_eq!(t.thread_candidate, U);
        let clean = format!("{}\n{{\"type\":\"assis", init(""));
        let e2 = read_claude_events(&clean, true);
        let t2 = claude_turn_outcome(
            &e2,
            -1,
            "",
            "failed: timeout after 5 s",
            "",
            "",
            &opts("sonnet"),
        );
        assert!(t2.proof_problem.is_empty());
        assert_eq!(t2.model_resolved, "claude-sonnet-5-5");
        assert_eq!(t2.outcome, "failed: timeout after 5 s");
    }

    #[test]
    fn salvage_and_tool_flight() {
        let asst = r#"{"type":"assistant","message":{"model":"m","content":[{"type":"thinking","thinking":"plan"},{"type":"text","text":"reading"},{"type":"tool_use","id":"toolu_1","name":"Read","input":{"file_path":"app.txt"}}]}}"#;
        let user = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"ok"}]}}"#;
        let s = claude_salvage(&format!("{asst}\n{user}\n"));
        assert_eq!(
            s.items.iter().map(|i| i.kind.as_str()).collect::<Vec<_>>(),
            vec!["reasoning", "message"]
        );
        assert_eq!(s.tools, vec!["Read: app.txt".to_string()]);
        assert_eq!(
            claude_tool_flight(asst),
            ToolFlight::Open {
                key: "claude:toolu_1".into(),
                label: "claude Read toolu_1".into()
            }
        );
        assert_eq!(
            claude_tool_flight(user),
            ToolFlight::Close {
                key: "claude:toolu_1".into(),
                fallback_prefix: String::new()
            }
        );
    }
}
