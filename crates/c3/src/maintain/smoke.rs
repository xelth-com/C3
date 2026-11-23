//! The smoke test a maintenance pass runs before trusting a CLI: a two-turn `c3 consult` through
//! the installed binary against a throwaway repo, and the choice of which roster seat to smoke.
//!
//! Turn 1 is a raw "reply with exactly OK"; turn 2 asks for the exact content of a `token.txt`
//! holding a random token, which forces a read (a tool call). The pass is: both turns exit 0 and the
//! reply of turn 2 contains the token (so the file was actually read). (C3 0.2.5) A codex smoke must
//! also prove the sub-agent switch: the rollouts of BOTH turns must show `multi_agent_version`
//! `disabled` and no multi-agent tool ([`subagent_switch_check`]) - a shown surface fails the
//! smoke, and (F11-7) a proof that could not be made (a turn without a thread id, a rollout that
//! cannot be found, read or identified, a seat that opts into sub-agents) leaves it
//! [`Smoke::Unproven`]; a post-update smoke that fails or is unproven rolls the update back.
//!
//! (F02-16) A roster seat's smoke is bound to that exact entry ([`Bind::Entry`]): both turns see a
//! one-entry roster holding it ([`bound_roster`]), so the label can never resolve to another entry
//! (an on-request or protected one with another endpoint, auth, plan or quota policy).
//!
//! The real two-turn path lives in [`run_consult_smoke`]; under `CODEX_CONSULT_TEST_MODE` a test can
//! script the outcome with `C3_MAINTAIN_TEST_SMOKE=provider::model=pass|fail,...` so the pass
//! orchestration (update → smoke → rollback) is exercised deterministically without a real model.

use chrono::{DateTime, Utc};

use super::env::Env;
use c3_core::roster::{Roster, RosterEntry};

/// The outcome of a smoke test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Smoke {
    Pass,
    Fail(String),
    /// (F11-7) The turns ran, but what the smoke must prove was not proven: a codex turn without a
    /// thread id, a rollout that cannot be found, read or identified, or that holds no terminal
    /// record for the turn, a seat that opts into sub-agents. NOT a pass: an automatic update holds
    /// on it exactly as on a failed smoke (rolled back where supported, never recorded as
    /// verified).
    Unproven(String),
}

impl Smoke {
    /// Why an automatic update must hold on this smoke (`None` for a pass): the failure, or the
    /// unproven reason. The first part of the pair is the word a report line uses (`failed` /
    /// `unproven`).
    pub fn hold(&self) -> Option<(&'static str, String)> {
        match self {
            Smoke::Pass => None,
            Smoke::Fail(why) => Some(("failed", why.clone())),
            Smoke::Unproven(why) => Some(("unproven", why.clone())),
        }
    }
}

/// A smoke target: the provider label and model id to consult, and what the smoke's consults are
/// bound to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub provider: String,
    pub model: String,
    pub bind: Bind,
}

/// (F02-16) What a smoke's two `c3 consult` turns resolve the reviewer against. A roster seat is
/// never re-resolved by its provider label (a label can name several entries - an on-request or a
/// protected one first - with other endpoints, auth, plans and quota policies).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bind {
    /// Exactly the roster entry at this position: the consults see a one-entry roster holding it
    /// (the operator's own JSON for that entry, its `model` overridden by [`Target::model`]), so its
    /// endpoint, auth, plan and quota policy are the ones the turns use ([`bound_roster`]).
    Entry(usize),
    /// A codex-config custom provider (not a roster entry): the consults run with no roster, so the
    /// label cannot resolve to a roster entry of the same name.
    CodexConfig,
    /// An explicit `C3_MAINTAIN_SMOKE_<ENGINE>=provider::model`: resolved by its label against the
    /// operator's roster, exactly like a consultation the operator named by hand.
    Label,
}

/// Pick the smoke target for engine `name`. `override_spec` is the `C3_MAINTAIN_SMOKE_<ENGINE>`
/// value (`provider::model`), which wins when present and well-formed. Otherwise, for codex, a
/// custom provider of the codex config first (a cheap plan carries the smoke); then an available
/// (non-inactive) roster seat of that engine: ordinary seats before `panel: weighty`, the most
/// budget headroom first, never a plan whose governing window is at 90 % or more. `None` when there
/// is no eligible seat.
pub fn pick_target(
    override_spec: &str,
    roster: &Roster,
    name: &str,
    now: DateTime<Utc>,
) -> Option<Target> {
    let spec = override_spec.trim();
    if !spec.is_empty() {
        if let Some((p, m)) = spec.split_once("::") {
            if !p.trim().is_empty() && !m.trim().is_empty() {
                return Some(Target {
                    provider: p.trim().to_string(),
                    model: m.trim().to_string(),
                    bind: Bind::Label,
                });
            }
        }
    }
    // Candidate seats as (target, rank, score): rank -1 a codex-config custom provider (codex only),
    // 0 ordinary roster seat, 1 weighty roster seat. A stable sort on (rank, -score) keeps roster /
    // config order within a tie.
    let mut cands: Vec<(Target, i32, f64)> = Vec::new();
    for e in roster.active_entries() {
        if engine_of(e) != name || e.model.is_empty() {
            continue;
        }
        // (0.2.4, reviewer-protection) the smoke never spends a protected or an on-request seat: a
        // protected reviewer's reserve is kept for acceptance work, and an on-request entry runs
        // only when named by the operator.
        if e.on_request || e.reserve_percent > 0 {
            continue;
        }
        let (headroom, saturated) = plan_headroom(roster, e, now);
        if saturated {
            continue; // the governing window is at 90 % or more
        }
        let rank = if e.panel.eq_ignore_ascii_case("weighty") {
            1
        } else {
            0
        };
        cands.push((
            Target {
                provider: e.provider.clone(),
                model: e.model.clone(),
                bind: Bind::Entry(e.position),
            },
            rank,
            headroom,
        ));
    }
    // codex: the custom providers of the codex config (ZAI / kimi / alibaba / mimo), each with a
    // declared model, preferring one whose id contains `flash`. Only for the codex engine, because
    // those endpoints are reachable through codex's own `model_provider` routing. They rank BEFORE
    // the roster seats (rank -1): the smoke checks the codex CLI, not the model, so it must not
    // spend the ChatGPT account's allowance when a cheap plan can carry it - and the account's
    // windows are not always known here (the rollouts of the last two days only: a weekly window
    // at 97 % observed three days ago reads as "no data", not as saturated).
    if name == "codex" {
        for t in codex_custom_targets() {
            let flash = t.model.contains("flash");
            cands.push((t, -1, if flash { 1.0 } else { 0.0 }));
        }
    }
    cands.sort_by(|a, b| {
        a.1.cmp(&b.1)
            .then(b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal))
    });
    cands.into_iter().next().map(|(t, _, _)| t)
}

