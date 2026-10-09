//! The consultation orchestrator for the codex engine (milestone 2c).
//!
//! It ties the self-contained pieces (`args`, `prompt`, `ingest`, `render`, `revision`,
//! `summary`) to the identity/launcher layer reused from [`crate::providers`] and the
//! [`crate::engines::codex`] adapter. It resolves the reviewer identity, the effort plan and
//! the schema transport, assembles the prompt, and then either prints the `--dry-run` block
//! (writing nothing) or runs one codex turn, ingests the reply, renders the handoff and
//! commits the ledger entry and findings through [`c3_core::store`].
//!
//! ## Scope (M2c, codex only)
//!
//! Implemented: the full argument surface and refusals, identity/effort/transport resolution,
//! the dry-run block, the open-findings prompt snapshot, a live codex run (structured, prose
//! and failure ingestion), the handoff render and the store commit, and the summary block.
//!
//! Deferred (documented in `docs/port/m2-status.md`): the format-repair turn, the timeout
//! continuation and `.partial.md` salvage, the pending/lock recovery of an interrupted run,
//! the peak-window evaluation, the endpoint-health 24h block, the prior-finding lifecycle
//! ingestion (retained blockers / verdict-vs-blocker), and the agy/muse engines (M2d).

use std::path::{Path, PathBuf};
use std::time::Duration;

use c3_core::effort::{caps, Models};
use c3_core::engine::{
    AttemptOutcome, ConsultationId, Engine, EngineKind, Mode, Request, StructuredReply, TurnKind,
    TurnRequest,
};
use c3_core::handoff::{Author, HandoffHeader, OptionalRecords, TokenReport};
use c3_core::ledger::{FindingCounts, LedgerEntry, RangeRecord, Reviewer, Usage};
use c3_core::lineage::{resolve_reviewer_identity, ReviewerIdentity};
use c3_core::store::{
    CommitRequest, EvidenceStore, FilesStore, FindingsDelta, LockRecord, PendingRecord, PendingRef,
    PendingState, RecoveryDisposition,
};
use c3_core::task_slug::TaskSlug;
use c3_core::verdict::{verdict_pre_credential, verdict_with_credential};

use crate::engines::codex::{CodexEngine, TurnFiles};
use crate::providers;
use crate::telemetry;

use super::args::{self, Options, Resolved};
use super::ingest;
use super::prompt::{self, OpenFinding, PromptInputs};
use super::render;
use super::revision::{self, RevisionInfo};
use super::summary;

const TOOL: &str = "codex-consult";

/// `-Range` size-warning thresholds (`$rangeWarnLines` / `$rangeWarnTimeout`).
const RANGE_WARN_LINES: i64 = 1500;
const RANGE_WARN_TIMEOUT: i64 = 2400;

/// Print a refusal (`Stop-WithError`) and return the usage exit code (1). A refusal after a
/// detached background's lock is remembered as that run's final status (D3).
fn refuse(msg: &str) -> i32 {
    let line = format!("{TOOL}: {msg}");
    eprintln!("{line}");
    super::detach::note_line(&line);
    1
}

/// (wave 27) Resolve the ledger `coordinator` record from the environment (`Get-CoordinatorHost` +
/// `CODEX_CONSULT_COORDINATOR`) - `Resolve-CoordinatorIdentity` with the reviewer roster and the
/// Codex config's defaults (F09-6: the one resolver a rating's actor shares). An unparseable value
/// refuses (`Err`, exit 1) before anything is planned. `roster` is `None` when there is no
/// reviewer roster file (for `#<n>`).
pub(crate) fn resolve_coordinator(
    roster: Option<&[c3_core::roster::RosterEntry]>,
) -> Result<c3_core::ledger::Coordinator, (String, i32)> {
    let value = std::env::var("CODEX_CONSULT_COORDINATOR").unwrap_or_default();
    let defaults = c3_core::host::codex_config_defaults(&providers::read_codex_config(
        &providers::get_codex_config_path(),
    ));
    c3_core::host::resolve_coordinator_identity(
        &value,
        roster,
        &defaults,
        c3_core::host::coordinator_host(),
    )
    .map_err(|refusal| (refusal, 1))
}

/// (wave 28b, D15) The `-c` items a codex reviewer with a roster `context_tokens` n gets on every
/// turn - `model_context_window=<n>` and `model_auto_compact_token_limit=<floor(0.8 n)>`, each only
/// when no operator item (`-CodexConfig` / the entry's `codex_config`) already sets that key
/// (`^<key>\s*=`, any case, as the plugin's `-match`) - and the ledger `context_window` record
/// `{tokens, auto_compact_limit, items}`; `(vec![], None)` without a window or for another engine.
pub(crate) fn context_window_config(
    context_tokens: i64,
    is_codex: bool,
    extra_config: &[String],
) -> (Vec<String>, Option<serde_json::Value>) {
    if context_tokens <= 0 || !is_codex {
        return (Vec::new(), None);
    }
    let compact_at = (0.8 * context_tokens as f64).floor() as i64;
    let mut items: Vec<String> = Vec::new();
    for (key, value) in [
        ("model_context_window", context_tokens),
        ("model_auto_compact_token_limit", compact_at),
    ] {
        // F09-1: split at the first `=` and compare the trimmed key - never slice the item at a
        // byte offset taken from another string (a Unicode value made that a char-boundary panic).
        let set_by_operator = extra_config.iter().any(|ec| {
            ec.trim()
                .split_once('=')
                .is_some_and(|(k, _)| k.trim_end().eq_ignore_ascii_case(key))
        });
        if !set_by_operator {
            items.push(format!("{key}={value}"));
        }
    }
    let record = serde_json::json!({
        "tokens": context_tokens,
        "auto_compact_limit": compact_at,
        "items": items.clone(),
    });
    (items, Some(record))
}

/// The codex `-c` items of a turn: the operator's (`-CodexConfig` / the roster's `codex_config`)
/// followed by the context-window items (`$extraConfig` then `$contextConfig`, as the plugin's argv).
fn with_context_config(extra_config: &[String], context_config: &[String]) -> Vec<String> {
    let mut v = extra_config.to_vec();
    v.extend(context_config.iter().cloned());
    v
}

/// (wave 28c, D11) The compactions an engine REPORTED in the event streams of a run's turns
/// (`Get-CompactionCount`): a line whose `type` is `context_compacted`, `compacted` or
/// `thread.compacted`, a `system` line with subtype `compact_boundary`, a `msg` of one of those
/// types, or an `item.completed` whose item (`type`, else `item_type`) is `context_compaction`,
/// `contextCompaction` or `compaction`. Missing files and lines that are no JSON object count
/// nothing.
pub(crate) fn compaction_count(paths: &[&Path]) -> u64 {
    const EVENT_TYPES: [&str; 3] = ["context_compacted", "compacted", "thread.compacted"];
    const ITEM_TYPES: [&str; 3] = ["context_compaction", "contextCompaction", "compaction"];
    let mut n = 0u64;
    for p in paths {
        let Ok(text) = std::fs::read_to_string(p) else {
            continue;
        };
        for line in text.lines() {
            if !line.to_ascii_lowercase().contains("ompact") {
                continue;
            }
            let t = line.trim();
            if !t.starts_with('{') {
                continue;
            }
            let Ok(o) = serde_json::from_str::<serde_json::Value>(t) else {
                continue;
            };
            if !o.is_object() {
                continue;
            }
            let ty = o.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if EVENT_TYPES.contains(&ty) {
                n += 1;
                continue;
            }
            if ty == "system"
                && o.get("subtype").and_then(|v| v.as_str()) == Some("compact_boundary")
            {
                n += 1;
                continue;
            }
            if let Some(msg_ty) = o
                .get("msg")
                .and_then(|m| m.get("type"))
                .and_then(|v| v.as_str())
            {
                if EVENT_TYPES.contains(&msg_ty) {
                    n += 1;
                    continue;
                }
            }
            if ty == "item.completed" {
                let item = o.get("item");
                let it = item
                    .and_then(|i| i.get("type"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .or_else(|| {
                        item.and_then(|i| i.get("item_type"))
                            .and_then(|v| v.as_str())
                    })
                    .unwrap_or("");
                if ITEM_TYPES.contains(&it) {
                    n += 1;
                }
            }
        }
    }
    n
}

/// The two secondary-turn mechanisms' results (timeout continuation + format repair), gathered
/// so [`render_handoff`], [`build_entry`] and the summary render at their exact points.
#[derive(Default)]
struct Secondary {
    // --- timeout continuation ---
    timeout_continue: Option<c3_core::ledger::TimeoutContinue>,
    continued: bool,
    continue_thread: String,
    continue_wall: f64,
    continue_usage: Option<Usage>,
    /// The continuation turn's event stream (`handoffs/...continue.events.jsonl`), a further turn.
    continue_events_rel: Option<String>,
    continue_rejected: bool,
    continue_rejected_text: String,
    continue_rejected_why: String,
    /// A continuation turn was actually launched (not the "not attempted" skip).
    continue_ran: bool,
    /// The continuation turn was itself killed on its timeout.
    continue_killed: bool,
    /// (wave 27c, D2) the operator stopped the TIMEOUT CONTINUATION (-Kick): the timeout outcome
    /// and its salvage stay (never rewritten to an operator stop), and a warning is emitted.
    continue_kicked: bool,
    /// The format-repair turn was killed on its timeout.
    repair_killed: bool,
    /// (wave 26c, D1) the operator stopped the FORMAT REPAIR (-Kick): the first reply stands, not
    /// converted; no operator class. A warning is emitted, the run stays a usable reply.
    repair_kicked: bool,
    /// The `-ContinueSec` used, for the partial-footer `killed at ...` line.
    repair_timeout: i64,
    // --- format repair ---
    format_retry: Option<c3_core::ledger::FormatRetry>,
    repaired_ok: bool,
    original_prose: String,
    original_rel: String,
    repair_wall: f64,
    repair_reason: String,
    drift_notes: Vec<String>,
    /// The `format repair: ...` console line (empty when no repair ran).
    repair_console: String,
    // --- salvaged partial reply ---
    partial_needed: bool,
    partial_rel: String,
    partial_footer: String,
    /// The salvaged partial body (`Format-PartialBody`), for the `.partial.md` file.
    partial_body: String,
    /// The `resume     :` summary command (the plugin's `$resumeCommand`).
    resume_command: String,
    // --- engine (agy/muse) ---
    /// The agy denial-retry record (`entry.denial_retry`); `None` when no retry ran.
    denial_retry: Option<c3_core::ledger::DenialRetry>,
    /// The `denial retry: succeeded|failed in N s` summary line (empty when none).
    denial_console: String,
    /// The engine turns started (1 + a denial retry + a continuation + a format repair).
    engine_turns: i64,
    /// The MSP schema version of a muse stream (`None` for agy/codex).
    msp_version: Option<i64>,
    /// The read-only tree-check problem (agy failure); empty when clean/warned.
    tree_problem: String,
    /// The tree-check outcome (`clean`/`warned`/`failed`) for the `tree_check{}` ledger record.
    tree_check_outcome: String,
    /// The changed files the tree check named (for `tree_check{}`).
    tree_check_files: Vec<String>,
    /// The engine turn warnings (denial notices, engine stderr warnings) — the ledger `warnings`
    /// of an engine run and the `warning    : ...` summary lines.
    engine_warnings: Vec<String>,
    /// The repair turn's kept event-stream (engine repair keeps a real file, unlike codex).
    repair_events_rel: Option<String>,
    /// (wave 28c, D11) the compactions a codex repair turn's TEMP event stream reported, counted
    /// before that file is removed.
    repair_compactions: u64,
    /// (wave 28c, D11) the ledger `compactions` (a number, `"unknown"`, or `None` = null).
    compactions: Option<serde_json::Value>,
    // --- the secondary turns' kills (wave 27c D16; 28e E1, E18, E23) ---
    /// Every secondary turn's kill check with its survivors (the ledger's `kill_confirmed`).
    kill_checks: Vec<(c3_core::engine::KillCheck, Vec<u32>)>,
    /// `Add-KillCheck`'s warnings of the secondary turns (`kill not confirmed (<turn>): ...`).
    kill_warnings: Vec<String>,
    /// What a secondary turn's kill left that keeps the recovery record (the last one wins).
    kept_kill: Option<KeptKill>,
    /// (wave 3c, F23-3) Where a secondary turn's kill writes the record it keeps, AT the kill
    /// (`None`: nothing is written - the unit tests).
    kill_site: Option<KillSite>,
    /// (wave 3c, F23-3) What the main turn's kill kept (its unknown tree's why stays in a secondary
    /// kill's record).
    main_kept: Option<KeptKill>,
}

/// (wave 28e, E1 / E18 / E23) What a tree kill left behind that keeps the recovery record (state
/// `survivors`): its survivors, the descendants it could not verify (`unverified[]` `{pid, why}`),
/// and - a kill that was not confirmed and names no pid - its why (`kill_unconfirmed`).
#[derive(Debug, Clone, Default)]
struct KeptKill {
    survivors: Vec<u32>,
    unverified: Vec<serde_json::Value>,
    kill_unconfirmed: String,
    /// (wave 3c, F23-3) The survivors' `{pid, start_time, name}` entries, read AT the kill
    /// (`New-SurvivorEntries` where the plugin calls it); `None` until the kill site reads them.
    entries: Option<Vec<serde_json::Value>>,
}

fn join_pids(pids: &[u32]) -> String {
    pids.iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// (wave 28d, D5) `Get-KillUnverifiedText`: the unverified group next to the survivors - `"; <why>;
/// pid <n> may still run"`, `""` when the kill left no descendant it could not verify.
fn kill_unverified_text(c: &c3_core::engine::KillCheck) -> String {
    if c.unverified.is_empty() {
        return String::new();
    }
    format!(
        "; {}; pid {} may still run",
        c.why,
        join_pids(&c.unverified)
    )
}

/// `Get-KillMayRunPids`: the unverified pids, else the root.
fn kill_may_run_pids(c: &c3_core::engine::KillCheck) -> String {
    if c.unverified.is_empty() {
        c.root_pid.to_string()
    } else {
        join_pids(&c.unverified)
    }
}

/// (wave 27c, D16) `Format-KillText`: `(process tree killed)` only when confirmed; else `(kill not
/// confirmed: <why>; pid <n> may still run)`; with survivors `(process tree killed; <n> processes
/// survived: pid <n>[; <why>; pid <n> may still run])`.
fn format_kill_text(c: &c3_core::engine::KillCheck, survivors: &[u32]) -> String {
    if !survivors.is_empty() {
        return format!(
            "(process tree killed; {} processes survived: pid {}{})",
            survivors.len(),
            join_pids(survivors),
            kill_unverified_text(c)
        );
    }
    if c.confirmed {
        return "(process tree killed)".to_string();
    }
    format!(
        "(kill not confirmed: {}; pid {} may still run)",
        c.why,
        kill_may_run_pids(c)
    )
}

/// (wave 28e, E23 / F30-1) `Get-KillUnconfirmedWhy`: a kill that was NOT confirmed and names no
/// pid - neither a survivor nor a descendant it could not verify - its why (`the kill was not
/// confirmed` when it has none); `""` for any other kill.
fn kill_unconfirmed_why(c: &c3_core::engine::KillCheck, survivors: &[u32]) -> String {
    if c.confirmed || !survivors.is_empty() || !c.unverified.is_empty() {
        return String::new();
    }
    if c.why.is_empty() {
        "the kill was not confirmed".to_string()
    } else {
        c.why.clone()
    }
}

/// `Add-KillCheck`'s warning for one kill (`turn`: `main turn`, `timeout continuation`, `format
/// repair`): a kill not confirmed with no survivor names the pid(s) that may still run; one with
/// survivors AND unverified descendants names both groups.
fn kill_check_warning(
    c: &c3_core::engine::KillCheck,
    survivors: &[u32],
    turn: &str,
) -> Option<String> {
    if c.confirmed {
        return None;
    }
    if survivors.is_empty() {
        return Some(format!(
            "kill not confirmed ({turn}): {}; pid {} may still run - check it, and stop it by hand if it does",
            c.why,
            kill_may_run_pids(c)
        ));
    }
    if !c.unverified.is_empty() {
        return Some(format!(
            "kill not confirmed ({turn}): {} processes survived: pid {}{} - check them, and stop them by hand if they do",
            survivors.len(),
            join_pids(survivors),
            kill_unverified_text(c)
        ));
    }
    None
}

/// (wave 28e, E1 / F54-1) `New-UnverifiedEntries`: `{pid, why}` per descendant the kill could not
/// verify (its why is the kill check's).
fn unverified_entries(c: &c3_core::engine::KillCheck) -> Vec<serde_json::Value> {
    c.unverified
        .iter()
        .map(|p| serde_json::json!({ "pid": p, "why": c.why }))
        .collect()
}

/// (wave 28e, E18 / E23) Whether a kill keeps the recovery record: survivors OR descendants it
/// could not verify OR a kill not confirmed with neither (an unknown tree).
fn kept_kill(c: &c3_core::engine::KillCheck, survivors: &[u32]) -> Option<KeptKill> {
    let unconfirmed = kill_unconfirmed_why(c, survivors);
    if survivors.is_empty() && c.unverified.is_empty() && unconfirmed.is_empty() {
        return None;
    }
    Some(KeptKill {
        survivors: survivors.to_vec(),
        unverified: unverified_entries(c),
        kill_unconfirmed: unconfirmed,
        entries: None,
    })
}

/// (wave 3c, F23-3) What the run's kills keep together: a secondary turn's lists replace the main
/// turn's; the main turn's unknown-tree why stays when the secondary kill has none.
fn merged_kept(main: Option<KeptKill>, secondary: Option<KeptKill>) -> Option<KeptKill> {
    match (main, secondary) {
        (Some(m), Some(mut s)) => {
            if s.kill_unconfirmed.is_empty() {
                s.kill_unconfirmed = m.kill_unconfirmed;
            }
            Some(s)
        }
        (m, s) => s.or(m),
    }
}

/// (wave 3c, F23-3) The recovery record a kill keeps: `on` in state `survivors`, with the kill's
/// `survivors[]` (the entries read at the kill), `unverified[]` and - an unknown tree -
/// `kill_unconfirmed`.
fn kept_record(on: &PendingRecord, k: &KeptKill) -> PendingRecord {
    let mut r = on.clone();
    r.state = PendingState::Survivors;
    r.survivors = k
        .entries
        .clone()
        .unwrap_or_else(|| survivor_entries(&k.survivors));
    r.unverified = k.unverified.clone();
    if !k.kill_unconfirmed.is_empty() {
        r.kill_unconfirmed = Some(k.kill_unconfirmed.clone());
    }
    r
}

/// (wave 3c, F23-3) Where a kill writes the record it keeps. The plugin writes it AT each of its
/// three kill sites (`$pendingRecord.state = 'survivors'` ... `Write-PendingFile`, right after
/// `Stop-ProcessTreeChecked`), before the run goes on: a bridge that dies after the kill (a crash,
/// a forced termination) leaves a record that names what the kill left, never only the killed
/// child. `on` is the record the run has on disk at that kill (the run's base record; the format
/// repair's, which names the saved prose). TEST HOOK (test mode only):
/// `CODEX_CONSULT_TEST_KILL_PAUSE_MS=<ms> | <model>=<ms>[|...]` - a pause held right after that
/// write (the window a harness terminates the bridge in).
#[derive(Clone)]
struct KillSite {
    store: FilesStore,
    pending: PendingRef,
    on: PendingRecord,
    pause_ms: u64,
}

impl KillSite {
    /// Reads the survivors' entries now (once), writes the kept record, then holds the test pause.
    fn write(&self, k: &mut KeptKill) -> std::io::Result<()> {
        if k.entries.is_none() {
            k.entries = Some(survivor_entries(&k.survivors));
        }
        let written = self
            .store
            .write_pending(&self.pending, &kept_record(&self.on, k));
        if self.pause_ms > 0 {
            std::thread::sleep(Duration::from_millis(self.pause_ms));
        }
        written
    }
}

/// The main turn's kill check: the tree kill's own (`None` - no process, the http engine: a
/// confirmed kill of the recorded child), then the main-turn test hooks, and survivors - the
/// hook's too - make it unconfirmed. (wave 28e, E1) TEST HOOK (test mode only):
/// `CODEX_CONSULT_TEST_UNVERIFIED=<pid>[,<pid>]` - these pids, when alive, are reported as
/// descendants of this kill whose start time could not be read (only ever stricter). C3's
/// `CODEX_CONSULT_TEST_KILL_UNCONFIRMED` (test mode only) forces the kill unconfirmed.
fn main_kill_check(
    kill: Option<c3_core::engine::KillCheck>,
    base: &PendingRecord,
    survivors: &[u32],
) -> c3_core::engine::KillCheck {
    let mut c =
        kill.unwrap_or_else(|| c3_core::engine::KillCheck::confirmed(base.child_pid.unwrap_or(0)));
    let hooked: Vec<u32> = crate::liveness::proc::pid_list_hook("CODEX_CONSULT_TEST_UNVERIFIED")
        .into_iter()
        .filter(|p| crate::liveness::proc::process_start_iso(*p).is_some())
        .filter(|p| !c.unverified.contains(p))
        .collect();
    if !hooked.is_empty() {
        c.unverified.extend(hooked);
        c.confirmed = false;
        if c.why.is_empty() || c.why.starts_with("start time of pid ") {
            c.why = format!("start time of pid {} unreadable", join_pids(&c.unverified));
        }
    }
    if let Some(v) = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_KILL_UNCONFIRMED") {
        let t = v.trim();
        if !t.is_empty() && t != "0" {
            c.confirmed = false;
            if c.why.is_empty() {
                c.why = "a test hook forced the kill unconfirmed".to_string();
            }
        }
    }
    if !survivors.is_empty() {
        c.confirmed = false;
    }
    c
}

/// The main turn's outcome after a kill (`codex-consult.ps1`): survivors - `failed: <stop> (process
/// tree killed; <n> processes survived: pid ...[; <why>; pid <u> may still run]; the next run for
/// this task is refused until they exit)`; (wave 28e, E18) no survivor but unverified descendants -
/// `failed: <stop> (kill not confirmed: <why>; pid <u> may still run; the next run for this task is
/// refused until it exits)`; else `failed: <stop> <Format-KillText>`.
fn main_kill_outcome(stop: &str, c: &c3_core::engine::KillCheck, survivors: &[u32]) -> String {
    if !survivors.is_empty() {
        return format!(
            "failed: {stop} (process tree killed; {} processes survived: pid {}{}; the next run for this task is refused until they exit)",
            survivors.len(),
            join_pids(survivors),
            kill_unverified_text(c)
        );
    }
    if !c.unverified.is_empty() {
        let until = if c.unverified.len() == 1 {
            "it exits"
        } else {
            "they exit"
        };
        return format!(
            "failed: {stop} (kill not confirmed: {}; pid {} may still run; the next run for this task is refused until {until})",
            c.why,
            join_pids(&c.unverified)
        );
    }
    format!("failed: {stop} {}", format_kill_text(c, survivors))
}

/// A secondary turn's kill (`Invoke-EngineTurn`, the format repair): its problem text (`<stop>
/// <Format-KillText>`), its warning, and - survivors, unverified descendants or an unknown tree -
/// the record it keeps (wave 28e, E18 / E23 at every kill site).
fn secondary_kill(
    stop: &str,
    kill: Option<c3_core::engine::KillCheck>,
    survivors: &[u32],
    turn: &str,
    sec: &mut Secondary,
) -> String {
    let mut c = kill.unwrap_or_else(|| c3_core::engine::KillCheck::confirmed(0));
    if !survivors.is_empty() {
        c.confirmed = false;
    }
    if let Some(w) = kill_check_warning(&c, survivors, turn) {
        sec.kill_warnings.push(w);
    }
    if let Some(mut k) = kept_kill(&c, survivors) {
        // (wave 3c, F23-3) the record is written HERE, at the kill (the plugin's `catch { }`: a
        // failed write is retried with the end of the run's)
        if let Some(site) = sec.kill_site.clone() {
            if let Some(mut merged) = merged_kept(sec.main_kept.clone(), Some(k.clone())) {
                let _ = site.write(&mut merged);
                k.entries = merged.entries;
            }
        }
        sec.kept_kill = Some(k);
    }
    let text = format!("{stop} {}", format_kill_text(&c, survivors));
    sec.kill_checks.push((c, survivors.to_vec()));
    text
}

/// The after-run drift (`Compare-TreeContent`, the brief/artifact re-hash).
struct Drift {
    tree_sha256_after: String,
    tree_changed: bool,
    /// The working-tree paths that changed (for the engine tree-check message).
    tree_changed_paths: Vec<String>,
    /// `"<old> -> <new>"` when HEAD moved with identical content, else `""`.
    revision_moved: String,
    brief_sha_after: String,
    brief_changed: bool,
    /// The `artifacts[]` ledger records after the run (`{path, sha256, sha256_after}`).
    artifacts: Vec<serde_json::Value>,
    /// The `path`s whose sha256 changed during the review (drives the WARNING + the flag).
    artifacts_changed_paths: Vec<String>,
}

/// One resolved artifact bound to the review (`Resolve-ArtifactPaths` + `Get-ArtifactHashes`):
/// the path as given, the absolute path the after-run rehash reads, and the pre-run sha256.
#[derive(Debug, Clone)]
pub(crate) struct ArtifactHash {
    pub(crate) path: String,
    pub(crate) full: PathBuf,
    pub(crate) sha256: String,
}

/// The effort plan for a run (`Resolve-EffortPlan`).
pub(crate) struct EffortPlan {
    pub(crate) requested: String,
    /// `None` when nothing is sent (model-tier engines).
    pub(crate) sent: Option<String>,
    pub(crate) mapping: String,
    pub(crate) caps: String,
    pub(crate) basis: String,
    pub(crate) error: String,
}

fn vocabulary_map(vocab: &str, requested: &str) -> Option<(&'static str, &'static str)> {
    // (mapping-label, sent-value) for the caps-v1 vocabularies (`$script:EffortVocabularies`).
    let (label, low, medium, high, xhigh) = match vocab {
        "openai" => ("openai", "low", "medium", "high", "xhigh"),
        "zai" => ("zai-v1", "low", "high", "high", "max"),
        "mimo" => ("mimo-v1", "low", "medium", "high", "high"),
        "ark" => ("ark-v1", "low", "medium", "high", "high"),
        "kimi" => ("kimi-v1", "low", "high", "high", "max"),
        "alibaba" => ("alibaba-v1", "low", "medium", "high", "xhigh"),
        "muse" => ("muse-v1", "low", "medium", "high", "xhigh"),
        _ => return None,
    };
    let sent = match requested {
        "low" => low,
        "medium" => medium,
        "high" => high,
        "xhigh" => xhigh,
        _ => return None,
    };
    Some((label, sent))
}

fn effort_plan(id: &ReviewerIdentity, requested: &str, native: &str) -> EffortPlan {
    let caps_v = c3_core::effort::CAPS_VERSION.to_string();
    if !native.is_empty() {
        return EffortPlan {
            requested: native.to_string(),
            sent: Some(native.to_string()),
            mapping: "native".into(),
            caps: caps_v,
            basis: "-NativeEffort, sent verbatim".into(),
            error: String::new(),
        };
    }
    let host = &id.host;
    let cap = if host.is_empty() { None } else { caps(host) };
    let cap = match cap {
        Some(c) => c,
        None => {
            let host_label = if host.is_empty() {
                "unknown-host"
            } else {
                host
            };
            return EffortPlan {
                requested: requested.to_string(),
                sent: None,
                mapping: String::new(),
                caps: caps_v,
                basis: String::new(),
                error: format!(
                    "no effort vocabulary declared for {host_label} ({} declares {}); pass -NativeEffort <value> to send a value verbatim",
                    c3_core::effort::CAPS_VERSION,
                    c3_core::effort::DECLARED_HOSTS.join(", ")
                ),
            };
        }
    };
    if cap.vocabulary == "model-tier" {
        return EffortPlan {
            requested: requested.to_string(),
            sent: None,
            mapping: "model-tier".into(),
            caps: caps_v,
            basis: format!(
                "{}: engine {}, the tier is part of the model id",
                c3_core::effort::CAPS_VERSION,
                host.trim_start_matches("engine:")
            ),
            error: String::new(),
        };
    }
    // Model-list check.
    if let Models::List(list) = cap.models {
        if id.model_source == "unknown" || !list.contains(&id.model.as_str()) {
            return EffortPlan {
                requested: requested.to_string(),
                sent: None,
                mapping: String::new(),
                caps: caps_v,
                basis: String::new(),
                error: format!(
                    "no effort vocabulary declared for model '{}' on {host} ({} declares: {}); pass -NativeEffort <value> to send a value verbatim",
                    id.model, c3_core::effort::CAPS_VERSION, list.join(", ")
                ),
            };
        }
    }
    match vocabulary_map(cap.vocabulary, requested) {
        Some((mapping, sent)) => {
            let basis = match cap.models {
                Models::Any => format!("{}: {host}, any model", c3_core::effort::CAPS_VERSION),
                Models::List(_) => format!("{}: {host}, {}", c3_core::effort::CAPS_VERSION, id.model),
            };
            EffortPlan {
                requested: requested.to_string(),
                sent: Some(sent.to_string()),
                mapping: mapping.to_string(),
                caps: caps_v,
                basis,
                error: String::new(),
            }
        }
        None => EffortPlan {
            requested: requested.to_string(),
            sent: None,
            mapping: String::new(),
            caps: caps_v,
            basis: String::new(),
            error: format!(
                "{} names the effort vocabulary '{}' for {host}, but no such vocabulary is declared",
                c3_core::effort::CAPS_VERSION, cap.vocabulary
            ),
        },
    }
}

/// (wave 1b, 0.6.0 E5) What the direct run's plan check needs: the roster, the run's roster entry,
/// the codex launcher and the OpenAI base URL the plan's identities resolve with.
pub(crate) struct PlanCheck<'a> {
    pub(crate) roster: &'a c3_core::roster::Roster,
    pub(crate) entry: &'a c3_core::roster::RosterEntry,
    pub(crate) codex_launcher: &'a str,
    pub(crate) openai_base_url: &'a str,
}

/// The preflight verdict for the codex engine (M2c: credentials only). Returns the
/// `preflight` string for the ledger and, for a real run, an optional `(refusal, exit_code)`.
/// A non-openai provider's credential check is deferred: it is left unevaluated (`""`) and
/// never refuses, so such a run still proceeds (the recorded endpoint-health 24h block and
/// the non-openai credential/env-key check land later).
fn resolve_preflight(
    id: &ReviewerIdentity,
    launcher: &str,
    config: &c3_core::config::CodexConfig,
    collab_root: &Path,
    // (wave 26b, D-auth) the selected reviewer's roster entry declares `auth: "none"`: a provider
    // whose table has no env_key/bearer is then "ok: declared anonymous in the roster".
    anonymous: bool,
    // (wave 1b, 0.6.0 E5) the roster and the run's roster entry: a usage limit on another route of
    // the entry's plan refuses the run too (`Get-PlanQuotaVerdict`, the direct-run form).
    plan: Option<PlanCheck<'_>>,
) -> (String, Option<(String, i32)>, String, Option<String>) {
    // The plugin refuses a preflight through `Stop-WithError` (exit 1, nothing written,
    // `codex-consult.ps1:304`); c3 matches that, not cli-surface.md's aspirational exit 2/3.
    if let Some(v) = verdict_pre_credential(id) {
        return (v.preflight, Some((v.refusal, 1)), v.label, None);
    }
    // Endpoint health from every task ledger of THIS repository, read at the consult clock and
    // matched by the resolved identity's provider fingerprint: a recorded auth failure (within
    // 24 h), usage limit (with a reset), burst 429 (10 min) or reset-less quota (60 min) blocks
    // a later run before the lock (F09-2/4).
    let utc_now = c3_core::peak::consult_clock(0)
        .map(|(u, _, _)| u)
        .unwrap_or_else(|_| chrono::Utc::now());
    let consults = if id.resolved {
        providers::read_all_task_consults_health(collab_root)
    } else {
        Vec::new()
    };
    let health = if id.resolved {
        Some(c3_core::health::endpoint_health(
            &consults,
            &id.fingerprint,
            utc_now,
        ))
    } else {
        None
    };
    // The credential check: openai runs `codex login status`; a third-party provider checks its
    // `env_key` (`env X not set`) / bearer token. `-SkipPreflight` bypasses this whole function.
    // The `codex login status` timeout is 15 s (`codex-consult-common.ps1:3035`); the refusal
    // names it, so the default must be 15, not a safer-looking rounder value.
    let timeout = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_LOGIN_TIMEOUT")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(15);
    // A CLI engine checks its own sign-in (`agy models` / muse `auth.json`), with the recorded
    // endpoint-health short-circuit; codex runs `codex login status` / the provider env-key.
    let cred = if id.engine == "http" {
        // The http engine has no CLI to sign into and no launcher to find; its key/billing guard
        // runs at launch in `consult::http` (decision 3, "before every turn"), reading the key
        // from the environment and never probing the network. The preflight is a no-op so the
        // seat is not refused for a missing launcher; the endpoint-health short-circuit above
        // still blocks a run after a recorded auth/quota failure on this http endpoint.
        c3_core::credential::CredentialResult::ok("checked at launch (http engine)")
    } else if id.engine.is_empty() || id.engine == "codex" {
        providers::identity_credential(config, id, launcher, anonymous, timeout)
    } else {
        providers::engine_consult_credential(&id.engine, launcher, health.as_ref())
    };
    // (wave 27c, D4) a launcher probe that was SKIPPED (its start-info could not be scrubbed) earns
    // a run warning naming why — the same `a launcher probe was skipped: <why>` the plugin records.
    let probe_warning = cred
        .reason
        .strip_prefix("not checked - `codex login status` was skipped: ")
        .map(|why| format!("a launcher probe was skipped: {why}"));
    let mut v = verdict_with_credential(id, health.as_ref(), cred, false);
    // (wave 29b, E5) a usage limit on another route of the roster entry's plan refuses the run too
    if let Some(pc) = plan {
        if pc.roster.exists && !pc.entry.plan.is_empty() {
            let ctx = providers::Ctx::for_consult(
                config.clone(),
                consults,
                pc.roster.clone(),
                pc.codex_launcher.to_string(),
                pc.openai_base_url.to_string(),
                utc_now,
            );
            v = ctx.plan_verdict(v, pc.entry, id, false);
        }
    }
    if v.state == "available" {
        (v.preflight, None, v.label, probe_warning)
    } else {
        (v.preflight, Some((v.refusal, 1)), v.label, probe_warning)
    }
}

/// The `-SkipPreflight` quota warning (`Format-QuotaWarning`): empty unless a quota record is
/// active on the endpoint. A known reset names the reset time; a reset-less limit names the
/// out-window end and marks a burst 429.
fn format_quota_warning(id: &ReviewerIdentity, health: &c3_core::health::EndpointHealth) -> String {
    let q = match &health.quota {
        Some(q) => q,
        None => return String::new(),
    };
    if health.quota_known {
        format!(
            "provider {} hit a usage limit {} min ago that lasts until {}: {}",
            id.provider, q.age_minutes, q.retry_after_iso, q.message
        )
    } else {
        let limit = if q.kind == "burst" {
            "burst limit (429)"
        } else {
            "usage limit"
        };
        format!(
            "provider {} hit a {limit} {} min ago (reset unknown; out until {}): {}",
            id.provider,
            q.age_minutes,
            c3_core::health::format_offset_iso(q.until),
            q.message
        )
    }
}

/// The resolved schema transport and its source.
pub(crate) struct Transport {
    /// `output-schema` | `prompt-only` | `native` | `""` (raw).
    pub(crate) transport: String,
    pub(crate) source: String,
    pub(crate) basis: String,
}

fn resolve_transport(id: &ReviewerIdentity, r: &Resolved) -> Transport {
    if r.raw {
        return Transport {
            transport: String::new(),
            source: String::new(),
            basis: String::new(),
        };
    }
    // The caps-v1 choice for this endpoint (what a run WITHOUT -SchemaTransport would use).
    let host = &id.host;
    let caps_transport = match if host.is_empty() { None } else { caps(host) } {
        Some(c) => Transport {
            transport: c.schema_transport.to_string(),
            source: c3_core::effort::CAPS_VERSION.to_string(),
            basis: format!("{}: {host}", c3_core::effort::CAPS_VERSION),
        },
        None => Transport {
            transport: "prompt-only".into(),
            source: c3_core::effort::CAPS_VERSION.to_string(),
            basis: format!(
                "default for an endpoint {} does not declare: {}",
                c3_core::effort::CAPS_VERSION,
                if host.is_empty() {
                    "unknown-host"
                } else {
                    host
                }
            ),
        },
    };
    if !r.transport_override.is_empty() {
        // The basis names the override AND what caps-v1 would have used (`$schemaTransportBasis`).
        return Transport {
            transport: r.transport_override.clone(),
            source: "-SchemaTransport".into(),
            basis: format!(
                "-SchemaTransport; {} would use {}",
                caps_transport.basis, caps_transport.transport
            ),
        };
    }
    caps_transport
}

/// Resolved run context, shared by the dry-run and live branches.
pub(crate) struct Context {
    pub(crate) o: Options,
    pub(crate) r: Resolved,
    pub(crate) repo_root: PathBuf,
    pub(crate) collab_root: PathBuf,
    pub(crate) task: TaskSlug,
    pub(crate) reply_name: String,
    pub(crate) nn: u32,
    pub(crate) consult_n: i64,
    pub(crate) consult_id: String,
    /// (0.6.1, U5) the ledger `consult_ref` (a fresh random lower-case guid, never derived).
    pub(crate) consult_ref: String,
    /// (wave 28b, D15) the reviewer's roster `context_tokens` (0 = none), the `-c` items it adds to
    /// every codex turn and the ledger `context_window` record (`None` = null).
    pub(crate) context_tokens: i64,
    pub(crate) context_config: Vec<String>,
    pub(crate) context_window: Option<serde_json::Value>,
    pub(crate) identity: ReviewerIdentity,
    /// (wave 27) The coordinator that started this run (`Resolve-CoordinatorIdentity`): the ledger
    /// `coordinator` record and the dry-run `coordinator :` line.
    pub(crate) coordinator: c3_core::ledger::Coordinator,
    /// (wave 27) The host-marker variable NAMES removed from every reviewer child's environment
    /// (`Get-HostMarkerNames`, captured once from this process's env). The ledger
    /// `child_env_scrubbed` value and the dry-run `child env   :` line.
    pub(crate) child_env_scrubbed: Vec<String>,
    pub(crate) effort: EffortPlan,
    pub(crate) transport: Transport,
    pub(crate) launcher: String,
    pub(crate) codex_version: String,
    /// The selected engine (`codex`/`agy`/`muse`) and where it came from (dry-run `engine :`).
    pub(crate) engine: String,
    /// The engine's handoff-file name prefix (`codex`/`agy`/`muse`), used for every primary and
    /// secondary turn file name (`NN-<prefix>-<reply>.*`).
    pub(crate) file_prefix: String,
    pub(crate) engine_from: String,
    /// The resolved launcher of the selected engine (codex uses [`launcher`]).
    pub(crate) engine_launcher: String,
    /// `reviewer.harness` for the run (`codex-cli <v>` / `agy-cli ...` / `muse-cli <v>`).
    pub(crate) harness: String,
    /// The ledger `sandbox` record (an engine's enforcement note; codex = the requested value).
    pub(crate) sandbox_record: String,
    /// The muse `--prompt-file` path (a temp file); `None` for stdin engines.
    pub(crate) prompt_file: Option<PathBuf>,
    pub(crate) prompt_text: String,
    pub(crate) argv_display: String,
    pub(crate) argv: Vec<String>,
    pub(crate) brief_ref: String,
    pub(crate) brief_path: Option<PathBuf>,
    /// The brief's sha256 hex taken before the run (`$briefSha`); empty when no brief.
    pub(crate) brief_sha: String,
    /// The `-Artifact` files bound to the review, resolved + pre-run-hashed (`[]` when none).
    pub(crate) artifacts: Vec<ArtifactHash>,
    pub(crate) schema_path: Option<PathBuf>,
    pub(crate) open_findings_count: usize,
    /// The effective mode after the parent-thread walk (`new`|`fork`|`resume`), recorded in the
    /// ledger and used to plan the codex `resume`/`fork` argv.
    pub(crate) effective_mode: String,
    /// The resolved parent thread (`Select-ParentThread`'s `$r.Parent`); empty for a new thread.
    pub(crate) parent_thread: String,
    /// The parent-thread note (`$r.Note`); empty when none.
    pub(crate) parent_note: String,
    /// The `Roster: ...` console/handoff line (empty when no roster file).
    pub(crate) roster_line: String,
    /// The `roster{}` ledger record (`None` for a run with no roster file).
    pub(crate) roster_record: Option<c3_core::ledger::RosterRef>,
    /// The `extra_config_source` (`""`, `-CodexConfig` or `roster`).
    pub(crate) extra_config_source: String,
    /// The preflight string recorded in the ledger / handoff (`""` when not evaluated).
    pub(crate) preflight: String,
    /// The dry-run `preflight :` label (verdict `.Label`).
    pub(crate) preflight_label: String,
    /// The `-SkipPreflight` quota warning (`Format-QuotaWarning`), printed `WARNING: ...` before
    /// a live launch and recorded in the ledger; empty otherwise.
    pub(crate) preflight_warning: String,
    /// A preflight refusal `(message, exit_code)` for a real run; `None` = available/skipped.
    pub(crate) preflight_refusal: Option<(String, i32)>,
    pub(crate) revision: RevisionInfo,
    /// The measured `-Range` record for the ledger; `None` when no range was given.
    pub(crate) range_record: Option<RangeRecord>,
    /// The reviewer note text (`the range changes N files, N lines`); empty when no range.
    pub(crate) range_text: String,
    /// Run warnings (`$runWarnings`): the range size warning, roster ambiguity, drift, etc.
    pub(crate) run_warnings: Vec<String>,
    /// The provider whose peak schedule is evaluated (`""` when the identity is unresolved).
    pub(crate) peak_provider: String,
    /// The peak-window state and its record fields (evaluated at launch for a live run).
    pub(crate) peak: Option<bool>,
    pub(crate) peak_schedule: String,
    pub(crate) peak_source: String,
    pub(crate) peak_evaluated_at: String,
    /// The `WARNING: ... peak window ...` line (empty when off-peak/unknown).
    pub(crate) peak_warning: String,
    /// The dry-run `peak` label (`PEAK (...)` / `off-peak (...)` / `unknown (...)`).
    pub(crate) peak_label: String,
    /// Whether telemetry is enabled for this run (`--telemetry`/env switch).
    pub(crate) telemetry_enabled: bool,
    /// The dry-run `pending :` recovery lines (a dry run reports, never refuses).
    pub(crate) recovery_dry_lines: Vec<String>,
    /// The recovered/cleared lines of consumed records (set under the lock in `run_live`),
    /// echoed to the console and carried into the handoff header as `Recovery record: ...`.
    pub(crate) recovery_lines: Vec<String>,
    /// The panel member spec this run honours (`--panel-spec`); `None` for a single run.
    pub(crate) panel_member: Option<crate::panel::member::MemberSpec>,
    /// (wave 1b, 0.6.0 E16) the run's roster entry's plan (`""` without one): its machine-wide
    /// running row carries it, so a panel elsewhere counts this run against the plan's limit.
    pub(crate) plan: String,
    /// The role name (a member's, or a single run's `-Role`; empty when none), for the ledger
    /// `role` field.
    pub(crate) role: String,
    /// (wave 26, R16) the resolved role file (name, source, path) for the dry run's `role :` line.
    pub(crate) role_info: Option<crate::panel::roles::RoleInfo>,
    /// (wave 26, D7) a single run's `-Require` positions, all available (the dry run's
    /// `required    :` line); empty without `-Require`.
    pub(crate) single_required: Vec<i64>,
    /// The panel-wide roles note (ledger `panel.roles_note`), if any.
    pub(crate) roles_note: String,
    /// (wave 26b, D16) a fork/resume the reviewer's context window forced down to a new thread
    /// (`{from, to, reason}`); `None` when the mode was not downgraded.
    pub(crate) mode_fallback: Option<c3_core::ledger::ModeFallback>,
    /// (wave 26b, D12) the effective stall cut in seconds for this run (`0` = off), after the
    /// roster entry's `stall_sec` override.
    pub(crate) stall_sec: i64,
    /// (wave 26b, D10) this run's kick file `<task>/.consult.kick-<NN>`; the primary turn watches
    /// it and, when it appears, stops as `stopped by the operator (-Kick)`.
    pub(crate) kick_path: PathBuf,
    // paths
    pub(crate) handoffs_dir: PathBuf,
    pub(crate) reply_path: PathBuf,
    pub(crate) reply_json_path: PathBuf,
    pub(crate) events_path: PathBuf,
    pub(crate) last_msg_path: PathBuf,
    pub(crate) stderr_path: PathBuf,
}

impl Context {
    /// `handoffs/NN-<prefix>-<reply>.<ext>` — the engine-prefixed handoff file (repo-relative).
    fn hf(&self, ext: &str) -> String {
        format!(
            "handoffs/{:02}-{}-{}.{}",
            self.nn, self.file_prefix, self.reply_name, ext
        )
    }

    /// The absolute path of a handoff file with the given extension.
    fn hpath(&self, ext: &str) -> PathBuf {
        self.handoffs_dir.join(format!(
            "{:02}-{}-{}.{}",
            self.nn, self.file_prefix, self.reply_name, ext
        ))
    }

    fn is_codex(&self) -> bool {
        self.engine == "codex"
    }
}

/// Run one consultation; return the exit code. Wraps [`run_inner`] with the telemetry
/// background flush and the one-time notice (a real run only — a dry run does nothing), and
/// joins the flush (capped at 3 s) at every exit path.
pub fn run(o: Options) -> i32 {
    let cfg = telemetry::Config {
        telemetry: o.telemetry,
    };
    let real = telemetry::is_enabled(&cfg) && !o.dry_run;
    let bg = if real {
        Some(telemetry::flush_in_background())
    } else {
        None
    };
    if real {
        if let Some(notice) = telemetry::first_run_notice() {
            println!("{notice}");
        }
    }
    let code = run_inner(o);
    if let Some(bg) = bg {
        bg.join_with_cap(Duration::from_secs(3));
    }
    code
}

fn run_inner(o: Options) -> i32 {
    // (Test harness) Share the fate of the PowerShell shim that launched this run: if the harness
    // kills the shim, this process exits too, rather than orphaning a run that keeps the task lock
    // and finishes a commit the test means to interrupt. No effect in production (the var is unset);
    // a panel member never has it (the panel run clears it when spawning members).
    crate::liveness::proc::watch_bridge();
    // Hand the validated bridge pid to c3-core once, so its lock records name it without c3-core
    // ever reading the environment itself (an invalid or non-ancestor hook resolves to our own pid).
    c3_core::store::set_bridge_pid(crate::liveness::proc::bridge_identity().0);
    // `-CodexConfig` `~` expansion uses the real user home ($HOME / $USERPROFILE), exactly like
    // the plugin (`codex-consult-common.ps1:2576`), NOT CODEX_HOME.
    let home = std::env::var("HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("USERPROFILE").ok().filter(|s| !s.is_empty()));
    // A panel member re-exec (`--panel-spec`): reconstruct the run from its spec and honour it.
    // `-Detach -PanelSpec` is a misuse the `-Detach` foreground refuses, so it takes precedence.
    if !o.panel_spec.is_empty() && !o.detach {
        return run_member(o, home.as_deref());
    }
    // ---- (wave 26b, D10) -Kick / -Member: stop one running member (before the run surface).
    if o.kick || !o.member.is_empty() {
        return super::kick::run(&o);
    }
    // ---- the detached surface (R12): -Status/-Wait (read only), -DetachId (the background), the
    // -Id/-Prune misuse refusal, and -Detach (the foreground). Ordered as `codex-consult.ps1`.
    if o.status || o.wait {
        return super::detach::query(&o);
    }
    if o.id_given || o.prune || o.wait_timeout_sec_given {
        return refuse("-Id and -Prune go with -Status (-Id and -WaitTimeoutSec with -Wait).");
    }
    if !o.detach_id.is_empty() {
        return super::detach::background(o, run_normal);
    }
    if o.detach {
        return detach_foreground(o, home.as_deref());
    }
    run_normal(o)
}

/// The normal run flow (a single run or a panel), reused by the detached background.
fn run_normal(mut o: Options) -> i32 {
    let home = std::env::var("HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("USERPROFILE").ok().filter(|s| !s.is_empty()));
    let r = match args::validate(&o, home.as_deref()) {
        Ok(r) => r,
        Err(msg) => return refuse(&msg),
    };
    r.apply_companions(&mut o);
    if o.panel || o.panel_all {
        return crate::panel::run::run(o, r, home.as_deref());
    }
    dispatch(build_context(o, r, None))
}

/// The foreground of `-Detach` (D2, D8): make every check a real run makes before its lock, then
/// spawn the background — or refuse with nothing left behind.
fn detach_foreground(mut o: Options, home: Option<&str>) -> i32 {
    if !o.panel_spec.is_empty() {
        return refuse("-Detach does not go with -PanelSpec (internal to -Panel).");
    }
    if o.dry_run {
        return refuse(
            "-Detach does not go with -DryRun: a dry run starts nothing to detach - run -DryRun alone first.",
        );
    }
    let r = match args::validate(&o, home) {
        Ok(r) => r,
        Err(msg) => return refuse(&msg),
    };
    r.apply_companions(&mut o);
    if o.panel || o.panel_all {
        return crate::panel::run::detach_foreground(o, r, home);
    }
    // A single detached run: build the context (the identity/preflight/launcher/brief refusals a
    // real run makes), then the pre-lock refusals (launcher missing, preflight, active record).
    let ctx = match build_context(o.clone(), r, None) {
        Ok(c) => c,
        Err((msg, code)) => {
            return if code == 1 {
                refuse(&msg)
            } else {
                eprintln!("{TOOL}: {msg}");
                code
            }
        }
    };
    if ctx.is_codex() && ctx.launcher.is_empty() {
        return refuse(
            "codex CLI not found on PATH (set -CodexExe <path> or the CODEX_CONSULT_EXE environment variable).",
        );
    }
    if !ctx.is_codex() && ctx.engine_launcher.is_empty() {
        let exe_env = c3_core::lineage::engine_spec(&ctx.engine)
            .map(|s| s.exe_env)
            .unwrap_or("");
        return refuse(&format!(
            "{} CLI not found on PATH (set -EngineExe <path> or the {exe_env} environment variable).",
            ctx.engine
        ));
    }
    if let Some((msg, code)) = ctx.preflight_refusal.clone() {
        eprintln!("{TOOL}: {msg}");
        return code;
    }
    // An active recovery record refuses before the lock (nothing consumed/written).
    let store = FilesStore::new(ctx.collab_root.clone());
    let assessed = super::recovery::assess(&store, &ctx.task);
    if let Some(err) = assessed.error {
        return refuse(&err);
    }
    if let Some(msg) = assessed.active_message() {
        return refuse(&msg);
    }
    // The budget (D4), the one planned member, the plan line, then spawn.
    let denial_on = ctx.engine == "agy" && ctx.o.denial_retry == 1;
    let budget = super::detach::single_run_budget(
        ctx.r.timeout_sec,
        ctx.r.continue_sec,
        ctx.r.repair_enabled,
        denial_on,
    );
    let lineage = c3_core::lineage::format_reviewer_lineage(
        &ctx.identity.provider,
        &ctx.identity.model,
        &ctx.engine,
    );
    let members = vec![super::detach::PlannedMember {
        position: 1,
        lineage: lineage.clone(),
        state: "pending".into(),
        outcome: String::new(),
    }];
    let plan = format!(
        "a single run of {lineage} (purpose {}, timeout {} s)",
        ctx.r.purpose_label, ctx.r.timeout_sec
    );
    let brief_full = ctx
        .brief_path
        .as_ref()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    // The run's warnings, the preflight's and the peak window's (`$detachWarnings`); the
    // test-mode line among them is printed after the detach lines (wave 28b, D10).
    let mut warnings: Vec<String> = ctx.run_warnings.clone();
    for w in [&ctx.preflight_warning, &ctx.peak_warning] {
        if !w.is_empty() {
            warnings.push(w.clone());
        }
    }
    super::detach::start_detached_run(&o, "run", &members, budget, &plan, &brief_full, &warnings)
}

/// The panel member re-exec (`--panel-spec`): decode the spec, reconstruct the run's options
/// from `spec.args`, and run it as the panel run's member (never a single-run reservation).
fn run_member(o: Options, home: Option<&str>) -> i32 {
    if o.panel || o.panel_all {
        return refuse("-PanelSpec is internal to -Panel; never combine them.");
    }
    let spec = match crate::panel::member::MemberSpec::from_wire(&o.panel_spec) {
        Ok(s) => s,
        Err(e) => {
            return refuse(&format!(
                "-PanelSpec is internal to -Panel and could not be read ({}).",
                c3_core::one_line(&e)
            ))
        }
    };
    if !spec.names_member() {
        return refuse(
            "-PanelSpec is internal to -Panel and does not name this member's numbers and parent (n, nn, parent_pid); this panel member was not started.",
        );
    }
    let mut mo = member_options(&o.task, &spec);
    let r = match args::validate(&mo, home) {
        Ok(r) => r,
        Err(msg) => return refuse(&msg),
    };
    r.apply_companions(&mut mo);
    // The member rewrites its reserved record as its own, waits the test-pause, and checks the
    // parent is alive — all BEFORE its preflight (`codex-consult.ps1:2074-2113`), so the
    // kill-after-rewrite window (F07-1/F11-6) is real and a parent that dies during the preflight
    // is caught by the pre-launch check (in `run_live`) with "stopped before starting", not here.
    // A dry-run member has no record.
    if !mo.dry_run {
        if let Err(msg) = member_early_accept(&mo, &spec) {
            return refuse(&msg);
        }
    }
    dispatch(build_context(mo, r, Some(&spec)))
}

/// The member's record rewrite + pause hook + parent-alive check, done before its preflight
/// (`codex-consult.ps1:2074-2113`). Rewrites the reserved record with this process's pid/host,
/// keeping every other field (state stays `reserved`), then — after the optional test pause —
/// refuses (and withdraws the record) if the parent is gone.
fn member_early_accept(mo: &Options, m: &crate::panel::member::MemberSpec) -> Result<(), String> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = providers::resolve_repo_root(&cwd);
    let collab_root = providers::resolve_collab_root(&repo_root, &mo.collab_dir);
    let task = TaskSlug::new(mo.task.clone()).map_err(|e| e.to_string())?;
    let store = FilesStore::new(collab_root);
    let pending = PendingRef::member(task, m.nn as u32);
    let path = store_pending_path(&store, &pending);
    let started = "This panel member was not started.";
    if !path.is_file() {
        return Err(format!(
            "this panel member's recovery record '{}' does not exist (the panel run writes it before it launches a member); this panel member was not started.",
            path.display()
        ));
    }
    let bytes = std::fs::read(&path).map_err(|e| {
        format!(
            "could not read the recovery record '{}' ({e}). {started}",
            path.display()
        )
    })?;
    let val: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| {
        format!(
            "the recovery record '{}' is unusable ({e}). {started}",
            path.display()
        )
    })?;

    let mut mism: Vec<String> = Vec::new();
    match val.get("panel") {
        None | Some(serde_json::Value::Null) => mism.push("the record names no panel".into()),
        Some(p) => {
            add_mismatch(&mut mism, "panel id", json_str(p, "id"), m.id.clone());
            add_mismatch(
                &mut mism,
                "parent pid",
                json_str(p, "parent_pid"),
                m.parent_pid.to_string(),
            );
            add_mismatch(
                &mut mism,
                "parent start time",
                json_str(p, "parent_start_time"),
                m.parent_start_time.clone(),
            );
        }
    }
    add_mismatch(&mut mism, "n", json_str(&val, "n"), m.n.to_string());
    add_mismatch(
        &mut mism,
        "nn",
        json_str(&val, "nn"),
        format!("{:02}", m.nn),
    );
    add_mismatch(
        &mut mism,
        "state",
        json_str(&val, "state"),
        "reserved".to_string(),
    );
    add_mismatch(
        &mut mism,
        "writer pid",
        json_str(&val, "pid"),
        m.parent_pid.to_string(),
    );
    if !mism.is_empty() {
        return Err(format!(
            "this panel member's recovery record '{}' does not match its spec ({}); this panel member was not started.",
            path.display(),
            mism.join("; ")
        ));
    }

    let mut rec: PendingRecord = serde_json::from_value(val).unwrap_or_default();
    // The member rewrites the reserved record as its OWN (the writer-pid rule): its own pid when
    // the panel run launched it (the panel clears the bridge var for members), or the shim's pid
    // when a harness launched the member directly through the shim (SPEC).
    let (pid, start_time) = crate::liveness::proc::bridge_identity();
    rec.pid = pid;
    rec.start_time = start_time;
    rec.host = pending_host();
    rec.note = format!(
        "review panel member (the panel run is pid {})",
        m.parent_pid
    );
    if let Err(e) = store.write_pending(&pending, &rec) {
        return Err(format!(
            "could not rewrite this panel member's recovery record '{}': {e}; this panel member was not started.",
            path.display()
        ));
    }

    // TEST HOOK: CODEX_CONSULT_TEST_MEMBER_PAUSE_MS - a pause between the rewrite and the parent
    // check (the harness kills the parent inside it).
    if let Some(ms) = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_MEMBER_PAUSE_MS") {
        if let Ok(ms) = ms.trim().parse::<u64>() {
            if ms > 0 {
                std::thread::sleep(Duration::from_millis(ms));
            }
        }
    }

    if !crate::liveness::proc::pid_alive(m.parent_pid as u32, &m.parent_start_time) {
        let withdrawn = std::fs::remove_file(&path).is_ok();
        return Err(format!(
            "the review panel run that launched this member (pid {}) is gone; this panel member was not started - nothing was started and its recovery record '{}' {}.",
            m.parent_pid,
            path.display(),
            if withdrawn {
                "was withdrawn".to_string()
            } else {
                "could not be withdrawn (the next run consumes it)".to_string()
            }
        ));
    }
    Ok(())
}

