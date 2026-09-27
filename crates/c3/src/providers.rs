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
            println!("c3 providers: {msg}");
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

struct Ctx {
    config: CodexConfig,
    consults: Vec<Value>,
    roster: Roster,
    launcher: String,
    openai_base_url: String,
    utc_now: DateTime<Utc>,
    no_network: bool,
    login_cache: RefCell<HashMap<String, CredentialResult>>,
    engine_launchers: RefCell<HashMap<String, String>>,
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
    let consults = read_all_task_consults(&collab_root);
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
        if let Ok(hook) = std::env::var("CODEX_CONSULT_TEST_LOGIN_TIMEOUT") {
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
        let cred = if engine != "codex" {
            self.engine_credential(engine, launcher, health)
        } else {
            let table = self.provider_table_ref(&id.provider);
            self.provider_credential(&id.provider, table, anonymous)
        };
        verdict_with_credential(id, health, cred, roster_walk)
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
                Some(self.preflight(
                    &id,
                    health.as_ref(),
                    &entry_launcher,
                    e.auth == "none",
                    true,
                ))
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

        let endpoint = format!(
            "{engine} ({})",
            if !engine_launcher.is_empty() {
                engine_launcher.clone()
            } else {
                "launcher not found".into()
            }
        );

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

fn strip_query(url: &str) -> String {
    match url.find('?') {
        Some(i) => format!("{}?...", &url[..i]),
        None => url.to_string(),
    }
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

fn get_codex_home() -> String {
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

fn read_all_task_consults(collab_root: &Path) -> Vec<Value> {
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

fn read_reviewer_roster() -> Result<Roster, String> {
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
    let validated = validate_roster(&path, &text, home.as_deref());
    if !validated.error.is_empty() {
        return Err(validated.error);
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

fn resolve_engine_launcher(engine: &str, explicit: &str) -> Result<Option<String>, String> {
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

fn find_roster_entry<'a>(
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

fn run_with_timeout(
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
    let mut child = Command::new(&prog)
        .args(&all_args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
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

fn get_agy_models_status(launcher: &str, timeout_sec: u64) -> CredentialResult {
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

struct MuseInfo {
    state: State,
    reason: String,
    cause: String,
    mechanism: String,
}

fn muse_credential_info() -> MuseInfo {
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

fn get_muse_launch_block() -> String {
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
