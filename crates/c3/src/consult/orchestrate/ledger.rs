//! The **ledger entry builders** of a `c3 consult` run: the pieces that turn the finished
//! run into its [`c3_core::ledger`] records. [`build_entry`] (with its borrowed inputs
//! [`BuildEntry`]) assembles the [`LedgerEntry`], [`build_finding`] one finding (with the
//! [`severity_token`] / [`evidence_token`] mappings), and [`build_reviewer`] the reviewer
//! record shared with the dry-run plan.

use super::*;

pub(super) fn build_finding(
    ctx: &Context,
    id: &str,
    rf: &c3_core::engine::ReplyFinding,
    handoff_rel: &str,
    thread: &str,
) -> c3_core::findings::Finding {
    use c3_core::findings::{Evidence, Finding, FindingStatus, HistoryEvent, Location, Source};
    let mut f = Finding::default();
    f.id = id.to_string();
    f.severity = severity_token(rf.severity).to_string();
    f.claim = rf.claim.clone();
    f.trigger = rf.trigger.clone();
    f.verification = rf.verification.clone();
    f.remedy = rf.remedy.clone();
    f.supersedes = rf.supersedes.clone();
    f.locations = rf
        .locations
        .iter()
        .map(|l| Location {
            path: l.path.clone(),
            line: l.line,
            extra: Default::default(),
        })
        .collect();
    f.evidence = rf
        .evidence
        .iter()
        .map(|e| Evidence {
            kind: evidence_token(e.kind).to_string(),
            reference: e.reference.clone(),
            observation: e.observation.clone(),
            extra: Default::default(),
        })
        .collect();
    f.source = Source {
        consult: ctx.consult_n,
        reply: handoff_rel.to_string(),
        thread: thread.to_string(),
        base_commit: ctx.revision.base_commit.clone(),
        tree_sha256: ctx.revision.tree_sha256.clone(),
        ..Default::default()
    };
    // The finding is created `proposed` with one initial history event, as the plugin does.
    f.history.push(HistoryEvent {
        when: iso_now(),
        status: FindingStatus::Proposed,
        by: "codex-consult".into(),
        note: String::new(),
        evidence: String::new(),
        base_commit: ctx.revision.base_commit.clone(),
        tree_sha256: ctx.revision.tree_sha256.clone(),
        extra: Default::default(),
    });
    f
}

fn severity_token(s: c3_core::engine::Severity) -> &'static str {
    use c3_core::engine::Severity::*;
    match s {
        Blocker => "blocker",
        Major => "major",
        Minor => "minor",
        Note => "note",
    }
}

fn evidence_token(k: c3_core::engine::EvidenceKind) -> &'static str {
    use c3_core::engine::EvidenceKind::*;
    match k {
        ReadCode => "read-code",
        RanCommand => "ran-command",
        Inferred => "inferred",
        Assumed => "assumed",
    }
}

/// Inputs to [`build_entry`]: the run context and outcome/wall/usage, the thread identity, the
/// structured reply and finding ids/counts, the validation/provider failure, the handoff/reply/
/// events relative paths, the drift, the secondary state and any stall. Borrows are shared; the
/// owned `provider_failure`/`stall` are moved in.
pub(super) struct BuildEntry<'a> {
    pub(super) ctx: &'a Context,
    pub(super) bridge_outcome: &'a str,
    pub(super) wall: f64,
    pub(super) usage: &'a Option<Usage>,
    pub(super) thread: &'a str,
    pub(super) thread_source: &'a str,
    pub(super) structured: Option<&'a StructuredReply>,
    pub(super) finding_ids: &'a [String],
    pub(super) counts: &'a FindingCounts,
    pub(super) validation_error: &'a str,
    pub(super) provider_failure: Option<c3_core::ledger::ProviderFailure>,
    pub(super) handoff_rel: &'a str,
    pub(super) reply_json_rel: &'a str,
    pub(super) events_rel: &'a str,
    pub(super) drift: &'a Drift,
    pub(super) sec: &'a Secondary,
    pub(super) stall: Option<c3_core::ledger::Stall>,
}

