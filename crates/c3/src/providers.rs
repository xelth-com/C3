//! `c3 providers`: list the Codex model providers (and the roster's non-codex engine
//! labels) and whether each is usable right now. A faithful port of
//! `codex-providers.ps1`, driving the pure formats in `c3-core` and adding the
//! runtime side: launcher discovery, `codex login status` / `agy models`, muse
//! `auth.json`, the task ledgers, the consult clock and the roster path.
//!
//! It honours the same environment as the PowerShell original: `CODEX_HOME`,
//! `CODEX_CONSULT_EXE` (`--codex-exe` wins), `CODEX_CONSULT_ROSTER`,
//! `CODEX_CONSULT_AGY_EXE` / `CODEX_CONSULT_MUSE_EXE` (`--engine-exe` wins),
//! `OPENAI_BASE_URL`, `CODEX_CONSULT_NOW` (test clock) and
//! `CODEX_CONSULT_TEST_LOGIN_TIMEOUT`. It never prints, logs or stores a credential
//! value, and it writes nothing.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use chrono::{DateTime, FixedOffset, Local, Utc};
use serde_json::{json, Value};

use c3_core::availability::{
    convert_to_availability_record, endpoint_groups, format_availability_line, format_roster_skips,
    AvailabilityRecord,
};
use c3_core::config::{
    self, config_not_found, config_unreadable, provider_endpoint, provider_names,
    provider_set_problem, provider_table, scan_config_text, CodexConfig, Table,
};
use c3_core::credential::{CredentialResult, State};
use c3_core::effort::{caps, Models};
use c3_core::health::{endpoint_health, format_offset_iso, EndpointHealth};
use c3_core::lineage::{
    engine_spec, format_reviewer_lineage, install_launchers, launcher_names,
    resolve_reviewer_identity, ReviewerIdentity,
};
use c3_core::plan::{plan_quota, plan_quota_verdict, plan_routes, PlanQuota};
use c3_core::roster::{roster_refusal, validate_roster, Roster, RosterEntry};
use c3_core::verdict::{verdict_pre_credential, verdict_with_credential, PreflightVerdict};

/// The `c3 providers` options (parsed by the CLI).
#[derive(Debug, Clone, Default)]
pub struct Options {
    pub provider: String,
    pub collab_dir: String,
    pub json: bool,
    pub codex_exe: String,
    pub engine_exe: String,
    pub no_network: bool,
    pub short: bool,
}

/// Run `c3 providers`; returns the process exit code.
pub fn run(opts: Options) -> i32 {
    match run_inner(&opts) {
        Ok(code) => code,
        Err(msg) => {
            println!("codex-providers: {msg}");
            1
        }
    }
}

// A verdict attached to a row, in row order (the -Short line without a roster).
struct RowVerdict {
    verdict: PreflightVerdict,
    block: String,
    engine: String,
}

enum EffortModels {
    Any,
    List(Vec<String>),
    None,
}

struct Row {
    name: String,
    engine: String,
    kind: String,
    endpoint: String,
    #[allow(dead_code)]
    wire_api: String,
    table: String,
    credentials: String,
    effort_vocabulary: String,
    effort_models: EffortModels,
    schema_transport: String,
    last_limit: Option<Value>,
    last_failure: Option<Value>,
    roster_position: Option<i64>,
    roster_positions: Vec<i64>,
    roster_selected: bool,
    verdict: String,
    health_source: String,
}

pub(crate) struct Ctx {
    config: CodexConfig,
    consults: Vec<Value>,
    roster: Roster,
    launcher: String,
    openai_base_url: String,
    utc_now: DateTime<Utc>,
    no_network: bool,
    login_cache: RefCell<HashMap<String, CredentialResult>>,
    engine_launchers: RefCell<HashMap<String, String>>,
    /// (wave 1b, E5) `Get-PlanQuota` per plan for this context's roster, consults and clock.
    plan_cache: RefCell<HashMap<String, PlanQuota>>,
}

/// Everything [`run_inner`] computes before it prints, so both the printing path and
/// [`short_line`] (the `c3 hook` reuse) can share one computation without a network call.
struct Prepared {
    ctx: Ctx,
    rows: Vec<Row>,
    where_: String,
    clock: DateTime<FixedOffset>,
    health_source: String,
    walk: Option<Walk>,
    availability_records: Vec<AvailabilityRecord>,
    availability_line: String,
}

fn prepare(opts: &Options) -> Result<Prepared, String> {
    let launcher = resolve_codex_launcher(&opts.codex_exe)?;
    let config_path = get_codex_config_path();
    let config = read_codex_config(&config_path);
    let where_ = if !config_path.is_empty() {
        config_path.clone()
    } else {
        "(no Codex home)".to_string()
    };
    let file_reason = if config.exists && !config.ok {
        config.reason.clone()
    } else {
        String::new()
    };

    let mut names: Vec<String> = vec!["openai".to_string()];
    for n in provider_names(&config) {
        if n != "openai" {
            names.push(n);
        }
    }

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = resolve_repo_root(&cwd);
    let collab_root = resolve_collab_root(&repo_root, &opts.collab_dir);
    let consults = read_all_task_consults_health(&collab_root);
    let clock = get_consult_clock_peek()?;
    let utc_now = clock.with_timezone(&Utc);

    let roster = read_reviewer_roster()?;

    let mut engine_launchers: HashMap<String, String> = HashMap::new();
    if !opts.engine_exe.is_empty() {
        let engine = resolve_engine_exe_binding(&roster, &opts.provider)?;
        let l = resolve_engine_launcher(&engine, &opts.engine_exe)?.unwrap_or_default();
        engine_launchers.insert(engine, l);
    }

    // The provider labels of the other engines: one row each, in roster order.
    let mut engine_labels: Vec<(String, String)> = Vec::new();
    for e in &roster.entries {
        if e.engine == "codex" {
            continue;
        }
        if !engine_labels.iter().any(|(n, _)| *n == e.provider) {
            engine_labels.push((e.provider.clone(), e.engine.clone()));
        }
    }

    if !opts.provider.is_empty() {
        let mut known: Vec<String> = names.clone();
        known.extend(engine_labels.iter().map(|(n, _)| n.clone()));
        if !known.contains(&opts.provider) {
            return Err(format!(
                "no provider '{}' in {where_} (providers: {}).",
                opts.provider,
                known.join(", ")
            ));
        }
        let keep = names.contains(&opts.provider)
            && !engine_labels.iter().any(|(n, _)| *n == opts.provider);
        names = if keep {
            vec![opts.provider.clone()]
        } else {
            vec![]
        };
        engine_labels = roster
            .entries
            .iter()
            .find(|e| e.engine != "codex" && e.provider == opts.provider)
            .map(|e| vec![(e.provider.clone(), e.engine.clone())])
            .unwrap_or_default();
    }
    if opts.short && !opts.provider.is_empty() {
        return Err("-Short summarizes every reviewer of the roster; drop -Provider (or drop -Short for one provider's row).".into());
    }

    let ledger_count = count_task_ledgers(&collab_root);
    let consult_count = consults.len();
    let health_source = format!(
        "{} ({} task ledger{}, {} consultation{})",
        collab_root.display(),
        ledger_count,
        if ledger_count != 1 { "s" } else { "" },
        consult_count,
        if consult_count != 1 { "s" } else { "" }
    );

    let ctx = Ctx {
        config,
        consults,
        roster,
        launcher,
        openai_base_url: std::env::var("OPENAI_BASE_URL").unwrap_or_default(),
        utc_now,
        no_network: opts.no_network,
        login_cache: RefCell::new(HashMap::new()),
        engine_launchers: RefCell::new(engine_launchers),
        plan_cache: RefCell::new(HashMap::new()),
    };

    // The single-run walk ("would select") and every entry's availability.
    let walk = if ctx.roster.exists {
        Some(ctx.select_roster_reviewer())
    } else {
        None
    };
    let avail_records = if ctx.roster.exists {
        Some(ctx.roster_availability())
    } else {
        None
    };

    // ---- provider (codex) rows
    let mut rows: Vec<Row> = Vec::new();
    let mut row_verdicts: Vec<RowVerdict> = Vec::new();
    for name in &names {
        let (row, rv) = ctx.build_codex_row(name, &where_, &file_reason, &walk, &health_source);
        rows.push(row);
        row_verdicts.push(rv);
    }

    // ---- engine rows
    for (label, engine) in &engine_labels {
        let (row, rv) = ctx.build_engine_row(label, engine, &walk, &health_source);
        rows.push(row);
        row_verdicts.push(rv);
    }

    // ---- availability line
    let (availability_records, noun, suffix): (Vec<AvailabilityRecord>, &str, &str) =
        if let Some(recs) = &avail_records {
            (recs.clone(), "reviewers", "")
        } else {
            let mut recs = Vec::new();
            for (i, rv) in row_verdicts.iter().enumerate() {
                recs.push(convert_to_availability_record(
                    (i + 1) as i64,
                    &rows[i].name,
                    "",
                    &rv.engine,
                    i,
                    Some(&rv.verdict),
                    &rv.block,
                    ctx.utc_now,
                ));
            }
            (recs, "providers", " (no reviewer roster)")
        };
    let availability_line =
        format_availability_line(&availability_records, noun, "codex-consult: ", suffix);

    Ok(Prepared {
        ctx,
        rows,
        where_,
        clock,
        health_source,
        walk,
        availability_records,
        availability_line,
    })
}

fn run_inner(opts: &Options) -> Result<i32, String> {
    let p = prepare(opts)?;

    // ---- output
    if opts.short {
        if opts.json {
            print_short_json(
                &p.availability_line,
                &p.health_source,
                &p.availability_records,
                &p.ctx.roster,
            );
        } else {
            println!("{}", p.availability_line);
        }
        return Ok(0);
    }

    if opts.json {
        print_rows_json(&p.rows);
    } else {
        print_table(
            &p.ctx,
            &p.rows,
            &p.where_,
            &p.clock,
            &p.health_source,
            &p.walk,
            &p.availability_line,
        );
    }

    if !opts.provider.is_empty() {
        return Ok(exit_code_for_verdict(&p.rows[0].verdict));
    }
    Ok(0)
}

/// The `codex-consult:` availability line `providers --short` prints, computed WITHOUT
/// printing anything and without a network call (the caller passes `no_network: true`).
/// This is the reuse `c3 hook` needs: it produces the SessionStart line in-process
/// instead of shelling out to `codex-providers.ps1 -Short -Json -NoNetwork` and parsing
/// its `line` field, as `codex-consult-hook.ps1` does.
pub fn short_line(opts: &Options) -> Result<String, String> {
    Ok(prepare(opts)?.availability_line)
}

/// The `-Provider` exit code: 0 available, 2 unavailable, 3 unknown.
pub fn exit_code_for_verdict(verdict: &str) -> i32 {
    if verdict == "available" {
        0
    } else if verdict.starts_with("unavailable") {
        2
    } else {
        3
    }
}

// (group, kind, reason, hit, until, short) for the D16 group-outage marking.
type GroupTrigger = (
    usize,
    String,
    String,
    Option<DateTime<FixedOffset>>,
    Option<DateTime<FixedOffset>>,
    String,
);

struct Walk {
    entry: Option<RosterEntry>,
    identity: Option<ReviewerIdentity>,
    skipped: Vec<(String, String, String, String)>,
}

/// The full `Select-RosterReviewer` result the consult path needs (the walk to the first
/// available entry, `-Model` narrowing, `-SkipPreflight`, the "no entry" refusals, the
/// `Considered` count and the skip records).
pub(crate) struct RosterWalk {
    pub entry: Option<RosterEntry>,
    pub identity: Option<ReviewerIdentity>,
    /// `(provider, model, engine, reason)` in roster order.
    pub skipped: Vec<(String, String, String, String)>,
    pub error: String,
}

