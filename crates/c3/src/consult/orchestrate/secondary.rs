//! The **secondary turns** of a `c3 consult` run: the engine turns that run *after* the main
//! turn, driven by [`super::finish`]. Each is a self-contained `resume <thread>` turn plus the
//! parameter struct the orchestrator fills:
//!
//!   * the **timeout continuation** ([`run_timeout_continuation`]) - one turn after the main turn
//!     was killed on its timeout;
//!   * the **format repair** ([`run_format_repair`]) - one turn converting a prose reply into the
//!     structured object;
//!   * the **engine denial retry** ([`run_engine_denial_retry`]) - the agy auto-denial replay;
//!   * the **partial-reply salvage** ([`build_partial_reply`]) - the handoff of a killed turn's
//!     salvaged content.
//!
//! The shared turn runners ([`run_codex_secondary`], [`run_engine_secondary`]) and the repair
//! prompt/effort helpers live here too. The pure text work over the event stream / reply object
//! (salvage, prose gate, repair drift) stays in the sibling [`crate::consult::secondary`] module.

use super::*;

/// The reply schema inlined into a prompt (the plugin's
/// `[IO.File]::ReadAllText(...).Trim() -replace "\r\n","\n" -replace "\n",$nl` with `$nl` = CRLF).
fn schema_text_crlf() -> String {
    c3_core::schema::REPLY_SCHEMA_V1
        .trim()
        .replace("\r\n", "\n")
        .replace('\n', "\r\n")
}

/// The convert-only format-repair prompt (the plugin's repair turn text). Shared by the codex/engine
/// format repair and the http seat's format-repair replay so both send the exact same instruction.
pub(crate) fn format_repair_prompt(consult_id: &str) -> String {
    format!(
        "Your last message was prose, not the required JSON. Reply with exactly one bare JSON object satisfying the JSON Schema below - no fence, nothing before or after it. Convert, do not re-answer: copy your previous content unchanged (the same Q1..Qn answers verbatim inside reply_markdown, the same findings, the same Requested checks, the same prior-finding statuses and the same verdict); add or omit nothing.\r\n\r\nJSON Schema of the reply:\r\n{}\r\n\r\nConsultation id: {}",
        schema_text_crlf(),
        consult_id
    )
}

/// Run one secondary turn (`resume <thread>`) through the selected engine and return its outcome
/// and measured wall. For a muse turn the prompt is written to a fresh temp `--prompt-file`.
/// Inputs to [`run_codex_secondary`]: the run context and turn kind, the sandbox/effort/schema for
/// the turn, the thread to resume, the prompt, the per-turn log paths, the timeout and the running/
/// kill hooks. Borrows are shared; the owned fields are moved in.
struct RunSecondary<'a> {
    ctx: &'a Context,
    kind: TurnKind,
    sandbox: &'a str,
    effort: Option<String>,
    schema_arg: Option<PathBuf>,
    thread: &'a str,
    prompt_text: &'a str,
    last_path: &'a Path,
    events_path: &'a Path,
    stderr_path: &'a Path,
    timeout_sec: f64,
    on_running: Option<std::sync::Arc<dyn Fn(u32, String) + Send + Sync>>,
    // (wave 3e, F30-2) the turn's kill writes its record BEFORE the kill
    on_kill: Option<crate::engines::subprocess::KillHook>,
}

fn run_codex_secondary(
    args: RunSecondary,
) -> (AttemptOutcome, f64, String, Option<ClaudeRunDetail>) {
    let RunSecondary {
        ctx,
        kind,
        sandbox,
        effort,
        schema_arg,
        thread,
        prompt_text,
        last_path,
        events_path,
        stderr_path,
        timeout_sec,
        on_running,
        on_kill,
    } = args;
    let engine = engine_kind_of(&ctx.engine);
    // muse takes its prompt through a fresh `--prompt-file` per turn (distinct from the main one).
    let prompt_file = if ctx.prompt_file.is_some() {
        let pf = std::env::temp_dir().join(format!(
            "codex-consult-prompt-{}.txt",
            uuid::Uuid::new_v4().simple()
        ));
        let _ = c3_core::store::write_text_atomic(&pf, prompt_text.as_bytes());
        Some(pf)
    } else {
        None
    };
    let mut request = Request {
        prompt: prompt_text.to_string(),
        brief_path: None,
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
        effort,
        timeout_sec,
        mode: Mode::New,
        sandbox: sandbox.to_string(),
        schema_path: schema_arg,
        extra_config: with_context_config(&ctx.r.extra_config, &ctx.context_config),
        agent_config: agent_config_for(engine, &ctx.r.extra_config),
        output_last_message: if engine == EngineKind::Codex {
            Some(last_path.to_path_buf())
        } else {
            None
        },
        prompt_file,
        max_model_steps: if ctx.o.max_model_steps > 0 {
            Some(ctx.o.max_model_steps as u32)
        } else {
            None
        },
        new_thread: None,
        add_dirs: Vec::new(),
    };
    // The resume must carry the request's own lineage (the core refuses a cross-lineage resume).
    let lineage = request.lineage();
    request.mode = Mode::Resume {
        thread: thread.to_string(),
        lineage,
    };
    let request = request.claude_turn(ctx);
    let files = TurnFiles {
        events: events_path.to_path_buf(),
        stderr: stderr_path.to_path_buf(),
    };
    let turn = TurnRequest {
        request,
        consultation: ConsultationId(ctx.consult_id.clone()),
        attempt: c3_core::engine::AttemptId(ctx.consult_id.clone()),
        kind,
        continuation: Some(c3_core::engine::Continuation::Native(
            c3_core::engine::ConversationId(thread.to_string()),
        )),
    };
    let plan_err = |e: c3_core::engine::EngineError| AttemptOutcome::LaunchFailed {
        child_exists: false,
        message: format!("the secondary turn could not be planned ({e:?})"),
    };
    let start = std::time::Instant::now();
    // (wave 26c, D1) the FORMAT REPAIR turn watches the kick file (a kicked repair leaves the first
    // reply standing); (wave 27c, D2 / F30-5) so does the TIMEOUT CONTINUATION - a kick addresses the
    // RUN: found during the continuation it cancels it, and the run keeps the main turn's timeout
    // outcome and its salvage (`run_timeout_continuation`). The denial retry does not watch it.
    let secondary_kick = if matches!(kind, TurnKind::FormatRepair | TurnKind::TimeoutContinuation) {
        Some(ctx.kick_path.clone())
    } else {
        None
    };
    // `engine_outcome` is the engine turn's own framed outcome text (`AgyTurn`/`MuseTurn`
    // `.outcome`), used to frame an engine repair/continuation failure exactly; empty for codex.
    let mut claude_run: Option<ClaudeRunDetail> = None;
    let (outcome, engine_outcome) = match ctx.engine.as_str() {
        "claude" => {
            let eng = claude_engine(
                ctx,
                TurnFiles::default(),
                files,
                0,
                secondary_kick.clone(),
                on_running,
                on_kill,
            );
            match eng.run_detailed(&turn) {
                Ok(r) => {
                    claude_run = Some(ClaudeRunDetail {
                        events: r.events.clone(),
                        turn: r.turn.clone(),
                        killed: r.killed,
                    });
                    (r.outcome, r.turn.outcome)
                }
                Err(e) => (plan_err(e), String::new()),
            }
        }
        "agy" => {
            let eng = crate::engines::agy::AgyEngine {
                launcher: ctx.engine_launcher.clone(),
                cwd: ctx.repo_root.clone(),
                primary: TurnFiles::default(),
                secondary: files,
                no_network: false,
                models_timeout_sec: 45,
                stall_sec: 0,
                kick_path: secondary_kick.clone(),
                on_running,
                on_kill,
            };
            match eng.run_detailed(&turn) {
                Ok(r) => (r.outcome, r.turn.outcome),
                Err(e) => (plan_err(e), String::new()),
            }
        }
        "muse" => {
            let eng = crate::engines::muse::MuseEngine {
                launcher: ctx.engine_launcher.clone(),
                cwd: ctx.repo_root.clone(),
                primary: TurnFiles::default(),
                secondary: files,
                stall_sec: 0,
                kick_path: secondary_kick.clone(),
                on_running,
                on_kill,
            };
            match eng.run_detailed(&turn) {
                Ok(r) => (r.outcome, r.turn.outcome),
                Err(e) => (plan_err(e), String::new()),
            }
        }
        "grok" => {
            let eng = crate::engines::grok::GrokEngine {
                launcher: ctx.engine_launcher.clone(),
                cwd: ctx.repo_root.clone(),
                primary: TurnFiles::default(),
                secondary: files,
                stall_sec: 0,
                // A secondary turn is exempt from the zero-tool-call review guard (it only fires on
                // the PRIMARY turn); the purpose is carried for completeness.
                purpose: ctx.o.purpose.clone(),
                kick_path: secondary_kick.clone(),
                on_running,
                on_kill,
            };
            match eng.run_detailed(&turn) {
                Ok(r) => (r.outcome, r.turn.outcome),
                Err(e) => (plan_err(e), String::new()),
            }
        }
        _ => {
            use c3_core::engine::Engine;
            let eng = CodexEngine {
                launcher: ctx.launcher.clone(),
                cwd: ctx.repo_root.clone(),
                primary: TurnFiles::default(),
                secondary: files,
                stall_sec: 0,
                kick_path: secondary_kick.clone(),
                on_running,
                on_kill,
                budget: None,
            };
            (eng.run(&turn).unwrap_or_else(plan_err), String::new())
        }
    };
    (
        outcome,
        round1(start.elapsed().as_secs_f64()),
        engine_outcome,
        claude_run,
    )
}

