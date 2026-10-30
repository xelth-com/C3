//! The **handoff render** of a `c3 consult` run: [`render_handoff`] turns the run context,
//! outcome and (structured or prose) reply into the handoff markdown (and the `.partial.md`
//! salvage when a turn was killed), with [`RenderHandoff`] carrying its borrowed inputs. The
//! per-finding / per-section drawing lives in the sibling [`crate::consult::section`]; this
//! module assembles the whole document around it ([`engine_prompt_via`] and the small
//! render-only text helpers).

use super::*;

/// How the prompt reached the engine, for the handoff `Argv: ... (<prompt_via>)` clause
/// (`EngineSpec.PromptVia`).
pub(super) fn engine_prompt_via(engine: &str) -> &'static str {
    match engine {
        "agy" => "prompt on stdin as one NDJSON line",
        "muse" | "grok" => "prompt from a file: --prompt-file",
        "http" => "reviewer pack sent as the request body",
        _ => "prompt on stdin",
    }
}

/// Inputs to [`render_handoff`]: the run context and outcome/wall/usage, the thread identity, the
/// structured reply and finding ids, the validation/provider failure, the raw text and drift, the
/// secondary state, the main turn's error/stderr, the prior blockers and any verdict warning.
/// Borrowed so the caller passes without cloning.
pub(super) struct RenderHandoff<'a> {
    pub(super) ctx: &'a Context,
    pub(super) bridge_outcome: &'a str,
    pub(super) wall: f64,
    pub(super) usage: &'a Option<Usage>,
    pub(super) thread: &'a str,
    pub(super) thread_source: &'a str,
    pub(super) thread_candidate: &'a str,
    pub(super) structured: Option<&'a StructuredReply>,
    pub(super) finding_ids: &'a [String],
    pub(super) validation_error: &'a str,
    pub(super) provider_failure: Option<&'a c3_core::ledger::ProviderFailure>,
    pub(super) raw_text: &'a str,
    pub(super) drift: &'a Drift,
    pub(super) sec: &'a Secondary,
    pub(super) main_event_error: &'a str,
    pub(super) main_stderr: &'a str,
    pub(super) prior_blockers: &'a [section::PriorBlockerLine],
    pub(super) verdict_warning: Option<&'a str>,
}