impl Ctx {
    /// Construct a context for the consult roster walk (its own fresh caches).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn for_consult(
        config: CodexConfig,
        consults: Vec<Value>,
        roster: Roster,
        launcher: String,
        openai_base_url: String,
        utc_now: DateTime<Utc>,
    ) -> Ctx {
        Ctx {
            config,
            consults,
            roster,
            launcher,
            openai_base_url,
            utc_now,
            no_network: false,
            login_cache: RefCell::new(HashMap::new()),
            engine_launchers: RefCell::new(HashMap::new()),
            plan_cache: RefCell::new(HashMap::new()),
        }
    }

    /// The full roster walk (`Select-RosterReviewer`): the first entry whose preflight is
    /// available. `model` (an explicit `-Model` without `-Provider`) restricts the walk to the
    /// entries that resolve to that model; `skip_preflight` takes the first unchecked entry.
    pub(crate) fn walk_full(&self, model: &str, engine: &str, skip_preflight: bool) -> RosterWalk {
        self.walk_full_ctx(model, engine, skip_preflight, 0)
    }

    /// [`walk_full`] with a prompt token estimate (`> 0`): an entry whose `context_tokens` the
    /// brief alone would fill beyond 80% is skipped before its preflight (wave 26b, D16 b).
    pub(crate) fn walk_full_ctx(
        &self,
        model: &str,
        engine: &str,
        skip_preflight: bool,
        context_estimate: i64,
    ) -> RosterWalk {
        let mut skipped: Vec<(String, String, String, String)> = Vec::new();
        let mut listing: Vec<String> = Vec::new();
        let mut considered = 0usize;
        for e in &self.roster.entries {
            let entry_engine = if e.engine.is_empty() {
                "codex"
            } else {
                &e.engine
            };
            // `-Engine X` restricts the walk to entries of that engine (`Select-RosterReviewer`).
            if !engine.is_empty() && entry_engine != engine {
                continue;
            }
            let entry_launcher = self.engine_launcher(entry_engine);
            let id = resolve_reviewer_identity(
                &self.config,
                &e.provider,
                &e.model,
                &self.openai_base_url,
                entry_engine,
                &entry_launcher,
            );
            if !model.is_empty() && id.model != *model {
                continue;
            }
            considered += 1;
            // (wave 26b, D16 b) skip a reviewer whose context window the brief alone would overflow.
            if context_estimate > 0
                && e.context_tokens > 0
                && (context_estimate as f64) > 0.8 * e.context_tokens as f64
            {
                let reason = format!(
                    "brief too large for this reviewer's context (est. {context_estimate} of {} tokens)",
                    e.context_tokens
                );
                skipped.push((
                    e.provider.clone(),
                    id.model.clone(),
                    entry_engine.to_string(),
                    reason.clone(),
                ));
                listing.push(format!(
                    "#{} {} ({reason})",
                    e.position,
                    format_reviewer_lineage(&id.provider, &id.model, entry_engine)
                ));
                continue;
            }
            let block = self.engine_launch_block(entry_engine);
            if !block.is_empty() {
                let reason = format!("refused: {block}");
                skipped.push((
                    e.provider.clone(),
                    id.model.clone(),
                    entry_engine.to_string(),
                    reason.clone(),
                ));
                listing.push(format!(
                    "#{} {} ({reason})",
                    e.position,
                    format_reviewer_lineage(&id.provider, &id.model, entry_engine)
                ));
                continue;
            }
            if skip_preflight {
                if !id.error.is_empty() {
                    return RosterWalk {
                        entry: None,
                        identity: None,
                        skipped,
                        error: id.error,
                    };
                }
                return RosterWalk {
                    entry: Some(e.clone()),
                    identity: Some(id),
                    skipped,
                    error: String::new(),
                };
            }
            let health = if id.resolved {
                Some(endpoint_health(
                    &self.consults,
                    &id.fingerprint,
                    self.utc_now,
                ))
            } else {
                None
            };
            let verdict = self.preflight(
                &id,
                health.as_ref(),
                &entry_launcher,
                e.auth == "none",
                true,
            );
            // (wave 29b, E5) a usage limit on another route of the entry's plan
            let verdict = self.plan_verdict(verdict, e, &id, true);
            if verdict.state == "available" {
                return RosterWalk {
                    entry: Some(e.clone()),
                    identity: Some(id),
                    skipped,
                    error: String::new(),
                };
            }
            listing.push(format!(
                "#{} {} ({})",
                e.position,
                format_reviewer_lineage(&id.provider, &id.model, entry_engine),
                verdict.reason
            ));
            skipped.push((
                e.provider.clone(),
                id.model.clone(),
                entry_engine.to_string(),
                verdict.reason.clone(),
            ));
        }
        if considered == 0 {
            let all = self
                .roster
                .entries
                .iter()
                .map(|e| {
                    format!(
                        "#{} {}",
                        e.position,
                        if !e.model.is_empty() {
                            format_reviewer_lineage(&e.provider, &e.model, &e.engine)
                        } else {
                            format!("{} (config model)", e.provider)
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            return RosterWalk {
                entry: None,
                identity: None,
                skipped,
                    error: format!(
                    "-Model {model}: no entry of the reviewer roster '{}' resolves to that model ({all}); pass -Provider <name> -Model {model} to choose a reviewer outside the roster",
                    self.roster.path
                ),
            };
        }
        let error = format!(
            "no reviewer of the roster '{}' is available; nothing was started: {} (run codex-providers.ps1 for the full picture)",
            self.roster.path,
            listing.join("; ")
        );
        RosterWalk {
            entry: None,
            identity: None,
            skipped,
            error,
        }
    }
}

/// The purposes on which a weighty roster entry still joins a panel without `-PanelAll` (and a
/// light one only stands in) (`$script:WeightyPurposes`).
const WEIGHTY_PURPOSES: &[&str] = &[
    "framing",
    "decision",
    "core-contract",
    "acceptance",
    "stuck",
];

/// One row of `Select-PanelMembers`: a roster entry, its resolved identity, and whether it runs.
pub(crate) struct PanelMemberRow {
    pub entry: RosterEntry,
    pub identity: ReviewerIdentity,
    /// `run` | `skipped`.
    pub state: String,
    pub reason: String,
    /// `""` | `refused` | `unavailable` | `weighty` | `context` | `light`.
    pub skip_kind: String,
    /// The verdict's `until` (an unavailable entry's way back), `None` otherwise.
    pub until: Option<DateTime<FixedOffset>>,
}

/// `Format-RequiredOutage`: `#<n> <lineage> (<reason>[; back <local>, <relative>])` - a required
/// reviewer that is not available, with its way back when the verdict knows it.
pub(crate) fn format_required_outage(m: &PanelMemberRow, now_utc: DateTime<Utc>) -> String {
    let engine = if m.entry.engine.is_empty() {
        "codex"
    } else {
        &m.entry.engine
    };
    let shown = format_reviewer_lineage(&m.identity.provider, &m.identity.model, engine);
    let mut text = format!("#{} {shown} ({}", m.entry.position, m.reason);
    if let Some(until) = m.until {
        text.push_str(&format!(
            "; back {}, {}",
            c3_core::availability::format_local_when(until, now_utc),
            c3_core::availability::format_relative_hint(until.with_timezone(&Utc) - now_utc)
        ));
    }
    text.push(')');
    text
}

/// `Select-PanelMembers`' result.
pub(crate) struct PanelSelection {
    pub members: Vec<PanelMemberRow>,
    pub error: String,
}

impl Ctx {
    /// `Select-PanelMembers`: every roster entry with its availability state, the weighty gate,
    /// (wave 26b, D16) the context skip (`estimate_tokens`: the new prompt's estimate, 0 = none),
    /// (0.6.0) the light gate and its stand-in, and the "no eligible member" / "-Model/-Engine no
    /// entry" refusals. The caller passes `all`/`skip_preflight` for `-PanelAll`/`-SkipPreflight`.
    /// A "light" entry on a weighty purpose without `all` is held back (skip kind `light`) once it
    /// passed every other check; after the whole roster is judged it stands in - state run, reason
    /// "stands in for #<p> (<that sibling's skip reason>)" or "stands in (no other entry of label
    /// <label>)" - when no other member of its provider label (compared ordinally) runs; in roster
    /// order, so a second light entry of the label sees the first one run.
    pub(crate) fn panel_members(
        &self,
        model: &str,
        engine: &str,
        purpose: &str,
        all: bool,
        skip_preflight: bool,
        estimate_tokens: i64,
    ) -> PanelSelection {
        self.panel_members_of(
            None,
            model,
            engine,
            purpose,
            all,
            skip_preflight,
            estimate_tokens,
        )
    }

    /// [`panel_members`] judging only the entries at `positions` (`None`: every entry) - (wave
    /// 2e, F11-3) the single run's `-Require` check: the context keeps the FULL roster, so a plan's
    /// quota (`plan_quota`) sees every route of the plan, also one of an entry that is not required.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn panel_members_of(
        &self,
        positions: Option<&[i64]>,
        model: &str,
        engine: &str,
        purpose: &str,
        all: bool,
        skip_preflight: bool,
        estimate_tokens: i64,
    ) -> PanelSelection {
        let purpose_label = if purpose.is_empty() { "none" } else { purpose };
        let weighty_purpose = WEIGHTY_PURPOSES.contains(&purpose);
        let mut members: Vec<PanelMemberRow> = Vec::new();
        for e in &self.roster.entries {
            if positions.is_some_and(|p| !p.contains(&(e.position as i64))) {
                continue;
            }
            let entry_engine = if e.engine.is_empty() {
                "codex"
            } else {
                &e.engine
            };
            if !engine.is_empty() && entry_engine != engine {
                continue;
            }
            let entry_launcher = self.engine_launcher(entry_engine);
            let id = resolve_reviewer_identity(
                &self.config,
                &e.provider,
                &e.model,
                &self.openai_base_url,
                entry_engine,
                &entry_launcher,
            );
            if !model.is_empty() && id.model != *model {
                continue;
            }
            let mut state = "run".to_string();
            let mut reason = String::new();
            let mut skip_kind = String::new();
            let mut until: Option<DateTime<FixedOffset>> = None;
            let health = if id.resolved {
                Some(endpoint_health(
                    &self.consults,
                    &id.fingerprint,
                    self.utc_now,
                ))
            } else {
                None
            };
            let block = self.engine_launch_block(entry_engine);
            if !block.is_empty() {
                state = "skipped".into();
                reason = format!("refused: {block}");
                skip_kind = "refused".into();
            } else if !skip_preflight {
                let verdict = self.preflight(
                    &id,
                    health.as_ref(),
                    &entry_launcher,
                    e.auth == "none",
                    true,
                );
                // (wave 29b, E5) a usage limit on another route of the entry's plan
                let verdict = self.plan_verdict(verdict, e, &id, true);
                if verdict.state != "available" {
                    state = "skipped".into();
                    reason = verdict.reason.clone();
                    skip_kind = "unavailable".into();
                    until = verdict.until;
                }
            }
            if state == "run" && e.panel == "weighty" && !all && !weighty_purpose {
                state = "skipped".into();
                reason =
                    format!("weighty reviewer; purpose {purpose_label} is light (use -PanelAll)");
                skip_kind = "weighty".into();
            }
            // (wave 26b, D16) skipped before its start: the brief is too large for its context
            // window (the new prompt's estimate beyond 80% of context_tokens)
            if state == "run"
                && estimate_tokens > 0
                && e.context_tokens > 0
                && (estimate_tokens as f64) > 0.8 * e.context_tokens as f64
            {
                state = "skipped".into();
                reason = format!(
                    "brief too large for this reviewer's context (est. {estimate_tokens} of {} tokens)",
                    e.context_tokens
                );
                skip_kind = "context".into();
            }
            // (0.6.0) a light entry on a weighty purpose: held back after every other check (a
            // stand-in below needs none again)
            if state == "run" && e.panel == "light" && !all && weighty_purpose {
                state = "skipped".into();
                reason = format!("light reviewer; purpose {purpose_label} is weighty - it stands in only when no entry of its label runs");
                skip_kind = "light".into();
            }
            members.push(PanelMemberRow {
                entry: e.clone(),
                identity: id,
                state,
                reason,
                skip_kind,
                until,
            });
        }
        stand_in_light_members(&mut members);
        let listing: Vec<String> = members
            .iter()
            .map(|m| {
                let entry_engine = if m.entry.engine.is_empty() {
                    "codex"
                } else {
                    &m.entry.engine
                };
                format!(
                    "#{} {} ({})",
                    m.entry.position,
                    format_reviewer_lineage(&m.identity.provider, &m.identity.model, entry_engine),
                    if m.state == "run" { "runs" } else { &m.reason }
                )
            })
            .collect();
        let mut error = String::new();
        if members.is_empty() {
            let all_list = self
                .roster
                .entries
                .iter()
                .map(|e| {
                    format!(
                        "#{} {}",
                        e.position,
                        if !e.model.is_empty() {
                            format_reviewer_lineage(&e.provider, &e.model, &e.engine)
                        } else {
                            format!("{} (config model)", e.provider)
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            if !engine.is_empty() && model.is_empty() {
                error = format!(
                    "-Engine {engine}: no entry of the reviewer roster '{}' uses that engine ({all_list})",
                    self.roster.path
                );
            } else {
                error = format!(
                    "-Model {model}: no entry of the reviewer roster '{}' resolves to that model ({all_list}); pass -Provider <name> -Model {model} to choose a reviewer outside the roster",
                    self.roster.path
                );
            }
        } else if !members.iter().any(|m| m.state == "run") {
            error = format!(
                "no reviewer of the roster '{}' is available; nothing was started: {} (run codex-providers.ps1 for the full picture)",
                self.roster.path,
                listing.join("; ")
            );
        }
        PanelSelection { members, error }
    }
}

/// (0.6.0) The light stand-in (`Select-PanelMembers`, after the whole roster is judged): a
/// held-back light entry (skip kind `light`) runs when no other member of its provider label runs;
/// the first skipped sibling that is not itself a held-back light entry is named. In roster order.
pub(crate) fn stand_in_light_members(members: &mut [PanelMemberRow]) {
    for i in 0..members.len() {
        if members[i].state != "skipped" || members[i].skip_kind != "light" {
            continue;
        }
        let label = members[i].entry.provider.clone();
        let siblings: Vec<usize> = (0..members.len())
            .filter(|&j| j != i && members[j].entry.provider == label)
            .collect();
        if siblings.iter().any(|&j| members[j].state == "run") {
            continue;
        }
        let named = siblings
            .iter()
            .find(|&&j| members[j].state == "skipped" && members[j].skip_kind != "light")
            .map(|&j| (members[j].entry.position, members[j].reason.clone()));
        let reason = match named {
            Some((pos, why)) => format!("stands in for #{pos} ({why})"),
            None if siblings.is_empty() => format!("stands in (no other entry of label {label})"),
            None => format!("stands in (no other entry of label {label} runs)"),
        };
        let m = &mut members[i];
        m.state = "run".into();
        m.skip_kind = String::new();
        m.reason = reason;
    }
}

impl Ctx {
    fn engine_launch_block(&self, engine: &str) -> String {
        if engine == "muse" {
            get_muse_launch_block()
        } else {
            String::new()
        }
    }

    fn engine_launcher(&self, engine: &str) -> String {
        if engine.is_empty() || engine == "codex" {
            return self.launcher.clone();
        }
        if let Some(l) = self.engine_launchers.borrow().get(engine) {
            return l.clone();
        }
        let l = resolve_engine_launcher(engine, "")
            .unwrap_or(None)
            .unwrap_or_default();
        self.engine_launchers
            .borrow_mut()
            .insert(engine.to_string(), l.clone());
        l
    }

    fn provider_table_ref(&self, name: &str) -> Option<&Table> {
        if self.config.exists && self.config.ok {
            let pt = provider_table(&self.config, name);
            if pt.found {
                if let Some(key) = pt.table_key {
                    return self.config.table_by_key(&key);
                }
            }
        }
        None
    }

    fn provider_credential(
        &self,
        name: &str,
        table: Option<&Table>,
        anonymous: bool,
    ) -> CredentialResult {
        let mut openai_auth = name == "openai";
        if !openai_auth {
            if let Some(t) = table {
                if let Some(e) = t.entry("requires_openai_auth") {
                    if e.supported && e.kind == "boolean" && e.value.as_deref() == Some("true") {
                        openai_auth = true;
                    }
                }
            }
        }
        if openai_auth {
            if let Some(c) = self.login_cache.borrow().get("login") {
                return c.clone();
            }
            let r = get_codex_login_status(&self.launcher, 15);
            self.login_cache
                .borrow_mut()
                .insert("login".into(), r.clone());
            return r;
        }
        let table = match table {
            Some(t) => t,
            None => return CredentialResult::unknown(format!("no [model_providers.{name}] table")),
        };
        let ek = config::get_toml_string(Some(table), "env_key");
        let bt = config::get_toml_string(Some(table), "experimental_bearer_token");
        let mut env_name = String::new();
        if ek.present && ek.reason.is_empty() {
            env_name = ek.value.trim().to_string();
        }
        if !env_name.is_empty() {
            if let Ok(v) = std::env::var(&env_name) {
                if !v.trim().is_empty() {
                    return CredentialResult::ok(format!("env {env_name} set"));
                }
            }
        }
        if bt.present && bt.reason.is_empty() && !bt.value.is_empty() {
            return CredentialResult::ok("bearer token in config");
        }
        if !env_name.is_empty() {
            return CredentialResult::missing(format!("env {env_name} not set"));
        }
        if anonymous {
            return CredentialResult::ok("declared anonymous in the roster");
        }
        CredentialResult::missing("no env_key/bearer token in the table")
    }

    /// Seed the launcher of one engine (`-EngineExe`): the consult walk and the row then use
    /// this path instead of re-resolving it (`$engineLaunchers[$engineExeEngine]`).
    pub(crate) fn seed_engine_launcher(&self, engine: &str, launcher: &str) {
        self.engine_launchers
            .borrow_mut()
            .insert(engine.to_string(), launcher.to_string());
    }

    fn engine_credential(
        &self,
        engine: &str,
        launcher: &str,
        health: Option<&EndpointHealth>,
    ) -> CredentialResult {
        let spec = match engine_spec(engine) {
            Some(s) => s,
            None => {
                return CredentialResult::unknown(format!(
                    "no credential check for engine '{engine}'"
                ))
            }
        };
        if launcher.is_empty() {
            return CredentialResult::missing(format!("{} CLI not found on PATH", spec.command));
        }
        if let Some(h) = health {
            if let Some(ru) = &h.recent_usable {
                return CredentialResult::ok(format!(
                    "signed in (usable reply {} min ago)",
                    ru.age_minutes
                ));
            }
        }
        if self.no_network && !spec.local_sign_in {
            return CredentialResult::with_detail(
                State::Unknown,
                "sign-in not checked",
                "not checked (launcher present; run codex-providers.ps1)",
            );
        }
        if spec.local_sign_in {
            // muse: local auth.json check, cached is unnecessary (deterministic)
            return get_muse_sign_in();
        }
        // agy: `agy models`, cached per launcher
        let key = format!("engine:{engine}|{launcher}");
        if let Some(c) = self.login_cache.borrow().get(&key) {
            return c.clone();
        }
        let mut timeout = 45u64;
        if let Some(hook) = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_LOGIN_TIMEOUT") {
            if let Ok(n) = hook.trim().parse::<u64>() {
                if n > 0 {
                    timeout = n;
                }
            }
        }
        let r = get_agy_models_status(launcher, timeout);
        self.login_cache.borrow_mut().insert(key, r.clone());
        r
    }

    /// `Get-PlanQuota` (wave 29b, E5) for this context: the routes of `plan` are the resolved
    /// identities of the roster's entries that name it (one per fingerprint, the first entry's
    /// label), read as one record set over this context's consults (the machine-wide records
    /// included) at its clock. Cached per plan.
    pub(crate) fn plan_quota(&self, plan: &str) -> PlanQuota {
        if let Some(c) = self.plan_cache.borrow().get(plan) {
            return c.clone();
        }
        let mut resolved: Vec<(String, String, String)> = Vec::new();
        for e in self.roster.entries.iter().filter(|e| e.plan == plan) {
            let eng = if e.engine.is_empty() {
                "codex"
            } else {
                &e.engine
            };
            let lau = self.engine_launcher(eng);
            let id = resolve_reviewer_identity(
                &self.config,
                &e.provider,
                &e.model,
                &self.openai_base_url,
                eng,
                &lau,
            );
            if id.resolved && !id.fingerprint.is_empty() {
                resolved.push((e.plan.clone(), id.fingerprint, e.provider.clone()));
            }
        }
        let routes = plan_routes(plan, &resolved);
        let pq = plan_quota(plan, &routes, &self.consults, self.utc_now);
        self.plan_cache
            .borrow_mut()
            .insert(plan.to_string(), pq.clone());
        pq
    }

    /// `Get-PlanQuotaVerdict` (wave 29b, E5): `verdict` (the entry's own) with the plan quota of
    /// `entry`'s plan applied - out when the plan is out on ANOTHER route. `roster_walk` drops
    /// the `-SkipPreflight` hint from the refusal (a direct run keeps it).
    pub(crate) fn plan_verdict(
        &self,
        verdict: PreflightVerdict,
        entry: &RosterEntry,
        id: &ReviewerIdentity,
        roster_walk: bool,
    ) -> PreflightVerdict {
        if entry.plan.is_empty()
            || !(verdict.state == "available"
                || (verdict.state == "unknown" && verdict.kind == "unknown"))
        {
            return verdict;
        }
        let pq = self.plan_quota(&entry.plan);
        let own = if id.resolved {
            id.fingerprint.as_str()
        } else {
            ""
        };
        plan_quota_verdict(verdict, &entry.plan, own, &id.provider, &pq, roster_walk)
    }

    fn preflight(
        &self,
        id: &ReviewerIdentity,
        health: Option<&EndpointHealth>,
        launcher: &str,
        anonymous: bool,
        roster_walk: bool,
    ) -> PreflightVerdict {
        if let Some(v) = verdict_pre_credential(id) {
            return v;
        }
        let engine = if id.engine.is_empty() {
            "codex"
        } else {
            &id.engine
        };
        let cred = if engine == "http" {
            self.http_credential(id, health)
        } else if engine != "codex" {
            self.engine_credential(engine, launcher, health)
        } else {
            let table = self.provider_table_ref(&id.provider);
            self.provider_credential(&id.provider, table, anonymous)
        };
        verdict_with_credential(id, health, cred, roster_walk)
    }

    /// The http engine's credential check: it has no CLI to sign into; it reads its key from the
    /// environment. The verdict is `available` when the reviewer's `key_env` is set (`env <NAME>
    /// set`), else `missing` (`env <NAME> not set`). Never reads or prints the key value; a
    /// recorded usable reply on this endpoint short-circuits like the CLI engines.
    fn http_credential(
        &self,
        id: &ReviewerIdentity,
        health: Option<&EndpointHealth>,
    ) -> CredentialResult {
        if let Some(h) = health {
            if let Some(ru) = &h.recent_usable {
                return CredentialResult::ok(format!(
                    "signed in (usable reply {} min ago)",
                    ru.age_minutes
                ));
            }
        }
        let key_env = self
            .roster
            .http_reviewers
            .iter()
            .find(|hr| hr.provider == id.provider && hr.model == id.model)
            .map(|hr| hr.key_env.clone())
            .unwrap_or_else(|| c3_core::roster_ext::DEFAULT_KEY_ENV.to_string());
        match std::env::var(&key_env) {
            Ok(v) if !v.trim().is_empty() => CredentialResult::ok(format!("env {key_env} set")),
            _ => CredentialResult::missing(format!("env {key_env} not set")),
        }
    }

    fn select_roster_reviewer(&self) -> Walk {
        let mut skipped: Vec<(String, String, String, String)> = Vec::new();
        for e in &self.roster.entries {
            let entry_engine = if e.engine.is_empty() {
                "codex"
            } else {
                &e.engine
            };
            let entry_launcher = self.engine_launcher(entry_engine);
            let id = resolve_reviewer_identity(
                &self.config,
                &e.provider,
                &e.model,
                &self.openai_base_url,
                entry_engine,
                &entry_launcher,
            );
            let block = self.engine_launch_block(entry_engine);
            if !block.is_empty() {
                skipped.push((
                    e.provider.clone(),
                    id.model.clone(),
                    entry_engine.to_string(),
                    format!("refused: {block}"),
                ));
                continue;
            }
            let health = if id.resolved {
                Some(endpoint_health(
                    &self.consults,
                    &id.fingerprint,
                    self.utc_now,
                ))
            } else {
                None
            };
            let verdict = self.preflight(
                &id,
                health.as_ref(),
                &entry_launcher,
                e.auth == "none",
                true,
            );
            // (wave 29b, E5) a usage limit on another route of the entry's plan
            let verdict = self.plan_verdict(verdict, e, &id, true);
            if verdict.state == "available" {
                return Walk {
                    entry: Some(e.clone()),
                    identity: Some(id),
                    skipped,
                };
            }
            skipped.push((
                e.provider.clone(),
                id.model.clone(),
                entry_engine.to_string(),
                verdict.reason.clone(),
            ));
        }
        Walk {
            entry: None,
            identity: None,
            skipped,
        }
    }

    fn roster_availability(&self) -> Vec<AvailabilityRecord> {
        struct Member {
            pos: i64,
            provider: String,
            model: String,
            engine: String,
            resolved: bool,
            fingerprint: String,
            verdict: Option<PreflightVerdict>,
            block: String,
        }
        let mut members: Vec<Member> = Vec::new();
        for e in &self.roster.entries {
            let entry_engine = if e.engine.is_empty() {
                "codex"
            } else {
                &e.engine
            };
            let entry_launcher = self.engine_launcher(entry_engine);
            let id = resolve_reviewer_identity(
                &self.config,
                &e.provider,
                &e.model,
                &self.openai_base_url,
                entry_engine,
                &entry_launcher,
            );
            let health = if id.resolved {
                Some(endpoint_health(
                    &self.consults,
                    &id.fingerprint,
                    self.utc_now,
                ))
            } else {
                None
            };
            let block = self.engine_launch_block(entry_engine);
            let verdict = if block.is_empty() {
                // (wave 29b, E5) with the plan quota of the entry (Select-PanelMembers -All)
                let v = self.preflight(
                    &id,
                    health.as_ref(),
                    &entry_launcher,
                    e.auth == "none",
                    true,
                );
                Some(self.plan_verdict(v, e, &id, true))
            } else {
                None
            };
            members.push(Member {
                pos: e.position as i64,
                provider: e.provider.clone(),
                model: id.model.clone(),
                engine: entry_engine.to_string(),
                resolved: id.resolved,
                fingerprint: id.fingerprint.clone(),
                verdict,
                block,
            });
        }
        let group_input: Vec<(i64, String, bool, String)> = members
            .iter()
            .map(|m| (m.pos, m.provider.clone(), m.resolved, m.fingerprint.clone()))
            .collect();
        let eg = endpoint_groups(&group_input);
        let mut records: Vec<AvailabilityRecord> = members
            .iter()
            .map(|m| {
                convert_to_availability_record(
                    m.pos,
                    &m.provider,
                    &m.model,
                    &m.engine,
                    *eg.group_of.get(&m.pos).unwrap_or(&0),
                    m.verdict.as_ref(),
                    &m.block,
                    self.utc_now,
                )
            })
            .collect();
        // D16: an outage recorded on the endpoint marks the whole group.
        let triggers: Vec<GroupTrigger> = records
            .iter()
            .filter(|r| {
                r.state == "out"
                    && ["auth", "quota", "quota-unknown-reset"].contains(&r.kind.as_str())
            })
            .map(|r| {
                (
                    r.group,
                    r.kind.clone(),
                    r.reason.clone(),
                    r.hit,
                    r.until,
                    r.short.clone(),
                )
            })
            .collect();
        for (group, kind, reason, hit, until, short) in triggers {
            for o in records.iter_mut() {
                if o.group == group && o.state != "out" {
                    o.state = "out".into();
                    o.kind = kind.clone();
                    o.reason = reason.clone();
                    o.hit = hit;
                    o.until = until;
                    o.short = short.clone();
                }
            }
        }
        records
    }

    fn build_codex_row(
        &self,
        name: &str,
        where_: &str,
        file_reason: &str,
        walk: &Option<Walk>,
        health_source: &str,
    ) -> (Row, RowVerdict) {
        let table_name = format!(
            "[{}]",
            config::format_toml_path(&["model_providers".into(), name.to_string()])
        );
        let (pt_found, pt_ok, pt_reason, pt_key) = if self.config.exists && self.config.ok {
            let pt = provider_table(&self.config, name);
            (pt.found, pt.ok, pt.reason, pt.table_key)
        } else {
            (false, false, String::new(), None)
        };
        let set_problem = if self.config.exists && self.config.ok {
            provider_set_problem(&self.config, name)
        } else {
            String::new()
        };
        let kind = if name == "openai" {
            "builtin"
        } else {
            "custom"
        };
        let mut endpoint = String::new();
        let mut wire = String::new();
        let mut table_state = "built in".to_string();
        let mut table_ok = true;
        let mut host = String::new();

        if pt_found {
            if pt_ok {
                let ep = provider_endpoint(
                    &self.config,
                    pt_key.as_deref().unwrap_or(""),
                    &table_name,
                    where_,
                );
                if !ep.error.is_empty() {
                    table_ok = false;
                    table_state = format!("unusable: {}", ep.error);
                } else {
                    table_state = "usable".into();
                    endpoint = if !ep.base_url.is_empty() {
                        strip_query(&ep.base_url)
                    } else {
                        "(default)".into()
                    };
                    if name == "openai" {
                        endpoint.push_str(" (user-defined table)");
                    }
                    wire = ep.wire_api;
                    host = ep.host;
                }
            } else {
                table_ok = false;
                table_state = format!("unusable: {pt_reason}");
            }
        } else if name == "openai" {
            endpoint = "builtin:openai".into();
            host = "builtin:openai".into();
            wire = "(built in)".into();
            if !self.openai_base_url.trim().is_empty() {
                let (curl, chost) = config::canonical_base_url(&self.openai_base_url);
                endpoint.push_str(&format!("+OPENAI_BASE_URL {}", strip_query(&curl)));
                host = chost;
            }
        } else {
            table_ok = false;
            table_state = "unusable: the config cannot be scanned".into();
        }

        let mut effort_models = EffortModels::None;
        let mut transport = "prompt-only".to_string();
        let vocab: String;
        match caps(&host).filter(|_| !host.is_empty()) {
            Some(cap) => {
                transport = cap.schema_transport.to_string();
                vocab = cap.vocabulary.to_string();
                effort_models = match cap.models {
                    Models::Any => EffortModels::Any,
                    Models::List(l) => {
                        EffortModels::List(l.iter().map(|s| s.to_string()).collect())
                    }
                };
            }
            None => vocab = "unknown (needs -NativeEffort)".into(),
        }

        // Recorded health of this endpoint (via a health-probe identity).
        let mut health: Option<EndpointHealth> = None;
        if file_reason.is_empty() && set_problem.is_empty() && table_ok {
            let probe = resolve_reviewer_identity(
                &self.config,
                name,
                "health-probe",
                &self.openai_base_url,
                "codex",
                &self.launcher,
            );
            if probe.resolved {
                health = Some(endpoint_health(
                    &self.consults,
                    &probe.fingerprint,
                    self.utc_now,
                ));
            }
        }

        let mut cred_text;
        let verdict;
        let mut row_verdict: Option<PreflightVerdict> = None;
        if !file_reason.is_empty() {
            cred_text = "not checked (config unreadable)".to_string();
            verdict = format!("unknown (config unreadable: {file_reason})");
        } else if !set_problem.is_empty() {
            cred_text = "not checked (providers could not be established)".to_string();
            verdict = format!("unknown (the providers could not be established: {set_problem})");
        } else if !table_ok {
            cred_text = "not checked (table unusable)".to_string();
            verdict = format!("unavailable (table {table_state})");
        } else {
            let probe = resolve_reviewer_identity(
                &self.config,
                name,
                "health-probe",
                &self.openai_base_url,
                "codex",
                &self.launcher,
            );
            let anonymous = self
                .roster
                .entries
                .iter()
                .any(|e| e.provider == *name && e.auth == "none");
            let v = self.preflight(&probe, health.as_ref(), &self.launcher, anonymous, true);
            // (wave 29b, E5) the plan of the label's first codex entry that names one: a usage
            // limit on another route of that plan
            let v = match self
                .roster
                .entries
                .iter()
                .find(|e| e.provider == *name && e.engine == "codex" && !e.plan.is_empty())
            {
                Some(pe) => self.plan_verdict(v, pe, &probe, true),
                None => v,
            };
            cred_text = v
                .credential
                .as_ref()
                .map(|c| c.detail.clone())
                .unwrap_or_else(|| "not checked (identity unresolved)".into());
            verdict = format_row_verdict(&v, "codex");
            row_verdict = Some(v);
        }
        let _ = &mut cred_text;

        let rv = row_verdict.unwrap_or_else(|| synthetic_verdict(&verdict));
        let last_limit = health
            .as_ref()
            .and_then(|h| h.last_limit.as_ref())
            .map(limit_json);
        let last_failure = health
            .as_ref()
            .and_then(|h| h.last_failure.as_ref())
            .map(failure_json);
        let roster_positions = c3_core::roster::positions_for(&self.roster.entries, name, "codex");
        let roster_selected = walk
            .as_ref()
            .and_then(|w| w.entry.as_ref())
            .map(|e| e.provider == *name && e.engine == "codex")
            .unwrap_or(false);

        let row = Row {
            name: name.to_string(),
            engine: "codex".into(),
            kind: kind.into(),
            endpoint,
            wire_api: wire,
            table: table_state,
            credentials: cred_text,
            effort_vocabulary: vocab,
            effort_models,
            schema_transport: transport,
            last_limit,
            last_failure,
            roster_position: roster_positions.first().copied(),
            roster_positions,
            roster_selected,
            verdict,
            health_source: health_source.to_string(),
        };
        (
            row,
            RowVerdict {
                verdict: rv,
                block: String::new(),
                engine: "codex".into(),
            },
        )
    }

    fn build_engine_row(
        &self,
        label: &str,
        engine: &str,
        walk: &Option<Walk>,
        health_source: &str,
    ) -> (Row, RowVerdict) {
        let engine_launcher = self.engine_launcher(engine);
        let model = self
            .roster
            .entries
            .iter()
            .find(|e| e.provider == label)
            .map(|e| e.model.clone())
            .unwrap_or_default();
        let probe =
            resolve_reviewer_identity(&self.config, label, &model, "", engine, &engine_launcher);
        let health = if probe.resolved {
            Some(endpoint_health(
                &self.consults,
                &probe.fingerprint,
                self.utc_now,
            ))
        } else {
            None
        };
        let v = self.preflight(&probe, health.as_ref(), &engine_launcher, false, true);
        // (wave 29b, E5) a usage limit on another route of the plan of the label's first entry
        let v = match self.roster.entries.iter().find(|e| e.provider == label) {
            Some(first) if !first.plan.is_empty() => self.plan_verdict(v, first, &probe, true),
            _ => v,
        };
        let cred_text = v
            .credential
            .as_ref()
            .map(|c| c.detail.clone())
            .unwrap_or_else(|| "not checked (identity unresolved)".into());
        let mut verdict = format_row_verdict(&v, engine);
        let launch_block = self.engine_launch_block(engine);
        if !launch_block.is_empty() {
            verdict = format!("unavailable (refused: {launch_block})");
        }

        // caps-v1 of the engine.
        let mut engine_vocab = format!("{engine} (tier in the model id)");
        let mut engine_models = EffortModels::Any;
        let mut engine_transport = "native".to_string();
        if let Some(cap) = caps(&format!("engine:{engine}")) {
            if cap.vocabulary != "model-tier" {
                engine_vocab = cap.vocabulary.to_string();
            }
            match cap.models {
                Models::Any => engine_models = EffortModels::Any,
                Models::List(l) => {
                    engine_models = EffortModels::List(l.iter().map(|s| s.to_string()).collect())
                }
            }
            engine_transport = cap.schema_transport.to_string();
        }

        let last_limit = health
            .as_ref()
            .and_then(|h| h.last_limit.as_ref())
            .map(limit_json);
        let last_failure = health
            .as_ref()
            .and_then(|h| h.last_failure.as_ref())
            .map(failure_json);
        let roster_positions = c3_core::roster::positions_for(&self.roster.entries, label, engine);
        let roster_selected = walk
            .as_ref()
            .and_then(|w| w.entry.as_ref())
            .map(|e| e.provider == label && e.engine == engine)
            .unwrap_or(false);

        // The http engine has no CLI launcher: its endpoint is the API base of the reviewer's
        // roster entry (`http (<base_url>)`), not a launcher path.
        let endpoint = if engine == "http" {
            let base = self
                .roster
                .http_reviewers
                .iter()
                .find(|hr| hr.provider == label)
                .map(|hr| hr.base_url.clone())
                .unwrap_or_else(|| c3_core::roster_ext::DEFAULT_BASE_URL.to_string());
            format!("http ({base})")
        } else {
            format!(
                "{engine} ({})",
                if !engine_launcher.is_empty() {
                    engine_launcher.clone()
                } else {
                    "launcher not found".into()
                }
            )
        };

        let row = Row {
            name: label.to_string(),
            engine: engine.to_string(),
            kind: format!("engine {engine}"),
            endpoint,
            wire_api: String::new(),
            table: "n/a".into(),
            credentials: cred_text,
            effort_vocabulary: engine_vocab,
            effort_models: engine_models,
            schema_transport: engine_transport,
            last_limit,
            last_failure,
            roster_position: roster_positions.first().copied(),
            roster_positions,
            roster_selected,
            verdict,
            health_source: health_source.to_string(),
        };
        (
            row,
            RowVerdict {
                verdict: v,
                block: launch_block,
                engine: engine.to_string(),
            },
        )
    }
}

// --------------------------------------------------------------------------- formatting

pub(crate) fn strip_query(url: &str) -> String {
    match url.find('?') {
        Some(i) => format!("{}?...", &url[..i]),
        None => url.to_string(),
    }
}

/// The provider's config table (`Runner::provider_table_ref` as a free helper for the consult
/// preflight).
pub(crate) fn resolve_provider_table<'a>(config: &'a CodexConfig, name: &str) -> Option<&'a Table> {
    if config.exists && config.ok {
        let pt = provider_table(config, name);
        if pt.found {
            if let Some(key) = pt.table_key {
                return config.table_by_key(&key);
            }
        }
    }
    None
}

/// The credential check for a codex-engine identity, for the consult preflight (no login cache).
/// Mirrors `Runner::provider_credential`: openai-auth (`openai` or `requires_openai_auth`) runs
/// `codex login status`; a third-party provider checks its `env_key` (`env X not set` when it is
/// unset), then a bearer token, then a roster-declared `auth = none`.
pub(crate) fn identity_credential(
    config: &CodexConfig,
    id: &ReviewerIdentity,
    launcher: &str,
    anonymous: bool,
    login_timeout: u64,
) -> CredentialResult {
    let name = &id.provider;
    let table = resolve_provider_table(config, name);
    let mut openai_auth = name == "openai";
    if !openai_auth {
        if let Some(t) = table {
            if let Some(e) = t.entry("requires_openai_auth") {
                if e.supported && e.kind == "boolean" && e.value.as_deref() == Some("true") {
                    openai_auth = true;
                }
            }
        }
    }
    if openai_auth {
        return get_codex_login_status(launcher, login_timeout);
    }
    let table = match table {
        Some(t) => t,
        None => return CredentialResult::unknown(format!("no [model_providers.{name}] table")),
    };
    let ek = config::get_toml_string(Some(table), "env_key");
    let bt = config::get_toml_string(Some(table), "experimental_bearer_token");
    let mut env_name = String::new();
    if ek.present && ek.reason.is_empty() {
        env_name = ek.value.trim().to_string();
    }
    if !env_name.is_empty() {
        if let Ok(v) = std::env::var(&env_name) {
            if !v.trim().is_empty() {
                return CredentialResult::ok(format!("env {env_name} set"));
            }
        }
    }
    if bt.present && bt.reason.is_empty() && !bt.value.is_empty() {
        return CredentialResult::ok("bearer token in config");
    }
    if !env_name.is_empty() {
        return CredentialResult::missing(format!("env {env_name} not set"));
    }
    if anonymous {
        return CredentialResult::ok("declared anonymous in the roster");
    }
    CredentialResult::missing("no env_key/bearer token in the table")
}

/// `Format-RowVerdict`.
fn format_row_verdict(v: &PreflightVerdict, engine: &str) -> String {
    if v.state == "available" {
        return "available".into();
    }
    if v.state == "unavailable" {
        if let Some(c) = v
            .credential
            .as_ref()
            .filter(|_| v.kind == "credentials" && engine != "codex")
        {
            return format!("unavailable ({})", c.reason);
        }
        return format!("unavailable ({})", v.reason);
    }
    if v.kind == "unknown" {
        if let Some(c) = &v.credential {
            if !c.reason.is_empty() {
                return format!("unknown ({})", c.reason);
            }
        }
    }
    let reason = v.reason.strip_prefix("unknown: ").unwrap_or(&v.reason);
    format!("unknown ({reason})")
}

/// A synthetic verdict for the config-unreadable / set-problem / table-unusable rows,
/// so the no-roster availability view can render them.
fn synthetic_verdict(verdict: &str) -> PreflightVerdict {
    let state = if verdict.starts_with("unknown") {
        "unknown"
    } else {
        "unavailable"
    };
    // reason = verdict minus the `<word> (...)` wrapper
    let reason = {
        let re = regex::Regex::new(r"^\w+ \((.*)\)$").unwrap();
        re.captures(verdict)
            .map(|c| c[1].to_string())
            .unwrap_or_else(|| verdict.to_string())
    };
    PreflightVerdict {
        state: state.into(),
        preflight: String::new(),
        reason,
        refusal: String::new(),
        label: String::new(),
        kind: "config".into(),
        hit: None,
        until: None,
        credential: None,
        burst: false,
        plan_quota: None,
    }
}

fn limit_json(r: &c3_core::health::Record) -> Value {
    json!({
        "when": r.when,
        "message": r.message,
        "retry_after": if r.retry_after_iso.is_empty() { Value::Null } else { Value::String(r.retry_after_iso.clone()) },
    })
}

fn failure_json(r: &c3_core::health::Record) -> Value {
    json!({
        "class": r.class,
        "code": r.code,
        "when": r.when,
        "message": r.message,
        "retry_after": if r.retry_after_iso.is_empty() { Value::Null } else { Value::String(r.retry_after_iso.clone()) },
    })
}

fn effort_models_json(e: &EffortModels) -> Value {
    match e {
        EffortModels::Any => json!("any"),
        EffortModels::List(l) => json!(l),
        EffortModels::None => Value::Null,
    }
}

fn effort_column(row: &Row) -> String {
    let vocab = &row.effort_vocabulary;
    match (&row.effort_models, row.engine.as_str()) {
        (EffortModels::List(l), e) if e != "codex" => {
            format!("{vocab} ({} declared models)", l.len())
        }
        (_, e) if e != "codex" => vocab.clone(),
        (EffortModels::Any, _) => format!("{vocab} (any model)"),
        (EffortModels::List(l), _) => format!("{vocab} ({} declared models)", l.len()),
        (EffortModels::None, _) => vocab.clone(),
    }
}

fn print_rows_json(rows: &[Row]) {
    let arr: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "name": r.name,
                "engine": r.engine,
                "kind": r.kind,
                "endpoint": r.endpoint,
                "wire_api": r.wire_api,
                "table": r.table,
                "credentials": r.credentials,
                "effort_vocabulary": r.effort_vocabulary,
                "effort_models": effort_models_json(&r.effort_models),
                "schema_transport": r.schema_transport,
                "last_limit": r.last_limit.clone().unwrap_or(Value::Null),
                "last_failure": r.last_failure.clone().unwrap_or(Value::Null),
                "roster_position": r.roster_position.map(Value::from).unwrap_or(Value::Null),
                "roster_positions": r.roster_positions,
                "roster_selected": r.roster_selected,
                "verdict": r.verdict,
                "health_source": r.health_source,
            })
        })
        .collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&Value::Array(arr)).unwrap()
    );
}