/// The timeout continuation (`codex-consult.ps1:3626`): after a killed main turn, ONE
/// `resume <thread>` turn — the main turn's options — asks the reviewer to finish now. Gated by
/// the thread, survivors, files-changed and the killed turn's own failure evidence.
/// Inputs to [`run_timeout_continuation`]: the run context, the resume candidate/drift/survivors,
/// the kill-confirmation state and the main turn's stderr/error/wall, plus the `&mut` run state the
/// continuation updates and the stall/running hooks. Borrows and `&mut` slots are moved in.
pub(super) struct TimeoutContinuation<'a> {
    pub(super) ctx: &'a Context,
    // (0.6.0, wave 29) the claude killed turn's session (its minted id the init confirmed) when the
    // run has no verified thread; `""` for every other engine.
    pub(super) candidate: &'a str,
    pub(super) drift: &'a Drift,
    pub(super) survivors: &'a [u32],
    // (wave 27c, D16) `Some((pid, why))` when the main turn's tree kill could not be confirmed:
    // no continuation is attempted (the orphan may still hold the thread).
    pub(super) kill_unconfirmed: Option<&'a (u32, String)>,
    pub(super) main_stderr: &'a str,
    pub(super) main_event_error: &'a str,
    pub(super) main_wall: f64,
    pub(super) sec: &'a mut Secondary,
    pub(super) bridge_outcome: &'a mut String,
    pub(super) raw_text: &'a mut String,
    pub(super) thread: &'a mut String,
    pub(super) thread_source: &'a mut String,
    pub(super) usable: &'a mut bool,
    pub(super) provider_failure: &'a mut Option<c3_core::ledger::ProviderFailure>,
    // (wave 26b, D12) `Some(n)` when the main turn was STOPPED by the stall cut (n s without an
    // event) rather than the wall-clock timeout — the console line says so.
    pub(super) stalled_secs: Option<i64>,
    // (wave 27c, D2) rewrites the pending record to `running` with the CONTINUATION's child pid the
    // moment it spawns, so a concurrent `--kick` (and the liveness check) find a live child of this
    // run — the main turn's child was killed at the timeout.
    pub(super) on_running: Option<std::sync::Arc<dyn Fn(u32, String) + Send + Sync>>,
}