pub(super) fn render_handoff(args: RenderHandoff) -> (String, Option<String>) {
    let RenderHandoff {
        ctx,
        bridge_outcome,
        wall,
        usage,
        thread,
        thread_source,
        thread_candidate,
        structured,
        finding_ids,
        validation_error,
        provider_failure,
        raw_text,
        drift,
        sec,
        main_event_error,
        main_stderr,
        prior_blockers,
        verdict_warning,
    } = args;
    let events_rel = ctx.hf("events.jsonl");
    let spec = c3_core::lineage::engine_spec(&ctx.engine);
    let has_usage = spec.as_ref().map(|s| s.has_usage).unwrap_or(true);
    let effort_sent = ctx.effort.sent.clone().unwrap_or_else(|| "nothing".into());
    // codex/agy report usage; a turn that produced none is `unknown` (`Format-Usage $null`); an
    // engine that reports none (muse) renders `not reported by <engine>`.
    let tokens = if !has_usage {
        TokenReport::NotReported {
            engine: ctx.engine.clone(),
        }
    } else {
        match usage {
            Some(u) => TokenReport::Reported {
                input: u.input_tokens,
                cached: u.cached_input_tokens,
                output: u.output_tokens,
                reasoning: u.reasoning_output_tokens,
            },
            None => TokenReport::Unknown,
        }
    };
    let mut records = OptionalRecords {
        // (wave 8 = plugin wave 30) `Quota (rollout): ...` after the Bridge outcome line
        quota: (!sec.quota_line.is_empty()).then(|| {
            format!(
                "Quota (rollout): {}; sub-agents {}; budget {}.",
                sec.quota_line
                    .trim_start_matches("quota")
                    .trim_start()
                    .trim_start_matches(':')
                    .trim_start(),
                if ctx.subagents_allowed == Some(true) {
                    "allowed"
                } else {
                    "off (features.multi_agent=false)"
                },
                if ctx.budget.percent > 0 {
                    format!("{} % of the 5-hour window", ctx.budget.percent)
                } else {
                    "off".to_string()
                }
            )
        }),
        recovery_lines: ctx
            .recovery_lines
            .iter()
            .map(|l| format!("Recovery record: {l}"))
            .collect(),
        ..Default::default()
    };
    if !ctx.peak_warning.is_empty() {
        // The handoff records the past tense ('ran at') of the console warning, with the
        // same `WARNING: ` prefix the console line carries.
        records.peak_warning = Some(format!(
            "WARNING: {}",
            ctx.peak_warning
                .replace("this consultation runs at", "this consultation ran at")
        ));
    }
    // Drift lines (`$driftLines`): tree, HEAD move, brief, artifacts — in that order.
    if drift.tree_changed {
        records.drift_lines.push(
            "WARNING: working tree changed during the review (fingerprint before/after differ)."
                .to_string(),
        );
    }
    if !drift.revision_moved.is_empty() {
        records.drift_lines.push(revision::revision_moved_note(
            &drift.revision_moved,
            drift.tree_changed,
        ));
    }
    if drift.brief_changed {
        records.drift_lines.push(format!(
            "WARNING: the brief changed during the review (sha256 {} before, {} after).",
            short_hash(&ctx.brief_sha),
            short_hash(&drift.brief_sha_after)
        ));
    }
    if !drift.artifacts_changed_paths.is_empty() {
        records.drift_lines.push(format!(
            "WARNING: artifact(s) changed during the review: {}.",
            drift.artifacts_changed_paths.join(", ")
        ));
    }
    // The handoff `Warnings:` line: the run warnings, then an engine run's turn warnings (denial
    // notices, engine stderr warnings) - the ledger's `warnings[]`.
    let mut warn_source: Vec<String> = ctx.run_warnings.clone();
    if !ctx.is_codex() {
        for ew in &sec.engine_warnings {
            if !warn_source.contains(ew) {
                warn_source.push(ew.clone());
            }
        }
    }
    if !warn_source.is_empty() {
        records.warnings = Some(format!(
            "Warnings: {}.",
            warn_source
                .iter()
                .map(|w| c3_core::one_line(w))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    // (14) Engine turns line (agy/muse only, always rendered — even a single turn).
    if !ctx.is_codex() {
        let mut l = format!("Engine turns: {}", sec.engine_turns);
        if ctx.engine == "muse" {
            l.push_str(" (each one a Muse Code subscription prompt)");
        }
        if ctx.engine == "grok" {
            l.push_str(" (each one a SuperGrok subscription prompt)");
        }
        if ctx.engine == "claude" {
            // (wave 29) the auth, the resolved model, the init tools and the denials
            let x = claude_engine_run_extra(ctx, sec);
            let tools: Vec<String> = x["init_tools"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            l.push_str(&format!(
                " (claude -p, auth {}{}; init tools {}; permission denials {})",
                ctx.engine_auth,
                x["model_resolved"]
                    .as_str()
                    .map(|m| format!("; model {m}"))
                    .unwrap_or_default(),
                if tools.is_empty() {
                    "(none seen)".to_string()
                } else {
                    tools.join(", ")
                },
                x["permission_denials"].as_i64().unwrap_or(0)
            ));
        }
        if ctx.o.max_model_steps > 0 {
            let flag = c3_core::lineage::engine_spec(&ctx.engine)
                .map(|s| s.steps_flag)
                .filter(|f| !f.is_empty())
                .unwrap_or("--max-model-steps");
            l.push_str(&format!("; {flag} {}", ctx.o.max_model_steps));
        }
        if let Some(v) = sec.msp_version {
            l.push_str(&format!("; MSP schema_version {v}"));
        }
        l.push('.');
        records.engine_turns = Some(l);
    }
    // (16) Denial retry line (agy).
    if let Some(dr) = &sec.denial_retry {
        records.denial_retry = Some(if dr.succeeded {
            format!(
                "Denial retry: succeeded in {} s - the first turn produced nothing (a tool was auto-denied); one more turn on conversation `{}` answered without it. Tokens of that turn: {}.",
                fmt_wall(dr.wall_seconds),
                dr.thread,
                usage_clause(&dr.usage),
            )
        } else {
            format!(
                "Denial retry: failed in {} s - the first turn produced nothing (a tool was auto-denied) and the retry turn did not answer either.",
                fmt_wall(dr.wall_seconds),
            )
        });
    }
    if let Some(pf) = provider_failure {
        let code = if pf.code.is_empty() {
            String::new()
        } else {
            format!(" ({})", pf.code)
        };
        records.provider_failure = Some(format!(
            "Provider failure: {}{} - {}.",
            pf.class, code, pf.message
        ));
    }
    // (18) Timeout continuation line.
    if let Some(tc) = &sec.timeout_continue {
        if sec.continued {
            records.timeout_continuation = Some(format!(
                "Timeout continuation: the main turn was killed at {} s of {} s; one continuation turn on thread `{}` answered in {} s. Tokens of that turn: {}.",
                fmt_wall(wall),
                ctx.r.timeout_sec,
                sec.continue_thread,
                fmt_wall(sec.continue_wall),
                usage_clause(&sec.continue_usage),
            ));
        } else {
            let mut h = format!("Timeout continuation: {}", tc.outcome);
            if sec.continue_wall > 0.0 || tc.events.is_some() {
                h.push_str(&format!(" (in {} s)", fmt_wall(sec.continue_wall)));
            }
            if !sec.continue_thread.is_empty() {
                h.push_str(&format!(" - thread `{}`", sec.continue_thread));
            }
            h.push('.');
            records.timeout_continuation = Some(h);
        }
    }
    // (19) Partial reply line.
    if sec.partial_needed {
        records.partial_reply = Some(format!(
            "Partial reply: `{}` - {}.",
            sec.partial_rel, sec.partial_footer
        ));
    }
    // (23) Format repair line.
    if let Some(fr) = &sec.format_retry {
        let drift_text = if sec.drift_notes.is_empty() {
            "none".to_string()
        } else {
            format!(
                "{} note(s): {}",
                sec.drift_notes.len(),
                sec.drift_notes.join("; ")
            )
        };
        // The http engine keeps no thread: its repair turn replays the conversation rather than
        // resuming a thread.
        let repair_how = if ctx.engine == "http" {
            "replayed the conversation (the http engine keeps no thread)".to_string()
        } else {
            format!("resumed thread `{thread}`")
        };
        records.format_repair = Some(if sec.repaired_ok {
            format!(
                "Format repair: succeeded in {} s - the first reply was prose ({}); one repair turn {} and converted it. Drift: {}. The original prose follows the structured section and is kept as `{}`.",
                fmt_wall(fr.wall_seconds),
                c3_core::one_line(&sec.repair_reason),
                repair_how,
                drift_text,
                sec.original_rel,
            )
        } else {
            format!(
                "Format repair: failed in {} s - the first reply was prose ({}) and the repair turn did not produce a valid object; the prose is kept below (also `{}`).",
                fmt_wall(fr.wall_seconds),
                c3_core::one_line(&sec.repair_reason),
                sec.original_rel,
            )
        });
    }
    let reply_json_rel = if !ctx.r.raw && !raw_text.is_empty() {
        ctx.hf("reply.json")
    } else {
        String::new()
    };
    // Format-StructuredStatusLine.
    let verdict_line = structured.map(|s| {
        let v = match s.verdict {
            c3_core::engine::Verdict::Accept => "ACCEPT",
            c3_core::engine::Verdict::Hold => "HOLD",
            c3_core::engine::Verdict::Reject => "REJECT",
            c3_core::engine::Verdict::Advise => "ADVISE",
        };
        let reason = section::close_sentence(&s.verdict_reason);
        let mut vt = format!("Verdict: {v}");
        if reason.is_empty() {
            vt.push('.');
        } else {
            vt.push_str(&format!(" - {reason}"));
        }
        let counts = section::severity_counts(s);
        let ft = if s.findings.is_empty() {
            "Findings: none.".to_string()
        } else {
            format!(
                "Findings: {} ({}, tracked in `findings.json`).",
                section::format_severity_counts(&counts),
                section::format_id_range(finding_ids)
            )
        };
        let mut line = format!("{vt} {ft}");
        if !reply_json_rel.is_empty() {
            let label = if ctx.transport.transport == "prompt-only" {
                "Structured reply (prompt-only transport)"
            } else {
                "Structured reply"
            };
            line.push_str(&format!(" {label}: `{reply_json_rel}`."));
        }
        line
    });
    // The status line renders only when a reply was ingested (`if ($parse)` in the plugin) —
    // i.e. a usable, non-raw run; a failed run (a killed turn) has no reply and no line.
    let ingest_ran = !ctx.r.raw && c3_core::health::is_usable_outcome(bridge_outcome);
    let structured_status = if ingest_ran && structured.is_none() {
        let mut line = format!(
            "Structured reply: INVALID ({validation_error}) - raw text kept; no findings recorded."
        );
        if !reply_json_rel.is_empty() {
            line.push_str(&format!(" Raw last message: `{reply_json_rel}`."));
        }
        Some(line)
    } else {
        None
    };

    // Engine-aware header parts: the title label, the author, the argv and how the prompt was
    // delivered.
    let engine_label = spec
        .as_ref()
        .map(|s| s.label)
        .unwrap_or("Codex")
        .to_string();
    let author = if ctx.is_codex() {
        Author::Codex {
            model: model_label(&ctx.identity),
            effort: effort_sent.clone(),
            cli_version: ctx.codex_version.replace("codex-cli ", ""),
        }
    } else {
        // agy's effort is the model tier; muse's is the sent effort value.
        let effort_desc = ctx
            .effort
            .sent
            .clone()
            .unwrap_or_else(|| "tier in the model id".to_string());
        Author::Engine {
            label: engine_label.clone(),
            model: model_label(&ctx.identity),
            effort_desc,
            harness: ctx.harness.clone(),
        }
    };
    let argv_line = if ctx.is_codex() {
        format!("codex {}", ctx.argv_display.trim_start_matches("codex "))
    } else {
        ctx.argv_display.clone()
    };
    let prompt_via = engine_prompt_via(&ctx.engine).to_string();
    // Further-turn event streams (denial retry / continuation / repair) for an engine run.
    let mut further_turns: Vec<String> = Vec::new();
    if let Some(dr) = &sec.denial_retry {
        if let Some(e) = &dr.events {
            further_turns.push(e.clone());
        }
    }
    further_turns.extend(sec.continue_events_rel.iter().cloned());
    if let Some(e) = &sec.repair_events_rel {
        further_turns.push(e.clone());
    }
    let header = HandoffHeader {
        nn: ctx.nn,
        engine_label,
        slug: ctx.reply_name.clone(),
        date: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
        author,
        effort_sent: effort_sent.clone(),
        effort_requested: ctx.effort.requested.clone(),
        effort_mapping: ctx.effort.mapping.clone(),
        effort_basis: ctx.effort.basis.clone(),
        consult_id: ctx.consult_id.clone(),
        mode: ctx.effective_mode.clone(),
        sandbox: sandbox_label(&ctx.o),
        purpose: ctx.r.purpose_label.clone(),
        argv: argv_line,
        prompt_via,
        bridge_outcome: bridge_outcome.to_string(),
        wall_seconds: fmt_wall(wall),
        tokens,
        events_rel: events_rel.clone(),
        further_turns,
        reviewer_line: reviewer_line(&ctx.identity, &ctx.engine, &ctx.harness),
        preflight_line: if ctx.preflight.is_empty() {
            "Preflight: not recorded (non-openai credential check deferred to M2c+).".into()
        } else {
            format!("Preflight: {}.", ctx.preflight)
        },
        roster_line: if ctx.roster_line.is_empty() {
            None
        } else {
            Some(format!("{}.", ctx.roster_line))
        },
        parent_result_line: {
            let parent_line = if !ctx.parent_thread.is_empty() {
                format!("Parent thread: `{}`.", ctx.parent_thread)
            } else if !ctx.parent_note.is_empty() {
                format!("Parent thread: (none - new thread; {}).", ctx.parent_note)
            } else {
                "Parent thread: (none - new thread).".to_string()
            };
            let result_thread = if thread.is_empty() {
                "(unknown)".to_string()
            } else {
                format!("`{thread}`")
            };
            let source = if thread.is_empty() && !thread_candidate.is_empty() {
                format!(
                    "unknown; unverified rollout candidate `{thread_candidate}` did not contain this run's consultation id - not used as a thread or a parent"
                )
            } else {
                thread_source.to_string()
            };
            format!("{parent_line} Result thread: {result_thread} (source: {source}).")
        },
        brief_reviewed_line: brief_reviewed_line(ctx),
        timeout_line: {
            let mut t = format!(
                "Timeout: {} s ({}); continuation after a timeout kill: {}.",
                ctx.r.timeout_sec,
                if ctx.r.timeout_source == "purpose" {
                    format!("the default of purpose {}", ctx.r.purpose_label)
                } else {
                    "-TimeoutSec".to_string()
                },
                if ctx.r.continue_sec > 0 {
                    format!("up to {} s", ctx.r.continue_sec)
                } else {
                    "off (-ContinueSec 0)".to_string()
                }
            );
            if let Some(rr) = &ctx.range_record {
                t.push_str(&format!(
                    " Range: `{}` - {} ({} insertions, {} deletions).",
                    rr.spec, ctx.range_text, rr.insertions, rr.deletions
                ));
            }
            t
        },
        verdict_line: verdict_line.or(structured_status),
        records,
    };

    let header_str = header.render();
    // The verbatim reply, then the structured section for a structured reply. A run with no
    // captured reply (a killed turn, no usable continuation) prints the placeholder body and,
    // when it exists, the engine error and stderr tail (`codex-consult.ps1:4382`).
    let body = if raw_text.trim().is_empty() {
        let mut b = if sec.partial_needed {
            format!(
                "_(no reply captured - what the killed turn(s) produced is salvaged in `{}`)_",
                sec.partial_rel
            )
        } else {
            "_(no reply captured)_".to_string()
        };
        if !main_event_error.is_empty() {
            b.push_str(&format!("\n\nCodex reported: {main_event_error}"));
        }
        if !main_stderr.trim().is_empty() {
            b.push_str(&format!("\n\n```\n{}\n```", main_stderr.trim()));
        }
        b
    } else {
        structured
            .map(|s| {
                if s.reply_markdown.trim().is_empty() {
                    "_(empty reply_markdown)_".to_string()
                } else {
                    s.reply_markdown.clone()
                }
            })
            // The reply body is the trimmed reply (`$rawReply = ...Trim()`); the byte-for-byte
            // copy lives in `.reply.json`.
            .unwrap_or_else(|| raw_text.trim().to_string())
    };
    // The header ends `...---\n`; the plugin puts a blank line before the verbatim reply.
    let mut out = header_str.clone();
    out.push('\n');
    out.push_str(&body.replace("\r\n", "\n"));
    out.push('\n');
    if let Some(s) = structured {
        out.push_str("\n---\n\n");
        out.push_str(&section::format_structured_section(
            s,
            finding_ids,
            prior_blockers,
            verdict_warning,
        ));
        out.push('\n');
    }
    // (`codex-consult.ps1:4399`) a repaired reply keeps the original prose below the section.
    if sec.repaired_ok {
        out.push_str("\n---\n\n## Original reply (prose, before format repair)\n\n");
        out.push_str(&sec.original_prose.trim().replace("\r\n", "\n"));
        out.push('\n');
    }

    // The salvaged partial file (`codex-consult.ps1:4402`): the same metadata block (a new
    // title, no `Verbatim reply follows.`), then the turns and the footer.
    let partial_md = if sec.partial_needed {
        let mut lines: Vec<String> = Vec::new();
        lines.push(format!(
            "# Handoff {:02} - {}: {} - partial reply (a turn was killed on its timeout)",
            ctx.nn,
            spec.as_ref().map(|s| s.label).unwrap_or("Codex"),
            ctx.reply_name
        ));
        for l in header_str.lines().skip(1) {
            if l == "Verbatim reply follows." {
                break;
            }
            lines.push(l.to_string());
        }
        lines.push("What the reviewer produced before the kill follows (every agent message and reasoning text of each turn's event stream, in order, then its tool calls).".to_string());
        let p_header = lines.join("\n");
        Some(format!(
            "{}\n\n---\n\n{}\n\n---\n\n{}\n",
            p_header.trim_end(),
            sec.partial_body.trim_end(),
            sec.partial_footer
        ))
    } else {
        None
    };
    (out, partial_md)
}

/// `Format-Usage` for a tokens clause: the reported counts, or `unknown` when null.
fn usage_clause(u: &Option<Usage>) -> String {
    match u {
        Some(u) => format!(
            "in {} (cached {}), out {}, reasoning {}",
            u.input_tokens, u.cached_input_tokens, u.output_tokens, u.reasoning_output_tokens
        ),
        None => "unknown".to_string(),
    }
}

/// `Brief: ... Reviewed: ...` (`$briefLine $reviewedLine`).
fn brief_reviewed_line(ctx: &Context) -> String {
    let brief_line = if ctx.brief_ref.is_empty() {
        "Brief: (none, prompt only).".to_string()
    } else {
        format!(
            "Brief: `{}` (sha256 {}).",
            ctx.brief_ref,
            short_hash(&ctx.brief_sha)
        )
    };
    let tree = if ctx.revision.tree_sha256.is_empty() {
        "(none)".to_string()
    } else {
        short_hash(&ctx.revision.tree_sha256)
    };
    let reviewed_line = format!(
        "Reviewed: {}, base {}, tree sha256 {}, {} changed files.",
        ctx.revision.reviewed_revision, ctx.revision.base_commit, tree, ctx.revision.changed_files
    );
    format!("{brief_line} {reviewed_line}")
}