#[allow(clippy::too_many_arguments)]
fn print_table(
    ctx: &Ctx,
    rows: &[Row],
    where_: &str,
    clock: &DateTime<FixedOffset>,
    health_source: &str,
    walk: &Option<Walk>,
    availability_line: &str,
) {
    let not_found = if !ctx.config.exists {
        " (not found - Codex runs on its built-in defaults)"
    } else {
        ""
    };
    println!("codex config: {where_}{not_found}");
    let read_at = clock.with_timezone(&Local).format("%Y-%m-%d %H:%M");
    println!(
        "endpoint health: {health_source}, read at {read_at} - the ledgers of THIS repository"
    );

    let with_roster = ctx.roster.exists;
    let cols: Vec<&str> = if with_roster {
        vec![
            "verdict",
            "name",
            "roster",
            "kind",
            "endpoint",
            "credentials",
            "effort",
            "limit",
        ]
    } else {
        vec![
            "verdict",
            "name",
            "kind",
            "endpoint",
            "credentials",
            "effort",
            "limit",
        ]
    };
    // header + rows as maps of col -> string
    let mut lines: Vec<HashMap<&str, String>> = Vec::new();
    let mut header = HashMap::new();
    for (k, v) in [
        ("verdict", "VERDICT"),
        ("name", "PROVIDER"),
        ("roster", "ROSTER"),
        ("kind", "KIND"),
        ("endpoint", "ENDPOINT"),
        ("credentials", "CREDENTIALS"),
        ("effort", "EFFORT"),
        ("limit", "LAST FAILURE"),
    ] {
        header.insert(k, v.to_string());
    }
    lines.push(header);
    for r in rows {
        let positions: Vec<String> = ctx
            .roster
            .entries
            .iter()
            .filter(|e| e.provider == r.name && e.engine == r.engine)
            .map(|e| e.position.to_string())
            .collect();
        let mut failure_label = String::new();
        if let Some(f) = &r.last_failure {
            failure_label = f["class"].as_str().unwrap_or("").to_string();
            if f["class"].as_str() == Some("quota") {
                if let Some(ra) = f["retry_after"].as_str() {
                    failure_label = format!("quota until {ra}");
                }
            }
        }
        let limit = match &r.last_failure {
            Some(f) => format!(
                "{failure_label}: {} - {}",
                f["when"].as_str().unwrap_or(""),
                f["message"].as_str().unwrap_or("")
            ),
            None => "-".into(),
        };
        let mut m = HashMap::new();
        m.insert("verdict", r.verdict.clone());
        m.insert("name", r.name.clone());
        m.insert(
            "roster",
            if positions.is_empty() {
                "-".into()
            } else {
                positions.join(",")
            },
        );
        m.insert("kind", r.kind.clone());
        m.insert(
            "endpoint",
            if r.endpoint.is_empty() {
                "-".into()
            } else {
                r.endpoint.clone()
            },
        );
        m.insert("credentials", r.credentials.clone());
        m.insert("effort", effort_column(r));
        m.insert("limit", limit);
        lines.push(m);
    }
    let mut width: HashMap<&str, usize> = HashMap::new();
    for col in &cols {
        let w = lines
            .iter()
            .map(|l| l.get(col).map(|s| s.chars().count()).unwrap_or(0))
            .max()
            .unwrap_or(0);
        width.insert(col, w);
    }
    for l in &lines {
        let parts: Vec<String> = cols
            .iter()
            .map(|col| pad_right(l.get(col).cloned().unwrap_or_default(), width[col]))
            .collect();
        println!("{}", parts.join("  ").trim_end());
    }
    if with_roster {
        let path = &ctx.roster.path;
        if let (Some(entry), Some(id)) = (
            walk.as_ref().and_then(|w| w.entry.as_ref()),
            walk.as_ref().and_then(|w| w.identity.as_ref()),
        ) {
            let _ = entry;
            let mut line = format!(
                "roster: {path} -> would select {}",
                format_reviewer_lineage(&id.provider, &id.model, &id.engine)
            );
            let skipped = &walk.as_ref().unwrap().skipped;
            if !skipped.is_empty() {
                line.push_str(&format!(" (skipped: {})", format_roster_skips(skipped)));
            }
            println!("{line}");
        } else {
            let skipped = walk.as_ref().map(|w| w.skipped.clone()).unwrap_or_default();
            println!(
                "roster: {path} -> no entry is available (skipped: {})",
                format_roster_skips(&skipped)
            );
        }
    }
    let av = availability_line
        .strip_prefix("codex-consult: ")
        .unwrap_or(availability_line);
    println!("availability: {av}");
}