/// The codex-config custom providers (`[model_providers.*]`) as smoke targets: for each provider
/// with declared models, one target whose model prefers an id containing `flash`, else the first
/// declared model. Network-free (reads `config.toml` and the built-in caps catalog).
pub fn codex_custom_targets() -> Vec<Target> {
    let path = crate::providers::get_codex_config_path();
    let config = crate::providers::read_codex_config(&path);
    let mut out = Vec::new();
    for provider in c3_core::config::provider_names(&config) {
        let pt = c3_core::config::provider_table(&config, &provider);
        if !pt.ok {
            continue;
        }
        let Some(key) = pt.table_key else {
            continue;
        };
        let table_name = format!("[model_providers.{provider}]");
        let ep = c3_core::config::provider_endpoint(&config, &key, &table_name, "config.toml");
        if !ep.error.is_empty() || ep.host.is_empty() {
            continue;
        }
        if let Some(cap) = c3_core::effort::caps(&ep.host) {
            if let c3_core::effort::Models::List(list) = cap.models {
                let model = list
                    .iter()
                    .find(|m| m.contains("flash"))
                    .or_else(|| list.first())
                    .map(|s| s.to_string());
                if let Some(model) = model {
                    out.push(Target {
                        provider: provider.clone(),
                        model,
                        bind: Bind::CodexConfig,
                    });
                }
            }
        }
    }
    out
}

/// Whether the entry's plan is saturated (its governing budget window at 90 %+ used), the same
/// guard [`pick_target`] applies. A model-swap smoke uses this so it never spends quota on an
/// exhausted plan (the swap is then held, the roster untouched).
pub fn seat_saturated(roster: &Roster, e: &RosterEntry, now: DateTime<Utc>) -> bool {
    plan_headroom(roster, e, now).1
}

/// The engine of a roster entry (empty engine means codex).
fn engine_of(e: &RosterEntry) -> &str {
    if e.engine.is_empty() {
        "codex"
    } else {
        e.engine.as_str()
    }
}

/// The budget headroom of the entry's plan and whether its governing window is at 90 %+ used.
/// Best-effort and network-free: no budget data → neutral headroom, not saturated.
fn plan_headroom(roster: &Roster, e: &RosterEntry, now: DateTime<Utc>) -> (f64, bool) {
    let key = crate::budget::budget_key(e);
    if key.is_empty() {
        return (1.0, false);
    }
    let group: Vec<RosterEntry> = roster
        .entries
        .iter()
        .filter(|x| crate::budget::budget_key(x) == key)
        .cloned()
        .collect();
    let plan_arg = if key == "openai-account" {
        ""
    } else {
        e.plan.as_str()
    };
    let windows = crate::budget::plan_windows(plan_arg, &group, now, false);
    if windows.is_empty() {
        return (1.0, false);
    }
    let (mult, governing) = crate::budget::budget_multiplier(&windows);
    let saturated = governing
        .as_ref()
        .and_then(|g| crate::budget::governing_window(&windows, g))
        .map(|w| w.used_ratio >= 0.9)
        .unwrap_or(false);
    (mult, saturated)
}

/// Run the smoke for a target. `self_exe` is this binary; `work` a scratch directory to build the
/// throwaway repo in; `token` the random token written into `token.txt`; `roster_text` the roster
/// a [`Bind::Entry`] target is taken from (for a model swap: the very snapshot the gate judged).
/// Under test mode a `C3_MAINTAIN_TEST_SMOKE` script decides the outcome without a real consult.
pub fn run_smoke(
    env: &dyn Env,
    self_exe: &str,
    target: &Target,
    engine: &str,
    work: &std::path::Path,
    token: &str,
    roster_text: Option<&str>,
) -> Smoke {
    if c3_core::test_hooks::mode_on() {
        if let Some(script) = std::env::var("C3_MAINTAIN_TEST_SMOKE")
            .ok()
            .filter(|s| !s.is_empty())
        {
            return scripted(&script, target);
        }
    }
    run_consult_smoke(env, self_exe, target, engine, work, token, roster_text)
}