/// The shared build-context → run dispatch (single run and panel member).
fn dispatch(built: Result<Context, (String, i32)>) -> i32 {
    match built {
        Ok(ctx) => {
            // The http engine sends one OpenAI-compatible request built from a reviewer pack; it
            // has no CLI launcher, so it takes its own dry-run render and live run path and is
            // exempt from the launcher-not-found refusal below (`consult::http`, M7b-b).
            if ctx.engine == "http" {
                return if ctx.o.dry_run {
                    super::http::render_dry_run(&ctx);
                    0
                } else if let Some((msg, code)) = ctx.preflight_refusal.clone() {
                    eprintln!("{TOOL}: {msg}");
                    code
                } else {
                    run_live(ctx)
                };
            }
            if ctx.o.dry_run {
                super::dryrun::render(&ctx);
                0
            } else if !ctx.is_codex() && ctx.engine_launcher.is_empty() {
                // A non-dry engine run with no resolved launcher is refused up front, exactly
                // like the codex `-CodexExe` case (`codex-consult.ps1:3044`).
                let exe_env = c3_core::lineage::engine_spec(&ctx.engine)
                    .map(|s| s.exe_env)
                    .unwrap_or("");
                refuse(&format!(
                    "{} CLI not found on PATH (set -EngineExe <path> or the {exe_env} environment variable).",
                    ctx.engine
                ))
            } else if let Some((msg, code)) = ctx.preflight_refusal.clone() {
                // A preflight refusal happens before the lock: nothing is started or written.
                eprintln!("{TOOL}: {msg}");
                code
            } else {
                run_live(ctx)
            }
        }
        Err((msg, code)) => {
            if code == 1 {
                refuse(&msg)
            } else {
                eprintln!("{TOOL}: {msg}");
                code
            }
        }
    }
}