fn pad_right(s: String, w: usize) -> String {
    let len = s.chars().count();
    if len >= w {
        s
    } else {
        format!("{s}{}", " ".repeat(w - len))
    }
}

fn print_short_json(
    line: &str,
    health_source: &str,
    records: &[AvailabilityRecord],
    roster: &Roster,
) {
    let entries: Vec<Value> = records
        .iter()
        .map(|r| {
            json!({
                "position": r.position,
                "provider": r.provider,
                "model": r.model,
                "engine": r.engine,
                "lineage": r.lineage,
                "group": r.group,
                "state": r.state,
                "kind": r.kind,
                "reason": r.reason,
                "short": r.short,
                "hit": r.hit.map(|h| Value::String(format_offset_iso(h))).unwrap_or(Value::Null),
                "until": r.until.map(|u| Value::String(format_offset_iso(u))).unwrap_or(Value::Null),
            })
        })
        .collect();
    let obj = json!({
        "line": line,
        "health_source": health_source,
        "total": records.len(),
        "available": records.iter().filter(|r| r.state == "available").count(),
        "out": records.iter().filter(|r| r.state == "out").count(),
        "not_checked": records.iter().filter(|r| r.state == "not checked").count(),
        "roster": if roster.exists { Value::String(roster.path.clone()) } else { Value::Null },
        "entries": entries,
    });
    println!("{}", serde_json::to_string_pretty(&obj).unwrap());
}