/// (F02-16) The one-entry roster a [`Bind::Entry`] smoke's consults see: the operator's own JSON
/// object of `reviewers[position]` (1-based) from `roster_text`, its `model` set to `model`, and
/// nothing else of the roster - no other entry the label could resolve to, no `require`, no
/// `ext.c3.reviewers`. Refused when the element is gone or no longer names `provider`, when it is
/// protected (`ext.reserve_percent`) or on-request (`ext.on_request`) - such an entry is never
/// smoked, not even indirectly - or when the roster's `ext.c3.inactive` names its plan or label.
pub fn bound_roster(
    roster_text: &str,
    position: usize,
    provider: &str,
    model: &str,
) -> Result<String, String> {
    let v: serde_json::Value =
        serde_json::from_str(roster_text).map_err(|_| "the roster does not parse".to_string())?;
    let entry = position
        .checked_sub(1)
        .and_then(|i| v.get("reviewers")?.as_array()?.get(i))
        .and_then(|e| e.as_object())
        .ok_or_else(|| format!("the roster has no reviewers entry {position}"))?;
    let str_of = |k: &str| entry.get(k).and_then(|x| x.as_str()).unwrap_or("");
    if str_of("provider") != provider {
        return Err(format!(
            "reviewers entry {position} is no longer provider {provider}"
        ));
    }
    let ext = entry.get("ext");
    if ext
        .and_then(|x| x.get("on_request"))
        .and_then(|b| b.as_bool())
        == Some(true)
    {
        return Err(format!(
            "reviewers entry {position} runs only on request; it is never smoked"
        ));
    }
    if ext
        .and_then(|x| x.get("reserve_percent"))
        .and_then(|n| n.as_i64())
        .unwrap_or(0)
        > 0
    {
        return Err(format!(
            "reviewers entry {position} is protected; it is never smoked"
        ));
    }
    let inactive = v
        .pointer("/ext/c3/inactive")
        .and_then(|a| a.as_array())
        .map(|a| {
            a.iter().filter_map(|x| x.as_str()).any(|i| {
                i.trim() == provider || (!str_of("plan").is_empty() && i.trim() == str_of("plan"))
            })
        })
        .unwrap_or(false);
    if inactive {
        return Err(format!("reviewers entry {position} is inactive"));
    }
    let mut one = entry.clone();
    one.insert(
        "model".to_string(),
        serde_json::Value::String(model.to_string()),
    );
    Ok(serde_json::json!({
        "roster_version": 1,
        "reviewers": [serde_json::Value::Object(one)],
    })
    .to_string())
}

/// A file removed when dropped (the bound roster of one smoke).
struct RemoveOnDrop(std::path::PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Per-seat call counter so a `pass-then-fail` verdict can distinguish the installed-version smoke
/// (call 0) from the post-update smoke (call 1) in tests.
fn call_counts() -> &'static std::sync::Mutex<std::collections::HashMap<String, usize>> {
    static C: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, usize>>> =
        std::sync::OnceLock::new();
    C.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// The scripted outcome under `C3_MAINTAIN_TEST_SMOKE=provider::model=<verdict>,...` where the
/// verdict is `pass`, `fail`, `unproven`, `pass-then-fail` (the first smoke of that seat passes,
/// the rest fail), `pass-then-unproven` (the first passes, the rest are unproven) or
/// `fail-then-pass` (the first fails, the rest pass). A seat not listed fails, so a test opts a
/// seat into passing explicitly.
fn scripted(script: &str, target: &Target) -> Smoke {
    let key = format!("{}::{}", target.provider, target.model);
    let first_call = |key: &str| {
        let mut g = call_counts().lock().unwrap_or_else(|e| e.into_inner());
        let n = g.entry(key.to_string()).or_insert(0);
        let first = *n == 0;
        *n += 1;
        first
    };
    for item in script.split(',') {
        if let Some((seat, verdict)) = item.split_once('=') {
            if seat.trim() != key {
                continue;
            }
            let verdict = verdict.trim().to_ascii_lowercase();
            return match verdict.as_str() {
                "pass" => Smoke::Pass,
                "unproven" => Smoke::Unproven("scripted unproven smoke".to_string()),
                "pass-then-unproven" => {
                    if first_call(&key) {
                        Smoke::Pass
                    } else {
                        Smoke::Unproven("scripted post-update unproven smoke".to_string())
                    }
                }
                "pass-then-fail" => {
                    if first_call(&key) {
                        Smoke::Pass
                    } else {
                        Smoke::Fail("scripted post-update failure".to_string())
                    }
                }
                "fail-then-pass" => {
                    if first_call(&key) {
                        Smoke::Fail("scripted first failure".to_string())
                    } else {
                        Smoke::Pass
                    }
                }
                _ => Smoke::Fail("scripted failure".to_string()),
            };
        }
    }
    Smoke::Fail("no scripted verdict".to_string())
}

/// The real two-turn smoke through `c3 consult`. Both turns run under the target's binding
/// ([`Bind`]): a roster seat through a one-entry roster written beside the scratch repo (outside
/// it) and named by `CODEX_CONSULT_ROSTER`, a codex-config provider with no roster.
fn run_consult_smoke(
    env: &dyn Env,
    self_exe: &str,
    target: &Target,
    engine: &str,
    work: &std::path::Path,
    token: &str,
    roster_text: Option<&str>,
) -> Smoke {
    // 0. the binding, before anything is created
    let mut _bound_file: Option<RemoveOnDrop> = None;
    let roster_env: Option<String> = match target.bind {
        Bind::Label => None,
        Bind::CodexConfig => Some("none".to_string()),
        Bind::Entry(position) => {
            let Some(text) = roster_text else {
                return Smoke::Fail(format!(
                    "the roster entry {position} could not be read for the smoke"
                ));
            };
            let one = match bound_roster(text, position, &target.provider, &target.model) {
                Ok(one) => one,
                Err(why) => return Smoke::Fail(format!("not smoked: {why}")),
            };
            let name = format!(
                "{}.roster.json",
                work.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "c3-maintain-smoke".to_string())
            );
            let path = work.with_file_name(name);
            if std::fs::write(&path, one).is_err() {
                return Smoke::Fail("could not write the smoke's roster".to_string());
            }
            let s = path.to_string_lossy().into_owned();
            _bound_file = Some(RemoveOnDrop(path));
            Some(s)
        }
    };
    let mut turn_env: Vec<(&str, &str)> = roster_env
        .as_deref()
        .map(|r| vec![("CODEX_CONSULT_ROSTER", r)])
        .unwrap_or_default();
    // (F11-8) the Codex home resolved to an absolute path ONCE: the consult children run from the
    // scratch repo, so a relative CODEX_HOME would name another directory there than the one this
    // process reads the rollouts from. Both use this one.
    let codex_home = absolute_home(&crate::providers::get_codex_home());
    if engine == "codex" && !codex_home.is_empty() {
        turn_env.push(("CODEX_HOME", codex_home.as_str()));
    }

    // 1. a throwaway git repo with token.txt
    if std::fs::create_dir_all(work).is_err() {
        return Smoke::Fail("could not create the smoke workdir".to_string());
    }
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "maintain@c3.local"],
        vec!["config", "user.name", "c3-maintain"],
    ] {
        let r = env.run("git", &args, &[], Some(work));
        if !r.ok() {
            return Smoke::Fail(format!("git setup failed ({})", r.fail_reason()));
        }
    }
    if std::fs::write(work.join("token.txt"), token).is_err() {
        return Smoke::Fail("could not write token.txt".to_string());
    }
    let _ = env.run("git", &["add", "-A"], &[], Some(work));
    let _ = env.run("git", &["commit", "-q", "-m", "smoke"], &[], Some(work));

    // The common consult flags that name the reviewer.
    let mut reviewer: Vec<String> = vec![
        "--provider".into(),
        target.provider.clone(),
        "--model".into(),
        target.model.clone(),
    ];
    if engine != "codex" {
        reviewer.push("--engine".into());
        reviewer.push(engine.to_string());
    }

    // Turn 1: raw "reply with exactly OK" (a new thread).
    let turn1 = consult_turn(
        env,
        self_exe,
        work,
        &reviewer,
        &turn_env,
        "new",
        "reply with exactly OK",
    );
    if !turn1.ok() {
        return Smoke::Fail(format!("turn 1 failed ({})", turn_reason(&turn1)));
    }

    // Turn 2: must read the file (a tool call) and echo its content; auto-mode forks the thread.
    let turn2 = consult_turn(
        env,
        self_exe,
        work,
        &reviewer,
        &turn_env,
        "",
        "Reply with the exact content of the file token.txt in this repository.",
    );
    if !turn2.ok() {
        return Smoke::Fail(format!("turn 2 failed ({})", turn_reason(&turn2)));
    }

    // Pass when the reply of turn 2 contains the token (the file was read) — the proxy for a
    // recorded tool call. The reply is in the handoff; fall back to the console output.
    if !(reply_contains(work, token) || turn2.combined().contains(token)) {
        return Smoke::Fail("the reply did not contain the token (no file read)".to_string());
    }
    // (C3 0.2.5) a codex smoke also proves the sub-agent switch from its own rollouts - for BOTH
    // turns (F11-7).
    if engine == "codex" {
        return subagent_switch_check(work, &codex_home, SMOKE_TURNS);
    }
    Smoke::Pass
}

