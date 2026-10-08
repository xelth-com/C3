//! The panel path of `c3 consult` (`--panel`/`--panel-all`): the parent scheduler that runs a
//! whole panel end to end (`codex-consult.ps1:2350-2888`).
//!
//! `build` computes the plan the parent would run (read the roster, resolve availability, the
//! required reviewers, the seeded seat draw, the roles, the concurrency plan) — everything that
//! touches no disk. `schedule` then takes the task lock (a real run), reserves every seat's
//! `.consult.pending-<NN>.json`, launches each seat as a child `c3 consult --task <t>
//! --panel-spec <b64>` process (its console to `<temp>/codex-consult-panel-<id>/<NN>.out|.err`),
//! polls them along the endpoint-group plan (parallel across groups up to the caps, one after
//! another within a group unless the roster's `parallel` raises it, `-PanelConcurrency` caps the
//! total), collects each member's outcome (its last `codex-consult: ` stdout line plus its
//! committed ledger entry), patches every member's `panel.started`/`panel.usable` after all
//! finish, prints the byte-identical summary block, and returns the exit code.
//!
//! A **dry run** takes the same path with `dry_run=true` members and no lock/records: each seat's
//! child prints its own single-run dry-run block, and the summary marks it `planned`/`refused`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Instant;

use c3_core::lineage::format_reviewer_lineage;
use c3_core::store::{
    EvidenceStore, FilesStore, LockRecord, PendingRecord, PendingRef, PendingState,
};
use c3_core::task_slug::TaskSlug;
use serde_json::{json, Value};

use crate::consult::args::{Options, Resolved};
use crate::panel::member::{MemberBrief, MemberSpec};
use crate::providers;

use super::plan::{self, MemberInput, PanelPlan, PanelRoutingResult, RoutingRecord, Runner};
use super::routing::Rating;

const TOOL: &str = "codex-consult";

/// Run the panel path; return the process exit code.
pub fn run(o: Options, r: Resolved, _home: Option<&str>) -> i32 {
    match build(&o, &r) {
        Ok(built) => schedule(o, r, built),
        Err((msg, code)) => {
            eprintln!("{TOOL}: {msg}");
            code
        }
    }
}

/// The foreground of `-Detach -Panel` (D2, D8): resolve the plan (no lock/records), compute the
/// budget (D4) and the planned members, then spawn the background — or refuse with nothing written.
pub fn detach_foreground(o: Options, r: Resolved, _home: Option<&str>) -> i32 {
    // A missing brief is refused in the foreground (D2), before anything is planned or written.
    if !o.brief.is_empty() {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let brief_path = if Path::new(&o.brief).is_absolute() {
            PathBuf::from(&o.brief)
        } else {
            cwd.join(&o.brief)
        };
        if !brief_path.is_file() {
            eprintln!(
                "{TOOL}: brief '{}' not found (this script never writes briefs; write it first).",
                o.brief
            );
            return 1;
        }
    }
    let b = match build(&o, &r) {
        Ok(b) => b,
        Err((msg, code)) => {
            eprintln!("{TOOL}: {msg}");
            return code;
        }
    };
    if b.seated_count == 0 {
        eprintln!(
            "{TOOL}: -Panel found no available reviewer to seat from the roster '{}'; nothing was started.",
            b.roster_path
        );
        return 1;
    }
    // An active recovery record refuses in the foreground, before anything is written (D2).
    {
        let store = FilesStore::new(b.collab_root.clone());
        let assessed = crate::consult::recovery::assess(&store, &b.task);
        if let Some(err) = assessed.error {
            eprintln!("{TOOL}: {err}");
            return 1;
        }
        if let Some(msg) = assessed.active_message() {
            eprintln!("{TOOL}: {msg}");
            return 1;
        }
    }
    let guard_hook: f64 = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_PANEL_GUARD_SEC")
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|v| *v > 0.0)
        .unwrap_or(0.0);
    let guard_of = |pos: i64| -> i64 {
        let ru = b.runners.iter().find(|ru| ru.position == pos);
        let g = if guard_hook > 0.0 {
            guard_hook
        } else {
            let denial = ru
                .map(|ru| {
                    c3_core::lineage::engine_spec(&ru.engine)
                        .map(|s| s.denial_retry)
                        .unwrap_or(false)
                })
                .unwrap_or(false)
                && o.denial_retry == 1;
            panel_member_guard(r.timeout_sec, r.continue_sec, r.repair_enabled, denial)
        };
        g.round() as i64
    };
    let groups: Vec<(i64, Vec<i64>)> = b
        .concurrency
        .groups
        .iter()
        .map(|g| (g.limit, g.positions.clone()))
        .collect();
    let budget =
        crate::consult::detached::detached_budget(&groups, &guard_of, o.panel_concurrency, 120);
    let mut members: Vec<crate::consult::detach::PlannedMember> = b
        .runners
        .iter()
        .map(|ru| crate::consult::detach::PlannedMember {
            position: ru.position,
            lineage: ru.shown.clone(),
            state: "pending".into(),
            outcome: String::new(),
        })
        .collect();
    members.sort_by_key(|m| m.position);
    let plan = format!(
        "a review panel of {} of {} roster entries, {} (purpose {}, timeout {} s per member)",
        b.seated_count, b.entry_count, b.concurrency.text, r.purpose_label, r.timeout_sec
    );
    let warnings = b.warnings.clone();
    crate::consult::detach::start_detached_run(&o, "panel", &members, budget, &plan, "", &warnings)
}

/// A seated runner (seat order), as `build` resolves it (no disk yet).
struct RunnerInfo {
    position: i64,
    entry: c3_core::roster::RosterEntry,
    engine: String,
    shown: String,
    role: String,
    required: bool,
    group: usize,
    /// (wave 26b, D13) the member's resolved endpoint fingerprint, for the machine-wide parallel
    /// limit (`""` when unresolved).
    fingerprint: String,
}

/// A display row (roster order), for the plan block and the summary.
struct DisplayRow {
    position: i64,
    shown: String,
    /// `run` | `not-picked` | `skipped`.
    state: String,
    reason: String,
    required: bool,
    role: String,
    /// The seated runner index (into `runners`) when `state == run` and picked.
    runner: Option<usize>,
}

/// Everything `build` resolves before the lock; `schedule` consumes it.
struct Built {
    panel_id: String,
    roster_path: String,
    collab_root: PathBuf,
    caller_cwd: PathBuf,
    task: TaskSlug,
    codex_launcher: String,
    entry_count: usize,
    seated_count: usize,
    rows: Vec<DisplayRow>,
    runners: Vec<RunnerInfo>,
    routing_rec: RoutingRecord,
    routing_value: Value,
    concurrency: PanelPlan,
    limits_value: Value,
    warnings: Vec<String>,
    /// (wave 27, R13 D3) the coordinator-is-a-reviewer warnings for seated members, printed in the
    /// plan but kept OUT of `warnings` (which becomes every member's `panel_warnings`).
    coordinator_warnings: Vec<String>,
    topics: Vec<String>,
    range_line: String,
    range_warning: String,
    size_asked: i64,
    k: i64,
    members_record: Vec<MemberBrief>,
    skipped_record: Value,
    roles_note: String,
    parent_start: String,
    verb: &'static str,
    /// (wave 26b, D11) per-member timeout exceptions for the `Timeout:` header, e.g.
    /// `#2 ZAI :: glm-5.3 120 s (roster)`; empty when every member takes the shared default.
    timeout_exceptions: Vec<String>,
}

