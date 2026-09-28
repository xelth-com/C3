//! The panel path of `c3 consult` (`--panel`/`--panel-all`): compute the plan the parent run
//! would build (`codex-consult.ps1:1967-2588`) and either print the dry-run block byte-identical
//! to the plugin, or refuse a real run — the panel runtime (launching the member children, the
//! scheduler, the summary block) lands in the next chunk.
//!
//! The plan is: read the roster, resolve every entry's availability (`Select-PanelMembers`),
//! resolve the required reviewers (`-Require`, else roster `require`; a required outage refuses
//! before anything starts, exit 5), seat them (`Select-PanelRouting` — the seeded weighted draw
//! or roster order), build the concurrency plan (`Get-PanelPlan`), assign the per-seat numbers,
//! and render.
//!
//! Ratings for the routed draw are not yet wired (the all-task rating store is the scoreboard's,
//! out of this chunk): the panel is fed an empty rating set, so a routed panel falls back to
//! roster order (`fallback: "no ratings"`) exactly as it would with no rated reviewer. Wiring the
//! rating reader is a follow-up.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use c3_core::lineage::format_reviewer_lineage;
use c3_core::store::{EvidenceStore, FilesStore};
use c3_core::task_slug::TaskSlug;

use crate::consult::args::{Options, Resolved};
use crate::providers;

use super::plan::{self, MemberInput, PanelPlan, PanelRoutingResult, RoutingRecord, Runner};
use super::routing::Rating;

const TOOL: &str = "codex-consult";

/// Run the panel path; return the process exit code.
pub fn run(o: Options, r: Resolved, _home: Option<&str>) -> i32 {
    match plan_panel(&o) {
        Ok(plan) => {
            if o.dry_run {
                for line in render_dry_run(&o, &r, &plan) {
                    println!("{line}");
                }
                0
            } else {
                eprintln!(
                    "{TOOL}: -Panel is planned but its runtime lands in the next chunk; run it with -DryRun to see the plan, or drop -Panel to consult one reviewer."
                );
                1
            }
        }
        Err((msg, code)) => {
            eprintln!("{TOOL}: {msg}");
            code
        }
    }
}

/// The computed panel plan (everything the dry-run block and, later, the scheduler need).
struct Plan {
    panel_id: String,
    roster_path: String,
    entry_count: usize,
    /// Per roster-order entry: position, shown lineage, state, reason, required, and its seat
    /// number/role (when seated).
    rows: Vec<PlanRow>,
    routing: RoutingRecord,
    concurrency: PanelPlan,
    warnings: Vec<String>,
    topics: Vec<String>,
    range_line: String,
    range_warning: String,
    /// The `pending     : ...` recovery lines a dry run reports (`codex-consult.ps1:2620`).
    pending_lines: Vec<String>,
    seated_count: usize,
    verb: &'static str,
}

struct PlanRow {
    position: i64,
    shown: String,
    /// `run` | `not-picked` | `skipped`.
    state: String,
    reason: String,
    required: bool,
    n: i64,
    nn: u32,
    role: String,
}