/// (F11-7) How many consults a smoke runs (turn 1 and turn 2); each must prove the switch.
pub(crate) const SMOKE_TURNS: usize = 2;

/// (F11-8) `home` as an absolute path (relative to this process's working directory when it is
/// relative); `""` stays `""`.
fn absolute_home(home: &str) -> String {
    if home.is_empty() {
        return String::new();
    }
    std::path::absolute(home)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| home.to_string())
}

/// (F11-8) How often, and how far apart, a rollout that cannot be found or read is looked for
/// again (a transient sharing violation, a file the exited child's writer has not closed yet).
const ROLLOUT_TRIES: usize = 3;
const ROLLOUT_RETRY_MS: u64 = 300;

/// (F11-8) The rollout of `thread` and its surface, retried a bounded number of times while it
/// cannot be found or read; `Err` is the unproven reason.
fn rollout_surface_of(
    thread: &str,
    codex_home: &str,
) -> Result<c3_core::quota::SubagentSurface, String> {
    let mut why = String::new();
    for attempt in 0..ROLLOUT_TRIES {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(ROLLOUT_RETRY_MS));
        }
        let Some(path) = c3_core::quota::codex_rollout_path(thread, codex_home) else {
            why = format!(
                "no rollout of thread {thread} under {} (the sub-agent guard reads the same rollouts)",
                std::path::Path::new(codex_home).join("sessions").display()
            );
            continue;
        };
        let s = c3_core::quota::rollout_subagent_surface(&path);
        if !s.found || !s.read_error.is_empty() {
            why = format!(
                "the rollout of thread {thread} could not be read ({})",
                if s.read_error.is_empty() {
                    "it is gone"
                } else {
                    s.read_error.as_str()
                }
            );
            continue;
        }
        return Ok(s);
    }
    Err(why)
}