/// Resolve the whole plan (`Select-PanelMembers`, required reviewers, `Select-PanelRouting`, the
/// roles, `Get-PanelPlan`) — everything before the lock.
fn build(o: &Options, r: &Resolved) -> Result<Built, (String, i32)> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = providers::resolve_repo_root(&cwd);
    let collab_root = providers::resolve_collab_root(&repo_root, &o.collab_dir);
    let task = TaskSlug::new(o.task.clone()).map_err(|e| (e.to_string(), 1))?;

    let launcher = providers::resolve_codex_launcher(&o.codex_exe).map_err(|m| (m, 1))?;
    let config = providers::read_codex_config(&providers::get_codex_config_path());
    let openai_base_url = std::env::var("OPENAI_BASE_URL").unwrap_or_default();

    let roster = providers::read_reviewer_roster().map_err(|m| (m, 1))?;
    if !roster.exists {
        let msg = if roster.disabled {
            "-Panel needs a reviewer roster, and CODEX_CONSULT_ROSTER=none switches it off."
                .to_string()
        } else {
            format!(
                "-Panel needs a reviewer roster: '{}' does not exist (CODEX_CONSULT_ROSTER, else <codex home>/codex-consult-roster.json).",
                roster.path
            )
        };
        return Err((msg, 1));
    }
    // (wave 27) an unparseable CODEX_CONSULT_COORDINATOR refuses before any member is planned or
    // started (each seated member's build_context records the coordinator; the top-level parse
    // failure must fire once here so nothing is started).
    let coordinator = crate::consult::orchestrate::resolve_coordinator(Some(&roster.entries[..]))?;
    // (D2) a missing brief is refused before any member is planned or started (no ledger).
    if !o.brief.is_empty() {
        let bp = if Path::new(&o.brief).is_absolute() {
            PathBuf::from(&o.brief)
        } else {
            cwd.join(&o.brief)
        };
        if !bp.is_file() {
            return Err((
                format!(
                    "brief '{}' not found (this script never writes briefs; write it first).",
                    o.brief
                ),
                1,
            ));
        }
    }

    let utc_now = c3_core::peak::consult_clock(0)
        .map(|(u, _, _)| u)
        .unwrap_or_else(|_| chrono::Utc::now());

    let ctx = providers::Ctx::for_consult(
        config.clone(),
        providers::read_all_task_consults_health(&collab_root),
        roster.clone(),
        launcher.clone(),
        openai_base_url.clone(),
        utc_now,
    );
    // (wave 26b, D16 b) the new prompt's estimated token size ((ask + brief) / 4 chars a token);
    // a member whose context window the brief alone would overflow is skipped before its start
    // (inside `panel_members`, before the light gate, as `Select-PanelMembers` does).
    let panel_estimate = {
        let mut chars = o.prompt.chars().count() as i64;
        if !o.brief.is_empty() {
            let bp = if Path::new(&o.brief).is_absolute() {
                PathBuf::from(&o.brief)
            } else {
                cwd.join(&o.brief)
            };
            if let Ok(m) = std::fs::metadata(&bp) {
                chars += m.len() as i64;
            }
        }
        ((chars as f64) / 4.0).ceil() as i64
    };
    let selection = ctx.panel_members(
        &o.model,
        &o.engine,
        &o.purpose,
        o.panel_all,
        o.skip_preflight,
        panel_estimate,
    );

    // Required reviewers (exit 5 on an outage).
    let require_given = !o.require.is_empty();
    let required =
        plan::resolve_required_reviewers(&roster, &o.require, &o.purpose, require_given, true);
    if !required.error.is_empty() {
        return Err((format!("{}.", required.error), 1));
    }
    let mut required_problems: Vec<String> = Vec::new();
    for &pos in &required.positions {
        match selection
            .members
            .iter()
            .find(|m| m.entry.position as i64 == pos)
        {
            None => {
                let entry = roster.entries.iter().find(|e| e.position as i64 == pos);
                let mut filters: Vec<String> = Vec::new();
                if !o.engine.is_empty() {
                    filters.push(format!("-Engine {}", o.engine));
                }
                if !o.model.is_empty() {
                    filters.push(format!("-Model {}", o.model));
                }
                let shown = entry
                    .map(|e| {
                        if !e.model.is_empty() {
                            format_reviewer_lineage(&e.provider, &e.model, &e.engine)
                        } else {
                            e.provider.clone()
                        }
                    })
                    .unwrap_or_default();
                required_problems.push(format!(
                    "#{pos} {shown} (not in this panel: {})",
                    filters.join(" ")
                ));
            }
            // a required entry only the weighty or light gate held back is no outage (it takes
            // a seat first)
            Some(m)
                if m.state == "skipped" && m.skip_kind != "weighty" && m.skip_kind != "light" =>
            {
                required_problems.push(format!(
                    "#{pos} {} ({})",
                    format_reviewer_lineage(
                        &m.identity.provider,
                        &m.identity.model,
                        if m.entry.engine.is_empty() {
                            "codex"
                        } else {
                            &m.entry.engine
                        }
                    ),
                    m.reason
                ));
            }
            _ => {}
        }
    }
    if !required_problems.is_empty() {
        let plural = required_problems.len() != 1;
        let them = if plural { "them" } else { "it" };
        let without = if required.source == "-Require" {
            "that -Require"
        } else {
            "the requirement (-Require none)"
        };
        return Err((
            format!(
                "required reviewer{} not available ({}): {}; nothing was started - wait for {them}, or run without {without} (exit 5).",
                if plural { "s" } else { "" },
                required.source,
                required_problems.join("; ")
            ),
            5,
        ));
    }

    // The seats (`Select-PanelRouting`) — routed by the real ratings, else roster order.
    let members: Vec<MemberInput> = selection
        .members
        .iter()
        .map(|m| MemberInput {
            entry: m.entry.clone(),
            provider: m.identity.provider.clone(),
            model: m.identity.model.clone(),
            engine: if m.entry.engine.is_empty() {
                "codex".into()
            } else {
                m.entry.engine.clone()
            },
            state: m.state.clone(),
            reason: m.reason.clone(),
            skip_kind: m.skip_kind.clone(),
        })
        .collect();

    // (wave 26b, D11) the per-member timeout exceptions for the `Timeout:` header: an eligible
    // member whose roster entry names a timeout_sec, unless an explicit --timeout-sec applies to all.
    let timeout_exceptions: Vec<String> = if r.timeout_source == "explicit" {
        Vec::new()
    } else {
        members
            .iter()
            .filter(|m| m.state != "skipped" && m.entry.timeout_sec >= 60)
            .map(|m| {
                format!(
                    "#{} {} {} s (roster)",
                    m.entry.position,
                    format_reviewer_lineage(&m.provider, &m.model, &m.engine),
                    m.entry.timeout_sec
                )
            })
            .collect()
    };

    let (size_wanted, size_source) = if o.panel_all {
        (0, "-PanelAll")
    } else if o.panel_size_given {
        (o.panel_size as i32, "-PanelSize")
    } else {
        (plan::default_size(&o.purpose), "purpose")
    };
    let (nonce, nonce_source) = panel_nonce(o, utc_now);
    let brief_sha = brief_sha256(o, &cwd);
    let (topics, _terr) = c3_core::roster::convert_to_slug_list(&o.topic, "-Topic");
    let order = if o.panel_order.trim().is_empty() {
        "routed".to_string()
    } else {
        o.panel_order.trim().to_lowercase()
    };
    // The all-task rating store, read for the routed draw (`Read-AllTaskRatings`).
    let ratings: Vec<Rating> = super::routing::read_all_task_ratings(&collab_root);

    let router_ctx = crate::router::load_context();
    let route: PanelRoutingResult = plan::select_panel_routing_with(
        &router_ctx,
        &members,
        size_wanted,
        size_source,
        &order,
        &ratings,
        &o.purpose,
        &topics,
        utc_now,
        &o.task,
        &brief_sha,
        &nonce,
        nonce_source,
        &required.positions,
    );
    if route.picked.is_empty() {
        return Err((
            if !selection.error.is_empty() {
                selection.error
            } else {
                format!(
                    "no reviewer of the roster '{}' is eligible for this panel; nothing was started (run codex-providers.ps1 for the full picture)",
                    roster.path
                )
            },
            1,
        ));
    }

    let mut warnings: Vec<String> = route.warnings.clone();

    // Roles: `-Role` names one role for every seat; `-Roles` assigns by score rank + willingness.
    let mut role_of: HashMap<i64, String> = HashMap::new();
    let mut roles_note = String::new();
    if !o.role.is_empty() {
        for p in &route.picked {
            role_of.insert(p.position, o.role.clone());
        }
    } else if !o.roles.is_empty() {
        let role_members: Vec<super::roles::RoleMember> = route
            .picked
            .iter()
            .map(|p| {
                let entry = selection
                    .members
                    .iter()
                    .find(|m| m.entry.position as i64 == p.position);
                super::roles::RoleMember {
                    position: p.position,
                    score: p.score.score,
                    roles: entry.map(|m| m.entry.roles.clone()).unwrap_or_default(),
                }
            })
            .collect();
        let assign = super::roles::select_role_assignment(&role_members, &o.roles);
        if !assign.error.is_empty() {
            return Err((format!("{}; nothing was started.", assign.error), 1));
        }
        role_of = assign.of;
        if !assign.note.is_empty() {
            roles_note = assign.note.clone();
            warnings.push(assign.note);
        }
    }
    // (wave 26b, D1) resolve each assigned role's file up front, so a role file outside its roles
    // directory, a reparse-point role file, or a junctioned roles directory refuses the whole
    // panel before any member starts (in the plugin's wording, `-Role`/`-Roles` prefix).
    if !role_of.is_empty() {
        let flag = if !o.role.is_empty() {
            "-Role"
        } else {
            "-Roles"
        };
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for name in role_of.values() {
            if name.is_empty() || !seen.insert(name.clone()) {
                continue;
            }
            let ri =
                super::roles::resolve_role_file(name, &collab_root, &super::roles::plugin_root());
            // Only a safety problem (reparse/containment) refuses the panel; a template-only role
            // (unknown to c3, which ships no templates) is tolerated — the willingness assignment
            // and its note stand.
            if super::roles::is_role_refusal(&ri.error) {
                return Err((format!("{flag}: {}", ri.error), 1));
            }
        }
    }

    // -MaxModelSteps only when a seat runs an engine with a step cap (muse).
    if o.max_model_steps > 0
        && !route.picked.iter().any(|p| {
            let e = selection
                .members
                .iter()
                .find(|m| m.entry.position as i64 == p.position)
                .map(|m| {
                    if m.entry.engine.is_empty() {
                        "codex".into()
                    } else {
                        m.entry.engine.clone()
                    }
                })
                .unwrap_or_default();
            c3_core::lineage::engine_spec(&e)
                .map(|s| !s.steps_flag.is_empty())
                .unwrap_or(false)
        })
    {
        return Err((
            "-MaxModelSteps applies to the muse members of a panel (--max-model-steps); no member of this panel runs the muse engine.".into(),
            1,
        ));
    }

    // The concurrency plan (`Get-PanelPlan`) over the seated runners.
    let plan_runners: Vec<Runner> = route
        .picked
        .iter()
        .map(|p| {
            let row = selection
                .members
                .iter()
                .find(|m| m.entry.position as i64 == p.position);
            Runner {
                position: p.position,
                label: row.map(|m| m.entry.provider.clone()).unwrap_or_default(),
                fingerprint: row
                    .map(|m| m.identity.fingerprint.clone())
                    .unwrap_or_default(),
            }
        })
        .collect();
    let concurrency = plan::panel_plan(&plan_runners, &roster.parallel, o.panel_concurrency);
    let group_of: HashMap<i64, usize> = concurrency.group_of.iter().copied().collect();

    // A launcher every member of an engine would miss is refused once, up front (a real run).
    if !o.dry_run {
        let mut seen: Vec<String> = Vec::new();
        for p in &route.picked {
            let m = selection
                .members
                .iter()
                .find(|m| m.entry.position as i64 == p.position);
            let eng = m
                .map(|m| {
                    if m.entry.engine.is_empty() {
                        "codex".into()
                    } else {
                        m.entry.engine.clone()
                    }
                })
                .unwrap_or_default();
            if seen.contains(&eng) {
                continue;
            }
            seen.push(eng.clone());
            // The http engine sends one OpenAI-compatible request built from a reviewer pack; it
            // has no CLI launcher, so it is exempt from the launcher-not-found refusal (its
            // billing/key guard runs per seat in `consult::http`).
            if eng == "http" {
                continue;
            }
            if eng == "codex" {
                if launcher.is_empty() {
                    return Err((
                        "codex CLI not found on PATH (set -CodexExe <path> or the CODEX_CONSULT_EXE environment variable).".into(),
                        1,
                    ));
                }
            } else if providers::resolve_engine_launcher(&eng, "")
                .unwrap_or(None)
                .unwrap_or_default()
                .is_empty()
            {
                let exe_env = c3_core::lineage::engine_spec(&eng)
                    .map(|s| s.exe_env)
                    .unwrap_or("");
                return Err((
                    format!("{eng} CLI not found on PATH (set -EngineExe <path> or the {exe_env} environment variable)."),
                    1,
                ));
            }
        }
    }

    let panel_id = uuid::Uuid::new_v4().to_string();

    // The members / skipped records (roster order), the display rows and the runners.
    let members_record: Vec<MemberBrief> = selection
        .members
        .iter()
        .map(|m| MemberBrief {
            provider: m.entry.provider.clone(),
            model: m.identity.model.clone(),
            state: m.state.clone(),
            reason: m.reason.clone(),
        })
        .collect();
    let skipped_record: Value = Value::Array(
        selection
            .members
            .iter()
            .filter(|m| m.state == "skipped")
            .map(|m| {
                json!({
                    "provider": m.entry.provider,
                    "model": m.identity.model,
                    "engine": if m.entry.engine.is_empty() { "codex".to_string() } else { m.entry.engine.clone() },
                    "reason": m.reason,
                })
            })
            .collect(),
    );

    let mut runners: Vec<RunnerInfo> = Vec::new();
    // (wave 27, R13 D3) seated members that ARE the coordinator's own model: warned once in the
    // plan (dry run and real-run header), NOT in the members' panel_warnings (each such member
    // warns in its own ledger entry via its build_context). Uses the seat's resolved identity.
    let mut coordinator_warnings: Vec<String> = Vec::new();
    for p in &route.picked {
        let m = selection
            .members
            .iter()
            .find(|m| m.entry.position as i64 == p.position);
        let engine = m
            .map(|m| {
                if m.entry.engine.is_empty() {
                    "codex".into()
                } else {
                    m.entry.engine.clone()
                }
            })
            .unwrap_or_else(|| "codex".into());
        if let Some(m) = m {
            if let Some(w) = c3_core::host::coordinator_reviewer_warning(
                &coordinator,
                &m.entry.provider,
                &m.identity.model,
                &engine,
                &p.lineage,
            ) {
                coordinator_warnings.push(w);
            }
        }
        runners.push(RunnerInfo {
            position: p.position,
            entry: m.map(|m| m.entry.clone()).unwrap_or_default(),
            engine,
            shown: p.lineage.clone(),
            role: role_of.get(&p.position).cloned().unwrap_or_default(),
            required: p.required,
            group: *group_of.get(&p.position).unwrap_or(&0),
            fingerprint: m
                .map(|m| m.identity.fingerprint.clone())
                .unwrap_or_default(),
        });
    }
    let runner_of: HashMap<i64, usize> = runners
        .iter()
        .enumerate()
        .map(|(i, ru)| (ru.position, i))
        .collect();

    let mut rows: Vec<DisplayRow> = Vec::new();
    for m in &route.members {
        rows.push(DisplayRow {
            position: m.position,
            shown: m.lineage.clone(),
            state: m.state.clone(),
            reason: m.reason.clone(),
            required: m.required,
            role: role_of.get(&m.position).cloned().unwrap_or_default(),
            runner: runner_of
                .get(&m.position)
                .copied()
                .filter(|_| m.state == "run"),
        });
    }

    let limits_value = {
        let mut map = serde_json::Map::new();
        for (label, limit) in &concurrency.limits {
            map.insert(label.clone(), json!(limit));
        }
        Value::Object(map)
    };

    // The panel run's bridge identity (the writer of the members' reserved records and the task
    // lock): c3's own pid in production, the shim's pid under the harnesses (so the members'
    // `panel.parent_pid` is the launched process the harness monitors and kills).
    let (_parent_pid_bridge, parent_start) = crate::liveness::proc::bridge_identity();

    // The range dry-run line (a diff-review/acceptance panel).
    let (range_line, range_warning) = range_lines(o, r, &repo_root);

    Ok(Built {
        panel_id,
        roster_path: roster.path.clone(),
        collab_root,
        caller_cwd: cwd,
        task,
        codex_launcher: launcher,
        entry_count: selection.members.len(),
        seated_count: route.picked.len(),
        rows,
        runners,
        routing_value: routing_value(&route.routing),
        routing_rec: route.routing,
        concurrency,
        limits_value,
        warnings,
        coordinator_warnings,
        topics,
        range_line,
        range_warning,
        size_asked: route.size_asked,
        k: route.k,
        members_record,
        skipped_record,
        roles_note,
        parent_start,
        verb: if o.dry_run { "would run" } else { "run" },
        timeout_exceptions,
    })
}