// --------------------------------------------------------------------------- runtime helpers

pub(crate) fn get_codex_home() -> String {
    if let Ok(h) = std::env::var("CODEX_HOME") {
        if !h.is_empty() {
            return win_sep(h);
        }
    }
    let home = std::env::var("HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("USERPROFILE").ok().filter(|s| !s.is_empty()));
    match home {
        Some(h) => win_sep(Path::new(&h).join(".codex").to_string_lossy().to_string()),
        None => String::new(),
    }
}

/// On Windows, `Join-Path` renders `\` separators; match it so a `CODEX_HOME` given
/// with `/` still displays and compares the way the PowerShell bridge prints it.
fn win_sep(s: String) -> String {
    if cfg!(windows) {
        s.replace('/', "\\")
    } else {
        s
    }
}

pub(crate) fn get_codex_config_path() -> String {
    let home = get_codex_home();
    if home.is_empty() {
        return String::new();
    }
    Path::new(&home)
        .join("config.toml")
        .to_string_lossy()
        .to_string()
}

pub(crate) fn read_codex_config(path: &str) -> CodexConfig {
    if path.is_empty() || !Path::new(path).exists() {
        return config_not_found(path);
    }
    if !Path::new(path).is_file() {
        return config_unreadable(
            path,
            &format!("the Codex config '{path}' could not be read: it is not a file"),
        );
    }
    match std::fs::read(path) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes).to_string();
            scan_config_text(path, &text)
        }
        Err(e) => config_unreadable(
            path,
            &format!(
                "the Codex config '{path}' could not be read: {}",
                c3_core::one_line(&e.to_string())
            ),
        ),
    }
}

pub(crate) fn get_consult_clock_peek() -> Result<DateTime<FixedOffset>, String> {
    let raw = std::env::var("CODEX_CONSULT_NOW").unwrap_or_default();
    if raw.trim().is_empty() {
        return Ok(Local::now().fixed_offset());
    }
    let items: Vec<&str> = raw
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let iso = regex::Regex::new(
        r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}(:[0-9]{2})?([+-][0-9]{2}:[0-9]{2}|Z)$",
    )
    .unwrap();
    let mut parsed: Vec<DateTime<FixedOffset>> = Vec::new();
    for item in &items {
        if !iso.is_match(item) {
            return Err(malformed_now(&raw, item));
        }
        // normalize_iso adds ":00" seconds when the token omits them
        match DateTime::parse_from_rfc3339(&normalize_iso(item)) {
            Ok(dt) => parsed.push(dt),
            Err(_) => return Err(malformed_now(&raw, item)),
        }
    }
    // -Peek, first evaluation (call index 0)
    Ok(parsed[0])
}