/// Reconstruct a run's [`Options`] from a panel member spec's `args` object (`$pa`,
/// `codex-consult.ps1:1749-1786`) plus the task from the command line. Provider/model/thread are
/// never in `args` (a member takes its identity from the spec's roster entry); the numbers,
/// consult id and role come from the spec's top-level fields, handled in `build_context`.
fn member_options(task: &str, spec: &crate::panel::member::MemberSpec) -> Options {
    let a = &spec.args;
    let s = |k: &str| a.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let i = |k: &str, d: i64| a.get(k).and_then(|v| v.as_i64()).unwrap_or(d);
    let b = |k: &str| a.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    let list = |k: &str| {
        a.get(k)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    let collab = s("collab_dir");
    let continue_sec = i("continue_sec", -1);
    Options {
        task: task.to_string(),
        // (M11) a panel passes `--peer`/`--peers` through to its http members only; the member
        // spec carries them and they are set below when present (parked default here).
        peer: list("peer"),
        peers: {
            let v = s("peers");
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        },
        collab_dir: if collab.is_empty() {
            ".collab".into()
        } else {
            collab
        },
        mode: s("mode"),
        thread: String::new(),
        brief: s("brief"),
        prompt: s("prompt"),
        model: String::new(),
        purpose: s("purpose"),
        effort: s("effort"),
        sandbox: s("sandbox"),
        max_words: i("max_words", 0),
        timeout_sec: i("timeout_sec", 0),
        continue_sec,
        continue_sec_given: continue_sec != -1,
        stall_sec: i("stall_sec", -1),
        stall_sec_given: i("stall_sec", -1) != -1,
        range: s("range"),
        reply_name: s("reply_name"),
        artifacts: list("artifact"),
        raw: b("raw"),
        codex_exe: s("codex_exe"),
        provider: String::new(),
        // A panel http seat takes its base_url/key_env/pack_tokens from the roster entry (looked
        // up by the child from its own roster read), never from the member spec's args.
        key_env: String::new(),
        base_url: String::new(),
        pack_budget: -1,
        native_effort: s("native_effort"),
        off_peak_only: b("off_peak_only"),
        skip_preflight: b("skip_preflight"),
        codex_config: list("codex_config"),
        schema_transport: s("schema_transport"),
        // (0.6.1 parity) the panel run's own `--telemetry on|off`, else the environment decides.
        telemetry: match s("telemetry").as_str() {
            "on" => Some(true),
            "off" => Some(false),
            _ => None,
        },
        format_retry: i("format_retry", 1),
        dry_run: b("dry_run"),
        engine: s("engine"),
        engine_exe: s("engine_exe"),
        denial_retry: i("denial_retry", 1),
        max_model_steps: i("max_model_steps", 0),
        panel: false,
        panel_all: false,
        panel_concurrency: 0,
        panel_concurrency_given: false,
        panel_size: 0,
        panel_size_given: false,
        panel_order: String::new(),
        panel_seed: String::new(),
        require: Vec::new(),
        role: String::new(),
        roles: Vec::new(),
        topic: list("topics"),
        panel_spec: String::new(),
        detach: false,
        status: false,
        id: String::new(),
        id_given: false,
        detach_id: String::new(),
        list: false,
        wait: false,
        wait_timeout_sec: 0,
        wait_timeout_sec_given: false,
        prune: false,
        kick: false,
        member: String::new(),
    }
}

fn build_context(
    mut o: Options,
    mut r: Resolved,
    member: Option<&crate::panel::member::MemberSpec>,
) -> Result<Context, (String, i32)> {
    let o_telemetry = o.telemetry; // captured before `o` moves into the Context
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = providers::resolve_repo_root(&cwd);
    let collab_root = providers::resolve_collab_root(&repo_root, &o.collab_dir);
    let task = TaskSlug::new(o.task.clone()).map_err(|e| (e.to_string(), 1))?;

    // (wave 26b, D16) the new prompt's estimated token size ((ask + brief file) / 4 chars a
    // token), computed early so the roster walk can skip a reviewer whose context window the brief
    // alone would overflow. The brief path is resolved fully later; here only its byte length.
    let prompt_estimate = {
        let bp = if o.brief.is_empty() {
            None
        } else if Path::new(&o.brief).is_absolute() {
            Some(PathBuf::from(&o.brief))
        } else {
            Some(cwd.join(&o.brief))
        };
        estimate_prompt_tokens(&o.prompt, bp.as_deref())
    };

    // Launcher + config + identity.
    let launcher = providers::resolve_codex_launcher(&o.codex_exe).map_err(|m| (m, 1))?;
    let config_path = providers::get_codex_config_path();
    let config = providers::read_codex_config(&config_path);
    let openai_base_url = std::env::var("OPENAI_BASE_URL").unwrap_or_default();

    // The reviewer roster (`Read-ReviewerRoster`). A missing `CODEX_CONSULT_ROSTER` file or an
    // unusable roster refuses before anything is planned (a dry run too — FILE).
    let roster = providers::read_reviewer_roster().map_err(|m| (m, 1))?;

    // (wave 27) The coordinator that started this run and the host markers scrubbed from every
    // reviewer child, both from THIS process's environment. An unparseable CODEX_CONSULT_COORDINATOR
    // and a bad brief prefix refuse here, before anything is planned or written (a dry run too). A
    // panel member / detached run inherits the coordinator's environment unchanged, so it recomputes
    // the identical record and scrub list.
    let coordinator = resolve_coordinator(if roster.exists {
        Some(&roster.entries[..])
    } else {
        None
    })?;
    let child_env_scrubbed = c3_core::host::host_marker_names();
    // (wave 26, D7) -Require: a panel, or a single run of a chosen reviewer (-Provider).
    let require_given = o.require.iter().any(|v| !v.trim().is_empty());
    if require_given && !(o.panel || o.panel_all) && member.is_none() && o.provider.is_empty() {
        return Err((
            "-Require goes with -Panel, or with -Provider (a single run of a chosen reviewer); a roster walk takes whichever reviewer is available.".to_string(),
            1,
        ));
    }

    let utc_now = c3_core::peak::consult_clock(0)
        .map(|(u, _, _)| u)
        .unwrap_or_else(|_| chrono::Utc::now());

    // The engine (0.4.0): `-Engine`, else the engine of the roster entry used, else codex.
    // `engine_from` labels the dry-run `engine     :` line. `-EngineExe` names the launcher of
    // the SELECTED non-codex engine (`Resolve-EngineExeBinding`, D3): bound once the roster is
    // read; the binding refusals fire here, and the resolved launcher seeds the walk.
    let mut engine_name = o.engine.clone();
    let mut engine_from = if o.engine.is_empty() {
        String::new()
    } else {
        "-Engine".to_string()
    };
    let engine_exe = o.engine_exe.trim().to_string();
    let mut engine_exe_engine = String::new();
    let mut engine_exe_launcher = String::new();
    if !engine_exe.is_empty() {
        engine_exe_engine = resolve_engine_exe_binding(&o.engine, &roster, &o.provider, &o.model)
            .map_err(|m| (m, 1))?;
        match providers::resolve_engine_launcher(&engine_exe_engine, &engine_exe) {
            Ok(Some(l)) => engine_exe_launcher = l,
            _ => {
                return Err((
                    format!(
                        "-EngineExe '{engine_exe}' is not a file and not an application on PATH."
                    ),
                    1,
                ))
            }
        }
    }

    // This task's ledger (read before the lock; `Select-ParentThread` re-checks under it), used
    // by the `-Thread` roster rule to source the reviewer from the thread's own entry.
    let store = FilesStore::new(collab_root.clone());
    let ledger_entries = store
        .read_sessions(&task)
        .ok()
        .flatten()
        .map(|s| s.codex.consults)
        .unwrap_or_default();

    // Which reviewer (`Read-ReviewerRoster`, `Select-RosterReviewer`): no roster → -Provider /
    // -Model, else the Codex config; -Provider → its entry supplies model/codex_config; -Thread →
    // the thread's reviewer, its entry supplies codex_config; otherwise the roster walk.
    let mut identity_provider = o.provider.clone();
    let mut identity_model = o.model.clone();
    let mut provider_source_override = String::new();
    let mut model_source_override = String::new();
    let mut roster_rule = String::new();
    let mut roster_entry: Option<c3_core::roster::RosterEntry> = None;
    let mut roster_skipped: Vec<(String, String, String, String)> = Vec::new();
    let mut roster_applied: Vec<String> = Vec::new();
    // (wave 26b, D12) the effective stall cut: the --stall-sec baseline, overridden by the
    // matched roster entry's stall_sec below (unless --stall-sec was given).
    let mut stall_sec = r.stall_sec;
    let mut run_warnings: Vec<String> = Vec::new();
    // (wave 27c, D14) a CODEX_CONSULT_TEST_* / CODEX_CONSULT_NOW hook set without
    // CODEX_CONSULT_TEST_MODE=1 is IGNORED; the run names it once. build_context runs per member,
    // so each panel member's preview carries its own copy (like the plugin's per-process warning).
    let ignored_hooks_warning = c3_core::test_hooks::ignored_hooks_warning();
    if !ignored_hooks_warning.is_empty() {
        run_warnings.push(ignored_hooks_warning);
    }
    // (wave 28b, D10 / F36-5) test mode never goes unnoticed: a run that finds
    // CODEX_CONSULT_TEST_MODE=1 says so on the console (a real run with its output, a dry run with
    // its warnings) and once in warnings[]; no engine child gets the test-mode variables
    // (`engines::scrub_host_markers`).
    if let Some(w) = c3_core::test_hooks::test_mode_warning() {
        run_warnings.push(w.to_string());
    }
    // (wave 27c, D11/D12) a coordinator that parses but names no seat is SAID, not refused: a
    // roster position with no seat here warns and the run goes on; a coordinator no reviewer can
    // match warns and the ledger records `coordinator.in_roster: false`.
    if let Some(pos) = &coordinator.unresolved {
        run_warnings.push(format!(
            "CODEX_CONSULT_COORDINATOR '{pos}' names no roster position here"
        ));
    }
    // (wave 27c, D11) a coordinator no roster entry matches is SAID on the console of a real run
    // (printed in `run_live`, matching the plugin's `Write-Host`), and recorded in the ledger's
    // `coordinator.in_roster: false` — it is NOT a warning and does not appear in warnings[].
    let mut extra_config_source = if r.extra_config.is_empty() {
        String::new()
    } else {
        "-CodexConfig".to_string()
    };

    // A panel member takes its identity from the roster entry its spec names (never a walk).
    if let Some(m) = member {
        identity_provider = m.provider.clone();
        if identity_model.is_empty() {
            identity_model = m.model.clone();
        }
        engine_name = if m.engine.is_empty() {
            "codex".to_string()
        } else {
            m.engine.clone()
        };
        engine_from = "roster".into();
        provider_source_override = "roster".into();
        roster_skipped = member_skips(&m.skipped);
    }

    if roster.exists {
        if let Some(m) = member {
            // The roster entry the panel run selected for this member (`roster_rule = 'panel'`);
            // a roster that changed under the panel refuses (`codex-consult.ps1:2940`).
            roster_rule = "panel".into();
            let member_engine = engine_name.clone();
            roster_entry = roster
                .entries
                .iter()
                .find(|e| e.position as i64 == m.roster_position)
                .cloned();
            let matches = roster_entry
                .as_ref()
                .map(|e| {
                    e.provider == m.provider
                        && e.model == m.model
                        && entry_engine(e) == member_engine
                })
                .unwrap_or(false);
            if !matches {
                let suffix = if member_engine != "codex" {
                    format!(" [{member_engine}]")
                } else {
                    String::new()
                };
                return Err((
                    format!(
                        "the reviewer roster '{}' changed while the panel ran (entry {} is no longer {} {}{suffix}); this panel member was not started.",
                        roster.path, m.roster_position, m.provider, m.model
                    ),
                    1,
                ));
            }
            if let Some(e) = &roster_entry {
                if o.model.is_empty() && !e.model.is_empty() {
                    identity_model = e.model.clone();
                    model_source_override = "roster".into();
                    roster_applied.push("model".into());
                }
            }
        } else if !o.provider.is_empty() {
            roster_rule = "provider".into();
            roster_entry = providers::find_roster_entry(&roster, &o.provider, &o.model).cloned();
            if let Some(e) = &roster_entry {
                let ee = entry_engine(e);
                if !o.engine.is_empty() && ee != o.engine {
                    return Err((format!(
                        "-Engine {}: the roster entry {} for -Provider {} is engine {ee} (a provider label names one engine); drop -Engine, or use another label",
                        o.engine, e.position, o.provider
                    ), 1));
                }
                if o.engine.is_empty() {
                    engine_name = ee;
                    engine_from = "roster".into();
                    if e.engine_declared {
                        roster_applied.push("engine".into());
                    }
                }
                if !o.model.is_empty() {
                    // model given: nothing applied from the roster's model
                } else if !e.model.is_empty() {
                    identity_model = e.model.clone();
                    model_source_override = "roster".into();
                    roster_applied.push("model".into());
                }
                let same = roster
                    .entries
                    .iter()
                    .filter(|x| x.provider == o.provider)
                    .count();
                if o.model.is_empty() && same > 1 {
                    let shown = if !e.model.is_empty() {
                        c3_core::lineage::format_reviewer_lineage(&e.provider, &e.model, &e.engine)
                    } else {
                        format!("{} (config model)", e.provider)
                    };
                    run_warnings.push(format!(
                        "roster: label {} names {same} entries; the first ({shown}) is used - pass -Model for another",
                        o.provider
                    ));
                }
            }
        } else if !o.thread.trim().is_empty() {
            roster_rule = "thread".into();
            if let Some(rev) = ledger_entries
                .iter()
                .rev()
                .find(|e| e.thread.trim() == o.thread.trim())
                .map(|e| &e.reviewer)
            {
                let rev_engine = if rev.engine.is_empty() {
                    "codex".to_string()
                } else {
                    rev.engine.clone()
                };
                if !o.engine.is_empty() && rev_engine != o.engine {
                    return Err((format!(
                        "-Engine {}: thread {} belongs to engine {rev_engine} ({}); a thread never changes engine - drop -Engine, or use -Mode new",
                        o.engine, o.thread, entry_reviewer_lineage(rev)
                    ), 1));
                }
                engine_name = rev_engine;
                engine_from = "-Thread".into();
                identity_provider = rev.provider.clone();
                provider_source_override = "-Thread".into();
                if o.model.is_empty() {
                    identity_model = rev.model.clone();
                    model_source_override = "-Thread".into();
                }
            }
            roster_entry =
                providers::find_roster_entry(&roster, &identity_provider, &identity_model).cloned();
        } else {
            roster_rule = "walk".into();
            let walk_ctx = providers::Ctx::for_consult(
                config.clone(),
                providers::read_all_task_consults_health(&collab_root),
                roster.clone(),
                launcher.clone(),
                openai_base_url.clone(),
                utc_now,
            );
            if !engine_exe_engine.is_empty() {
                walk_ctx.seed_engine_launcher(&engine_exe_engine, &engine_exe_launcher);
            }
            let walk =
                walk_ctx.walk_full_ctx(&o.model, &o.engine, o.skip_preflight, prompt_estimate);
            if !walk.error.is_empty() {
                return Err((walk.error, 1));
            }
            let e = walk.entry.expect("an available walk has an entry");
            roster_skipped = walk.skipped;
            identity_provider = e.provider.clone();
            provider_source_override = "roster".into();
            engine_name = entry_engine(&e);
            if o.engine.is_empty() {
                engine_from = "roster".into();
                if e.engine_declared {
                    roster_applied.push("engine".into());
                }
            }
            if o.model.is_empty() && !e.model.is_empty() {
                identity_model = e.model.clone();
                model_source_override = "roster".into();
                roster_applied.push("model".into());
            }
            roster_entry = Some(e);
        }
        // codex_config from the entry when -CodexConfig is empty.
        if let Some(e) = &roster_entry {
            if r.extra_config.is_empty() && !e.codex_config.is_empty() {
                r.extra_config = e.codex_config.clone();
                extra_config_source = "roster".into();
                roster_applied.push("codex_config".into());
            }
        }
        // (wave 26b, D11/D12) the matched entry's own timeout_sec / stall_sec. An explicit
        // --timeout-sec / --stall-sec wins (a panel member inherits the panel run's resolution);
        // timeout_sec replaces the purpose default and continue_sec follows it.
        if let Some(e) = &roster_entry {
            if r.timeout_source == "purpose" && e.timeout_sec >= 60 {
                r.timeout_sec = e.timeout_sec;
                r.timeout_source = "roster".into();
                if !o.continue_sec_given {
                    r.continue_sec = r.timeout_sec.min(900);
                }
                roster_applied.push("timeout_sec".into());
            }
            if !r.stall_given && e.stall_sec >= 0 {
                stall_sec = e.stall_sec;
                roster_applied.push("stall_sec".into());
            }
        }
    }

    // (wave 26, D7) -Require on a single run (with -Provider): every required reviewer must be
    // available by the roster walk's verdict (stricter than a plain -Provider run: a usage limit
    // without a reset time is out) - else the run is refused before anything starts, exit 5 (a dry
    // run too).
    let mut single_required: Vec<i64> = Vec::new();
    if require_given && !(o.panel || o.panel_all) && member.is_none() {
        if !roster.exists {
            return Err((
                format!(
                    "-Require names reviewers of the roster, and there is no reviewer roster{}.",
                    if roster.disabled {
                        " (CODEX_CONSULT_ROSTER=none)"
                    } else {
                        ""
                    }
                ),
                1,
            ));
        }
        let required = crate::panel::plan::resolve_required_reviewers(
            &roster, &o.require, &o.purpose, true, false,
        );
        if !required.error.is_empty() {
            return Err((format!("{}.", required.error), 1));
        }
        if !required.positions.is_empty() {
            // (wave 2e, F11-3) the FULL roster: a plan's quota is judged over every route of the
            // plan (a usage limit on an entry that is not required still puts its plan out); only
            // the required entries are judged. (F11-2) The launcher -EngineExe resolved seeds the
            // context, as for the roster walk.
            let req_ctx = providers::Ctx::for_consult(
                config.clone(),
                providers::read_all_task_consults_health(&collab_root),
                roster.clone(),
                launcher.clone(),
                openai_base_url.clone(),
                utc_now,
            );
            if !engine_exe_engine.is_empty() {
                req_ctx.seed_engine_launcher(&engine_exe_engine, &engine_exe_launcher);
            }
            let sel = req_ctx.panel_members_of(
                Some(&required.positions),
                "",
                "",
                &o.purpose,
                true,
                false,
                0,
            );
            let out: Vec<String> = sel
                .members
                .iter()
                .filter(|m| m.state != "run")
                .map(|m| providers::format_required_outage(m, utc_now))
                .collect();
            if !out.is_empty() {
                let plural = out.len() != 1;
                return Err((
                    format!(
                        "required reviewer{} not available (-Require, judged like the roster walk): {}; nothing was started - wait for {}, or run without -Require (exit 5).",
                        if plural { "s" } else { "" },
                        out.join("; "),
                        if plural { "them" } else { "it" }
                    ),
                    5,
                ));
            }
            single_required = required.positions.clone();
        }
    }

    // Finalize the engine (`codex-consult.ps1:2798`): default codex, its spec, its launcher and
    // harness. The engine launcher is the codex launcher for codex; otherwise the resolved
    // engine launcher (the `-EngineExe` seed when it bound this engine, else PATH/install).
    if engine_name.is_empty() {
        engine_name = "codex".into();
    }
    if engine_from.is_empty() {
        engine_from = "default".into();
    }
    let is_codex = engine_name == "codex";
    let spec = c3_core::lineage::engine_spec(&engine_name)
        .ok_or_else(|| (format!("unknown engine '{engine_name}'"), 1))?;
    // (M11) `--peer`/`--peers` bring federation peers into the reviewer PACK. Only the http engine
    // builds a pack; codex/agy/muse read the repository through their own tools, so the flags are
    // refused before launch — the peers of the index reach a reviewer only through a pack.
    if let Some(v) = &o.peers {
        let t = v.trim();
        if !t.is_empty() && !t.eq_ignore_ascii_case("all") {
            return Err(("-Peers accepts only 'all'".to_string(), 1));
        }
    }
    let peers_requested = !o.peer.is_empty()
        || o.peers
            .as_deref()
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
    if peers_requested && engine_name != "http" {
        return Err((
            format!(
                "-Peer/-Peers apply only to the http engine, which builds the reviewer pack; the {engine_name} engine reads the repository through its own tools, so the peers of the index reach a reviewer only through a pack."
            ),
            1,
        ));
    }
    let engine_launcher = if is_codex {
        launcher.clone()
    } else if !engine_exe_engine.is_empty() && engine_exe_engine == engine_name {
        engine_exe_launcher.clone()
    } else {
        providers::resolve_engine_launcher(&engine_name, "")
            .unwrap_or(None)
            .unwrap_or_default()
    };
    let harness = if is_codex {
        format!(
            "codex-cli {}",
            get_codex_version(&launcher).replace("codex-cli ", "")
        )
    } else {
        providers::engine_harness(&engine_name, &engine_launcher)
    };
    let sandbox_record = if is_codex {
        sandbox_label(&o)
    } else {
        spec.sandbox_record.to_string()
    };

    let mut identity = resolve_reviewer_identity(
        &config,
        &identity_provider,
        &identity_model,
        &openai_base_url,
        &engine_name,
        &engine_launcher,
    );
    if !provider_source_override.is_empty() {
        identity.provider_source = provider_source_override.clone();
    }
    if !model_source_override.is_empty() {
        identity.model_source = model_source_override.clone();
    }

    // (wave 27) The reviewer being consulted IS the coordinator's own model
    // (`Test-CoordinatorReviewer`/`Format-CoordinatorWarning`): a second opinion, not an
    // independent one. A warning, never a refusal.
    if let Some(w) = c3_core::host::coordinator_reviewer_warning(
        &coordinator,
        &identity.provider,
        &identity.model,
        &engine_name,
        &identity.lineage,
    ) {
        run_warnings.push(w);
    }

    // An EXPLICIT `-Provider` whose table is unusable is refused up front with the scanner's
    // reason (`Resolve-ReviewerIdentity`'s `Stop-WithError`), for a dry run too - the reviewer
    // the caller named cannot be used (`identity.error` is only set for an explicit -Provider).
    if !identity.error.is_empty() {
        return Err((identity.error.clone(), 1));
    }
    // A resolved identity is required for fork/resume (F02-1).
    if !identity.resolved && (o.mode == "fork" || o.mode == "resume") {
        return Err((
            format!(
                "provider identity could not be resolved ({}); pass -Provider and -Model explicitly, or use -Mode new",
                if identity.note.is_empty() {
                    identity.error.clone()
                } else {
                    identity.note.clone()
                }
            ),
            1,
        ));
    }
    // `-Provider` without a model that neither `-Model` nor the roster supplied.
    if !o.provider.is_empty() && identity_model.is_empty() {
        return Err((
            format!("-Provider needs -Model: the bridge cannot know which model a provider serves by default (e.g. -Provider {} -Model <model>).", o.provider),
            1,
        ));
    }

    // What a non-codex engine does not support is refused with one message each, after the
    // identity is resolved (`codex-consult.ps1:2804-2830`). The effective mode carries the agy/
    // muse default (`new`, or `resume` with a thread) so the parent walk never forks an engine.
    let mut mode = o.mode.clone();
    if !is_codex {
        if o.mode == "fork" {
            return Err((
                format!("the {engine_name} engine has no fork; use -Mode resume or new."),
                1,
            ));
        }
        if !o.sandbox.is_empty() && o.sandbox != "read-only" {
            return Err((format!(
                "-Sandbox {} is refused for the {engine_name} engine: consultations are read-only there ({}).",
                o.sandbox, spec.read_only_note
            ), 1));
        }
        if o.codex_config.iter().any(|c| !c.trim().is_empty()) {
            return Err((format!(
                "-CodexConfig does not apply to the {engine_name} engine (it configures codex exec).",
            ), 1));
        }
        if !r.transport_override.is_empty()
            && !spec.transports.contains(&r.transport_override.as_str())
        {
            return Err((format!(
                "-SchemaTransport {} is refused for the {engine_name} engine: it takes {} (native = the schema is passed as {}).",
                r.transport_override, spec.transports.join(" or "), spec.schema_flag
            ), 1));
        }
        if o.mode.is_empty() {
            mode = if !o.thread.trim().is_empty() {
                "resume".to_string()
            } else {
                spec.default_mode.to_string()
            };
        }
    } else if !r.transport_override.is_empty()
        && !spec.transports.contains(&r.transport_override.as_str())
    {
        return Err((format!(
            "-SchemaTransport {} is for the agy and muse engines; codex takes output-schema or prompt-only.",
            r.transport_override
        ), 1));
    }
    // -MaxModelSteps: only an engine with a model-step cap (muse, D9); a panel member of another
    // engine drops it (the panel passes it to every member; the members whose engine has a cap
    // use it - the plugin's `if ($panelMember) { $MaxModelSteps = 0 }`).
    if o.max_model_steps > 0 && spec.steps_flag.is_empty() {
        if member.is_some() {
            o.max_model_steps = 0;
        } else {
            return Err((format!(
                "-MaxModelSteps is for the muse engine (--max-model-steps); the {engine_name} engine has no model-step cap.",
            ), 1));
        }
    }

    // (M7b-b) --key-env / --base-url configure the http engine only; on an http run --base-url
    // must be https:// and the reviewer needs a provider label (from -Provider or the roster).
    if (!o.key_env.trim().is_empty() || !o.base_url.trim().is_empty() || o.pack_budget != -1)
        && engine_name != "http"
    {
        return Err((format!(
            "--key-env / --base-url / --pack-budget configure the http engine only (this run's engine is {engine_name})."
        ), 1));
    }
    if engine_name == "http" {
        // (S3) `--key-env` / `--base-url` are validated with the same S1/S2 rules the roster
        // parser uses: the base URL is parsed strictly and the key may go only to the host it
        // belongs to. A roster-matched seat's config is already validated at load, so these
        // checks bind the direct-run flags (and their defaults). Refused in the pre-launch style.
        if !o.key_env.trim().is_empty() {
            // (S6) The same check the roster parser uses, with the same message that never echoes
            // the value (it may itself be a pasted key).
            c3_core::roster_ext::check_key_env_name(o.key_env.trim())
                .map_err(|why| (format!("--key-env: {why}"), 1))?;
        }
        let base_url = if o.base_url.trim().is_empty() {
            c3_core::roster_ext::DEFAULT_BASE_URL.to_string()
        } else {
            o.base_url.trim().to_string()
        };
        let key_env = if o.key_env.trim().is_empty() {
            c3_core::roster_ext::DEFAULT_KEY_ENV.to_string()
        } else {
            o.key_env.trim().to_string()
        };
        let host = c3_core::roster_ext::parse_base_url(&base_url)
            .map_err(|why| (format!("--base-url: {why}."), 1))?;
        // The proxy auth mode (`C3_HTTP_AUTH_PROXY` lists this host) sends no key at all, so there
        // is nothing to bind; every other host keeps the S1 binding unchanged.
        if !crate::http_engine::host_uses_proxy_auth(&host, &crate::http_engine::proxy_auth_hosts())
        {
            c3_core::roster_ext::check_key_host(&key_env, &host)
                .map_err(|why| (format!("{why}."), 1))?;
        }
        if o.pack_budget != -1
            && (o.pack_budget < 0 || o.pack_budget > c3_core::roster_ext::MAX_PACK_TOKENS)
        {
            return Err((
                format!(
                    "--pack-budget must be an integer from 0 to {} (0 = no periphery; got {}).",
                    c3_core::roster_ext::MAX_PACK_TOKENS,
                    o.pack_budget
                ),
                1,
            ));
        }
        if identity.provider.trim().is_empty() {
            return Err((
                "the http engine needs -Provider <label> (e.g. -Provider openrouter -Model openai/gpt-5), or an ext.c3.reviewers roster entry.".to_string(),
                1,
            ));
        }
    }
    // -EngineExe bound to one engine (D3): a run of another non-codex engine is refused.
    if !engine_exe_engine.is_empty() && !is_codex && engine_exe_engine != engine_name {
        return Err((format!(
            "-EngineExe names the {engine_exe_engine} launcher, but this run's engine is {engine_name} (from {engine_from}); pass -Engine {engine_name} with -EngineExe.",
        ), 1));
    }
    // The engine launch invariant (muse billing guard, D4): never bypassed by -SkipPreflight,
    // refused for a dry run too (nothing started).
    if engine_name == "muse" {
        let block = providers::get_muse_launch_block();
        if !block.is_empty() {
            return Err((
                format!("the {engine_name} engine is refused: {block}; nothing was started."),
                1,
            ));
        }
    }

    // Preflight (M2c: credentials only, no recorded endpoint-health/24h block). openai runs
    // `codex login status`; a non-openai provider's credential check is deferred (its
    // preflight is left unevaluated and never refuses). `-SkipPreflight` bypasses the check.
    let (preflight, mut preflight_refusal, preflight_warning, mut preflight_label) =
        if o.skip_preflight {
            // The check is skipped, but an active quota record still earns a warning (the plugin's
            // `Format-QuotaWarning`), printed before launch and recorded in the ledger.
            let warning = if identity.resolved {
                let consults = providers::read_all_task_consults_health(&collab_root);
                let health =
                    c3_core::health::endpoint_health(&consults, &identity.fingerprint, utc_now);
                format_quota_warning(&identity, &health)
            } else {
                String::new()
            };
            (
                "skipped".to_string(),
                None,
                warning,
                "skipped (-SkipPreflight)".to_string(),
            )
        } else {
            let anon = roster_entry
                .as_ref()
                .map(|e| e.auth == "none")
                .unwrap_or(false);
            let plan_check = roster_entry.as_ref().map(|e| PlanCheck {
                roster: &roster,
                entry: e,
                codex_launcher: &launcher,
                openai_base_url: &openai_base_url,
            });
            let (p, refusal, label, probe_warning) = resolve_preflight(
                &identity,
                &engine_launcher,
                &config,
                &collab_root,
                anon,
                plan_check,
            );
            // (D4) a skipped launcher probe warns (deduped), in both the dry-run preview and the
            // real run's ledger warnings[].
            if let Some(w) = probe_warning {
                if !run_warnings.contains(&w) {
                    run_warnings.push(w);
                }
            }
            (p, refusal, String::new(), label)
        };

    // When a `-Thread` run's endpoint is unavailable, name the reviewer a new thread would get
    // (`codex-consult.ps1:2910`).
    if roster_rule == "thread" {
        if let Some((refusal, code)) = preflight_refusal.take() {
            let alt = providers::Ctx::for_consult(
                config.clone(),
                providers::read_all_task_consults_health(&collab_root),
                roster.clone(),
                launcher.clone(),
                openai_base_url.clone(),
                utc_now,
            )
            .walk_full("", "", false);
            let hint = if let Some(aid) = &alt.identity {
                format!(
                    "; to continue with another reviewer, start a new thread: -Mode new; the roster would select {}",
                    c3_core::lineage::format_reviewer_lineage(&aid.provider, &aid.model, &aid.engine)
                )
            } else {
                "; the roster has no available reviewer for a new thread either".to_string()
            };
            preflight_refusal = Some((format!("{refusal}{hint}"), code));
            preflight_label.push_str(&hint);
        }
    }

    let effort = effort_plan(
        &identity,
        effort_requested(&o, &r).as_str(),
        &o.native_effort,
    );
    // An effort-vocabulary refusal is a real error only for a RESOLVED identity (an undeclared
    // host, F02-4). When the identity itself is unresolved (a bad config, an unusable provider
    // table), the identity/preflight refusal — or, on a dry run, the rendered plan — must
    // surface instead of the effort error (which would just report the unknown host). The
    // plugin plans effort only after the identity is confirmed usable.
    if !effort.error.is_empty() && identity.resolved {
        return Err((effort.error, 1));
    }
    let transport = resolve_transport(&identity, &r);

    // Peak window (evaluated once now — the early check). A malformed schedule refuses (naming
    // the variable and the bad token); `-OffPeakOnly` refuses with no schedule or inside the
    // window. The status that the LEDGER records is re-evaluated right before launch
    // (`run_live`), which may cross a window boundary. `peakProvider` is the resolved provider,
    // or `""` when the identity is unresolved.
    let peak_provider = if identity.provider_source.is_empty() {
        String::new()
    } else {
        identity.provider.clone()
    };
    let peak = c3_core::peak::peak_status_now(&peak_provider, 0);
    if !peak.error.is_empty() {
        return Err((peak.error, 1));
    }
    if o.off_peak_only {
        if peak_provider.is_empty() {
            return Err((format!(
                "no schedule for provider unknown (the reviewer identity is unresolved: {}); -OffPeakOnly needs a known provider and its CODEX_CONSULT_PEAK_<PROVIDER>.",
                identity.note
            ), 1));
        }
        if peak.peak.is_none() {
            return Err((
                format!(
                    "no schedule for provider {peak_provider}; -OffPeakOnly needs {}.",
                    peak.variable
                ),
                1,
            ));
        }
        if peak.peak == Some(true) {
            return Err((format!(
                "-OffPeakOnly: {peak_provider} is inside its peak window ({}; now {}); nothing was started.",
                peak.schedule, peak.local
            ), 1));
        }
    }
    let (peak_warning, peak_label) = peak_display(&peak_provider, &peak);

    // Parent-thread walk (`Select-ParentThread`): validate `-Thread` against this task's ledger
    // by lineage + provenance, or resolve the automatic parent (the newest verified thread of
    // this lineage) for `-Mode fork|resume`. A refusal here happens before the lock (nothing
    // started). `-Thread needs -Mode fork or resume` is already refused in `args::validate`.
    let parent = match select_parent_thread(&ledger_entries, &identity, &mode, &o.thread) {
        Ok(p) => p,
        Err(refusal) => return Err((refusal, 1)),
    };
    let mut effective_mode = parent.mode.clone();
    let mut parent_thread = parent.parent_thread.clone();
    let parent_note = parent.note.clone();
    // (wave 26b, D16) the reviewer's context window (its roster entry's context_tokens; 0 = none).
    let context_tokens = roster_entry.as_ref().map(|e| e.context_tokens).unwrap_or(0);
    // (wave 28b, D15) the window reaches the ENGINE: a codex reviewer with a roster context_tokens n
    // gets `-c model_context_window=<n>` and `-c model_auto_compact_token_limit=<floor 0.8 n>` on
    // every turn - unless `-CodexConfig` or the entry's `codex_config` already sets that key (the
    // operator's value wins). The ledger records `context_window` {tokens, auto_compact_limit, items}
    // (null without one); agy and muse take no such option.
    let (context_config, context_window) =
        context_window_config(context_tokens, is_codex, &r.extra_config);

    let nn_n = store.next_numbers(&task).map_err(|e| (e.to_string(), 1))?;
    let (mut nn, mut consult_n) = (nn_n.nn, nn_n.n);
    // A panel member takes its numbers from its spec, not the disk (the panel run reserved them
    // up front); `next_numbers` still ran above to refuse an unusable store.
    if let Some(m) = member {
        nn = m.nn as u32;
        consult_n = m.n;
    }

    let reply_name = if o.reply_name.is_empty() {
        "reply".to_string()
    } else {
        o.reply_name.clone()
    };
    let handoffs_dir = collab_root.join(task.as_str()).join("handoffs");
    // The handoff file prefix is the engine's (`capabilities(kind).file_prefix` / the spec).
    let file_prefix = spec.prefix;
    let stem = format!("{:02}-{}-{}", nn, file_prefix, reply_name);
    let reply_path = handoffs_dir.join(format!("{stem}.md"));
    let reply_json_path = handoffs_dir.join(format!("{stem}.reply.json"));
    let events_path = handoffs_dir.join(format!("{stem}.events.jsonl"));
    // The `-o` last-message file and the stderr sidecar are system-temp files named exactly
    // like the plugin (`<temp>/codex-consult-last-<guidN>.md` / `-stderr-<guidN>.txt`), not
    // handoff files: they are transient and removed after the run.
    let tmp_id = uuid::Uuid::new_v4().simple().to_string();
    let tmp_root = std::env::temp_dir();
    let last_msg_path = tmp_root.join(format!("codex-consult-last-{tmp_id}.md"));
    let stderr_path = tmp_root.join(format!("codex-consult-stderr-{tmp_id}.txt"));
    // muse takes its prompt through `--prompt-file` (D1): a temp file named exactly like the
    // plugin (`<temp>/codex-consult-prompt-<guidN>.txt`); other engines read stdin.
    let prompt_file: Option<PathBuf> = if spec.prompt_by_file {
        Some(tmp_root.join(format!("codex-consult-prompt-{tmp_id}.txt")))
    } else {
        None
    };

    // Brief ref (repo-relative) + existence check.
    let mut brief_ref = String::new();
    let mut brief_path = None;
    if !o.brief.is_empty() {
        let bp = if Path::new(&o.brief).is_absolute() {
            PathBuf::from(&o.brief)
        } else {
            cwd.join(&o.brief)
        };
        if !bp.is_file() {
            return Err((
                format!(
                    "brief '{}' not found (this script never writes briefs; write it first).",
                    o.brief
                ),
                1,
            ));
        }
        brief_ref =
            c3_core::paths::repo_relative(&repo_root, &bp).unwrap_or_else(|| o.brief.clone());
        brief_path = Some(bp);
    }
    let brief_sha = brief_path
        .as_ref()
        .and_then(|p| std::fs::read(p).ok())
        .map(|b| c3_core::sha256_hex(&b))
        .unwrap_or_default();

    // (wave 26b, D16 b) a single run's explicit reviewer (-Provider / -Thread) whose context
    // window the new prompt alone would fill beyond 80% is refused before anything starts (a
    // roster walk skips it instead; a panel member is skipped at selection).
    if member.is_none() && roster_rule != "walk" && context_tokens > 0 {
        if let Some(e) = &roster_entry {
            if (prompt_estimate as f64) > 0.8 * context_tokens as f64 {
                return Err((
                    format!(
                        "brief too large for this reviewer's context (est. {prompt_estimate} of {context_tokens} tokens) - roster entry #{}; nothing was started (a shorter brief, or another reviewer).",
                        e.position
                    ),
                    1,
                ));
            }
        }
    }
    // (wave 26b, D16) the fork/resume -> new fallback when the continued thread's last context
    // plus this prompt would exceed 80% of the reviewer's context window.
    let mut mode_fallback: Option<c3_core::ledger::ModeFallback> = None;
    let mut prev_reply_line = String::new();
    if context_tokens > 0
        && (effective_mode == "fork" || effective_mode == "resume")
        && !parent_thread.is_empty()
    {
        let parent_entry = ledger_entries.iter().rfind(|e| e.thread == parent_thread);
        let prior_tokens = parent_entry
            .and_then(|e| e.usage.as_ref())
            .map(|u| u.input_tokens)
            .unwrap_or(0);
        if (prior_tokens + prompt_estimate) as f64 > 0.8 * context_tokens as f64 {
            let reason = format!(
                "the {effective_mode} thread {parent_thread} last carried {prior_tokens} tokens; with this prompt (est. {prompt_estimate}) that exceeds 80% of the reviewer's context window ({context_tokens} tokens)"
            );
            println!("codex-consult: mode {effective_mode} -> new: {reason}");
            let prev_reply = parent_entry
                .map(|e| e.reply.clone())
                .filter(|s| !s.is_empty())
                .map(|reply| {
                    format!(
                        "{}/{}/{}",
                        o.collab_dir.trim_end_matches(['/', '\\']),
                        task.as_str(),
                        reply
                    )
                    .replace('\\', "/")
                })
                .unwrap_or_default();
            if !prev_reply.is_empty() {
                prev_reply_line = format!(
                    "Your previous reply in this task is `{prev_reply}`: this consultation starts a new thread because the previous one is too large for your context window - re-read that reply if you need your earlier review."
                );
            }
            mode_fallback = Some(c3_core::ledger::ModeFallback {
                from: effective_mode.clone(),
                to: "new".into(),
                reason,
                extra: Default::default(),
            });
            effective_mode = "new".into();
            parent_thread = String::new();
        }
    }
    // (wave 26b, D16) the context-window line the reviewer is told, right after the ask.
    let context_line = if context_tokens > 0 {
        format!("Your context window is {context_tokens} tokens: read only what the brief points to; prefer targeted reads.")
    } else {
        String::new()
    };

    // -Artifact: resolve each path (absolute, or relative to the cwd / repo root), refuse a
    // missing one (`Resolve-ArtifactPaths`; a review cannot be bound to a file that is not
    // there), and hash it now (`Get-ArtifactHashes`). The after-run rehash reads `full` (never
    // a lookup by name, so case-distinct paths stay two files).
    let artifacts = match resolve_artifacts(&o.artifacts, &cwd, &repo_root) {
        Ok(a) => a,
        Err(msg) => return Err((msg, 1)),
    };

    // The reply schema path (shipped with the tool; in prompt-only its text is inlined).
    let schema_path = if r.raw { None } else { schema_file(&repo_root) };
    let schema_text = if transport.transport == "prompt-only" {
        // The embedded schema, trimmed and normalised to CRLF exactly as the plugin inlines
        // its on-disk file (`.Trim() -replace "`r`n","`n" -replace "`n", $nl`).
        c3_core::schema::REPLY_SCHEMA_V1
            .trim()
            .replace("\r\n", "\n")
            .replace('\n', "\r\n")
    } else {
        String::new()
    };

    // Open findings snapshot. A panel member lists only what was open when the panel started —
    // the same set for every member (`listed_ids`), never a sibling's mid-wave answer.
    let (mut open_findings, mut open_findings_count) = read_open_findings(&store, &task);
    if let Some(m) = member {
        open_findings.retain(|f| m.listed_ids.contains(&f.id));
        open_findings_count = open_findings.len();
    }

    let mut consult_id = uuid::Uuid::new_v4().to_string();
    // (0.6.1, U5) consult_ref: a SECOND random 128-bit id of this consultation, derived from nothing
    // (not from the consult_id, which the reviewer sees in the prompt, nor from anything local) - the
    // telemetry events carry it. Minted by the process that commits the entry (a panel member its own).
    let consult_ref = uuid::Uuid::new_v4().to_string().to_ascii_lowercase();
    // A member reuses the consult id its spec reserved (the prompt's last line, so the parent's
    // rollout scan and the reserved pending record all agree), when it is a well-formed uuid.
    if let Some(m) = member {
        if is_uuid36(&m.consult_id) {
            consult_id = m.consult_id.clone();
        }
    }

    // Range (`git diff --shortstat <spec> --`, measured once, before the lock). An unknown
    // range refuses here (nothing started). The counts go to the prompt, the ledger `range{}`
    // record and — over 1500 lines under a sub-2400 s timeout — a size warning.
    let mut range_record: Option<RangeRecord> = None;
    let mut range_text = String::new();
    let mut prompt_range: Option<prompt::Range> = None;
    if !o.range.is_empty() {
        let rs = revision::range_stat(&repo_root, &o.range);
        if !rs.error.is_empty() {
            return Err((format!("{}; nothing was started.", rs.error), 1));
        }
        range_text = revision::range_text(rs.files, rs.lines);
        if rs.lines > RANGE_WARN_LINES && r.timeout_sec < RANGE_WARN_TIMEOUT {
            run_warnings.push(format!(
                "a range of {} lines with a {} s timeout: pass -TimeoutSec or a reading plan in the brief",
                rs.lines, r.timeout_sec
            ));
        }
        prompt_range = Some(prompt::Range {
            spec: o.range.clone(),
            text: range_text.clone(),
            insertions: rs.insertions,
            deletions: rs.deletions,
        });
        range_record = Some(RangeRecord {
            spec: o.range.clone(),
            files: rs.files,
            insertions: rs.insertions,
            deletions: rs.deletions,
            lines: rs.lines,
            ..Default::default()
        });
    }

    // A panel member surfaces the panel-level warnings (`codex-consult.ps1:2928`) among its own
    // run warnings, printed before launch and recorded in the ledger.
    if let Some(m) = member {
        for w in &m.panel_warnings {
            if !w.is_empty() {
                run_warnings.push(w.clone());
            }
        }
    }

    // A panel member's role paragraph (`Resolve-RoleFile` + `role_prompt_line`), after the ask
    // and before the brief. A bad/unknown role refuses (naming the member); no role → empty.
    let mut role_line = String::new();
    let mut roles_note = String::new();
    let mut role_info: Option<crate::panel::roles::RoleInfo> = None;
    // (wave 26, R16) the role block - <CollabDir>/roles/<name>.md, else the plugin's
    // templates/role-<name>.md - is resolved now: an unknown role refuses the run, nothing started.
    // Without a known plugin root (no CLAUDE_PLUGIN_ROOT, no scripts dir, no plugin directory
    // beside the binary) only a safety problem refuses: the built-in roles cannot be told apart
    // from unknown ones there, and the role is left out.
    let role_name = match member {
        Some(m) => {
            roles_note = m.roles_note.clone();
            m.role.clone()
        }
        None => o.role.clone(),
    };
    if !role_name.is_empty() {
        let plugin_root = crate::panel::roles::plugin_root();
        let ri = crate::panel::roles::resolve_role_file(&role_name, &collab_root, &plugin_root);
        if !ri.error.is_empty()
            && (!plugin_root.is_empty() || crate::panel::roles::is_role_refusal(&ri.error))
        {
            let tail = if member.is_some() {
                "; this panel member was not started"
            } else {
                ""
            };
            return Err((format!("-Role: {}{tail}.", ri.error), 1));
        }
        if ri.error.is_empty() {
            role_line = crate::panel::roles::role_prompt_line(&ri);
            role_info = Some(ri);
        }
    }

    let prompt_text = prompt::assemble(&PromptInputs {
        raw: r.raw,
        purpose: &o.purpose,
        prompt: &o.prompt,
        brief_ref: &brief_ref,
        range: prompt_range.as_ref(),
        open_findings: &open_findings,
        schema_transport: &transport.transport,
        schema_text: &schema_text,
        max_words: r.max_words,
        consult_id: &consult_id,
        tools_line: spec.tools_line,
        role_line: &role_line,
        context_line: &context_line,
        prev_reply_line: &prev_reply_line,
        reread_line: &prompt::reread_line(context_tokens, &brief_ref, &o.prompt),
    });

    // The argv (byte-identical to the core plan): build the Request and plan it for the
    // selected engine.
    let engine_kind = engine_kind_of(&engine_name);
    let request = make_request(
        &o,
        &r,
        &identity,
        &effort,
        &transport,
        &prompt_text,
        &schema_path,
        &last_msg_path,
        resolved_mode(&effective_mode, &parent_thread, &identity),
        engine_kind,
        prompt_file.as_deref(),
        &context_config,
    );
    let argv = match c3_core::engine::SubprocessEngine::new(engine_kind).plan(&request) {
        Ok(c3_core::engine::LaunchPlan::Subprocess(a)) => a,
        // The http engine has no subprocess argv: it sends one OpenAI-compatible request built
        // from a reviewer pack (M7b). Its command line and dry-run block are rendered by
        // `consult::http` from the request plan (the `Authorization` header redacted), so the
        // Context carries an empty argv here.
        Ok(c3_core::engine::LaunchPlan::Http(_)) => c3_core::engine::Argv {
            command: String::new(),
            args: Vec::new(),
        },
        Err(_) => return Err((format!("could not plan the {engine_name} argv"), 1)),
    };
    let argv_display = argv.to_command_string();

    let codex_version = get_codex_version(&launcher);
    let revision = revision::revision_info(&repo_root, Some(&collab_root));

    // Recovery records: a dry run only reports them (`pending :` lines); a real run reads and
    // acts on them under the lock (`run_live`). An unusable record refuses even a dry run.
    let mut recovery_dry_lines: Vec<String> = Vec::new();
    if o.dry_run {
        let rec = super::recovery::assess(&store, &task);
        if let Some(err) = rec.error {
            return Err((err, 1));
        }
        recovery_dry_lines = rec.items.iter().map(super::recovery::dry_line).collect();
    }

    // The roster decision line (console/dry-run/handoff) and the `roster{}` ledger record
    // (`None` without a roster file). `codex-consult.ps1:2920-2949`.
    let mut roster_line = String::new();
    let mut roster_record: Option<c3_core::ledger::RosterRef> = None;
    if roster.exists {
        let count = roster.entries.len();
        let position = roster_entry.as_ref().map(|e| e.position as i64);
        let applied_text = if roster_applied.is_empty() {
            "nothing applied".to_string()
        } else {
            format!("{} applied", roster_applied.join(", "))
        };
        let lineage_shown = c3_core::lineage::format_reviewer_lineage(
            &identity.provider,
            &identity.model,
            &engine_name,
        );
        roster_line = match roster_rule.as_str() {
            // A panel member's roster decision is the panel run's; its own console line is not the
            // `codex-consult:` line the parent collects, so it is left unprinted here.
            "panel" => String::new(),
            "walk" => {
                let mut l = format!(
                    "Roster: {} - position {} of {count}",
                    roster.path,
                    position.map(|p| p.to_string()).unwrap_or_default()
                );
                if o.skip_preflight {
                    l.push_str(" (-SkipPreflight: taken unchecked)");
                }
                if !roster_skipped.is_empty() {
                    l.push_str(&format!(
                        "; skipped {}",
                        c3_core::availability::format_roster_skips(&roster_skipped)
                    ));
                }
                l
            }
            "provider" => {
                if let Some(pos) = position {
                    format!(
                        "Roster: {} - entry {pos} of {count} for -Provider {} ({applied_text})",
                        roster.path, o.provider
                    )
                } else {
                    format!(
                        "Roster: {} - no entry for -Provider {} (nothing applied)",
                        roster.path, o.provider
                    )
                }
            }
            _ => {
                if let Some(pos) = position {
                    format!(
                        "Roster: {} - entry {pos} of {count} for -Thread {}, {lineage_shown} ({applied_text})",
                        roster.path, o.thread
                    )
                } else {
                    format!(
                        "Roster: {} - no entry for -Thread {}, {lineage_shown} (nothing applied)",
                        roster.path, o.thread
                    )
                }
            }
        };
        roster_record = Some(c3_core::ledger::RosterRef {
            path: roster.path.clone(),
            position,
            skipped: roster_skipped
                .iter()
                .map(|(provider, model, engine, reason)| {
                    serde_json::json!({
                        "provider": provider,
                        "model": model,
                        "engine": if engine.is_empty() { "codex" } else { engine },
                        "reason": reason,
                    })
                })
                .collect(),
            applied: roster_applied
                .iter()
                .map(|a| serde_json::Value::String(a.clone()))
                .collect(),
            ..Default::default()
        });
    }

    let kick_path = collab_root
        .join(task.as_str())
        .join(format!(".consult.kick-{nn:02}"));
    Ok(Context {
        o,
        r,
        repo_root,
        collab_root,
        task,
        reply_name,
        nn,
        consult_n,
        consult_id,
        consult_ref,
        context_tokens,
        context_config,
        context_window,
        identity,
        coordinator,
        child_env_scrubbed,
        effort,
        transport,
        launcher,
        codex_version,
        engine: engine_name,
        file_prefix: file_prefix.to_string(),
        engine_from,
        engine_launcher,
        harness,
        sandbox_record,
        prompt_file,
        prompt_text,
        argv_display,
        argv: argv.args,
        brief_ref,
        brief_path,
        brief_sha,
        artifacts,
        schema_path,
        open_findings_count,
        effective_mode,
        parent_thread,
        parent_note,
        roster_line,
        roster_record,
        extra_config_source,
        preflight,
        preflight_label,
        preflight_warning,
        preflight_refusal,
        revision,
        range_record,
        range_text,
        run_warnings,
        peak_provider,
        peak: peak.peak,
        peak_schedule: peak.schedule.clone(),
        peak_source: peak.source.clone(),
        peak_evaluated_at: peak.evaluated_at.clone(),
        peak_warning,
        peak_label,
        telemetry_enabled: telemetry::is_enabled(&telemetry::Config {
            telemetry: o_telemetry,
        }),
        recovery_dry_lines,
        recovery_lines: Vec::new(),
        panel_member: member.cloned(),
        plan: roster_entry
            .as_ref()
            .map(|e| e.plan.clone())
            .unwrap_or_default(),
        role: role_info
            .as_ref()
            .map(|ri| ri.name.clone())
            .unwrap_or_else(|| role_name.clone()),
        role_info,
        single_required,
        roles_note,
        mode_fallback,
        stall_sec,
        kick_path,
        handoffs_dir,
        reply_path,
        reply_json_path,
        events_path,
        last_msg_path,
        stderr_path,
    })
}

/// `($peakWarning, $peakLabel)` for a peak status (`codex-consult.ps1:2625-2627`).
fn peak_display(provider: &str, st: &c3_core::peak::PeakStatus) -> (String, String) {
    let warning = if st.peak == Some(true) {
        format!(
            "{provider} peak window ({}) - this consultation runs at peak tariff.",
            st.schedule
        )
    } else {
        String::new()
    };
    let label = match st.peak {
        None => {
            if st.variable.is_empty() {
                "unknown (provider unknown)".to_string()
            } else {
                format!("unknown ({} not set)", st.variable)
            }
        }
        Some(true) => format!("PEAK ({}; now {})", st.schedule, st.local),
        Some(false) => format!(
            "off-peak ({}; now {}; {})",
            st.schedule, st.local, st.detail
        ),
    };
    (warning, label)
}

fn effort_requested(o: &Options, _r: &Resolved) -> String {
    if !o.effort.is_empty() {
        o.effort.clone()
    } else {
        prompt::presets::effort(&o.purpose).to_string()
    }
}

#[allow(clippy::too_many_arguments)]
fn make_request(
    o: &Options,
    r: &Resolved,
    id: &ReviewerIdentity,
    effort: &EffortPlan,
    transport: &Transport,
    prompt_text: &str,
    schema_path: &Option<PathBuf>,
    last_msg_path: &Path,
    mode: Mode,
    engine: EngineKind,
    prompt_file: Option<&Path>,
    context_config: &[String],
) -> Request {
    let sandbox = if o.sandbox.is_empty() {
        "read-only".to_string()
    } else {
        o.sandbox.clone()
    };
    // The schema flag is passed when the transport carries the schema natively: codex
    // `--output-schema`, agy `--json-schema` and muse `--output-schema` all key off a
    // non-`prompt-only`, non-raw transport (`output-schema` for codex, `native` for the engines).
    let schema_arg = if transport.transport == "output-schema" || transport.transport == "native" {
        schema_path.clone()
    } else {
        None
    };
    Request {
        prompt: prompt_text.to_string(),
        brief_path: None,
        model: if id.model_source == "unknown" {
            String::new()
        } else {
            id.model.clone()
        },
        provider: if id.provider_source.is_empty() {
            String::new()
        } else {
            id.provider.clone()
        },
        engine,
        effort: effort.sent.clone(),
        timeout_sec: r.timeout_sec as f64,
        mode,
        sandbox,
        schema_path: schema_arg,
        // (wave 28b, D15) the operator's items, then the context-window items (codex only)
        extra_config: with_context_config(&r.extra_config, context_config),
        output_last_message: if engine == EngineKind::Codex {
            Some(last_msg_path.to_path_buf())
        } else {
            None
        },
        prompt_file: prompt_file.map(|p| p.to_path_buf()),
        max_model_steps: if o.max_model_steps > 0 {
            Some(o.max_model_steps as u32)
        } else {
            None
        },
    }
}

/// Map the engine name to its [`EngineKind`] (unknown → codex; unknown is refused earlier).
fn engine_kind_of(engine: &str) -> EngineKind {
    match engine {
        "agy" => EngineKind::Agy,
        "muse" => EngineKind::Muse,
        "http" => EngineKind::Http,
        _ => EngineKind::Codex,
    }
}

/// The consult-path `-EngineExe` binding (`Resolve-EngineExeBinding`, D3): the engine it names —
/// `-Engine`'s, else the `-Provider` roster entry's, else the roster's only non-codex engine.
fn resolve_engine_exe_binding(
    engine: &str,
    roster: &c3_core::roster::Roster,
    provider: &str,
    model: &str,
) -> Result<String, String> {
    let others: Vec<&str> = c3_core::lineage::ENGINE_NAMES
        .iter()
        .copied()
        .filter(|e| *e != "codex")
        .collect();
    if !engine.is_empty() {
        if engine == "codex" {
            return Err(format!(
                "-EngineExe names the launcher of an engine other than codex ({}); codex takes -CodexExe.",
                others.join(", ")
            ));
        }
        return Ok(engine.to_string());
    }
    if !provider.is_empty() {
        if let Some(pe) = providers::find_roster_entry(roster, provider, model) {
            let pe_engine = entry_engine(pe);
            if pe_engine != "codex" {
                return Ok(pe_engine);
            }
            return Err(format!(
                "-EngineExe: the roster entry {} for -Provider {provider} is engine codex, which takes -CodexExe.",
                pe.position
            ));
        }
    }
    let mut used: Vec<String> = Vec::new();
    if roster.exists {
        for e in &roster.entries {
            let ee = entry_engine(e);
            if ee != "codex" && !used.contains(&ee) {
                used.push(ee);
            }
        }
    }
    if used.len() == 1 {
        return Ok(used[0].clone());
    }
    if used.len() > 1 {
        Err(format!(
            "-EngineExe is ambiguous: the reviewer roster has entries of the engines {}; pass -Engine <{}> to name the one it launches.",
            used.join(" and "),
            others.join("|")
        ))
    } else {
        Err(format!(
            "-EngineExe names the launcher of an engine other than codex: pass -Engine <{}> with it.",
            others.join("|")
        ))
    }
}

/// A roster entry's engine, defaulting an absent one to `codex`.
fn entry_engine(e: &c3_core::roster::RosterEntry) -> String {
    if e.engine.is_empty() {
        "codex".to_string()
    } else {
        e.engine.clone()
    }
}

/// The codex `Mode` from the resolved parent walk: a `fork`/`resume` on the resolved parent
/// thread (carrying the run's own lineage so the core refuses a cross-lineage resume), else a
/// fresh thread.
fn resolved_mode(effective_mode: &str, parent_thread: &str, id: &ReviewerIdentity) -> Mode {
    if parent_thread.is_empty() {
        return Mode::New;
    }
    // The Mode's lineage must equal `Request::lineage()` (the core's `check_lineage`), which is
    // the reviewer lineage `provider :: model [engine]` — not the plain `id.lineage` (identical
    // for codex, but the engine suffix matters for agy/muse).
    let lineage = c3_core::engine::Lineage(c3_core::lineage::format_reviewer_lineage(
        &id.provider,
        &id.model,
        &id.engine,
    ));
    if effective_mode == "resume" {
        Mode::Resume {
            thread: parent_thread.to_string(),
            lineage,
        }
    } else {
        Mode::Fork {
            thread: parent_thread.to_string(),
            lineage,
        }
    }
}

/// The resolved parent-thread walk result (`Select-ParentThread`'s `$r`).
#[derive(Debug)]
struct ParentResolved {
    parent_thread: String,
    /// The mode after the walk (`new` when no parent; `fork` defaulted from auto with a parent).
    mode: String,
    /// The parent note text (`$r.Note`); empty when none.
    note: String,
}

/// `Test-SameReviewer`: provider and model equal (ordinal, case-sensitive), each on its own,
/// and the same engine (an absent one is codex).
fn same_reviewer(rev: &Reviewer, id: &ReviewerIdentity) -> bool {
    let id_engine = if id.engine.is_empty() {
        "codex"
    } else {
        id.engine.as_str()
    };
    let entry_engine = if rev.engine.is_empty() {
        "codex"
    } else {
        rev.engine.as_str()
    };
    rev.provider == id.provider && rev.model == id.model && entry_engine == id_engine
}

/// An entry's reviewer lineage (`Get-EntryReviewer`'s `Display` = `Format-ReviewerLineage`).
fn entry_reviewer_lineage(rev: &Reviewer) -> String {
    let engine = if rev.engine.is_empty() {
        "codex"
    } else {
        rev.engine.as_str()
    };
    c3_core::lineage::format_reviewer_lineage(&rev.provider, &rev.model, engine)
}

/// Whether an entry predates the reviewer record (0.1/0.2): no reviewer fields at all. C3 cannot
/// see a truly absent `reviewer` key (it deserializes to a blank record), so an all-empty
/// reviewer is treated as legacy.
fn reviewer_absent(rev: &Reviewer) -> bool {
    rev.provider.is_empty()
        && rev.model.is_empty()
        && rev.engine.is_empty()
        && rev.provider_fingerprint.is_empty()
}

/// `Select-ParentThread`: resolve the parent thread (a `resume`/`fork` target) and the effective
/// mode from `-Mode`/`-Thread` and this task's ledger, or an `Err` refusal (byte-identical to the
/// plugin). `-Thread` is validated by lineage and provenance; the automatic parent is the newest
/// verified thread of the same lineage.
fn select_parent_thread(
    entries: &[LedgerEntry],
    id: &ReviewerIdentity,
    mode: &str,
    thread: &str,
) -> Result<ParentResolved, String> {
    let lineage = &id.lineage;
    let unresolved_msg = format!(
        "provider identity could not be resolved ({}); pass -Provider and -Model explicitly, or use -Mode new",
        id.note
    );
    let drift_msg = |t: &str, n: &str, fp: &str| {
        format!(
            "endpoint or protocol of provider {} changed since thread {t} (consult n={n} recorded provider fingerprint {}, now {}); start a new thread with -Mode new",
            id.provider,
            short_hash(fp),
            short_hash(&id.fingerprint)
        )
    };
    let thread = thread.trim();
    let mut res = ParentResolved {
        parent_thread: String::new(),
        mode: mode.to_string(),
        note: String::new(),
    };

    if !thread.is_empty() {
        if !id.resolved {
            return Err(unresolved_msg);
        }
        // Find-ThreadEntry: the newest entry recording this thread as a verified thread.
        let match_entry = match entries.iter().rev().find(|e| e.thread.trim() == thread) {
            Some(e) => e,
            None => {
                // A codex candidate (a foreign rollout) is never a parent; name it if present.
                let cand = entries
                    .iter()
                    .rev()
                    .find(|e| e.thread_candidate.trim() == thread);
                return Err(match cand {
                    Some(c) => format!(
                        "thread {thread} has unknown provenance: it is only an unverified rollout candidate of consult n={} (that rollout did not contain the run's consultation id); use -Mode new",
                        c.n
                    ),
                    None => format!(
                        "thread {thread} has unknown provenance: it is not in this task's ledger; use -Mode new"
                    ),
                });
            }
        };
        let n = match_entry.n;
        if reviewer_absent(&match_entry.reviewer) {
            return Err(format!(
                "thread {thread} has unknown provenance (recorded before 0.3.0); use -Mode new"
            ));
        }
        if match_entry.reviewer.provider_fingerprint.is_empty() {
            return Err(format!(
                "thread {thread} has unknown provenance: consult n={n} ran with an unresolved reviewer identity ({}); use -Mode new",
                entry_reviewer_lineage(&match_entry.reviewer)
            ));
        }
        if !same_reviewer(&match_entry.reviewer, id) {
            let theirs = entry_reviewer_lineage(&match_entry.reviewer);
            return Err(format!(
                "thread {thread} belongs to lineage {theirs} (consult n={n}); this run is {lineage}. A thread never changes provider or model: use -Mode new, or run as {theirs}"
            ));
        }
        let fp = &match_entry.reviewer.provider_fingerprint;
        if *fp != id.fingerprint {
            return Err(drift_msg(thread, &n.to_string(), fp));
        }
        if res.mode.is_empty() {
            res.mode = "fork".into();
        }
        res.parent_thread = thread.to_string();
        res.note = format!("-Thread, lineage {lineage} (consult n={n})");
        return Ok(res);
    }

    // No -Thread.
    if !id.resolved {
        if mode == "fork" || mode == "resume" {
            return Err(unresolved_msg);
        }
        res.mode = "new".into();
        res.note = format!(
            "reviewer identity unresolved, automatic fork/resume is off ({})",
            id.note
        );
        return Ok(res);
    }

    // The automatic parent: the newest verified thread of the same lineage.
    let mut parent: Option<&LedgerEntry> = None;
    let (mut legacy, mut unresolved, mut candidates) = (0i64, 0i64, 0i64);
    let mut others: Vec<String> = Vec::new();
    for c in entries.iter().rev() {
        let t = c.thread.trim();
        if t.is_empty() {
            if !c.thread_candidate.trim().is_empty() {
                candidates += 1;
            }
            continue;
        }
        if reviewer_absent(&c.reviewer) {
            legacy += 1;
            continue;
        }
        if c.reviewer.provider_fingerprint.is_empty() {
            unresolved += 1;
            continue;
        }
        if !same_reviewer(&c.reviewer, id) {
            let d = entry_reviewer_lineage(&c.reviewer);
            if !others.contains(&d) {
                others.push(d);
            }
            continue;
        }
        if parent.is_none() {
            parent = Some(c);
        }
    }
    if let Some(p) = parent {
        if mode == "new" {
            return Ok(res); // -Mode new starts a fresh thread even when a parent is available.
        }
        let pt = p.thread.trim().to_string();
        let n = p.n;
        let fp = &p.reviewer.provider_fingerprint;
        if *fp != id.fingerprint {
            return Err(drift_msg(&pt, &n.to_string(), fp));
        }
        if res.mode.is_empty() {
            res.mode = "fork".into();
        }
        res.parent_thread = pt;
        res.note = format!("newest thread of lineage {lineage} (consult n={n})");
        return Ok(res);
    }

    // No parent of this lineage.
    let mut why: Vec<String> = Vec::new();
    if legacy > 0 {
        why.push(format!(
            "{legacy} thread(s) recorded before 0.3.0 have unknown provenance and are never automatic parents"
        ));
    }
    if !others.is_empty() {
        why.push(format!("other lineage(s): {}", others.join(", ")));
    }
    if unresolved > 0 {
        why.push(format!(
            "{unresolved} thread(s) of runs with an unresolved reviewer identity are never parents"
        ));
    }
    if candidates > 0 {
        why.push(format!(
            "{candidates} unverified rollout candidate(s) are never parents"
        ));
    }
    let mut note = format!("no thread of lineage {lineage} in this task's ledger");
    if !why.is_empty() {
        note.push_str(&format!("; {}", why.join("; ")));
    }
    if mode == "fork" || mode == "resume" {
        return Err(format!(
            "-Mode {mode} needs a parent thread: {note}. Pass -Thread <uuid> of lineage {lineage}, or use -Mode new"
        ));
    }
    res.mode = "new".into();
    if !entries.is_empty() {
        res.note = note;
    }
    Ok(res)
}

fn schema_file(_repo_root: &Path) -> Option<PathBuf> {
    // (wave 2b) `C3_SCHEMA_FILE` names an on-disk copy of the reply schema to pass instead - the
    // plugin's own `<scripts>\..\schemas\consult-reply.schema.json` when the harness shim fronts
    // the plugin's script (`--json-schema`/`--output-schema` then name the file the plugin would).
    // Honoured only when the file holds exactly the embedded schema (CRLF read as LF), so what the
    // engine is given and what C3 validates against never differ.
    if let Ok(v) = std::env::var("C3_SCHEMA_FILE") {
        let p = PathBuf::from(v.trim());
        if !v.trim().is_empty() {
            if let Ok(bytes) = std::fs::read(&p) {
                let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
                if text == c3_core::schema::REPLY_SCHEMA_V1 {
                    return Some(p);
                }
            }
        }
    }
    let home = providers::get_codex_home();
    if home.is_empty() {
        return None;
    }
    let home = PathBuf::from(&home);
    match c3_core::schema::materialize(&home) {
        Ok(p) => Some(p),
        Err(_) => Some(c3_core::schema::materialized_path(&home)),
    }
}

fn get_codex_version(launcher: &str) -> String {
    if launcher.is_empty() {
        return "codex (version unknown)".to_string();
    }
    let (prog, args): (String, Vec<String>) = if cfg!(windows)
        && Path::new(launcher)
            .extension()
            .map(|e| {
                let e = e.to_string_lossy().to_lowercase();
                e == "cmd" || e == "bat"
            })
            .unwrap_or(false)
    {
        (
            "cmd".into(),
            vec!["/c".into(), launcher.into(), "--version".into()],
        )
    } else {
        (launcher.into(), vec!["--version".into()])
    };
    let mut cmd = std::process::Command::new(prog);
    cmd.args(args);
    // (wave 27 / 27b) the `codex --version` probe gets no host marker either.
    crate::engines::scrub_host_markers(&mut cmd);
    cmd.output()
        .ok()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "codex (version unknown)".to_string())
}