pub(super) fn run_timeout_continuation(args: TimeoutContinuation) {
    let TimeoutContinuation {
        ctx,
        candidate,
        drift,
        survivors,
        kill_unconfirmed,
        main_stderr,
        main_event_error,
        main_wall,
        sec,
        bridge_outcome,
        raw_text,
        thread,
        thread_source,
        usable,
        provider_failure,
        stalled_secs,
        on_running,
    } = args;
    let continue_thread = if thread.is_empty() && !candidate.is_empty() {
        candidate.to_string()
    } else {
        thread.clone()
    };
    sec.continue_thread = continue_thread.clone();
    let continue_events_rel = ctx.hf("continue.events.jsonl");
    let continue_events_path = ctx.hpath("continue.events.jsonl");

    // Gate (the plugin's `$continueSkip`).
    let mut skip = String::new();
    if let Some((pid, why)) = kill_unconfirmed {
        // (D16) the kill of the main turn was not confirmed: the orphan may still hold the thread,
        // so no continuation runs on it.
        skip = format!(
            "the kill of the main turn was not confirmed ({why}) - pid {pid} may still hold the thread"
        );
    } else if ctx.r.continue_sec <= 0 {
        skip = "-ContinueSec 0".to_string();
    } else if !sec.claude_proof_skip.is_empty() {
        // (0.6.0, wave 29b, E12) a continuation would resume a session that ran outside the proof
        skip = sec.claude_proof_skip.clone();
    } else if continue_thread.is_empty() {
        skip = "the thread of the killed turn is not known".to_string();
    } else if !survivors.is_empty() {
        skip = format!("{} process(es) survived the kill", survivors.len());
    } else {
        let mut moved: Vec<&str> = Vec::new();
        if drift.tree_changed {
            moved.push("the working tree");
        }
        if drift.brief_changed {
            moved.push("the brief");
        }
        if !drift.artifacts_changed_paths.is_empty() {
            moved.push("an artifact");
        }
        if !moved.is_empty() {
            skip = format!("files changed during the run ({})", moved.join(", "));
        }
    }
    // (wave 8 = plugin wave 30, T15) a continuation spends more of the window: none once the
    // thread's 5-hour window is already at the quota budget
    if skip.is_empty() && ctx.is_codex() && ctx.budget.percent > 0 {
        let rp = c3_core::quota::codex_rollout_path(&continue_thread, &providers::get_codex_home());
        let w = c3_core::quota::read_primary_window(rp.as_deref());
        if let Some(used) = w.used {
            let ahead = w
                .resets_at
                .map(|r| r.with_timezone(&chrono::Utc) > chrono::Utc::now())
                .unwrap_or(true);
            if used >= ctx.budget.percent as f64 && ahead {
                skip = format!(
                    "the {} is {} % used - at the quota budget {} %",
                    w.name,
                    c3_core::quota::format_percent(used),
                    ctx.budget.percent
                );
            }
        }
    }
    if skip.is_empty()
        && (sec.killed_adapter_class == "quota" || sec.killed_adapter_class == "auth")
    {
        // (wave 29, D6) the adapter's class of the killed turn (claude: a rejecting rate-limit
        // event before the kill) - `Get-KilledTurnFailure -AdapterClass`
        let text = sec
            .killed_adapter_texts
            .iter()
            .find(|t| !t.trim().is_empty())
            .map(|t| c3_core::one_line(t))
            .unwrap_or_else(|| {
                format!("the {} class of the turn's rules", sec.killed_adapter_class)
            });
        skip = format!(
            "the killed turn reported a {} failure ({text})",
            sec.killed_adapter_class
        );
    }
    if skip.is_empty() {
        if let Some((class, text)) =
            crate::consult::secondary::get_killed_turn_failure(main_event_error, main_stderr)
        {
            skip = format!("the killed turn reported a {class} failure ({text})");
        }
    }
    if !skip.is_empty() {
        sec.timeout_continue = Some(c3_core::ledger::TimeoutContinue {
            thread: continue_thread,
            wall_seconds: 0.0,
            outcome: format!("not attempted: {skip}"),
            events: None,
            usage: None,
            ..Default::default()
        });
        return;
    }

    // The continuation prompt (the main turn's contract; prompt-only re-sends the schema).
    let mut parts: Vec<String> = Vec::new();
    if !ctx.r.raw {
        parts.push(prompt::FINAL_OUTPUT_CONTRACT.to_string());
    }
    // (wave 26c, D3) a stall kill names the silence outside a tool call; a timeout names the limit.
    let stopped_reason = match stalled_secs {
        Some(n) => format!("stopped after no output for {n} s outside a tool call"),
        None => format!("stopped by a time limit after {} s", ctx.r.timeout_sec),
    };
    parts.push(format!(
        "Your previous turn was {stopped_reason}. Do not start over and do not read more files than you must: finish now and output your final answer in the required format.",
    ));
    if !ctx.r.raw && ctx.transport.transport == "prompt-only" {
        parts.push(prompt::schema_lines(
            &ctx.o.purpose,
            ctx.open_findings_count > 0,
            true,
        ));
        parts.push(format!(
            "JSON Schema of the reply:\r\n{}",
            schema_text_crlf()
        ));
    }
    parts.push(format!("Consultation id: {}", ctx.consult_id));
    let continue_prompt = parts.join("\r\n\r\n");

    let tmp = std::env::temp_dir();
    let id = uuid::Uuid::new_v4().simple().to_string();
    let continue_last = tmp.join(format!("codex-consult-continue-last-{id}.md"));
    let continue_stderr = tmp.join(format!("codex-consult-continue-stderr-{id}.txt"));
    // The schema flag rides a native (agy/muse) or output-schema (codex) transport; prompt-only
    // re-sends the schema in the prompt instead.
    let schema_arg = if !ctx.r.raw
        && (ctx.transport.transport == "output-schema" || ctx.transport.transport == "native")
    {
        ctx.schema_path.clone()
    } else {
        None
    };

    let stopped_at = if let Some(n) = stalled_secs {
        format!(
            "stopped at {} s ({n} s without an event)",
            fmt_wall(main_wall)
        )
    } else {
        format!(
            "killed at {} s of {} s",
            fmt_wall(main_wall),
            ctx.r.timeout_sec
        )
    };
    println!(
        "{TOOL}: the main turn was {stopped_at}; one continuation turn on thread {} (up to {} s)",
        continue_thread, ctx.r.continue_sec
    );
    sec.continue_ran = true;
    // The continuation event stream is a further turn (added before the run, like the plugin).
    sec.continue_events_rel = Some(continue_events_rel.clone());

    let (outcome, wall, engine_outcome, claude_run) = run_codex_secondary(RunSecondary {
        ctx,
        kind: TurnKind::TimeoutContinuation,
        sandbox: &sandbox_label(&ctx.o),
        effort: ctx.effort.sent.clone(),
        schema_arg,
        thread: &continue_thread,
        prompt_text: &continue_prompt,
        last_path: &continue_last,
        events_path: &continue_events_path,
        stderr_path: &continue_stderr,
        timeout_sec: ctx.r.continue_sec as f64,
        on_running,
        // (wave 3e, F30-2) its kill writes the record that names the tree BEFORE the kill
        on_kill: sec
            .kill_site
            .as_ref()
            .map(|s| s.hook("timeout continuation")),
    });
    let is_engine = !ctx.is_codex();
    sec.continue_wall = wall;
    sec.claude_turns.extend(claude_run);
    let events_field = if continue_events_path.is_file() {
        Some(continue_events_rel.clone())
    } else {
        None
    };

    let mut continue_problem = String::new();
    let mut cont_usage: Option<Usage> = None;
    match outcome {
        AttemptOutcome::Completed(reply) => {
            cont_usage = reply.usage.clone();
            let cont_raw = reply.raw_text.trim().to_string();
            let cont_thread = match &reply.conversation {
                c3_core::engine::ConversationTrust::Verified(c)
                | c3_core::engine::ConversationTrust::Candidate(c) => c.0.clone(),
                _ => String::new(),
            };
            if !cont_thread.is_empty() && cont_thread != continue_thread {
                continue_problem = format!(
                    "the continuation came back on thread {cont_thread}, not {continue_thread}"
                );
            } else if cont_raw.is_empty() {
                continue_problem = "empty reply".to_string();
            } else {
                let (ok, reason) =
                    crate::consult::secondary::test_continuation_reply(&cont_raw, ctx.r.raw);
                if !ok {
                    continue_problem = format!("not a usable reply - {reason}");
                    sec.continue_rejected = true;
                    sec.continue_rejected_text = cont_raw.clone();
                    sec.continue_rejected_why = reason;
                }
            }
            if continue_problem.is_empty() {
                sec.continued = true;
                *bridge_outcome = "usable reply".to_string();
                *usable = true;
                *raw_text = reply.raw_text.clone();
                *thread = continue_thread.clone();
                *thread_source = "events".to_string();
            }
        }
        AttemptOutcome::Stopped {
            kind: c3_core::engine::StopKind::Kick,
            survivors,
            kill,
            ..
        } => {
            // (D2) the operator kicked the continuation: the continuation records the operator
            // stop, but the RUN keeps the main turn's timeout outcome and its salvage. No operator
            // provider_failure (the failure of record is the timeout, not the operator).
            // (wave 28e, E18/E23) its kill keeps the record as any kill does
            let _ = secondary_kill(
                "stopped by the operator (-Kick)",
                kill,
                &survivors,
                "timeout continuation",
                sec,
            );
            continue_problem = "stopped by the operator (-Kick)".to_string();
            sec.continue_kicked = true;
            let _ = std::fs::remove_file(&ctx.kick_path);
        }
        AttemptOutcome::TimedOut {
            survivors, kill, ..
        }
        | AttemptOutcome::Stopped {
            survivors, kill, ..
        } => {
            // (wave 27c D16; 28e E1/E18/E23) the kill told as confirmed or not; survivors,
            // unverified descendants or an unknown tree keep the record
            continue_problem = secondary_kill(
                &format!("timeout after {} s", ctx.r.continue_sec),
                kill,
                &survivors,
                "timeout continuation",
                sec,
            );
            sec.continue_killed = true;
        }
        AttemptOutcome::ProviderFailure {
            failure: pf,
            exit_code,
        } => {
            // An engine frames the failure with its own turn outcome; codex frames a non-clean
            // continuation exit as `codex exit N - <err>` (bare `codex exit N` with no error text).
            if is_engine {
                continue_problem = engine_outcome.trim_start_matches("failed: ").to_string();
                *provider_failure = Some(finalize_pf(pf));
            } else {
                let n = exit_code.unwrap_or(-1);
                continue_problem = if pf.message.trim().is_empty() {
                    format!("codex exit {n}")
                } else {
                    format!("codex exit {n} - {}", pf.message)
                };
                *provider_failure = Some(finalize_pf(pf));
            }
        }
        AttemptOutcome::LaunchFailed { message, .. } => {
            continue_problem = format!("could not start codex - {message}");
        }
        AttemptOutcome::Cancelled => {
            continue_problem = "cancelled".to_string();
        }
    }

    sec.continue_usage = cont_usage.clone();
    sec.timeout_continue = Some(c3_core::ledger::TimeoutContinue {
        thread: continue_thread,
        wall_seconds: wall,
        outcome: if sec.continued {
            "usable reply".to_string()
        } else {
            format!("failed: {continue_problem}")
        },
        events: events_field,
        usage: cont_usage,
        ..Default::default()
    });
}