fn normalize_iso(s: &str) -> String {
    // add ":00" seconds if absent before offset/Z
    let re = regex::Regex::new(r"^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2})([+-]\d{2}:\d{2}|Z)$").unwrap();
    if let Some(c) = re.captures(s) {
        return format!("{}:00{}", &c[1], &c[2]);
    }
    s.to_string()
}

fn malformed_now(raw: &str, item: &str) -> String {
    format!("CODEX_CONSULT_NOW='{raw}' is malformed: bad token '{item}' (a test hook: ISO timestamps with an offset, e.g. 2026-09-24T13:59:59+08:00, comma-separated)")
}

pub fn resolve_repo_root(cwd: &Path) -> PathBuf {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["rev-parse", "--show-toplevel"])
        .output();
    if let Ok(o) = out {
        if o.status.success() {
            let first = String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !first.is_empty() {
                if let Ok(p) = std::fs::canonicalize(&first) {
                    return dunce_simplify(p);
                }
                return PathBuf::from(first);
            }
        }
    }
    cwd.to_path_buf()
}

// Strip the Windows verbatim prefix that canonicalize adds.
pub fn dunce_simplify(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        p
    }
}

pub fn resolve_collab_root(repo_root: &Path, collab_dir: &str) -> PathBuf {
    let dir = if collab_dir.is_empty() {
        ".collab"
    } else {
        collab_dir
    };
    let p = Path::new(dir);
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else {
        repo_root.join(p)
    };
    // GetFullPath-like normalisation
    std::fs::canonicalize(&joined)
        .map(dunce_simplify)
        .unwrap_or(joined)
}

fn count_task_ledgers(collab_root: &Path) -> usize {
    if !collab_root.is_dir() {
        return 0;
    }
    std::fs::read_dir(collab_root)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir() && e.path().join("sessions.json").is_file())
                .count()
        })
        .unwrap_or(0)
}

/// (wave 26b, D13) The machine-wide health file's endpoint records as synthetic consults, so a
/// repository's endpoint-health checks fold in what other repositories of the machine recorded.
/// Empty when `CODEX_CONSULT_HEALTH=none`, no file, or an unreadable one.
pub(crate) fn machine_health_consults() -> Vec<Value> {
    match c3_core::health::machine_health_path(&get_codex_home()) {
        Some(p) => c3_core::health::machine_endpoint_consults_all(&p),
        None => Vec::new(),
    }
}

/// [`read_all_task_consults`] plus the machine-wide health records (wave 26b, D13), for endpoint
/// -health decisions (a preflight or a roster walk); never for ratings/routing.
pub(crate) fn read_all_task_consults_health(collab_root: &Path) -> Vec<Value> {
    let mut all = read_all_task_consults(collab_root);
    all.extend(machine_health_consults());
    all
}

pub(crate) fn read_all_task_consults(collab_root: &Path) -> Vec<Value> {
    let mut all = Vec::new();
    if !collab_root.is_dir() {
        return all;
    }
    let rd = match std::fs::read_dir(collab_root) {
        Ok(rd) => rd,
        Err(_) => return all,
    };
    for entry in rd.filter_map(|e| e.ok()) {
        let p = entry.path();
        if !p.is_dir() {
            continue;
        }
        let f = p.join("sessions.json");
        if !f.is_file() {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&f) {
            if let Ok(data) = serde_json::from_str::<Value>(&text) {
                if let Some(consults) = data
                    .get("codex")
                    .and_then(|c| c.get("consults"))
                    .and_then(|c| c.as_array())
                {
                    for c in consults {
                        if !c.is_null() {
                            all.push(c.clone());
                        }
                    }
                }
            }
        }
    }
    all
}

// ---- roster path + reading

fn get_roster_path() -> (String, bool, bool) {
    let p = std::env::var("CODEX_CONSULT_ROSTER").unwrap_or_default();
    let p = p.trim().to_string();
    if p.eq_ignore_ascii_case("none") {
        return (String::new(), true, true);
    }
    if !p.is_empty() {
        let path = if Path::new(&p).is_absolute() {
            p
        } else {
            std::env::current_dir()
                .unwrap_or_default()
                .join(&p)
                .to_string_lossy()
                .to_string()
        };
        return (path, true, false);
    }
    let home = get_codex_home();
    let default = if home.is_empty() {
        String::new()
    } else {
        Path::new(&home)
            .join("codex-consult-roster.json")
            .to_string_lossy()
            .to_string()
    };
    (default, false, false)
}

pub(crate) fn read_reviewer_roster() -> Result<Roster, String> {
    let (path, from_env, disabled) = get_roster_path();
    let mut r = Roster {
        path: path.clone(),
        disabled,
        ..Default::default()
    };
    if disabled {
        return Ok(r);
    }
    if !path.is_empty() && from_env && !Path::new(&path).exists() {
        return Err(format!(
            "the reviewer roster '{path}' named by CODEX_CONSULT_ROSTER does not exist; unset CODEX_CONSULT_ROSTER to use <codex home>/codex-consult-roster.json when it exists, or set it to none for no roster."
        ));
    }
    if path.is_empty() || !Path::new(&path).exists() {
        return Ok(r);
    }
    r.exists = true;
    if !Path::new(&path).is_file() {
        return Err(roster_refusal(&path, "it is not a file"));
    }
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    if text.trim().is_empty() {
        return Err(roster_refusal(&path, "it is empty or could not be read"));
    }
    let home = std::env::var("HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("USERPROFILE").ok());
    let mut validated = validate_roster(&path, &text, home.as_deref());
    if !validated.error.is_empty() {
        return Err(validated.error);
    }
    // (M7b-b) The C3-only `http` reviewers under the top-level `ext.c3.reviewers` extension: the
    // plugin validates `ext` as an object and ignores its content, so http reviewers live there
    // and never among the plugin-visible entries. C3 validates them and appends them AFTER the
    // plugin's entries (positions continue the numbering), so the panel and `c3 providers` — which
    // iterate `roster.entries` — see them exactly as they see the plugin's own entries.
    if let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) {
        match c3_core::roster_ext::parse_ext_reviewers(&data, validated.entries.len()) {
            Ok(http_reviewers) => {
                for hr in &http_reviewers {
                    validated.entries.push(hr.to_entry());
                }
                validated.http_reviewers = http_reviewers;
            }
            Err(why) => return Err(roster_refusal(&path, &why)),
        }
    }
    Ok(validated)
}

// ---- launcher discovery

fn find_on_path(name: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    let has_ext = Path::new(name).extension().is_some();
    let pathext: Vec<String> = if cfg!(windows) && !has_ext {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
            .split(';')
            .map(|s| s.to_string())
            .collect()
    } else {
        vec![]
    };
    for dir in std::env::split_paths(&path) {
        let direct = dir.join(name);
        if direct.is_file() {
            return Some(direct.to_string_lossy().to_string());
        }
        for ext in &pathext {
            let cand = dir.join(format!("{name}{ext}"));
            if cand.is_file() {
                return Some(cand.to_string_lossy().to_string());
            }
        }
    }
    None
}

fn resolve_from_sources(sources: &[(String, &str)]) -> Result<Option<String>, String> {
    for (candidate, label) in sources {
        if candidate.is_empty() {
            continue;
        }
        if Path::new(candidate).exists() {
            let abs = std::fs::canonicalize(candidate)
                .map(dunce_simplify)
                .unwrap_or_else(|_| PathBuf::from(candidate));
            return Ok(Some(abs.to_string_lossy().to_string()));
        }
        if let Some(found) = find_on_path(candidate) {
            return Ok(Some(found));
        }
        return Err(format!(
            "{label} '{candidate}' is not a file and not an application on PATH."
        ));
    }
    Ok(None)
}

pub fn resolve_codex_launcher(explicit: &str) -> Result<String, String> {
    let sources = vec![
        (explicit.to_string(), "-CodexExe"),
        (
            std::env::var("CODEX_CONSULT_EXE").unwrap_or_default(),
            "CODEX_CONSULT_EXE",
        ),
    ];
    if let Some(l) = resolve_from_sources(&sources)? {
        return Ok(l);
    }
    for name in launcher_names("codex") {
        if let Some(found) = find_on_path(name) {
            return Ok(found);
        }
    }
    Ok(String::new())
}

pub(crate) fn resolve_engine_launcher(
    engine: &str,
    explicit: &str,
) -> Result<Option<String>, String> {
    if engine.is_empty() || engine == "codex" {
        return Ok(Some(resolve_codex_launcher(explicit)?));
    }
    let spec = match engine_spec(engine) {
        Some(s) => s,
        None => return Ok(None),
    };
    let sources = vec![
        (explicit.to_string(), "-EngineExe"),
        (
            std::env::var(spec.exe_env).unwrap_or_default(),
            spec.exe_env,
        ),
    ];
    if let Some(l) = resolve_from_sources(&sources)? {
        return Ok(Some(l));
    }
    for name in launcher_names(engine) {
        if let Some(found) = find_on_path(name) {
            return Ok(Some(found));
        }
    }
    for (env, rel) in install_launchers(engine) {
        if let Ok(base) = std::env::var(env) {
            if !base.trim().is_empty() {
                let cand = Path::new(&base).join(rel);
                if cand.is_file() {
                    let abs = std::fs::canonicalize(&cand)
                        .map(dunce_simplify)
                        .unwrap_or(cand);
                    return Ok(Some(abs.to_string_lossy().to_string()));
                }
            }
        }
    }
    Ok(None)
}

fn resolve_engine_exe_binding(roster: &Roster, provider: &str) -> Result<String, String> {
    let others: Vec<&str> = c3_core::lineage::ENGINE_NAMES
        .iter()
        .copied()
        .filter(|e| *e != "codex")
        .collect();
    if !provider.is_empty() {
        if let Some(pe) = find_roster_entry(roster, provider, "") {
            if pe.engine != "codex" {
                return Ok(pe.engine.clone());
            }
            return Err(format!(
                "-EngineExe: the roster entry {} for -Provider {provider} is engine codex, which takes -CodexExe",
                pe.position
            ));
        }
    }
    let mut used: Vec<String> = Vec::new();
    if roster.exists {
        for e in &roster.entries {
            if e.engine != "codex" && !used.contains(&e.engine) {
                used.push(e.engine.clone());
            }
        }
    }
    if used.len() == 1 {
        return Ok(used[0].clone());
    }
    if used.len() > 1 {
        Err(format!(
            "-EngineExe is ambiguous: the reviewer roster has entries of the engines {}; pass -Engine <{}> to name the one it launches",
            used.join(" and "),
            others.join("|")
        ))
    } else {
        Err(format!(
            "-EngineExe names the launcher of an engine other than codex: pass -Engine <{}> with it",
            others.join("|")
        ))
    }
}

pub(crate) fn find_roster_entry<'a>(
    roster: &'a Roster,
    provider: &str,
    model: &str,
) -> Option<&'a RosterEntry> {
    if !roster.exists {
        return None;
    }
    let by_provider: Vec<&RosterEntry> = roster
        .entries
        .iter()
        .filter(|e| e.provider == provider)
        .collect();
    if by_provider.is_empty() {
        return None;
    }
    if !model.is_empty() && by_provider.len() > 1 {
        if let Some(exact) = by_provider.iter().find(|e| e.model == model) {
            return Some(exact);
        }
    }
    Some(by_provider[0])
}

// ---- credential subprocess checks

pub(crate) fn run_with_timeout(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Option<(i32, String, String)> {
    // .cmd / .bat launchers go through cmd.exe on Windows.
    let (prog, all_args): (String, Vec<String>) = if cfg!(windows)
        && Path::new(program)
            .extension()
            .map(|e| {
                let e = e.to_string_lossy().to_lowercase();
                e == "cmd" || e == "bat"
            })
            .unwrap_or(false)
    {
        let mut v = vec!["/c".to_string(), program.to_string()];
        v.extend(args.iter().map(|s| s.to_string()));
        ("cmd".to_string(), v)
    } else {
        (
            program.to_string(),
            args.iter().map(|s| s.to_string()).collect(),
        )
    };
    let mut cmd = Command::new(&prog);
    cmd.args(&all_args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // (wave 27 / 27b) a launcher probe (login status, models, `--version`) gets no host marker.
    crate::engines::scrub_host_markers(&mut cmd);
    let mut child = cmd.spawn().ok()?;
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    let out_h = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = out.read_to_end(&mut s);
        String::from_utf8_lossy(&s).to_string()
    });
    let err_h = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = err.read_to_end(&mut s);
        String::from_utf8_lossy(&s).to_string()
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let o = out_h.join().unwrap_or_default();
                let e = err_h.join().unwrap_or_default();
                return Some((status.code().unwrap_or(-1), o, e));
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return None,
        }
    }
}