pub(super) fn build_entry(args: BuildEntry) -> LedgerEntry {
    let BuildEntry {
        ctx,
        bridge_outcome,
        wall,
        usage,
        thread,
        thread_source,
        structured,
        finding_ids,
        counts,
        validation_error,
        provider_failure,
        handoff_rel,
        reply_json_rel,
        events_rel,
        drift,
        sec,
        stall,
    } = args;
    let mut e = LedgerEntry {
        n: ctx.consult_n,
        when: iso_now(),
        purpose: ctx.o.purpose.clone(),
        // `-Topic`/`-Role` (the roster/M4 surface): a panel member (or a `-Topic` run) writes
        // them; a plain single run writes the plugin's empty defaults (`[]` / `""`).
        topics: Some(
            ctx.o
                .topic
                .iter()
                .map(|t| serde_json::Value::String(t.clone()))
                .collect(),
        ),
        role: Some(ctx.role.clone()),
        consult_id: ctx.consult_id.clone(),
        // (0.6.1, U5) right after consult_id.
        consult_ref: Some(ctx.consult_ref.clone()),
        lineage: ctx.identity.lineage.clone(),
        // (wave 27) the coordinator record right after `lineage`.
        coordinator: Some(ctx.coordinator.clone()),
        preflight: ctx.preflight.clone(),
        preflight_warning: ctx.preflight_warning.clone(),
        parent_thread: ctx.parent_thread.clone(),
        thread: thread.to_string(),
        thread_source: thread_source.to_string(),
        mode: ctx.effective_mode.clone(),
        // (wave 26b, D16) a fork/resume the reviewer's context window forced to a new thread, else
        // `null` in position (the harness `$order` requires the key present).
        mode_fallback: Some(ctx.mode_fallback.clone()),
        command: if ctx.is_codex() {
            format!("codex {}", ctx.argv_display.trim_start_matches("codex "))
        } else {
            ctx.argv_display.clone()
        },
        // (wave 27) the scrubbed host-marker NAMES right after `command` (never a value).
        child_env_scrubbed: Some(
            ctx.child_env_scrubbed
                .iter()
                .map(|s| serde_json::Value::String(s.clone()))
                .collect(),
        ),
        brief: ctx.brief_ref.clone(),
        prompt_chars: ctx.prompt_text.chars().count() as i64,
        reply: handoff_rel.to_string(),
        reply_json: reply_json_rel.to_string(),
        events: events_rel.to_string(),
        partial_reply: sec.partial_rel.clone(),
        model: model_label(&ctx.identity),
        effort: ctx.effort.sent.clone(),
        effort_requested: ctx.effort.requested.clone(),
        effort_sent: ctx.effort.sent.clone(),
        effort_mapping: ctx.effort.mapping.clone(),
        effort_caps: ctx.effort.caps.clone(),
        max_words: ctx.r.max_words as i64,
        sandbox: sandbox_label(&ctx.o),
        timeout_sec: ctx.r.timeout_sec,
        timeout_source: ctx.r.timeout_source.clone(),
        continue_sec: ctx.r.continue_sec,
        extra_config: ctx
            .r
            .extra_config
            .iter()
            .map(|s| serde_json::Value::String(s.clone()))
            .collect(),
        extra_config_source: ctx.extra_config_source.clone(),
        // (wave 28b, D15) the context window as it reached the engine (null without one).
        context_window: Some(ctx.context_window.clone()),
        roster: ctx.roster_record.clone(),
        // Peak status evaluated at launch (`run_live` re-evaluated it as call 1).
        peak: ctx.peak,
        peak_schedule: ctx.peak_schedule.clone(),
        peak_source: ctx.peak_source.clone(),
        peak_evaluated_at: ctx.peak_evaluated_at.clone(),
        schema: if ctx.r.raw {
            String::new()
        } else {
            "consult-reply v1".into()
        },
        schema_transport: ctx.transport.transport.clone(),
        schema_transport_source: ctx.transport.source.clone(),
        validation_error: validation_error.to_string(),
        format_retry: sec.format_retry.clone(),
        denial_retry: sec.denial_retry.clone(),
        timeout_continue: sec.timeout_continue.clone(),
        // (wave 26b, D12) the stall kill's `{seconds, last_event}`, else `null` in position.
        stall: Some(stall),
        engine_run: if ctx.is_codex() {
            None
        } else {
            Some(c3_core::ledger::EngineRun {
                turns: sec.engine_turns,
                max_model_steps: if ctx.o.max_model_steps > 0 {
                    Some(ctx.o.max_model_steps)
                } else {
                    None
                },
                msp_schema_version: sec.msp_version,
                extra: if ctx.engine == "claude" {
                    claude_engine_run_extra(ctx, sec)
                } else {
                    Default::default()
                },
            })
        },
        range: ctx.range_record.clone(),
        // The run warnings (range size, roster ambiguity, test mode, semantics) for EVERY engine
        // (`foreach ($rw in $runWarnings) { $engineWarnings.Add($rw) }`), then an engine run's turn
        // warnings (denial notices etc.).
        warnings: {
            let mut w: Vec<String> = ctx.run_warnings.clone();
            if !ctx.is_codex() {
                for ew in &sec.engine_warnings {
                    if !w.contains(ew) {
                        w.push(ew.clone());
                    }
                }
            }
            w.into_iter().map(serde_json::Value::String).collect()
        },
        base_commit: ctx.revision.base_commit.clone(),
        reviewed_revision: ctx.revision.reviewed_revision.clone(),
        tree_sha256: ctx.revision.tree_sha256.clone(),
        tree_sha256_after: drift.tree_sha256_after.clone(),
        tree_changed_during_review: drift.tree_changed,
        revision_moved: Some(if drift.revision_moved.is_empty() {
            None
        } else {
            Some(drift.revision_moved.clone())
        }),
        changed_files: ctx.revision.changed_files,
        brief_sha256: ctx.brief_sha.clone(),
        brief_sha256_after: drift.brief_sha_after.clone(),
        brief_changed_during_review: drift.brief_changed,
        artifacts: drift.artifacts.clone(),
        artifacts_changed_during_review: !drift.artifacts_changed_paths.is_empty(),
        fingerprint_note: ctx.revision.fingerprint_note.clone(),
        bridge_outcome: bridge_outcome.to_string(),
        provider_failure,
        structured: structured.is_some(),
        findings: counts.clone(),
        finding_ids: finding_ids
            .iter()
            .map(|s| serde_json::Value::String(s.clone()))
            .collect(),
        usage: usage.clone(),
        // (wave 28c, D11) right after usage.
        compactions: Some(sec.compactions.clone()),
        wall_seconds: wall,
        finished_at: iso_now(),
        reviewer: build_reviewer(&ctx.identity, &ctx.harness),
        ..Default::default()
    };
    if let Some(s) = structured {
        e.verdict = match s.verdict {
            c3_core::engine::Verdict::Accept => "ACCEPT",
            c3_core::engine::Verdict::Hold => "HOLD",
            c3_core::engine::Verdict::Reject => "REJECT",
            c3_core::engine::Verdict::Advise => "ADVISE",
        }
        .to_string();
        e.verdict_reason = s.verdict_reason.clone();
    }
    // The panel record a member writes into its ledger entry (`codex-consult.ps1:3216`): the
    // panel id, this seat, the panel-wide member list, the plan and the routing record. `started`
    // and `usable` are the panel run's to patch after every member finishes (chunk 2). `roles_note`
    // rides `extra` (after `routing`, matching the plugin's key order).
    e.panel = build_member_panel(ctx);

    // The engine tree-check record (wave 26b, D9): `{outcome, files[]}` in the named field,
    // between `artifacts_changed_during_review` and `bridge_outcome`. `null` for codex (no
    // check); for an engine, the record whenever the check ran (`clean`/`warned`/`failed`).
    e.tree_check = if ctx.is_codex() || sec.tree_check_outcome.is_empty() {
        Some(None)
    } else {
        Some(Some(c3_core::ledger::TreeCheck {
            outcome: sec.tree_check_outcome.clone(),
            files: sec
                .tree_check_files
                .iter()
                .map(|f| serde_json::Value::String(f.clone()))
                .collect(),
            ..Default::default()
        }))
    };
    e
}

