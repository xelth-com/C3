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
    // Telemetry status (C3's addition per README; the plugin has no such line). The plugin's
    // dry-run block would place it after the `peak` line, but peak is not implemented yet, so
    // it goes last in the label block to avoid disturbing the existing console-parity lines.
    {
        let cfg = crate::telemetry::Config {
            telemetry: ctx.o.telemetry,
        };
        let st = crate::telemetry::status(&cfg);
        let body = st.strip_prefix("telemetry: ").unwrap_or(&st);
        out.push(format!("telemetry   : {body}"));
    }
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
            let stdin_len = crate::engines::agy::convert_to_agy_stdin(&ctx.prompt_text)
                .chars()
                .count();
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
    json!({
        "n": ctx.consult_n,
        "when": chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%:z").to_string(),
        "purpose": ctx.o.purpose,
        "consult_id": ctx.consult_id,
        "reviewer": serde_json::to_value(super::orchestrate::build_reviewer(&ctx.identity, &ctx.harness)).unwrap_or(Value::Null),
        "lineage": ctx.identity.lineage,
        "preflight": ctx.preflight,
        "preflight_warning": ctx.preflight_warning,
        "roster": ctx.roster_record.as_ref().map(|r| serde_json::to_value(r).unwrap_or(Value::Null)).unwrap_or(Value::Null),
        "panel": super::orchestrate::build_member_panel(ctx).map(|p| serde_json::to_value(p).unwrap_or(Value::Null)).unwrap_or(Value::Null),
        "extra_config": ctx.r.extra_config.clone(),
        "extra_config_source": ctx.extra_config_source,
        "parent_thread": ctx.parent_thread,
        "thread": "<filled from the event stream>",
        "thread_source": "events|rollout (verified by consultation id)|unknown",
        "mode": ctx.effective_mode,
        // (wave 26b, D16) the context-window fork/resume -> new downgrade, else null.
        "mode_fallback": ctx.mode_fallback.as_ref().map(|m| serde_json::to_value(m).unwrap_or(Value::Null)).unwrap_or(Value::Null),
        "command": command_str(ctx),
        "reply": preview_rel(ctx, "md"),
        "reply_json": if ctx.r.raw { Value::Null } else { Value::String(preview_rel(ctx, "reply.json")) },
        "events": preview_rel(ctx, "events.jsonl"),
        "brief": ctx.brief_ref,
        "range": ctx.range_record.as_ref().map(|r| serde_json::to_value(r).unwrap_or(Value::Null)).unwrap_or(Value::Null),
        "prompt_chars": ctx.prompt_text.chars().count(),
        "model": model_label(ctx),
        "effort": effort_sent,
        "effort_requested": ctx.effort.requested,
        "effort_sent": ctx.effort.sent.clone().map(Value::String).unwrap_or(Value::Null),
        "effort_mapping": ctx.effort.mapping,
        "effort_caps": ctx.effort.caps,
        "max_words": ctx.r.max_words,
        "sandbox": ctx.sandbox_record.clone(),
        "timeout_sec": ctx.r.timeout_sec,
        "timeout_source": ctx.r.timeout_source,
        "continue_sec": ctx.r.continue_sec,
        "peak": ctx.peak,
        "peak_schedule": ctx.peak_schedule,
        "peak_source": ctx.peak_source,
        "peak_evaluated_at": ctx.peak_evaluated_at,
        "schema": if ctx.r.raw { "" } else { "consult-reply v1" },
        "schema_transport": ctx.transport.transport,
        "schema_transport_source": ctx.transport.source,
        "structured": if ctx.r.raw { Value::Bool(false) } else { Value::String("<true when the reply validates>".into()) },
        "base_commit": ctx.revision.base_commit,
        "reviewed_revision": ctx.revision.reviewed_revision,
        "tree_sha256": ctx.revision.tree_sha256,
        "tree_sha256_after": "<computed after the run>",
        "tree_changed_during_review": "<true|false: a file's content changed during the run>",
        "revision_moved": "<null, or \"<old base_commit> -> <new base_commit>\" when HEAD moved during the run>",
        "changed_files": ctx.revision.changed_files,
        "fingerprint_note": ctx.revision.fingerprint_note,
        "bridge_outcome": "<usable reply | failed: ...>",
        "warnings": ctx.run_warnings.clone(),
        "denial_retry": Value::Null,
        "usage": preview_usage(ctx),
        "engine_run": preview_engine_run(ctx),
        "wall_seconds": 0,
        "finished_at": "<written at the commit>",
        "commit_wait_ms": "<ms the commit waited for the write lock>"
    })
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