/// A member's runtime state.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SlotState {
    Waiting,
    Running,
    Done,
    Killed,
    Blocked,
    FailedStart,
}

/// One seat's runtime record.
struct Slot {
    runner: usize,
    k: i64,
    n: i64,
    nn: u32,
    engine: String,
    reply_name: String,
    reply_rel: String,
    record_path: PathBuf,
    consult_id: String,
    group: usize,
    guard: f64,
    // runtime
    state: SlotState,
    child: Option<Child>,
    watch: Option<Instant>,
    exit: Option<i32>,
    wall: f64,
    out_path: PathBuf,
    err_path: PathBuf,
    lines: Vec<String>,
    refusal: String,
    entry: Option<c3_core::ledger::LedgerEntry>,
    record_state: String,
    record_kept: bool,
    not_started: String,
    start_error: String,
}

/// The scheduler proper: launch the members and print the whole panel block, dry or real.
fn schedule(o: Options, r: Resolved, b: Built) -> i32 {
    let dry = o.dry_run;
    let store = FilesStore::new(b.collab_root.clone());
    let task_dir = store.task_dir(&b.task);
    let short = &b.panel_id[..b.panel_id.len().min(8)];
    // The bridge pid the members record as their `panel.parent_pid` and re-check before launching:
    // c3's own pid in production, the shim's pid under the harnesses.
    let parent_pid = crate::liveness::proc::bridge_identity().0;
    let panel_tmp = std::env::temp_dir().join(format!("codex-consult-panel-{}", b.panel_id));

    // A real run creates the handoffs dir and takes the task lock (held to the end).
    let _task_lock = if dry {
        None
    } else {
        let _ = std::fs::create_dir_all(task_dir.join("handoffs"));
        let record = LockRecord::now(&b.task, Some(Value::String(b.panel_id.clone())));
        match store.take_task_lock(&b.task, &record) {
            Ok(l) => Some(l),
            Err(_) => {
                let lock_path = task_dir.join(".consult.lock");
                eprintln!(
                    "{TOOL}: {}",
                    crate::consult::orchestrate::format_task_lock_refusal(
                        &lock_path,
                        b.task.as_str()
                    )
                );
                return 1;
            }
        }
    };

    // Every recovery record of the task, judged before anything is written.
    let assessed = crate::consult::recovery::assess(&store, &b.task);
    if let Some(err) = assessed.error {
        eprintln!("{TOOL}: {err}");
        return 1;
    }
    let mut pending_refusal = String::new();
    if let Some(msg) = assessed.active_message() {
        if dry {
            pending_refusal = msg;
        } else {
            eprintln!("{TOOL}: {msg}");
            return 1;
        }
    }

    // The numbers of every member, past everything on disk.
    let nn_n = match store.next_numbers(&b.task) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("{TOOL}: {e}");
            return 1;
        }
    };

    // The findings every member is shown (open when the panel starts).
    let listed_ids: Vec<String> = if r.raw {
        Vec::new()
    } else {
        store
            .read_findings(&b.task)
            .ok()
            .flatten()
            .map(|f| {
                f.findings
                    .iter()
                    .filter(|fd| matches!(fd.status().as_str(), "proposed" | "implemented"))
                    .map(|fd| fd.id.clone())
                    .collect()
            })
            .unwrap_or_default()
    };

    // Build the slots (seat order).
    let mut slots: Vec<Slot> = Vec::new();
    let guard_hook: f64 = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_PANEL_GUARD_SEC")
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|v| *v > 0.0)
        .unwrap_or(0.0);
    let reply_base = if o.reply_name.is_empty() {
        "reply".to_string()
    } else {
        o.reply_name.clone()
    };
    for (k0, ru) in b.runners.iter().enumerate() {
        let k = k0 as i64 + 1;
        let n = nn_n.n + k0 as i64;
        let nn = nn_n.nn + k0 as u32;
        let slug = provider_slug(&ru.entry.provider);
        let prefix = c3_core::lineage::engine_spec(&ru.engine)
            .map(|s| s.prefix)
            .unwrap_or("codex");
        let reply_name = format!("{reply_base}-{slug}");
        let reply_rel = format!("handoffs/{nn:02}-{prefix}-{reply_name}.md");
        let denial = c3_core::lineage::engine_spec(&ru.engine)
            .map(|s| s.denial_retry)
            .unwrap_or(false)
            && o.denial_retry == 1;
        let guard = if guard_hook > 0.0 {
            guard_hook
        } else {
            panel_member_guard(r.timeout_sec, r.continue_sec, r.repair_enabled, denial)
        };
        slots.push(Slot {
            runner: k0,
            k,
            n,
            nn,
            engine: ru.engine.clone(),
            reply_name,
            reply_rel,
            record_path: task_dir.join(format!(".consult.pending-{nn:02}.json")),
            consult_id: uuid::Uuid::new_v4().to_string(),
            group: ru.group,
            guard,
            state: SlotState::Waiting,
            child: None,
            watch: None,
            exit: None,
            wall: 0.0,
            out_path: panel_tmp.join(format!("{nn:02}.out")),
            err_path: panel_tmp.join(format!("{nn:02}.err")),
            lines: Vec::new(),
            refusal: String::new(),
            entry: None,
            record_state: String::new(),
            record_kept: false,
            not_started: String::new(),
            start_error: String::new(),
        });
    }

    let panel_watch = Instant::now();

    // -------------------------------------------------------------------- the plan block
    println!(
        "Panel {short}{}: {} of {} roster entries {}, {} (roster {}; panel id {})",
        if dry {
            " (dry run - nothing is executed or written)"
        } else {
            ""
        },
        b.seated_count,
        b.entry_count,
        b.verb,
        b.concurrency.text,
        b.roster_path,
        b.panel_id
    );
    for row in &b.rows {
        let state_shown = if row.state == "run" {
            let s = &slots[row.runner.unwrap()];
            let role = if row.role.is_empty() {
                String::new()
            } else {
                format!(", role {}", row.role)
            };
            let required = if row.required { ", required" } else { "" };
            // (0.6.0) a light stand-in says so: ", stands in for #1 (...)"
            let reason = if row.reason.is_empty() {
                String::new()
            } else {
                format!(", {}", row.reason)
            };
            format!(
                "member, n={}, handoff {:02}{role}{required}{reason}",
                s.n, s.nn
            )
        } else if row.state == "not-picked" {
            format!("not picked: {}", row.reason)
        } else {
            format!("skipped: {}", row.reason)
        };
        println!("  #{} {} - {state_shown}", row.position, row.shown);
    }
    for line in routing_lines(&b.routing_rec, &o.purpose) {
        println!("{line}");
    }
    if !b.topics.is_empty() {
        println!("Topics: {}", b.topics.join(", "));
    }
    for w in &b.warnings {
        println!("WARNING: {w}");
    }
    // (wave 27, R13 D3) the coordinator-is-a-reviewer warnings for seated members (the dry run and
    // the real-run header alike); kept out of the members' panel_warnings.
    for w in &b.coordinator_warnings {
        println!("WARNING: {w}");
    }
    println!("{}", concurrency_line(&o, &b.concurrency));
    println!("{}", timeout_line(&r, &b.timeout_exceptions));
    if !b.range_line.is_empty() {
        println!("{}", b.range_line);
    }
    if !b.range_warning.is_empty() {
        println!("WARNING: {}", b.range_warning);
    }
    if dry && !pending_refusal.is_empty() {
        println!("pending     : the real run would be REFUSED - {pending_refusal}");
    }

    // -------------------------------------------------------------------- reserved records
    if !dry {
        let mut written: Vec<PathBuf> = Vec::new();
        for s in &slots {
            let ru = &b.runners[s.runner];
            let launcher_k = if s.engine == "codex" {
                b.codex_launcher.clone()
            } else {
                providers::resolve_engine_launcher(&s.engine, "")
                    .unwrap_or(None)
                    .unwrap_or_default()
            };
            let rec = reserved_record(&b, s, ru, launcher_k, parent_pid, short);
            let pref = PendingRef::member(b.task.clone(), s.nn);
            if let Err(e) = store.write_pending(&pref, &rec) {
                for w in &written {
                    let _ = std::fs::remove_file(w);
                }
                eprintln!("{TOOL}: could not write the recovery record '{}': {e}; no panel member was started.", s.record_path.display());
                return 1;
            }
            written.push(s.record_path.clone());
        }
        // Consume the other pending records (numbering skipped past them already).
        for item in &assessed.items {
            let is_member_own = slots.iter().any(|s| {
                s.record_path
                    .file_name()
                    .zip(item.path.file_name())
                    .map(|(a, c)| {
                        a.to_string_lossy()
                            .eq_ignore_ascii_case(&c.to_string_lossy())
                    })
                    .unwrap_or(false)
            });
            if !is_member_own {
                if let Err(e) = std::fs::remove_file(&item.path) {
                    println!(
                        "{TOOL}: could not remove the consumed recovery record {} ({e})",
                        item.path.display()
                    );
                }
            }
            println!("{TOOL}: {}", crate::consult::recovery::run_line(item));
        }
    } else {
        for item in &assessed.items {
            if !item.active {
                println!("pending     : {}", pending_dry_tail(item));
            }
        }
    }

    // -------------------------------------------------------------------- launch + poll
    let _ = std::fs::create_dir_all(&panel_tmp);
    let mut blocked = String::new();
    let mut required_failed = false;
    // (wave 26b, D13) the machine-wide health file (endpoint parallel limit); which seats have
    // already printed their "waits: N run(s) elsewhere" line (once each).
    let machine_health_path = c3_core::health::machine_health_path(&providers::get_codex_home());
    let mut ext_wait_shown: std::collections::HashSet<usize> = std::collections::HashSet::new();
    loop {
        let mut progress = false;
        // Reap finished/killed members.
        #[allow(clippy::needless_range_loop)]
        for i in 0..slots.len() {
            if slots[i].state != SlotState::Running {
                continue;
            }
            let exited = slots[i]
                .child
                .as_mut()
                .map(|c| matches!(c.try_wait(), Ok(Some(_)) | Err(_)))
                .unwrap_or(true);
            let elapsed = slots[i]
                .watch
                .map(|w| w.elapsed().as_secs_f64())
                .unwrap_or(0.0);
            if exited {
                let code = slots[i]
                    .child
                    .as_mut()
                    .and_then(|c| c.wait().ok())
                    .and_then(|st| st.code())
                    .unwrap_or(-1);
                slots[i].exit = Some(code);
                slots[i].wall = round1(elapsed);
                slots[i].state = SlotState::Done;
            } else if elapsed > slots[i].guard {
                if let Some(c) = slots[i].child.as_mut() {
                    kill_tree(c);
                }
                slots[i].exit = Some(-1);
                slots[i].wall = round1(elapsed);
                slots[i].state = SlotState::Killed;
            } else {
                continue;
            }
            progress = true;
            complete_member(&mut slots[i], &store, &b, dry);
            // A detached panel: record this member's final state as it finishes (D11).
            {
                let pos = b.runners[slots[i].runner].position;
                let (mstate, moutcome) = detach_member_state(&slots[i], dry);
                let n = slots[i].n;
                let nn = format!("{:02}", slots[i].nn);
                let wall = slots[i].wall;
                let reply = slots[i].reply_rel.clone();
                crate::consult::detach::update_member(pos, |m| {
                    m.state = mstate;
                    m.outcome = moutcome;
                    m.n = Some(n);
                    m.handoff = nn;
                    m.wall_seconds = Some(wall);
                    m.reply = reply;
                });
            }
            let shown = b.runners[slots[i].runner].shown.clone();
            let mut member_status = member_status(&slots[i], dry);
            let chars: Vec<char> = member_status.chars().collect();
            if chars.len() > 110 {
                member_status = chars[..110].iter().collect::<String>() + "...";
            }
            println!(
                "  panel member {} of {} finished: {shown} - {member_status} ({} s)",
                slots[i].k,
                b.seated_count,
                fmt_ps(slots[i].wall, 1)
            );
            // Required-member propagation.
            if !dry
                && b.runners[slots[i].runner].required
                && !required_failed
                && !slot_usable(&slots[i])
            {
                required_failed = true;
                if blocked.is_empty() {
                    blocked = format!("not started: the required member {shown} produced no usable reply - the panel stops (exit 5)");
                }
            }
            // F15-3: -PanelConcurrency 1 and a member that left survivors stops the rest.
            if !dry && o.panel_concurrency == 1 && blocked.is_empty() {
                let rd = crate::liveness::pending::read_pending_file(&slots[i].record_path);
                if let Some(err) = rd.error {
                    blocked = format!(
                        "not started: the previous member's recovery record cannot be used ({})",
                        c3_core::one_line(&err)
                    );
                } else if let Some(rec) = rd.record {
                    let chk =
                        crate::liveness::pending::test_pending_active(&rec, &slots[i].record_path);
                    if chk.active {
                        let file = slots[i]
                            .record_path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let st = rec.get("state").and_then(|v| v.as_str()).unwrap_or("");
                        blocked = format!("not started: the previous member ({shown}) left surviving processes ({file} state {st}); recover the task first");
                    }
                }
            }
        }
        // Start waiting members along the plan.
        let mut running = slots
            .iter()
            .filter(|s| s.state == SlotState::Running)
            .count() as i64;
        for i in 0..slots.len() {
            if slots[i].state != SlotState::Waiting {
                continue;
            }
            if !blocked.is_empty() {
                slots[i].state = SlotState::Blocked;
                slots[i].not_started = blocked.clone();
                progress = true;
                continue;
            }
            if o.panel_concurrency > 0 && running >= o.panel_concurrency {
                break;
            }
            let grp = slots[i].group;
            let limit = b.concurrency.groups.get(grp).map(|g| g.limit).unwrap_or(1);
            if slots
                .iter()
                .filter(|s| s.group == grp && s.state == SlotState::Running)
                .count() as i64
                >= limit
            {
                continue;
            }
            // (wave 26b, D13) runs on this group's endpoints in OTHER repositories/panels of the
            // machine count against the limit too (the machine-wide health file's running[]).
            if !dry {
                if let Some(hp) = &machine_health_path {
                    let fps: Vec<String> = {
                        let mut v: Vec<String> = b
                            .runners
                            .iter()
                            .filter(|ru| ru.group == grp && !ru.fingerprint.is_empty())
                            .map(|ru| ru.fingerprint.clone())
                            .collect();
                        v.sort();
                        v.dedup();
                        v
                    };
                    if !fps.is_empty() {
                        let ext = c3_core::health::machine_running_count(
                            hp,
                            &fps,
                            &b.panel_id,
                            &|pid, st| crate::liveness::proc::pid_alive(pid, st),
                        );
                        let local = slots
                            .iter()
                            .filter(|s| s.group == grp && s.state == SlotState::Running)
                            .count() as i64;
                        if !ext.is_empty() && local + ext.len() as i64 >= limit {
                            if !ext_wait_shown.contains(&i) {
                                ext_wait_shown.insert(i);
                                let who = ext
                                    .iter()
                                    .map(|row| {
                                        format!(
                                            "{} in {} task {} handoff {} (pid {})",
                                            row.label, row.repo, row.task, row.nn, row.pid
                                        )
                                    })
                                    .collect::<Vec<_>>()
                                    .join("; ");
                                println!(
                                    "  panel member {} of {} waits: {} run(s) elsewhere on this machine use its endpoint (parallel limit {limit}): {who}",
                                    slots[i].k,
                                    b.runners.len(),
                                    ext.len()
                                );
                            }
                            continue;
                        }
                    }
                }
            }
            // Roster order within an endpoint group.
            let my_k = slots[i].k;
            if slots
                .iter()
                .any(|s| s.group == grp && s.state == SlotState::Waiting && s.k < my_k)
            {
                continue;
            }
            start_member(
                &mut slots,
                i,
                &o,
                &r,
                &b,
                parent_pid,
                &panel_tmp,
                &listed_ids,
            );
            progress = true;
            match slots[i].state {
                SlotState::Running => running += 1,
                SlotState::FailedStart
                    if !dry && b.runners[slots[i].runner].required && !required_failed =>
                {
                    required_failed = true;
                    let shown = b.runners[slots[i].runner].shown.clone();
                    blocked = format!("not started: the required member {shown} could not be started - the panel stops (exit 5)");
                }
                _ => {}
            }
        }
        if slots
            .iter()
            .all(|s| s.state != SlotState::Waiting && s.state != SlotState::Running)
        {
            break;
        }
        if !progress {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }
    let panel_wall = round1(panel_watch.elapsed().as_secs_f64());

    // -------------------------------------------------------------------- console output
    for s in &slots {
        if s.state != SlotState::Done && s.state != SlotState::Killed {
            continue;
        }
        println!();
        println!(
            "=== panel {short} member {} of {}: {} (roster #{}) ===",
            s.k, b.seated_count, b.runners[s.runner].shown, b.runners[s.runner].position
        );
        for l in &s.lines {
            println!("{l}");
        }
    }

    // -------------------------------------------------------------------- unused records
    if !dry {
        for s in &mut slots {
            let rd = crate::liveness::pending::read_pending_file(&s.record_path);
            if rd.error.is_some() || rd.record.is_none() {
                continue;
            }
            let st = rd
                .record
                .as_ref()
                .and_then(|r| r.get("state"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let never = matches!(
                s.state,
                SlotState::Waiting | SlotState::Blocked | SlotState::FailedStart
            );
            if never || st == "reserved" {
                if let Err(e) = std::fs::remove_file(&s.record_path) {
                    s.record_kept = true;
                    println!(
                        "{TOOL}: could not remove the unused recovery record {} ({e})",
                        s.record_path.display()
                    );
                }
            } else {
                s.record_kept = true;
                s.record_state = st;
            }
        }
    }

    // -------------------------------------------------------------------- summary + counts
    let exit = render_summary(&o, &b, &slots, short, panel_wall, dry);

    // Patch panel.started / panel.usable in every member's ledger entry (a real run).
    if !dry {
        let started = slots
            .iter()
            .filter(|s| matches!(s.state, SlotState::Done | SlotState::Killed))
            .count() as i64;
        let usable = slots.iter().filter(|s| slot_usable(s)).count() as i64;
        patch_counts(&store, &b.task, &b.panel_id, started, usable);
    }

    // The task lock releases on drop; the temp dir is cleaned up.
    if panel_tmp.exists() {
        let _ = std::fs::remove_dir_all(&panel_tmp);
    }
    exit
}

/// One summary-table row (`$rows`).
struct SummaryRow {
    lineage: String,
    status: String,
    counts: String,
    prior: String,
    tail: String,
    /// A wide row spans the columns (a skip / failure / not-started), not the padded grid.
    wide: bool,
}

/// The summary table's member rows, laid out exactly like the plugin (`codex-consult.ps1:2838`):
/// column widths come from the NARROW rows; a wide row pads its status only when `skipped`; each
/// row is `"  " + fields.join("  ").TrimEnd()`.
fn format_summary_rows(rows: &[SummaryRow]) -> Vec<String> {
    let w_lineage = rows
        .iter()
        .map(|r| r.lineage.chars().count())
        .max()
        .unwrap_or(0);
    let narrow: Vec<&SummaryRow> = rows.iter().filter(|r| !r.wide).collect();
    let (mut w_status, mut w_counts, mut w_prior) = (7usize, 0usize, 0usize);
    if !narrow.is_empty() {
        w_status = narrow
            .iter()
            .map(|r| r.status.chars().count())
            .max()
            .unwrap_or(0)
            .max(7);
        w_counts = narrow
            .iter()
            .map(|r| r.counts.chars().count())
            .max()
            .unwrap_or(0);
        w_prior = narrow
            .iter()
            .map(|r| r.prior.chars().count())
            .max()
            .unwrap_or(0);
    }
    let mut out: Vec<String> = Vec::new();
    for row in rows {
        let mut parts: Vec<String> = vec![pad_right(&row.lineage, w_lineage)];
        if row.wide {
            let w = if row.status == "skipped" { w_status } else { 0 };
            parts.push(pad_right(&row.status, w));
            if !row.counts.is_empty() {
                parts.push(row.counts.clone());
            }
        } else {
            parts.push(pad_right(&row.status, w_status));
            if w_counts > 0 {
                parts.push(pad_right(&row.counts, w_counts));
            }
            if w_prior > 0 {
                parts.push(pad_right(&row.prior, w_prior));
            }
        }
        if !row.tail.is_empty() {
            parts.push(row.tail.clone());
        }
        out.push(format!("  {}", parts.join("  ").trim_end()));
    }
    out
}

/// Render the summary block and return the exit code.
fn render_summary(
    _o: &Options,
    b: &Built,
    slots: &[Slot],
    short: &str,
    panel_wall: f64,
    dry: bool,
) -> i32 {
    let mut rows: Vec<SummaryRow> = Vec::new();
    let mut all_usable = b.seated_count > 0;
    let started = slots
        .iter()
        .filter(|s| matches!(s.state, SlotState::Done | SlotState::Killed))
        .count();
    let usable_count = slots.iter().filter(|s| slot_usable(s)).count();

    for dr in b.rows.iter().filter(|r| r.state != "not-picked") {
        let mut row = SummaryRow {
            lineage: dr.shown.clone(),
            status: String::new(),
            counts: String::new(),
            prior: String::new(),
            tail: String::new(),
            wide: false,
        };
        if dr.state != "run" {
            row.status = "skipped".into();
            row.counts = dr.reason.clone();
            row.wide = true;
        } else {
            let s = &slots[dr.runner.unwrap()];
            if matches!(s.state, SlotState::Waiting | SlotState::Blocked) {
                row.status = "skipped".into();
                row.counts = if s.not_started.is_empty() {
                    "not started".into()
                } else {
                    s.not_started.clone()
                };
                row.wide = true;
                all_usable = false;
            } else if dry && s.state == SlotState::Done {
                if s.exit == Some(0) {
                    row.status = "planned".into();
                } else {
                    row.status = format!("refused: {}", c3_core::one_line(&s.refusal));
                    row.wide = true;
                    all_usable = false;
                }
            } else if let Some(me) = s.entry.as_ref() {
                let outcome = me.bridge_outcome.clone();
                let mut tail = format!("{} s  {}", me.wall_seconds, me.reply);
                if outcome != "usable reply" && c3_core::health::is_usable_outcome(&outcome) {
                    tail += "  (after a timeout continuation)";
                }
                if !me.partial_reply.is_empty() {
                    tail += &format!("  partial {}", me.partial_reply);
                }
                if s.state == SlotState::Killed {
                    row.status = member_status(s, dry);
                    row.tail = tail;
                    row.wide = true;
                    all_usable = false;
                } else if !c3_core::health::is_usable_outcome(&outcome) {
                    let short_o = outcome.strip_prefix("failed: ").unwrap_or(&outcome);
                    let chars: Vec<char> = short_o.chars().collect();
                    let short_o = if chars.len() > 90 {
                        chars[..90].iter().collect::<String>() + "..."
                    } else {
                        short_o.to_string()
                    };
                    row.status = format!("failed: {short_o}");
                    row.tail = tail;
                    row.wide = true;
                    all_usable = false;
                } else if !me.verdict.is_empty() && me.structured {
                    row.status = me.verdict.clone();
                    row.counts = format!(
                        "{} blocker, {} major, {} minor",
                        me.findings.blocker, me.findings.major, me.findings.minor
                    );
                    if me.findings.note > 0 {
                        row.counts += &format!(", {} note", me.findings.note);
                    }
                    row.prior = format_prior_counts(&me.prior_findings);
                    row.tail = tail;
                } else {
                    row.status = "prose (no verdict)".into();
                    row.tail = tail;
                }
            } else {
                // No committed ledger entry (refused/failed before its commit).
                row.status = member_status(s, dry);
                if !dry && !s.record_kept {
                    row.status += &format!(" (n={} and handoff {:02} stay unused)", s.n, s.nn);
                } else if !dry {
                    let file = s
                        .record_path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    row.status += &format!(" ({file} kept, state {})", s.record_state);
                }
                row.wide = true;
                all_usable = false;
            }
        }
        rows.push(row);
    }

    println!();
    let head = if dry {
        format!(
            "{} of {} entries would run (dry run) (wall clock {} s; {})",
            b.seated_count,
            b.entry_count,
            fmt_ps(panel_wall, 1),
            b.concurrency.text
        )
    } else {
        format!(
            "{} of {} entries ran (asked {}, started {}, usable {}; wall clock {} s; {})",
            started,
            b.entry_count,
            b.size_asked,
            started,
            usable_count,
            fmt_ps(panel_wall, 1),
            b.concurrency.text
        )
    };
    // Collect the block (from the `Panel <id8>:` line down) so a detached panel keeps it verbatim
    // in its status file (the same slice the harness's `PanelSummary` takes).
    let mut block: Vec<String> = Vec::new();
    block.push(format!("Panel {short}: {head}"));
    for line in format_summary_rows(&rows) {
        block.push(line);
    }

    let not_picked: Vec<&DisplayRow> = b.rows.iter().filter(|r| r.state == "not-picked").collect();
    if !not_picked.is_empty() {
        let names: Vec<String> = not_picked.iter().map(|r| r.shown.clone()).collect();
        block.push(format!(
            "  not picked (panel size {}): {}",
            b.k,
            names.join(", ")
        ));
    }

    let mut exit = if all_usable { 0 } else { 1 };
    if !dry {
        let missing: Vec<String> = slots
            .iter()
            .filter(|s| b.runners[s.runner].required && !slot_usable(s))
            .map(|s| b.runners[s.runner].shown.clone())
            .collect();
        if !missing.is_empty() {
            block.push(format!(
                "  required member{} without a usable reply: {} - exit 5",
                if missing.len() != 1 { "s" } else { "" },
                missing.join(", ")
            ));
            exit = 5;
        }
    }
    for l in &block {
        println!("{l}");
    }
    crate::consult::detach::note_summary(&block);
    exit
}

/// PowerShell `PadRight`: pad with spaces to `width` chars (no truncation).
fn pad_right(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len >= width {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(width - len))
    }
}

/// `Format-PriorCounts`: `prior: none` or `prior: <n fixed>, <n still-open>, ...`.
fn format_prior_counts(prior: &[c3_core::ledger::PriorFindingRef]) -> String {
    let prior: Vec<&c3_core::ledger::PriorFindingRef> = prior
        .iter()
        .filter(|p| !p.id.is_empty() || !p.status.is_empty())
        .collect();
    if prior.is_empty() {
        return "prior: none".into();
    }
    let mut parts: Vec<String> = Vec::new();
    for st in ["fixed", "still-open", "not-checked", "unknown-id"] {
        let n = prior.iter().filter(|p| p.status == st).count();
        if n > 0 {
            parts.push(format!("{n} {st}"));
        }
    }
    format!("prior: {}", parts.join(", "))
}

/// `Get-PanelMemberStatus`: one phrase for a member that ran/stopped.
fn member_status(s: &Slot, dry: bool) -> String {
    match s.state {
        SlotState::Killed => format!(
            "killed by the panel after {} s (its guard: {} s)",
            fmt_ps(s.wall, 1),
            fmt_ps(s.guard, 1)
        ),
        SlotState::FailedStart => format!(
            "failed: could not start the member process - {}",
            s.start_error
        ),
        _ => {
            if dry {
                if s.exit == Some(0) {
                    return "planned".into();
                }
                return format!("refused: {}", c3_core::one_line(&s.refusal));
            }
            if let Some(e) = &s.entry {
                return e.bridge_outcome.clone();
            }
            if s.record_state == "committing" {
                if let Some(rest) = s.refusal.strip_prefix("commit blocked:") {
                    return format!("commit blocked: {}", c3_core::one_line(rest.trim_start()));
                }
                return format!(
                    "failed: stopped inside its commit (exit {}); findings it wrote have no ledger entry (ORPHAN)",
                    s.exit.unwrap_or(-1)
                );
            }
            format!("failed: {}", c3_core::one_line(&s.refusal))
        }
    }
}

/// The detached-run member state + outcome for a finished slot (D11).
fn detach_member_state(s: &Slot, dry: bool) -> (String, String) {
    if s.state == SlotState::Killed {
        return ("killed".into(), member_status(s, dry));
    }
    if slot_usable(s) {
        return ("usable".into(), member_status(s, dry));
    }
    ("failed".into(), member_status(s, dry))
}

/// `Test-PanelSlotUsable`.
fn slot_usable(s: &Slot) -> bool {
    if s.state != SlotState::Done {
        return false;
    }
    s.entry
        .as_ref()
        .map(|e| c3_core::health::is_usable_outcome(&e.bridge_outcome))
        .unwrap_or(false)
}

/// `Complete-PanelMember`: the member's console lines, its refusal, its ledger entry and record.
fn complete_member(s: &mut Slot, store: &FilesStore, b: &Built, dry: bool) {
    let mut lines: Vec<String> = Vec::new();
    for f in [&s.out_path, &s.err_path] {
        let t = std::fs::read(f)
            .map(|b| String::from_utf8_lossy(&b).to_string())
            .unwrap_or_default();
        if t.trim().is_empty() {
            continue;
        }
        for l in t.trim_end().split('\n') {
            lines.push(l.trim_end_matches('\r').to_string());
        }
    }
    s.refusal = lines
        .iter()
        .rev()
        .find(|l| l.starts_with("codex-consult: "))
        .map(|l| l.trim_start_matches("codex-consult: ").to_string())
        .unwrap_or_else(|| format!("exit {}", s.exit.unwrap_or(-1)));
    s.lines = lines;
    if !dry {
        s.entry = find_panel_entry(store, &b.task, &b.panel_id, s.k);
        let rd = crate::liveness::pending::read_pending_file(&s.record_path);
        s.record_state = rd
            .record
            .as_ref()
            .and_then(|r| r.get("state"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
    }
}

/// `Find-PanelEntry`: the ledger entry a member recorded (by panel id + seat position).
fn find_panel_entry(
    store: &FilesStore,
    task: &TaskSlug,
    panel_id: &str,
    position: i64,
) -> Option<c3_core::ledger::LedgerEntry> {
    let sessions = store.read_sessions(task).ok().flatten()?;
    sessions.codex.consults.into_iter().rfind(|c| {
        c.panel
            .as_ref()
            .map(|p| p.id == panel_id && p.position == position)
            .unwrap_or(false)
    })
}

/// Launch one member as a child `c3 consult --task <t> --panel-spec <b64>` process.
#[allow(clippy::too_many_arguments)]
fn start_member(
    slots: &mut [Slot],
    i: usize,
    o: &Options,
    r: &Resolved,
    b: &Built,
    parent_pid: u32,
    panel_tmp: &Path,
    listed: &[String],
) {
    let spec = build_spec(&slots[..], i, o, r, b, parent_pid, listed);
    let wire = match spec.to_wire() {
        Ok(w) => w,
        Err(e) => {
            slots[i].state = SlotState::FailedStart;
            slots[i].start_error = c3_core::one_line(&e.to_string());
            return;
        }
    };
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            slots[i].state = SlotState::FailedStart;
            slots[i].start_error = c3_core::one_line(&e.to_string());
            return;
        }
    };
    let out = std::fs::File::create(&slots[i].out_path);
    let err = std::fs::File::create(&slots[i].err_path);
    let (out, err) = match (out, err) {
        (Ok(o), Ok(e)) => (o, e),
        _ => {
            slots[i].state = SlotState::FailedStart;
            slots[i].start_error = format!(
                "could not create the member output files under {}",
                panel_tmp.display()
            );
            return;
        }
    };
    let mut cmd = Command::new(&exe);
    cmd.arg("consult")
        .arg("--task")
        .arg(&o.task)
        .arg("--panel-spec")
        .arg(&wire)
        .current_dir(&b.caller_cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err));
    // The panel run is the member's bridge (its `panel.parent_pid`), launched directly here (no
    // shim), so the member must NOT inherit the harness shim's bridge pid: clear it so the member
    // records its OWN pid as the writer and starts no fate-sharing watchdog of its own.
    cmd.env_remove("CODEX_CONSULT_TEST_BRIDGE_PID");
    match cmd.spawn() {
        Ok(child) => {
            slots[i].child = Some(child);
            slots[i].watch = Some(Instant::now());
            slots[i].state = SlotState::Running;
            // A detached panel keeps each member's state in the status file (D11).
            let pos = b.runners[slots[i].runner].position;
            let n = slots[i].n;
            let nn = format!("{:02}", slots[i].nn);
            crate::consult::detach::update_member(pos, |m| {
                m.state = "running".into();
                m.n = Some(n);
                m.handoff = nn;
            });
        }
        Err(e) => {
            slots[i].state = SlotState::FailedStart;
            slots[i].start_error = c3_core::one_line(&e.to_string());
        }
    }
}

