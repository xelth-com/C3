//! Stdio MCP server: `c3 mcp`.
//!
//! A newline-delimited JSON-RPC 2.0 server over stdin/stdout (the MCP stdio
//! transport) that exposes C3's *read-and-record* tools to a coordinator model
//! (Claude Code, or any MCP client). It carries no commit or finish tool: no
//! tool dispatched here runs a Git write command (design invariant 3, "C3
//! never commits"). That is a statement about C3's own operations, not a
//! sandbox around what a reviewer process a tool launches might do on its
//! own; reviewers run in their own sandboxes/read-only modes, and the engine
//! tree check reports any working-tree or HEAD change it observes.
//!
//! Every tool runs the *same code path* as the matching CLI subcommand: the
//! server re-invokes its own executable (`std::env::current_exe()`) with the CLI
//! arguments and captures stdout / stderr / the exit code. That guarantees the
//! same implementation and the same file set as a CLI run — not byte-identical
//! artifacts across separate runs, since some fields (timestamps, ids, anchors)
//! are run-dependent by design — and isolates each tool's locks and timeouts in
//! a child process. Console output only leaves through the captured pipe, so
//! this server's stdout carries JSON-RPC frames alone.
//!
//! CRITICAL: stdout is JSON-RPC only. All logging goes to stderr and only when
//! `C3_DEBUG=1`.

use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use c3_core::task_slug::{contained_join, is_slug};

/// The protocol version echoed when a client's `initialize` omits its own.
const DEFAULT_PROTOCOL_VERSION: &str = "2025-06-18";

/// Per-tool timeouts: a consultation may take an hour; everything else is bounded.
const CONSULT_TIMEOUT: Duration = Duration::from_secs(3600);
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// Log to stderr only under `C3_DEBUG=1`.
fn dlog(msg: &str) {
    if std::env::var("C3_DEBUG").as_deref() == Ok("1") {
        eprintln!("[c3 mcp] {msg}");
    }
}

/// Entry point for `c3 mcp`: serve until stdin closes. Returns the process exit code.
pub fn run() -> i32 {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut writer = stdout.lock();
    match serve(stdin.lock(), &mut writer) {
        Ok(()) => 0,
        Err(e) => {
            dlog(&format!("serve error: {e}"));
            1
        }
    }
}

/// The stdio serve loop: one JSON-RPC message per line in, one frame per response
/// out. Parse failures answer with a `-32700` error; notifications (no `id`) get
/// no response. Split out from [`run`] so tests can drive it with in-memory pipes.
pub fn serve<R: BufRead, W: Write>(reader: R, writer: &mut W) -> std::io::Result<()> {
    dlog("stdio MCP server ready");
    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        dlog(&format!("<- {line}"));
        let reply = match serde_json::from_str::<Value>(line) {
            Ok(msg) => handle_message(&msg),
            Err(e) => Some(rpc_error(Value::Null, -32700, &format!("parse error: {e}"))),
        };
        if let Some(v) = reply {
            write_frame(writer, &v)?;
        }
    }
    Ok(())
}

fn write_frame<W: Write>(writer: &mut W, v: &Value) -> std::io::Result<()> {
    let s = v.to_string();
    dlog(&format!("-> {s}"));
    writer.write_all(s.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()
}

/// Route one parsed JSON-RPC message. Returns `None` for notifications (no `id`),
/// which take no response.
pub fn handle_message(msg: &Value) -> Option<Value> {
    let method = msg
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));
    // Notifications carry no `id` and get no reply (this also covers
    // `notifications/initialized`).
    let id = msg.get("id").cloned()?;

    match method {
        "initialize" => {
            let pv = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_PROTOCOL_VERSION);
            Some(rpc_result(
                id,
                json!({
                    "protocolVersion": pv,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "c3", "version": env!("CARGO_PKG_VERSION") },
                }),
            ))
        }
        "ping" => Some(rpc_result(id, json!({}))),
        "tools/list" => Some(rpc_result(id, json!({ "tools": tool_defs() }))),
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let (text, is_error) = match call_tool(name, &args) {
                Ok(t) => (t, false),
                Err(t) => (t, true),
            };
            Some(rpc_result(
                id,
                json!({
                    "content": [{ "type": "text", "text": text }],
                    "isError": is_error,
                }),
            ))
        }
        other => Some(rpc_error(id, -32601, &format!("method not found: {other}"))),
    }
}

fn rpc_result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

// ---------------------------------------------------------------------------
// Argument accessors
// ---------------------------------------------------------------------------

/// A non-empty trimmed string argument, if present.
fn opt_str(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// A required non-empty string argument.
fn req_str(args: &Value, key: &str) -> Result<String, String> {
    opt_str(args, key).ok_or_else(|| format!("missing required string argument '{key}'"))
}

fn opt_bool(args: &Value, key: &str) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn opt_i64(args: &Value, key: &str) -> Option<i64> {
    args.get(key).and_then(Value::as_i64)
}

fn opt_arr(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Reject a path argument that is absolute, contains a `..` component, or —
/// after both the working directory and the argument are resolved
/// (canonicalized, following symlinks/junctions) — lands outside the server's
/// working directory. The MCP server inherits the coordinator's repo as its
/// cwd, and no read-and-record tool has any business outside it; lexical
/// containment alone is not enough because a normal-looking path component can
/// be a symlink or junction that points elsewhere (F03-1).
fn reject_escape(val: &str) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| format!("cannot read cwd: {e}"))?;
    check_contained(&cwd, val)
}

/// The resolved-path half of [`reject_escape`], split out so tests can pass an explicit
/// base directory instead of the process cwd.
fn check_contained(base: &Path, val: &str) -> Result<(), String> {
    let joined =
        contained_join(base, val).map_err(|e| format!("path argument '{val}' is refused: {e}"))?;
    let real_base = canonicalize_best_effort(base).map_err(|e| {
        format!("path argument '{val}' is refused: cannot resolve the working directory: {e}")
    })?;
    let real_target = canonicalize_best_effort(&joined).map_err(|e| {
        format!("path argument '{val}' is refused: cannot resolve the target path: {e}")
    })?;
    if real_target.starts_with(&real_base) {
        Ok(())
    } else {
        Err(format!(
            "path argument '{val}' is refused: resolves outside the working directory"
        ))
    }
}