/// The format repair (`codex-consult.ps1:3845`): a substantive prose reply on a verified thread
/// earns ONE convert-only `resume <thread>` turn at the lowest effort, no `--output-schema`.
/// Inputs to [`run_format_repair`]: the run context and its store/records, the verified thread and
/// whether the reply is usable, plus the `&mut` reply state the repair turn updates. Borrows and
/// `&mut` slots are moved in.
pub(super) struct FormatRepair<'a> {
    pub(super) ctx: &'a Context,
    pub(super) store: &'a FilesStore,
    pub(super) pending: &'a PendingRef,
    pub(super) base_record: &'a PendingRecord,
    pub(super) thread: &'a str,
    pub(super) thread_source: &'a str,
    pub(super) usable: bool,
    pub(super) sec: &'a mut Secondary,
    pub(super) structured: &'a mut Option<StructuredReply>,
    pub(super) raw_text: &'a mut String,
    pub(super) validation_error: &'a mut String,
}

pub(super) fn run_format_repair(args: FormatRepair) {
    let FormatRepair {
        ctx,
        store,
        pending,
        base_record,
        thread,
        thread_source,
        usable,
        sec,
        structured,
        raw_text,
        validation_error,
    } = args;
    let eligible = ctx.r.repair_enabled
        && structured.is_none()
        && usable
        && !ctx.r.raw
        && !thread.is_empty()
        && thread_source == "events";
    if !eligible {
        return;
    }
    let gate = crate::consult::secondary::prose_gate(raw_text);
    if !gate.substantive {
        // No repair turn: the validation_error names why (`(format repair not attempted: ...)`).
        *validation_error = format!(
            "{} (format repair not attempted: {})",
            validation_error, gate.reason
        );
        return;
    }

    let original_prose = raw_text.clone();
    let mut repair_reason = validation_error.clone();
    if repair_reason.chars().count() > 200 {
        repair_reason = repair_reason.chars().take(200).collect();
    }
    sec.repair_reason = repair_reason;

    // The original prose, byte for byte, kept next to the handoff BEFORE the repair process.
    let original_full = ctx.hpath("original.md");
    let _ = c3_core::store::write_text_atomic(&original_full, original_prose.as_bytes());
    sec.original_rel = ctx.hf("original.md");
    sec.original_prose = original_prose;

    // The recovery record names the saved prose BEFORE the repair process exists (state
    // launching), then the callback flips it to running with the repair pid.
    let original_repo_rel = c3_core::paths::repo_relative(&ctx.repo_root, &original_full)
        .unwrap_or_else(|| original_full.to_string_lossy().to_string());
    let mut launching = base_record.clone();
    launching.state = PendingState::Launching;
    launching.original = Some(original_repo_rel.clone());
    launching.first_reply = Some("usable prose (format repair in progress)".to_string());
    launching.note = "format repair turn being started; its pid is not recorded yet".to_string();
    let _ = store.write_pending(pending, &launching);

    let rec_arc = std::sync::Arc::new(std::sync::Mutex::new(launching));
    let on_running: std::sync::Arc<dyn Fn(u32, String) + Send + Sync> = {
        let cb_store = store.clone();
        let cb_pending = pending.clone();
        let cb_rec = std::sync::Arc::clone(&rec_arc);
        std::sync::Arc::new(move |child_pid: u32, child_start: String| {
            if let Ok(mut r) = cb_rec.lock() {
                r.state = PendingState::Running;
                r.child_pid = Some(child_pid);
                r.child_start_time = child_start;
                r.note = "format repair turn".to_string();
                let _ = cb_store.write_pending(&cb_pending, &r);
            }
        })
    };

    let repair_timeout = ctx.r.timeout_sec.min(300);
    sec.repair_timeout = repair_timeout;
    let tmp = std::env::temp_dir();
    let id = uuid::Uuid::new_v4().simple().to_string();
    let repair_last = tmp.join(format!("codex-consult-repair-last-{id}.md"));
    // codex keeps the repair event stream in a temp file (removed → the ledger `events` is null);
    // an engine keeps it next to the handoff (`NN-<prefix>-<reply>.repair.events.jsonl`).
    let is_engine = !ctx.is_codex();
    let repair_events = if is_engine {
        ctx.hpath("repair.events.jsonl")
    } else {
        tmp.join(format!("codex-consult-repair-events-{id}.jsonl"))
    };
    let repair_events_rel = ctx.hf("repair.events.jsonl");
    let repair_stderr = tmp.join(format!("codex-consult-repair-stderr-{id}.txt"));
    // An engine on a native transport passes the schema flag on the repair turn too (codex is
    // always prompt-only on a repair turn — no schema flag).
    let repair_schema = if is_engine && ctx.transport.transport == "native" {
        ctx.schema_path.clone()
    } else {
        None
    };

    let repair_prompt = format_repair_prompt(&ctx.consult_id);

    let (outcome, wall, engine_outcome, claude_run) = run_codex_secondary(RunSecondary {
        ctx,
        kind: TurnKind::FormatRepair,
        sandbox: "read-only",
        effort: repair_effort(ctx),
        schema_arg: repair_schema,
        thread,
        prompt_text: &repair_prompt,
        last_path: &repair_last,
        events_path: &repair_events,
        stderr_path: &repair_stderr,
        timeout_sec: repair_timeout as f64,
        on_running: Some(on_running),
        // (wave 3e, F30-2) its kill writes the record that names the tree BEFORE the kill
        on_kill: sec.kill_site.as_ref().map(|s| s.hook("format repair")),
    });
    // (wave 3c, F23-3) a kill of this turn writes its record on the one this turn has on disk (it
    // names the saved prose and the repair's pid)
    if let (Some(site), Ok(r)) = (sec.kill_site.as_mut(), rec_arc.lock()) {
        site.on = r.clone();
    }

    sec.claude_turns.extend(claude_run);
    let mut repair_problem = String::new();
    let mut repair_usage: Option<Usage> = None;
    let mut repair_thread = String::new();
    let mut repaired: Option<StructuredReply> = None;
    match outcome {
        AttemptOutcome::Completed(reply) => {
            repair_usage = reply.usage.clone();
            repair_thread = match &reply.conversation {
                c3_core::engine::ConversationTrust::Verified(c)
                | c3_core::engine::ConversationTrust::Candidate(c) => c.0.clone(),
                _ => String::new(),
            };
            let rr = reply.raw_text.trim();
            if rr.is_empty() {
                repair_problem = "empty reply".to_string();
            } else {
                match crate::engines::codex::parse_structured(rr) {
                    Some(s) => {
                        // .reply.json holds the repaired object, byte for byte.
                        *raw_text = reply.raw_text.clone();
                        repaired = Some(s);
                    }
                    None => {
                        repair_problem = format!(
                            "still not valid: {}",
                            ingest::first_validation_error_engine(rr, &ctx.engine)
                        );
                    }
                }
            }
        }
        AttemptOutcome::Stopped {
            kind: c3_core::engine::StopKind::Kick,
            survivors,
            kill,
            ..
        } => {
            // (wave 26c, D1) the operator stopped the format repair only: the first reply STANDS
            // (not converted). No operator class; a warning is emitted in the caller.
            // (wave 28e, E18/E23) its kill keeps the record as any kill does
            let _ = secondary_kill(
                "stopped by the operator (-Kick)",
                kill,
                &survivors,
                "format repair",
                sec,
            );
            sec.repair_kicked = true;
            repair_problem = "the operator stopped the format repair (-Kick)".to_string();
        }
        AttemptOutcome::TimedOut {
            survivors, kill, ..
        }
        | AttemptOutcome::Stopped {
            survivors, kill, ..
        } => {
            // (wave 27c D16; 28e E1/E18/E23) as the continuation's kill
            repair_problem = secondary_kill(
                &format!("timeout after {repair_timeout} s"),
                kill,
                &survivors,
                "format repair",
                sec,
            );
            sec.repair_killed = true;
        }
        AttemptOutcome::ProviderFailure { failure, exit_code } => {
            if is_engine {
                // An engine repair failure keeps the turn's own framed outcome (e.g. a resume that
                // landed on a new conversation: `parent conversation X not found, agy started Y`).
                let framed = engine_outcome.trim_start_matches("failed: ").to_string();
                repair_problem = if !framed.is_empty() {
                    framed
                } else if failure.message.trim().is_empty() {
                    format!("{} exit {}", ctx.engine, exit_code.unwrap_or(-1))
                } else {
                    failure.message.clone()
                };
            } else {
                // The plugin frames a non-clean codex repair exit as a bare `codex exit N`.
                repair_problem = format!("codex exit {}", exit_code.unwrap_or(-1));
            }
        }
        AttemptOutcome::LaunchFailed { message, .. } => {
            repair_problem = format!("could not start {} - {message}", ctx.engine);
        }
        AttemptOutcome::Cancelled => {
            repair_problem = "cancelled".to_string();
        }
    }

    // Drift notes: a different repair thread, then the prose-vs-object comparison.
    let mut drift_notes: Vec<String> = Vec::new();
    if !repair_thread.is_empty() && repair_thread != thread {
        drift_notes.push("repair returned a different thread id".to_string());
    }
    if let Some(s) = &repaired {
        for d in crate::consult::secondary::get_format_repair_drift(&sec.original_prose, s) {
            drift_notes.push(d);
        }
        *validation_error = String::new();
        sec.repaired_ok = true;
        *structured = repaired;
    } else {
        *validation_error = format!(
            "{} (format repair failed: {})",
            validation_error,
            c3_core::one_line(&repair_problem)
        );
    }
    sec.drift_notes = drift_notes.clone();

    sec.format_retry = Some(c3_core::ledger::FormatRetry {
        attempted: true,
        reason: sec.repair_reason.clone(),
        succeeded: sec.repaired_ok,
        thread: repair_thread,
        wall_seconds: wall,
        usage: repair_usage,
        drift: drift_notes
            .iter()
            .map(|d| serde_json::Value::String(d.clone()))
            .collect(),
        original: sec.original_rel.clone(),
        // codex's repair event stream is a temp file (removed → null); an engine keeps it at the
        // handoff path.
        events: if is_engine && repair_events.is_file() {
            Some(repair_events_rel.clone())
        } else {
            None
        },
        schema_transport: if is_engine {
            ctx.transport.transport.clone()
        } else {
            "prompt-only".to_string()
        },
        ..Default::default()
    });
    if is_engine {
        sec.repair_events_rel = Some(repair_events_rel);
    }
    sec.repair_wall = wall;
    sec.repair_console = format!(
        "format repair: {} in {} s; drift: {} note(s)",
        if sec.repaired_ok {
            "succeeded"
        } else {
            "failed"
        },
        fmt_wall(wall),
        drift_notes.len()
    );

    // temp cleanup (an engine's kept repair events file is NOT removed). (wave 28c, D11) A codex
    // repair turn's temp event stream is counted for compactions before it goes.
    let _ = std::fs::remove_file(&repair_last);
    if !is_engine {
        sec.repair_compactions = compaction_count(&[repair_events.as_path()]);
        let _ = std::fs::remove_file(&repair_events);
    }
    let _ = std::fs::remove_file(&repair_stderr);
}