/// The `MemberSpec` for seat `i` (`Start-PanelMember`'s `$spec`).
#[allow(clippy::too_many_arguments)]
fn build_spec(
    slots: &[Slot],
    i: usize,
    o: &Options,
    r: &Resolved,
    b: &Built,
    parent_pid: u32,
    listed: &[String],
) -> MemberSpec {
    let s = &slots[i];
    let ru = &b.runners[s.runner];
    let siblings: Vec<i64> = if b.concurrency.effective > 1 {
        slots
            .iter()
            .filter(|x| x.k != s.k)
            .map(|x| x.nn as i64)
            .collect()
    } else {
        Vec::new()
    };
    let args = json!({
        "collab_dir": o.collab_dir,
        "mode": o.mode,
        "brief": o.brief,
        "prompt": o.prompt,
        "purpose": o.purpose,
        "effort": o.effort,
        "sandbox": o.sandbox,
        "max_words": o.max_words,
        // (wave 26b, D11/D12) forward timeout/continue/stall to the member ONLY when they were
        // explicit; otherwise the member re-derives the purpose default and its own roster entry's
        // timeout_sec/stall_sec (so a per-member roster override records timeout_source "roster").
        "timeout_sec": if r.timeout_source == "explicit" { r.timeout_sec } else { 0 },
        "continue_sec": if o.continue_sec_given { r.continue_sec } else { -1 },
        "stall_sec": if r.stall_given { r.stall_sec } else { -1 },
        "range": o.range,
        "reply_name": s.reply_name,
        "artifact": o.artifacts,
        // (M11) `--peer`/`--peers` reach a reviewer only through a pack, which only the http engine
        // builds — so a panel forwards them to its http members only; other members get none.
        "peer": if ru.entry.engine == "http" { o.peer.clone() } else { Vec::new() },
        "peers": if ru.entry.engine == "http" { o.peers.clone() } else { None },
        "raw": r.raw,
        "codex_exe": o.codex_exe,
        "native_effort": o.native_effort,
        "off_peak_only": o.off_peak_only,
        "skip_preflight": o.skip_preflight,
        "codex_config": o.codex_config,
        "schema_transport": r.transport_override,
        "format_retry": o.format_retry,
        "engine": o.engine,
        "engine_exe": o.engine_exe,
        "denial_retry": o.denial_retry,
        "max_model_steps": o.max_model_steps,
        "topics": b.topics,
        "dry_run": o.dry_run,
    });
    MemberSpec {
        id: b.panel_id.clone(),
        position: s.k,
        of: b.seated_count as i64,
        members: b.members_record.clone(),
        roster_position: ru.entry.position as i64,
        provider: ru.entry.provider.clone(),
        model: ru.entry.model.clone(),
        engine: ru.entry.engine.clone(),
        skipped: b.skipped_record.clone(),
        listed_ids: listed.to_vec(),
        n: s.n,
        nn: s.nn as i64,
        consult_id: s.consult_id.clone(),
        parent_pid: parent_pid as i64,
        parent_start_time: b.parent_start.clone(),
        sibling_nns: siblings,
        concurrency: b.concurrency.effective,
        limits: b.limits_value.clone(),
        asked: b.size_asked,
        routing: Some(b.routing_value.clone()),
        role: ru.role.clone(),
        roles_note: b.roles_note.clone(),
        panel_warnings: b.warnings.clone(),
        args,
    }
}