/// Canonicalize `path`, resolving symlinks and junctions. When `path` (or a trailing part
/// of it) does not exist yet — the common case for an `out` file the tool is about to
/// create — canonicalize the deepest existing ancestor instead and re-append the missing
/// tail, so the check still follows any link earlier in the chain.
fn canonicalize_best_effort(path: &Path) -> std::io::Result<PathBuf> {
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    let mut cur = path.to_path_buf();
    loop {
        match std::fs::canonicalize(&cur) {
            Ok(mut resolved) => {
                for part in tail.into_iter().rev() {
                    resolved.push(part);
                }
                return Ok(resolved);
            }
            Err(e) => {
                let popped = cur.file_name().map(|n| n.to_os_string());
                let had_parent = cur.pop();
                match (popped, had_parent) {
                    (Some(name), true) => tail.push(name),
                    _ => return Err(e),
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tool dispatch: build the CLI argv, validate paths, run the child, format.
// ---------------------------------------------------------------------------

fn call_tool(name: &str, args: &Value) -> Result<String, String> {
    let (argv, timeout) = build_argv(name, args)?;
    let result = run_cli(&argv, timeout)?;
    let text = format_result(&result);
    if is_failure(&result) {
        Err(text)
    } else {
        Ok(text)
    }
}

/// Map a tool name + its JSON arguments to the `c3` CLI argv and a timeout.
/// Every path-shaped argument is checked for containment first.
fn build_argv(name: &str, args: &Value) -> Result<(Vec<String>, Duration), String> {
    let mut v: Vec<String> = Vec::new();

    match name {
        "c3_providers" => {
            v.push("providers".into());
            if let Some(p) = opt_str(args, "provider") {
                v.push("--provider".into());
                v.push(p);
            }
            if opt_bool(args, "short") {
                v.push("--short".into());
            }
            if opt_bool(args, "no_network") {
                v.push("--no-network".into());
            }
            if let Some(c) = opt_str(args, "collab_dir") {
                reject_escape(&c)?;
                v.push("--collab-dir".into());
                v.push(c);
            }
            Ok((v, DEFAULT_TIMEOUT))
        }
        "c3_consult" => {
            v.push("consult".into());
            let task = req_str(args, "task")?;
            let purpose = req_str(args, "purpose")?;
            let brief = req_str(args, "brief")?;
            let prompt = req_str(args, "prompt")?;
            let reply_name = req_str(args, "reply_name")?;
            reject_escape(&brief)?;
            v.push("--task".into());
            v.push(task);
            v.push("--purpose".into());
            v.push(purpose);
            v.push("--brief".into());
            v.push(brief);
            v.push("--prompt".into());
            v.push(prompt);
            v.push("--reply-name".into());
            v.push(reply_name);
            for (key, flag) in [
                ("mode", "--mode"),
                ("thread", "--thread"),
                ("provider", "--provider"),
                ("model", "--model"),
                ("effort", "--effort"),
                ("range", "--range"),
                ("schema_transport", "--schema-transport"),
                ("telemetry", "--telemetry"),
            ] {
                if let Some(val) = opt_str(args, key) {
                    v.push(flag.into());
                    v.push(val);
                }
            }
            for (key, flag) in [
                ("max_words", "--max-words"),
                ("timeout_sec", "--timeout-sec"),
                ("continue_sec", "--continue-sec"),
                ("format_retry", "--format-retry"),
            ] {
                if let Some(n) = opt_i64(args, key) {
                    v.push(flag.into());
                    v.push(n.to_string());
                }
            }
            for a in opt_arr(args, "artifact") {
                reject_escape(&a)?;
                v.push("--artifact".into());
                v.push(a);
            }
            for c in opt_arr(args, "codex_config") {
                v.push("--codex-config".into());
                v.push(c);
            }
            if opt_bool(args, "raw") {
                v.push("--raw".into());
            }
            if opt_bool(args, "dry_run") {
                v.push("--dry-run".into());
            }
            if opt_bool(args, "skip_preflight") {
                v.push("--skip-preflight".into());
            }
            if opt_bool(args, "off_peak_only") {
                v.push("--off-peak-only".into());
            }
            if let Some(c) = opt_str(args, "collab_dir") {
                reject_escape(&c)?;
                v.push("--collab-dir".into());
                v.push(c);
            }
            Ok((v, CONSULT_TIMEOUT))
        }
        "c3_panel" => {
            v.push("consult".into());
            let task = req_str(args, "task")?;
            validate_slug(&task, "task")?;
            let brief = req_str(args, "brief")?;
            reject_escape(&brief)?;
            v.push("--task".into());
            v.push(task);
            v.push("--brief".into());
            v.push(brief);
            v.push("--panel".into());
            if let Some(p) = opt_str(args, "purpose") {
                validate_enum(&p, &crate::consult::prompt::presets::VALID, "purpose")?;
                v.push("--purpose".into());
                v.push(p);
            }
            for a in opt_arr(args, "artifacts") {
                reject_escape(&a)?;
                v.push("--artifact".into());
                v.push(a);
            }
            if let Some(n) = opt_i64(args, "size") {
                validate_range(n, 1, 16, "size")?;
                v.push("--panel-size".into());
                v.push(n.to_string());
            }
            if let Some(o) = opt_str(args, "order") {
                validate_enum(&o, &["routed", "roster"], "order")?;
                v.push("--panel-order".into());
                v.push(o);
            }
            let require = opt_arr(args, "require");
            validate_require(&require)?;
            for r in require {
                v.push("--require".into());
                v.push(r);
            }
            if let Some(r) = opt_str(args, "role") {
                validate_slug(&r, "role")?;
                v.push("--role".into());
                v.push(r);
            }
            for r in opt_arr(args, "roles") {
                validate_slug(&r, "roles")?;
                v.push("--roles".into());
                v.push(r);
            }
            for t in opt_arr(args, "topics") {
                validate_slug(&t, "topics")?;
                v.push("--topic".into());
                v.push(t);
            }
            if let Some(n) = opt_i64(args, "timeout_sec") {
                v.push("--timeout-sec".into());
                v.push(n.to_string());
            }
            let dry_run = opt_bool(args, "dry_run");
            // detach defaults to true: a panel takes minutes and an MCP call must
            // return. --detach and --dry-run are refused together by the CLI, so a
            // dry run never carries --detach even though the default is on.
            let detach = args.get("detach").and_then(Value::as_bool).unwrap_or(true);
            if dry_run {
                v.push("--dry-run".into());
            } else if detach {
                v.push("--detach".into());
            }
            push_collab(&mut v, args)?;
            Ok((v, CONSULT_TIMEOUT))
        }
        "c3_status" => {
            v.push("consult".into());
            let task = req_str(args, "task")?;
            validate_slug(&task, "task")?;
            v.push("--task".into());
            v.push(task);
            let wait = opt_bool(args, "wait");
            if wait {
                v.push("--wait".into());
            } else {
                v.push("--status".into());
            }
            if let Some(id) = opt_str(args, "id") {
                v.push("--id".into());
                v.push(id);
            }
            if wait {
                let wt = opt_i64(args, "wait_timeout_sec").unwrap_or(300);
                validate_range(wt, 1, 3600, "wait_timeout_sec")?;
                v.push("--wait-timeout-sec".into());
                v.push(wt.to_string());
            }
            push_collab(&mut v, args)?;
            let timeout = if wait {
                CONSULT_TIMEOUT
            } else {
                DEFAULT_TIMEOUT
            };
            Ok((v, timeout))
        }
        "c3_router_explain" => {
            v.push("router".into());
            v.push("explain".into());
            let purpose = req_str(args, "purpose")?;
            validate_enum(&purpose, &crate::consult::prompt::presets::VALID, "purpose")?;
            v.push("--purpose".into());
            v.push(purpose);
            for t in opt_arr(args, "topic") {
                validate_slug(&t, "topic")?;
                v.push("--topic".into());
                v.push(t);
            }
            push_collab(&mut v, args)?;
            Ok((v, DEFAULT_TIMEOUT))
        }
        "c3_router_replay" => {
            v.push("router".into());
            v.push("replay".into());
            let task = req_str(args, "task")?;
            validate_slug(&task, "task")?;
            v.push("--task".into());
            v.push(task);
            if let Some(n) = opt_i64(args, "nn") {
                v.push("--nn".into());
                v.push(n.to_string());
            }
            push_collab(&mut v, args)?;
            Ok((v, DEFAULT_TIMEOUT))
        }
        "c3_findings_list" => {
            v.push("findings".into());
            v.push("--task".into());
            v.push(req_str(args, "task")?);
            v.push("--list".into());
            if opt_bool(args, "all") {
                v.push("--all".into());
            }
            push_collab(&mut v, args)?;
            Ok((v, DEFAULT_TIMEOUT))
        }
        "c3_findings_stats" => {
            v.push("findings".into());
            v.push("--task".into());
            v.push(req_str(args, "task")?);
            v.push("--stats".into());
            push_collab(&mut v, args)?;
            Ok((v, DEFAULT_TIMEOUT))
        }
        "c3_findings_status" => {
            v.push("findings".into());
            v.push("--task".into());
            v.push(req_str(args, "task")?);
            v.push("--id".into());
            v.push(req_str(args, "id")?);
            v.push("--status".into());
            v.push(req_str(args, "status")?);
            if let Some(n) = opt_str(args, "note") {
                v.push("--note".into());
                v.push(n);
            }
            if let Some(e) = opt_str(args, "evidence") {
                v.push("--evidence".into());
                v.push(e);
            }
            push_collab(&mut v, args)?;
            Ok((v, DEFAULT_TIMEOUT))
        }
        "c3_rate" => {
            v.push("findings".into());
            v.push("--task".into());
            v.push(req_str(args, "task")?);
            let n = opt_i64(args, "n")
                .ok_or_else(|| "missing required integer argument 'n'".to_string())?;
            v.push("--rate".into());
            v.push(n.to_string());
            v.push("--useful".into());
            v.push(req_str(args, "useful")?);
            if let Some(note) = opt_str(args, "note") {
                v.push("--note".into());
                v.push(note);
            }
            push_collab(&mut v, args)?;
            Ok((v, DEFAULT_TIMEOUT))
        }
        "c3_scoreboard" => {
            v.push("scoreboard".into());
            if let Some(t) = opt_str(args, "task") {
                v.push("--task".into());
                v.push(t);
            }
            if opt_bool(args, "json") {
                v.push("--json".into());
            }
            push_collab(&mut v, args)?;
            Ok((v, DEFAULT_TIMEOUT))
        }
        "c3_pack" => {
            v.push("pack".into());
            let brief = req_str(args, "brief")?;
            reject_escape(&brief)?;
            v.push("--brief".into());
            v.push(brief);
            let out = req_str(args, "out")?;
            reject_escape(&out)?;
            v.push("--out".into());
            v.push(out);
            for f in opt_arr(args, "focus") {
                reject_escape(&f)?;
                v.push("--focus".into());
                v.push(f);
            }
            if let Some(b) = opt_i64(args, "budget") {
                v.push("--budget".into());
                v.push(b.to_string());
            }
            if let Some(t) = opt_str(args, "task") {
                v.push("--task".into());
                v.push(t);
            }
            push_collab(&mut v, args)?;
            Ok((v, DEFAULT_TIMEOUT))
        }
        "c3_explain" => {
            v.push("explain".into());
            v.push("--claim".into());
            v.push(req_str(args, "claim")?);
            // The explainer pack is meant to leave the machine; the CLI asks
            // before writing, so the tool always confirms.
            v.push("--yes".into());
            for f in opt_arr(args, "focus") {
                reject_escape(&f)?;
                v.push("--focus".into());
                v.push(f);
            }
            if let Some(b) = opt_i64(args, "budget") {
                v.push("--budget".into());
                v.push(b.to_string());
            }
            if let Some(a) = opt_str(args, "audience") {
                v.push("--audience".into());
                v.push(a);
            }
            if let Some(o) = opt_str(args, "out") {
                reject_escape(&o)?;
                v.push("--out".into());
                v.push(o);
            }
            push_collab(&mut v, args)?;
            Ok((v, DEFAULT_TIMEOUT))
        }
        "c3_snapshot" => {
            v.push("snapshot".into());
            if let Some(o) = opt_str(args, "out") {
                reject_escape(&o)?;
                v.push("--out".into());
                v.push(o);
            }
            if opt_bool(args, "delta") {
                v.push("--delta".into());
            }
            if let Some(d) = opt_i64(args, "depth") {
                v.push("--depth".into());
                v.push(d.to_string());
            }
            if let Some(b) = opt_i64(args, "budget") {
                v.push("--budget".into());
                v.push(b.to_string());
            }
            for f in opt_arr(args, "focus") {
                reject_escape(&f)?;
                v.push("--focus".into());
                v.push(f);
            }
            push_collab(&mut v, args)?;
            Ok((v, DEFAULT_TIMEOUT))
        }
        "c3_telemetry_status" => {
            v.push("telemetry".into());
            v.push("status".into());
            Ok((v, DEFAULT_TIMEOUT))
        }
        "c3_index_stats" => {
            v.push("index".into());
            v.push("stats".into());
            v.push("--json".into());
            if let Some(c) = opt_str(args, "conn") {
                validate_conn(&c)?;
                v.push("--conn".into());
                v.push(c);
            }
            push_collab(&mut v, args)?;
            Ok((v, DEFAULT_TIMEOUT))
        }
        "c3_index_query" => {
            v.push("index".into());
            v.push("query".into());
            v.push(req_str(args, "query")?);
            if let Some(b) = opt_i64(args, "budget") {
                v.push("--budget".into());
                v.push(b.to_string());
            }
            if opt_bool(args, "json") {
                v.push("--json".into());
            }
            if let Some(c) = opt_str(args, "conn") {
                validate_conn(&c)?;
                v.push("--conn".into());
                v.push(c);
            }
            push_collab(&mut v, args)?;
            Ok((v, DEFAULT_TIMEOUT))
        }
        other => Err(format!("unknown tool: {other}")),
    }
}

/// Validate an `index` `conn` string's path portion, if it names a local path
/// (`surrealkv:<path>` / `surrealkv://<path>`); a `none` or `ws://` connection string
/// carries no path to contain.
fn validate_conn(conn: &str) -> Result<(), String> {
    let c = conn.trim();
    let path = c
        .strip_prefix("surrealkv://")
        .or_else(|| c.strip_prefix("surrealkv:"));
    if let Some(p) = path {
        reject_escape(p)?;
    }
    Ok(())
}

/// Validate a slug argument (task id, topic, role name) against the plugin's slug rule.
fn validate_slug(val: &str, field: &str) -> Result<(), String> {
    if is_slug(val) {
        Ok(())
    } else {
        Err(format!(
            "{field} argument '{val}' is not a slug (letters, digits, dot, dash, underscore; the first character a letter or digit)"
        ))
    }
}

/// Validate `val` against a fixed enumeration.
fn validate_enum(val: &str, allowed: &[&str], field: &str) -> Result<(), String> {
    if allowed.contains(&val) {
        Ok(())
    } else {
        Err(format!(
            "{field} must be one of {} (got '{val}')",
            allowed.join(", ")
        ))
    }
}

/// Validate an integer argument is within `lo..=hi`.
fn validate_range(val: i64, lo: i64, hi: i64, field: &str) -> Result<(), String> {
    if (lo..=hi).contains(&val) {
        Ok(())
    } else {
        Err(format!("{field} must be between {lo} and {hi} (got {val})"))
    }
}

/// Validate `--require` values: at most 8, each at most 128 bytes, no control characters,
/// and never starting with `-` (they are passed as the VALUE of `--require`, one flag per
/// value, so an argument can never be read as a flag).
fn validate_require(items: &[String]) -> Result<(), String> {
    if items.len() > 8 {
        return Err(format!(
            "--require accepts at most 8 values (got {})",
            items.len()
        ));
    }
    // The refusal names the position, never the value: the answer goes into a model's context,
    // and a value that is refused for its control characters must not be echoed with them.
    for (i, r) in items.iter().enumerate() {
        let n = i + 1;
        if r.is_empty() {
            return Err(format!("--require value {n} is empty"));
        }
        if r.len() > 128 {
            return Err(format!("--require value {n} exceeds 128 bytes"));
        }
        if r.chars().any(|c| c.is_control()) {
            return Err(format!("--require value {n} contains a control character"));
        }
        if r.starts_with('-') {
            return Err(format!("--require value {n} must not start with '-'"));
        }
    }
    Ok(())
}

/// Push a validated `--collab-dir` if the argument is present.
fn push_collab(v: &mut Vec<String>, args: &Value) -> Result<(), String> {
    if let Some(c) = opt_str(args, "collab_dir") {
        reject_escape(&c)?;
        v.push("--collab-dir".into());
        v.push(c);
    }
    Ok(())
}

/// The captured outcome of a child `c3` run.
struct CliResult {
    stdout: String,
    stderr: String,
    code: Option<i32>,
    timed_out: bool,
}

/// Re-invoke this executable with `argv`, capturing stdout/stderr and enforcing a
/// timeout by polling `try_wait` (no extra dependency). The child inherits the
/// environment and this process's working directory; its stdin is closed.
fn run_cli(argv: &[String], timeout: Duration) -> Result<CliResult, String> {
    let exe =
        std::env::current_exe().map_err(|e| format!("cannot locate the c3 executable: {e}"))?;
    dlog(&format!("run: {} {:?}", exe.display(), argv));
    let mut child = Command::new(&exe)
        .args(argv)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn c3: {e}"))?;

    // Drain both pipes on threads so a full pipe never deadlocks the wait loop.
    let mut out_pipe = child.stdout.take().expect("stdout piped");
    let mut err_pipe = child.stderr.take().expect("stderr piped");
    let out_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out_pipe.read_to_end(&mut buf);
        buf
    });
    let err_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = err_pipe.read_to_end(&mut buf);
        buf
    });

    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    timed_out = true;
                    break None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("waiting on c3 failed: {e}")),
        }
    };

    let stdout = String::from_utf8_lossy(&out_handle.join().unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&err_handle.join().unwrap_or_default()).into_owned();
    Ok(CliResult {
        stdout,
        stderr,
        code: status.and_then(|s| s.code()),
        timed_out,
    })
}