/// The lowest effort of the endpoint's vocabulary for a repair turn (`Get-RepairEffort`).
fn repair_effort(ctx: &Context) -> Option<String> {
    if ctx.effort.mapping == "native" {
        return ctx.effort.sent.clone();
    }
    let host = &ctx.identity.host;
    if !host.is_empty() {
        if let Some(cap) = caps(host) {
            if let Some((_, low)) = vocabulary_map(cap.vocabulary, "low") {
                return Some(low.to_string());
            }
        }
    }
    ctx.effort.sent.clone()
}

/// Run one engine secondary turn (currently the agy denial retry) and return the outcome, the
/// engine detail, the wall seconds and the kept events-file rel path.
fn run_engine_secondary(
    ctx: &Context,
    kind: TurnKind,
    thread: &str,
    prompt_text: &str,
    events_suffix: &str,
    timeout_sec: f64,
) -> (AttemptOutcome, EngineDetail, f64, String) {
    let events_path = ctx.hpath(events_suffix);
    let id = uuid::Uuid::new_v4().simple().to_string();
    let stderr_path =
        std::env::temp_dir().join(format!("codex-consult-{}-stderr-{id}.txt", ctx.file_prefix));
    let prompt_file = if ctx.prompt_file.is_some() {
        let pf = std::env::temp_dir().join(format!("codex-consult-prompt-{id}.txt"));
        let _ = c3_core::store::write_text_atomic(&pf, prompt_text.as_bytes());
        Some(pf)
    } else {
        None
    };
    let mut request = make_live_request_engine(ctx, Mode::New);
    request.prompt = prompt_text.to_string();
    request.prompt_file = prompt_file;
    request.timeout_sec = timeout_sec;
    let lineage = request.lineage();
    request.mode = Mode::Resume {
        thread: thread.to_string(),
        lineage,
    };
    let files = TurnFiles {
        events: events_path.clone(),
        stderr: stderr_path.clone(),
    };
    let turn = TurnRequest {
        request,
        consultation: ConsultationId(ctx.consult_id.clone()),
        attempt: c3_core::engine::AttemptId(ctx.consult_id.clone()),
        kind,
        continuation: Some(c3_core::engine::Continuation::Native(
            c3_core::engine::ConversationId(thread.to_string()),
        )),
    };
    let start = std::time::Instant::now();
    let (outcome, detail) = match ctx.engine.as_str() {
        "claude" => {
            let eng = claude_engine(ctx, TurnFiles::default(), files, 0, None, None, None);
            match eng.run_detailed(&turn) {
                Ok(r) => (r.outcome.clone(), claude_detail(&r)),
                Err(e) => (
                    AttemptOutcome::LaunchFailed {
                        child_exists: false,
                        message: format!("{e:?}"),
                    },
                    EngineDetail::default(),
                ),
            }
        }
        "agy" => {
            let eng = crate::engines::agy::AgyEngine {
                launcher: ctx.engine_launcher.clone(),
                cwd: ctx.repo_root.clone(),
                primary: TurnFiles::default(),
                secondary: files,
                no_network: false,
                models_timeout_sec: 45,
                stall_sec: 0,
                kick_path: None,
                on_running: None,
                on_kill: None,
            };
            match eng.run_detailed(&turn) {
                Ok(r) => (r.outcome, agy_detail(&r.turn)),
                Err(e) => (
                    AttemptOutcome::LaunchFailed {
                        child_exists: false,
                        message: format!("{e:?}"),
                    },
                    EngineDetail::default(),
                ),
            }
        }
        "grok" => {
            let eng = crate::engines::grok::GrokEngine {
                launcher: ctx.engine_launcher.clone(),
                cwd: ctx.repo_root.clone(),
                primary: TurnFiles::default(),
                secondary: files,
                stall_sec: 0,
                // A secondary turn is exempt from the review guard (primary-only); purpose carried
                // for completeness.
                purpose: ctx.o.purpose.clone(),
                kick_path: None,
                on_running: None,
                on_kill: None,
            };
            match eng.run_detailed(&turn) {
                Ok(r) => (r.outcome, grok_detail(&r.turn)),
                Err(e) => (
                    AttemptOutcome::LaunchFailed {
                        child_exists: false,
                        message: format!("{e:?}"),
                    },
                    EngineDetail::default(),
                ),
            }
        }
        _ => {
            let eng = crate::engines::muse::MuseEngine {
                launcher: ctx.engine_launcher.clone(),
                cwd: ctx.repo_root.clone(),
                primary: TurnFiles::default(),
                secondary: files,
                stall_sec: 0,
                kick_path: None,
                on_running: None,
                on_kill: None,
            };
            match eng.run_detailed(&turn) {
                Ok(r) => (r.outcome, muse_detail(&r.turn)),
                Err(e) => (
                    AttemptOutcome::LaunchFailed {
                        child_exists: false,
                        message: format!("{e:?}"),
                    },
                    EngineDetail::default(),
                ),
            }
        }
    };
    let wall = round1(start.elapsed().as_secs_f64());
    let events_rel = if events_path.is_file() {
        ctx.hf(events_suffix)
    } else {
        String::new()
    };
    let _ = std::fs::remove_file(&stderr_path);
    (outcome, detail, wall, events_rel)
}