pub(crate) fn get_codex_login_status(launcher: &str, timeout_sec: u64) -> CredentialResult {
    // (wave 27c, D4) the launcher probe never fails open. C3 scrubs the host markers from the probe
    // CHILD's environment directly, so a simulated start-info scrub failure alone
    // (`CODEX_CONSULT_TEST_PROBE_SCRUB_FAIL`) does NOT stop the probe — it still runs from a clean
    // environment (D4 F30-4). Only when the markers ALSO cannot be hidden at all
    // (`CODEX_CONSULT_TEST_HIDE_FAIL`, the transactional-hide failure) is there no safe way to start
    // the probe: it is SKIPPED and the credential is `unknown` (a real run is then refused).
    let scrub_fail = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_PROBE_SCRUB_FAIL")
        .map(|v| v.trim() == "1")
        .unwrap_or(false);
    if scrub_fail && crate::engines::host_marker_hide_failure().is_some() {
        return CredentialResult::unknown(
            "not checked - `codex login status` was skipped: the start-info block could not be scrubbed (test hook CODEX_CONSULT_TEST_PROBE_SCRUB_FAIL)",
        );
    }
    if launcher.is_empty() {
        return CredentialResult::unknown(
            "codex CLI not found, `codex login status` could not run",
        );
    }
    match run_with_timeout(
        launcher,
        &["login", "status"],
        Duration::from_secs(timeout_sec),
    ) {
        None => CredentialResult::unknown(format!(
            "`codex login status` did not finish within {timeout_sec} s"
        )),
        Some((code, out, err)) => {
            let merged = format!("{out}\n{err}");
            let lines: Vec<String> = merged
                .split(['\r', '\n'])
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            let logged_in = lines.iter().find(|l| l.contains("Logged in"));
            if code == 0 {
                if let Some(li) = logged_in {
                    return CredentialResult::ok(li.clone());
                }
            }
            let first = lines
                .first()
                .cloned()
                .unwrap_or_else(|| format!("exit {code}"));
            CredentialResult::missing(first)
        }
    }
}

pub(crate) fn get_agy_models_status(launcher: &str, timeout_sec: u64) -> CredentialResult {
    if launcher.is_empty() {
        return CredentialResult::missing("agy CLI not found on PATH");
    }
    match run_with_timeout(launcher, &["models"], Duration::from_secs(timeout_sec)) {
        None => CredentialResult::unknown(format!(
            "`agy models` did not finish within {timeout_sec} s"
        )),
        Some((code, out, err)) => {
            let out_lines: Vec<&str> = out
                .split(['\r', '\n'])
                .filter(|l| !l.trim().is_empty())
                .collect();
            let merged = format!("{out}\n{err}");
            let all_lines: Vec<String> = merged
                .split(['\r', '\n'])
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            let model_re = regex::Regex::new(r"^\S+\t").unwrap();
            let models = out_lines.iter().filter(|l| model_re.is_match(l)).count();
            if code == 0 && models >= 1 {
                return CredentialResult::ok(format!("signed in ({models} models)"));
            }
            let auth_re =
                regex::Regex::new(r"(?i)log ?in|sign in|signed in|\bauth|unauthenticated").unwrap();
            if let Some(al) = all_lines.iter().find(|l| auth_re.is_match(l)) {
                return CredentialResult::missing(format!("`agy models`: {al}"));
            }
            let first = all_lines
                .last()
                .cloned()
                .unwrap_or_else(|| "no output".into());
            CredentialResult::unknown(format!(
                "`agy models` exit {code} without a model list ({first})"
            ))
        }
    }
}

// ---- muse local sign-in

fn muse_auth_path() -> String {
    let h = if cfg!(windows) {
        std::env::var("USERPROFILE").ok().filter(|s| !s.is_empty())
    } else {
        std::env::var("HOME").ok().filter(|s| !s.is_empty())
    };
    let h = h.or_else(|| std::env::var("HOME").ok().filter(|s| !s.is_empty()));
    match h {
        Some(h) => Path::new(&h)
            .join(".config")
            .join("muse")
            .join("auth.json")
            .to_string_lossy()
            .to_string(),
        None => String::new(),
    }
}

pub(crate) struct MuseInfo {
    state: State,
    reason: String,
    cause: String,
    mechanism: String,
}

pub(crate) fn muse_credential_info() -> MuseInfo {
    const SHOWN: &str = "~/.config/muse/auth.json";
    let backend = std::env::var("TBH_CREDENTIAL_BACKEND")
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    let path = muse_auth_path();
    let mut info = MuseInfo {
        state: State::Unknown,
        reason: String::new(),
        cause: String::new(),
        mechanism: String::new(),
    };
    if backend != "file" {
        let b = if backend.is_empty() {
            "not set".to_string()
        } else {
            format!("'{backend}'")
        };
        info.cause = format!("TBH_CREDENTIAL_BACKEND is {b}: the keychain backend cannot be read");
        info.reason = format!("sign-in not checkable: TBH_CREDENTIAL_BACKEND is {b} (the keychain backend cannot be read; set TBH_CREDENTIAL_BACKEND=file - required on Windows - and run `muse login`)");
        return info;
    }
    if path.is_empty() {
        info.cause = "no home directory (USERPROFILE / HOME)".into();
        info.reason = "sign-in not checkable: no home directory (USERPROFILE / HOME)".into();
        return info;
    }
    if !Path::new(&path).is_file() {
        info.state = State::Missing;
        info.cause = format!("{SHOWN} does not exist");
        info.reason = format!("not signed in: {SHOWN} does not exist (run `muse login` with TBH_CREDENTIAL_BACKEND=file)");
        return info;
    }
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    if text.trim().is_empty() {
        info.cause = format!("{SHOWN} is empty or could not be read");
        info.reason = format!("sign-in not checkable: {SHOWN} is empty or could not be read");
        return info;
    }
    let data: Value = match serde_json::from_str::<Value>(&text) {
        Ok(v) if v.is_object() => v,
        Ok(_) => {
            info.cause = format!("{SHOWN} is not a JSON object");
            info.reason = format!("sign-in not checkable: {SHOWN} is not a JSON object");
            return info;
        }
        Err(_) => {
            info.cause = format!("{SHOWN} does not parse as JSON");
            info.reason = format!("sign-in not checkable: {SHOWN} does not parse as JSON");
            return info;
        }
    };
    let meta = data
        .get("providers")
        .filter(|p| p.is_object())
        .and_then(|p| p.get("meta"))
        .filter(|m| m.is_object());
    match meta {
        None => {
            info.state = State::Missing;
            info.cause = format!("{SHOWN} has no Meta sign-in (providers.meta)");
            info.reason = format!(
                "not signed in: {SHOWN} has no Meta sign-in (providers.meta; run `muse login`)"
            );
        }
        Some(m) => match m
            .get("mechanism")
            .and_then(|x| x.as_str())
            .filter(|s| !s.trim().is_empty())
        {
            None => {
                info.cause = format!("providers.meta in {SHOWN} names no mechanism");
                info.reason =
                    format!("sign-in not checkable: providers.meta in {SHOWN} names no mechanism");
            }
            Some(mech) => {
                info.state = State::Ok;
                let re = regex::Regex::new(r"^[A-Za-z][A-Za-z0-9_.-]{0,31}$").unwrap();
                info.mechanism = if re.is_match(mech) {
                    mech.to_string()
                } else {
                    "unrecognized".to_string()
                };
                info.reason = format!(
                    "signed in ({SHOWN}: providers.meta, mechanism {})",
                    info.mechanism
                );
            }
        },
    }
    info
}

fn get_muse_sign_in() -> CredentialResult {
    let c = muse_credential_info();
    CredentialResult::new(c.state, c.reason)
}

pub(crate) fn get_muse_launch_block() -> String {
    const SHOWN: &str = "~/.config/muse/auth.json";
    for name in ["META_API_KEY", "MODEL_API_KEY"] {
        if let Ok(v) = std::env::var(name) {
            if !v.trim().is_empty() {
                return format!("{name} is set: a muse run would bill per token instead of the Muse Code subscription; unset it (the muse process would inherit it)");
            }
        }
    }
    let c = muse_credential_info();
    if c.state == State::Ok && c.mechanism == "oauth" {
        return String::new();
    }
    if !c.mechanism.is_empty() && c.mechanism != "oauth" {
        return format!("the Muse sign-in in {SHOWN} uses mechanism '{}', not oauth: a muse run would not bill the Muse Code subscription; sign in with `muse login`", c.mechanism);
    }
    let cause = if !c.cause.is_empty() {
        c.cause
    } else {
        c.reason
    };
    format!("the Muse sign-in is not established as oauth ({cause}): a muse run might bill per token instead of the Muse Code subscription; set TBH_CREDENTIAL_BACKEND=file and run `muse login`")
}

/// The engine credential for the CONSULT preflight (no `-NoNetwork`; the health short-circuit
/// still applies): a recorded usable reply on this endpoint answers without a network check;
/// muse reads `auth.json`; agy runs `agy models` (45 s, or `CODEX_CONSULT_TEST_LOGIN_TIMEOUT`).
pub(crate) fn engine_consult_credential(
    engine: &str,
    launcher: &str,
    health: Option<&EndpointHealth>,
) -> CredentialResult {
    let spec = match engine_spec(engine) {
        Some(s) => s,
        None => {
            return CredentialResult::unknown(format!("no credential check for engine '{engine}'"))
        }
    };
    if launcher.is_empty() {
        return CredentialResult::missing(format!("{} CLI not found on PATH", spec.command));
    }
    if let Some(h) = health {
        if let Some(ru) = &h.recent_usable {
            return CredentialResult::ok(format!(
                "signed in (usable reply {} min ago)",
                ru.age_minutes
            ));
        }
    }
    if spec.local_sign_in {
        return get_muse_sign_in();
    }
    let mut timeout = 45u64;
    if let Some(hook) = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_LOGIN_TIMEOUT") {
        if let Ok(n) = hook.trim().parse::<u64>() {
            if n > 0 {
                timeout = n;
            }
        }
    }
    get_agy_models_status(launcher, timeout)
}

/// `reviewer.harness` of a CLI engine run (`Get-EngineHarness`): muse reads its version from the
/// install directory (`.muse-version` / `.muse-release-info.json` / `<launcher> --version`); agy
/// has no `--version`, so its version comes from the launcher file's metadata (unavailable on a
/// `.cmd` shim, hence `(version unknown)`).
pub(crate) fn engine_harness(engine: &str, launcher: &str) -> String {
    if engine == "muse" {
        return get_muse_harness(launcher);
    }
    // agy (and any other CLI engine): the launcher file's ProductVersion, else version unknown.
    let ver = launcher_file_version(launcher);
    if !ver.trim().is_empty() {
        format!("{engine}-cli {}", ver.trim())
    } else {
        format!("{engine}-cli (version unknown)")
    }
}

/// A best-effort file version of a launcher. On Windows a `.exe` carries a ProductVersion; a
/// `.cmd`/`.bat` shim does not (so agy shims read as "version unknown", matching the plugin).
fn launcher_file_version(_launcher: &str) -> String {
    // c3 does not read PE version resources; a shim launcher has none anyway. Left empty so the
    // harness string is "<engine>-cli (version unknown)", exactly as for a `.cmd` launcher.
    String::new()
}

/// muse version regex: a semver-ish token (`1.3.0`, `9.9.9-fake`, `v2.0`), used by
/// [`get_muse_harness`].
fn muse_version_token(s: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^v?\d+\.\d+(\.\d+)?([.-][0-9A-Za-z.-]+)?$").unwrap())
        .is_match(s)
}

/// `Get-MuseHarness` (D8): `muse-cli <version>` from `.muse-version` next to the launcher, else
/// the `version` of `.muse-release-info.json` there, else `<launcher> --version` (15 s), else
/// `muse-cli (version unknown)`.
pub(crate) fn get_muse_harness(launcher: &str) -> String {
    if launcher.trim().is_empty() {
        return "muse-cli (version unknown)".to_string();
    }
    let mut ver = String::new();
    if let Some(dir) = Path::new(launcher).parent() {
        let vf = dir.join(".muse-version");
        if vf.is_file() {
            if let Ok(text) = std::fs::read_to_string(&vf) {
                let first = text
                    .split(['\r', '\n'])
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if muse_version_token(&first) {
                    ver = first;
                }
            }
        }
        if ver.is_empty() {
            let rf = dir.join(".muse-release-info.json");
            if rf.is_file() {
                if let Ok(text) = std::fs::read_to_string(&rf) {
                    if let Ok(v) = serde_json::from_str::<Value>(&text) {
                        if let Some(s) = v.get("version").and_then(|x| x.as_str()) {
                            if muse_version_token(s.trim()) {
                                ver = s.trim().to_string();
                            }
                        }
                    }
                }
            }
        }
    }
    if ver.is_empty() {
        if let Some((code, out, err)) =
            run_with_timeout(launcher, &["--version"], Duration::from_secs(15))
        {
            if code == 0 {
                let merged = format!("{out} {err}");
                for tok in merged.split_whitespace() {
                    if muse_version_token(tok) {
                        ver = tok.to_string();
                        break;
                    }
                }
            }
        }
    }
    if ver.is_empty() {
        "muse-cli (version unknown)".to_string()
    } else {
        format!("muse-cli {}", ver.trim_start_matches('v'))
    }
}