fn plan_panel(o: &Options) -> Result<Plan, (String, i32)> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = providers::resolve_repo_root(&cwd);
    let collab_root = providers::resolve_collab_root(&repo_root, &o.collab_dir);
    let task = TaskSlug::new(o.task.clone()).map_err(|e| (e.to_string(), 1))?;

    let launcher = providers::resolve_codex_launcher(&o.codex_exe).map_err(|m| (m, 1))?;
    let config_path = providers::get_codex_config_path();
    let config = providers::read_codex_config(&config_path);
    let openai_base_url = std::env::var("OPENAI_BASE_URL").unwrap_or_default();

    let roster = providers::read_reviewer_roster().map_err(|m| (m, 1))?;
    if !roster.exists {
        return Err((
            format!(
                "-Panel needs a reviewer roster: '{}' does not exist (CODEX_CONSULT_ROSTER, else <codex home>/codex-consult-roster.json).",
                roster.path
            ),
            1,
        ));
    }

    let utc_now = c3_core::peak::consult_clock(0)
        .map(|(u, _, _)| u)
        .unwrap_or_else(|_| chrono::Utc::now());

    // Availability + the weighty gate (`Select-PanelMembers`).
    let ctx = providers::Ctx::for_consult(
        config.clone(),
        providers::read_all_task_consults(&collab_root),
        roster.clone(),
        launcher.clone(),
        openai_base_url.clone(),
        utc_now,
    );
    let selection = ctx.panel_members(
        &o.model,
        &o.engine,
        &o.purpose,
        o.panel_all,
        o.skip_preflight,
    );

    // Required reviewers (`-Require`, else roster `require.<purpose>`); a required outage refuses
    // the whole panel before anything starts (exit 5), a dry run included.
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
            Some(m) if m.state == "skipped" && m.skip_kind != "weighty" => {
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

    // The seats (`Select-PanelRouting`). Ratings are empty until the rating reader is wired, so a
    // routed panel falls back to roster order (`fallback: "no ratings"`).
    let members: Vec<MemberInput> = selection
        .members
        .iter()
        .map(|m| MemberInput {
            entry: m.entry.clone(),
            provider: m.identity.provider.clone(),
            model: m.identity.model.clone(),
            engine: if m.entry.engine.is_empty() {
                "codex".to_string()
            } else {
                m.entry.engine.clone()
            },
            state: m.state.clone(),
            reason: m.reason.clone(),
            skip_kind: m.skip_kind.clone(),
        })
        .collect();

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
    let ratings: Vec<Rating> = Vec::new();

    let route: PanelRoutingResult = plan::select_panel_routing(
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

    // Roles: `-Role` names one role for every seated member (`-Roles` rank matching lands with the
    // scheduler). `role_of`: seat position -> role slug.
    let mut role_of: HashMap<i64, String> = HashMap::new();
    if !o.role.is_empty() {
        for p in &route.picked {
            role_of.insert(p.position, o.role.clone());
        }
    }

    // The concurrency plan (`Get-PanelPlan`) over the seated runners.
    let runners: Vec<Runner> = route
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
    let concurrency = plan::panel_plan(&runners, &roster.parallel, o.panel_concurrency);

    // Recovery records: a corrupt one refuses even the dry run (fail-closed); an inactive one is
    // reported (`the real run recovers it`); an active one would refuse the real run.
    let store = FilesStore::new(collab_root.clone());
    let mut pending_lines: Vec<String> = Vec::new();
    {
        let assessed = crate::consult::recovery::assess(&store, &task);
        if let Some(err) = assessed.error {
            return Err((err, 1));
        }
        for item in &assessed.items {
            let n_shown = if item.n > 0 {
                item.n.to_string()
            } else {
                String::new()
            };
            if item.active {
                pending_lines.push(format!(
                    "pending     : the real run would be REFUSED - {}",
                    item.message
                ));
            } else {
                pending_lines.push(format!(
                    "pending     : {} (state '{}', n={n_shown}, nn={}; {}): the real run recovers it; numbering continues past it.",
                    item.path.display(),
                    item.state,
                    item.nn,
                    item.check
                ));
            }
        }
    }

    // The per-seat numbers (`Get-NextNumbers` + `n0+k-1`/`nn0+k-1`, seat order).
    let nn_n = store.next_numbers(&task).map_err(|e| (e.to_string(), 1))?;
    let mut seat_of: HashMap<i64, (i64, u32)> = HashMap::new();
    for (k, p) in route.picked.iter().enumerate() {
        let n = nn_n.n + k as i64;
        let nn = nn_n.nn + k as u32;
        seat_of.insert(p.position, (n, nn));
    }

    // Warnings (`panelRoute.Warnings`); `-Roles` note not modelled yet.
    let warnings: Vec<String> = route.warnings.clone();

    // The per-entry display rows (roster order).
    let mut rows: Vec<PlanRow> = Vec::new();
    for m in &route.members {
        let (n, nn) = seat_of.get(&m.position).copied().unwrap_or((0, 0));
        rows.push(PlanRow {
            position: m.position,
            shown: m.lineage.clone(),
            state: m.state.clone(),
            reason: m.reason.clone(),
            required: m.required,
            n,
            nn,
            role: role_of.get(&m.position).cloned().unwrap_or_default(),
        });
    }

    let panel_id = uuid::Uuid::new_v4().to_string();

    Ok(Plan {
        panel_id,
        roster_path: roster.path.clone(),
        entry_count: selection.members.len(),
        rows,
        routing: route.routing,
        concurrency,
        warnings,
        topics,
        range_line: String::new(),
        range_warning: String::new(),
        pending_lines,
        seated_count: route.picked.len(),
        verb: if o.dry_run { "would run" } else { "run" },
    })
}

/// The panel draw nonce and its source (`-PanelSeed`, else the env hook, else today's UTC date).
fn panel_nonce(o: &Options, utc_now: chrono::DateTime<chrono::Utc>) -> (String, &'static str) {
    let seed = o.panel_seed.trim();
    if !seed.is_empty() {
        return (seed.to_string(), "-PanelSeed");
    }
    if let Ok(env) = std::env::var("CODEX_CONSULT_TEST_PANEL_SEED") {
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

/// Render the whole panel dry-run block (`codex-consult.ps1:2562-2588`).
fn render_dry_run(o: &Options, r: &Resolved, plan: &Plan) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let short = &plan.panel_id[..plan.panel_id.len().min(8)];
    let dry = if o.dry_run {
        " (dry run - nothing is executed or written)"
    } else {
        ""
    };
    out.push(format!(
        "Panel {short}{dry}: {} of {} roster entries {}, {} (roster {}; panel id {})",
        plan.seated_count,
        plan.entry_count,
        plan.verb,
        plan.concurrency.text,
        plan.roster_path,
        plan.panel_id
    ));
    for row in &plan.rows {
        let state_shown = if row.state == "run" {
            let role = if row.role.is_empty() {
                String::new()
            } else {
                format!(", role {}", row.role)
            };
            let required = if row.required { ", required" } else { "" };
            format!("member, n={}, handoff {:02}{role}{required}", row.n, row.nn)
        } else if row.state == "not-picked" {
            format!("not picked: {}", row.reason)
        } else {
            format!("skipped: {}", row.reason)
        };
        out.push(format!("  #{} {} - {state_shown}", row.position, row.shown));
    }
    for line in routing_lines(&plan.routing, &o.purpose) {
        out.push(line);
    }
    if !plan.topics.is_empty() {
        out.push(format!("Topics: {}", plan.topics.join(", ")));
    }
    for w in &plan.warnings {
        out.push(format!("WARNING: {w}"));
    }
    out.push(concurrency_line(o, plan));
    out.push(timeout_line(o, r, plan));
    if !plan.range_line.is_empty() {
        out.push(plan.range_line.clone());
    }
    if !plan.range_warning.is_empty() {
        out.push(format!("WARNING: {}", plan.range_warning));
    }
    for line in &plan.pending_lines {
        out.push(line.clone());
    }
    out
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

/// The `Concurrency:` line (`codex-consult.ps1:2576-2582`).
fn concurrency_line(o: &Options, plan: &Plan) -> String {
    let group_texts: Vec<String> = plan
        .concurrency
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
        plan.concurrency.text,
        group_texts.join(", ")
    )
}

/// The `Timeout:` line (`codex-consult.ps1:2584-2585`). Per-member roster timeout exceptions are
/// not modelled here (no roster `timeout_sec` in the port yet).
fn timeout_line(_o: &Options, r: &Resolved, _plan: &Plan) -> String {
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
    format!(
        "Timeout: {} s per member ({source}); continuation after a timeout kill: {cont}",
        r.timeout_sec
    )
}

/// Format a number as PowerShell's `ToString('0.###'|'0.##')` would: fixed to `decimals`, then
/// trailing zeros and a trailing dot trimmed.
fn fmt_ps(x: f64, decimals: usize) -> String {
    let s = format!("{:.1$}", x, decimals);
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}
