//! The findings tool behind `c3 findings` (milestone 3): `--list` with orphan
//! detection, status transitions with their required note/evidence, ratings, and
//! `--stats`, all through `c3_core::store` under the task's write lock and refused
//! while a consultation holds the task lock.
//!
//! A byte-for-byte port of `codex-findings.ps1`. It reuses `c3_core`'s store contracts:
//! [`EvidenceStore::take_task_lock`] (fail-fast ownership lock), `take_write_lock` (the
//! commit lock), [`c3_core::store::write_text_atomic`], the typed [`FindingsFile`] (which
//! round-trips byte-for-byte) for the rewrite, and [`c3_core::findings::Finding::set_status`]
//! for the gated transition. The refusal wording, the `-List`/`-Stats` layout and the
//! ratings-replace semantics are reproduced here to match the plugin exactly.

mod revision;

use crate::liveness::{pending, proc};

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::Value;

use c3_core::findings::{Finding, FindingStatus, FindingsFile};
use c3_core::lineage::format_reviewer_lineage;
use c3_core::store::{write_text_atomic, EvidenceStore, FilesStore, LockRecord};
use c3_core::task_slug::{is_slug, TaskSlug};

use crate::cli::findings::FindingsArgs;
use crate::providers::{resolve_collab_root, resolve_repo_root};

const TOOL: &str = "codex-findings";
const FINDING_STATUSES: [&str; 6] = [
    "proposed",
    "implemented",
    "verified",
    "rejected",
    "wontfix",
    "superseded",
];
const OPEN_STATUSES: [&str; 2] = ["proposed", "implemented"];

/// Run `c3 findings`; returns the process exit code.
pub fn run(args: FindingsArgs) -> i32 {
    match run_inner(args) {
        Ok(code) => code,
        Err(msg) => {
            // Stop-WithError: `codex-findings: <msg>`, exit 1.
            println!("{TOOL}: {msg}");
            1
        }
    }
}

// ----------------------------------------------------------------------------- Value access

fn pv<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.get(key).filter(|x| !x.is_null())
}