pub(crate) fn build_reviewer(id: &ReviewerIdentity, harness: &str) -> Reviewer {
    // provider_config is the echo `Resolve-ReviewerIdentity` recorded: `{builtin:"openai"}`
    // (with `base_url`/`base_url_source` when `OPENAI_BASE_URL` is set) for the built-in codex
    // endpoint, the raw provider-table echo for a user table, or `{engine, launcher[,
    // credential_mechanism]}` for a CLI engine (`Resolve-EngineIdentity`). Only for a codex
    // built-in run with a null config is the `{builtin:"openai"}` default applied.
    let engine = if id.engine.is_empty() {
        "codex".to_string()
    } else {
        id.engine.clone()
    };
    let provider_config = if id.provider_config.is_null() {
        if engine == "codex" {
            serde_json::json!({ "builtin": "openai" })
        } else {
            serde_json::Value::Null
        }
    } else {
        id.provider_config.clone()
    };
    Reviewer {
        provider: id.provider.clone(),
        provider_source: id.provider_source.clone(),
        model: id.model.clone(),
        model_source: id.model_source.clone(),
        engine,
        harness: harness.to_string(),
        provider_fingerprint: id.fingerprint.clone(),
        provider_config,
        identity_note: id.note.clone(),
        ..Default::default()
    }
}