/// A seat's reserved recovery record (`New-PendingRecord -State reserved ... -Panel ...`).
fn reserved_record(
    b: &Built,
    s: &Slot,
    _ru: &RunnerInfo,
    launcher: String,
    parent_pid: u32,
    short: &str,
) -> PendingRecord {
    PendingRecord {
        state: PendingState::Reserved,
        n: s.n,
        nn: format!("{:02}", s.nn),
        reply: s.reply_rel.clone(),
        consult_id: s.consult_id.clone(),
        started: iso_now(),
        pid: parent_pid,
        start_time: b.parent_start.clone(),
        host: pending_host(),
        launcher,
        engine: s.engine.clone(),
        note: format!(
            "reserved by review panel {short} (pid {parent_pid}); the member has not started"
        ),
        panel: Some(json!({
            "id": b.panel_id,
            "position": s.k,
            "of": b.seated_count,
            "parent_pid": parent_pid,
            "parent_start_time": b.parent_start,
        })),
        ..Default::default()
    }
}

/// Patch `panel.started`/`panel.usable` in every member's committed entry after the panel ends.
fn patch_counts(store: &FilesStore, task: &TaskSlug, panel_id: &str, started: i64, usable: i64) {
    let lock = match store.take_write_lock(task) {
        Ok(l) => l,
        Err(e) => {
            println!(
                "{TOOL}: the panel's counts were not written to the ledger ({})",
                c3_core::one_line(&e.to_string())
            );
            return;
        }
    };
    let mut sessions = match store.read_sessions(task) {
        Ok(Some(s)) => s,
        _ => {
            drop(lock);
            return;
        }
    };
    let mut changed = false;
    for e in &mut sessions.codex.consults {
        if let Some(p) = &mut e.panel {
            if p.id == panel_id {
                p.started = Some(Some(started));
                p.usable = Some(Some(usable));
                changed = true;
            }
        }
    }
    if changed {
        if let Ok(bytes) = sessions.to_bytes() {
            let _ = c3_core::store::write_text_atomic(
                &store.task_dir(task).join("sessions.json"),
                &bytes,
            );
        }
    }
    drop(lock);
}