/// Resolve + hash the `-Artifact` paths (`Resolve-ArtifactPaths` + `Get-ArtifactHashes`). Each
/// path is used as given if absolute (must exist), else looked up under `cwd` then `repo_root`.
/// A missing artifact refuses (the review is bound to it). Returns the pre-run hashes.
fn resolve_artifacts(
    paths: &[String],
    cwd: &Path,
    repo_root: &Path,
) -> Result<Vec<ArtifactHash>, String> {
    let mut out = Vec::new();
    let bases = [cwd.to_path_buf(), repo_root.to_path_buf()];
    for raw in paths {
        if raw.is_empty() {
            continue;
        }
        let resolved: Option<PathBuf> = if Path::new(raw).is_absolute() {
            let p = PathBuf::from(raw);
            if p.is_file() {
                Some(p)
            } else {
                None
            }
        } else {
            bases.iter().map(|b| b.join(raw)).find(|c| c.is_file())
        };
        // The ledger records the RAW argument in `path` (`Resolve-ArtifactPaths` sets
        // `path = $raw`), so a relative artifact reads back as e.g. `A.bin`/`build.bin` in the
        // ledger and the "changed during the review" warning; the canonicalized form is kept
        // only in `full` for reading/hashing.
        let full = match resolved {
            Some(p) => p.canonicalize().unwrap_or(p),
            None => {
                let looked = if Path::new(raw).is_absolute() {
                    raw.clone()
                } else {
                    let mut seen: Vec<String> = Vec::new();
                    for b in &bases {
                        let s = b.to_string_lossy().to_string();
                        if !seen.contains(&s) {
                            seen.push(s);
                        }
                    }
                    seen.join(", ")
                };
                return Err(format!(
                    "artifact '{raw}' not found (looked in: {looked}); a review cannot be bound to a missing artifact."
                ));
            }
        };
        let sha256 = std::fs::read(&full)
            .map(|b| c3_core::sha256_hex(&b))
            .unwrap_or_else(|_| "missing".into());
        out.push(ArtifactHash {
            path: raw.clone(),
            full,
            sha256,
        });
    }
    Ok(out)
}

fn read_open_findings(store: &FilesStore, task: &TaskSlug) -> (Vec<OpenFinding>, usize) {
    let findings = match store.read_findings(task) {
        Ok(Some(f)) => f,
        _ => return (Vec::new(), 0),
    };
    let mut out = Vec::new();
    for f in &findings.findings {
        let status = f.status().as_str().to_string();
        if status != "proposed" && status != "implemented" {
            continue;
        }
        let locs: Vec<c3_core::engine::ReplyLocation> = f
            .locations
            .iter()
            .map(|l| c3_core::engine::ReplyLocation {
                path: l.path.clone(),
                line: l.line,
            })
            .collect();
        out.push(OpenFinding {
            id: f.id.clone(),
            status,
            locations: render::format_locations(&locs, true),
            claim: f.claim.clone(),
            trigger: f.trigger.clone(),
            verification: f.verification.clone(),
        });
    }
    let n = out.len();
    (out, n)
}

// ------------------------------------------------------------------------------- live run

fn run_live(mut ctx: Context) -> i32 {
    // Ensure the handoffs dir exists.
    if let Err(e) = std::fs::create_dir_all(&ctx.handoffs_dir) {
        return refuse(&format!("could not create the handoffs directory: {e}"));
    }
    // The roster decision line prints before the lock on a real run (`codex-consult.ps1:2968`).
    if !ctx.roster_line.is_empty() {
        println!("{}", ctx.roster_line);
    }
    // (wave 27c, D11) a coordinator that parses but matches no roster entry is SAID on the console
    // of a real run (`codex-consult.ps1:2274`), not refused; the ledger already carries
    // `coordinator.in_roster: false`.
    if ctx.coordinator.in_roster == Some(false) {
        let id = std::env::var("CODEX_CONSULT_COORDINATOR").unwrap_or_default();
        println!(
            "coordinator: {} (not in the roster - no reviewer can match it)",
            id.trim()
        );
    }
    // The `-SkipPreflight` quota warning prints before the lock (never on a dry run, which
    // never reaches `run_live`).
    if !ctx.preflight_warning.is_empty() {
        println!("WARNING: {}", ctx.preflight_warning);
    }
    let store = FilesStore::new(ctx.collab_root.clone());
    let reply_rel = ctx.hf("md");
    let is_member = ctx.panel_member.is_some();
    let pending = match &ctx.panel_member {
        Some(m) => PendingRef::member(ctx.task.clone(), m.nn as u32),
        None => PendingRef::single(ctx.task.clone()),
    };

    // Task ownership lock: a single run takes it (fail-fast); a panel member runs under the panel
    // run's lock and never takes one of its own (`codex-consult.ps1:3387`).
    let _task_lock = if is_member {
        None
    } else {
        let lock_record = LockRecord::now(&ctx.task, None);
        match store.take_task_lock(&ctx.task, &lock_record) {
            Ok(l) => Some(l),
            Err(_) => {
                let lock_path = store.task_dir(&ctx.task).join(".consult.lock");
                return refuse(&format_task_lock_refusal(&lock_path, ctx.task.as_str()));
            }
        }
    };

    let mut rec = if let Some(m) = ctx.panel_member.clone() {
        // A panel member never consumes the task's other records; it accepts the reserved record
        // the panel run wrote for it (matching its spec), rewrites it as its own, checks the
        // parent is alive, then records this run's reply/consult id/launcher/engine.
        match member_accept(&store, &ctx, &pending, &m, &reply_rel) {
            Ok(r) => r,
            Err(msg) => return refuse(&msg),
        }
    } else {
        // Recovery records: read and judge every `.consult.pending*.json` of the task under the
        // lock, BEFORE anything is written (`codex-consult.ps1` ~2766). A corrupt record refuses;
        // a live process of an interrupted run refuses (its message names the pid); a dead record
        // is consumed — numbering already skipped past it (`next_numbers`), a recovered/cleared
        // line is printed and carried into the handoff, and a consumed panel-member record is
        // removed (the single-run record is overwritten by this run's reservation below).
        {
            let assessed = super::recovery::assess(&store, &ctx.task);
            if let Some(err) = assessed.error {
                return refuse(&err);
            }
            if let Some(msg) = assessed.active_message() {
                return refuse(&msg);
            }
            let own = pending.file_name();
            for item in &assessed.items {
                let line = super::recovery::run_line(item);
                println!("{TOOL}: {line}");
                ctx.recovery_lines.push(line);
                let is_own = item
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().eq_ignore_ascii_case(&own))
                    .unwrap_or(false);
                if !is_own {
                    if let Err(e) = std::fs::remove_file(&item.path) {
                        println!(
                            "{TOOL}: could not remove the consumed recovery record {} ({e})",
                            item.path.display()
                        );
                    }
                }
            }
        }

        // Reserve the recovery record (`New-PendingRecord -State reserved`): this bridge's pid and
        // start time (the writer-pid liveness rule), the host and this run's numbers/reply.
        let rec = new_pending_record(PendingState::Reserved, &ctx, &reply_rel);
        let _ = store.write_pending(&pending, &rec);
        // A detached single run: its one member is running now (its numbers are assigned).
        if super::detach::is_active() {
            let n = ctx.consult_n;
            let handoff = format!("{:02}", ctx.nn);
            super::detach::update_member(1, |m| {
                m.state = "running".into();
                m.n = Some(n);
                m.handoff = handoff;
            });
        }
        rec
    };

    // The plugin fingerprints the tree AFTER the lock and recovery record exist, so the
    // `.consult.*` files under the collab dir are counted among the excluded entries
    // (`fingerprint_note`). Re-fingerprint here to match (`$revBefore` in the plugin flow).
    ctx.revision = revision::revision_info(&ctx.repo_root, Some(&ctx.collab_root));
    // The collab-directory snapshot (`$collabBefore`), taken after the reserved record and the
    // tree fingerprint, before launch — the read-only tree check compares it after the last turn
    // (agy/muse only; `.consult.*` control files are excluded). `codex-consult.ps1:3661`.
    let collab_before = if ctx.is_codex() {
        std::collections::HashMap::new()
    } else {
        crate::engines::tree_check::collab_snapshot(&ctx.collab_root)
    };

    // Peak status AT LAUNCH (the one the ledger records; the early check was call 0, this is
    // call 1). Preparation between the two may cross a window boundary: under `-OffPeakOnly` a
    // window entered since the early check stops the run here — the reservation is withdrawn
    // (nothing written under its numbers), no ledger entry.
    let launch_peak = c3_core::peak::peak_status_now(&ctx.peak_provider, 1);
    if !launch_peak.error.is_empty() {
        let _ = std::fs::remove_file(store_pending_path(&store, &pending));
        return refuse(&launch_peak.error);
    }
    if ctx.o.off_peak_only && launch_peak.peak != Some(false) {
        let _ = std::fs::remove_file(store_pending_path(&store, &pending));
        return refuse(&format!(
            "-OffPeakOnly: {} entered its peak window before launch ({}; now {}); nothing was started.",
            ctx.peak_provider, launch_peak.schedule, launch_peak.local
        ));
    }
    let (launch_warning, _) = peak_display(&ctx.peak_provider, &launch_peak);
    ctx.peak = launch_peak.peak;
    ctx.peak_schedule = launch_peak.schedule.clone();
    ctx.peak_source = launch_peak.source.clone();
    ctx.peak_evaluated_at = launch_peak.evaluated_at.clone();
    ctx.peak_warning = launch_warning;
    if !ctx.peak_warning.is_empty() {
        println!("WARNING: {}", ctx.peak_warning);
    }

    // (wave 27c, D3) the host-marker hide is transactional and fail-closed: if it cannot hide every
    // marker (only the `CODEX_CONSULT_TEST_HIDE_FAIL` hook forces this in C3), the engine start is
    // refused before launch — nothing is started, no record is written, no ledger entry.
    if let Some(why) = crate::engines::host_marker_hide_failure() {
        let _ = std::fs::remove_file(store_pending_path(&store, &pending));
        return refuse(&format!(
            "the {} run is refused before launch: bridge failure: {why}; nothing was started.",
            ctx.engine
        ));
    }

    // (3) launching — from here on a crash may leave a codex process whose pid is not yet
    // recorded; the next run then scans the tree (`codex-consult.ps1:3299`).
    rec.state = PendingState::Launching;
    rec.note = format!(
        "{} is being started; its pid is not recorded yet",
        ctx.engine
    );
    let _ = store.write_pending(&pending, &rec);

    // muse takes its prompt through `--prompt-file`: write it (UTF-8, no BOM) before launch.
    if let Some(pf) = &ctx.prompt_file {
        if let Err(e) = c3_core::store::write_text_atomic(pf, ctx.prompt_text.as_bytes()) {
            let _ = std::fs::remove_file(store_pending_path(&store, &pending));
            return refuse(&format!("could not write the prompt file: {e}"));
        }
    }

    // The cmd.exe `%`-argument hazard (F02-14): a `.cmd`/`.bat` engine launcher with a `%` in any
    // argument would have cmd.exe expand `%VAR%`. Refuse before launch (nothing started).
    if !ctx.is_codex() {
        let hazard = cmd_argv_hazard(&ctx.engine_launcher, &ctx.argv);
        if !hazard.is_empty() {
            let _ = std::fs::remove_file(store_pending_path(&store, &pending));
            return refuse(&format!(
                "the {} run is refused before launch: {hazard}; nothing was started.",
                ctx.engine
            ));
        }
    }

    // A panel member re-checks its parent right before launching its reviewer (D1,
    // `codex-consult.ps1:3973`): if the panel run died during the member's preflight, the member
    // stops here, withdraws its record, and starts nothing.
    // TEST HOOK (test mode only): CODEX_CONSULT_TEST_MEMBER_LAUNCH_MARK=<file> is written (this pid)
    // when a member reaches this point, and CODEX_CONSULT_TEST_MEMBER_LAUNCH_PAUSE_MS=<ms> pauses it
    // here - the harness kills the panel run inside that pause (harness-panel SPEC, D1).
    if let Some(m) = &ctx.panel_member {
        if let Some(mark) = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_MEMBER_LAUNCH_MARK") {
            if !mark.trim().is_empty() {
                let _ = std::fs::write(mark.trim(), std::process::id().to_string());
            }
        }
        if let Some(ms) = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_MEMBER_LAUNCH_PAUSE_MS") {
            if let Ok(ms) = ms.trim().parse::<u64>() {
                if ms > 0 {
                    std::thread::sleep(Duration::from_millis(ms));
                }
            }
        }
        if !crate::liveness::proc::pid_alive(m.parent_pid as u32, &m.parent_start_time) {
            let path = store_pending_path(&store, &pending);
            let rm = std::fs::remove_file(&path);
            let suffix = match rm {
                Ok(_) => String::new(),
                Err(e) => format!(
                    " (its recovery record '{}' could not be removed: {e})",
                    path.display()
                ),
            };
            return refuse(&format!(
                "the review panel run that launched this member (pid {}) is gone; this member stopped before starting {} - nothing was started{suffix}.",
                m.parent_pid, ctx.engine
            ));
        }
    }

    // (4) running — the callback flips the record to `running` right after the child spawns,
    // recording its pid and start time so the next run detects an interrupted-run process.
    let events_rel_for_record = c3_core::paths::repo_relative(&ctx.repo_root, &ctx.events_path)
        .unwrap_or_else(|| ctx.events_path.to_string_lossy().to_string());
    let rec_arc = std::sync::Arc::new(std::sync::Mutex::new(rec));
    // (rows (e)) The registration write can fail (a real I/O error, or the
    // `CODEX_CONSULT_TEST_REGISTER_FAIL` hook). When it does, the run FAILS CLOSED: the just-spawned
    // child tree is stopped by pid, the record is left at `launching`, and the outcome says the
    // engine could not be registered. The error travels back to `finish` through this cell.
    let register_error: std::sync::Arc<std::sync::Mutex<Option<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let on_running: std::sync::Arc<dyn Fn(u32, String) + Send + Sync> = {
        let cb_store = store.clone();
        let cb_pending = pending.clone();
        let cb_rec = std::sync::Arc::clone(&rec_arc);
        let cb_events = events_rel_for_record.clone();
        let cb_error = std::sync::Arc::clone(&register_error);
        std::sync::Arc::new(move |child_pid: u32, child_start: String| {
            // A forced failure (test hook) or a real write failure of the `running` record.
            let forced = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_REGISTER_FAIL")
                .map(|v| !v.trim().is_empty() && v.trim() != "0")
                .unwrap_or(false);
            if let Ok(mut r) = cb_rec.lock() {
                r.state = PendingState::Running;
                r.child_pid = Some(child_pid);
                r.child_start_time = child_start;
                r.events = cb_events.clone();
                r.note = String::new();
                let write = if forced {
                    Err("injected: registration write failed".to_string())
                } else {
                    cb_store
                        .write_pending(&cb_pending, &r)
                        .map_err(|e| e.to_string())
                };
                if let Err(err) = write {
                    // Fail closed: stop the child tree by pid, leave the record at `launching`.
                    crate::engines::subprocess::kill_tree_by_pid(child_pid);
                    r.state = PendingState::Launching;
                    r.child_pid = None;
                    r.child_start_time = String::new();
                    r.note =
                        "the engine process could not be registered; it was stopped".to_string();
                    let _ = cb_store.write_pending(&cb_pending, &r);
                    if let Ok(mut slot) = cb_error.lock() {
                        *slot = Some(err);
                    }
                }
            }
        })
    };

    // The consultation's start time (`$startedAt`, `codex-consult.ps1:3992`), captured just before
    // the turn launches — the ledger's `when`. A panel member captures its OWN start here, so
    // concurrent members share a close `when` while the slower one commits (`finished_at`) later.
    let when_iso = iso_now();

    // (wave 26b, D13) register this run on its endpoint in the machine-wide health file while its
    // engine turn runs, so panels of other repositories count it against the endpoint's parallel
    // limit. Removed after the run finishes (below); a failed update only warns.
    let health_path = if ctx.identity.resolved {
        c3_core::health::machine_health_path(&providers::get_codex_home())
    } else {
        None
    };
    let (bridge_pid, _bridge_start) = crate::liveness::proc::bridge_identity();
    if let Some(hp) = &health_path {
        let (bpid, bstart) = crate::liveness::proc::bridge_identity();
        let row = c3_core::health::MachineRunning {
            endpoint: ctx.identity.fingerprint.clone(),
            label: ctx.identity.provider.clone(),
            pid: bpid,
            start_time: bstart,
            repo: ctx.repo_root.to_string_lossy().to_string(),
            task: ctx.o.task.clone(),
            nn: format!("{:02}", ctx.nn),
            panel: ctx
                .panel_member
                .as_ref()
                .map(|m| m.id.clone())
                .unwrap_or_default(),
            since: iso_now(),
            // (wave 29b, E16) with the roster entry's plan: a panel elsewhere counts it against
            // the plan
            plan: ctx.plan.clone(),
        };
        let _ = c3_core::health::register_machine_running(hp, row, &|pid, st| {
            crate::liveness::proc::pid_alive(pid, st)
        });
    }

    // (wave 26c, D1) remove a stale kick file and its acknowledgement of THIS run's number before
    // the primary turn, so a leftover from a prior run of the same handoff never fires.
    let _ = std::fs::remove_file(&ctx.kick_path);
    let _ = std::fs::remove_file(crate::engines::subprocess::kick_ack_path(&ctx.kick_path));

    // Run the primary turn through the selected engine adapter.
    let (outcome, detail) = match run_primary_turn(&ctx, Some(on_running)) {
        Ok(r) => r,
        Err(e) => {
            let _ = std::fs::remove_file(store_pending_path(&store, &pending));
            if let Some(hp) = &health_path {
                let _ = c3_core::health::unregister_machine_running(hp, bridge_pid, &|pid, st| {
                    crate::liveness::proc::pid_alive(pid, st)
                });
            }
            return refuse(&e);
        }
    };

    let base_record = rec_arc.lock().map(|r| r.clone()).unwrap_or_default();
    let register_failure = register_error.lock().ok().and_then(|g| g.clone());
    let code = finish(
        ctx,
        store,
        pending,
        outcome,
        base_record,
        detail,
        collab_before,
        when_iso,
        register_failure,
    );
    if let Some(hp) = &health_path {
        let _ = c3_core::health::unregister_machine_running(hp, bridge_pid, &|pid, st| {
            crate::liveness::proc::pid_alive(pid, st)
        });
    }
    code
}