/// The agy denial retry (F11): a tool was auto-denied and the turn produced nothing → one more
/// `--conversation <thread>` turn telling the model not to call it. Gated by the engine's
/// denial-retry capability, `--denial-retry 1`, a verified thread and no tree problem.
/// Inputs to [`run_engine_denial_retry`]: the run context, the engine detail and tree-kill problem,
/// plus the `&mut` run state the retry updates. Borrows and `&mut` slots are moved in.
pub(super) struct DenialRetry<'a> {
    pub(super) ctx: &'a Context,
    pub(super) detail: &'a EngineDetail,
    pub(super) tree_problem: &'a str,
    pub(super) sec: &'a mut Secondary,
    pub(super) bridge_outcome: &'a mut String,
    pub(super) raw_text: &'a mut String,
    pub(super) thread: &'a mut String,
    pub(super) thread_source: &'a mut String,
    pub(super) usable: &'a mut bool,
    pub(super) forced_class: &'a mut String,
}

pub(super) fn run_engine_denial_retry(args: DenialRetry) {
    let DenialRetry {
        ctx,
        detail,
        tree_problem,
        sec,
        bridge_outcome,
        raw_text,
        thread,
        thread_source,
        usable,
        forced_class,
    } = args;
    let denial_ok = c3_core::lineage::engine_spec(&ctx.engine)
        .map(|s| s.denial_retry)
        .unwrap_or(false);
    if !(denial_ok
        && ctx.o.denial_retry == 1
        && detail.denied_empty
        && !thread.is_empty()
        && tree_problem.is_empty())
    {
        return;
    }
    let mut reason = detail.denial_line.clone();
    if reason.chars().count() > 200 {
        reason = reason.chars().take(200).collect();
    }
    let denied_tool = if !detail.tool_name.is_empty() {
        detail.tool_name.clone()
    } else {
        detail.denied_action.clone()
    };
    let tool_text = if denied_tool.is_empty() {
        "a tool".to_string()
    } else {
        format!("the tool {denied_tool}")
    };
    let perm_text = if detail.permission.is_empty() {
        "(headless print mode cannot grant its permission)".to_string()
    } else {
        format!(
            "(headless print mode has no \"{}\" permission)",
            detail.permission
        )
    };
    let mut parts: Vec<String> = Vec::new();
    if !ctx.r.raw {
        parts.push(prompt::FINAL_OUTPUT_CONTRACT.to_string());
        parts.push(format!(
            "Your previous turn produced no output: {tool_text} was auto-denied {perm_text}. Do NOT call it again; answer from what you have read, as the JSON object."
        ));
        parts.push(prompt::schema_lines(
            &ctx.o.purpose,
            ctx.open_findings_count > 0,
            true,
        ));
        if ctx.transport.transport == "prompt-only" {
            parts.push(format!(
                "JSON Schema of the reply:\r\n{}",
                schema_text_crlf()
            ));
        }
    } else {
        parts.push(format!(
            "Your previous turn produced no output: {tool_text} was auto-denied {perm_text}. Do NOT call it again; answer from what you have read."
        ));
    }
    parts.push(format!("Consultation id: {}", ctx.consult_id));
    let retry_prompt = parts.join("\r\n\r\n");

    let thread_id = thread.clone();
    let timeout = ctx.r.timeout_sec.min(300);
    let (outcome, det, wall, events_rel) = run_engine_secondary(
        ctx,
        TurnKind::DenialRetry,
        &thread_id,
        &retry_prompt,
        "denial-retry.events.jsonl",
        timeout as f64,
    );
    sec.engine_turns += 1;
    sec.claude_turns.extend(det.claude.clone());

    let mut succeeded = false;
    // Default to the failure thread (the turn's observed thread); a usable retry overrides it.
    let mut retry_thread = det.thread.clone();
    let mut retry_usage: Option<Usage> = None;
    match outcome {
        AttemptOutcome::Completed(reply) => {
            succeeded = true;
            *bridge_outcome = "usable reply".to_string();
            *usable = true;
            *forced_class = String::new();
            *raw_text = reply.raw_text.clone();
            retry_usage = reply.usage.clone();
            retry_thread = match &reply.conversation {
                c3_core::engine::ConversationTrust::Verified(c)
                | c3_core::engine::ConversationTrust::Candidate(c) => c.0.clone(),
                _ => String::new(),
            };
            if !retry_thread.is_empty() {
                *thread = retry_thread.clone();
            }
            *thread_source = "events".to_string();
            sec.engine_warnings.push(format!(
                "denial notice (the first turn produced nothing; the denial-retry turn answered): {}",
                c3_core::one_line(&detail.denial_line)
            ));
            for w in &det.warnings {
                sec.engine_warnings.push(w.clone());
            }
        }
        _ => {
            let stripped = det.turn_outcome.trim_start_matches("failed: ");
            *bridge_outcome = format!(
                "{} (denial retry failed: {})",
                bridge_outcome,
                c3_core::one_line(stripped)
            );
        }
    }

    sec.denial_retry = Some(c3_core::ledger::DenialRetry {
        attempted: true,
        reason,
        succeeded,
        thread: retry_thread,
        wall_seconds: wall,
        usage: retry_usage,
        events: if events_rel.is_empty() {
            None
        } else {
            Some(events_rel)
        },
        ..Default::default()
    });
    sec.denial_console = format!(
        "denial retry: {} in {} s",
        if succeeded { "succeeded" } else { "failed" },
        fmt_wall(wall)
    );
}