/// `Get-PanelMemberGuard`: timeout + 60 + 120, plus one repair turn, one denial-retry turn
/// (min(timeout, 300) each) and the continuation budget.
fn panel_member_guard(timeout: i64, continue_sec: i64, repair: bool, denial: bool) -> f64 {
    let mut g = timeout + 60 + 120;
    if repair {
        g += timeout.min(300);
    }
    if denial {
        g += timeout.min(300);
    }
    if continue_sec > 0 {
        g += continue_sec;
    }
    g as f64
}

/// Kill a member's process tree (`Stop-ProcessTree`): `taskkill /F /T` on Windows, else the
/// descendants from the process table (best effort) and then the member itself.
fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let pid = child.id();
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(windows))]
    {
        for pid in crate::liveness::proc::descendants_of(child.id()) {
            let _ = Command::new("kill")
                .args(["-9", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// The dry-run `pending :` tail for an inactive record (`the real run recovers it`).
fn pending_dry_tail(item: &crate::consult::recovery::RecoveryItem) -> String {
    let n = if item.n > 0 {
        item.n.to_string()
    } else {
        String::new()
    };
    format!(
        "{} (state '{}', n={n}, nn={}; {}): the real run recovers it; numbering continues past it.",
        item.path.display(),
        item.state,
        item.nn,
        item.check
    )
}

/// The routing record as the member's ledger `panel.routing` value (`Select-PanelRouting`'s
/// object: mode, order, fallback, seed, nonce, nonce_source, size, size_asked, size_source,
/// reserve, eligible[], picked[], explored[], required[]).
fn routing_value(rt: &RoutingRecord) -> Value {
    let mut v = routing_value_plugin(rt);
    // (C3, M9) ONE trailing key, an object, only when the router has something to say; the
    // plugin's own keys stay untouched and in their order.
    if let Some(ext) = &rt.ext {
        v["ext"] = ext.clone();
    }
    v
}

/// The plugin's keys of `panel.routing`, in the plugin's order.
fn routing_value_plugin(rt: &RoutingRecord) -> Value {
    json!({
        "mode": rt.mode,
        "order": rt.order,
        "fallback": rt.fallback,
        "seed": rt.seed,
        "nonce": rt.nonce,
        "nonce_source": rt.nonce_source,
        "size": rt.size,
        "size_asked": rt.size_asked,
        "size_source": rt.size_source,
        "reserve": rt.reserve,
        "eligible": rt.eligible.iter().map(|e| json!({
            "position": e.position, "lineage": e.lineage, "lab": e.lab, "lab_source": e.lab_source,
            "score": e.score, "basis": e.basis, "ratings": e.ratings, "required": e.required,
        })).collect::<Vec<_>>(),
        "picked": rt.picked.iter().map(|p| json!({
            "slot": p.slot, "position": p.position, "lineage": p.lineage, "lab": p.lab, "rule": p.rule,
        })).collect::<Vec<_>>(),
        "explored": rt.explored,
        "required": rt.required,
    })
}

/// A provider label as a handoff slug (`$slug`): lowercase, non-`[a-z0-9._-]` → `-`, trimmed;
/// empty → `reviewer`.
fn provider_slug(provider: &str) -> String {
    let mut s: String = provider
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect();
    while s.starts_with('-') {
        s.remove(0);
    }
    while s.ends_with('-') {
        s.pop();
    }
    if s.is_empty() {
        "reviewer".into()
    } else {
        s
    }
}

fn iso_now() -> String {
    chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string()
}

fn pending_host() -> String {
    c3_core::host::machine_name()
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// The panel draw nonce and its source (`-PanelSeed`, else the env hook, else today's UTC date).
fn panel_nonce(o: &Options, utc_now: chrono::DateTime<chrono::Utc>) -> (String, &'static str) {
    let seed = o.panel_seed.trim();
    if !seed.is_empty() {
        return (seed.to_string(), "-PanelSeed");
    }
    if let Some(env) = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_PANEL_SEED") {
        let env = env.trim().to_string();
        if !env.is_empty() {
            return (env, "CODEX_CONSULT_TEST_PANEL_SEED");
        }
    }
    (utc_now.format("%Y-%m-%d").to_string(), "date")
}

/// The brief's sha256 for the seed (`Get-FileSha256OrMissing`); `""` with no brief.
fn brief_sha256(o: &Options, cwd: &Path) -> String {
    if o.brief.is_empty() {
        return String::new();
    }
    let p = if Path::new(&o.brief).is_absolute() {
        PathBuf::from(&o.brief)
    } else {
        cwd.join(&o.brief)
    };
    std::fs::read(&p)
        .map(|b| c3_core::sha256_hex(&b))
        .unwrap_or_else(|_| "missing".into())
}

/// The `Range:` dry-run line + a size warning (a diff-review/acceptance panel).
fn range_lines(o: &Options, r: &Resolved, repo_root: &Path) -> (String, String) {
    if o.range.is_empty() {
        return (String::new(), String::new());
    }
    let rs = crate::consult::revision::range_stat(repo_root, &o.range);
    if !rs.error.is_empty() {
        return (String::new(), String::new());
    }
    let text = crate::consult::revision::range_text(rs.files, rs.lines);
    let line = format!(
        "Range: {} - {text} ({} insertions, {} deletions)",
        o.range, rs.insertions, rs.deletions
    );
    let warning = if rs.lines > 1500 && r.timeout_sec < 2400 {
        format!(
            "a range of {} lines with a {} s timeout: pass -TimeoutSec or a reading plan in the brief",
            rs.lines, r.timeout_sec
        )
    } else {
        String::new()
    };
    (line, warning)
}

/// `Format-RoutingLines`.
fn routing_lines(rt: &RoutingRecord, purpose: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let purpose_label = if purpose.is_empty() { "none" } else { purpose };
    let size_text = format!(
        "size {} ({})",
        rt.size,
        if rt.size_source == "purpose" {
            format!("the default of purpose {purpose_label}")
        } else if rt.size_source == "required" {
            // (wave 26c) the required reviewers raised the size above the asked size.
            format!("required; asked {}", rt.size_asked)
        } else {
            rt.size_source.clone()
        }
    );
    if rt.mode == "routed" {
        let seed = &rt.seed[..rt.seed.len().min(12)];
        lines.push(format!(
            "Routing: routed by the ratings - {size_text}; seed {seed} (nonce {} from {}); exploration 0.2 per slot",
            rt.nonce, rt.nonce_source
        ));
    } else {
        let why = if !rt.fallback.is_empty() {
            format!(
                "fallback: {} - no eligible reviewer has 3 ratings in 90 days",
                rt.fallback
            )
        } else {
            "-PanelOrder roster".to_string()
        };
        lines.push(format!("Routing: roster order ({why}) - {size_text}"));
    }
    let el: Vec<String> = rt
        .eligible
        .iter()
        .map(|e| {
            let lab_by = if e.lab_source != "roster" {
                format!(" by {}", e.lab_source)
            } else {
                String::new()
            };
            let of = if e.ratings > 0.0 {
                format!(" of {}", fmt_ps(e.ratings, 2))
            } else {
                String::new()
            };
            let req = if e.required { "; required" } else { "" };
            format!(
                "#{} {} (lab {}{lab_by}; score {} {}{of}{req})",
                e.position,
                e.lineage,
                e.lab,
                fmt_ps(e.score, 3),
                e.basis
            )
        })
        .collect();
    lines.push(format!(
        "  eligible: {}",
        if el.is_empty() {
            "(none)".to_string()
        } else {
            el.join(", ")
        }
    ));
    let pk: Vec<String> = rt
        .picked
        .iter()
        .map(|p| format!("{}. #{} {} ({})", p.slot, p.position, p.lineage, p.rule))
        .collect();
    lines.push(format!(
        "  picked  : {}",
        if pk.is_empty() {
            "(none)".to_string()
        } else {
            pk.join(", ")
        }
    ));
    if !rt.required.is_empty() {
        lines.push(format!("  required: {}", rt.required.join(", ")));
    }
    lines
}

/// The `Concurrency:` line.
fn concurrency_line(o: &Options, plan: &PanelPlan) -> String {
    let group_texts: Vec<String> = plan
        .groups
        .iter()
        .map(|g| {
            let cnt = g.positions.len();
            let mut t = format!("{} x{cnt}", g.labels.join("+"));
            if cnt > 1 {
                if g.limit >= cnt as i64 {
                    t.push_str(" at once");
                } else if g.limit <= 1 {
                    t.push_str(" one after another");
                } else {
                    t.push_str(&format!(" {} at a time", g.limit));
                }
            }
            t
        })
        .collect();
    let cap = if o.panel_concurrency > 0 {
        o.panel_concurrency.to_string()
    } else {
        "0 (no cap)".to_string()
    };
    format!(
        "Concurrency: {} - endpoint groups: {}; -PanelConcurrency {cap}",
        plan.text,
        group_texts.join(", ")
    )
}

/// The `Timeout:` line.
fn timeout_line(r: &Resolved, exceptions: &[String]) -> String {
    let source = if r.timeout_source == "purpose" {
        let label = if r.purpose_label == "none" {
            "none".to_string()
        } else {
            r.purpose_label.clone()
        };
        format!("the default of purpose {label}")
    } else {
        "-TimeoutSec".to_string()
    };
    let cont = if r.continue_sec > 0 {
        format!("up to {} s on the same thread", r.continue_sec)
    } else {
        "off (-ContinueSec 0)".to_string()
    };
    // (wave 26b, D11) the members whose roster entry overrode the shared default.
    let exc = if exceptions.is_empty() {
        String::new()
    } else {
        format!("; {}", exceptions.join("; "))
    };
    format!(
        "Timeout: {} s per member ({source}){exc}; continuation after a timeout kill: {cont}",
        r.timeout_sec
    )
}

/// Format a number as PowerShell's `ToString('0.###')` would: fixed to `decimals`, trailing
/// zeros and a trailing dot trimmed.
fn fmt_ps(x: f64, decimals: usize) -> String {
    let s = format!("{:.1$}", x, decimals);
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_right_pads_and_never_truncates() {
        assert_eq!(pad_right("ab", 5), "ab   ");
        assert_eq!(pad_right("abcdef", 3), "abcdef");
    }

    #[test]
    fn provider_slug_sanitizes() {
        assert_eq!(provider_slug("ZAI"), "zai");
        assert_eq!(provider_slug("byteplus"), "byteplus");
        assert_eq!(provider_slug("!!!"), "reviewer");
    }

    #[test]
    fn guard_sums_the_budgets() {
        // 1800 + 60 + 120 + min(1800,300) repair + min(1800,300) denial + 900 continue.
        assert_eq!(
            panel_member_guard(1800, 900, true, true),
            (1800 + 60 + 120 + 300 + 300 + 900) as f64
        );
        // no repair / denial / continue.
        assert_eq!(
            panel_member_guard(600, 0, false, false),
            (600 + 60 + 120) as f64
        );
    }

    #[test]
    fn prior_counts_none_and_some() {
        assert_eq!(format_prior_counts(&[]), "prior: none");
        let prior = vec![
            c3_core::ledger::PriorFindingRef {
                id: "F01-1".into(),
                status: "fixed".into(),
                ..Default::default()
            },
            c3_core::ledger::PriorFindingRef {
                id: "F01-2".into(),
                status: "still-open".into(),
                ..Default::default()
            },
        ];
        assert_eq!(format_prior_counts(&prior), "prior: 1 fixed, 1 still-open");
    }

    #[test]
    fn fmt_ps_trims_trailing_zeros() {
        assert_eq!(fmt_ps(277.0, 1), "277");
        assert_eq!(fmt_ps(243.4, 1), "243.4");
        assert_eq!(fmt_ps(1.125, 3), "1.125");
    }

    fn row(
        lineage: &str,
        status: &str,
        counts: &str,
        prior: &str,
        tail: &str,
        wide: bool,
    ) -> SummaryRow {
        SummaryRow {
            lineage: lineage.into(),
            status: status.into(),
            counts: counts.into(),
            prior: prior.into(),
            tail: tail.into(),
            wide,
        }
    }

    #[test]
    fn summary_rows_match_the_panel_01_layout() {
        // The ten member rows of the real panel-01.log (scratchpad), fed post-truncation.
        let rows = vec![
            row("ZAI :: glm-5.3", "ADVISE", "0 blocker, 4 major, 2 minor, 1 note", "prior: none", "243.4 s  handoffs/02-codex-merge-framing-zai.md", false),
            row("mimo :: mimo-v2.6-pro", "prose (no verdict)", "", "", "405.7 s  handoffs/03-codex-merge-framing-mimo.md", false),
            row("gemini :: gemini-3.8-flash-high [agy]", "failed: agy exit 3 - Individual quota reached. Please upgrade your subscription to increase your l...", "", "", "297.6 s  handoffs/04-agy-merge-framing-gemini.md", true),
            row("gemini :: gemini-3.1-pro-high [agy]", "failed: provider gemini is not usable: its usage limit (hit at 2026-09-26T17:47:18+02:00: Individual quota reached. Please upgrade your subscription to increase your limits. Resets in 51h38m) lasts until 2026-09-28T21:30:50+02:00; nothing was started (pass -SkipPreflight to launch anyway) (n=4 and handoff 05 stay unused)", "", "", "", true),
            row("byteplus :: deepseek-v4.1-flash", "failed: codex exit 1 - exceeded retry limit, last status: 429 Too Many Requests, request id: 02179...", "", "", "34.7 s  handoffs/06-codex-merge-framing-byteplus.md", true),
            row("byteplus :: dola-seed-2.0-pro", "failed: codex exit 1 - exceeded retry limit, last status: 429 Too Many Requests, request id: 02179...", "", "", "44.3 s  handoffs/07-codex-merge-framing-byteplus.md", true),
            row("byteplus :: kimi-k2.5", "failed: codex exit 1 - exceeded retry limit, last status: 429 Too Many Requests, request id: 02179...", "", "", "56.9 s  handoffs/08-codex-merge-framing-byteplus.md", true),
            row("kimi :: k3", "ADVISE", "0 blocker, 3 major, 2 minor, 1 note", "prior: none", "277 s  handoffs/09-codex-merge-framing-kimi.md", false),
            row("alibaba :: qwen3.8-max", "ADVISE", "2 blocker, 6 major, 2 minor, 1 note", "prior: none", "763.1 s  handoffs/10-codex-merge-framing-alibaba.md", false),
            row("meta :: muse-spark-1.3-contributor [muse]", "ADVISE", "1 blocker, 5 major, 0 minor", "prior: none", "103.6 s  handoffs/11-muse-merge-framing-meta.md", false),
        ];
        let out = format_summary_rows(&rows);
        // w_lineage = 41 ("meta :: muse-spark-1.3-contributor [muse]"); w_status = 18
        // ("prose (no verdict)"); w_counts = 35; w_prior = 11 (all from the narrow rows).
        let expected0 = format!(
            "  ZAI :: glm-5.3{}  ADVISE{}  0 blocker, 4 major, 2 minor, 1 note  prior: none  243.4 s  handoffs/02-codex-merge-framing-zai.md",
            " ".repeat(27),
            " ".repeat(12)
        );
        assert_eq!(out[0], expected0);
        // The prose row: empty counts (35 spaces) and empty prior (11 spaces) survive as the gap.
        let expected1 = format!(
            "  mimo :: mimo-v2.6-pro{}  prose (no verdict)  {}  {}  405.7 s  handoffs/03-codex-merge-framing-mimo.md",
            " ".repeat(20),
            " ".repeat(35),
            " ".repeat(11)
        );
        assert_eq!(out[1], expected1);
        // A wide non-skipped row pads its status to 0 (no grid): just lineage(41) + status.
        let expected3 = format!(
            "  gemini :: gemini-3.1-pro-high [agy]{}  failed: provider gemini is not usable: its usage limit (hit at 2026-09-26T17:47:18+02:00: Individual quota reached. Please upgrade your subscription to increase your limits. Resets in 51h38m) lasts until 2026-09-28T21:30:50+02:00; nothing was started (pass -SkipPreflight to launch anyway) (n=4 and handoff 05 stay unused)",
            " ".repeat(6)
        );
        assert_eq!(out[3], expected3);
        // The muse row's shorter counts (27) pad to w_counts=35.
        let expected9 = format!(
            "  meta :: muse-spark-1.3-contributor [muse]  ADVISE{}  1 blocker, 5 major, 0 minor{}  prior: none  103.6 s  handoffs/11-muse-merge-framing-meta.md",
            " ".repeat(12),
            " ".repeat(8)
        );
        assert_eq!(out[9], expected9);
    }
}