#[cfg(test)]
mod roster_walk_tests {
    use super::*;
    use c3_core::config::scan_config_text;
    use c3_core::roster::{Roster, RosterEntry};

    fn entry(pos: usize, provider: &str, model: &str) -> RosterEntry {
        RosterEntry {
            position: pos,
            provider: provider.into(),
            model: model.into(),
            codex_config: vec![],
            auth: String::new(),
            panel: "always".into(),
            engine: "codex".into(),
            engine_declared: false,
            ..Default::default()
        }
    }

    fn roster(entries: Vec<RosterEntry>) -> Roster {
        Roster {
            exists: true,
            path: "R.json".into(),
            entries,
            ..Default::default()
        }
    }

    #[test]
    fn find_roster_entry_narrows_by_model_when_ambiguous() {
        let r = roster(vec![entry(1, "gemini", "flash"), entry(2, "gemini", "pro")]);
        // No model: the first entry of the label.
        assert_eq!(find_roster_entry(&r, "gemini", "").unwrap().model, "flash");
        // A model that a second entry names: that one.
        assert_eq!(find_roster_entry(&r, "gemini", "pro").unwrap().model, "pro");
        // A label no entry uses.
        assert!(find_roster_entry(&r, "openai", "").is_none());
    }

    #[test]
    fn walk_model_narrowing_refuses_when_no_entry_resolves_to_it() {
        // openai resolves without config/network; the only entry resolves to gpt-5.1.
        let ctx = Ctx::for_consult(
            scan_config_text("", ""),
            vec![],
            roster(vec![entry(1, "openai", "gpt-5.1")]),
            String::new(),
            String::new(),
            Utc::now(),
        );
        let w = ctx.walk_full("gpt-99", "", false);
        assert!(w.entry.is_none());
        assert!(
            w.error.starts_with(
                "-Model gpt-99: no entry of the reviewer roster 'R.json' resolves to that model"
            ),
            "got: {}",
            w.error
        );
        assert!(w.error.contains("#1 openai :: gpt-5.1"));
    }

    #[test]
    fn walk_skip_preflight_takes_first_entry_unchecked() {
        let ctx = Ctx::for_consult(
            scan_config_text("", ""),
            vec![],
            roster(vec![
                entry(1, "openai", "gpt-5.1"),
                entry(2, "openai", "gpt-6"),
            ]),
            String::new(),
            String::new(),
            Utc::now(),
        );
        let w = ctx.walk_full("", "", true);
        let e = w.entry.expect("first entry taken");
        assert_eq!(e.position, 1);
        assert!(w.skipped.is_empty());
    }

    /// (wave 1b, 0.6.0 E5) Two codex routes of plan zai on two endpoints (ZAI, ZAIB) and a third
    /// provider without a plan, every entry anonymous (no credential to set); a usage limit
    /// recorded on ZAI's endpoint.
    fn plan_ctx(plan_b: &str, failure_on: &str) -> (Ctx, String, String) {
        let toml = "model = \"gpt-5.1\"\n\n[model_providers.ZAI]\nbase_url = \"https://api.z.ai/api/v1\"\nwire_api = \"responses\"\n\n[model_providers.ZAIB]\nbase_url = \"https://open.bigmodel.cn/api/v1\"\nwire_api = \"responses\"\n\n[model_providers.local]\nbase_url = \"http://localhost:8080/v1\"\nwire_api = \"responses\"\n";
        let config = scan_config_text("", toml);
        let fp = |p: &str| resolve_reviewer_identity(&config, p, "m", "", "codex", "").fingerprint;
        let (fp_a, fp_b) = (fp("ZAI"), fp("ZAIB"));
        assert!(!fp_a.is_empty() && fp_a != fp_b);
        let now = Utc::now();
        let iso = |d: DateTime<Utc>| {
            format_offset_iso(d.with_timezone(&FixedOffset::east_opt(0).unwrap()))
        };
        let failed_fp = if failure_on == "ZAI" { &fp_a } else { &fp_b };
        let consult = json!({
            "n": 1,
            "when": iso(now - chrono::Duration::minutes(10)),
            "finished_at": iso(now - chrono::Duration::minutes(9)),
            "bridge_outcome": "failed: provider error",
            "reviewer": { "provider_fingerprint": failed_fp },
            "provider_failure": {
                "class": "quota", "code": "429",
                "message": "usage limit reached for the 5 hour window",
                "when": iso(now - chrono::Duration::minutes(10)),
                "retry_after": iso(now + chrono::Duration::hours(2))
            }
        });
        let anon = |pos: usize, p: &str, plan: &str| RosterEntry {
            auth: "none".into(),
            plan: plan.into(),
            ..entry(pos, p, "m")
        };
        let ctx = Ctx::for_consult(
            config.clone(),
            vec![consult],
            roster(vec![
                anon(1, "ZAI", "zai"),
                anon(2, "ZAIB", plan_b),
                anon(3, "local", ""),
            ]),
            String::new(),
            String::new(),
            now,
        );
        (ctx, fp_a, fp_b)
    }

    #[test]
    fn a_usage_limit_on_one_route_marks_every_route_of_its_plan_out() {
        // (harness-claude E5, with two codex routes) the walk skips ZAI (its own limit) and ZAIB
        // (its plan) and selects local
        let (ctx, _, _) = plan_ctx("zai", "ZAI");
        let w = ctx.walk_full("", "", false);
        assert_eq!(w.entry.as_ref().map(|e| e.provider.as_str()), Some("local"));
        assert_eq!(w.skipped.len(), 2);
        assert!(
            w.skipped[0].3.starts_with("usage limit until "),
            "{}",
            w.skipped[0].3
        );
        assert!(
            w.skipped[1]
                .3
                .starts_with("plan zai (usage limit on ZAI until "),
            "{}",
            w.skipped[1].3
        );
        // the "would select" walk of c3 providers says the same
        let s = ctx.select_roster_reviewer();
        assert_eq!(s.entry.map(|e| e.provider), Some("local".to_string()));
        assert!(s.skipped[1]
            .3
            .starts_with("plan zai (usage limit on ZAI until "));
        // the panel skips both
        let sel = ctx.panel_members("", "", "checkpoint", false, false, 0);
        let states: Vec<(&str, &str)> = sel
            .members
            .iter()
            .map(|m| (m.state.as_str(), m.skip_kind.as_str()))
            .collect();
        assert_eq!(
            states,
            vec![
                ("skipped", "unavailable"),
                ("skipped", "unavailable"),
                ("run", "")
            ]
        );
        assert!(sel.members[1]
            .reason
            .starts_with("plan zai (usage limit on ZAI until "));
        // the availability view: ZAIB out through its plan
        let recs = ctx.roster_availability();
        assert_eq!(recs[1].state, "out");
        assert!(
            recs[1]
                .short
                .starts_with("plan zai (usage limit on ZAI until "),
            "{}",
            recs[1].short
        );
        assert_eq!(recs[2].state, "available");
        // a direct run of the other route is refused with the -SkipPreflight hint
        let id = resolve_reviewer_identity(&ctx.config, "ZAIB", "m", "", "codex", "");
        let v = ctx.preflight(&id, None, "", true, false);
        assert_eq!(v.state, "available");
        let v = ctx.plan_verdict(v, &ctx.roster.entries[1], &id, false);
        assert!(v
            .refusal
            .starts_with("provider ZAIB is not usable: its plan zai hit a usage limit on ZAI at "));
        assert!(v
            .refusal
            .ends_with("; nothing was started (pass -SkipPreflight to launch anyway)"));
    }

    #[test]
    fn the_plan_propagates_both_ways_and_not_without_a_plan() {
        // a limit on ZAIB marks ZAI out through the plan
        let (ctx, _, _) = plan_ctx("zai", "ZAIB");
        let w = ctx.walk_full("", "", false);
        assert_eq!(w.entry.map(|e| e.provider), Some("local".to_string()));
        assert!(w.skipped[0]
            .3
            .starts_with("plan zai (usage limit on ZAIB until "));
        // ZAIB without the plan: only ZAI is out, ZAIB is selected
        let (ctx, _, _) = plan_ctx("", "ZAI");
        let w = ctx.walk_full("", "", false);
        assert_eq!(w.entry.map(|e| e.provider), Some("ZAIB".to_string()));
        // another plan: no propagation either
        let (ctx, _, _) = plan_ctx("mimo", "ZAI");
        let w = ctx.walk_full("", "", false);
        assert_eq!(w.entry.map(|e| e.provider), Some("ZAIB".to_string()));
    }

    fn weighted(pos: usize, provider: &str, model: &str, panel: &str, ctx: i64) -> RosterEntry {
        RosterEntry {
            panel: panel.into(),
            context_tokens: ctx,
            ..entry(pos, provider, model)
        }
    }

    /// The `harness-panel` LIGHT roster: #1 the weighty reviewer of the label (a small window), #2
    /// its light sibling, #3 another label. Preflight skipped (no config, no network).
    fn light_ctx(entries: Vec<RosterEntry>) -> Ctx {
        Ctx::for_consult(
            scan_config_text("", ""),
            vec![],
            roster(entries),
            String::new(),
            String::new(),
            Utc::now(),
        )
    }

    fn plan(sel: &PanelSelection) -> Vec<String> {
        sel.members
            .iter()
            .map(|m| format!("#{} {} {}", m.entry.position, m.state, m.reason))
            .collect()
    }

    const STAND_REASON: &str =
        "light reviewer; purpose acceptance is weighty - it stands in only when no entry of its label runs";

    #[test]
    fn panel_light_joins_light_purposes_and_stands_in_on_weighty_ones() {
        let ctx = light_ctx(vec![
            weighted(1, "ZAI", "glm-5.3", "weighty", 32000),
            weighted(2, "ZAI", "glm-5.3-flash", "light", 0),
            entry(3, "openai", "gpt-5.1"),
        ]);
        // a checkpoint: the light entry runs, its weighty sibling is held back
        let sel = ctx.panel_members("", "", "checkpoint", false, true, 0);
        assert_eq!(
            plan(&sel),
            vec![
                "#1 skipped weighty reviewer; purpose checkpoint is light (use -PanelAll)",
                "#2 run ",
                "#3 run ",
            ]
        );
        // an acceptance: the weighty sibling runs, the light one is held back
        let sel = ctx.panel_members("", "", "acceptance", false, true, 0);
        assert_eq!(
            plan(&sel),
            vec![
                "#1 run ".to_string(),
                format!("#2 skipped {STAND_REASON}"),
                "#3 run ".to_string(),
            ]
        );
        assert_eq!(sel.members[1].skip_kind, "light");
        // the weighty sibling skipped for its context window: the light entry stands in for it
        let ctx_reason = "brief too large for this reviewer's context (est. 30001 of 32000 tokens)";
        let sel = ctx.panel_members("", "", "acceptance", false, true, 30001);
        assert_eq!(
            plan(&sel),
            vec![
                format!("#1 skipped {ctx_reason}"),
                format!("#2 run stands in for #1 ({ctx_reason})"),
                "#3 run ".to_string(),
            ]
        );
        assert_eq!(sel.members[0].skip_kind, "context");
        assert_eq!(sel.members[1].skip_kind, "");
        // -PanelAll: every entry, the light one too
        let sel = ctx.panel_members("", "", "acceptance", true, true, 0);
        assert!(
            sel.members.iter().all(|m| m.state == "run"),
            "{:?}",
            plan(&sel)
        );
    }

    #[test]
    fn panel_light_stand_in_without_a_sibling_and_the_second_light_entry() {
        let ctx = light_ctx(vec![
            entry(1, "openai", "gpt-5.1"),
            weighted(2, "mimo", "mimo-v2.6-pro", "light", 0),
        ]);
        let sel = ctx.panel_members("", "", "decision", false, true, 0);
        assert_eq!(
            plan(&sel),
            vec!["#1 run ", "#2 run stands in (no other entry of label mimo)",]
        );
        // two light entries of one label on a weighty purpose: the first stands in, the second
        // sees it run and stays held back
        let ctx = light_ctx(vec![
            weighted(1, "mimo", "mimo-a", "light", 0),
            weighted(2, "mimo", "mimo-b", "light", 0),
        ]);
        let sel = ctx.panel_members("", "", "stuck", false, true, 0);
        assert_eq!(sel.members[0].state, "run");
        assert_eq!(
            sel.members[0].reason,
            "stands in (no other entry of label mimo runs)"
        );
        assert_eq!(sel.members[1].state, "skipped");
        assert_eq!(sel.members[1].skip_kind, "light");
        assert!(sel.error.is_empty());
    }
}