/// Engine-turn detail carried out of the primary turn (empty/false for codex). The agy/muse
/// finish path uses it for the denial-retry gate, the bridge-outcome text, the forced failure
/// class, the engine warnings and the MSP schema version.
#[derive(Default, Clone)]
struct EngineDetail {
    /// The turn's own outcome string (`AgyTurn`/`MuseTurn` `.outcome`), the engine bridge-outcome.
    turn_outcome: String,
    /// The forced `provider_failure` class (`''` = classify the texts).
    forced_class: String,
    /// A tool was auto-denied and the turn produced nothing (run the agy denial retry).
    denied_empty: bool,
    denial_line: String,
    permission: String,
    tool_name: String,
    denied_action: String,
    /// A conversation id observed but not verified (never a thread/parent).
    thread_candidate: String,
    /// A verified thread the turn named even on a failure (e.g. agy status ERROR).
    thread: String,
    /// The reply text the turn produced, even on a failure (kept as `.reply.json`).
    reply: String,
    /// The turn's warnings (denial notices, engine warnings).
    warnings: Vec<String>,
    /// The MSP schema version of a muse stream (`None` for agy).
    msp_schema_version: Option<i64>,
    /// (M7b-b) The http seat's `reviewer.provider_config` (`{engine, base_url, model, pack,
    /// pack_sha256}`), built by the adapter and applied to `ctx.identity.provider_config` in
    /// `finish` so it lands on the ledger. `None` for every other engine.
    http_provider_config: Option<serde_json::Value>,
    /// (STEP 2) The http seat's secondary turn (a format-repair replay or a timeout retry) when one
    /// ran, so the caller records `engine_turns: 2` and the `format_repair` fields. `None` otherwise.
    http_secondary: Option<crate::consult::http::HttpSecondary>,
}

/// Build the request for a live primary/secondary turn of the selected engine.
fn make_live_request_engine(ctx: &Context, mode: Mode) -> Request {
    let sandbox = sandbox_label(&ctx.o);
    let engine = engine_kind_of(&ctx.engine);
    // The schema flag rides a non-prompt-only, non-raw transport: codex `output-schema`, agy/muse
    // `native`.
    let schema_arg = if !ctx.r.raw
        && (ctx.transport.transport == "output-schema" || ctx.transport.transport == "native")
    {
        ctx.schema_path.clone()
    } else {
        None
    };
    Request {
        prompt: ctx.prompt_text.clone(),
        brief_path: ctx.brief_path.clone(),
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
        effort: ctx.effort.sent.clone(),
        timeout_sec: ctx.r.timeout_sec as f64,
        mode,
        sandbox,
        schema_path: schema_arg,
        extra_config: with_context_config(&ctx.r.extra_config, &ctx.context_config),
        output_last_message: if engine == EngineKind::Codex {
            Some(ctx.last_msg_path.clone())
        } else {
            None
        },
        prompt_file: ctx.prompt_file.clone(),
        max_model_steps: if ctx.o.max_model_steps > 0 {
            Some(ctx.o.max_model_steps as u32)
        } else {
            None
        },
    }
}

/// Run the primary turn through the selected engine (codex/agy/muse) and normalize the outcome
/// plus the engine detail.
fn run_primary_turn(
    ctx: &Context,
    on_running: Option<std::sync::Arc<dyn Fn(u32, String) + Send + Sync>>,
) -> Result<(AttemptOutcome, EngineDetail), String> {
    let primary = TurnFiles {
        events: ctx.events_path.clone(),
        stderr: ctx.stderr_path.clone(),
    };
    match ctx.engine.as_str() {
        "agy" => {
            let eng = crate::engines::agy::AgyEngine {
                launcher: ctx.engine_launcher.clone(),
                cwd: ctx.repo_root.clone(),
                primary,
                secondary: TurnFiles::default(),
                no_network: false,
                models_timeout_sec: 45,
                stall_sec: ctx.stall_sec,
                kick_path: Some(ctx.kick_path.clone()),
                on_running,
            };
            let mode = resolved_mode(&ctx.effective_mode, &ctx.parent_thread, &ctx.identity);
            let turn = engine_turn(ctx, make_live_request_engine(ctx, mode), TurnKind::Primary);
            let run = eng
                .run_detailed(&turn)
                .map_err(|e| format!("the agy run could not be planned: {e:?}"))?;
            let mut d = agy_detail(&run.turn);
            // The denied tool / action come from the events (the retry prompt names them).
            let ev = crate::engines::agy::read_agy_events(
                &std::fs::read_to_string(&ctx.events_path).unwrap_or_default(),
                true,
            );
            d.tool_name = ev.tool_name;
            d.denied_action = ev.denied_action;
            Ok((run.outcome, d))
        }
        "muse" => {
            let eng = crate::engines::muse::MuseEngine {
                launcher: ctx.engine_launcher.clone(),
                cwd: ctx.repo_root.clone(),
                primary,
                secondary: TurnFiles::default(),
                stall_sec: ctx.stall_sec,
                kick_path: Some(ctx.kick_path.clone()),
                on_running,
            };
            let mode = resolved_mode(&ctx.effective_mode, &ctx.parent_thread, &ctx.identity);
            let turn = engine_turn(ctx, make_live_request_engine(ctx, mode), TurnKind::Primary);
            let run = eng
                .run_detailed(&turn)
                .map_err(|e| format!("the muse run could not be planned: {e:?}"))?;
            let msp = muse_msp_version(&ctx.events_path);
            let mut d = muse_detail(&run.turn);
            d.msp_schema_version = msp;
            Ok((run.outcome, d))
        }
        "http" => {
            // The http engine has no subprocess: it builds a reviewer pack, runs the billing/key
            // guard, retains the pack, and sends one OpenAI-compatible request. The whole run path
            // lives in `consult::http`; here it just yields the outcome and the ledger's
            // provider_config (applied to the identity in `finish`).
            let seat = crate::consult::http::run_seat(ctx)?;
            let d = EngineDetail {
                turn_outcome: seat.bridge_outcome,
                reply: seat.reply_text,
                http_provider_config: Some(seat.provider_config),
                warnings: seat.warnings,
                http_secondary: seat.secondary,
                ..EngineDetail::default()
            };
            Ok((seat.outcome, d))
        }
        _ => {
            let eng = CodexEngine {
                launcher: ctx.launcher.clone(),
                cwd: ctx.repo_root.clone(),
                primary,
                secondary: TurnFiles::default(),
                stall_sec: ctx.stall_sec,
                kick_path: Some(ctx.kick_path.clone()),
                on_running,
            };
            let request = make_live_request(ctx);
            let turn = TurnRequest {
                request,
                consultation: ConsultationId(ctx.consult_id.clone()),
                attempt: c3_core::engine::AttemptId(ctx.consult_id.clone()),
                kind: TurnKind::Primary,
                continuation: None,
            };
            use c3_core::engine::Engine;
            let outcome = eng
                .run(&turn)
                .map_err(|e| format!("the codex run could not be planned: {e}"))?;
            Ok((outcome, EngineDetail::default()))
        }
    }
}

/// Assemble a `TurnRequest` for an engine turn (agy/muse). The continuation is the native thread
/// resume when the request's mode names one.
fn engine_turn(ctx: &Context, request: Request, kind: TurnKind) -> TurnRequest {
    let continuation = match &request.mode {
        Mode::Resume { thread, .. } | Mode::Fork { thread, .. } => Some(
            c3_core::engine::Continuation::Native(c3_core::engine::ConversationId(thread.clone())),
        ),
        _ => None,
    };
    TurnRequest {
        request,
        consultation: ConsultationId(ctx.consult_id.clone()),
        attempt: c3_core::engine::AttemptId(ctx.consult_id.clone()),
        kind,
        continuation,
    }
}

fn agy_detail(t: &crate::engines::agy::AgyTurn) -> EngineDetail {
    EngineDetail {
        turn_outcome: t.outcome.clone(),
        forced_class: t.class.clone(),
        denied_empty: t.denied_empty,
        denial_line: t.denial_line.clone(),
        permission: t.permission.clone(),
        tool_name: String::new(),
        denied_action: String::new(),
        thread_candidate: t.thread_candidate.clone(),
        thread: t.thread.clone(),
        reply: t.reply.clone(),
        warnings: t.warnings.clone(),
        msp_schema_version: None,
        http_provider_config: None,
        http_secondary: None,
    }
}

fn muse_detail(t: &crate::engines::muse::MuseTurn) -> EngineDetail {
    EngineDetail {
        turn_outcome: t.outcome.clone(),
        forced_class: t.class.clone(),
        denied_empty: false,
        denial_line: String::new(),
        permission: String::new(),
        tool_name: String::new(),
        denied_action: String::new(),
        thread_candidate: t.thread_candidate.clone(),
        thread: t.thread.clone(),
        reply: t.reply.clone(),
        warnings: t.warnings.clone(),
        msp_schema_version: None,
        http_provider_config: None,
        http_secondary: None,
    }
}

/// The MSP `schema_version` of a muse event stream (`None` when unreadable).
fn muse_msp_version(events_path: &Path) -> Option<i64> {
    let text = std::fs::read_to_string(events_path).unwrap_or_default();
    crate::engines::muse::read_muse_events(&text, true).schema_version
}

/// `Get-CmdArgvHazard` (F02-14): a `.cmd`/`.bat` launcher with a `%` in any argument. Returns the
/// hazard note (empty when safe).
pub(crate) fn cmd_argv_hazard(launcher: &str, argv: &[String]) -> String {
    let ext = Path::new(launcher)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if launcher.is_empty() || (ext != "cmd" && ext != "bat") {
        return String::new();
    }
    let bad: Vec<&String> = argv.iter().filter(|a| a.contains('%')).collect();
    if bad.is_empty() {
        return String::new();
    }
    let first2 = bad
        .iter()
        .take(2)
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "the launcher {launcher} is a cmd.exe script and {} argument(s) contain '%' ({first2}): cmd.exe would expand %VAR% in them; set TEMP and TMP to a directory without '%', or point -EngineExe at the CLI's .exe",
        bad.len()
    )
}

/// A fresh recovery record for this run (`New-PendingRecord`): this bridge's pid/start/host
/// (the writer-pid liveness rule) plus the run's numbers, reply and launcher.
fn new_pending_record(state: PendingState, ctx: &Context, reply_rel: &str) -> PendingRecord {
    // The bridge that wrote the record (the writer-pid liveness rule): c3's own pid in production,
    // the shim's pid under the harnesses.
    let (pid, start_time) = crate::liveness::proc::bridge_identity();
    PendingRecord {
        state,
        n: ctx.consult_n,
        nn: format!("{:02}", ctx.nn),
        reply: reply_rel.to_string(),
        consult_id: ctx.consult_id.clone(),
        started: iso_now(),
        pid,
        start_time,
        host: pending_host(),
        launcher: if ctx.is_codex() {
            ctx.launcher.clone()
        } else {
            ctx.engine_launcher.clone()
        },
        engine: ctx.engine.clone(),
        ..Default::default()
    }
}

fn pending_host() -> String {
    c3_core::host::machine_name()
}

/// `Enter-TaskLock`'s refusal (`codex-consult-common.ps1:7160`): the task lock is held. Re-read
/// the informational lock record briefly and name a pid only while it is alive; a panel holder is
/// named (`review panel <short>`). Byte-identical to the plugin's message.
pub(crate) fn format_task_lock_refusal(lock_path: &Path, task: &str) -> String {
    let mut who = "a live process".to_string();
    for attempt in 0..8 {
        if let Ok(bytes) = std::fs::read(lock_path) {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                let pid = v.get("pid").and_then(|x| x.as_i64()).unwrap_or(0);
                let start = v.get("start_time").and_then(|x| x.as_str()).unwrap_or("");
                if pid > 0 && crate::liveness::proc::pid_alive(pid as u32, start) {
                    let host = v.get("host").and_then(|x| x.as_str()).unwrap_or("?");
                    let started = v.get("started").and_then(|x| x.as_str()).unwrap_or("?");
                    who = format!("pid {pid} on {host} since {started}");
                    if let Some(panel) = v
                        .get("panel")
                        .and_then(|x| x.as_str())
                        .filter(|s| !s.is_empty())
                    {
                        who += &format!(" (review panel {})", &panel[..panel.len().min(8)]);
                    }
                    break;
                }
            }
        }
        if attempt < 7 {
            std::thread::sleep(Duration::from_millis(125));
        }
    }
    format!(
        "another consultation or status update for task '{task}' is running: {} is held open by {who}. Wait for it to finish; the lock is released when that process exits.",
        lock_path.display()
    )
}

/// The write-lock refusal message (`Enter-WriteLock`, D3): `the write lock '<path>' of task
/// '<task>' was not acquired within <N> s: it is held open by pid <pid> on <host> since <started>`
/// (naming the live holder), else `... by a live process`. The timeout is the current
/// `write_lock_timeout_secs()`.
pub(crate) fn format_write_lock_refusal(wl_path: &Path, task: &str) -> String {
    let secs = c3_core::store::write_lock_timeout_secs();
    let mut who = "a live process".to_string();
    if let Ok(bytes) = std::fs::read(wl_path) {
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            let pid = v.get("pid").and_then(|x| x.as_i64()).unwrap_or(0);
            let start = v.get("start_time").and_then(|x| x.as_str()).unwrap_or("");
            if pid > 0 && crate::liveness::proc::pid_alive(pid as u32, start) {
                let host = v.get("host").and_then(|x| x.as_str()).unwrap_or("?");
                let started = v.get("started").and_then(|x| x.as_str()).unwrap_or("?");
                who = format!("pid {pid} on {host} since {started}");
            }
        }
    }
    format!(
        "the write lock '{}' of task '{task}' was not acquired within {secs} s: it is held open by {who}",
        wl_path.display()
    )
}

/// Survivor entries `{pid, start_time, name}` for the pids a timeout kill left alive
/// (`New-SurvivorEntries`); a pid already gone is left out so a reused pid is never mistaken
/// for the survivor later.
fn survivor_entries(pids: &[u32]) -> Vec<serde_json::Value> {
    // `New-SurvivorEntries`: { pid, start_time, name } per pid (`Get-ProcessInfo`); a pid that is
    // already gone is left out
    pids.iter()
        .filter_map(|&pid| {
            crate::liveness::proc::process_info(pid).map(|info| {
                serde_json::json!({ "pid": pid, "start_time": info.start, "name": info.name })
            })
        })
        .collect()
}

fn make_live_request(ctx: &Context) -> Request {
    let o = &ctx.o;
    let sandbox = if o.sandbox.is_empty() {
        "read-only".to_string()
    } else {
        o.sandbox.clone()
    };
    let schema_arg = if ctx.transport.transport == "output-schema" {
        ctx.schema_path.clone()
    } else {
        None
    };
    Request {
        prompt: ctx.prompt_text.clone(),
        brief_path: ctx.brief_path.clone(),
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
        engine: EngineKind::Codex,
        effort: ctx.effort.sent.clone(),
        timeout_sec: ctx.r.timeout_sec as f64,
        mode: resolved_mode(&ctx.effective_mode, &ctx.parent_thread, &ctx.identity),
        sandbox,
        schema_path: schema_arg,
        extra_config: with_context_config(&ctx.r.extra_config, &ctx.context_config),
        output_last_message: Some(ctx.last_msg_path.clone()),
        prompt_file: None,
        max_model_steps: None,
    }
}

fn store_pending_path(store: &FilesStore, pending: &PendingRef) -> PathBuf {
    store.task_dir(&pending.task).join(pending.file_name())
}

/// Record a member-record/spec mismatch in the plugin's phrasing (`$compare`, line 2083).
fn add_mismatch(mism: &mut Vec<String>, name: &str, in_record: String, in_spec: String) {
    if in_record != in_spec {
        let shown = if in_record.is_empty() {
            "(none)".to_string()
        } else {
            in_record
        };
        mism.push(format!(
            "{name} {shown} in the record, {in_spec} in the spec"
        ));
    }
}