/// Assemble the tool result text: the console output plus the exit code (and any
/// stderr). A non-zero exit or a timeout is reported by the caller via `isError`.
fn format_result(r: &CliResult) -> String {
    let mut out = String::new();
    out.push_str(r.stdout.trim_end());
    if !r.stderr.trim().is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str("--- stderr ---\n");
        out.push_str(r.stderr.trim_end());
    }
    if !out.is_empty() {
        out.push('\n');
    }
    if r.timed_out {
        out.push_str("[timed out]");
    } else {
        match r.code {
            Some(c) => out.push_str(&format!("[exit code: {c}]")),
            None => out.push_str("[terminated without an exit code]"),
        }
    }
    out
}

/// True when the CLI run indicates failure (non-zero exit or a timeout).
fn is_failure(r: &CliResult) -> bool {
    r.timed_out || r.code != Some(0)
}

// ---------------------------------------------------------------------------
// Tool definitions (name, description for a coordinator model, input schema)
// ---------------------------------------------------------------------------

fn tool_defs() -> Value {
    json!([
        {
            "name": "c3_providers",
            "description": "List the configured C3 reviewer providers (Codex model endpoints) and whether each is usable right now — the same table the `c3 providers` CLI prints. Use it before a consultation to sanity-check who is available, or with `short: true` for a one-line summary of what is OUT and how many reviewers are ready. Read-only; writes nothing.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "provider": { "type": "string", "description": "Report only this provider (case-sensitive, as in the config)." },
                    "short": { "type": "boolean", "description": "One line: what is OUT per roster entry and how many reviewers are available." },
                    "no_network": { "type": "boolean", "description": "Do no network call (an agy sign-in reads 'not checked'; muse stays local)." },
                    "collab_dir": { "type": "string", "description": "Where consultations are stored; a relative path resolves against the repo root. Must stay inside the working directory." }
                }
            }
        },
        {
            "name": "c3_consult",
            "description": "Ask one read-only reviewer (a different lab's model, via the Codex engine) to look at a one-page brief and reply with structured findings, recorded as files next to the code (the brief, the reply verbatim, a JSON ledger entry, findings tracked by id). This is the core consultation and MAY TAKE MINUTES — the call blocks until the reviewer finishes (timeout up to one hour). A background panel with detach/status is NOT available yet (milestone 4), so run one reviewer at a time. C3 never commits: this only reads and records. `task` groups the consultation; `purpose` picks the preset (framing/decision/checkpoint/acceptance/diff-review/stuck/chore); `brief` is a repo path C3 reads, never writes; `prompt` is the ask.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task": { "type": "string", "description": "Task slug (letters, digits, dot, dash, underscore) grouping this consultation." },
                    "purpose": { "type": "string", "description": "framing | decision | checkpoint | core-contract | acceptance | diff-review | stuck | chore." },
                    "brief": { "type": "string", "description": "The one-page brief file, relative to the working directory with no '..' component; C3 reads it, never writes." },
                    "prompt": { "type": "string", "description": "The ask, prepended to the prompt." },
                    "reply_name": { "type": "string", "description": "The handoff base name (a slug), e.g. `reply`." },
                    "mode": { "type": "string", "description": "new | fork | resume (empty = auto)." },
                    "thread": { "type": "string", "description": "The thread uuid to fork or resume (needs mode fork or resume)." },
                    "provider": { "type": "string", "description": "The provider label; needs `model`." },
                    "model": { "type": "string", "description": "The model id." },
                    "effort": { "type": "string", "description": "low | medium | high | xhigh." },
                    "max_words": { "type": "integer", "description": "reply_markdown word cap (0 = the purpose preset)." },
                    "timeout_sec": { "type": "integer", "description": "Run timeout in seconds (0 = the purpose default)." },
                    "continue_sec": { "type": "integer", "description": "Timeout-continuation budget (-1 = min(timeout,900); 0 = none)." },
                    "range": { "type": "string", "description": "base..head or base...head (diff-review / acceptance only)." },
                    "artifact": { "type": "array", "items": { "type": "string" }, "description": "File(s) bound to the review (repo-relative)." },
                    "raw": { "type": "boolean", "description": "A plain-text consultation: no schema, no findings bookkeeping." },
                    "dry_run": { "type": "boolean", "description": "Print the plan and exit; writes nothing." },
                    "skip_preflight": { "type": "boolean", "description": "Skip the availability preflight (then warn)." },
                    "off_peak_only": { "type": "boolean", "description": "Refuse a run inside the provider's declared peak window." },
                    "codex_config": { "type": "array", "items": { "type": "string" }, "description": "Per-run `-c key=value` overrides (identity/effort keys are refused)." },
                    "schema_transport": { "type": "string", "description": "output-schema | prompt-only (empty = the endpoint default)." },
                    "format_retry": { "type": "integer", "description": "One format-repair turn if the reply is not valid JSON (0|1)." },
                    "collab_dir": { "type": "string", "description": "Where consultations are stored; inside the working directory." },
                    "telemetry": { "type": "string", "description": "on | off (default on)." }
                },
                "required": ["task", "purpose", "brief", "prompt", "reply_name"]
            }
        },
        {
            "name": "c3_panel",
            "description": "Start a review panel: several roster reviewers read the same brief and reply independently, recorded next to the code exactly like `c3_consult`. Runs `c3 consult --panel`. A panel takes MINUTES, so `detach` DEFAULTS TO TRUE: the call returns at once with the three lines `c3 consult --status`/`--wait`/`--id` print, which carry the detach id `c3_status` reads later. Pass `dry_run: true` to print the plan and write nothing (dry_run never carries --detach, since the CLI refuses the two together). C3 never commits: this only reads and records.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task": { "type": "string", "description": "Task slug grouping this panel." },
                    "brief": { "type": "string", "description": "The one-page brief file, relative to the working directory with no '..' component; C3 reads it, never writes." },
                    "artifacts": { "type": "array", "items": { "type": "string" }, "description": "File(s) bound to the review (repo-relative)." },
                    "purpose": { "type": "string", "description": "framing | decision | checkpoint | core-contract | acceptance | diff-review | stuck | chore." },
                    "size": { "type": "integer", "description": "Panel seats, 1..=16 (omit for the purpose default)." },
                    "order": { "type": "string", "description": "routed (a seeded weighted draw, default) | roster." },
                    "require": { "type": "array", "items": { "type": "string" }, "description": "Required reviewer matcher(s) (#n, a label, or `<provider> :: <model> [engine]`); at most 8, each at most 128 bytes, no control characters, never starting with '-'." },
                    "role": { "type": "string", "description": "One role (a slug) assigned to every panel member." },
                    "roles": { "type": "array", "items": { "type": "string" }, "description": "Roles (slugs) assigned to panel seats by score rank and willingness." },
                    "topics": { "type": "array", "items": { "type": "string" }, "description": "Topic tag(s) (slugs) for routing/rating." },
                    "dry_run": { "type": "boolean", "description": "Print the plan and exit; writes nothing (default false)." },
                    "detach": { "type": "boolean", "description": "Run in the background and return at once. DEFAULT TRUE." },
                    "timeout_sec": { "type": "integer", "description": "Run timeout in seconds (0 = the purpose default)." },
                    "collab_dir": { "type": "string", "description": "Where consultations are stored; inside the working directory." }
                },
                "required": ["task", "brief"]
            }
        },
        {
            "name": "c3_status",
            "description": "Read the state of a task's detached runs (from `c3_panel` or a detached `c3_consult`): the same output as `c3 consult --status` or, with `wait: true`, `c3 consult --wait`. THE EXIT CODE IS THE ANSWER, returned alongside the text: 0 usable, 1 failed or refused, 2 still running, 3 wait timeout, 4 ambiguous id (name it more precisely with `id`), 5 a required reviewer is missing. Read-only; writes nothing except with `wait: true`, which blocks until the run finishes or `wait_timeout_sec` elapses.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task": { "type": "string", "description": "The task slug." },
                    "id": { "type": "string", "description": "A detach id, or its beginning, to select one run." },
                    "wait": { "type": "boolean", "description": "Block until the detached run(s) are done instead of reporting the current state (default false)." },
                    "wait_timeout_sec": { "type": "integer", "description": "1..=3600; default 300 when wait is true." },
                    "collab_dir": { "type": "string", "description": "Where consultations are stored; inside the working directory." }
                },
                "required": ["task"]
            }
        },
        {
            "name": "c3_router_explain",
            "description": "Print the score table the next routed panel draw would use, one line per roster lineage — the same output as `c3 router explain`. Read-only.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "purpose": { "type": "string", "description": "framing | decision | checkpoint | core-contract | acceptance | diff-review | stuck | chore." },
                    "topic": { "type": "array", "items": { "type": "string" }, "description": "Topic tag(s) (slugs) to score against." },
                    "collab_dir": { "type": "string", "description": "Where consultation ratings are stored; inside the working directory." }
                },
                "required": ["purpose"]
            }
        },
        {
            "name": "c3_router_replay",
            "description": "Replay a routed panel from the ledger and confirm the seats it drew match — the same output as `c3 router replay`. Read-only.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task": { "type": "string", "description": "The task (its directory under collab_dir)." },
                    "nn": { "type": "integer", "description": "Replay only this consult number; omit to replay every distinct routed panel." },
                    "collab_dir": { "type": "string", "description": "Where consultations are stored; inside the working directory." }
                },
                "required": ["task"]
            }
        },
        {
            "name": "c3_findings_list",
            "description": "List the tracked findings for a task (from `findings.json`). By default it prints the open findings (proposed | implemented); pass `all: true` for every finding including verified/rejected/wontfix/superseded. Read-only.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task": { "type": "string", "description": "The task slug." },
                    "all": { "type": "boolean", "description": "Print every finding, not just the open ones." },
                    "collab_dir": { "type": "string", "description": "Where consultations are stored; inside the working directory." }
                },
                "required": ["task"]
            }
        },
        {
            "name": "c3_findings_stats",
            "description": "One line per consultation of a task (from `sessions.json`), with its findings broken down by status. Use it to see how each reviewer's findings have landed. Read-only.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task": { "type": "string", "description": "The task slug." },
                    "collab_dir": { "type": "string", "description": "Where consultations are stored; inside the working directory." }
                },
                "required": ["task"]
            }
        },
        {
            "name": "c3_findings_status",
            "description": "Move one tracked finding to a new status and record it in `findings.json`. Status is one of proposed | implemented | verified | rejected | wontfix | superseded. A `note` is required for `rejected` and for reopening to `proposed`; `evidence` (what was run / where the proof is) is required for `verified`. This records the state change only — it never commits code.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task": { "type": "string", "description": "The task slug." },
                    "id": { "type": "string", "description": "Finding id (`F<NN>-<k>`)." },
                    "status": { "type": "string", "description": "proposed | implemented | verified | rejected | wontfix | superseded." },
                    "note": { "type": "string", "description": "Why (required for rejected and for a reopen to proposed)." },
                    "evidence": { "type": "string", "description": "What was run / where the proof is (required for verified)." },
                    "collab_dir": { "type": "string", "description": "Where consultations are stored; inside the working directory." }
                },
                "required": ["task", "id", "status"]
            }
        },
        {
            "name": "c3_rate",
            "description": "Rate consultation number `n` of a task for how useful it was (yes | partly | no), recorded against that ledger entry. This feeds the reviewer scoreboard and later routing. Records only; no commit.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task": { "type": "string", "description": "The task slug." },
                    "n": { "type": "integer", "description": "The consultation number (a ledger entry of the task) to rate." },
                    "useful": { "type": "string", "description": "yes | partly | no." },
                    "note": { "type": "string", "description": "Optional note on the rating." },
                    "collab_dir": { "type": "string", "description": "Where consultations are stored; inside the working directory." }
                },
                "required": ["task", "n", "useful"]
            }
        },
        {
            "name": "c3_scoreboard",
            "description": "Print the per-reviewer usefulness scoreboard — how each provider/model has scored across rated consultations. Omit `task` for every task, or name one to scope it; `json: true` returns row objects instead of the table. Read-only.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task": { "type": "string", "description": "One task only; omit for every task." },
                    "json": { "type": "boolean", "description": "Return an array of row objects instead of the printed table." },
                    "collab_dir": { "type": "string", "description": "Where consultations are stored; inside the working directory." }
                }
            }
        },
        {
            "name": "c3_pack",
            "description": "Build a reviewer pack (a self-contained context bundle) from a one-page brief plus focus files, for the http engine, writing the pack and a `.pack.json` sidecar to `out`. Use it to prepare context for a reviewer that cannot browse the repo. Reads the repo and writes only the pack files (no commit).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "brief": { "type": "string", "description": "The one-page brief file (repo path)." },
                    "focus": { "type": "array", "items": { "type": "string" }, "description": "Focus file(s): path or glob, included in full (repeatable)." },
                    "budget": { "type": "integer", "description": "Token budget for the periphery (0 = none)." },
                    "task": { "type": "string", "description": "Task slug whose findings.json supplies the open-findings snapshot." },
                    "out": { "type": "string", "description": "Output path for the pack; the .pack.json sidecar is written beside it." },
                    "collab_dir": { "type": "string", "description": "Where consultations are stored; inside the working directory." }
                },
                "required": ["brief", "out"]
            }
        },
        {
            "name": "c3_explain",
            "description": "Build an explainer pack for one claim: a context bundle assembled around focus files and written to a markdown file. IMPORTANT: this file is meant to LEAVE THE MACHINE (it is what you hand to an outside audience), so the tool always confirms the write on your behalf. Reads the repo and writes the explainer file only; it never commits.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "claim": { "type": "string", "description": "The claim to explain, verbatim (marked unverified in the pack)." },
                    "focus": { "type": "array", "items": { "type": "string" }, "description": "Focus file(s): path or glob, in full with line numbers (repeatable)." },
                    "budget": { "type": "integer", "description": "Token budget for the periphery (0 = none)." },
                    "audience": { "type": "string", "description": "Who the explanation is for (free text)." },
                    "out": { "type": "string", "description": "Output path. Default: <collab>/.c3/explains/<repo>_<ts>.md." },
                    "collab_dir": { "type": "string", "description": "Where consultations are stored; inside the working directory." }
                },
                "required": ["claim"]
            }
        },
        {
            "name": "c3_snapshot",
            "description": "Take a repository snapshot (a single markdown context file), optionally as a git delta since C3's last full-snapshot anchor. Depth 0-9 controls truncation (0 tree only, 5-6 skeleton, 7-9 full); a token budget trims the periphery while focus globs are kept whole. Reads the repo and writes the snapshot file; a full (non-delta) snapshot also updates C3's anchor and sequence state under <collab>/.c3/. No commit.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "out": { "type": "string", "description": "Output path. Default under <collab>/.c3/snapshots/." },
                    "delta": { "type": "boolean", "description": "Snapshot only what changed since C3's anchor." },
                    "depth": { "type": "integer", "description": "0 tree only, 1-4 truncate, 5-6 skeleton, 7-9 full (default 9)." },
                    "budget": { "type": "integer", "description": "Token budget; periphery is trimmed to fit (0 = none)." },
                    "focus": { "type": "array", "items": { "type": "string" }, "description": "Keep matching files whole regardless of depth/budget (repeatable glob)." },
                    "collab_dir": { "type": "string", "description": "Where consultations are stored; inside the working directory." }
                }
            }
        },
        {
            "name": "c3_telemetry_status",
            "description": "Print C3's telemetry on/off status and this installation's instance id (the same output as `c3 telemetry status`). Telemetry is on by default with a one-line off switch. Read-only; creates nothing — if no instance id has been created yet, it reports that instead of creating one.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "c3_index_stats",
            "description": "Counts per table, the generation, the backend and its path for the derived context index (the same output as `c3 index stats --json`). Read-only; never builds or rebuilds the index (that stays CLI-only — it takes the index lock for minutes).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "collab_dir": { "type": "string", "description": "Where the index lives (<collab>/.c3/index by default); inside the working directory." },
                    "conn": { "type": "string", "description": "Connection string: none | surrealkv:<path> | ws://host. Default: an embedded store at <collab>/.c3/index/." }
                }
            }
        },
        {
            "name": "c3_index_query",
            "description": "BM25 + reciprocal-rank retrieval with a bounded 1-hop expansion over the derived context index (the same output as `c3 index query`). Read-only; never builds or rebuilds the index (that stays CLI-only — it takes the index lock for minutes).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "The search text." },
                    "budget": { "type": "integer", "description": "Token budget for the returned hits (0 = no cap)." },
                    "json": { "type": "boolean", "description": "Return hit objects instead of the rendered text." },
                    "collab_dir": { "type": "string", "description": "Where the index lives (<collab>/.c3/index by default); inside the working directory." },
                    "conn": { "type": "string", "description": "Connection string: none | surrealkv:<path> | ws://host. Default: an embedded store at <collab>/.c3/index/." }
                },
                "required": ["query"]
            }
        }
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(id: i64, method: &str, params: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
    }

    #[test]
    fn initialize_echoes_protocol_and_declares_tools() {
        let reply = handle_message(&req(
            1,
            "initialize",
            json!({ "protocolVersion": "2025-03-26" }),
        ))
        .unwrap();
        assert_eq!(reply["jsonrpc"], "2.0");
        assert_eq!(reply["id"], 1);
        assert_eq!(reply["result"]["protocolVersion"], "2025-03-26");
        assert!(reply["result"]["capabilities"]["tools"].is_object());
        assert_eq!(reply["result"]["serverInfo"]["name"], "c3");
    }

    #[test]
    fn initialize_defaults_protocol_when_absent() {
        let reply = handle_message(&req(1, "initialize", json!({}))).unwrap();
        assert_eq!(reply["result"]["protocolVersion"], DEFAULT_PROTOCOL_VERSION);
    }

    #[test]
    fn ping_returns_empty_result() {
        let reply = handle_message(&req(7, "ping", json!({}))).unwrap();
        assert_eq!(reply["id"], 7);
        assert!(reply["result"].is_object());
    }

    #[test]
    fn tools_list_names_every_tool() {
        let reply = handle_message(&req(2, "tools/list", json!({}))).unwrap();
        let tools = reply["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        for expected in [
            "c3_providers",
            "c3_consult",
            "c3_panel",
            "c3_status",
            "c3_router_explain",
            "c3_router_replay",
            "c3_findings_list",
            "c3_findings_stats",
            "c3_findings_status",
            "c3_rate",
            "c3_scoreboard",
            "c3_pack",
            "c3_explain",
            "c3_snapshot",
            "c3_telemetry_status",
            "c3_index_stats",
            "c3_index_query",
        ] {
            assert!(names.contains(&expected), "missing tool {expected}");
        }
        // Every tool carries a description and an object input schema.
        for t in tools {
            assert!(t["description"]
                .as_str()
                .map(|s| !s.is_empty())
                .unwrap_or(false));
            assert_eq!(t["inputSchema"]["type"], "object");
        }
    }

    #[test]
    fn unknown_method_is_method_not_found() {
        let reply = handle_message(&req(3, "does/not/exist", json!({}))).unwrap();
        assert_eq!(reply["error"]["code"], -32601);
        assert_eq!(reply["id"], 3);
    }

    #[test]
    fn notifications_get_no_reply() {
        let note = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        assert!(handle_message(&note).is_none());
    }

    #[test]
    fn parse_error_frame_on_bad_json() {
        let input = b"not json at all\n" as &[u8];
        let mut out: Vec<u8> = Vec::new();
        serve(std::io::BufReader::new(input), &mut out).unwrap();
        let line = String::from_utf8(out).unwrap();
        let v: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(v["error"]["code"], -32700);
        assert!(v["id"].is_null());
    }

    #[test]
    fn serve_answers_a_script_of_messages() {
        let mut input = String::new();
        input.push_str(
            &req(1, "initialize", json!({ "protocolVersion": "2025-06-18" })).to_string(),
        );
        input.push('\n');
        input.push_str(
            &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }).to_string(),
        );
        input.push('\n');
        input.push_str(&req(2, "tools/list", json!({})).to_string());
        input.push('\n');
        let mut out: Vec<u8> = Vec::new();
        serve(std::io::BufReader::new(input.as_bytes()), &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        // Two responses: initialize + tools/list. The notification produced none.
        assert_eq!(lines.len(), 2, "got: {text}");
        let init: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(init["id"], 1);
        let list: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(list["id"], 2);
        assert!(list["result"]["tools"].is_array());
    }

    #[test]
    fn build_argv_maps_flags() {
        let (argv, _t) = build_argv(
            "c3_providers",
            &json!({ "short": true, "no_network": true }),
        )
        .unwrap();
        assert_eq!(argv[0], "providers");
        assert!(argv.contains(&"--short".to_string()));
        assert!(argv.contains(&"--no-network".to_string()));
    }

    #[test]
    fn build_argv_rejects_path_escape() {
        let err = build_argv(
            "c3_pack",
            &json!({ "brief": "../evil.md", "out": "pack.md" }),
        )
        .unwrap_err();
        assert!(err.contains("refused"), "got: {err}");
    }

    #[test]
    fn build_argv_requires_consult_fields() {
        let err = build_argv("c3_consult", &json!({ "task": "t" })).unwrap_err();
        assert!(
            err.contains("purpose") || err.contains("required"),
            "got: {err}"
        );
    }

    #[test]
    fn explain_always_confirms() {
        let (argv, _t) = build_argv("c3_explain", &json!({ "claim": "x" })).unwrap();
        assert!(argv.contains(&"--yes".to_string()));
    }

    #[test]
    fn unknown_tool_errors() {
        assert!(build_argv("c3_nope", &json!({})).is_err());
    }

    #[test]
    fn index_stats_always_requests_json() {
        let (argv, _t) = build_argv("c3_index_stats", &json!({})).unwrap();
        assert_eq!(argv[0], "index");
        assert_eq!(argv[1], "stats");
        assert!(argv.contains(&"--json".to_string()));
    }

    #[test]
    fn index_query_maps_flags() {
        let (argv, _t) = build_argv(
            "c3_index_query",
            &json!({ "query": "consult flow", "budget": 500, "json": true }),
        )
        .unwrap();
        assert_eq!(argv[0], "index");
        assert_eq!(argv[1], "query");
        assert_eq!(argv[2], "consult flow");
        assert!(argv.contains(&"--budget".to_string()));
        assert!(argv.contains(&"500".to_string()));
        assert!(argv.contains(&"--json".to_string()));
    }

    #[test]
    fn index_query_requires_query() {
        let err = build_argv("c3_index_query", &json!({})).unwrap_err();
        assert!(err.contains("query"), "got: {err}");
    }

    #[test]
    fn index_conn_rejects_path_escape() {
        let err = build_argv(
            "c3_index_query",
            &json!({ "query": "x", "conn": "surrealkv:../evil" }),
        )
        .unwrap_err();
        assert!(err.contains("refused"), "got: {err}");
    }

    // ------------------------------------------------------------------
    // c3_panel / c3_status / c3_router_explain / c3_router_replay
    // ------------------------------------------------------------------

    #[test]
    fn panel_dry_run_builds_expected_argv() {
        let (argv, _t) = build_argv(
            "c3_panel",
            &json!({
                "task": "wave27",
                "brief": "brief.md",
                "purpose": "diff-review",
                "size": 4,
                "order": "roster",
                "require": ["#1", "openai::gpt-6"],
                "topics": ["consult-flow"],
                "dry_run": true,
            }),
        )
        .unwrap();
        assert_eq!(
            argv,
            vec![
                "consult",
                "--task",
                "wave27",
                "--brief",
                "brief.md",
                "--panel",
                "--purpose",
                "diff-review",
                "--panel-size",
                "4",
                "--panel-order",
                "roster",
                "--require",
                "#1",
                "--require",
                "openai::gpt-6",
                "--topic",
                "consult-flow",
                "--dry-run",
            ]
        );
    }

    #[test]
    fn panel_detach_defaults_to_true() {
        let (argv, _t) =
            build_argv("c3_panel", &json!({ "task": "t", "brief": "brief.md" })).unwrap();
        assert!(argv.contains(&"--detach".to_string()));
        assert!(!argv.contains(&"--dry-run".to_string()));
    }

    #[test]
    fn panel_detach_false_is_honoured() {
        let (argv, _t) = build_argv(
            "c3_panel",
            &json!({ "task": "t", "brief": "brief.md", "detach": false }),
        )
        .unwrap();
        assert!(!argv.contains(&"--detach".to_string()));
    }

    #[test]
    fn panel_dry_run_never_carries_detach() {
        let (argv, _t) = build_argv(
            "c3_panel",
            &json!({ "task": "t", "brief": "brief.md", "dry_run": true, "detach": true }),
        )
        .unwrap();
        assert!(argv.contains(&"--dry-run".to_string()));
        assert!(!argv.contains(&"--detach".to_string()));
    }

    #[test]
    fn panel_rejects_brief_path_escape() {
        let err =
            build_argv("c3_panel", &json!({ "task": "t", "brief": "../evil.md" })).unwrap_err();
        assert!(err.contains("refused"), "got: {err}");
    }

    #[test]
    fn panel_rejects_artifact_path_escape() {
        let err = build_argv(
            "c3_panel",
            &json!({ "task": "t", "brief": "brief.md", "artifacts": ["../evil.md"] }),
        )
        .unwrap_err();
        assert!(err.contains("refused"), "got: {err}");
    }

    #[test]
    fn panel_rejects_require_starting_with_dash() {
        let err = build_argv(
            "c3_panel",
            &json!({ "task": "t", "brief": "brief.md", "require": ["-x"] }),
        )
        .unwrap_err();
        assert!(err.contains("start"), "got: {err}");
    }

    #[test]
    fn panel_rejects_too_many_require_values() {
        let items: Vec<String> = (0..9).map(|i| format!("r{i}")).collect();
        let err = build_argv(
            "c3_panel",
            &json!({ "task": "t", "brief": "brief.md", "require": items }),
        )
        .unwrap_err();
        assert!(err.contains("at most 8"), "got: {err}");
    }

    #[test]
    fn panel_rejects_size_out_of_range() {
        for bad in [0, 17] {
            let err = build_argv(
                "c3_panel",
                &json!({ "task": "t", "brief": "brief.md", "size": bad }),
            )
            .unwrap_err();
            assert!(err.contains("between"), "got: {err}");
        }
    }

    #[test]
    fn panel_rejects_non_slug_task() {
        let err = build_argv(
            "c3_panel",
            &json!({ "task": "not a slug", "brief": "brief.md" }),
        )
        .unwrap_err();
        assert!(err.contains("slug"), "got: {err}");
    }

    #[test]
    fn panel_rejects_bad_order() {
        let err = build_argv(
            "c3_panel",
            &json!({ "task": "t", "brief": "brief.md", "order": "sideways" }),
        )
        .unwrap_err();
        assert!(err.contains("one of"), "got: {err}");
    }

    #[test]
    fn status_builds_expected_argv() {
        let (argv, _t) = build_argv("c3_status", &json!({ "task": "wave27" })).unwrap();
        assert_eq!(argv, vec!["consult", "--task", "wave27", "--status"]);
    }

    #[test]
    fn status_wait_builds_expected_argv_with_default_timeout() {
        let (argv, _t) = build_argv(
            "c3_status",
            &json!({ "task": "t", "wait": true, "id": "abcd" }),
        )
        .unwrap();
        assert_eq!(
            argv,
            vec![
                "consult",
                "--task",
                "t",
                "--wait",
                "--id",
                "abcd",
                "--wait-timeout-sec",
                "300"
            ]
        );
    }

    #[test]
    fn status_rejects_wait_timeout_out_of_range() {
        let err = build_argv(
            "c3_status",
            &json!({ "task": "t", "wait": true, "wait_timeout_sec": 3601 }),
        )
        .unwrap_err();
        assert!(err.contains("between"), "got: {err}");
    }

    #[test]
    fn router_explain_builds_expected_argv() {
        let (argv, _t) = build_argv(
            "c3_router_explain",
            &json!({ "purpose": "framing", "topic": ["consult-flow"] }),
        )
        .unwrap();
        assert_eq!(
            argv,
            vec![
                "router",
                "explain",
                "--purpose",
                "framing",
                "--topic",
                "consult-flow"
            ]
        );
    }

    #[test]
    fn router_explain_rejects_bad_purpose() {
        let err = build_argv("c3_router_explain", &json!({ "purpose": "nope" })).unwrap_err();
        assert!(err.contains("one of"), "got: {err}");
    }

    #[test]
    fn router_replay_builds_expected_argv() {
        let (argv, _t) =
            build_argv("c3_router_replay", &json!({ "task": "wave27", "nn": 3 })).unwrap();
        assert_eq!(
            argv,
            vec!["router", "replay", "--task", "wave27", "--nn", "3"]
        );
    }

    // ------------------------------------------------------------------
    // F03-1: symlink/junction path containment
    // ------------------------------------------------------------------

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "c3-mcp-contain-{tag}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[cfg(windows)]
    fn make_link(link: &Path, target: &Path) -> bool {
        std::process::Command::new("cmd")
            .arg("/c")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(target)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[cfg(not(windows))]
    fn make_link(link: &Path, target: &Path) -> bool {
        std::os::unix::fs::symlink(target, link).is_ok()
    }

    #[test]
    fn check_contained_follows_symlink_targets() {
        let base = scratch_dir("base");
        let outside = scratch_dir("outside");
        let link = base.join("inside-link");

        if !make_link(&link, &outside) {
            eprintln!(
                "skipping check_contained_follows_symlink_targets: could not create a \
                 junction/symlink (needs Developer Mode or admin on this Windows host)"
            );
            let _ = std::fs::remove_dir_all(&base);
            let _ = std::fs::remove_dir_all(&outside);
            return;
        }

        // A path lexically inside `base` but through the link resolves outside it, so
        // both an existing and a not-yet-created target must be refused.
        let err = check_contained(&base, "inside-link").unwrap_err();
        assert!(err.contains("refused"), "got: {err}");
        let err = check_contained(&base, "inside-link/pack.md").unwrap_err();
        assert!(err.contains("refused"), "got: {err}");

        // A plain in-base path is still accepted.
        assert!(check_contained(&base, "plain.md").is_ok());

        let _ = std::fs::remove_dir_all(&link);
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_dir_all(&outside);
    }
}
