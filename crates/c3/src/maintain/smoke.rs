//! The smoke test a maintenance pass runs before trusting a CLI: a two-turn `c3 consult` through
//! the installed binary against a throwaway repo, and the choice of which roster seat to smoke.
//!
//! Turn 1 is a raw "reply with exactly OK"; turn 2 asks for the exact content of a `token.txt`
//! holding a random token, which forces a read (a tool call). The pass is: both turns exit 0 and the
//! reply of turn 2 contains the token (so the file was actually read). (C3 0.2.5) A codex smoke must
//! also prove the sub-agent switch: the run's own rollouts must show `multi_agent_version`
//! `disabled` and no multi-agent tool ([`subagent_switch_check`]) - otherwise it fails, and a
//! post-update smoke failure rolls the update back.
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
}

/// A smoke target: the provider label and model id to consult.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub provider: String,
    pub model: String,
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
/// throwaway repo in; `token` the random token written into `token.txt`. Under test mode a
/// `C3_MAINTAIN_TEST_SMOKE` script decides the outcome without a real consult.
pub fn run_smoke(
    env: &dyn Env,
    self_exe: &str,
    target: &Target,
    engine: &str,
    work: &std::path::Path,
    token: &str,
) -> Smoke {
    if c3_core::test_hooks::mode_on() {
        if let Some(script) = std::env::var("C3_MAINTAIN_TEST_SMOKE")
            .ok()
            .filter(|s| !s.is_empty())
        {
            return scripted(&script, target);
        }
    }
    run_consult_smoke(env, self_exe, target, engine, work, token)
}

/// Per-seat call counter so a `pass-then-fail` verdict can distinguish the installed-version smoke
/// (call 0) from the post-update smoke (call 1) in tests.
fn call_counts() -> &'static std::sync::Mutex<std::collections::HashMap<String, usize>> {
    static C: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, usize>>> =
        std::sync::OnceLock::new();
    C.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// The scripted outcome under `C3_MAINTAIN_TEST_SMOKE=provider::model=<verdict>,...` where the
/// verdict is `pass`, `fail` or `pass-then-fail` (the first smoke of that seat passes, the rest
/// fail). A seat not listed fails, so a test opts a seat into passing explicitly.
fn scripted(script: &str, target: &Target) -> Smoke {
    let key = format!("{}::{}", target.provider, target.model);
    for item in script.split(',') {
        if let Some((seat, verdict)) = item.split_once('=') {
            if seat.trim() != key {
                continue;
            }
            let verdict = verdict.trim().to_ascii_lowercase();
            return match verdict.as_str() {
                "pass" => Smoke::Pass,
                "pass-then-fail" => {
                    let mut g = call_counts().lock().unwrap_or_else(|e| e.into_inner());
                    let n = g.entry(key.clone()).or_insert(0);
                    let first = *n == 0;
                    *n += 1;
                    if first {
                        Smoke::Pass
                    } else {
                        Smoke::Fail("scripted post-update failure".to_string())
                    }
                }
                _ => Smoke::Fail("scripted failure".to_string()),
            };
        }
    }
    Smoke::Fail("no scripted verdict".to_string())
}

