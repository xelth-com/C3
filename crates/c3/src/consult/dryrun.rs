//! The `--dry-run` block (`codex-consult.ps1:2500-2665`): the console preview and the
//! `sessions.json` entry preview. A dry run writes nothing.
//!
//! M2c renders the console block (the `repo root`/`task dir`/.../`prompt`/`argv`/`command`
//! lines) faithfully and prints a `sessions.json entry preview` built from the resolved
//! context with the plugin's placeholder strings for the fields only a real run fills.

use serde_json::{json, Value};

use c3_core::ps_json;

use super::orchestrate::{fmt_wall, Context};

/// Print the whole dry-run block for `ctx` (writes nothing).
pub(crate) fn render(ctx: &Context) {
    println!("DRY RUN - nothing was executed and no file was written.");
    println!();
    for line in console_lines(ctx) {
        println!("{line}");
    }
}

/// The console lines of the dry-run block (without the leading DRY RUN banner).
pub(crate) fn console_lines(ctx: &Context) -> Vec<String> {
    let task_dir = ctx.collab_root.join(ctx.task.as_str());
    let sessions = task_dir.join("sessions.json");
    let findings = task_dir.join("findings.json");
    let lock = task_dir.join(".consult.lock");
    let effort_shown = ctx.effort.sent.clone().unwrap_or_else(|| "nothing".into());
    let mut out: Vec<String> = Vec::new();

    out.push(format!("repo root   : {}", ctx.repo_root.display()));
    out.push(format!("task dir    : {}", task_dir.display()));
    out.push(format!("sessions    : {}", sessions.display()));
    if !ctx.r.raw {
        out.push(format!(
            "findings    : {} ({} open finding(s) listed in the prompt)",
            findings.display(),
            ctx.open_findings_count
        ));
    }
    out.push(format!(
        "lock        : {} (held open for the run; not in a dry run)",
        lock.display()
    ));
    for line in &ctx.recovery_dry_lines {
        out.push(format!("pending     : {line}"));
    }
    let is_codex = ctx.engine == "codex";
    let spec = c3_core::lineage::engine_spec(&ctx.engine);
    if is_codex {
        if ctx.launcher.is_empty() {
            out.push("launcher    : (codex not found on PATH)".into());
        } else {
            out.push(format!("launcher    : {}", ctx.launcher));
        }
        out.push(format!("codex       : {}", ctx.codex_version));
        let config_path = crate::providers::get_codex_config_path();
        out.push(format!(
            "config      : {}",
            if config_path.is_empty() {
                "(no Codex home)".to_string()
            } else {
                config_path
            }
        ));
    } else {
        let label = spec.as_ref().map(|s| s.label).unwrap_or("");
        let exe_env = spec.as_ref().map(|s| s.exe_env).unwrap_or("");
        out.push(format!(
            "engine      : {} - {label} (from {})",
            ctx.engine, ctx.engine_from
        ));
        if ctx.engine_launcher.is_empty() {
            out.push(format!(
                "launcher    : ({} CLI not found on PATH; -EngineExe or {exe_env})",
                ctx.engine
            ));
        } else {
            out.push(format!("launcher    : {}", ctx.engine_launcher));
        }
        out.push(format!("harness     : {}", ctx.harness));
        out.push(format!(
            "config      : (not used by the {} engine)",
            ctx.engine
        ));
    }
    // The full reviewer line, minus the `Reviewer: ` prefix (`$reviewerLine -replace ...`).
    let reviewer_full = super::orchestrate::reviewer_line(&ctx.identity, &ctx.engine, &ctx.harness);
    out.push(format!(
        "reviewer    : {}",
        reviewer_full
            .strip_prefix("Reviewer: ")
            .unwrap_or(&reviewer_full)
    ));
    out.push(format!(
        "lineage     : {}",
        c3_core::lineage::format_reviewer_lineage(
            &ctx.identity.provider,
            &ctx.identity.model,
            &ctx.engine
        )
    ));
    let preflight_line = if !ctx.preflight_label.is_empty() {
        ctx.preflight_label.clone()
    } else if ctx.o.skip_preflight {
        "skipped (-SkipPreflight)".to_string()
    } else if let Some((refusal, _)) = &ctx.preflight_refusal {
        format!("a real run is refused - {refusal}")
    } else if ctx.preflight.is_empty() {
        "(non-openai credential check deferred to M2c+)".to_string()
    } else {
        format!("available ({})", ctx.preflight)
    };
    out.push(format!("preflight   : {preflight_line}"));
    if !ctx.roster_line.is_empty() {
        out.push(ctx.roster_line.clone());
    }
    for w in &ctx.run_warnings {
        out.push(format!("WARNING: {w}"));
    }
    if !ctx.preflight_warning.is_empty() {
        out.push(format!("WARNING: {}", ctx.preflight_warning));
    }
    out.push(format!("model       : {}", model_label(ctx)));
    out.push(format!(
        "purpose     : {} (effort {}, max words {})",
        ctx.r.purpose_label,
        if ctx.effort.sent.is_none() {
            "none sent".to_string()
        } else {
            effort_shown.clone()
        },
        ctx.r.max_words
    ));
    // (wave 26, R14-R16, D7) the companions: the topics, the role block, the required reviewers
    if !ctx.o.topic.is_empty() {
        out.push(format!("topics      : {}", ctx.o.topic.join(", ")));
    }
    if let Some(ri) = &ctx.role_info {
        out.push(format!(
            "role        : {} ({}: {}) - in the prompt after the ask",
            ri.name, ri.source, ri.path
        ));
    }
    if !ctx.single_required.is_empty() {
        out.push(format!(
            "required    : {} available (-Require)",
            ctx.single_required
                .iter()
                .map(|p| format!("#{p}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    out.push(format!(
        "effort      : {} sent (requested {}, mapping {}, by {})",
        effort_shown, ctx.effort.requested, ctx.effort.mapping, ctx.effort.basis
    ));
    out.push(format!(
        "timeout     : {} s ({}); continuation after a timeout kill: {}",
        ctx.r.timeout_sec,
        match ctx.r.timeout_source.as_str() {
            "purpose" => format!(
                "the default of purpose {}; -TimeoutSec overrides",
                ctx.r.purpose_label
            ),
            "roster" => "the roster entry's timeout_sec".to_string(),
            _ => "-TimeoutSec".to_string(),
        },
        if ctx.r.continue_sec > 0 {
            format!(
                "one turn of up to {} s on the same thread (-ContinueSec; 0 = off)",
                ctx.r.continue_sec
            )
        } else {
            "off (-ContinueSec 0)".to_string()
        }
    ));
    if let Some(rr) = &ctx.range_record {
        out.push(format!(
            "range       : {} - {} ({} insertions, {} deletions)",
            rr.spec, ctx.range_text, rr.insertions, rr.deletions
        ));
    }
    out.push(format!("peak        : {}", ctx.peak_label));
    if !ctx.peak_warning.is_empty() {
        out.push(format!("WARNING: {}", ctx.peak_warning));
    }
    if ctx.r.raw {
        out.push(format!(
            "reply format: raw text ({}: no schema, no findings bookkeeping)",
            if ctx.o.purpose == "chore" {
                "-Purpose chore"
            } else {
                "-Raw"
            }
        ));
    } else {
        let schema = ctx
            .schema_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        out.push(format!("schema      : {schema}"));
        let schema_flag = spec
            .as_ref()
            .map(|s| s.schema_flag)
            .unwrap_or("--output-schema");
        match ctx.transport.transport.as_str() {
            "output-schema" => out.push(format!(
                "transport   : output-schema ({}): passed as --output-schema",
                ctx.transport.basis
            )),
            "native" => out.push(format!(
                "transport   : native ({}): passed as {schema_flag}, the reply is {} (validated locally too)",
                ctx.transport.basis,
                spec.as_ref().map(|s| s.reply_source).unwrap_or("")
            )),
            _ if is_codex => out.push(format!(
                "transport   : prompt-only ({}): --output-schema is NOT passed; the schema travels in the prompt, the reply is validated locally",
                ctx.transport.basis
            )),
            _ => out.push(format!(
                "transport   : prompt-only ({}): {schema_flag} is NOT passed; the schema travels in the prompt, the reply is validated locally",
                ctx.transport.basis
            )),
        }
    }
    if !ctx.r.raw && ctx.o.format_retry == 1 {
        out.push("format retry : 1 attempt if the reply is not valid JSON".into());
    } else {
        out.push("format retry : 0 (off)".into());
    }
    // The engine-only lines (`codex-consult.ps1:3565-3569`): denial retry, the model-step cap,
    // and the sandbox record (an engine's read-only enforcement).
    if !is_codex {
        if let Some(s) = &spec {
            if !s.denial_retry {
                out.push(format!(
                    "denial retry: n/a (the {} engine runs with its write, shell and web tools disabled - nothing is auto-denied)",
                    ctx.engine
                ));
            } else if ctx.o.denial_retry == 1 {
                out.push("denial retry: 1 attempt if a tool was auto-denied and the turn produced nothing".into());
            } else {
                out.push("denial retry: 0 (off)".into());
            }
            if !s.steps_flag.is_empty() {
                if ctx.o.max_model_steps > 0 {
                    out.push(format!(
                        "max steps   : {} ({})",
                        ctx.o.max_model_steps, s.steps_flag
                    ));
                } else {
                    out.push(format!(
                        "max steps   : the {} CLI's default (no {})",
                        ctx.engine, s.steps_flag
                    ));
                }
            }
        }
        out.push(format!("sandbox     : {}", ctx.sandbox_record));
        // The cmd.exe `%`-argument hazard (F02-14): a real run would be refused before launch.
        let hazard = super::orchestrate::cmd_argv_hazard(&ctx.engine_launcher, &ctx.argv);
        if !hazard.is_empty() {
            out.push(format!(
                "launch      : a real run is refused before launch - {hazard}"
            ));
        }
        // (0.6.0, wave 29, D9) the prompt is larger than the engine's stdin bound
        if !ctx.prompt_bound.is_empty() {
            out.push(format!(
                "launch      : a real run is refused before launch - {}",
                ctx.prompt_bound
            ));
        }
    }
    out.push(format!("mode        : {}", ctx.effective_mode));
    // The parent-thread note (`Select-ParentThread`'s note): why this run is a new thread, or
    // which thread it forks/resumes. Printed only when there is a note (`if ($parentNote)`).
    if !ctx.parent_note.is_empty() {
        out.push(format!("parent      : {}", ctx.parent_note));
    }
    if ctx.effective_mode == "new" {
        out.push("thread      : (a new thread will be created)".into());
    } else if !is_codex && ctx.effective_mode == "resume" {
        // An engine resume shows how it continues (`--session-id`/`--conversation`).
        let noun = spec.as_ref().map(|s| s.thread_noun).unwrap_or("thread");
        let flag = spec.as_ref().map(|s| s.thread_flag).unwrap_or("");
        out.push(format!(
            "thread      : {} ({noun} resumed with {flag})",
            ctx.parent_thread
        ));
    } else {
        out.push(format!(
            "thread      : {} (parent for {})",
            ctx.parent_thread, ctx.effective_mode
        ));
    }
    out.push(format!(
        "consult id  : {} (the prompt's last line)",
        ctx.consult_id
    ));
    out.push(format!(
        "handoff     : {:02} (consult n = {})",
        ctx.nn, ctx.consult_n
    ));
    out.push(format!(
        "reviewed    : {}, base {}, {} changed files",
        ctx.revision.reviewed_revision, ctx.revision.base_commit, ctx.revision.changed_files
    ));
    let tree = if ctx.revision.tree_sha256.is_empty() {
        "(none)".to_string()
    } else {
        ctx.revision.tree_sha256.clone()
    };
    out.push(format!(
        "tree sha256 : {} ({})",
        tree, ctx.revision.fingerprint_note
    ));
    if let Some(bp) = &ctx.brief_path {
        if let Ok(bytes) = std::fs::read(bp) {
            out.push(format!("brief sha256: {}", c3_core::sha256_hex(&bytes)));
        }
    }
    // Telemetry: the plugin's dry-run line (`telemetry   : on|off (<source>) - ...`, the switch as
    // `Get-TelemetrySwitch` resolves it: the run's --telemetry wins over the variable). It goes
    // last in the label block to avoid disturbing the existing console-parity lines.
    {
        let sw = crate::telemetry::switch(ctx.o.telemetry);
        if sw.on {
            out.push(format!("telemetry   : on ({}) - after the commit ONE anonymised event of this consultation goes to the spool and a background sender delivers it (README \"Telemetry\"; CODEX_CONSULT_TELEMETRY=off or --telemetry off switches it off)", sw.source));
        } else {
            out.push(format!(
                "telemetry   : off ({}) - nothing is spooled or sent",
                sw.source
            ));
        }
    }
    // (wave 27) the coordinator, the scrubbed child environment and the brief prefix.
    // (wave 27c, D11/D12) a coordinator that parses but names no seat is said on the line, not
    // refused: `not in the roster` for a value no reviewer can match, `names no roster position
    // here` for a `#n` with no seat.
    let coordinator_note = if let Some(pos) = &ctx.coordinator.unresolved {
        format!(" ({pos} names no roster position here)")
    } else if ctx.coordinator.in_roster == Some(false) {
        " (not in the roster - no reviewer can match it)".to_string()
    } else {
        String::new()
    };
    out.push(format!(
        "coordinator : {}{coordinator_note}",
        c3_core::host::format_coordinator_text(&ctx.coordinator)
    ));
    if ctx.engine == "claude" {
        // (0.6.0, wave 29, D2) the claude child's ALLOW-listed environment; (wave 29b, E4) auth
        // endpoint: the route's base URL, the token variable's NAME (never its value), the plan
        let ce =
            crate::engines::claude_auth::child_env(&ctx.engine_auth, ctx.engine_endpoint.as_ref());
        if ctx.engine_auth == "endpoint" {
            out.push(format!(
                "child env   : an allow list (auth endpoint): {} - every other variable (the host markers, ANTHROPIC_* but ANTHROPIC_BASE_URL and ANTHROPIC_AUTH_TOKEN, CLAUDE_* but CLAUDE_CONFIG_DIR) is left out{}",
                ce.names.join(", "),
                if ce.problem.is_empty() {
                    String::new()
                } else {
                    format!("; a real run is refused: {}", ce.problem)
                }
            ));
            if let Some(ep) = &ctx.engine_endpoint {
                out.push(format!(
                    "endpoint    : {} (ANTHROPIC_BASE_URL); token from env {} (ANTHROPIC_AUTH_TOKEN - the value is never shown); API_TIMEOUT_MS {}; plan {}; no claude auth status - the model the init event names is the proof",
                    ep.base_url,
                    ep.env_key,
                    ep.timeout_ms,
                    if ep.plan.is_empty() { "(none)" } else { ep.plan.as_str() }
                ));
            }
        } else {
            out.push(format!(
                "child env   : an allow list (auth {}): {} - every other variable (the host markers, ANTHROPIC_*, CLAUDE_* but CLAUDE_CONFIG_DIR) is left out",
                ctx.engine_auth,
                ce.names.join(", ")
            ));
        }
    } else {
        out.push(format!(
            "child env   : {}",
            if ctx.child_env_scrubbed.is_empty() {
                "no host marker set - the environment is passed as it is".to_string()
            } else {
                format!(
                    "without the host markers {} (every other variable is kept)",
                    ctx.child_env_scrubbed.join(", ")
                )
            }
        ));
    }
    // (wave 27, R13 D6) the coordinator's brief prefix and this reply's name
    out.push(format!(
        "brief prefix: {} ({}) - the coordinator's briefs are handoffs/<NN>-{}-<slug>.md, this reply {:02}-{}-{}.*",
        ctx.r.brief_prefix,
        ctx.r.brief_prefix_source,
        ctx.r.brief_prefix,
        ctx.nn,
        c3_core::lineage::engine_spec(&ctx.engine)
            .map(|s| s.prefix)
            .unwrap_or("codex"),
        ctx.reply_name
    ));
    out.push(String::new());
    out.push("argv        :".into());
    // The argv block lists the launcher's arguments only (starting with `exec`); the launcher
    // itself is shown on the `command` line, matching the plugin's `foreach ($a in $argv)`.
    for a in &ctx.argv {
        out.push(format!("    {a}"));
    }
    out.push(String::new());
    out.push(format!("command     : {}", command_str(ctx)));
    let prompt_chars = ctx.prompt_text.chars().count();
    if let Some(pf) = &ctx.prompt_file {
        // muse reads its prompt through --prompt-file; stdin stays empty.
        out.push(format!(
            "prompt file : {} (the prompt below, UTF-8 without BOM, written at launch; stdin is empty)",
            pf.display()
        ));
        out.push(format!("prompt (--prompt-file, {prompt_chars} chars):"));
    } else {
        if !is_codex {
            // (the plugin prints this line for every stdin engine; claude's stdin is the prompt)
            let stdin_len = if ctx.engine == "claude" {
                ctx.prompt_text.encode_utf16().count()
            } else {
                crate::engines::agy::convert_to_agy_stdin(&ctx.prompt_text)
                    .chars()
                    .count()
            };
            out.push(format!(
                "stdin       : one NDJSON line {{\"event\":\"user\",\"message\":{{\"content\":<the prompt>}}}} ({stdin_len} chars, UTF-8, LF)"
            ));
        }
        out.push(format!("prompt (stdin, {prompt_chars} chars):"));
    }
    out.push("----".into());
    out.push(ctx.prompt_text.clone());
    out.push("----".into());
    out.push(String::new());
    out.push(format!("reply file  : {}", ctx.reply_path.display()));
    if !ctx.r.raw {
        out.push(format!("reply json  : {}", ctx.reply_json_path.display()));
    }
    out.push(format!("events file : {}", ctx.events_path.display()));
    if is_codex {
        out.push(format!(
            "last message: {} (temp)",
            ctx.last_msg_path.display()
        ));
    } else {
        out.push(format!(
            "reply source: {}, extracted to the reply json before validation",
            spec.as_ref().map(|s| s.reply_source).unwrap_or("")
        ));
    }
    out.push(String::new());
    out.push("sessions.json entry preview:".into());
    out.push(ps_json::format_value_root(&preview(ctx)));
    out
}

fn preview(ctx: &Context) -> Value {
    let effort_sent = ctx
        .effort
        .sent
        .clone()
        .map(Value::String)
        .unwrap_or(Value::Null);
    let to_v = |r: Option<Value>| r.unwrap_or(Value::Null);
    // The plugin's preview literal, key for key in its order (`codex-consult.ps1`, the `-DryRun`
    // `$preview`); keys C3 does not model yet are left out rather than invented.
    let mut m = serde_json::Map::new();
    let mut put = |k: &str, v: Value| {
        m.insert(k.to_string(), v);
    };
    put("n", json!(ctx.consult_n));
    put(
        "when",
        json!(chrono::Local::now()
            .format("%Y-%m-%dT%H:%M:%S%:z")
            .to_string()),
    );
    put("purpose", json!(ctx.o.purpose));
    put(
        "topics",
        Value::Array(ctx.o.topic.iter().map(|t| json!(t)).collect()),
    );
    put("role", json!(ctx.role));
    put("consult_id", json!(ctx.consult_id));
    // (0.6.1, U5) right after consult_id.
    put("consult_ref", json!(ctx.consult_ref));
    put(
        "reviewer",
        serde_json::to_value(super::orchestrate::build_reviewer(
            &ctx.identity,
            &ctx.harness,
        ))
        .unwrap_or(Value::Null),
    );
    put("lineage", json!(ctx.identity.lineage));
    put(
        "coordinator",
        serde_json::to_value(&ctx.coordinator).unwrap_or(Value::Null),
    );
    put("preflight", json!(ctx.preflight));
    put("preflight_warning", json!(ctx.preflight_warning));
    put(
        "roster",
        to_v(
            ctx.roster_record
                .as_ref()
                .map(|r| serde_json::to_value(r).unwrap_or(Value::Null)),
        ),
    );
    put(
        "panel",
        to_v(
            super::orchestrate::build_member_panel(ctx)
                .map(|p| serde_json::to_value(p).unwrap_or(Value::Null)),
        ),
    );
    put("parent_thread", json!(ctx.parent_thread));
    put("thread", json!("<filled from the event stream>"));
    put(
        "thread_source",
        json!("events|rollout (verified by consultation id)|unknown"),
    );
    put("mode", json!(ctx.effective_mode));
    // (wave 26b, D16) the context-window fork/resume -> new downgrade, else null.
    put(
        "mode_fallback",
        to_v(
            ctx.mode_fallback
                .as_ref()
                .map(|m| serde_json::to_value(m).unwrap_or(Value::Null)),
        ),
    );
    put("command", json!(command_str(ctx)));
    // (wave 27) child_env_scrubbed right after command.
    put(
        "child_env_scrubbed",
        Value::Array(ctx.child_env_scrubbed.iter().map(|s| json!(s)).collect()),
    );
    put("brief", json!(ctx.brief_ref));
    put(
        "range",
        to_v(
            ctx.range_record
                .as_ref()
                .map(|r| serde_json::to_value(r).unwrap_or(Value::Null)),
        ),
    );
    put("prompt_chars", json!(ctx.prompt_text.chars().count()));
    put("reply", json!(preview_rel(ctx, "md")));
    put(
        "reply_json",
        if ctx.r.raw {
            Value::Null
        } else {
            Value::String(preview_rel(ctx, "reply.json"))
        },
    );
    put("events", json!(preview_rel(ctx, "events.jsonl")));
    put("model", json!(model_label(ctx)));
    put("effort", effort_sent.clone());
    put("effort_requested", json!(ctx.effort.requested));
    put("effort_sent", effort_sent);
    put("effort_mapping", json!(ctx.effort.mapping));
    put("effort_caps", json!(ctx.effort.caps));
    put("max_words", json!(ctx.r.max_words));
    put("sandbox", json!(ctx.sandbox_record));
    put("timeout_sec", json!(ctx.r.timeout_sec));
    put("timeout_source", json!(ctx.r.timeout_source));
    put("continue_sec", json!(ctx.r.continue_sec));
    put("extra_config", json!(ctx.r.extra_config.clone()));
    put("extra_config_source", json!(ctx.extra_config_source));
    // (wave 28b, D15) the context window as it would reach the engine (null without one).
    put("context_window", to_v(ctx.context_window.clone()));
    put("peak", json!(ctx.peak));
    put("peak_schedule", json!(ctx.peak_schedule));
    put("peak_source", json!(ctx.peak_source));
    put("peak_evaluated_at", json!(ctx.peak_evaluated_at));
    put(
        "structured",
        if ctx.r.raw {
            Value::Bool(false)
        } else {
            Value::String("<true when the reply validates>".into())
        },
    );
    put(
        "schema",
        json!(if ctx.r.raw { "" } else { "consult-reply v1" }),
    );
    put("schema_transport", json!(ctx.transport.transport));
    put("schema_transport_source", json!(ctx.transport.source));
    // (compat 0.3.0) the format-repair placeholder (null when the repair is off).
    put(
        "format_retry",
        if ctx.r.repair_enabled {
            Value::String("<null, or {attempted, reason, succeeded, thread, wall_seconds, usage, drift, original, events, schema_transport} after a format-repair turn>".into())
        } else {
            Value::Null
        },
    );
    put("denial_retry", Value::Null);
    put("base_commit", json!(ctx.revision.base_commit));
    put("reviewed_revision", json!(ctx.revision.reviewed_revision));
    put("tree_sha256", json!(ctx.revision.tree_sha256));
    put("tree_sha256_after", json!("<computed after the run>"));
    put(
        "tree_changed_during_review",
        json!("<true|false: a file's content changed during the run>"),
    );
    put(
        "revision_moved",
        json!(
            "<null, or \"<old base_commit> -> <new base_commit>\" when HEAD moved during the run>"
        ),
    );
    put("changed_files", json!(ctx.revision.changed_files));
    put("fingerprint_note", json!(ctx.revision.fingerprint_note));
    put("bridge_outcome", json!("<usable reply | failed: ...>"));
    put("warnings", json!(ctx.run_warnings.clone()));
    put("usage", preview_usage(ctx));
    // (wave 28c, D11) right after usage.
    put(
        "compactions",
        json!(if ctx.context_tokens > 0 {
            "<n (the compactions the engine's stream reported), else 'unknown'>"
        } else {
            "<null, or n when the engine's stream reported a compaction>"
        }),
    );
    put("engine_run", preview_engine_run(ctx));
    put("wall_seconds", json!(0));
    put("finished_at", json!("<written at the commit>"));
    put(
        "commit_wait_ms",
        json!("<ms the commit waited for the write lock>"),
    );
    Value::Object(m)
}

/// The engine's command string for the dry-run `command :` line and the preview `command` field
/// (the launcher plus its argv; byte-identical to the plugin's `$commandStr`).
fn command_str(ctx: &Context) -> String {
    if ctx.engine == "codex" {
        format!("codex {}", ctx.argv_display.trim_start_matches("codex "))
    } else {
        ctx.argv_display.clone()
    }
}

/// `handoffs/NN-<prefix>-<reply>.<ext>` for the preview's `reply`/`reply_json`/`events` fields.
fn preview_rel(ctx: &Context, ext: &str) -> String {
    let prefix = c3_core::lineage::engine_spec(&ctx.engine)
        .map(|s| s.prefix)
        .unwrap_or("codex");
    format!(
        "handoffs/{:02}-{}-{}.{}",
        ctx.nn, prefix, ctx.reply_name, ext
    )
}

/// The preview `usage` placeholder: codex/agy report token usage, muse does not (`null`).
fn preview_usage(ctx: &Context) -> Value {
    if ctx.engine == "codex" {
        return json!({
            "input_tokens": "<n>",
            "cached_input_tokens": "<n>",
            "output_tokens": "<n>",
            "reasoning_output_tokens": "<n>"
        });
    }
    match c3_core::lineage::engine_spec(&ctx.engine) {
        Some(s) if s.has_usage => json!({
            "input_tokens": "<n>",
            "cached_input_tokens": "<n (cache_read_tokens)>",
            "output_tokens": "<n>",
            "reasoning_output_tokens": "<n (thinking_tokens)>",
            "total_tokens": "<n>"
        }),
        _ => Value::Null,
    }
}

/// The preview `engine_run` placeholder: `null` for codex, else `{turns, max_model_steps,
/// msp_schema_version}` (`max_model_steps` filled when >0; `msp_schema_version` for a
/// prompt-file engine, muse).
fn preview_engine_run(ctx: &Context) -> Value {
    if ctx.engine == "codex" {
        return Value::Null;
    }
    if ctx.engine == "claude" {
        // (0.6.0, wave 29) the claude engine's evidence fields, the placeholders the plugin writes
        let names =
            crate::engines::claude_auth::child_env(&ctx.engine_auth, ctx.engine_endpoint.as_ref())
                .names;
        let endpoint = ctx.engine_auth == "endpoint";
        return json!({
            "turns": "<the turns started: 1, + a denial retry, + a format repair>",
            "max_model_steps": if ctx.o.max_model_steps > 0 { json!(ctx.o.max_model_steps) } else { Value::Null },
            "msp_schema_version": Value::Null,
            "auth": ctx.engine_auth,
            "init_tools": "<the tools the init events listed: Glob, Grep, Read, StructuredOutput>",
            "mcp_servers": "<0>",
            "permission_mode": "<dontAsk>",
            "api_key_source": if ctx.engine_auth == "api-key" { "<ANTHROPIC_API_KEY>" } else if endpoint { "<none (recorded raw; ANTHROPIC_API_KEY fails the turn)>" } else { "<none>" },
            "model_resolved": if endpoint { "<the model id the init event names - it must equal the pinned id>" } else { "<the model id the init event resolved>" },
            "other_models": "<[] or the other models a turn named>",
            "permission_denials": "<n>",
            "denied_tools": "<[] or the tools denied>",
            "rate_limit": "<null, or the most severe rate_limit_event as the CLI wrote it>",
            "quota_mark": "<null, or {class quota, kind, code, message, when, retry_after, hint}: a usable reply whose turn saw a rejecting rate_limit_event - the route is out as after a failed quota turn>",
            "cost_usd": "<the notional total_cost_usd>",
            "child_env_allowed": names,
            "switched_off": c3_core::claude::CLAUDE_SWITCHED_OFF,
        });
    }
    let spec = c3_core::lineage::engine_spec(&ctx.engine);
    let max_steps = if ctx.o.max_model_steps > 0 {
        json!(ctx.o.max_model_steps)
    } else {
        Value::Null
    };
    let msp = if spec.as_ref().map(|s| s.prompt_by_file).unwrap_or(false) {
        Value::String("<the MSP schema_version of the stream: 1>".into())
    } else {
        Value::Null
    };
    json!({
        "turns": "<the turns started: 1, + a denial retry, + a format repair>",
        "max_model_steps": max_steps,
        "msp_schema_version": msp
    })
}

fn model_label(ctx: &Context) -> String {
    if ctx.identity.model_source == "unknown" {
        "unknown".to_string()
    } else {
        ctx.identity.model.clone()
    }
}

// Keep fmt_wall referenced (used by summary/orchestrate); silences an unused import if the
// dry-run ever needs the wall formatting.
#[allow(dead_code)]
fn _wall(w: f64) -> String {
    fmt_wall(w)
}