/// Inputs to [`build_partial_reply`]: the run context, the main turn's events text and the `&mut`
/// secondary state, the timeout/stall/kick/usable flags, the main wall and outcome/thread, and any
/// budget-stop text. Borrows and the `&mut` slot are moved in.
pub(super) struct PartialReply<'a> {
    pub(super) ctx: &'a Context,
    pub(super) main_events_text: &'a str,
    pub(super) sec: &'a mut Secondary,
    pub(super) main_timed_out: bool,
    pub(super) main_wall: f64,
    // (wave 26b) extra context: a stall kill, an operator kick, whether the run is usable, the
    // failure outcome text (for D15's "the run ended: <why>") and the run's own thread.
    pub(super) stall: bool,
    pub(super) run_kicked: bool,
    pub(super) usable: bool,
    pub(super) bridge_outcome: &'a str,
    pub(super) main_thread: &'a str,
    // (wave 8 = plugin wave 30, T15) the main turn was stopped at the quota budget: its stop text
    pub(super) budget_stop: Option<&'a str>,
}

pub(super) fn build_partial_reply(args: PartialReply) {
    let PartialReply {
        ctx,
        main_events_text,
        sec,
        main_timed_out,
        main_wall,
        stall,
        run_kicked,
        usable,
        bridge_outcome,
        main_thread,
        budget_stop,
    } = args;
    let killed_partial = (main_timed_out && !sec.continued)
        || sec.continue_killed
        || sec.repair_killed
        || run_kicked
        || budget_stop.is_some();
    // (wave 26b, D15) ANY failed run keeps what its reviewer produced: when the run is not usable
    // and its main event stream holds at least one agent message, reasoning text or tool call, the
    // salvage is written even though no turn was killed (codex only; engines keep their own path).
    let mut partial_on_failure = false;
    if !killed_partial && !usable && ctx.is_codex() {
        let s = crate::consult::secondary::read_codex_salvage(main_events_text);
        if !s.items.is_empty() || !s.tools.is_empty() {
            partial_on_failure = true;
        }
    }
    if !killed_partial && !partial_on_failure {
        return;
    }
    sec.partial_needed = true;
    sec.partial_rel = ctx.hf("partial.md");
    // The one-lined failure reason for the "it ended at ... / the run ended: ..." wording (D15).
    let ended_why = {
        let raw = bridge_outcome
            .strip_prefix("failed: ")
            .unwrap_or(bridge_outcome);
        let one = c3_core::one_line(raw);
        if one.chars().count() > 200 {
            let cut: String = one.chars().take(200).collect();
            format!("{cut}...")
        } else {
            one
        }
    };

    let mut turns: Vec<crate::consult::secondary::PartialTurn> = Vec::new();
    let mut killed_at: Vec<String> = Vec::new();
    turns.push(crate::consult::secondary::PartialTurn {
        label: "Turn 1 - the main turn".to_string(),
        note: if let Some(bs) = budget_stop {
            format!("stopped at {} s: {bs}", fmt_wall(main_wall))
        } else if run_kicked {
            format!(
                "stopped by the operator (-Kick) at {} s",
                fmt_wall(main_wall)
            )
        } else if stall {
            format!(
                "stopped at {} s: {} s without an event",
                fmt_wall(main_wall),
                ctx.stall_sec
            )
        } else if main_timed_out {
            format!(
                "killed at {} s of {} s",
                fmt_wall(main_wall),
                ctx.r.timeout_sec
            )
        } else if partial_on_failure {
            format!("it ended at {} s: {}", fmt_wall(main_wall), ended_why)
        } else {
            "it ended by itself".to_string()
        },
        salvage: turn_salvage(&ctx.engine, main_events_text),
    });
    if budget_stop.is_some() {
        killed_at.push(format!(
            "{} s at the quota budget (the main turn)",
            fmt_wall(main_wall)
        ));
    } else if run_kicked {
        killed_at.push(format!("{} s (the main turn)", fmt_wall(main_wall)));
    } else if main_timed_out && !stall {
        killed_at.push(format!(
            "{} s of {} s (the main turn)",
            fmt_wall(main_wall),
            ctx.r.timeout_sec
        ));
    }
    if sec.continue_ran {
        let cont_events = ctx.hpath("continue.events.jsonl");
        let cont_text = std::fs::read_to_string(&cont_events).unwrap_or_default();
        let note = if sec.continue_killed {
            format!(
                "killed at {} s of {} s",
                fmt_wall(sec.continue_wall),
                ctx.r.continue_sec
            )
        } else if sec.continued {
            format!("it answered in {} s", fmt_wall(sec.continue_wall))
        } else {
            let why = sec
                .timeout_continue
                .as_ref()
                .map(|t| t.outcome.trim_start_matches("failed: ").to_string())
                .unwrap_or_default();
            format!("failed: {why}")
        };
        turns.push(crate::consult::secondary::PartialTurn {
            label: format!("Turn {} - the timeout continuation", turns.len() + 1),
            note,
            salvage: turn_salvage(&ctx.engine, &cont_text),
        });
        if sec.continue_killed {
            killed_at.push(format!(
                "{} s of {} s (the timeout continuation)",
                fmt_wall(sec.continue_wall),
                ctx.r.continue_sec
            ));
        }
    }

    let mut body = crate::consult::secondary::format_partial_body(&turns);
    // A continuation reply the checks REJECTED is not thrown away.
    if sec.continue_rejected && !sec.continue_rejected_text.trim().is_empty() {
        body = format!(
            "{}\n\n## continuation reply (rejected: {})\n\n{}\n",
            body.trim_end(),
            sec.continue_rejected_why,
            sec.continue_rejected_text.trim().replace("\r\n", "\n")
        );
        if let Some(tc) = &mut sec.timeout_continue {
            tc.outcome = format!(
                "{}; its text is kept in {} under \"continuation reply (rejected)\"",
                tc.outcome, sec.partial_rel
            );
        }
    }
    sec.partial_body = body;

    // The resume thread: the killed turn's continuation thread, else (D15, a mid-run failure that
    // never ran a continuation) the run's own thread.
    // (wave 8) a kicked or budget-stopped codex turn names its own thread too (the plugin's
    // `$threadId` first).
    let resume_thread = if !sec.continue_thread.is_empty() {
        sec.continue_thread.clone()
    } else if (partial_on_failure || run_kicked || budget_stop.is_some()) && !main_thread.is_empty()
    {
        main_thread.to_string()
    } else {
        String::new()
    };
    let killed_text = if killed_at.len() == 1 {
        // strip the trailing " (...)"
        let s = &killed_at[0];
        s.rsplit_once(" (")
            .map(|(a, _)| a.to_string())
            .unwrap_or_else(|| s.clone())
    } else {
        killed_at.join(", ")
    };
    // (wave 26b, D15) a turn was killed -> "killed at ..."; no turn was killed (a mid-run
    // failure or a stall with no continuation) -> "the run ended: <why>".
    let ended_text = if killed_at.is_empty() {
        format!("the run ended: {ended_why}")
    } else {
        format!("killed at {killed_text}")
    };
    if !resume_thread.is_empty() {
        // (wave 8 = plugin wave 30, T15) a codex thread whose rollout is over
        // CODEX_CONSULT_FORK_MAX_KB would be refused by the replay guard: the hint continues in a
        // NEW thread with the brief and the partial
        let resume_guard = ctx.is_codex().then(|| {
            c3_core::quota::fork_replay_guard(
                &resume_thread,
                &providers::get_codex_home(),
                std::env::var("CODEX_CONSULT_FORK_MAX_KB").ok().as_deref(),
            )
        });
        let resume_new = resume_guard.as_ref().map(|g| g.over_limit).unwrap_or(false);
        let args = summary::build_resume_command(&summary::ResumeInputs {
            task: ctx.o.task.clone(),
            collab_dir: ctx.o.collab_dir.clone(),
            thread: resume_thread.clone(),
            no_roster: true,
            provider: ctx.identity.provider.clone(),
            model: ctx.identity.model.clone(),
            purpose: ctx.o.purpose.clone(),
            raw: ctx.r.raw,
            reply_name: ctx.o.reply_name.clone(),
            reply_name_given: !ctx.o.reply_name.is_empty(),
            timeout_source: ctx.r.timeout_source.clone(),
            timeout_sec: ctx.r.timeout_sec,
            continue_sec: ctx.r.continue_sec,
            effort: ctx.o.effort.clone(),
            native_effort: ctx.o.native_effort.clone(),
            max_words: ctx.o.max_words,
            transport_override: ctx.r.transport_override.clone(),
            codex_config: ctx.o.codex_config.clone(),
            artifacts: ctx.o.artifacts.clone(),
            range: ctx.o.range.clone(),
            sandbox: sandbox_label(&ctx.o),
            format_retry: ctx.o.format_retry,
            off_peak_only: ctx.o.off_peak_only,
            skip_preflight: ctx.o.skip_preflight,
            codex_exe: ctx.o.codex_exe.clone(),
            budget: ctx.o.budget.filter(|_| ctx.o.budget_source.is_empty()),
            new_thread_partial: resume_new.then(|| sec.partial_rel.clone()),
            brief: ctx.brief_ref.clone(),
        });
        sec.partial_footer = match resume_guard.filter(|g| g.over_limit) {
            Some(g) => format!(
                "{ended_text}; thread {resume_thread} replays {} KB on every tool call (over CODEX_CONSULT_FORK_MAX_KB {}) - continue in a new thread with `{args}`",
                g.kb, g.max_kb
            ),
            None => format!("{ended_text}; thread {resume_thread} - continue with `{args}`"),
        };
        sec.resume_command = args;
    } else {
        let which = if killed_at.is_empty() {
            "failed"
        } else {
            "killed"
        };
        sec.partial_footer = format!(
            "{ended_text}; the thread of the {which} turn is not known - no resume is possible (start again with -Mode new)"
        );
    }
}