/// (C3 0.2.5) The codex smoke proves the sub-agent switch: a Codex CLI update can break the bridge's
/// off switches (`-c features.multi_agent=false -c agents.enabled=false`) silently - codex-cli
/// 0.169.0 did, and two runs spent 14 % of a ChatGPT week in five minutes. Every codex thread the
/// smoke's consults recorded (`.collab/maintain-smoke/sessions.json`) with the off switches passed
/// (`subagents_allowed` not `true`) is read from its rollout under `codex_home`
/// ([`c3_core::quota::codex_rollout_path`], [`c3_core::quota::rollout_subagent_surface`]); the smoke
/// FAILS when a thread shows a `multi_agent_version` other than `disabled`, offers or calls a
/// multi-agent tool, or cannot be read.
///
/// (F11-7) Nothing short of that proof passes: the smoke is [`Smoke::Unproven`] - which an automatic
/// update holds on exactly as on a failure - when fewer consults than `expected` were recorded, when
/// a consult recorded no thread id, when a consult ran with sub-agents allowed by the seat's
/// `codex_config` (the switch was not exercised), and (F11-8) when a thread's rollout cannot be
/// found or read after a few tries, names another session than the thread it was looked up for,
/// holds an unreadable record, has no terminal record for its turn (`task_complete` after its last
/// `task_started`; a thread two consults share needs one per consult), or names neither a
/// `multi_agent_version` nor a tool list. The budget watch's sub-agent guard - the defence behind
/// the switch - reads the same rollouts, so a CLI whose rollouts C3 cannot read runs with a blind
/// guard: holding such an update costs one held update, letting it through risks the incident
/// again. A shown surface (a version other than `disabled`, a multi-agent tool offered or called)
/// is a [`Smoke::Fail`].
pub(crate) fn subagent_switch_check(
    work: &std::path::Path,
    codex_home: &str,
    expected: usize,
) -> Smoke {
    let unproven =
        |why: String| Smoke::Unproven(format!("the sub-agent switch is unproven: {why}"));
    let sessions = work
        .join(".collab")
        .join("maintain-smoke")
        .join("sessions.json");
    let consults: Vec<serde_json::Value> = std::fs::read(&sessions)
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| {
            v.pointer("/codex/consults")
                .and_then(|c| c.as_array())
                .cloned()
        })
        .unwrap_or_default();
    if consults.is_empty() {
        return unproven("the smoke's consults recorded no codex thread".to_string());
    }
    if consults.len() < expected {
        return unproven(format!(
            "the smoke recorded {} consult(s), {expected} expected",
            consults.len()
        ));
    }
    // (thread, how many consults recorded it), first seen first
    let mut threads: Vec<(String, usize)> = Vec::new();
    for (i, c) in consults.iter().enumerate() {
        let n = c
            .get("n")
            .and_then(|n| n.as_i64())
            .map(|n| n.to_string())
            .unwrap_or_else(|| (i + 1).to_string());
        let Some(t) = c
            .get("thread")
            .and_then(|t| t.as_str())
            .filter(|t| !t.is_empty())
        else {
            return unproven(format!("consult n={n} recorded no thread id"));
        };
        if c.get("subagents_allowed").and_then(|a| a.as_bool()) == Some(true) {
            return unproven(format!(
                "consult n={n} ran with sub-agents allowed by the seat's codex_config, so the switch was not exercised (smoke a seat that keeps them off)"
            ));
        }
        match threads.iter_mut().find(|(x, _)| x.eq_ignore_ascii_case(t)) {
            Some((_, k)) => *k += 1,
            None => threads.push((t.to_string(), 1)),
        }
    }
    for (t, k) in &threads {
        let s = match rollout_surface_of(t, codex_home) {
            Ok(s) => s,
            Err(why) => return unproven(why),
        };
        if !s.session_id.eq_ignore_ascii_case(t) {
            return unproven(format!(
                "the rollout found for thread {t} names session `{}`, not that thread",
                s.session_id
            ));
        }
        if let Some(why) = s.breach() {
            return Smoke::Fail(format!(
                "the sub-agent switch did not hold: thread {t} - {why} - although the bridge passed -c {} -c {}",
                c3_core::quota::MULTI_AGENT_OFF,
                c3_core::quota::AGENTS_OFF
            ));
        }
        if s.malformed > 0 {
            return unproven(format!(
                "the rollout of thread {t} holds {} unreadable record(s)",
                s.malformed
            ));
        }
        if s.terminals == 0 || s.completed < *k {
            return unproven(format!(
                "the rollout of thread {t} holds no terminal record for its turn ({} task_complete for {k} consult(s))",
                s.completed
            ));
        }
        if !s.has_evidence() {
            return unproven(format!(
                "the rollout of thread {t} names no multi_agent_version and no tool list"
            ));
        }
    }
    Smoke::Pass
}

fn consult_turn(
    env: &dyn Env,
    self_exe: &str,
    work: &std::path::Path,
    reviewer: &[String],
    turn_env: &[(&str, &str)],
    mode: &str,
    prompt: &str,
) -> super::env::RunResult {
    let mut args: Vec<&str> = vec![
        "consult",
        "--task",
        "maintain-smoke",
        "--raw",
        "--prompt",
        prompt,
    ];
    if !mode.is_empty() {
        args.push("--mode");
        args.push(mode);
    }
    for r in reviewer {
        args.push(r.as_str());
    }
    env.run(self_exe, &args, turn_env, Some(work))
}

/// Why a consult turn failed: its `codex-consult: ...` outcome line when it printed one (the first
/// stderr line is often the informational `Roster: ...` line), else the first output line.
fn turn_reason(r: &super::env::RunResult) -> String {
    r.combined()
        .lines()
        .map(str::trim)
        .rfind(|l| l.starts_with("codex-consult:"))
        .map(|l| l.to_string())
        .unwrap_or_else(|| r.fail_reason())
}