/// The real two-turn smoke through `c3 consult`.
fn run_consult_smoke(
    env: &dyn Env,
    self_exe: &str,
    target: &Target,
    engine: &str,
    work: &std::path::Path,
    token: &str,
) -> Smoke {
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
        "new",
        "reply with exactly OK",
    );
    if !turn1.ok() {
        return Smoke::Fail(format!("turn 1 failed ({})", turn1.fail_reason()));
    }

    // Turn 2: must read the file (a tool call) and echo its content; auto-mode forks the thread.
    let turn2 = consult_turn(
        env,
        self_exe,
        work,
        &reviewer,
        "",
        "Reply with the exact content of the file token.txt in this repository.",
    );
    if !turn2.ok() {
        return Smoke::Fail(format!("turn 2 failed ({})", turn2.fail_reason()));
    }

    // Pass when the reply of turn 2 contains the token (the file was read) — the proxy for a
    // recorded tool call. The reply is in the handoff; fall back to the console output.
    if !(reply_contains(work, token) || turn2.combined().contains(token)) {
        return Smoke::Fail("the reply did not contain the token (no file read)".to_string());
    }
    // (C3 0.2.5) a codex smoke also proves the sub-agent switch from its own rollouts.
    if engine == "codex" {
        return subagent_switch_check(work, &crate::providers::get_codex_home());
    }
    Smoke::Pass
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
/// A missing rollout FAILS too (it is not a pass with a warning): the switch is then unproven, and the
/// budget watch's sub-agent guard - the defence behind the switch - reads the same rollouts, so a CLI
/// whose rollouts C3 cannot find runs with a blind guard. Rolling such an update back costs one held
/// update; letting it through risks the incident again. A rollout that names neither a
/// `multi_agent_version` nor a tool list fails for the same reason. A consult the operator opted into
/// sub-agents (its roster entry's `codex_config`) is not judged.
pub(crate) fn subagent_switch_check(work: &std::path::Path, codex_home: &str) -> Smoke {
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
    let mut threads: Vec<String> = Vec::new();
    let mut opted_in = 0usize;
    for c in &consults {
        let Some(t) = c
            .get("thread")
            .and_then(|t| t.as_str())
            .filter(|t| !t.is_empty())
        else {
            continue;
        };
        if c.get("subagents_allowed").and_then(|a| a.as_bool()) == Some(true) {
            opted_in += 1;
            continue;
        }
        if !threads.iter().any(|x| x == t) {
            threads.push(t.to_string());
        }
    }
    if threads.is_empty() {
        if opted_in > 0 {
            eprintln!(
                "c3 maintain smoke: the sub-agent switch was not exercised (the seat's codex_config turns sub-agents on)"
            );
            return Smoke::Pass;
        }
        return Smoke::Fail(
            "the sub-agent switch is unproven: the smoke's consults recorded no codex thread"
                .to_string(),
        );
    }
    for t in &threads {
        let Some(path) = c3_core::quota::codex_rollout_path(t, codex_home) else {
            return Smoke::Fail(format!(
                "the sub-agent switch is unproven: no rollout of thread {t} under {} (the sub-agent guard reads the same rollouts)",
                std::path::Path::new(codex_home).join("sessions").display()
            ));
        };
        let s = c3_core::quota::rollout_subagent_surface(&path);
        if let Some(why) = s.breach() {
            return Smoke::Fail(format!(
                "the sub-agent switch did not hold: thread {t} - {why} - although the bridge passed -c {} -c {}",
                c3_core::quota::MULTI_AGENT_OFF,
                c3_core::quota::AGENTS_OFF
            ));
        }
        if !s.has_evidence() {
            return Smoke::Fail(format!(
                "the sub-agent switch is unproven: the rollout of thread {t} names no multi_agent_version and no tool list"
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
    env.run(self_exe, &args, &[], Some(work))
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
        ]
    }

    fn fail_reason(s: Smoke) -> String {
        match s {
            Smoke::Fail(why) => why,
            Smoke::Pass => panic!("expected a failure"),
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
        let got = subagent_switch_check(&work, &home);
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
        let why = fail_reason(subagent_switch_check(&work, &home));
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
        let why = fail_reason(subagent_switch_check(&work, &home));
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            why.contains(&format!("thread {T1} - offers spawn_agent, wait_agent")),
            "{why}"
        );
    }

    /// (C3 0.2.5) A recorded thread without a rollout, a rollout that names no version and no tool
    /// list, and a smoke that recorded no thread all FAIL (the switch is unproven, and the guard
    /// reads the same rollouts); a consult the seat opted into sub-agents is not judged.
    #[test]
    fn the_codex_smoke_fails_when_the_switch_is_unproven() {
        let (root, work, home) = switch_fixture("missing", &[(T1, Some(false))], &[]);
        let why = fail_reason(subagent_switch_check(&work, &home));
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            why.starts_with(&format!(
                "the sub-agent switch is unproven: no rollout of thread {T1} under "
            )),
            "{why}"
        );

        let bare = vec![
            r#"{"timestamp":"2026-11-22T12:09:52.300Z","type":"event_msg","payload":{"type":"task_started"}}"#.to_string(),
        ];
        let (root, work, home) = switch_fixture("bare", &[(T1, Some(false))], &[(T1, bare)]);
        let why = fail_reason(subagent_switch_check(&work, &home));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(
            why,
            format!("the sub-agent switch is unproven: the rollout of thread {T1} names no multi_agent_version and no tool list")
        );

        let (root, work, home) = switch_fixture("nothread", &[], &[]);
        let why = fail_reason(subagent_switch_check(&work, &home));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(
            why,
            "the sub-agent switch is unproven: the smoke's consults recorded no codex thread"
        );

        let (root, work, home) = switch_fixture(
            "optin",
            &[(T1, Some(true))],
            &[(T1, smoke_rollout(T1, "v2", true))],
        );
        let got = subagent_switch_check(&work, &home);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(got, Smoke::Pass, "an opted-in consult is not judged");
    }

    #[test]
    fn scripted_outcomes() {
        let t = Target {
            provider: "grok".into(),
            model: "grok-4.7".into(),
        };
        assert_eq!(scripted("grok::grok-4.7=pass", &t), Smoke::Pass);
        assert!(matches!(
            scripted("grok::grok-4.7=fail", &t),
            Smoke::Fail(_)
        ));
        assert!(matches!(scripted("other::x=pass", &t), Smoke::Fail(_)));
    }
}