/// A JSON value's field as its string form (a number renders without quotes, matching the
/// plugin's `[string]$value`).
fn json_str(v: &serde_json::Value, key: &str) -> String {
    match v.get(key) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

/// Phase B of the panel-member accept (`codex-consult.ps1:3516-3524`): the record was already
/// rewritten as this process's own and its parent verified alive by [`member_early_accept`]
/// (before the preflight); reopen it and stamp this run's reply/consult id/launcher/engine. A
/// missing/unreadable record here means it was withdrawn under this process (a race the caller
/// treats as "not started"); the base record is used as-is.
fn member_accept(
    store: &FilesStore,
    ctx: &Context,
    pending: &PendingRef,
    _m: &crate::panel::member::MemberSpec,
    reply_rel: &str,
) -> Result<PendingRecord, String> {
    let path = store_pending_path(store, pending);
    let mut rec: PendingRecord = std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    rec.reply = reply_rel.to_string();
    rec.consult_id = ctx.consult_id.clone();
    rec.launcher = if ctx.is_codex() {
        ctx.launcher.clone()
    } else {
        ctx.engine_launcher.clone()
    };
    rec.engine = ctx.engine.clone();
    let _ = store.write_pending(pending, &rec);
    Ok(rec)
}

/// (wave 27c, D2) The run warning recorded when the operator kicks the timeout CONTINUATION: the
/// continuation is cancelled but the run keeps the main turn's timeout outcome and its salvage.
const CONTINUATION_KICK_WARNING: &str =
    "kick: the operator stopped the timeout continuation (-Kick); the timeout outcome and its salvage stay";

/// (wave 27c, D2) The `on_running` callback for the timeout continuation turn: it rewrites the
/// pending record to `running` naming the CONTINUATION's child pid/start the moment it spawns, so a
/// concurrent `--kick` (and the liveness check) find a live child of this run — the main turn's child
/// was killed at the timeout. Built here so it is unit-testable in isolation.
fn continuation_on_running(
    store: &FilesStore,
    pending: &PendingRef,
    base: &PendingRecord,
) -> std::sync::Arc<dyn Fn(u32, String) + Send + Sync> {
    let cb_store = store.clone();
    let cb_pending = pending.clone();
    let cb_base = base.clone();
    std::sync::Arc::new(move |child_pid: u32, child_start: String| {
        let mut r = cb_base.clone();
        r.state = PendingState::Running;
        r.child_pid = Some(child_pid);
        r.child_start_time = child_start;
        r.note = "the timeout continuation turn is running".to_string();
        let _ = cb_store.write_pending(&cb_pending, &r);
    })
}

/// Ingest the outcome, render, commit and print the summary.
#[allow(clippy::too_many_arguments)]
fn finish(
    mut ctx: Context,
    store: FilesStore,
    pending: PendingRef,
    outcome: AttemptOutcome,
    base_record: PendingRecord,
    detail: EngineDetail,
    collab_before: std::collections::HashMap<String, String>,
    when_iso: String,
    // (rows (e)) `Some(err)` when the engine process could not be registered: the child was already
    // stopped in the `on_running` callback, so this run fails closed — the outcome names the
    // failure, no findings/verdict, and the recovery record is left at `launching`.
    register_failure: Option<String>,
) -> i32 {
    let is_engine = !ctx.is_codex();
    let register_failure_text = register_failure.map(|err| {
        format!(
            "failed: could not register the {} process ({err}); {} was stopped",
            ctx.engine, ctx.engine
        )
    });
    // (M7b-b) The http seat's provider_config (`{engine, base_url, model, pack, pack_sha256}`) is
    // built by the adapter from the retained pack; apply it to the identity so `build_reviewer`
    // places it on the ledger's `reviewer.provider_config`.
    if let Some(pc) = detail.http_provider_config.clone() {
        ctx.identity.provider_config = pc;
    }
    let mut bridge_outcome;
    let mut structured: Option<StructuredReply> = None;
    let mut raw_text = String::new();
    let mut usage: Option<Usage> = None;
    let mut wall_seconds = 0.0_f64;
    let mut thread = String::new();
    let mut thread_source = "unknown".to_string();
    let mut validation_error = String::new();
    let mut provider_failure = None;
    let mut usable = false;
    let mut main_timed_out = false;
    // (wave 26b, D12) a stall kill's ledger record `{seconds, last_event}`; `None` unless a stall
    // fired. (wave 26b, D10) whether the operator stopped this run with -Kick.
    let mut stall_record: Option<c3_core::ledger::Stall> = None;
    let mut run_kicked = false;
    let mut thread_candidate = String::new();
    // The engine adapter's classified failure (correct class/code/message from the turn's
    // evidence); finalized at commit time with retry_after/kind/when and the forced class.
    let mut engine_pf: Option<c3_core::ledger::ProviderFailure> = None;
    // A failed MAIN codex turn: its `failed: codex exit N - <detail>` outcome is built AFTER the
    // event/stderr evidence is read (`codex-consult.ps1:3840`), so remember the exit code here.
    let mut main_provider_failure = false;
    let mut main_pf_exit: Option<i32> = None;
    // Pids that survived a timeout kill: their recovery record is kept (`survivors`) so the
    // next run for this task is refused until they exit.
    let mut timeout_survivors: Vec<u32> = Vec::new();
    // (wave 27c, D16) `Some((pid, why))` when a process-tree kill could not be confirmed to have
    // stopped the root the bridge started: the outcome says so, the ledger records
    // `kill_confirmed: false`, a warning is written and no continuation turn runs on that thread.
    let mut kill_unconfirmed: Option<(u32, String)> = None;
    // (wave 27c, D16) the ledger's `kill_confirmed`: `None` when no tree kill happened this run
    // (recorded as JSON `null`), `Some(true)` when a kill was confirmed to have stopped the root,
    // `Some(false)` when it could not be confirmed.
    let mut kill_confirmed_state: Option<bool> = None;
    // (wave 28e, E1 / E18 / E23) the main turn's kill check (`None`: no kill).
    let mut main_kill: Option<c3_core::engine::KillCheck> = None;

    match outcome {
        AttemptOutcome::Completed(reply) => {
            raw_text = reply.raw_text.clone();
            usage = reply.usage.clone();
            wall_seconds = reply.wall_seconds;
            if let c3_core::engine::ConversationTrust::Verified(c) = &reply.conversation {
                thread = c.0.clone();
                thread_source = "events".into();
            }
            bridge_outcome = "usable reply".to_string();
            usable = true;
        }
        AttemptOutcome::TimedOut {
            survivors,
            wall_seconds: w,
            conversation,
            kill,
            ..
        } => {
            wall_seconds = w;
            main_timed_out = true;
            // A killed codex turn still emitted `thread.started`, so its events file carries the
            // thread; take it as the resume target (source `events`). A killed ENGINE turn never
            // verifies a thread (no clean result), so its conversation id is only a candidate.
            match &conversation {
                c3_core::engine::ConversationTrust::Verified(c) if !c.0.is_empty() => {
                    thread = c.0.clone();
                    thread_source = "events".into();
                }
                c3_core::engine::ConversationTrust::Candidate(c) if !c.0.is_empty() => {
                    if is_engine {
                        thread_candidate = c.0.clone();
                    } else {
                        thread = c.0.clone();
                        thread_source = "events".into();
                    }
                }
                _ => {}
            }
            timeout_survivors = survivors;
            // (wave 27c, D16; 28e, E1/E18) the tree kill CONFIRMED: "(process tree killed)" only
            // then; survivors and descendants it could not verify are named, and the record is kept.
            let check = main_kill_check(kill, &base_record, &timeout_survivors);
            bridge_outcome = main_kill_outcome(
                &format!("timeout after {} s", ctx.r.timeout_sec),
                &check,
                &timeout_survivors,
            );
            kill_unconfirmed = (!check.confirmed && timeout_survivors.is_empty())
                .then(|| (check.root_pid, check.why.clone()));
            kill_confirmed_state = Some(check.confirmed);
            main_kill = Some(check);
        }
        AttemptOutcome::Stopped {
            kind,
            survivors,
            wall_seconds: w,
            conversation,
            kill,
            ..
        } => {
            wall_seconds = w;
            // Thread extraction, exactly as the timeout path (a killed turn still emitted the
            // thread; a killed engine turn leaves only a candidate).
            match &conversation {
                c3_core::engine::ConversationTrust::Verified(c) if !c.0.is_empty() => {
                    thread = c.0.clone();
                    thread_source = "events".into();
                }
                c3_core::engine::ConversationTrust::Candidate(c) if !c.0.is_empty() => {
                    if is_engine {
                        thread_candidate = c.0.clone();
                    } else {
                        thread = c.0.clone();
                        thread_source = "events".into();
                    }
                }
                _ => {}
            }
            // (wave 27c, D6) the stall stop text, with the tool-open clause when a tool call was
            // open at the kill; reused by the unconfirmed-kill tail below.
            let mut stall_stop_text = String::new();
            match kind {
                c3_core::engine::StopKind::Stall {
                    last_event,
                    silent_seconds,
                    tool_open_seconds,
                    open_tools,
                } => {
                    // (wave 26b, D12) a stall is stopped like a timeout: one continuation turn
                    // follows (gated by --continue-sec) and the salvage is written.
                    main_timed_out = true;
                    stall_record = Some(c3_core::ledger::Stall {
                        seconds: ctx.stall_sec,
                        last_event,
                        extra: Default::default(),
                    });
                    let mut stop_text =
                        format!("stalled after {} s without an event", ctx.stall_sec);
                    // (wave 28b, D12) the cut names the open call(s).
                    if tool_open_seconds > 0 || !open_tools.is_empty() {
                        let named = if open_tools.is_empty() {
                            String::new()
                        } else {
                            format!(": {open_tools}")
                        };
                        stop_text += &format!(
                            " - no output for {silent_seconds} s (a tool call open for {tool_open_seconds} s{named})"
                        );
                    }
                    stall_stop_text = stop_text.clone();
                    bridge_outcome = format!("failed: {stop_text} (process tree killed)");
                }
                c3_core::engine::StopKind::Kick => {
                    // (wave 26b, D10) the operator stopped it: no continuation; the salvage follows
                    // and the provider failure is class `operator`.
                    run_kicked = true;
                    bridge_outcome = "failed: stopped by the operator (-Kick)".to_string();
                    // Consume this run's kick file (the -Kick command waits for it to disappear).
                    let _ = std::fs::remove_file(&ctx.kick_path);
                }
            }
            timeout_survivors = survivors;
            // (wave 27c, D16; 28e, E1/E18) a stall or kick kill is a process-tree kill too: told
            // as the timeout's (a kick names the kill only when it was not confirmed, or left
            // survivors or descendants it could not verify).
            let check = main_kill_check(kill, &base_record, &timeout_survivors);
            let unconfirmed_main = !check.confirmed && timeout_survivors.is_empty();
            if main_timed_out {
                bridge_outcome = main_kill_outcome(&stall_stop_text, &check, &timeout_survivors);
            } else if !timeout_survivors.is_empty() || !check.unverified.is_empty() {
                bridge_outcome = main_kill_outcome(
                    "stopped by the operator (-Kick)",
                    &check,
                    &timeout_survivors,
                );
            } else if unconfirmed_main {
                bridge_outcome = format!(
                    "failed: stopped by the operator (-Kick) {}",
                    format_kill_text(&check, &timeout_survivors)
                );
            }
            kill_unconfirmed = unconfirmed_main.then(|| (check.root_pid, check.why.clone()));
            kill_confirmed_state = Some(check.confirmed);
            main_kill = Some(check);
        }
        AttemptOutcome::ProviderFailure {
            failure: pf,
            exit_code,
        } => {
            if is_engine {
                // The engine adapter already framed the outcome (`AgyTurn`/`MuseTurn` `.outcome`):
                // `failed: agy exit N - ...`, `failed: muse terminal ...`, etc. The classified
                // provider_failure is (re)built after the tree check so its forced class (D12)
                // can outrank the turn's own class.
                bridge_outcome = detail.turn_outcome.clone();
                thread = detail.thread.clone();
                if !thread.is_empty() {
                    thread_source = "events".into();
                }
                // A failing engine turn may still have produced a reply (e.g. a resume that
                // landed on a new conversation): keep it so `.reply.json` is written and named,
                // though nothing is ingested from a failed run.
                raw_text = detail.reply.clone();
                // The adapter classified the failure from the turn's evidence (its first failure
                // text): the correct class/code/message (verbatim reason for a quota terminal).
                engine_pf = Some(pf);
            } else {
                main_provider_failure = true;
                main_pf_exit = exit_code;
                // Provisional; rebuilt below once the event/stderr evidence has been read so the
                // detail matches the plugin's `codex exit N - <event error | last stderr line>`.
                bridge_outcome = format!("failed: {} - {}", pf.class, pf.message);
            }
        }
        AttemptOutcome::LaunchFailed { message, .. } => {
            bridge_outcome = format!("failed: could not start {} - {message}", ctx.engine);
        }
        AttemptOutcome::Cancelled => {
            bridge_outcome = "failed: cancelled".to_string();
        }
    }

    // (wave 3c, F23-3) The main turn's kill writes the record it keeps NOW - survivors, unverified
    // pids or an unknown tree - before anything else of the run proceeds (the plugin's kill site):
    // a bridge that dies from here on leaves that evidence on disk. A failed write is said in the
    // outcome, as the plugin's (the end of the run writes it again).
    let kill_site = KillSite {
        store: store.clone(),
        pending: pending.clone(),
        on: base_record.clone(),
        pause_ms: test_hook_ms(
            &c3_core::test_hooks::hook("CODEX_CONSULT_TEST_KILL_PAUSE_MS").unwrap_or_default(),
            &ctx.identity.model,
        )
        .unwrap_or(0),
    };
    let mut main_kept: Option<KeptKill> = None;
    if register_failure_text.is_none() {
        if let Some(mut k) = main_kill
            .as_ref()
            .and_then(|c| kept_kill(c, &timeout_survivors))
        {
            if let Err(e) = kill_site.write(&mut k) {
                let what = if !k.survivors.is_empty() {
                    "survivors"
                } else if !k.unverified.is_empty() {
                    "unverified pids"
                } else {
                    "unconfirmed kill"
                };
                let child = base_record
                    .child_pid
                    .or(main_kill.as_ref().map(|c| c.root_pid))
                    .unwrap_or(0);
                bridge_outcome += &format!(
                    "; WARNING: the {what} could not be recorded ({}) - {} still names only child pid {child}",
                    c3_core::one_line(&e.to_string()),
                    store_pending_path(&store, &pending).display()
                );
            }
            main_kept = Some(k);
        }
    }

    // Rollout-file thread verification (`Find-ThreadInRollouts`): when the event stream named
    // no thread, look at codex's rollout files written since the run started. A rollout whose
    // name carries a uuid AND whose content holds this run's consultation id verifies the
    // thread; a newer unrelated rollout is only an unverified `thread_candidate` (never a
    // thread or a parent).
    if is_engine {
        // An engine turn's thread comes only from its own event stream; a failure that observed
        // a conversation id keeps it as an unverified candidate (never a thread/parent). A timed
        // -out engine turn already set `thread_candidate` above.
        if thread.is_empty() && thread_candidate.is_empty() {
            thread_candidate = detail.thread_candidate.clone();
        }
    } else if thread.is_empty() && usable {
        let started_at = chrono::DateTime::parse_from_rfc3339(&base_record.started)
            .map(|d| d.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now());
        let (t, cand) = crate::engines::codex::find_thread_in_rollouts(
            &providers::get_codex_home(),
            started_at,
            &ctx.consult_id,
        );
        if !t.is_empty() {
            thread = t;
            thread_source = "rollout (verified by consultation id)".into();
        } else {
            thread_candidate = cand;
        }
    }

    // Live drift (`Compare-TreeContent` + brief re-hash). The turn ran in `run_live` before
    // this; re-fingerprint the tree and re-hash the brief now. A file's CONTENT changing is a
    // tree change (warning); HEAD moving with identical content is only a `revision_moved`
    // note. The brief changing during the review is a warning too.
    let rev_after = revision::revision_info(&ctx.repo_root, Some(&ctx.collab_root));
    let tree_cmp = revision::compare_tree_content(&ctx.revision, &rev_after);
    let brief_sha_after = ctx
        .brief_path
        .as_ref()
        .and_then(|p| std::fs::read(p).ok())
        .map(|b| c3_core::sha256_hex(&b))
        .unwrap_or_default();
    // Artifact drift: rehash each bound artifact from its absolute path and compare
    // (`codex-consult.ps1:4429`). A changed artifact is a WARNING and sets the ledger flag.
    let mut artifact_records: Vec<serde_json::Value> = Vec::new();
    let mut artifacts_changed_paths: Vec<String> = Vec::new();
    for a in &ctx.artifacts {
        let after = std::fs::read(&a.full)
            .map(|b| c3_core::sha256_hex(&b))
            .unwrap_or_else(|_| "missing".into());
        if after != a.sha256 {
            artifacts_changed_paths.push(a.path.clone());
        }
        artifact_records.push(serde_json::json!({
            "path": a.path,
            "sha256": a.sha256,
            "sha256_after": after,
        }));
    }
    let drift = Drift {
        tree_sha256_after: rev_after.tree_sha256.clone(),
        tree_changed: tree_cmp.changed,
        tree_changed_paths: tree_cmp.paths.clone(),
        revision_moved: tree_cmp.revision_moved.clone(),
        brief_sha_after: brief_sha_after.clone(),
        brief_changed: !ctx.brief_sha.is_empty() && ctx.brief_sha != brief_sha_after,
        artifacts: artifact_records,
        artifacts_changed_paths,
    };

    // (rows (e)) A registration failure overrides everything: the child is already stopped, so the
    // outcome is the "could not register" text, no secondary turn runs, no findings are ingested.
    if let Some(txt) = &register_failure_text {
        bridge_outcome = txt.clone();
        usable = false;
        structured = None;
        main_timed_out = false;
        timeout_survivors.clear();
        kill_unconfirmed = None;
        kill_confirmed_state = None;
        main_kill = None;
        stall_record = None;
    }

    // ------------------------------------------------------------- secondary turns
    // The two codex mechanisms (`codex-consult.ps1` waves 24/24b/24c): the timeout continuation
    // (one `resume <thread>` turn after the main turn was killed) and the format repair (one
    // `resume <thread>` turn that converts a prose reply into the object). Both run under the
    // same task lock and recovery record, at most once.
    let mut sec = Secondary {
        kill_site: Some(kill_site.clone()),
        main_kept: main_kept.clone(),
        ..Secondary::default()
    };
    let main_events_text = std::fs::read_to_string(&ctx.events_path).unwrap_or_default();
    // (wave 26b, D15) a codex run that failed mid-run still emitted `thread.started`; take that
    // thread so the salvage footer can name it for a resume.
    if !is_engine && thread.is_empty() {
        let t = crate::engines::codex::parse_thread_id(&main_events_text);
        if !t.is_empty() {
            thread = t;
            thread_source = "events".into();
        }
    }
    let main_stderr = std::fs::read_to_string(&ctx.stderr_path).unwrap_or_default();
    let main_event_error = crate::engines::codex::parse_error(&main_events_text);
    // The forced provider_failure class for an engine run: the D12 tree check forces `permission`,
    // outranking a turn's own class; else the turn's class.
    let mut engine_forced_class = detail.forced_class.clone();
    sec.msp_version = detail.msp_schema_version;

    // Engine (agy/muse) read-only tree check + denial retry, after the main turn and before the
    // secondary turns. A write to the working tree or the collab dir fails the run
    // (`Get-EngineTreeProblem`, D12): forced class `permission`, the reply discarded.
    if is_engine {
        sec.engine_turns = 1;
        // (STEP 2) The http seat runs its own secondary turn (a format-repair replay or a timeout
        // retry) in-process; copy its result into the secondary record so the ledger and handoff
        // show `engine_turns: 2` and the `format_repair` fields via the existing recording path.
        if let Some(hs) = &detail.http_secondary {
            sec.engine_turns = hs.engine_turns;
            sec.repaired_ok = hs.repaired_ok;
            sec.repair_reason = hs.repair_reason.clone();
            sec.original_rel = hs.original_rel.clone();
            sec.original_prose = hs.original_prose.clone();
            sec.drift_notes = hs.drift_notes.clone();
            sec.format_retry = hs.format_retry.clone();
        }
        // The engine warnings of a usable primary turn (denial notices, engine stderr warnings).
        if usable {
            sec.engine_warnings.extend(detail.warnings.clone());
        }
        let collab_after = crate::engines::tree_check::collab_snapshot(&ctx.collab_root);
        let check = engine_tree_check(&ctx, &drift, &collab_before, &collab_after);
        sec.tree_check_outcome = check.outcome.clone();
        sec.tree_check_files = check.files.clone();
        // A write-disabled engine (muse) WARNS: the reply stays usable, the change is recorded as
        // a warning (D9). agy FAILS: forced class `permission`, the reply discarded (D12).
        for w in &check.warnings {
            sec.engine_warnings.push(w.clone());
        }
        let tree_problem = check.problem;
        if !tree_problem.is_empty() {
            engine_forced_class = "permission".to_string();
            if bridge_outcome == "usable reply" {
                bridge_outcome = format!("failed: {tree_problem}");
                usable = false;
                structured = None;
            } else {
                bridge_outcome = format!("{bridge_outcome}; also: {tree_problem}");
            }
        }
        sec.tree_problem = tree_problem;
        run_engine_denial_retry(
            &ctx,
            &detail,
            &sec.tree_problem.clone(),
            &mut sec,
            &mut bridge_outcome,
            &mut raw_text,
            &mut thread,
            &mut thread_source,
            &mut usable,
            &mut engine_forced_class,
        );
    }

    // A failed main turn's outcome (`codex-consult.ps1:3840-3847`): `failed: codex exit N`, with
    // ` - <detail>` where detail is the event error, else the last non-empty stderr line. (rows (e),
    // wave 28b D19) never over a registration failure: its "could not register" outcome stands (the
    // child it stopped exits non-zero).
    if main_provider_failure && register_failure_text.is_none() {
        if let Some(n) = main_pf_exit.filter(|n| *n != 0) {
            let detail = if !main_event_error.is_empty() {
                main_event_error.clone()
            } else {
                main_stderr
                    .split(['\r', '\n'])
                    .map(|l| l.trim())
                    .rfind(|l| !l.is_empty())
                    .unwrap_or("")
                    .to_string()
            };
            let tail = if detail.is_empty() {
                String::new()
            } else {
                format!(" - {detail}")
            };
            bridge_outcome = format!("failed: codex exit {n}{tail}");
        }
    }

    if main_timed_out {
        // (wave 27c, D2) the continuation turn registers its own child in the pending record, so a
        // concurrent `--kick` during it finds a live child of this run.
        let cont_on_running = continuation_on_running(&store, &pending, &base_record);
        run_timeout_continuation(
            &ctx,
            &drift,
            &timeout_survivors,
            kill_unconfirmed.as_ref(),
            &main_stderr,
            &main_event_error,
            wall_seconds,
            &mut sec,
            &mut bridge_outcome,
            &mut raw_text,
            &mut thread,
            &mut thread_source,
            &mut usable,
            &mut provider_failure,
            stall_record.as_ref().map(|s| s.seconds),
            Some(cont_on_running),
        );
    }
    // (wave 27c, D2) a kick that cancelled the timeout continuation: warn, but keep the timeout
    // outcome and its salvage (bridge_outcome is left as the main turn's timeout text).
    if sec.continue_kicked {
        ctx.run_warnings.push(CONTINUATION_KICK_WARNING.to_string());
    }

    // Structured / prose classification (`ConvertFrom-StructuredReply`), on a usable, non-raw
    // reply — the main reply, or a continuation that answered.
    if !ctx.r.raw && usable {
        match crate::engines::codex::parse_structured(&raw_text) {
            Some(s) => structured = Some(s),
            None => {
                validation_error = ingest::first_validation_error_engine(&raw_text, &ctx.engine)
            }
        }
    }

    // The handoff's `Structured reply: INVALID (...)` line names the FIRST reply's parse error
    // (`$parse.ValidationError`), never the `(format repair …)` suffix the ledger's
    // `validation_error` carries — capture it before the repair turn augments the ledger value.
    let base_validation_error = validation_error.clone();

    // Format repair: a substantive prose reply on a verified thread earns ONE convert-only turn.
    if structured.is_none() {
        run_format_repair(
            &ctx,
            &store,
            &pending,
            &base_record,
            &thread,
            &thread_source,
            usable,
            &mut sec,
            &mut structured,
            &mut raw_text,
            &mut validation_error,
        );
    }

    // (wave 26c, D1) the operator stopped only the FORMAT REPAIR: the first reply stands (not
    // converted), no operator class — a warning, and the run stays a usable reply.
    if sec.repair_kicked {
        ctx.run_warnings.push(
            "kick: the operator stopped the format repair (-Kick); the first reply stands, not converted".to_string(),
        );
    }

    // (F04-11) Preserve the raw reply as `.reply.json` at the handoff path BEFORE the write lock
    // (README "Write order": `.reply.json` first), so a crash during the commit still leaves the
    // raw reply recoverable. `raw_text` is now final (after any continuation and format repair).
    // A copy that FAILS (e.g. the target path is blocked by a directory) is a total bridge
    // failure: the original is kept, the outcome says so, and no findings/verdict are ingested.
    let mut reply_json_rel = if !ctx.r.raw && !raw_text.is_empty() {
        ctx.hf("reply.json")
    } else {
        String::new()
    };
    let mut keep_last_msg = false;
    if !reply_json_rel.is_empty() {
        if let Err(e) = c3_core::store::write_text_atomic(&ctx.reply_json_path, raw_text.as_bytes())
        {
            let note = format!(
                "could not preserve the raw reply ({}); original kept at {}",
                e,
                ctx.last_msg_path.display()
            );
            if bridge_outcome == "usable reply" {
                bridge_outcome = format!("failed: {note}");
            }
            reply_json_rel = String::new();
            structured = None;
            usable = false;
            validation_error = String::new();
            keep_last_msg = true;
        }
    }

    // Count the secondary engine turns that ran (the base primary + a denial retry counted in
    // `run_engine_denial_retry`; here the continuation and the format repair).
    if is_engine {
        if sec.continue_ran {
            sec.engine_turns += 1;
        }
        if sec.format_retry.is_some() {
            sec.engine_turns += 1;
        }
    }

    // (wave 26b, D10) a turn the operator stopped (-Kick): the failure is by the operator's hand,
    // class `operator` (never an endpoint's fault; it is excluded from the machine health file).
    if run_kicked && provider_failure.is_none() {
        provider_failure = Some(c3_core::ledger::ProviderFailure {
            class: "operator".into(),
            message: "stopped by the operator (-Kick)".into(),
            ..Default::default()
        });
    }
    // A failed run records a classified provider_failure derived from its evidence
    // (`codex-consult.ps1:4093`), unless a continuation already supplied one. An engine failure
    // keeps the adapter's classified failure (verbatim reason/class from the turn), with the
    // tree check's forced `permission` (D12) outranking it, and a parsed reset time.
    if !usable && provider_failure.is_none() {
        provider_failure = Some(if is_engine {
            let base = engine_pf.take().unwrap_or_else(|| {
                let reason = bridge_outcome
                    .strip_prefix("failed: ")
                    .unwrap_or(&bridge_outcome);
                let (code, message) = c3_core::health::convert_from_provider_error_text(reason);
                let class = c3_core::health::provider_failure_class(&format!("{code} {message}"));
                c3_core::ledger::ProviderFailure {
                    class,
                    code,
                    message,
                    ..Default::default()
                }
            });
            finalize_engine_pf(base, &engine_forced_class)
        } else {
            codex_failure_pf(&bridge_outcome, &main_event_error, &main_stderr)
        });
    }

    // (`codex-consult.ps1:4106`) a usable reply produced by the continuation says so everywhere.
    if sec.continued && usable {
        bridge_outcome = "usable reply (after a timeout continuation)".to_string();
    }

    // Salvaged partial reply: a turn the bridge killed on its timeout with no usable
    // continuation leaves `handoffs/NN-codex-<slug>.partial.md` (`codex-consult.ps1:4108`).
    build_partial_reply(
        &ctx,
        &main_events_text,
        &mut sec,
        main_timed_out,
        wall_seconds,
        stall_record.is_some(),
        run_kicked,
        usable,
        &bridge_outcome,
        &thread,
    );

    // Build the finding delta + ids for a structured reply.
    let mut finding_ids: Vec<String> = Vec::new();
    let mut delta = FindingsDelta::default();
    let mut counts = FindingCounts::default();
    if let Some(s) = &structured {
        counts = render::severity_counts(s);
        let handoff_rel = ctx.hf("md");
        for (k, rf) in s.findings.iter().enumerate() {
            let id = c3_core::findings::finding_id(ctx.nn, k + 1);
            finding_ids.push(id.clone());
            delta
                .new
                .push(build_finding(&ctx, &id, rf, &handoff_rel, &thread));
        }
    }

    // Prior-finding lifecycle (`Add-ReplyFindings` + `Test-ReplySemantics`): ingest the reply's
    // `prior_findings` reports onto the stored findings' `reviewer_checks[]`, fold each new
    // finding's `supersedes` into the old finding's `superseded_by[]`, run the verdict-vs-purpose
    // (F04-6) and ACCEPT-vs-blocker/still-open-prior-blocker (F04-4) semantics.
    let mut semantics = super::semantics::Semantics::default();
    let mut prior_ledger: Vec<c3_core::ledger::PriorFindingRef> = Vec::new();
    let mut unknown_prior_ids: Vec<String> = Vec::new();
    let mut unknown_supersedes: Vec<String> = Vec::new();
    // (F04-4) OPEN prior blockers carried into the handoff's ### Blockers, with the stored finding's
    // location and claim and this reply's disposition.
    let mut prior_blocker_lines: Vec<render::PriorBlockerLine> = Vec::new();
    if let Some(s) = &structured {
        // The findings store as it stands (read fresh; the commit re-reads under the lock).
        let existing = store.read_findings(&ctx.task).ok().flatten();
        let known: std::collections::HashSet<String> = existing
            .as_ref()
            .map(|f| f.findings.iter().map(|fd| fd.id.clone()).collect())
            .unwrap_or_default();
        let open_priors: Vec<super::semantics::OpenPrior> = existing
            .as_ref()
            .map(|f| {
                f.findings
                    .iter()
                    .map(|fd| super::semantics::OpenPrior {
                        id: fd.id.clone(),
                        severity: fd.severity.clone(),
                        status: fd.status().as_str().to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let now = iso_now();
        for p in &s.prior_findings {
            let status = match p.status {
                c3_core::engine::PriorStatus::Fixed => "fixed",
                c3_core::engine::PriorStatus::StillOpen => "still-open",
                c3_core::engine::PriorStatus::NotChecked => "not-checked",
                c3_core::engine::PriorStatus::UnknownId => "unknown-id",
            };
            prior_ledger.push(c3_core::ledger::PriorFindingRef {
                id: p.id.clone(),
                status: status.to_string(),
                ..Default::default()
            });
            if known.contains(&p.id) {
                delta.reviewer_checks.push((
                    p.id.clone(),
                    c3_core::findings::ReviewerCheck {
                        consult: ctx.consult_n,
                        when: now.clone(),
                        status: status.to_string(),
                        note: p.note.clone(),
                        base_commit: ctx.revision.base_commit.clone(),
                        tree_sha256: ctx.revision.tree_sha256.clone(),
                        ..Default::default()
                    },
                ));
            } else {
                unknown_prior_ids.push(p.id.clone());
            }
        }
        // supersedes: each NEW finding names old ids; the old finding gains this new id.
        for (k, rf) in s.findings.iter().enumerate() {
            let new_id = c3_core::findings::finding_id(ctx.nn, k + 1);
            for old in &rf.supersedes {
                if known.contains(old) {
                    delta.superseded_by.push((old.clone(), new_id.clone()));
                } else if !unknown_supersedes.contains(old) {
                    unknown_supersedes.push(old.clone());
                }
            }
        }
        semantics = super::semantics::test_reply_semantics(s, &ctx.o.purpose, &open_priors);
        if let Some(ff) = &existing {
            for pb in super::semantics::prior_blocker_dispositions(s, &open_priors) {
                if let Some(fd) = ff.findings.iter().find(|f| f.id == pb.id) {
                    let location = fd
                        .locations
                        .first()
                        .map(|l| match l.line {
                            Some(n) => format!("`{}:{}`", l.path, n),
                            None => format!("`{}`", l.path),
                        })
                        .unwrap_or_default();
                    prior_blocker_lines.push(render::PriorBlockerLine {
                        id: pb.id,
                        disposition: pb.disposition,
                        location,
                        claim: fd.claim.clone(),
                    });
                }
            }
        }
    }
    let verdict_warning_line = semantics.warning.as_ref().map(|w| format!("WARNING: {w}"));
    // ACCEPT that leaves prior blockers unchecked keeps its verdict but earns an operator
    // WARNING - shown on the console and in the handoff (a run warning, before it is rendered).
    if let Some(w) = &semantics.warning {
        ctx.run_warnings.push(w.clone());
    }

    // (wave 28c, D11) a reviewer that compacted its context is seen: the compactions its engine
    // REPORTED in the event streams of every turn - n > 0 is recorded (ledger `compactions`) and
    // warned about; none reported by a reviewer with a context window (context_tokens) is
    // `unknown` (the installed codex's `exec --json` reports no compaction event: none seen is not
    // none happened); otherwise null.
    let compaction_warning = {
        let cont = ctx.hpath("continue.events.jsonl");
        let retry = ctx.hpath("denial-retry.events.jsonl");
        let repair = ctx.hpath("repair.events.jsonl");
        let mut paths: Vec<&Path> =
            vec![ctx.events_path.as_path(), retry.as_path(), cont.as_path()];
        if !ctx.is_codex() {
            paths.push(repair.as_path());
        }
        let n = compaction_count(&paths) + sec.repair_compactions;
        if n > 0 {
            sec.compactions = Some(serde_json::Value::from(n));
            let w = format!(
                "the reviewer compacted its context {n} time(s) - the reply may rest on a summary of the brief"
            );
            sec.engine_warnings.push(w.clone());
            Some(w)
        } else {
            if ctx.context_tokens > 0 {
                sec.compactions = Some(serde_json::Value::String("unknown".into()));
            }
            None
        }
    };

    // Render the handoff markdown (and the salvaged partial file, when a turn was killed).
    let (handoff_md, partial_md) = render_handoff(
        &ctx,
        &bridge_outcome,
        wall_seconds,
        &usage,
        &thread,
        &thread_source,
        &thread_candidate,
        structured.as_ref(),
        &finding_ids,
        &base_validation_error,
        provider_failure.as_ref(),
        &raw_text,
        &drift,
        &sec,
        &main_event_error,
        &main_stderr,
        &prior_blocker_lines,
        verdict_warning_line.as_deref(),
    );
    let handoff_rel = ctx.hf("md");
    let events_rel = ctx.hf("events.jsonl");
    // `.reply.json` was already written (or its failure handled as a total bridge failure) above,
    // before the format-repair/findings work, so `reply_json_rel` is final here.

    // Build the ledger entry.
    let mut entry = build_entry(
        &ctx,
        &bridge_outcome,
        wall_seconds,
        &usage,
        &thread,
        &thread_source,
        structured.as_ref(),
        &finding_ids,
        &counts,
        &validation_error,
        provider_failure.clone(),
        &handoff_rel,
        &reply_json_rel,
        &events_rel,
        &drift,
        &sec,
        stall_record.clone(),
    );
    // The ledger `when` is the run's START (`$startedAt`), not the commit time (`build_entry`
    // stamps `iso_now()` as a placeholder). This makes the panel's overlap check (last start <
    // first finish) hold for concurrent members.
    entry.when = when_iso;
    entry.thread_candidate = thread_candidate.clone();
    // (wave 27c, D16) `kill_confirmed`: present as `null` when no tree kill happened this run,
    // `true` when the kill was confirmed, `false` when it could not be. An unconfirmed kill also
    // pushes the warning naming the pid that may still run (the continuation was already
    // suppressed and the outcome already says so).
    // (wave 27c, D16) every kill of the run counts - the secondary turns' too: confirmed only when
    // every one was confirmed and left no survivor.
    if !sec.kill_checks.is_empty() {
        let secondary_ok = sec
            .kill_checks
            .iter()
            .all(|(c, s)| c.confirmed && s.is_empty());
        kill_confirmed_state = Some(kill_confirmed_state.unwrap_or(true) && secondary_ok);
    }
    entry.kill_confirmed = Some(kill_confirmed_state);
    // (wave 28c, D11) the compaction warning in warnings[] (an engine's already came with its turn
    // warnings; a codex entry records the run warnings).
    if let Some(w) = &compaction_warning {
        let v = serde_json::Value::String(w.clone());
        if !entry.warnings.contains(&v) {
            entry.warnings.push(v);
        }
    }
    // `Add-KillCheck`'s warnings: the main turn's, then the secondary turns'.
    let main_warning = main_kill
        .as_ref()
        .and_then(|c| kill_check_warning(c, &timeout_survivors, "main turn"));
    for w in main_warning
        .into_iter()
        .chain(sec.kill_warnings.iter().cloned())
    {
        entry.warnings.push(serde_json::Value::String(w));
    }
    // Prior-finding lifecycle records (F04-4): this reply's `prior_findings` reports, the
    // unchecked prior blockers, and — when the semantics contradict the verdict — the blanked
    // verdict and the `validation_error` naming the contradiction (the findings stay ingested).
    let prior_ledger_for_summary: Vec<(String, String)> = prior_ledger
        .iter()
        .map(|p| (p.id.clone(), p.status.clone()))
        .collect();
    entry.prior_findings = prior_ledger;
    entry.unchecked_prior_blockers = semantics
        .unchecked
        .iter()
        .map(|s| serde_json::Value::String(s.clone()))
        .collect();
    if structured.is_some() && semantics.verdict_invalid() {
        entry.verdict = String::new();
        entry.validation_error = semantics.validation_error();
    }

    // Keep a copy for telemetry (the entry is moved into the commit below).
    let entry_for_telemetry = if ctx.telemetry_enabled {
        Some(entry.clone())
    } else {
        None
    };

    // Commit under the write lock: the handoff `.md` (and the salvaged `.partial.md`). The raw
    // `.reply.json` was already written above, before the lock (crash-safety), so it is not in
    // the commit `files` list.
    let mut files = vec![(handoff_rel.clone(), handoff_md.clone().into_bytes())];
    if let Some(pm) = &partial_md {
        files.push((sec.partial_rel.clone(), pm.clone().into_bytes()));
    }
    // Timeout survivors: keep the recovery record in the `survivors` state (a live process of
    // this run is still out there) so the commit does not delete it and the next run is
    // refused until they exit. Written before the write lock, as the plugin does after the
    // kill (`codex-consult.ps1:3375`).
    // (wave 28e, E1 / E18 / E23) what the kills left that keeps the record: the main turn's, then a
    // secondary turn's (whose lists replace the main turn's; an unknown tree's why stays).
    // (wave 3c, F23-3) Each kill wrote it already, at the kill; it is written again here, with the
    // survivors' entries read at the kill, on the run's base record.
    let kept = merged_kept(main_kept.clone(), sec.kept_kill.clone());
    let mut kept_rec: Option<PendingRecord> = None;
    let disposition = if register_failure_text.is_some() {
        // (rows (e)) leave the record at `launching` (child cleared) so the next run recovers it;
        // the ledger entry below records the failed outcome.
        let mut launching = base_record.clone();
        launching.state = PendingState::Launching;
        launching.child_pid = None;
        launching.child_start_time = String::new();
        launching.note = "the engine process could not be registered; it was stopped".to_string();
        let _ = store.write_pending(&pending, &launching);
        RecoveryDisposition::Retain
    } else if let Some(k) = kept {
        // (wave 28e, E1 / E18 / E23) survivors OR descendants the kill could not verify OR an
        // unknown tree (`kill_unconfirmed`): the record is kept in state `survivors`.
        let survivor_rec = kept_record(&base_record, &k);
        let _ = store.write_pending(&pending, &survivor_rec);
        kept_rec = Some(survivor_rec);
        RecoveryDisposition::Retain
    } else {
        RecoveryDisposition::Remove
    };
    // `recovery record kept: <path> (state '<state>')` in the summary (`$pendingNote`).
    let pending_note = if disposition == RecoveryDisposition::Retain {
        let state = if register_failure_text.is_some() {
            "launching"
        } else {
            "survivors"
        };
        format!(
            "recovery record kept: {} (state '{state}')",
            store_pending_path(&store, &pending).display()
        )
    } else {
        String::new()
    };
    // (D2-D4) The run is over and its reply files are on disk. Mark the recovery record
    // `committing`, naming this run's kept `.reply.json` (collab-relative), BEFORE the write lock:
    // a commit that never completes — the lock never had (D3), or the bridge stopped inside it
    // (D4) — leaves a record that says where the reply is and stays `committing` for the next run
    // to consume. The timeout-survivors path keeps its own `survivors` record instead.
    let reply_json_record = if reply_json_rel.is_empty() {
        String::new()
    } else {
        c3_core::paths::repo_relative(&ctx.repo_root, &ctx.reply_json_path)
            .unwrap_or_else(|| ctx.reply_json_path.to_string_lossy().to_string())
    };
    // (wave 3c, F23-3) a kept record stays the kept one (a blocked commit below rewrites it with
    // its note, never with the base record that lacks the kill's evidence)
    let mut committing_record = kept_rec.unwrap_or_else(|| base_record.clone());
    if disposition == RecoveryDisposition::Remove {
        committing_record.state = PendingState::Committing;
        committing_record.note = "the run is over; committing under the write lock".to_string();
        if !reply_json_record.is_empty() {
            committing_record.reply_json = Some(reply_json_record.clone());
        }
        let _ = store.write_pending(&pending, &committing_record);
    }
    // (wave 26b, D13; 26c, D2; 27c, D7; 28b, D13 / F36-6, F37-1) the run's outcome on its endpoint
    // into the machine-wide health file (a usable reply clears the endpoint - class ok -, a provider
    // failure marks it, class operator excepted) - now, BEFORE the write lock, with the full budget
    // (3 x 5 s). The record is built ONCE: the same record goes into the journal at the commit and
    // into the retry after it (applying is idempotent). ANY failure is retried after the commit, its
    // cause named; the ledger keeps the truth either way. Disabled with CODEX_CONSULT_HEALTH=none.
    let health_repo = ctx.repo_root.to_string_lossy().to_string();
    let health_path = if ctx.identity.resolved {
        c3_core::health::machine_health_path(&providers::get_codex_home())
    } else {
        None
    };
    let health_record = health_path.as_ref().and_then(|_| {
        let failure = provider_failure
            .as_ref()
            .map(|pf| c3_core::health::MachineFailure {
                class: pf.class.clone(),
                kind: pf.kind.clone().unwrap_or_default(),
                when: pf.when.clone(),
                retry_after: pf.retry_after.clone(),
                message: pf.message.clone(),
            });
        c3_core::health::new_machine_health_record(
            &ctx.identity.fingerprint,
            &bridge_outcome,
            failure.as_ref(),
            &health_repo,
        )
    });
    let health_alive = |pid: u32, st: &str| crate::liveness::proc::pid_alive(pid, st);
    let mut health_retry_cause: Option<String> = None;
    if let (Some(hp), Some(rec)) = (&health_path, &health_record) {
        let res = c3_core::health::add_machine_health_endpoint(hp, rec, &health_alive);
        if res.failed() {
            health_retry_cause = Some(res.cause().unwrap_or_default());
        }
    }
    // (wave 28c, D10 / F42-6, F44-1) a journal line that could not be applied was moved aside, never
    // dropped silently: said in warnings[] and the summary (this run's updates so far - its
    // registration, its outcome).
    let mut health_summary_warnings: Vec<String> = Vec::new();
    for note in c3_core::health::take_machine_health_journal_notes() {
        let v = serde_json::Value::String(note.clone());
        if !entry.warnings.contains(&v) {
            entry.warnings.push(v);
            health_summary_warnings.push(note);
        }
    }
    // TEST HOOK: CODEX_CONSULT_TEST_COMMIT_PAUSE_MS=<ms> | <model>=<ms>[|...] — a pause held INSIDE
    // the commit, between findings.json and sessions.json (the ORPHAN window a kill can hit; the
    // write-lock contention window), applied by the store (`codex-consult.ps1:5176`).
    let commit_pause_ms = test_hook_ms(
        &c3_core::test_hooks::hook("CODEX_CONSULT_TEST_COMMIT_PAUSE_MS").unwrap_or_default(),
        &ctx.identity.model,
    )
    .unwrap_or(0);
    let write_lock = match store.take_write_lock(&ctx.task) {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
            // D3 commit blocked: the write lock was not acquired within the wait. Keep the reply,
            // leave the record `committing` naming it, touch no store, exit 1. The next run
            // consumes the record like any interrupted reservation.
            let wl_path = store.task_dir(&ctx.task).join(c3_core::store::WRITE_LOCK);
            let msg = format_write_lock_refusal(&wl_path, ctx.task.as_str());
            committing_record.note =
                format!("commit blocked: {msg}; findings.json and sessions.json were not touched");
            let _ = store.write_pending(&pending, &committing_record);
            let kept = if reply_json_record.is_empty() {
                "nothing to keep".to_string()
            } else {
                reply_json_record.clone()
            };
            let pending_path = store_pending_path(&store, &pending);
            return refuse(&format!(
                "commit blocked: {msg}. This run's reply is kept ({kept}); no ledger entry was written and the stores were not touched - {} stays in state committing and the next run consumes it (bridge outcome: {bridge_outcome}).",
                pending_path.display()
            ));
        }
        Err(e) => return refuse(&format!("could not take the write lock: {e}")),
    };
    // (wave 26c, D2 / 28b, D13) the machine-wide health update that failed before the commit: its
    // record goes into the JOURNAL beside the health file now, inside the write lock (a local append -
    // no wait for the health lock, so the hold on this task's lock does not grow); this entry's
    // warning says a retry follows the commit; the retry after the lock is released - or the next
    // run of any repository, should this one die first - applies the journal and empties it.
    let mut health_journal_failed = false;
    if let (Some(cause), Some(hp), Some(rec)) = (&health_retry_cause, &health_path, &health_record)
    {
        let mid = match c3_core::health::add_machine_health_journal(hp, rec) {
            Ok(()) => "the record is kept in the journal; ".to_string(),
            Err(why) => {
                health_journal_failed = true;
                format!("the journal could not be written ({why}); ")
            }
        };
        let w = format!(
            "machine-wide health not updated at the commit ({cause}); {mid}a retry follows the commit"
        );
        entry.warnings.push(serde_json::Value::String(w.clone()));
        health_summary_warnings.push(w);
    }

    // The write-lock wait (`commit_wait_ms`): 0 when the first attempt won it, else the measured
    // wait; a contended commit says so in a summary line (F11-3).
    let commit_wait_ms = write_lock.wait_ms() as i64;
    entry.commit_wait_ms = commit_wait_ms;
    let commit = CommitRequest {
        entry,
        findings: delta,
        pending: &pending,
        disposition,
        files: &files,
        bootstrap_cwd: ctx.repo_root.to_string_lossy().to_string(),
        bootstrap_tool: ctx.codex_version.clone(),
        commit_pause_ms,
    };
    let receipt = match store.commit(&write_lock, commit) {
        Ok(r) => r,
        Err(e) => return refuse(&format!("the commit failed: {e}")),
    };
    drop(write_lock);
    let _ = receipt;

    // (wave 27c, D8 / 28b, D13) the FULL machine-health retry (3 x 5 s), outside the write lock: it
    // applies the journal (this run's record and any other) and empties it; its OUTCOME, either way,
    // is in the summary (the console, and a detached run's status record). Never fails the run.
    let mut health_lines: Vec<String> = Vec::new();
    if let (Some(cause), Some(hp), Some(rec)) = (&health_retry_cause, &health_path, &health_record)
    {
        let res = c3_core::health::add_machine_health_endpoint(hp, rec, &health_alive);
        if res == c3_core::health::HealthUpdate::Written {
            health_lines.push(
                "health     : machine-wide health updated by the retry after the commit (the journal applied)"
                    .to_string(),
            );
        } else {
            let why = res.cause().unwrap_or_else(|| cause.clone());
            let waits = if health_journal_failed {
                String::new()
            } else {
                format!(
                    " - the record waits in {} for the next run",
                    c3_core::health::machine_health_journal_path(hp).display()
                )
            };
            health_lines.push(format!(
                "warning    : machine-wide health not updated by the retry after the commit ({why}){waits}"
            ));
        }
    }
    // (wave 28c, D10) the journal's unreadable lines found by the updates after the commit: the
    // summary
    for note in c3_core::health::take_machine_health_journal_notes() {
        health_lines.push(format!("warning    : {note}"));
    }

    // Telemetry: record this consultation to the spool (errors ignored) - after the commit, so an
    // event is never spooled for an uncommitted entry. The run's switch is re-checked inside
    // `record_consultation`; a dry run never reaches this point.
    // (wave 2b) an event that is not spooled is never lost silently: said on the console and
    // counted (`c3 telemetry --status`); the entry is already committed.
    if let Some(entry) = &entry_for_telemetry {
        if let Err(e) = telemetry::record_consultation(
            entry,
            None,
            &telemetry::Config {
                telemetry: ctx.o.telemetry,
            },
        ) {
            telemetry::note_not_spooled(&e.to_string());
            println!(
                "warning    : telemetry event not spooled ({e}) - counted (c3 telemetry --status)"
            );
        }
    }

    // Summary.
    let reply_body = structured
        .as_ref()
        .map(|s| {
            if s.reply_markdown.trim().is_empty() {
                "_(empty reply_markdown)_".to_string()
            } else {
                s.reply_markdown.clone()
            }
        })
        .unwrap_or_else(|| raw_text.trim().to_string());
    // The `structured : INVALID (...)` summary line (a prose reply kept as the reply of record);
    // it uses the FULL ledger validation_error (with the repair suffix), unlike the handoff.
    let structured_invalid =
        if !ctx.r.raw && usable && structured.is_none() && !raw_text.trim().is_empty() {
            format!(
                "structured : INVALID ({validation_error}) - raw text kept; no findings recorded"
            )
        } else {
            String::new()
        };
    let section = structured
        .as_ref()
        .map(|s| {
            render::format_structured_section(
                s,
                &finding_ids,
                &prior_blocker_lines,
                verdict_warning_line.as_deref(),
            )
        })
        .unwrap_or_default();

    let verdict_line = structured.as_ref().map(|s| {
        // A verdict the semantics invalidated (F04-4/6) shows `(invalid: <reason>)`, not the
        // reviewer's token (`codex-consult.ps1:4989`).
        if semantics.verdict_invalid() {
            return format!("verdict    : (invalid: {})", semantics.validation_error());
        }
        let v = match s.verdict {
            c3_core::engine::Verdict::Accept => "ACCEPT",
            c3_core::engine::Verdict::Hold => "HOLD",
            c3_core::engine::Verdict::Reject => "REJECT",
            c3_core::engine::Verdict::Advise => "ADVISE",
        };
        format!(
            "verdict    : {} - {}",
            v,
            c3_core::one_line(&s.verdict_reason)
        )
    });
    // The `prior      :`/`unknown ids:`/`supersedes :` lines (`codex-consult.ps1:4995-5004`).
    let prior_line = if prior_ledger_for_summary.is_empty() {
        String::new()
    } else {
        format!(
            "prior      : {}",
            prior_ledger_for_summary
                .iter()
                .map(|(id, st)| format!("{id} {st}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let unknown_ids_line = if unknown_prior_ids.is_empty() {
        String::new()
    } else {
        format!(
            "unknown ids: {} (not in findings.json; ignored)",
            unknown_prior_ids.join(", ")
        )
    };
    let supersedes_line = if unknown_supersedes.is_empty() {
        String::new()
    } else {
        format!(
            "supersedes : {} not in findings.json (kept on the new finding only)",
            unknown_supersedes.join(", ")
        )
    };
    let findings_line = structured.as_ref().map(|_| {
        if finding_ids.is_empty() {
            "findings   : none".to_string()
        } else {
            format!(
                "findings   : {} -> {} in findings.json",
                render::format_severity_counts(&counts),
                render::format_id_range(&finding_ids)
            )
        }
    });

    // The `continued  :` console line (`codex-consult.ps1:4559`).
    let continue_line = sec.timeout_continue.as_ref().map(|tc| {
        if sec.continued {
            format!(
                "continued  : the main turn was killed at {} s of {} s; one continuation turn on thread {} answered in {} s",
                fmt_wall(wall_seconds),
                ctx.r.timeout_sec,
                sec.continue_thread,
                fmt_wall(sec.continue_wall),
            )
        } else {
            let mut l = format!("continued  : {}", tc.outcome);
            if sec.continue_wall > 0.0 || tc.events.is_some() {
                l.push_str(&format!(" (in {} s)", fmt_wall(sec.continue_wall)));
            }
            if !sec.continue_thread.is_empty() {
                l.push_str(&format!(" - thread {}", sec.continue_thread));
            }
            l
        }
    });
    let partial_abs = if sec.partial_needed {
        ctx.hpath("partial.md").to_string_lossy().to_string()
    } else {
        String::new()
    };
    // The summary/handoff show the reviewer lineage with the engine tag (`... [agy]`); the ledger
    // `lineage` field stays bare.
    let lineage_shown = c3_core::lineage::format_reviewer_lineage(
        &ctx.identity.provider,
        &ctx.identity.model,
        &ctx.engine,
    );
    let s = summary::SummaryInputs {
        bridge_outcome: bridge_outcome.clone(),
        usable,
        wall_seconds: fmt_wall(wall_seconds),
        lineage_shown,
        mode: ctx.effective_mode.clone(),
        mode_fallback: ctx
            .mode_fallback
            .as_ref()
            .map(|mf| (mf.from.clone(), mf.reason.clone())),
        thread: thread.clone(),
        thread_source: thread_source.clone(),
        thread_candidate: thread_candidate.clone(),
        consult_id: ctx.consult_id.clone(),
        continue_line: continue_line.unwrap_or_default(),
        repair_console: sec.repair_console.clone(),
        repair_drift: sec.drift_notes.clone(),
        partial_path: partial_abs,
        partial_footer: sec.partial_footer.clone(),
        resume_command: sec.resume_command.clone(),
        commit_wait_line: if commit_wait_ms > 0 {
            format!("write lock : waited {commit_wait_ms} ms for another commit of this task")
        } else {
            String::new()
        },
        verdict_line: verdict_line.unwrap_or_default(),
        findings_line: findings_line.unwrap_or_default(),
        prior_line,
        unknown_ids_line,
        supersedes_line,
        structured_invalid,
        reply_path: ctx.reply_path.to_string_lossy().to_string(),
        reply_json_path: if reply_json_rel.is_empty() {
            String::new()
        } else {
            ctx.reply_json_path.to_string_lossy().to_string()
        },
        events_path: ctx.events_path.to_string_lossy().to_string(),
        reply_body,
        section,
        engine_warnings: {
            // (wave 26c D2 / 28b D13 / 28c D10) the machine-health warnings of warnings[] (the
            // update at the commit, the journal's unreadable lines) print as `warning    :` summary
            // lines too, after any engine-turn warnings.
            let mut w = sec.engine_warnings.clone();
            for hw in &health_summary_warnings {
                if !w.contains(hw) {
                    w.push(hw.clone());
                }
            }
            w
        },
        health_lines,
        pending_note,
        denial_retry_line: sec.denial_console.clone(),
        ..Default::default()
    };
    // Run warnings print before the summary block (`foreach ($rw in $runWarnings)`).
    for w in &ctx.run_warnings {
        println!("WARNING: {w}");
    }
    let rendered = summary::render_summary(&s);
    for line in &rendered {
        println!("{line}");
    }
    // A detached single run keeps its summary block and its one member's final state in the
    // status file (a panel member's own run never has the sink active — the panel scheduler
    // reports its members instead).
    if super::detach::is_active() && ctx.panel_member.is_none() {
        super::detach::note_single_run_summary(&rendered);
        let handoff = format!("{:02}", ctx.nn);
        let n = ctx.consult_n;
        let (state, outcome) = if usable {
            ("usable".to_string(), "usable reply".to_string())
        } else {
            // `bridge_outcome` already carries the `failed: ...` phrasing.
            ("failed".to_string(), bridge_outcome.clone())
        };
        let wall = round1(wall_seconds);
        let reply_rel_final = ctx.hf("md");
        super::detach::update_member(1, |m| {
            m.state = state;
            m.outcome = outcome;
            m.n = Some(n);
            m.handoff = handoff;
            m.wall_seconds = Some(wall);
            m.reply = reply_rel_final;
        });
    }
    // temp file cleanup (F04-11: keep the raw last message when a failed `.reply.json` copy names
    // it as the kept original).
    if !keep_last_msg {
        let _ = std::fs::remove_file(&ctx.last_msg_path);
    }
    let _ = std::fs::remove_file(&ctx.stderr_path);

    if usable {
        0
    } else {
        classify_exit(&bridge_outcome, provider_failure.as_ref())
    }
}

fn round1(s: f64) -> f64 {
    (s * 10.0).round() / 10.0
}

/// Give a recorded provider failure the plugin's full shape (`New-ProviderFailure` always
/// writes `kind` and `hint`, and stamps `when`).
fn finalize_pf(mut pf: c3_core::ledger::ProviderFailure) -> c3_core::ledger::ProviderFailure {
    if pf.kind.is_none() {
        pf.kind = Some(String::new());
    }
    if pf.hint.is_none() {
        pf.hint = Some(String::new());
    }
    if pf.when.is_empty() {
        pf.when = iso_now();
    }
    pf
}

/// Build a codex run's `provider_failure` from its evidence (`codex-consult.ps1:4093`): an SSE
/// `data:{...}` line on stderr, the event-stream error, the stderr tail, then the bridge's own
/// reason — the first non-empty through the one classifier.
fn codex_failure_pf(
    bridge_outcome: &str,
    event_error: &str,
    stderr_text: &str,
) -> c3_core::ledger::ProviderFailure {
    let sse_last = stderr_text
        .split(['\r', '\n'])
        .map(|l| l.trim())
        .rfind(|l| l.starts_with("data:") && l.contains('{'))
        .unwrap_or("");
    let stderr_tail = stderr_text
        .split(['\r', '\n'])
        .map(|l| l.trim())
        .rfind(|l| !l.is_empty())
        .unwrap_or("");
    let reason = bridge_outcome
        .strip_prefix("failed: ")
        .unwrap_or(bridge_outcome);
    let source = [sse_last, event_error, stderr_tail, reason]
        .into_iter()
        .find(|t| !t.is_empty())
        .unwrap_or("");
    let (code, message) = c3_core::health::convert_from_provider_error_text(source);
    let class = c3_core::health::provider_failure_class(&format!("{code} {message}"));
    let kind = c3_core::health::failure_kind(&class, &format!("{code} {message}"));
    // Parse a reset hint out of the failure message (`try again at <date>`, `try again at 9:43
    // PM.`, `retry in 32s`), so a later run on the endpoint honours it — the plugin records
    // `retry_after` on a codex failure too (`New-ProviderFailure`: this machine's zone, at the
    // moment of parsing).
    let retry_after = c3_core::health::retry_after_in(&message, parse_reference(), &chrono::Local)
        .map(c3_core::health::format_offset_iso);
    finalize_pf(c3_core::ledger::ProviderFailure {
        class,
        kind: Some(kind),
        code,
        message,
        retry_after,
        ..Default::default()
    })
}

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
#[allow(clippy::too_many_arguments)]
fn run_codex_secondary(
    ctx: &Context,
    kind: TurnKind,
    sandbox: &str,
    effort: Option<String>,
    schema_arg: Option<PathBuf>,
    thread: &str,
    prompt_text: &str,
    last_path: &Path,
    events_path: &Path,
    stderr_path: &Path,
    timeout_sec: f64,
    on_running: Option<std::sync::Arc<dyn Fn(u32, String) + Send + Sync>>,
) -> (AttemptOutcome, f64, String) {
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
    };
    // The resume must carry the request's own lineage (the core refuses a cross-lineage resume).
    let lineage = request.lineage();
    request.mode = Mode::Resume {
        thread: thread.to_string(),
        lineage,
    };
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
    let (outcome, engine_outcome) = match ctx.engine.as_str() {
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
            };
            (eng.run(&turn).unwrap_or_else(plan_err), String::new())
        }
    };
    (
        outcome,
        round1(start.elapsed().as_secs_f64()),
        engine_outcome,
    )
}

/// The timeout continuation (`codex-consult.ps1:3626`): after a killed main turn, ONE
/// `resume <thread>` turn — the main turn's options — asks the reviewer to finish now. Gated by
/// the thread, survivors, files-changed and the killed turn's own failure evidence.
#[allow(clippy::too_many_arguments)]
fn run_timeout_continuation(
    ctx: &Context,
    drift: &Drift,
    survivors: &[u32],
    // (wave 27c, D16) `Some((pid, why))` when the main turn's tree kill could not be confirmed:
    // no continuation is attempted (the orphan may still hold the thread).
    kill_unconfirmed: Option<&(u32, String)>,
    main_stderr: &str,
    main_event_error: &str,
    main_wall: f64,
    sec: &mut Secondary,
    bridge_outcome: &mut String,
    raw_text: &mut String,
    thread: &mut String,
    thread_source: &mut String,
    usable: &mut bool,
    provider_failure: &mut Option<c3_core::ledger::ProviderFailure>,
    // (wave 26b, D12) `Some(n)` when the main turn was STOPPED by the stall cut (n s without an
    // event) rather than the wall-clock timeout — the console line says so.
    stalled_secs: Option<i64>,
    // (wave 27c, D2) rewrites the pending record to `running` with the CONTINUATION's child pid the
    // moment it spawns, so a concurrent `--kick` (and the liveness check) find a live child of this
    // run — the main turn's child was killed at the timeout.
    on_running: Option<std::sync::Arc<dyn Fn(u32, String) + Send + Sync>>,
) {
    let continue_thread = thread.clone();
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
    if skip.is_empty() {
        if let Some((class, text)) =
            super::secondary::get_killed_turn_failure(main_event_error, main_stderr)
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

    let (outcome, wall, engine_outcome) = run_codex_secondary(
        ctx,
        TurnKind::TimeoutContinuation,
        &sandbox_label(&ctx.o),
        ctx.effort.sent.clone(),
        schema_arg,
        &continue_thread,
        &continue_prompt,
        &continue_last,
        &continue_events_path,
        &continue_stderr,
        ctx.r.continue_sec as f64,
        on_running,
    );
    let is_engine = !ctx.is_codex();
    sec.continue_wall = wall;
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
                let (ok, reason) = super::secondary::test_continuation_reply(&cont_raw, ctx.r.raw);
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
#[allow(clippy::too_many_arguments)]
fn run_format_repair(
    ctx: &Context,
    store: &FilesStore,
    pending: &PendingRef,
    base_record: &PendingRecord,
    thread: &str,
    thread_source: &str,
    usable: bool,
    sec: &mut Secondary,
    structured: &mut Option<StructuredReply>,
    raw_text: &mut String,
    validation_error: &mut String,
) {
    let eligible = ctx.r.repair_enabled
        && structured.is_none()
        && usable
        && !ctx.r.raw
        && !thread.is_empty()
        && thread_source == "events";
    if !eligible {
        return;
    }
    let gate = super::secondary::prose_gate(raw_text);
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

    let (outcome, wall, engine_outcome) = run_codex_secondary(
        ctx,
        TurnKind::FormatRepair,
        "read-only",
        repair_effort(ctx),
        repair_schema,
        thread,
        &repair_prompt,
        &repair_last,
        &repair_events,
        &repair_stderr,
        repair_timeout as f64,
        Some(on_running),
    );
    // (wave 3c, F23-3) a kill of this turn writes its record on the one this turn has on disk (it
    // names the saved prose and the repair's pid)
    if let (Some(site), Ok(r)) = (sec.kill_site.as_mut(), rec_arc.lock()) {
        site.on = r.clone();
    }

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
        for d in super::secondary::get_format_repair_drift(&sec.original_prose, s) {
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

// ------------------------------------------------------------------- engine (agy/muse) turns

/// How the prompt reached the engine, for the handoff `Argv: ... (<prompt_via>)` clause
/// (`EngineSpec.PromptVia`).
fn engine_prompt_via(engine: &str) -> &'static str {
    match engine {
        "agy" => "prompt on stdin as one NDJSON line",
        "muse" => "prompt from a file: --prompt-file",
        "http" => "reviewer pack sent as the request body",
        _ => "prompt on stdin",
    }
}

/// The per-engine `TreeNote` appended to the tree-check problem (`EngineSpec.TreeNote`).
fn engine_tree_note(engine: &str) -> String {
    match engine {
        "muse" => {
            "muse ran with --disable-write --disable-shell (the check cannot tell who changed it)"
                .to_string()
        }
        "http" => {
            "the http reviewer received only a pack and never touched the machine (the change is not the reviewer's)".to_string()
        }
        other => format!("{other}'s sandbox does not block writes"),
    }
}

/// `.collab/`-style prefix prepended to the collab-relative changed names in the tree problem.
fn collab_shown(ctx: &Context) -> String {
    let rel = c3_core::paths::repo_relative(&ctx.repo_root, &ctx.collab_root)
        .unwrap_or_else(|| ctx.o.collab_dir.clone());
    if rel.ends_with('/') {
        rel
    } else {
        format!("{rel}/")
    }
}

/// `<N> file(s): a, b, c, d, e, ...` (the plugin's `$cut`).
fn cut_files(names: &[String]) -> String {
    let n = names.len();
    let mut list = names.iter().take(5).cloned().collect::<Vec<_>>().join(", ");
    if n > 5 {
        list.push_str(", ...");
    }
    format!("{n} file{}: {list}", if n != 1 { "s" } else { "" })
}

/// Whether the engine ran write-disabled (muse `--disable-write --disable-shell`): its own tools
/// cannot have made a change, so the tree check WARNS instead of failing (D9).
fn engine_write_disabled(engine: &str) -> bool {
    // muse runs with --disable-write --disable-shell; the http reviewer receives only a pack and
    // never touches the machine at all — so any tree change during an http run is not the
    // reviewer's, and the check WARNS instead of failing (D9).
    engine == "muse" || engine == "http"
}

/// The structured read-only tree check (`Get-EngineTreeCheck`, wave 26b D9). Compares the working
/// tree, the collab directory (this run's own handoff prefix ignored), the brief and the
/// artifacts. A write-disabled engine (muse) WARNS (`outcome = "warned"`, the reply stays usable);
/// agy FAILS (`outcome = "failed"`, `problem` set, forced class `permission`). `outcome = "clean"`
/// when nothing changed.
struct EngineTreeCheck {
    outcome: String,
    problem: String,
    warnings: Vec<String>,
    files: Vec<String>,
}

fn engine_tree_check(
    ctx: &Context,
    drift: &Drift,
    collab_before: &std::collections::HashMap<String, String>,
    collab_after: &std::collections::HashMap<String, String>,
) -> EngineTreeCheck {
    let mut why: Vec<String> = Vec::new();
    let mut warn: Vec<String> = Vec::new();
    let mut files: Vec<String> = Vec::new();
    let not_mine = format!(
        "{} ran write-disabled, the change is not the reviewer's",
        ctx.engine
    );
    if drift.tree_changed {
        files.extend(drift.tree_changed_paths.iter().cloned());
        let cut = cut_files(&drift.tree_changed_paths);
        why.push(format!(
            "the working tree changed during the run (by the reviewer or anyone else): {cut}"
        ));
        warn.push(format!(
            "the working tree changed during the run ({cut}) - {not_mine}"
        ));
    }
    let own = format!(
        "{}/handoffs/{:02}-{}-{}.",
        ctx.task, ctx.nn, ctx.file_prefix, ctx.reply_name
    );
    // A member of a panel whose members run at the same time (D7): the task's two stores and the
    // siblings' handoffs are theirs to write meanwhile (each with its atomic-write temp); every
    // other collab path stays monitored.
    let mut own_prefixes = vec![own];
    if let Some(m) = &ctx.panel_member {
        own_prefixes.extend(crate::engines::tree_check::panel_ignore_prefixes(
            ctx.task.as_str(),
            &m.sibling_nns,
        ));
    }
    let collab_changed =
        crate::engines::tree_check::compare_collab(collab_before, collab_after, &own_prefixes);
    if !collab_changed.is_empty() {
        let shown = collab_shown(ctx);
        let names: Vec<String> = collab_changed
            .iter()
            .map(|n| format!("{shown}{n}"))
            .collect();
        files.extend(names.iter().cloned());
        let cut = cut_files(&names);
        why.push(format!(
            "the collab directory changed during the run (by the reviewer or anyone else): {cut}"
        ));
        warn.push(format!(
            "the collab directory changed during the run ({cut}) - {not_mine}"
        ));
    }
    if drift.brief_changed {
        files.push("brief".to_string());
        why.push("the brief changed during the run (by the reviewer or anyone else)".to_string());
        warn.push(format!("the brief changed during the run - {not_mine}"));
    }
    if !drift.artifacts_changed_paths.is_empty() {
        files.extend(drift.artifacts_changed_paths.iter().cloned());
        let list = drift.artifacts_changed_paths.join(", ");
        why.push(format!(
            "artifact(s) changed during the run (by the reviewer or anyone else): {list}"
        ));
        warn.push(format!(
            "artifact(s) changed during the run ({list}) - {not_mine}"
        ));
    }
    if why.is_empty() {
        return EngineTreeCheck {
            outcome: "clean".into(),
            problem: String::new(),
            warnings: Vec::new(),
            files,
        };
    }
    if engine_write_disabled(&ctx.engine) {
        EngineTreeCheck {
            outcome: "warned".into(),
            problem: String::new(),
            warnings: warn,
            files,
        }
    } else {
        EngineTreeCheck {
            outcome: "failed".into(),
            problem: format!("{} - {}", why.join("; "), engine_tree_note(&ctx.engine)),
            warnings: Vec::new(),
            files,
        }
    }
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
        _ => {
            let eng = crate::engines::muse::MuseEngine {
                launcher: ctx.engine_launcher.clone(),
                cwd: ctx.repo_root.clone(),
                primary: TurnFiles::default(),
                secondary: files,
                stall_sec: 0,
                kick_path: None,
                on_running: None,
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
#[allow(clippy::too_many_arguments)]
fn run_engine_denial_retry(
    ctx: &Context,
    detail: &EngineDetail,
    tree_problem: &str,
    sec: &mut Secondary,
    bridge_outcome: &mut String,
    raw_text: &mut String,
    thread: &mut String,
    thread_source: &mut String,
    usable: &mut bool,
    forced_class: &mut String,
) {
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

/// (0.6.1) The moment a provider failure's reset time is parsed at - the reference of a duration
/// and of a reset time without a date ("try again at 9:43 PM.": today, or tomorrow once past):
/// the system clock; TEST HOOK: `CODEX_CONSULT_NOW`, the consult clock (`Get-ConsultClock -Peek`
/// in `New-ProviderFailure`). The failure's `when` stays the system's.
fn parse_reference() -> chrono::DateTime<chrono::FixedOffset> {
    crate::providers::get_consult_clock_peek()
        .unwrap_or_else(|_| chrono::Local::now().fixed_offset())
}

/// An engine run's `provider_failure`: the forced class (D12 tree check / a turn's own class)
/// outranks the classified evidence.
fn finalize_engine_pf(
    mut pf: c3_core::ledger::ProviderFailure,
    forced_class: &str,
) -> c3_core::ledger::ProviderFailure {
    // The tree check's forced class (D12, `permission`) outranks the adapter's class.
    if forced_class == "permission" {
        pf.class = "permission".to_string();
    }
    pf.kind = Some(c3_core::health::failure_kind(
        &pf.class,
        &format!("{} {}", pf.code, pf.message),
    ));
    // Parse a reset hint out of the message (`Please retry in 32s`, `Try again in 2 hours`).
    if pf.retry_after.is_none() {
        pf.retry_after =
            c3_core::health::retry_after_in(&pf.message, parse_reference(), &chrono::Local)
                .map(c3_core::health::format_offset_iso);
    }
    finalize_pf(pf)
}

/// Build the salvaged `.partial.md` body, footer and resume command when a killed turn had no
/// usable continuation (`codex-consult.ps1:4108`).
/// The `entry.panel` object a panel member records (`codex-consult.ps1:3216`): the panel id, this
/// seat, the panel-wide member list (with each seat's state/reason — the context skip included),
/// the plan and the routing record. `started`/`usable` are the panel run's to patch after every
/// member finishes. `None` for a single run. Shared by the ledger entry and the dry-run preview.
pub(crate) fn build_member_panel(ctx: &Context) -> Option<c3_core::ledger::Panel> {
    let m = ctx.panel_member.as_ref()?;
    let members = m
        .members
        .iter()
        .map(|b| c3_core::ledger::PanelMember {
            provider: b.provider.clone(),
            model: b.model.clone(),
            state: b.state.clone(),
            reason: b.reason.clone(),
            ..Default::default()
        })
        .collect();
    let routing = m
        .routing
        .as_ref()
        .and_then(|v| serde_json::from_value::<c3_core::ledger::PanelRouting>(v.clone()).ok());
    Some(c3_core::ledger::Panel {
        id: m.id.clone(),
        position: m.position,
        of: m.of,
        members,
        concurrency: m.concurrency,
        limits: m.limits.clone(),
        asked: Some(if m.asked > 0 { m.asked } else { m.of }),
        started: Some(None),
        usable: Some(None),
        routing,
        roles_note: Some(Some(ctx.roles_note.clone())),
        ..Default::default()
    })
}

/// (wave 26b, D16) The new prompt's estimated token size: the ask's character count plus the
/// brief file's byte length, at 4 characters a token (ceiling), matching the plugin's
/// `$promptEstimate`.
fn estimate_prompt_tokens(prompt: &str, brief_path: Option<&Path>) -> i64 {
    let mut chars = prompt.chars().count() as i64;
    if let Some(p) = brief_path {
        if let Ok(m) = std::fs::metadata(p) {
            chars += m.len() as i64;
        }
    }
    ((chars as f64) / 4.0).ceil() as i64
}

#[allow(clippy::too_many_arguments)]
fn build_partial_reply(
    ctx: &Context,
    main_events_text: &str,
    sec: &mut Secondary,
    main_timed_out: bool,
    main_wall: f64,
    // (wave 26b) extra context: a stall kill, an operator kick, whether the run is usable, the
    // failure outcome text (for D15's "the run ended: <why>") and the run's own thread.
    stall: bool,
    run_kicked: bool,
    usable: bool,
    bridge_outcome: &str,
    main_thread: &str,
) {
    let killed_partial = (main_timed_out && !sec.continued)
        || sec.continue_killed
        || sec.repair_killed
        || run_kicked;
    // (wave 26b, D15) ANY failed run keeps what its reviewer produced: when the run is not usable
    // and its main event stream holds at least one agent message, reasoning text or tool call, the
    // salvage is written even though no turn was killed (codex only; engines keep their own path).
    let mut partial_on_failure = false;
    if !killed_partial && !usable && ctx.is_codex() {
        let s = super::secondary::read_codex_salvage(main_events_text);
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

    let mut turns: Vec<super::secondary::PartialTurn> = Vec::new();
    let mut killed_at: Vec<String> = Vec::new();
    turns.push(super::secondary::PartialTurn {
        label: "Turn 1 - the main turn".to_string(),
        note: if run_kicked {
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
        salvage: super::secondary::read_codex_salvage(main_events_text),
    });
    if run_kicked {
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
        turns.push(super::secondary::PartialTurn {
            label: format!("Turn {} - the timeout continuation", turns.len() + 1),
            note,
            salvage: super::secondary::read_codex_salvage(&cont_text),
        });
        if sec.continue_killed {
            killed_at.push(format!(
                "{} s of {} s (the timeout continuation)",
                fmt_wall(sec.continue_wall),
                ctx.r.continue_sec
            ));
        }
    }

    let mut body = super::secondary::format_partial_body(&turns);
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
    let resume_thread = if !sec.continue_thread.is_empty() {
        sec.continue_thread.clone()
    } else if partial_on_failure && !main_thread.is_empty() {
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
        });
        sec.partial_footer =
            format!("{ended_text}; thread {resume_thread} - continue with `{args}`");
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

fn classify_exit(outcome: &str, pf: Option<&c3_core::ledger::ProviderFailure>) -> i32 {
    // The plugin exits 1 for every non-usable outcome (`codex-consult.ps1` summary block);
    // the class-`transport` signal codes 130/143 (a codex process killed by SIGINT/SIGTERM)
    // are an M2c+ refinement, not the timeout-kill path (which is a plain failed run = 1).
    let _ = (outcome, pf);
    1
}

fn build_finding(
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

/// A well-formed 36-char uuid string (`^[0-9a-fA-F-]{36}$`), the member's `consult_id` gate.
fn is_uuid36(s: &str) -> bool {
    s.len() == 36 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// Parse a panel member spec's `skipped` value into the roster-skip tuples the ledger record
/// uses: each `{provider, model, engine?, reason}` in order.
fn member_skips(skipped: &serde_json::Value) -> Vec<(String, String, String, String)> {
    let arr = match skipped.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };
    arr.iter()
        .map(|v| {
            let g = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
            (g("provider"), g("model"), g("engine"), g("reason"))
        })
        .collect()
}

fn iso_now() -> String {
    chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string()
}

/// `Get-TestHookMs`: parse a test-hook value that is either a plain `<ms>` or a
/// `<model>=<ms>[|<model>=<ms>...]` map; return the ms for `model` (or the plain value), else
/// `None`.
fn test_hook_ms(value: &str, model: &str) -> Option<u64> {
    let v = value.trim();
    if v.is_empty() {
        return None;
    }
    if !v.contains('=') {
        return v.parse::<u64>().ok();
    }
    for part in v.split('|') {
        if let Some((m, ms)) = part.split_once('=') {
            if m.trim() == model {
                return ms.trim().parse::<u64>().ok();
            }
        }
    }
    None
}

pub(crate) fn fmt_wall(w: f64) -> String {
    if (w.fract()).abs() < f64::EPSILON {
        format!("{}", w as i64)
    } else {
        format!("{w}")
    }
}

#[allow(clippy::too_many_arguments)]
fn render_handoff(
    ctx: &Context,
    bridge_outcome: &str,
    wall: f64,
    usage: &Option<Usage>,
    thread: &str,
    thread_source: &str,
    thread_candidate: &str,
    structured: Option<&StructuredReply>,
    finding_ids: &[String],
    validation_error: &str,
    provider_failure: Option<&c3_core::ledger::ProviderFailure>,
    raw_text: &str,
    drift: &Drift,
    sec: &Secondary,
    main_event_error: &str,
    main_stderr: &str,
    prior_blockers: &[render::PriorBlockerLine],
    verdict_warning: Option<&str>,
) -> (String, Option<String>) {
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
        if ctx.o.max_model_steps > 0 {
            l.push_str(&format!("; --max-model-steps {}", ctx.o.max_model_steps));
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
        let reason = render::close_sentence(&s.verdict_reason);
        let mut vt = format!("Verdict: {v}");
        if reason.is_empty() {
            vt.push('.');
        } else {
            vt.push_str(&format!(" - {reason}"));
        }
        let counts = render::severity_counts(s);
        let ft = if s.findings.is_empty() {
            "Findings: none.".to_string()
        } else {
            format!(
                "Findings: {} ({}, tracked in `findings.json`).",
                render::format_severity_counts(&counts),
                render::format_id_range(finding_ids)
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
        out.push_str(&render::format_structured_section(
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

fn model_label(id: &ReviewerIdentity) -> String {
    if id.model_source == "unknown" {
        "unknown".to_string()
    } else {
        id.model.clone()
    }
}

fn sandbox_label(o: &Options) -> String {
    if o.sandbox.is_empty() {
        "read-only".to_string()
    } else {
        o.sandbox.clone()
    }
}

/// The first 12 hex chars of a hash (`Format-ShortHash`).
fn short_hash(h: &str) -> String {
    h.chars().take(12).collect()
}

/// `Reviewer: <lineage> (provider from <src>, model from <src>; endpoint <host>; provider
/// fingerprint <short>[; <note>]; harness <harness>).` (`$reviewerLine`).
/// The reviewer identity's `endpoint <base_url>, wire_api: <x>` display (`$identity.Display`):
/// the full canonical base_url with the query redacted (`(default)` when the endpoint is
/// Codex's own default), and the wire_api label (`(default)` when the table declares none).
pub(crate) fn identity_display(id: &ReviewerIdentity) -> String {
    // The built-in openai endpoint honouring OPENAI_BASE_URL (no user table): the plugin's
    // Display is `endpoint builtin:openai via OPENAI_BASE_URL <audit>` (no wire_api clause).
    if !id.base_url.trim().is_empty() && id.provider_config.get("builtin").is_some() {
        return format!(
            "endpoint builtin:openai via OPENAI_BASE_URL {}",
            c3_core::config::audit_base_url(&id.base_url)
        );
    }
    let audit = if id.base_url.trim().is_empty() {
        "(default)".to_string()
    } else {
        crate::providers::strip_query(&id.base_url)
    };
    let wire_label = match id.wire_api.trim() {
        "" | "(built in)" => "(default)",
        other => other,
    };
    format!("endpoint {audit}, wire_api: {wire_label}")
}

pub(crate) fn reviewer_line(id: &ReviewerIdentity, engine: &str, harness: &str) -> String {
    // The reviewer line shows the engine-tagged lineage (`... [agy]`); a codex run keeps the
    // identity's own lineage byte-for-byte (an unresolved codex identity has a special form).
    let lineage = if engine.is_empty() || engine == "codex" {
        id.lineage.clone()
    } else {
        c3_core::lineage::format_reviewer_lineage(&id.provider, &id.model, engine)
    };
    let mut line = format!(
        "Reviewer: {} (provider from {}, model from {}; {}",
        lineage,
        if id.provider_source.is_empty() {
            "codex default"
        } else {
            &id.provider_source
        },
        if id.model_source.is_empty() {
            "config"
        } else {
            &id.model_source
        },
        identity_display(id)
    );
    if id.resolved {
        line.push_str(&format!(
            "; provider fingerprint {}",
            short_hash(&id.fingerprint)
        ));
        if !id.note.is_empty() {
            line.push_str(&format!("; {}", id.note));
        }
        line.push_str(&format!("; harness {harness}).",));
    } else {
        line.push_str(&format!(
            "; identity UNRESOLVED - never a parent thread: {}; harness {harness}).",
            id.note
        ));
    }
    line
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

#[allow(clippy::too_many_arguments)]
fn build_entry(
    ctx: &Context,
    bridge_outcome: &str,
    wall: f64,
    usage: &Option<Usage>,
    thread: &str,
    thread_source: &str,
    structured: Option<&StructuredReply>,
    finding_ids: &[String],
    counts: &FindingCounts,
    validation_error: &str,
    provider_failure: Option<c3_core::ledger::ProviderFailure>,
    handoff_rel: &str,
    reply_json_rel: &str,
    events_rel: &str,
    drift: &Drift,
    sec: &Secondary,
    stall: Option<c3_core::ledger::Stall>,
) -> LedgerEntry {
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
                ..Default::default()
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

#[cfg(test)]
mod continuation_kick_tests {
    use super::*;

    #[test]
    fn continuation_registers_its_child_in_the_pending_record() {
        // (D2) a `running` record naming the OLD (main-turn) child; the continuation's on_running
        // rewrites it to name the CONTINUATION's live child, so a concurrent `--kick` finds it.
        let dir = std::env::temp_dir().join(format!(
            "c3-contkick-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        let task = TaskSlug::new("t").unwrap();
        std::fs::create_dir_all(dir.join("t")).unwrap();
        let store = FilesStore::new(dir.clone());
        let pending = PendingRef::single(task.clone());
        let base = PendingRecord {
            state: PendingState::Running,
            n: 1,
            nn: "01".into(),
            child_pid: Some(999_999), // the main turn's child, killed at the timeout
            child_start_time: "old".into(),
            ..Default::default()
        };
        let cb = continuation_on_running(&store, &pending, &base);
        let me = std::process::id();
        let start = crate::liveness::proc::process_start_iso(me).unwrap_or_default();
        cb(me, start.clone());

        let rd = crate::liveness::pending::read_pending_file(
            &store.task_dir(&task).join(".consult.pending.json"),
        );
        let rec = rd.record.expect("the record was rewritten");
        assert_eq!(
            rec.get("child_pid").and_then(|v| v.as_u64()),
            Some(me as u64),
            "the record names the continuation's child, not the first turn's"
        );
        assert_eq!(rec.get("state").and_then(|v| v.as_str()), Some("running"));
        // A concurrent `--kick` liveness check would now find a live child of this run.
        assert!(crate::liveness::proc::pid_alive(me, &start));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn continuation_kick_keeps_the_timeout_outcome_and_names_the_turn() {
        // (D2) the operator kicking the continuation records the operator stop on the continuation
        // turn, keeps the run's timeout outcome, and warns naming the cancelled turn.
        assert_eq!(
            CONTINUATION_KICK_WARNING,
            "kick: the operator stopped the timeout continuation (-Kick); the timeout outcome and its salvage stay"
        );
        // The `sec.continue_kicked` branch records the operator stop as the continuation's outcome
        // without touching the run's bridge_outcome (verified end-to-end by fixes27c KICK D2).
        let mut sec = Secondary {
            continue_kicked: true,
            ..Default::default()
        };
        assert!(sec.continue_kicked);
        sec.timeout_continue = Some(c3_core::ledger::TimeoutContinue {
            outcome: "failed: stopped by the operator (-Kick)".into(),
            ..Default::default()
        });
        assert!(sec
            .timeout_continue
            .as_ref()
            .unwrap()
            .outcome
            .contains("stopped by the operator (-Kick)"));
    }
}

#[cfg(test)]
mod telemetry_tests {
    use super::*;
    use c3_core::ledger::LedgerEntry;
    use std::sync::Mutex;

    // Serialises the env-mutating part of this file's tests against itself and the panel-member
    // tests below (env is process-global).
    pub(super) static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn spool_pending(home: &Path) -> usize {
        let p = home.join("c3").join("telemetry").join("spool.ndjson");
        std::fs::read_to_string(p)
            .map(|t| t.lines().filter(|l| !l.trim().is_empty()).count())
            .unwrap_or(0)
    }

    #[test]
    fn telemetry_gates_govern_enqueue() {
        let _g = ENV_LOCK.lock().unwrap();

        // The consult flow enqueues only inside `finish()`, gated by `ctx.telemetry_enabled`
        // (= `is_enabled`), and `finish()` runs only for a real (non-dry) run. `run()` gates
        // the background flush by `is_enabled(cfg) && !dry_run`.
        let on = telemetry::Config {
            telemetry: Some(true),
        };
        // A dry run never flushes and never reaches the enqueue site.
        let dry_run = true;
        assert!(!(telemetry::is_enabled(&on) && !dry_run));
        // `--telemetry off` disables the enqueue gate regardless of the environment.
        assert!(!telemetry::is_enabled(&telemetry::Config {
            telemetry: Some(false),
        }));

        // env `CODEX_CONSULT_TELEMETRY=off`: `is_enabled` false AND `record_consultation`
        // enqueues nothing (observed on an isolated spool).
        let home = std::env::temp_dir().join(format!("c3-tele-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("CODEX_HOME", &home);
        std::env::set_var("CODEX_CONSULT_TELEMETRY", "off");
        let off_cfg = telemetry::Config::default();
        assert!(!telemetry::is_enabled(&off_cfg));
        let entry = LedgerEntry {
            n: 7,
            ..Default::default()
        };
        let _ = telemetry::record_consultation(&entry, None, &off_cfg);
        assert_eq!(spool_pending(&home), 0, "env off must enqueue nothing");
        // (0.6.1 parity, Get-TelemetrySwitch) any value that is not on counts as off.
        std::env::set_var("CODEX_CONSULT_TELEMETRY", "maybe");
        assert!(!telemetry::is_enabled(&off_cfg));
        // A run's own `--telemetry on` wins over the environment (the plugin's -Telemetry on).
        std::env::set_var("CODEX_CONSULT_TELEMETRY", "off");
        let on_cfg = telemetry::Config {
            telemetry: Some(true),
        };
        assert!(telemetry::is_enabled(&on_cfg));
        assert_eq!(telemetry::switch(Some(true)).source, "-Telemetry");

        // Control: with the switch on, the same record DOES enqueue one event (proves the
        // guard suppresses, rather than the path being a no-op).
        std::env::remove_var("CODEX_CONSULT_TELEMETRY");
        assert!(telemetry::is_enabled(&telemetry::Config::default()));
        let _ = telemetry::record_consultation(&entry, None, &telemetry::Config::default());
        assert_eq!(spool_pending(&home), 1, "switch on enqueues one event");

        std::env::remove_var("CODEX_HOME");
        let _ = std::fs::remove_dir_all(&home);
    }
}

#[cfg(test)]
mod member_tests {
    use super::*;
    use crate::panel::member::{MemberBrief, MemberSpec};
    use serde_json::json;

    fn scratch() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "c3-member-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(d.join("t")).unwrap();
        d
    }

    fn sample_spec(collab: &str, listed: Vec<String>, role: &str) -> MemberSpec {
        MemberSpec {
            id: "043d5bfe-1111-2222-3333-444455556666".into(),
            position: 2,
            of: 3,
            members: vec![MemberBrief {
                provider: "openai".into(),
                model: "gpt-6-astra".into(),
                state: "run".into(),
                reason: String::new(),
            }],
            roster_position: 1,
            provider: "openai".into(),
            model: "gpt-6-astra".into(),
            engine: String::new(),
            skipped: json!([]),
            listed_ids: listed,
            n: 12,
            nn: 7,
            consult_id: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".into(),
            parent_pid: 4321,
            parent_start_time: "2026-09-27T10:11:12.3456789Z".into(),
            sibling_nns: vec![6, 8],
            concurrency: 3,
            limits: json!({"openai": 1}),
            asked: 3,
            routing: Some(json!({"mode": "roster", "size": 3})),
            role: role.into(),
            roles_note: String::new(),
            panel_warnings: vec!["a framing panel seated below 2".into()],
            args: json!({
                "collab_dir": collab,
                "purpose": "framing",
                "prompt": "look at this",
                "reply_name": "reply-openai",
                "timeout_sec": 0,
                "continue_sec": -1,
                "format_retry": 1,
                "skip_preflight": true,
                "raw": false,
                "dry_run": false
            }),
        }
    }

    #[test]
    fn build_context_honours_the_member_spec() {
        let _g = super::telemetry_tests::ENV_LOCK.lock().unwrap();
        let root = scratch();
        let roster_path = root.join("roster.json");
        std::fs::write(
            &roster_path,
            r#"{"roster_version":1,"reviewers":[{"provider":"openai","model":"gpt-6-astra"}]}"#,
        )
        .unwrap();
        // Two open findings; the spec lists only the first.
        std::fs::write(
            root.join("t").join("findings.json"),
            r#"{"task_id":"t","findings":[{"id":"F01-1","status":"proposed","claim":"a"},{"id":"F02-1","status":"proposed","claim":"b"}]}"#,
        )
        .unwrap();
        // A repository role the member's role paragraph resolves.
        std::fs::create_dir_all(root.join(".collab").join("roles")).ok();
        std::fs::create_dir_all(root.join("t").join("roles")).ok();
        // The collab dir is `<root>` and the task `t`, so the roles dir is `<root>/roles`.
        std::fs::create_dir_all(root.join("roles")).unwrap();
        std::fs::write(root.join("roles").join("adversary.md"), "Attack it.").unwrap();

        std::env::set_var("CODEX_CONSULT_ROSTER", &roster_path);
        std::env::set_var("CODEX_CONSULT_TELEMETRY", "off");

        let spec = sample_spec(root.to_str().unwrap(), vec!["F01-1".into()], "adversary");
        let mo = member_options("t", &spec);
        let r = args::validate(&mo, None).expect("member options validate");
        let ctx = build_context(mo, r, Some(&spec)).expect("member build_context");

        // Numbers, consult id and reply file name come from the spec.
        assert_eq!(ctx.nn, 7);
        assert_eq!(ctx.consult_n, 12);
        assert_eq!(ctx.consult_id, "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee");
        assert!(
            ctx.reply_path
                .to_string_lossy()
                .ends_with("07-codex-reply-openai.md"),
            "{}",
            ctx.reply_path.display()
        );
        // The open-findings snapshot is intersected with `listed_ids`.
        assert_eq!(ctx.open_findings_count, 1);
        // The role paragraph is in the prompt, after the ask and before nothing else brief-side.
        assert!(ctx
            .prompt_text
            .contains("Your role in this review: adversary."));
        assert!(ctx.prompt_text.contains("Attack it."));
        // The panel warning is carried into the run warnings and the ledger.
        assert!(ctx
            .run_warnings
            .iter()
            .any(|w| w == "a framing panel seated below 2"));
        // The pending record is the member's, not the single-run file.
        assert_eq!(ctx.role, "adversary");
        assert!(ctx.panel_member.is_some());

        std::env::remove_var("CODEX_CONSULT_ROSTER");
        std::env::remove_var("CODEX_CONSULT_TELEMETRY");
        let _ = std::fs::remove_dir_all(root);
    }

    // (muse PANEL) a panel passes -MaxModelSteps to every member; a codex member (no step cap)
    // drops it instead of refusing (`if ($panelMember) { $MaxModelSteps = 0 }`), a single codex run
    // still refuses it.
    #[test]
    fn a_codex_member_drops_the_step_cap() {
        let _g = super::telemetry_tests::ENV_LOCK.lock().unwrap();
        let root = scratch();
        let roster_path = root.join("roster.json");
        std::fs::write(
            &roster_path,
            r#"{"roster_version":1,"reviewers":[{"provider":"openai","model":"gpt-6-astra"}]}"#,
        )
        .unwrap();
        std::env::set_var("CODEX_CONSULT_ROSTER", &roster_path);
        std::env::set_var("CODEX_CONSULT_TELEMETRY", "off");
        let mut spec = sample_spec(root.to_str().unwrap(), vec![], "");
        spec.args["max_model_steps"] = json!(40);
        let mo = member_options("t", &spec);
        assert_eq!(mo.max_model_steps, 40);
        let r = args::validate(&mo, None).unwrap();
        let ctx = build_context(mo, r, Some(&spec)).expect("a codex member runs");
        assert_eq!(ctx.o.max_model_steps, 0);
        let mut single = member_options("t", &spec);
        single.collab_dir = root.to_str().unwrap().to_string();
        let r2 = args::validate(&single, None).unwrap();
        let err = match build_context(single, r2, None) {
            Ok(_) => panic!("a single codex run refuses -MaxModelSteps"),
            Err(e) => e,
        };
        assert!(
            err.0.starts_with("-MaxModelSteps is for the muse engine"),
            "{}",
            err.0
        );
        std::env::remove_var("CODEX_CONSULT_ROSTER");
        std::env::remove_var("CODEX_CONSULT_TELEMETRY");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn build_context_refuses_a_roster_that_changed_under_the_panel() {
        let _g = super::telemetry_tests::ENV_LOCK.lock().unwrap();
        let root = scratch();
        let roster_path = root.join("roster.json");
        // The roster entry at position 1 is a DIFFERENT reviewer than the spec names.
        std::fs::write(
            &roster_path,
            r#"{"roster_version":1,"reviewers":[{"provider":"zai","model":"glm-5.3"}]}"#,
        )
        .unwrap();
        std::env::set_var("CODEX_CONSULT_ROSTER", &roster_path);
        std::env::set_var("CODEX_CONSULT_TELEMETRY", "off");

        let spec = sample_spec(root.to_str().unwrap(), vec![], "");
        let mo = member_options("t", &spec);
        let r = args::validate(&mo, None).unwrap();
        let err = match build_context(mo, r, Some(&spec)) {
            Ok(_) => panic!("expected a roster-changed refusal"),
            Err(e) => e,
        };
        assert!(err.0.contains("changed while the panel ran"), "{}", err.0);

        std::env::remove_var("CODEX_CONSULT_ROSTER");
        std::env::remove_var("CODEX_CONSULT_TELEMETRY");
        let _ = std::fs::remove_dir_all(root);
    }
}

#[cfg(test)]
mod engine_tests {
    use super::*;

    #[test]
    fn cmd_argv_hazard_only_for_cmd_launcher_with_percent() {
        // A .exe launcher never triggers the hazard.
        assert_eq!(
            cmd_argv_hazard(
                "C:/x/muse.exe",
                &["--prompt-file".into(), "C:/T%MP/p.txt".into()]
            ),
            ""
        );
        // A .cmd launcher with a '%' argument is refused; the note names the count and launcher.
        let h = cmd_argv_hazard(
            "C:/x/fake-muse.cmd",
            &["exec".into(), "C:/T%MP/prompt.txt".into()],
        );
        assert!(
            h.contains("fake-muse.cmd is a cmd.exe script and 1 argument(s) contain '%'"),
            "{h}"
        );
        // A .cmd launcher with no '%' is safe.
        assert_eq!(
            cmd_argv_hazard("C:/x/f.cmd", &["exec".into(), "clean".into()]),
            ""
        );
    }

    #[test]
    fn engine_tree_note_per_engine() {
        assert_eq!(
            engine_tree_note("agy"),
            "agy's sandbox does not block writes"
        );
        assert_eq!(
            engine_tree_note("muse"),
            "muse ran with --disable-write --disable-shell (the check cannot tell who changed it)"
        );
    }

    #[test]
    fn engine_prompt_via_per_engine() {
        assert_eq!(
            engine_prompt_via("agy"),
            "prompt on stdin as one NDJSON line"
        );
        assert_eq!(
            engine_prompt_via("muse"),
            "prompt from a file: --prompt-file"
        );
        assert_eq!(engine_prompt_via("codex"), "prompt on stdin");
    }

    #[test]
    fn cut_files_shape() {
        assert_eq!(cut_files(&["a.txt".into()]), "1 file: a.txt");
        assert_eq!(cut_files(&["a".into(), "b".into()]), "2 files: a, b");
        let many: Vec<String> = (1..=7).map(|n| format!("f{n}")).collect();
        assert_eq!(cut_files(&many), "7 files: f1, f2, f3, f4, f5, ...");
    }

    #[test]
    fn engine_kind_mapping() {
        assert_eq!(engine_kind_of("agy"), EngineKind::Agy);
        assert_eq!(engine_kind_of("muse"), EngineKind::Muse);
        assert_eq!(engine_kind_of("codex"), EngineKind::Codex);
    }
}

#[cfg(test)]
mod context_budget_tests {
    use super::*;

    // (wave 26b, D16) the 80% rule's estimate: (ask chars + brief bytes) / 4, ceiling. A ~120 KB
    // brief of `'word ' * 24000` (120000 bytes) plus a one-char ask estimates 30001 tokens, which
    // is > 80% of a 32000-token window (25600) — the reviewer is skipped / the mode falls back.
    #[test]
    fn prompt_token_estimate_and_80pct_threshold() {
        let dir = std::env::temp_dir().join(format!("c3-est-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let brief = dir.join("big.md");
        std::fs::write(&brief, "word ".repeat(24000)).unwrap();
        let est = estimate_prompt_tokens("x", Some(brief.as_path()));
        assert_eq!(est, 30001);
        assert!((est as f64) > 0.8 * 32000.0);
        // The ask alone is tiny; ceiling of 1/4 = 1.
        assert_eq!(estimate_prompt_tokens("x", None), 1);
        // A small brief that fits stays under 80%.
        let small = dir.join("small.md");
        std::fs::write(&small, "word ".repeat(100)).unwrap();
        assert!((estimate_prompt_tokens("x", Some(small.as_path())) as f64) <= 0.8 * 32000.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // F09-1 / RC1: an operator item whose value is Unicode (`notify` is 6 bytes, the item 35, byte
    // 20 a continuation byte) must not be sliced at the length of the context keys - the helper
    // returns normally and adds BOTH generated defaults.
    #[test]
    fn context_window_config_survives_a_unicode_operator_item() {
        let extra = vec![r#"notify=["日本日本日本日本"]"#.to_string()];
        assert!(!extra[0].is_char_boundary("model_context_window".len()));
        let (items, record) = context_window_config(256000, true, &extra);
        assert_eq!(
            items,
            vec![
                "model_context_window=256000".to_string(),
                "model_auto_compact_token_limit=204800".to_string(),
            ]
        );
        let record = record.expect("a context window record");
        assert_eq!(record["tokens"], 256000);
        assert_eq!(record["auto_compact_limit"], 204800);
        assert_eq!(record["items"].as_array().map(|a| a.len()), Some(2));
        // A Unicode KEY-shaped prefix of the same length is no match either, and never panics.
        let odd = vec![
            "日本日本日本日=1".to_string(),
            "Model_Context_Window =9".to_string(),
        ];
        let (items, _) = context_window_config(1000, true, &odd);
        assert_eq!(
            items,
            vec!["model_auto_compact_token_limit=800".to_string()]
        );
    }
}

#[cfg(test)]
mod parent_walk_tests {
    use super::*;
    use c3_core::ledger::{LedgerEntry, Reviewer};

    fn id(provider: &str, model: &str, fp: &str) -> ReviewerIdentity {
        ReviewerIdentity {
            provider: provider.into(),
            provider_source: "-Provider".into(),
            model: model.into(),
            model_source: "-Model".into(),
            lineage: c3_core::lineage::format_reviewer_lineage(provider, model, "codex"),
            resolved: true,
            note: String::new(),
            error: String::new(),
            fingerprint: fp.into(),
            compat_string: String::new(),
            host: String::new(),
            base_url: String::new(),
            wire_api: String::new(),
            engine: "codex".into(),
            provider_config: serde_json::Value::Null,
        }
    }

    fn entry(n: i64, thread: &str, provider: &str, model: &str, fp: &str) -> LedgerEntry {
        LedgerEntry {
            n,
            thread: thread.into(),
            reviewer: Reviewer {
                provider: provider.into(),
                model: model.into(),
                engine: "codex".into(),
                provider_fingerprint: fp.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn unknown_thread_refused() {
        let e =
            select_parent_thread(&[], &id("openai", "gpt-5.1", "fp1"), "fork", "abc").unwrap_err();
        assert_eq!(
            e,
            "thread abc has unknown provenance: it is not in this task's ledger; use -Mode new"
        );
    }

    #[test]
    fn cross_lineage_thread_refused() {
        let entries = vec![entry(1, "t-1", "ZAI", "glm-5.3", "fpz")];
        let e = select_parent_thread(&entries, &id("openai", "gpt-5.1", "fp1"), "fork", "t-1")
            .unwrap_err();
        assert!(
            e.contains(
                "belongs to lineage ZAI :: glm-5.3 (consult n=1); this run is openai :: gpt-5.1"
            ),
            "{e}"
        );
        assert!(
            e.ends_with("use -Mode new, or run as ZAI :: glm-5.3"),
            "{e}"
        );
    }

    #[test]
    fn legacy_entry_refused_for_thread() {
        // A pre-0.3 entry (no reviewer fields at all) recorded this thread.
        let mut leg = entry(1, "t-1", "", "", "");
        leg.reviewer = Reviewer::default();
        let e = select_parent_thread(&[leg], &id("openai", "gpt-5.1", "fp1"), "fork", "t-1")
            .unwrap_err();
        assert_eq!(
            e,
            "thread t-1 has unknown provenance (recorded before 0.3.0); use -Mode new"
        );
    }

    #[test]
    fn fork_needs_parent_when_none_of_lineage() {
        let entries = vec![entry(1, "t-1", "ZAI", "glm-5.3", "fpz")];
        let e = select_parent_thread(&entries, &id("openai", "gpt-5.1", "fp1"), "fork", "")
            .unwrap_err();
        assert!(e.starts_with("-Mode fork needs a parent thread: no thread of lineage openai :: gpt-5.1 in this task's ledger"), "{e}");
        assert!(e.contains("other lineage(s): ZAI :: glm-5.3"), "{e}");
        assert!(
            e.ends_with("Pass -Thread <uuid> of lineage openai :: gpt-5.1, or use -Mode new"),
            "{e}"
        );
    }

    #[test]
    fn auto_forks_newest_same_lineage() {
        let entries = vec![
            entry(1, "t-1", "openai", "gpt-5.1", "fp1"),
            entry(2, "t-2", "openai", "gpt-5.1", "fp1"),
        ];
        let r = select_parent_thread(&entries, &id("openai", "gpt-5.1", "fp1"), "", "").unwrap();
        assert_eq!(r.parent_thread, "t-2");
        assert_eq!(r.mode, "fork");
        assert_eq!(
            r.note,
            "newest thread of lineage openai :: gpt-5.1 (consult n=2)"
        );
    }

    #[test]
    fn thread_drift_refused() {
        let entries = vec![entry(1, "t-1", "openai", "gpt-5.1", "OLDfp")];
        let e = select_parent_thread(&entries, &id("openai", "gpt-5.1", "NEWfp"), "resume", "t-1")
            .unwrap_err();
        assert!(
            e.contains("endpoint or protocol of provider openai changed since thread t-1"),
            "{e}"
        );
        assert!(e.contains("start a new thread with -Mode new"), "{e}");
    }
}

#[cfg(test)]
mod kill_record_tests {
    //! (wave 28e, E1 / E18 / E23) The kill's texts and the record it keeps, against the plugin's
    //! (`Format-KillText`, `Get-KillUnverifiedText`, `Get-KillUnconfirmedWhy`,
    //! `New-UnverifiedEntries`, `Add-KillCheck` and the main turn's outcome in `codex-consult.ps1`).
    use super::*;
    use c3_core::engine::KillCheck;

    fn check(confirmed: bool, why: &str, root: u32, unverified: &[u32]) -> KillCheck {
        KillCheck {
            root_pid: root,
            confirmed,
            why: why.to_string(),
            unverified: unverified.to_vec(),
        }
    }

    #[test]
    fn main_turn_outcomes_name_every_group() {
        // E1: a survivor AND a descendant whose start time could not be read
        let k = check(false, "start time of pid 22 unreadable", 10, &[22]);
        assert_eq!(
            main_kill_outcome("timeout after 4 s", &k, &[11]),
            "failed: timeout after 4 s (process tree killed; 1 processes survived: pid 11; start time of pid 22 unreadable; pid 22 may still run; the next run for this task is refused until they exit)"
        );
        // E18: zero survivors, one unverified descendant (two: "they exit")
        assert_eq!(
            main_kill_outcome("timeout after 4 s", &k, &[]),
            "failed: timeout after 4 s (kill not confirmed: start time of pid 22 unreadable; pid 22 may still run; the next run for this task is refused until it exits)"
        );
        let k2 = check(false, "start time of pid 22, 23 unreadable", 10, &[22, 23]);
        assert!(main_kill_outcome("timeout after 4 s", &k2, &[]).ends_with(
            "pid 22, 23 may still run; the next run for this task is refused until they exit)"
        ));
        // E23: a kill not confirmed that names no pid - the root may still run
        let d = check(
            false,
            "the children could not be enumerated (process inspection denied (test hook CODEX_CONSULT_TEST_KILL_DENIED)) and taskkill /T /F failed (exit 1) - the root exited, its children may not have",
            10,
            &[],
        );
        assert_eq!(
            main_kill_outcome("timeout after 4 s", &d, &[]),
            "failed: timeout after 4 s (kill not confirmed: the children could not be enumerated (process inspection denied (test hook CODEX_CONSULT_TEST_KILL_DENIED)) and taskkill /T /F failed (exit 1) - the root exited, its children may not have; pid 10 may still run)"
        );
        // a confirmed kill, and survivors without unverified descendants
        assert_eq!(
            main_kill_outcome("timeout after 4 s", &KillCheck::confirmed(10), &[]),
            "failed: timeout after 4 s (process tree killed)"
        );
        assert_eq!(
            main_kill_outcome("timeout after 4 s", &check(false, "", 10, &[]), &[11, 12]),
            "failed: timeout after 4 s (process tree killed; 2 processes survived: pid 11, 12; the next run for this task is refused until they exit)"
        );
        // a secondary turn's text has no refusal clause
        assert_eq!(
            format_kill_text(&k, &[11]),
            "(process tree killed; 1 processes survived: pid 11; start time of pid 22 unreadable; pid 22 may still run)"
        );
        assert_eq!(
            format_kill_text(&k, &[]),
            "(kill not confirmed: start time of pid 22 unreadable; pid 22 may still run)"
        );
    }

    #[test]
    fn kill_unconfirmed_why_only_for_a_kill_that_names_no_pid() {
        // the harness's Get-KillUnconfirmedWhy table: w1 | confirmed | survivors | unverified | no why
        let cases = [
            (check(false, "w1", 1, &[]), vec![]),
            (KillCheck::confirmed(1), vec![]),
            (check(false, "w2", 1, &[]), vec![5]),
            (check(false, "w3", 1, &[6]), vec![]),
            (check(false, "", 1, &[]), vec![]),
        ];
        let got: Vec<String> = cases
            .iter()
            .map(|(k, s)| kill_unconfirmed_why(k, s))
            .collect();
        assert_eq!(got.join("|"), "w1||||the kill was not confirmed");
    }

    #[test]
    fn the_record_is_kept_for_survivors_unverified_pids_or_an_unknown_tree() {
        // New-UnverifiedEntries: {pid, why} per unverified pid (none: no entry)
        let k = check(false, "start time of pid 8, 9 unreadable", 1, &[8, 9]);
        assert_eq!(
            serde_json::Value::Array(unverified_entries(&k)),
            serde_json::json!([
                { "pid": 8, "why": "start time of pid 8, 9 unreadable" },
                { "pid": 9, "why": "start time of pid 8, 9 unreadable" }
            ])
        );
        assert!(unverified_entries(&check(false, "w", 1, &[])).is_empty());
        // E18: only unverified pids keep it (survivors [] beside them)
        let kept = kept_kill(&k, &[]).expect("kept for unverified pids");
        assert!(kept.survivors.is_empty() && kept.unverified.len() == 2);
        assert!(kept.kill_unconfirmed.is_empty());
        // E23: neither survivors nor unverified pids, the kill not confirmed: the unknown tree
        let kept = kept_kill(&check(false, "denied", 1, &[]), &[]).expect("kept");
        assert!(kept.survivors.is_empty() && kept.unverified.is_empty());
        assert_eq!(kept.kill_unconfirmed, "denied");
        // survivors
        assert_eq!(
            kept_kill(&check(false, "", 1, &[]), &[7]).map(|k| k.survivors),
            Some(vec![7])
        );
        // a confirmed kill keeps nothing
        assert!(kept_kill(&KillCheck::confirmed(1), &[]).is_none());
    }

    #[test]
    fn add_kill_check_warnings() {
        let k = check(false, "start time of pid 22 unreadable", 10, &[22]);
        assert_eq!(
            kill_check_warning(&k, &[], "main turn").as_deref(),
            Some("kill not confirmed (main turn): start time of pid 22 unreadable; pid 22 may still run - check it, and stop it by hand if it does")
        );
        assert_eq!(
            kill_check_warning(&k, &[11], "format repair").as_deref(),
            Some("kill not confirmed (format repair): 1 processes survived: pid 11; start time of pid 22 unreadable; pid 22 may still run - check them, and stop them by hand if they do")
        );
        assert_eq!(
            kill_check_warning(&check(false, "denied", 10, &[]), &[], "timeout continuation")
                .as_deref(),
            Some("kill not confirmed (timeout continuation): denied; pid 10 may still run - check it, and stop it by hand if it does")
        );
        // survivors only, or a confirmed kill: no warning
        assert!(kill_check_warning(&check(false, "", 10, &[]), &[11], "main turn").is_none());
        assert!(kill_check_warning(&KillCheck::confirmed(10), &[], "main turn").is_none());
    }

    #[test]
    fn a_kill_writes_the_record_it_keeps_at_the_kill() {
        // (wave 3c, F23-3) the record is on disk when the kill's handling returns - before the run
        // goes on - built on the record the run has on disk; the end of the run writes the same
        let dir = std::env::temp_dir().join(format!(
            "c3-killsite-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        let task = TaskSlug::new("t").unwrap();
        std::fs::create_dir_all(dir.join("t")).unwrap();
        let store = FilesStore::new(dir.clone());
        let pending = PendingRef::single(task);
        let path = store_pending_path(&store, &pending);
        let base = PendingRecord {
            state: PendingState::Running,
            n: 1,
            nn: "01".into(),
            child_pid: Some(999_999),
            ..Default::default()
        };
        let read = || -> serde_json::Value {
            serde_json::from_slice(&std::fs::read(&path).expect("the record")).unwrap()
        };
        let mut sec = Secondary {
            kill_site: Some(KillSite {
                store: store.clone(),
                pending: pending.clone(),
                on: base.clone(),
                pause_ms: 0,
            }),
            main_kept: Some(KeptKill {
                kill_unconfirmed: "the main kill".into(),
                ..Default::default()
            }),
            ..Secondary::default()
        };
        // a kill that keeps nothing writes nothing
        let _ = secondary_kill(
            "timeout after 30 s",
            Some(KillCheck::confirmed(6)),
            &[],
            "timeout continuation",
            &mut sec,
        );
        assert!(!path.exists());
        // an unverified descendant: written at once; the main turn's unknown-tree why stays
        let _ = secondary_kill(
            "timeout after 30 s",
            Some(check(false, "start time of pid 5 unreadable", 4, &[5])),
            &[],
            "timeout continuation",
            &mut sec,
        );
        let r = read();
        assert_eq!(r["state"], "survivors");
        assert_eq!(r["child_pid"], 999_999);
        assert_eq!(r["survivors"], serde_json::json!([]));
        assert_eq!(
            r["unverified"],
            serde_json::json!([{ "pid": 5, "why": "start time of pid 5 unreadable" }])
        );
        assert_eq!(r["kill_unconfirmed"], "the main kill");
        // a survivor (this process): its entry read AT the kill, and reused by the end of the run
        let me = std::process::id();
        let _ = secondary_kill(
            "timeout after 30 s",
            Some(check(false, "", 4, &[])),
            &[me],
            "format repair",
            &mut sec,
        );
        let r = read();
        let info = crate::liveness::proc::process_info(me).expect("this process");
        assert_eq!(
            r["survivors"],
            serde_json::json!([{ "pid": me, "start_time": info.start, "name": info.name }])
        );
        assert_eq!(r["unverified"], serde_json::json!([]));
        let kept = sec.kept_kill.clone().expect("kept");
        assert_eq!(kept.entries.as_ref().map(|e| e.len()), Some(1));
        let end = kept_record(
            &base,
            &merged_kept(sec.main_kept.clone(), Some(kept)).expect("merged"),
        );
        let end: serde_json::Value = serde_json::from_slice(&end.to_bytes().unwrap()).unwrap();
        assert_eq!(end, r);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_secondary_kill_keeps_the_record_and_warns() {
        let mut sec = Secondary::default();
        let text = secondary_kill(
            "timeout after 30 s",
            Some(check(false, "start time of pid 5 unreadable", 4, &[5])),
            &[],
            "format repair",
            &mut sec,
        );
        assert_eq!(
            text,
            "timeout after 30 s (kill not confirmed: start time of pid 5 unreadable; pid 5 may still run)"
        );
        assert_eq!(sec.kill_warnings.len(), 1);
        assert!(sec.kept_kill.is_some());
        assert_eq!(sec.kill_checks.len(), 1);
        // a clean kill of the next turn neither warns nor replaces what was kept
        let text = secondary_kill(
            "timeout after 30 s",
            Some(KillCheck::confirmed(6)),
            &[],
            "timeout continuation",
            &mut sec,
        );
        assert_eq!(text, "timeout after 30 s (process tree killed)");
        assert_eq!(sec.kill_warnings.len(), 1);
        assert_eq!(sec.kill_checks.len(), 2);
        assert!(sec
            .kept_kill
            .as_ref()
            .is_some_and(|k| k.unverified.len() == 1));
    }
}