/// Whether any handoff reply file under the smoke task contains the token.
fn reply_contains(work: &std::path::Path, token: &str) -> bool {
    let handoffs = work.join(".collab").join("maintain-smoke").join("handoffs");
    let rd = match std::fs::read_dir(&handoffs) {
        Ok(rd) => rd,
        Err(_) => return false,
    };
    for e in rd.flatten() {
        if let Ok(text) = std::fs::read_to_string(e.path()) {
            if text.contains(token) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use c3_core::roster::RosterEntry;

    fn entry(
        pos: usize,
        provider: &str,
        engine: &str,
        model: &str,
        panel: &str,
        plan: &str,
    ) -> RosterEntry {
        RosterEntry {
            position: pos,
            provider: provider.into(),
            engine: engine.into(),
            model: model.into(),
            panel: panel.into(),
            plan: plan.into(),
            ..Default::default()
        }
    }

    fn roster(entries: Vec<RosterEntry>) -> Roster {
        Roster {
            exists: true,
            entries,
            ..Default::default()
        }
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-15T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn override_spec_wins() {
        let r = roster(vec![]);
        let t = pick_target("openai::gpt-6-astra", &r, "codex", now()).unwrap();
        assert_eq!(t.provider, "openai");
        assert_eq!(t.model, "gpt-6-astra");
    }

    #[test]
    fn ordinary_seat_before_weighty() {
        let r = roster(vec![
            entry(0, "grok", "grok", "grok-weighty", "weighty", "supergrok"),
            entry(1, "grok", "grok", "grok-ordinary", "", "supergrok"),
        ]);
        let t = pick_target("", &r, "grok", now()).unwrap();
        assert_eq!(t.model, "grok-ordinary", "ordinary beats weighty");
    }

    #[test]
    fn only_the_named_engine_and_no_empty_model() {
        let r = roster(vec![
            entry(0, "meta", "muse", "muse-x", "", "meta"),
            entry(1, "grok", "grok", "grok-4.7", "", "supergrok"),
        ]);
        let t = pick_target("", &r, "grok", now()).unwrap();
        assert_eq!(t.model, "grok-4.7");
        assert!(pick_target("", &r, "kimi", now()).is_none());
    }

    /// A smoke work dir whose `sessions.json` records `consults` (`(thread, subagents_allowed)`),
    /// and a codex home whose rollouts are `rollouts` (`(thread, lines)`). Returns (root, work, home).
    fn switch_fixture(
        tag: &str,
        consults: &[(&str, Option<bool>)],
        rollouts: &[(&str, Vec<String>)],
    ) -> (std::path::PathBuf, std::path::PathBuf, String) {
        let now = Utc::now();
        let root = std::env::temp_dir().join(format!(
            "c3-smoke-switch-{tag}-{}-{}",
            std::process::id(),
            now.timestamp_nanos_opt().unwrap_or(0)
        ));
        let task = root.join("work").join(".collab").join("maintain-smoke");
        std::fs::create_dir_all(&task).unwrap();
        let entries: Vec<serde_json::Value> = consults
            .iter()
            .enumerate()
            .map(|(i, (t, allowed))| {
                serde_json::json!({"n": i + 1, "thread": t, "subagents_allowed": allowed})
            })
            .collect();
        std::fs::write(
            task.join("sessions.json"),
            serde_json::json!({"task_id": "maintain-smoke", "codex": {"tool": "", "consults": entries}})
                .to_string(),
        )
        .unwrap();
        let local = now.with_timezone(&chrono::Local);
        let day = root
            .join("home")
            .join("sessions")
            .join(local.format("%Y").to_string())
            .join(local.format("%m").to_string())
            .join(local.format("%d").to_string());
        std::fs::create_dir_all(&day).unwrap();
        for (t, lines) in rollouts {
            std::fs::write(
                day.join(format!(
                    "rollout-{}-{t}.jsonl",
                    local.format("%Y-%m-%dT%H-%M-%S")
                )),
                format!("{}\n", lines.join("\n")),
            )
            .unwrap();
        }
        let home = root.join("home").to_string_lossy().to_string();
        (root.clone(), root.join("work"), home)
    }

    const T1: &str = "01a20000-0000-7000-8000-000000000001";
    const T2: &str = "01a20000-0000-7000-8000-000000000002";

    /// A minimal excerpt of a codex-cli 0.169.0 smoke rollout (the bridge's off switches passed):
    /// `session_meta`, `task_started`, the tool list, `turn_context`. `mav` is the thread's
    /// `multi_agent_version`; `collab` adds the multi-agent v2 `collaboration` namespace.
    fn smoke_rollout(thread: &str, mav: &str, collab: bool) -> Vec<String> {
        let at = "2026-11-22T12:09:52.300Z";
        let collab = if collab {
            r#",{"type":"namespace","name":"collaboration","tools":[{"type":"function","name":"spawn_agent"},{"type":"function","name":"wait_agent"}]}"#
        } else {
            ""
        };
        vec![
            format!(
                r#"{{"timestamp":"{at}","type":"session_meta","payload":{{"id":"{thread}","originator":"codex_exec","cli_version":"0.169.0","source":"exec","model_provider":"ZAI","history_mode":"paginated","multi_agent_version":"{mav}"}}}}"#
            ),
            format!(
                r#"{{"timestamp":"{at}","type":"event_msg","payload":{{"type":"task_started"}}}}"#
            ),
            format!(
                r#"{{"timestamp":"{at}","type":"response_item","payload":{{"type":"additional_tools","role":"developer","tools":[{{"type":"namespace","name":"functions","tools":[{{"type":"custom","name":"exec"}},{{"type":"function","name":"wait"}},{{"type":"function","name":"send_message_to_user_async"}}]}},{{"type":"namespace","name":"clock","tools":[{{"type":"function","name":"sleep"}}]}}{collab}]}}}}"#
            ),
            format!(
                r#"{{"timestamp":"{at}","type":"turn_context","payload":{{"turn_id":"t","model":"glm-5.3-flash","multi_agent_version":"{mav}"}}}}"#
            ),
            format!(
                r#"{{"timestamp":"{at}","type":"event_msg","payload":{{"type":"task_complete","last_agent_message":"OK"}}}}"#
            ),
        ]
    }

    fn fail_reason(s: Smoke) -> String {
        match s {
            Smoke::Fail(why) => why,
            other => panic!("expected a failure, got {other:?}"),
        }
    }

    fn unproven_reason(s: Smoke) -> String {
        match s {
            Smoke::Unproven(why) => why,
            other => panic!("expected an unproven smoke, got {other:?}"),
        }
    }

    /// (C3 0.2.5) Both threads of the smoke (turn 1 and its fork) show `multi_agent_version`
    /// `disabled` and no multi-agent tool: the switch held, the smoke passes.
    #[test]
    fn the_codex_smoke_passes_when_the_switch_held() {
        let (root, work, home) = switch_fixture(
            "ok",
            &[(T1, Some(false)), (T2, Some(false))],
            &[
                (T1, smoke_rollout(T1, "disabled", false)),
                (T2, smoke_rollout(T2, "disabled", false)),
            ],
        );
        let got = subagent_switch_check(&work, &home, SMOKE_TURNS);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(got, Smoke::Pass);
    }

    /// (C3 0.2.5) A thread on multi-agent v2, or one offered `spawn_agent` despite a `disabled`
    /// version, fails the smoke with the reason (so a post-update smoke rolls the update back).
    #[test]
    fn the_codex_smoke_fails_when_the_switch_broke() {
        let (root, work, home) = switch_fixture(
            "v2",
            &[(T1, Some(false)), (T2, Some(false))],
            &[
                (T1, smoke_rollout(T1, "disabled", false)),
                (T2, smoke_rollout(T2, "v2", false)),
            ],
        );
        let why = fail_reason(subagent_switch_check(&work, &home, SMOKE_TURNS));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(
            why,
            format!("the sub-agent switch did not hold: thread {T2} - multi_agent_version v2 - although the bridge passed -c features.multi_agent=false -c agents.enabled=false")
        );

        let (root, work, home) = switch_fixture(
            "tools",
            &[(T1, None)],
            &[(T1, smoke_rollout(T1, "disabled", true))],
        );
        let why = fail_reason(subagent_switch_check(&work, &home, 1));
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            why.contains(&format!("thread {T1} - offers spawn_agent, wait_agent")),
            "{why}"
        );

        // (F10-5) the multi-agent tool under an MCP server's namespace counts by its name
        let mut mcp = smoke_rollout(T1, "disabled", false);
        mcp[2] = mcp[2].replace(
            r#"{"type":"namespace","name":"clock""#,
            r#"{"type":"namespace","name":"mcp__srv","tools":[{"type":"function","name":"spawn_agent"}]},{"type":"namespace","name":"clock""#,
        );
        let (root, work, home) = switch_fixture("mcp", &[(T1, None)], &[(T1, mcp)]);
        let why = fail_reason(subagent_switch_check(&work, &home, 1));
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            why.contains(&format!("thread {T1} - offers spawn_agent")),
            "{why}"
        );
    }

    /// (F11-7, F11-8) A smoke whose proof is incomplete is UNPROVEN (never a pass): a recorded thread
    /// without a rollout, a rollout without a session id of that thread, one consult without a
    /// thread id beside a clean one, fewer consults than turns, a seat that opted into sub-agents, a
    /// rollout without a terminal record for its turn, one with an unreadable record, and a smoke
    /// that recorded no thread.
    #[test]
    fn the_codex_smoke_is_unproven_without_full_evidence() {
        let (root, work, home) = switch_fixture("missing", &[(T1, Some(false))], &[]);
        let why = unproven_reason(subagent_switch_check(&work, &home, 1));
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            why.starts_with(&format!(
                "the sub-agent switch is unproven: no rollout of thread {T1} under "
            )),
            "{why}"
        );

        // a rollout without a session_meta: its identity is unknown
        let bare = vec![
            r#"{"timestamp":"2026-11-22T12:09:52.300Z","type":"event_msg","payload":{"type":"task_started"}}"#.to_string(),
        ];
        let (root, work, home) = switch_fixture("bare", &[(T1, Some(false))], &[(T1, bare)]);
        let why = unproven_reason(subagent_switch_check(&work, &home, 1));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(
            why,
            format!("the sub-agent switch is unproven: the rollout found for thread {T1} names session ``, not that thread")
        );

        // a rollout whose session_meta names ANOTHER thread
        let (root, work, home) = switch_fixture(
            "otherid",
            &[(T1, Some(false))],
            &[(T1, smoke_rollout(T2, "disabled", false))],
        );
        let why = unproven_reason(subagent_switch_check(&work, &home, 1));
        let _ = std::fs::remove_dir_all(&root);
        assert!(why.contains(&format!("names session `{T2}`")), "{why}");

        // the reviewer's Verify: one clean thread and one successful consult with a blank thread
        let (root, work, home) = switch_fixture(
            "blank",
            &[(T1, Some(false)), ("", Some(false))],
            &[(T1, smoke_rollout(T1, "disabled", false))],
        );
        let why = unproven_reason(subagent_switch_check(&work, &home, SMOKE_TURNS));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(
            why,
            "the sub-agent switch is unproven: consult n=2 recorded no thread id"
        );

        // only one of the two turns recorded
        let (root, work, home) = switch_fixture(
            "one",
            &[(T1, Some(false))],
            &[(T1, smoke_rollout(T1, "disabled", false))],
        );
        let why = unproven_reason(subagent_switch_check(&work, &home, SMOKE_TURNS));
        let _ = std::fs::remove_dir_all(&root);
        assert!(why.contains("recorded 1 consult(s), 2 expected"), "{why}");

        // an opted-in seat (even beside a clean consult) did not exercise the switch
        let (root, work, home) = switch_fixture(
            "optin",
            &[(T1, Some(false)), (T2, Some(true))],
            &[
                (T1, smoke_rollout(T1, "disabled", false)),
                (T2, smoke_rollout(T2, "v2", true)),
            ],
        );
        let why = unproven_reason(subagent_switch_check(&work, &home, SMOKE_TURNS));
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            why.contains("consult n=2 ran with sub-agents allowed by the seat's codex_config"),
            "{why}"
        );

        // no terminal record for the turn; an unreadable (split) record
        let mut open = smoke_rollout(T1, "disabled", false);
        open.pop();
        let (root, work, home) = switch_fixture("open", &[(T1, Some(false))], &[(T1, open)]);
        let why = unproven_reason(subagent_switch_check(&work, &home, 1));
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            why.contains("holds no terminal record for its turn"),
            "{why}"
        );
        let mut split = smoke_rollout(T1, "disabled", false);
        split.push(r#"{"timestamp":"2026-11-22T12:09:53.000Z","type":"turn_cont"#.to_string());
        let (root, work, home) = switch_fixture("split", &[(T1, Some(false))], &[(T1, split)]);
        let why = unproven_reason(subagent_switch_check(&work, &home, 1));
        let _ = std::fs::remove_dir_all(&root);
        assert!(why.contains("holds 1 unreadable record(s)"), "{why}");

        // a thread two consults share needs a terminal record per consult
        let (root, work, home) = switch_fixture(
            "shared",
            &[(T1, Some(false)), (T1, Some(false))],
            &[(T1, smoke_rollout(T1, "disabled", false))],
        );
        let why = unproven_reason(subagent_switch_check(&work, &home, SMOKE_TURNS));
        let _ = std::fs::remove_dir_all(&root);
        assert!(why.contains("1 task_complete for 2 consult(s)"), "{why}");

        let (root, work, home) = switch_fixture("nothread", &[], &[]);
        let why = unproven_reason(subagent_switch_check(&work, &home, SMOKE_TURNS));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(
            why,
            "the sub-agent switch is unproven: the smoke's consults recorded no codex thread"
        );
        assert_eq!(
            Smoke::Unproven("x".into()).hold(),
            Some(("unproven", "x".to_string())),
            "an automatic update holds on an unproven smoke"
        );
        assert_eq!(Smoke::Pass.hold(), None);
    }

    /// (F11-8) A relative Codex home is resolved once to an absolute path (the consult children run
    /// from the scratch repo).
    #[test]
    fn a_relative_codex_home_is_made_absolute() {
        let abs = absolute_home("rel-home");
        assert!(std::path::Path::new(&abs).is_absolute(), "{abs}");
        assert!(abs.ends_with("rel-home"), "{abs}");
        assert_eq!(absolute_home(""), "");
    }

    #[test]
    fn scripted_outcomes() {
        let t = Target {
            provider: "grok".into(),
            model: "grok-4.7".into(),
            bind: Bind::Entry(1),
        };
        assert_eq!(scripted("grok::grok-4.7=pass", &t), Smoke::Pass);
        assert!(matches!(
            scripted("grok::grok-4.7=fail", &t),
            Smoke::Fail(_)
        ));
        assert!(matches!(scripted("other::x=pass", &t), Smoke::Fail(_)));
        // fail-then-pass: the first smoke of the seat fails, the next passes
        let u = Target {
            provider: "unit-ftp".into(),
            model: "m-1".into(),
            bind: Bind::Label,
        };
        assert!(matches!(
            scripted("unit-ftp::m-1=fail-then-pass", &u),
            Smoke::Fail(_)
        ));
        assert_eq!(scripted("unit-ftp::m-1=fail-then-pass", &u), Smoke::Pass);
    }

    /// (F02-16) A roster seat is bound by its position; an explicit override is a label.
    #[test]
    fn a_roster_seat_target_is_bound_to_its_position() {
        let mut on_req = entry(1, "alibaba", "claude", "qwen3.7-max", "", "unit-nobudget");
        on_req.on_request = true;
        let r = roster(vec![
            on_req,
            entry(2, "alibaba", "claude", "qwen3.8-max", "", "unit-nobudget"),
        ]);
        let t = pick_target("", &r, "claude", now()).unwrap();
        assert_eq!(t.model, "qwen3.8-max");
        assert_eq!(t.bind, Bind::Entry(2));
        let t = pick_target("alibaba::qwen3.7-max", &r, "claude", now()).unwrap();
        assert_eq!(t.bind, Bind::Label);
    }

    /// (F02-16) The bound roster holds exactly the entry at that position - the operator's own JSON
    /// for it, the model overridden - and nothing else a label could resolve to; a protected,
    /// on-request, inactive, moved or missing entry is refused.
    #[test]
    fn the_bound_roster_holds_only_that_entry() {
        let text = r##"{"roster_version":1,"require":{"framing":["#1"]},"reviewers":[
            {"provider":"alibaba","engine":"claude","model":"qwen3.7-max","auth":"endpoint","endpoint":{"base_url":"https://a.example/v1","env_key":"KEY_A"},"ext":{"on_request":true}},
            {"provider":"alibaba","engine":"claude","model":"qwen3.8-max","auth":"endpoint","endpoint":{"base_url":"https://b.example/v1","env_key":"KEY_B"},"plan":"alibaba","panel":"weighty"},
            {"provider":"grok","engine":"grok","model":"grok-4.7","ext":{"reserve_percent":15}}
        ],"ext":{"c3":{"inactive":["dormant"],"reviewers":[]}}}"##;
        let one = bound_roster(text, 2, "alibaba", "qwen4-max").unwrap();
        let v: serde_json::Value = serde_json::from_str(&one).unwrap();
        let mut expected: serde_json::Value = serde_json::from_str(text).unwrap();
        let mut e = expected["reviewers"][1].take();
        e["model"] = serde_json::Value::String("qwen4-max".into());
        assert_eq!(
            v,
            serde_json::json!({"roster_version": 1, "reviewers": [e]}),
            "only that entry, model overridden"
        );
        // and it is a roster the validator accepts, with the entry's own endpoint
        let r = c3_core::roster::validate_roster("bound.json", &one, None);
        assert!(r.error.is_empty(), "{}", r.error);
        assert_eq!(r.entries.len(), 1);
        assert_eq!(
            r.entries[0].endpoint.as_ref().unwrap().base_url,
            "https://b.example/v1"
        );
        assert!(bound_roster(text, 1, "alibaba", "qwen4-max")
            .unwrap_err()
            .contains("on request"));
        assert!(bound_roster(text, 3, "grok", "grok-4.8")
            .unwrap_err()
            .contains("protected"));
        assert!(bound_roster(text, 2, "grok", "qwen4-max").is_err());
        assert!(bound_roster(text, 4, "alibaba", "qwen4-max").is_err());
        assert!(bound_roster(text, 0, "alibaba", "qwen4-max").is_err());
        let dormant = text.replace("\"plan\":\"alibaba\"", "\"plan\":\"dormant\"");
        assert!(bound_roster(&dormant, 2, "alibaba", "qwen4-max")
            .unwrap_err()
            .contains("inactive"));
    }
}