fn pv_str(v: &Value, key: &str) -> String {
    match pv(v, key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => (if *b { "True" } else { "False" }).to_string(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

/// A canonical `8-4-4-4-12` hex guid (`codex-findings.ps1`'s consult_id guard).
fn is_uuid(s: &str) -> bool {
    let parts: [usize; 5] = [8, 4, 4, 4, 12];
    let segs: Vec<&str> = s.split('-').collect();
    segs.len() == 5
        && segs
            .iter()
            .zip(parts.iter())
            .all(|(seg, &n)| seg.len() == n && seg.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn pv_str_or(v: &Value, key: &str, default: &str) -> String {
    match pv(v, key) {
        Some(_) => pv_str(v, key),
        None => default.to_string(),
    }
}

fn pv_i64(v: &Value, key: &str) -> Option<i64> {
    match pv(v, key) {
        Some(Value::Number(n)) => n.as_i64(),
        Some(Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    }
}

fn finding_consult(f: &Value) -> Option<i64> {
    let src = pv(f, "source")?;
    pv_i64(src, "consult")
}

fn orphan_check_consults(f: &Value, ledger_ns: &HashSet<i64>) -> Vec<i64> {
    let mut out: Vec<i64> = Vec::new();
    if let Some(Value::Array(checks)) = pv(f, "reviewer_checks") {
        for rc in checks {
            if rc.is_null() {
                continue;
            }
            if let Some(v) = pv_i64(rc, "consult") {
                if !ledger_ns.contains(&v) && !out.contains(&v) {
                    out.push(v);
                }
            }
        }
    }
    out
}

fn comparable_path(p: &str) -> String {
    p.replace('\\', "/").to_lowercase()
}

// ----------------------------------------------------------------------------- formatting

/// Pad `value` to `width` (by character count); left-justify when `left`, else right.
/// .NET composite formatting never truncates an over-long value.
fn pad(value: &str, width: usize, left: bool) -> String {
    let len = value.chars().count();
    if len >= width {
        return value.to_string();
    }
    let sp = " ".repeat(width - len);
    if left {
        format!("{value}{sp}")
    } else {
        format!("{sp}{value}")
    }
}

/// `Format-Locations` for the first location shown by `Format-FindingLine`.
fn format_one_location(loc: &Value) -> Option<String> {
    let p = pv_str(loc, "path");
    let line = pv(loc, "line");
    let mut text = p.clone();
    if let Some(l) = line {
        let ls = match l {
            Value::Number(n) => n.to_string(),
            Value::String(s) => s.clone(),
            _ => String::new(),
        };
        if !ls.is_empty() {
            text = format!("{p}:{ls}");
        }
    }
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// `Format-FindingLine`: `id  status  severity  location  claim` (claim clipped to 100).
fn format_finding_line(f: &Value, orphan: bool) -> String {
    let id = {
        let s = pv_str(f, "id");
        if s.is_empty() {
            "?".to_string()
        } else {
            s
        }
    };
    let status = {
        let s = pv_str(f, "status");
        if s.is_empty() {
            "?".to_string()
        } else {
            s
        }
    };
    let severity = {
        let s = pv_str(f, "severity");
        if s.is_empty() {
            "?".to_string()
        } else {
            s
        }
    };
    let locs: Vec<&Value> = match pv(f, "locations") {
        Some(Value::Array(a)) => a.iter().filter(|x| !x.is_null()).collect(),
        _ => Vec::new(),
    };
    let mut loc = "-".to_string();
    if !locs.is_empty() {
        if let Some(first) = format_one_location(locs[0]) {
            loc = first;
        } else {
            loc = "(no location)".to_string();
        }
        if locs.len() > 1 {
            loc += &format!(" (+{})", locs.len() - 1);
        }
    }
    let mut claim = c3_core::one_line(&pv_str(f, "claim"));
    if claim.chars().count() > 100 {
        claim = claim.chars().take(100).collect();
    }
    let mut line = format!(
        "{} {} {} {} {}",
        pad(&id, 9, true),
        pad(&status, 11, true),
        pad(&severity, 7, true),
        pad(&loc, 28, true),
        claim
    );
    if orphan {
        line += "  [ORPHAN: no ledger entry for its consult]";
    }
    line
}

// ----------------------------------------------------------------------------- store reads

/// `Read-JsonStore` + `Read-FindingsFile`, returning the tolerant `Value` store (with a
/// guaranteed `findings` array), `None` when the file does not exist, or the plugin's exact
/// refusal message.
fn read_findings_value(path: &Path, _task: &str) -> Result<Option<Value>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let restore = "An existing store is never replaced by a new one - restore it (e.g. from git) or move it aside deliberately, then retry.";
    if text.trim().is_empty() {
        return Err(format!(
            "refusing to use '{}': it is empty or could not be read. {restore}",
            path.display()
        ));
    }
    let value: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            return Err(format!(
                "refusing to use '{}': it does not parse: {}. {restore}",
                path.display(),
                c3_core::one_line(&e.to_string())
            ))
        }
    };
    if !value.is_object() {
        return Err(format!(
            "refusing to use '{}': it is not a JSON object. {restore}",
            path.display()
        ));
    }
    if value.get("findings").is_none() {
        return Err(format!(
            "refusing to use '{}': it has no findings array. Restore it (e.g. from git) or move it aside deliberately, then retry.",
            path.display()
        ));
    }
    Ok(Some(value))
}

/// The `findings[]` array (nulls dropped), as `Read-FindingsFile` keeps it.
fn findings_of(store: &Value) -> Vec<Value> {
    match store.get("findings") {
        Some(Value::Array(a)) => a.iter().filter(|x| !x.is_null()).cloned().collect(),
        _ => Vec::new(),
    }
}

/// The ledger's `codex.consults` (nulls dropped).
fn read_ledger(sessions_path: &Path) -> Vec<Value> {
    let text = match std::fs::read_to_string(sessions_path) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let v: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    match v.get("codex").and_then(|c| c.get("consults")) {
        Some(Value::Array(a)) => a.iter().filter(|x| !x.is_null()).cloned().collect(),
        _ => Vec::new(),
    }
}

// ----------------------------------------------------------------------------- pending line

/// `Write-PendingLine`: one line per interrupted consultation of the task.
fn write_pending_line(task_dir: &Path) {
    for path in pending::pending_paths(task_dir) {
        let rd = pending::read_pending_file(&path);
        if !rd.exists {
            continue;
        }
        if let Some(err) = rd.error {
            println!("pending: {err}");
            continue;
        }
        let r = match rd.record {
            Some(r) => r,
            None => continue,
        };
        let mut line = format!(
            "pending: state={}, n={}, nn={}, reply={}, started {} - an interrupted consultation; the next consultation consumes it ({})",
            pv_disp(&r, "state"),
            pv_disp(&r, "n"),
            pv_disp(&r, "nn"),
            pv_disp(&r, "reply"),
            pv_disp(&r, "started"),
            path.display()
        );
        let note = pending::pending_original_note(&r);
        if !note.is_empty() {
            line += &format!("; {note}");
        }
        println!("{line}");
    }
}

fn pv_disp(v: &Value, key: &str) -> String {
    let s = pv_str(v, key);
    if pv(v, key).is_none() {
        "?".to_string()
    } else {
        s
    }
}

// ----------------------------------------------------------------------------- lock refusals

fn read_lock_content(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    if text.trim().is_empty() {
        return None;
    }
    let v: Value = serde_json::from_str(&text).ok()?;
    if v.is_object() {
        Some(v)
    } else {
        None
    }
}

/// The `Enter-TaskLock` refusal message (the ownership lock is held by another handle).
fn task_lock_refusal(path: &Path, task: &str) -> String {
    let mut who = "a live process".to_string();
    for _ in 0..8 {
        if let Some(rec) = read_lock_content(path) {
            let pid = pv_i64(&rec, "pid").unwrap_or(0);
            let start = pv_str(&rec, "start_time");
            if pid > 0 && proc::pid_alive(pid as u32, &start) {
                who = format!(
                    "pid {} on {} since {}",
                    pv_str(&rec, "pid"),
                    pv_disp(&rec, "host"),
                    pv_disp(&rec, "started")
                );
                let panel = pv_str(&rec, "panel");
                if !panel.is_empty() {
                    let short: String = panel.chars().take(8).collect();
                    who += &format!(" (review panel {short})");
                }
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(125));
    }
    format!(
        "another consultation or status update for task '{task}' is running: {} is held open by {who}. Wait for it to finish; the lock is released when that process exits.",
        path.display()
    )
}

fn write_lock_timeout() -> f64 {
    std::env::var("CODEX_CONSULT_TEST_WRITE_LOCK_SEC")
        .ok()
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|v| *v > 0.0)
        .unwrap_or(60.0)
}

fn fmt_secs(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// The `Enter-WriteLock` timeout refusal (the write lock was held past the timeout).
fn write_lock_refusal(path: &Path, task: &str) -> String {
    let mut who = "a live process".to_string();
    if let Some(rec) = read_lock_content(path) {
        let pid = pv_i64(&rec, "pid").unwrap_or(0);
        let start = pv_str(&rec, "start_time");
        if pid > 0 && proc::pid_alive(pid as u32, &start) {
            who = format!(
                "pid {} on {} since {}",
                pv_str(&rec, "pid"),
                pv_disp(&rec, "host"),
                pv_disp(&rec, "started")
            );
        }
    }
    format!(
        "the write lock '{}' of task '{task}' was not acquired within {} s: it is held open by {who}",
        path.display(),
        fmt_secs(write_lock_timeout())
    )
}

// ----------------------------------------------------------------------------- entry point

struct Ctx {
    args: FindingsArgs,
    status: String,
    useful: String,
    rating: bool,
    repo_root: PathBuf,
    collab_root: PathBuf,
    task_dir: PathBuf,
    findings_path: PathBuf,
    sessions_path: PathBuf,
    store: FilesStore,
    slug: TaskSlug,
}

/// The derived, validated arguments (`-Status`/`-Useful` are lower-cased/trimmed here, as
/// the plugin does before it uses them).
#[derive(Debug)]
struct Validated {
    status: String,
    useful: String,
    rating: bool,
}

/// The plugin's argument validation (`codex-findings.ps1` lines 100-136), returning the
/// derived status/useful/rating or the exact `Stop-WithError` message.
fn validate(args: &FindingsArgs) -> Result<Validated, String> {
    let mut actions = 0;
    if args.list {
        actions += 1;
    }
    if args.stats {
        actions += 1;
    }
    if !args.id.is_empty() || !args.status.is_empty() {
        actions += 1;
    }
    let rating = args.rate.is_some() || !args.useful.is_empty();
    if rating {
        actions += 1;
    }
    if actions != 1 {
        return Err("choose exactly one of: -List [-All], -Stats, -Id <F..> -Status <status>, or -Rate <n> -Useful yes|partly|no.".to_string());
    }
    if args.all && !args.list {
        return Err("-All only goes with -List.".to_string());
    }
    if !args.evidence.is_empty() && args.id.is_empty() {
        return Err("-Evidence only goes with -Id/-Status.".to_string());
    }
    if !args.note.is_empty() && args.id.is_empty() && !rating {
        return Err("-Note only goes with -Id/-Status or -Rate.".to_string());
    }
    let mut useful = args.useful.clone();
    if rating {
        let rate = match args.rate {
            Some(r) => r,
            None => return Err("-Useful needs -Rate <consult n>.".to_string()),
        };
        if rate <= 0 {
            return Err(format!(
                "-Rate takes a consult number n greater than 0 (got {rate})."
            ));
        }
        useful = useful.trim().to_lowercase();
        if useful.is_empty() {
            return Err("-Rate needs -Useful yes|partly|no.".to_string());
        }
        if !["yes", "partly", "no"].contains(&useful.as_str()) {
            return Err(format!(
                "-Useful must be yes, partly or no (got '{useful}')."
            ));
        }
        if useful == "no" && args.note.trim().is_empty() {
            return Err(
                "-Useful no needs -Note (why the consultation was not useful).".to_string(),
            );
        }
    }
    if !is_slug(&args.task) {
        return Err("-Task must be a slug (letters, digits, dot, dash, underscore).".to_string());
    }
    let mut status = args.status.clone();
    if !args.id.is_empty() || !args.status.is_empty() {
        if args.id.is_empty() {
            return Err("-Status needs -Id <finding id>.".to_string());
        }
        if args.status.is_empty() {
            return Err(format!(
                "-Id needs -Status <{}>.",
                FINDING_STATUSES.join("|")
            ));
        }
        status = status.trim().to_lowercase();
        if !FINDING_STATUSES.contains(&status.as_str()) {
            return Err(format!(
                "-Status must be one of: {} (got '{status}').",
                FINDING_STATUSES.join(", ")
            ));
        }
        if status == "verified" && args.evidence.trim().is_empty() {
            return Err(
                "-Status verified requires -Evidence (what was run, or where the proof is)."
                    .to_string(),
            );
        }
        if status == "rejected" && args.note.trim().is_empty() {
            return Err(
                "-Status rejected requires -Note (why the finding is rejected).".to_string(),
            );
        }
    }
    Ok(Validated {
        status,
        useful,
        rating,
    })
}

fn run_inner(mut args: FindingsArgs) -> Result<i32, String> {
    let Validated {
        status,
        useful,
        rating,
    } = validate(&args)?;

    // ------------------------------------------------------------------- paths
    let slug = TaskSlug::new(args.task.clone()).map_err(|e| e.to_string())?;
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = resolve_repo_root(&cwd);
    let collab_root = resolve_collab_root(&repo_root, &args.collab_dir);
    let task_dir = collab_root.join(&args.task);
    let findings_path = task_dir.join("findings.json");
    let sessions_path = task_dir.join("sessions.json");

    args.status = status.clone();
    let store = FilesStore::new(collab_root.clone());
    let ctx = Ctx {
        args,
        status,
        useful,
        rating,
        repo_root,
        collab_root,
        task_dir,
        findings_path,
        sessions_path,
        store,
        slug,
    };

    if ctx.args.list {
        return mode_list(&ctx);
    }
    if ctx.args.stats {
        return mode_stats(&ctx);
    }
    if ctx.rating {
        return mode_rate(&ctx);
    }
    mode_status(&ctx)
}

// ----------------------------------------------------------------------------- -List

fn mode_list(ctx: &Ctx) -> Result<i32, String> {
    if !ctx.findings_path.exists() {
        println!(
            "{TOOL}: no findings recorded for task '{}' ({} does not exist).",
            ctx.args.task,
            ctx.findings_path.display()
        );
        write_pending_line(&ctx.task_dir);
        write_detached_lines(&ctx.task_dir, &ctx.args.task);
        return Ok(0);
    }
    let store =
        read_findings_value(&ctx.findings_path, &ctx.args.task)?.expect("exists checked above");
    let ledger = read_ledger(&ctx.sessions_path);

    let mut ledger_replies: std::collections::HashMap<i64, String> =
        std::collections::HashMap::new();
    for c in &ledger {
        if let Some(n) = pv_i64(c, "n") {
            ledger_replies.insert(n, pv_str(c, "reply"));
        }
    }
    let findings = findings_of(&store);
    let ledger_ns: HashSet<i64> = ledger_replies.keys().copied().collect();

    let mut shown = 0usize;
    let mut orphans = 0usize;
    let mut orphan_checks = 0usize;
    let mut by_status: Vec<(String, usize)> = FINDING_STATUSES
        .iter()
        .map(|s| (s.to_string(), 0))
        .collect();
    let mut other = 0usize;

    for f in &findings {
        let st = pv_str(f, "status");
        if let Some(entry) = by_status.iter_mut().find(|(k, _)| *k == st) {
            entry.1 += 1;
        } else {
            other += 1;
        }
        let n = finding_consult(f);
        let src_reply = pv(f, "source")
            .map(|s| pv_str(s, "reply"))
            .unwrap_or_default();
        let mut orphan = true;
        if let Some(n) = n {
            if let Some(ledger_reply) = ledger_replies.get(&n) {
                orphan = !src_reply.is_empty()
                    && !ledger_reply.is_empty()
                    && comparable_path(&src_reply) != comparable_path(ledger_reply);
            }
        }
        if orphan {
            orphans += 1;
        }
        let bad_checks = orphan_check_consults(f, &ledger_ns);
        orphan_checks += bad_checks.len();
        if !ctx.args.all && !OPEN_STATUSES.contains(&st.as_str()) && bad_checks.is_empty() {
            continue;
        }
        let mut line = format_finding_line(f, orphan);
        if !bad_checks.is_empty() {
            let joined = bad_checks
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            line += &format!("  [ORPHAN reviewer check: consult {joined} has no ledger entry]");
        }
        println!("{line}");
        shown += 1;
    }

    let mut parts: Vec<String> = by_status.iter().map(|(k, c)| format!("{k} {c}")).collect();
    if other > 0 {
        parts.push(format!("other {other}"));
    }
    let scope = if ctx.args.all {
        "all"
    } else {
        "open only, plus any with an orphan check; -All for every finding"
    };
    println!();
    println!(
        "{TOOL}: {shown} shown ({scope}); total {}: {}; orphans: {orphans} finding(s), {orphan_checks} reviewer check(s).",
        findings.len(),
        parts.join(", ")
    );
    write_pending_line(&ctx.task_dir);
    write_detached_lines(&ctx.task_dir, &ctx.args.task);
    Ok(0)
}

/// `Format-DetachedListLine` for every not-done detached run of the task (R12): the `-List` line
/// naming the come-back `-Status -Id <id8>` command.
fn write_detached_lines(task_dir: &Path, task: &str) {
    for line in crate::consult::detach::list_lines(task_dir, task, chrono::Utc::now()) {
        println!("{line}");
    }
}

// ----------------------------------------------------------------------------- -Stats

fn mode_stats(ctx: &Ctx) -> Result<i32, String> {
    let ledger = read_ledger(&ctx.sessions_path);
    let mut findings: Vec<Value> = Vec::new();
    let mut store: Option<Value> = None;
    if ctx.findings_path.exists() {
        let s = read_findings_value(&ctx.findings_path, &ctx.args.task)?.expect("exists checked");
        findings = findings_of(&s);
        store = Some(s);
    }
    if ledger.is_empty() {
        println!(
            "{TOOL}: no consultations recorded for task '{}' ({}).",
            ctx.args.task,
            ctx.sessions_path.display()
        );
        write_pending_line(&ctx.task_dir);
        return Ok(0);
    }

    // Per-consultation table.
    let row = |a: &str, b: &str, c: &str, d: &str, e: &str, f: &str, g: &str| {
        format!(
            "{}  {}  {}  {}  {}  {}  {}",
            pad(a, 4, false),
            pad(b, 13, true),
            pad(c, 6, true),
            pad(d, 7, false),
            pad(e, 11, false),
            pad(f, 7, true),
            g
        )
    };
    println!(
        "{}",
        row(
            "n",
            "purpose",
            "effort",
            "wall_s",
            "tokens(out)",
            "verdict",
            "findings: proposed/implemented/verified/rejected"
        )
    );
    let mut seen: HashSet<i64> = HashSet::new();
    for c in &ledger {
        let n = pv_i64(c, "n");
        let mine: Vec<&Value> = findings
            .iter()
            .filter(|f| n.is_some() && finding_consult(f) == n)
            .collect();
        if let Some(n) = n {
            seen.insert(n);
        }
        let count = |s: &str| mine.iter().filter(|f| pv_str(f, "status") == s).count();
        let (cp, ci, cv, cr) = (
            count("proposed"),
            count("implemented"),
            count("verified"),
            count("rejected"),
        );
        let purpose = {
            let p = pv_str(c, "purpose");
            if p.is_empty() {
                "-".to_string()
            } else {
                p
            }
        };
        let effort = pv_str_or(c, "effort", "-");
        let wall = {
            let w = pv_str(c, "wall_seconds");
            if pv(c, "wall_seconds").is_none() {
                "-".to_string()
            } else {
                w
            }
        };
        let mut tokens = "-".to_string();
        if let Some(u) = pv(c, "usage") {
            if let Some(ot) = pv(u, "output_tokens") {
                tokens = match ot {
                    Value::Number(n) => n.to_string(),
                    Value::String(s) => s.clone(),
                    _ => "-".to_string(),
                };
            }
        }
        let mut verdict = pv_str(c, "verdict");
        if verdict.is_empty() {
            verdict = "-".to_string();
            let outcome = {
                let bo = pv_str(c, "bridge_outcome");
                if bo.is_empty() {
                    pv_str(c, "outcome")
                } else {
                    bo
                }
            };
            if outcome.starts_with("failed") {
                verdict = "failed".to_string();
            } else if !pv_str(c, "validation_error").is_empty() {
                verdict = "invalid".to_string();
            }
        }
        let label = n.map(|n| n.to_string()).unwrap_or_else(|| "-".to_string());
        let mut findings_text = format!("{cp}/{ci}/{cv}/{cr}");
        if !mine.is_empty() {
            findings_text += &format!(" ({} total)", mine.len());
        }
        println!(
            "{}",
            row(
                &label,
                &purpose,
                &effort,
                &wall,
                &tokens,
                &verdict,
                &findings_text
            )
        );
    }

    // Per-reviewer board.
    let mut lineage_of: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    let mut lineage_order: Vec<String> = Vec::new();
    for c in &ledger {
        let n = match pv_i64(c, "n") {
            Some(n) => n,
            None => continue,
        };
        let label = match pv(c, "reviewer") {
            Some(rev) => format_reviewer_lineage(
                &pv_str(rev, "provider"),
                &pv_str(rev, "model"),
                &pv_str(rev, "engine"),
            ),
            None => "unknown provenance".to_string(),
        };
        lineage_of.insert(n, label.clone());
        if label != "unknown provenance" && !lineage_order.contains(&label) {
            lineage_order.push(label);
        }
    }
    let mut board: std::collections::HashMap<String, Vec<Value>> = std::collections::HashMap::new();
    for f in &findings {
        let label = finding_consult(f)
            .and_then(|fn_| lineage_of.get(&fn_).cloned())
            .unwrap_or_else(|| "unknown provenance".to_string());
        board.entry(label).or_default().push(f.clone());
    }
    if board.contains_key("unknown provenance") {
        lineage_order.push("unknown provenance".to_string());
    }
    if !lineage_order.is_empty() {
        let w_name = lineage_order
            .iter()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0)
            .max(8);
        let brow = |label: &str,
                    raised: &str,
                    verified: &str,
                    implemented: &str,
                    proposed: &str,
                    rejected: &str,
                    wontfix: &str,
                    superseded: &str,
                    yes: &str,
                    partly: &str,
                    no: &str| {
            format!(
                "{}  {}  {}  {}  {}  {}  {}  {}  {}  {}  {}",
                pad(label, w_name, true),
                pad(raised, 6, false),
                pad(verified, 8, false),
                pad(implemented, 11, false),
                pad(proposed, 8, false),
                pad(rejected, 8, false),
                pad(wontfix, 7, false),
                pad(superseded, 10, false),
                pad(yes, 3, false),
                pad(partly, 6, false),
                pad(no, 2, false)
            )
        };
        // ratings marks per lineage.
        let mut marks: std::collections::HashMap<String, (i64, i64, i64)> =
            std::collections::HashMap::new();
        if let Some(s) = &store {
            if let Some(Value::Array(ratings)) = pv(s, "ratings") {
                for mk in ratings {
                    if mk.is_null() {
                        continue;
                    }
                    let ml = {
                        let l = pv_str(mk, "lineage");
                        if pv(mk, "lineage").is_none() {
                            "unknown provenance".to_string()
                        } else {
                            l
                        }
                    };
                    let e = marks.entry(ml).or_insert((0, 0, 0));
                    match pv_str(mk, "useful").as_str() {
                        "yes" => e.0 += 1,
                        "partly" => e.1 += 1,
                        "no" => e.2 += 1,
                        _ => {}
                    }
                }
            }
        }
        println!();
        println!("reviewers (findings by the lineage of the consultation that raised them; ratings = the judge's marks, -Rate):");
        println!(
            "{}",
            brow(
                "reviewer",
                "raised",
                "verified",
                "implemented",
                "proposed",
                "rejected",
                "wontfix",
                "superseded",
                "yes",
                "partly",
                "no"
            )
        );
        for label in &lineage_order {
            let mine_b = board.get(label).cloned().unwrap_or_default();
            let cnt = |s: &str| mine_b.iter().filter(|f| pv_str(f, "status") == s).count();
            let mk = marks.get(label).copied().unwrap_or((0, 0, 0));
            println!(
                "{}",
                brow(
                    label,
                    &mine_b.len().to_string(),
                    &cnt("verified").to_string(),
                    &cnt("implemented").to_string(),
                    &cnt("proposed").to_string(),
                    &cnt("rejected").to_string(),
                    &cnt("wontfix").to_string(),
                    &cnt("superseded").to_string(),
                    &mk.0.to_string(),
                    &mk.1.to_string(),
                    &mk.2.to_string()
                )
            );
        }
    }

    let orphan_count = findings
        .iter()
        .filter(|f| {
            let n = finding_consult(f);
            n.is_none() || !seen.contains(&n.unwrap())
        })
        .count();
    let mut orphan_check_count = 0usize;
    for f in &findings {
        orphan_check_count += orphan_check_consults(f, &seen).len();
    }
    if orphan_count > 0 || orphan_check_count > 0 {
        println!();
        println!(
            "{TOOL}: {orphan_count} finding(s) and {orphan_check_count} reviewer check(s) belong to no ledger entry (ORPHAN; see -List -All)."
        );
    }
    write_pending_line(&ctx.task_dir);
    Ok(0)
}

// ----------------------------------------------------------------------------- -Rate

fn mode_rate(ctx: &Ctx) -> Result<i32, String> {
    if !ctx.sessions_path.is_file() {
        return Err(format!(
            "no consultations recorded for task '{}' ({} does not exist); -Rate takes the n of a ledger entry.",
            ctx.args.task,
            ctx.sessions_path.display()
        ));
    }
    let rate = ctx.args.rate.expect("rating implies rate");
    let _guard = TaskLockGuard::acquire(ctx)?;

    // Refused while any recovery record of the task is active.
    let pending = pending::read_task_pending_records(&ctx.task_dir);
    if let Some(err) = pending.error {
        return Err(err);
    }
    if let Some(msg) = pending.active_message {
        return Err(msg);
    }

    let write_lock = ctx
        .store
        .take_write_lock(&ctx.slug)
        .map_err(|e| write_lock_err(ctx, e))?;

    // Re-read the ledger under the lock; find entry n == Rate.
    let sessions = ctx
        .store
        .read_sessions(&ctx.slug)
        .map_err(|e| e.to_string())?;
    let entry = sessions
        .as_ref()
        .and_then(|s| s.codex.consults.iter().find(|e| e.n == rate));
    let entry = match entry {
        Some(e) => e,
        None => {
            return Err(format!(
                "no consultation n={rate} in {}; -Rate takes the n of a ledger entry (see -Stats).",
                ctx.sessions_path.display()
            ))
        }
    };

    let mut provider = String::new();
    let mut model = entry.model.clone();
    let mut engine = "codex".to_string();
    let mut lineage = "unknown provenance".to_string();
    if !entry.reviewer.provider.is_empty()
        || !entry.reviewer.model.is_empty()
        || !entry.reviewer.engine.is_empty()
    {
        provider = entry.reviewer.provider.clone();
        model = entry.reviewer.model.clone();
        engine = if entry.reviewer.engine.is_empty() {
            "codex".to_string()
        } else {
            entry.reviewer.engine.clone()
        };
        lineage = format_reviewer_lineage(&provider, &model, &engine);
    }
    // (wave 26, D2) the mark is keyed by the consultation's id; a non-empty id must be a guid.
    let consult_id = entry.consult_id.clone();
    if !consult_id.is_empty() && !is_uuid(&consult_id) {
        return Err(format!(
            "consultation n={rate} in {} has a malformed consult_id '{consult_id}'; nothing was changed.",
            ctx.sessions_path.display()
        ));
    }
    // (wave 26, D2) the mark copies everything the routing score needs from the ledger entry
    // (no later join by n): the engine, the topics and the consultation's own time.
    let topics: Vec<Value> = entry
        .topics
        .as_ref()
        .map(|ts| {
            ts.iter()
                .filter(|t| !t.is_null() && !matches!(t, Value::String(s) if s.is_empty()))
                .map(|t| match t {
                    Value::String(s) => Value::String(s.clone()),
                    Value::Bool(b) => Value::String((if *b { "True" } else { "False" }).into()),
                    Value::Number(n) => Value::String(n.to_string()),
                    other => Value::String(other.to_string()),
                })
                .collect()
        })
        .unwrap_or_default();
    let mark = c3_core::findings::Rating {
        n: rate,
        consult_id: consult_id.clone(),
        lineage: lineage.clone(),
        provider,
        model,
        engine: Some(engine),
        purpose: entry.purpose.clone(),
        topics: Some(topics),
        consult_when: Some(Some(entry.when.clone())),
        useful: ctx.useful.clone(),
        note: ctx.args.note.trim().to_string(),
        when: iso_timestamp(),
        extra: Default::default(),
    };

    // Re-read findings.json under the lock; replace-or-append the mark. (wave 26, D2) the same
    // consultation is matched by consult_id (case-insensitive) when both carry one, else by n
    // (a mark recorded before wave 26 without a consult_id). The first match takes the new
    // mark's position; any further duplicate of the same key is dropped.
    let mut findings = read_findings_typed(ctx)?;
    let mut replaced = false;
    let mut previous = String::new();
    {
        let ratings = findings.ratings.get_or_insert_with(Vec::new);
        let mut kept: Vec<c3_core::findings::Rating> = Vec::with_capacity(ratings.len() + 1);
        for old in ratings.drain(..) {
            let same = if !consult_id.is_empty() && !old.consult_id.is_empty() {
                old.consult_id.eq_ignore_ascii_case(&consult_id)
            } else {
                old.n == rate
            };
            if same {
                if !replaced {
                    previous = old.useful.clone();
                    kept.push(mark.clone());
                    replaced = true;
                }
                continue;
            }
            kept.push(old);
        }
        if !replaced {
            kept.push(mark.clone());
        }
        *ratings = kept;
    }

    write_text_atomic(
        &ctx.findings_path,
        &findings.to_bytes().map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    drop(write_lock);

    // (M9) the mark reaches the hub as its own allowlisted event - classes only (lineage,
    // purpose, vocabulary topics, the mark, the age in days), spooled and sent with the next
    // flush. It never blocks and never fails the rating; the environment switch is honoured
    // inside.
    {
        let age_days = chrono::DateTime::parse_from_rfc3339(&entry.when)
            .map(|w| {
                (chrono::Utc::now() - w.with_timezone(&chrono::Utc))
                    .num_days()
                    .max(0)
            })
            .unwrap_or(0);
        let raw_topics: Vec<String> = mark
            .topics
            .as_ref()
            .map(|ts| {
                ts.iter()
                    .filter_map(|t| t.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let _ = crate::telemetry::record_rating(entry, &ctx.useful, age_days, &raw_topics);
    }

    let purpose_text = if mark.purpose.is_empty() {
        "no purpose".to_string()
    } else {
        mark.purpose.clone()
    };
    if replaced {
        println!(
            "{TOOL}: consult n={rate} ({lineage}, {purpose_text}) re-rated {} (was {previous}).",
            ctx.useful
        );
    } else {
        println!(
            "{TOOL}: consult n={rate} ({lineage}, {purpose_text}) rated {}.",
            ctx.useful
        );
    }
    Ok(0)
}

// ----------------------------------------------------------------------------- -Id -Status

fn mode_status(ctx: &Ctx) -> Result<i32, String> {
    if !ctx.findings_path.exists() {
        return Err(format!(
            "no findings recorded for task '{}' ({} does not exist).",
            ctx.args.task,
            ctx.findings_path.display()
        ));
    }
    let _guard = TaskLockGuard::acquire(ctx)?;

    let pending = pending::read_task_pending_records(&ctx.task_dir);
    if let Some(err) = pending.error {
        return Err(err);
    }
    if let Some(msg) = pending.active_message {
        return Err(msg);
    }

    let write_lock = ctx
        .store
        .take_write_lock(&ctx.slug)
        .map_err(|e| write_lock_err(ctx, e))?;

    let mut findings = read_findings_typed(ctx)?;
    let id = ctx.args.id.trim().to_string();
    let idx = findings.findings.iter().position(|f| f.id == id);
    let idx = match idx {
        Some(i) => i,
        None => {
            return Err(format!(
                "unknown finding id '{}' in {} (codex-findings.ps1 -Task {} -List -All shows every id).",
                ctx.args.id,
                ctx.findings_path.display(),
                ctx.args.task
            ))
        }
    };
    let current = findings.findings[idx].status().as_str().to_string();
    if ctx.status == "proposed" && current != "proposed" && ctx.args.note.trim().is_empty() {
        return Err(format!(
            "reopening {} ({current} -> proposed) requires -Note (why it is open again).",
            findings.findings[idx].id
        ));
    }

    let rev = revision::revision_info(&ctx.repo_root, &ctx.collab_root);
    let to = FindingStatus::from_token(&ctx.status);
    let target: &mut Finding = &mut findings.findings[idx];
    target
        .set_status(
            to,
            ctx.args.note.trim(),
            ctx.args.evidence.trim(),
            &iso_timestamp(),
            "coordinator",
        )
        .map_err(|e| format!("{} {e}", target.id))?;
    if let Some(last) = target.history.last_mut() {
        last.base_commit = rev.base_commit.clone();
        last.tree_sha256 = rev.tree_sha256.clone();
    }
    let history_len = target.history.len();
    let id_display = target.id.clone();

    write_text_atomic(
        &ctx.findings_path,
        &findings.to_bytes().map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    drop(write_lock);

    let sha = if rev.tree_sha256.is_empty() {
        "none (no git)".to_string()
    } else {
        rev.tree_sha256.chars().take(12).collect()
    };
    println!(
        "{TOOL}: {id_display} {current} -> {} (history: {history_len} records; tree sha256 {sha}).",
        ctx.status
    );
    // The line for the changed finding, as a Value (orphan not computed for a status change).
    let value = serde_json::to_value(&findings.findings[idx]).unwrap_or(Value::Null);
    println!("{}", format_finding_line(&value, false));
    Ok(0)
}

// ----------------------------------------------------------------------------- helpers

/// The task (ownership) lock, released when dropped.
struct TaskLockGuard {
    _lock: c3_core::store::TaskLock,
}

impl TaskLockGuard {
    fn acquire(ctx: &Ctx) -> Result<TaskLockGuard, String> {
        let record = LockRecord::now(&ctx.slug, None);
        match ctx.store.take_task_lock(&ctx.slug, &record) {
            Ok(lock) => Ok(TaskLockGuard { _lock: lock }),
            Err(_) => {
                let path = ctx.task_dir.join(c3_core::store::OWNERSHIP_LOCK);
                Err(task_lock_refusal(&path, &ctx.args.task))
            }
        }
    }
}

fn write_lock_err(ctx: &Ctx, e: std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::TimedOut {
        let path = ctx.task_dir.join(c3_core::store::WRITE_LOCK);
        format!(
            "{}; nothing was changed.",
            write_lock_refusal(&path, &ctx.args.task)
        )
    } else {
        format!("could not take the write lock: {e}; nothing was changed.")
    }
}

/// Read `findings.json` into the typed model for a byte-identical rewrite, applying the
/// plugin's refusal checks first. A missing file is a fresh store.
fn read_findings_typed(ctx: &Ctx) -> Result<FindingsFile, String> {
    match read_findings_value(&ctx.findings_path, &ctx.args.task)? {
        None => Ok(FindingsFile {
            task_id: ctx.args.task.clone(),
            ..Default::default()
        }),
        Some(value) => serde_json::from_value(value).map_err(|e| {
            format!(
                "refusing to use '{}': it does not parse into the findings model: {}.",
                ctx.findings_path.display(),
                c3_core::one_line(&e.to_string())
            )
        }),
    }
}

/// `Get-IsoTimestamp`: local time as `yyyy-MM-ddTHH:mm:sszzz`.
fn iso_timestamp() -> String {
    let now = chrono::Local::now();
    now.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base_args() -> FindingsArgs {
        FindingsArgs {
            task: "demo".into(),
            collab_dir: ".collab".into(),
            ..Default::default()
        }
    }

    #[test]
    fn validate_requires_exactly_one_mode() {
        let a = base_args(); // no mode
        assert_eq!(
            validate(&a).unwrap_err(),
            "choose exactly one of: -List [-All], -Stats, -Id <F..> -Status <status>, or -Rate <n> -Useful yes|partly|no."
        );
    }

    #[test]
    fn validate_status_transition_refusals() {
        // verified requires evidence
        let mut a = base_args();
        a.id = "F02-1".into();
        a.status = "verified".into();
        assert_eq!(
            validate(&a).unwrap_err(),
            "-Status verified requires -Evidence (what was run, or where the proof is)."
        );
        // rejected requires note
        let mut a = base_args();
        a.id = "F02-1".into();
        a.status = "rejected".into();
        assert_eq!(
            validate(&a).unwrap_err(),
            "-Status rejected requires -Note (why the finding is rejected)."
        );
        // unknown status token
        let mut a = base_args();
        a.id = "F02-1".into();
        a.status = "archived".into();
        assert_eq!(
            validate(&a).unwrap_err(),
            "-Status must be one of: proposed, implemented, verified, rejected, wontfix, superseded (got 'archived')."
        );
        // -Id without -Status
        let mut a = base_args();
        a.id = "F02-1".into();
        assert_eq!(
            validate(&a).unwrap_err(),
            "-Id needs -Status <proposed|implemented|verified|rejected|wontfix|superseded>."
        );
        // -Status without -Id
        let mut a = base_args();
        a.status = "verified".into();
        assert_eq!(validate(&a).unwrap_err(), "-Status needs -Id <finding id>.");
        // valid verified passes and lower-cases the status
        let mut a = base_args();
        a.id = "F02-1".into();
        a.status = "VERIFIED".into();
        a.evidence = "ran cargo test".into();
        assert_eq!(validate(&a).unwrap().status, "verified");
    }

    #[test]
    fn validate_rating_refusals() {
        // -Useful no needs -Note
        let mut a = base_args();
        a.rate = Some(3);
        a.useful = "no".into();
        assert_eq!(
            validate(&a).unwrap_err(),
            "-Useful no needs -Note (why the consultation was not useful)."
        );
        // bad useful value
        let mut a = base_args();
        a.rate = Some(3);
        a.useful = "maybe".into();
        assert_eq!(
            validate(&a).unwrap_err(),
            "-Useful must be yes, partly or no (got 'maybe')."
        );
        // -Useful without -Rate
        let mut a = base_args();
        a.useful = "yes".into();
        assert_eq!(
            validate(&a).unwrap_err(),
            "-Useful needs -Rate <consult n>."
        );
        // rate <= 0
        let mut a = base_args();
        a.rate = Some(0);
        a.useful = "yes".into();
        assert_eq!(
            validate(&a).unwrap_err(),
            "-Rate takes a consult number n greater than 0 (got 0)."
        );
        // valid
        let mut a = base_args();
        a.rate = Some(2);
        a.useful = "Partly".into();
        let v = validate(&a).unwrap();
        assert!(v.rating);
        assert_eq!(v.useful, "partly");
    }

    #[test]
    fn format_finding_line_layout_and_orphan() {
        let f = json!({
            "id": "F02-1",
            "status": "proposed",
            "severity": "major",
            "locations": [{"path": "src/x.rs", "line": 12}, {"path": "src/y.rs"}],
            "claim": "the   thing is broken"
        });
        let line = format_finding_line(&f, false);
        assert_eq!(
            line,
            "F02-1     proposed    major   src/x.rs:12 (+1)             the thing is broken"
        );
        let orphan = format_finding_line(&f, true);
        assert!(orphan.ends_with("  [ORPHAN: no ledger entry for its consult]"));
    }

    #[test]
    fn orphan_check_and_comparable_path() {
        let mut ns = HashSet::new();
        ns.insert(1i64);
        let f = json!({"reviewer_checks": [{"consult": 1}, {"consult": 2}, {"consult": 2}]});
        assert_eq!(orphan_check_consults(&f, &ns), vec![2]);
        assert_eq!(comparable_path("A\\B/C.md"), "a/b/c.md");
    }

    #[test]
    fn finding_consult_reads_source() {
        let f = json!({"source": {"consult": 4}});
        assert_eq!(finding_consult(&f), Some(4));
        let f2 = json!({"id": "x"});
        assert_eq!(finding_consult(&f2), None);
    }
}
