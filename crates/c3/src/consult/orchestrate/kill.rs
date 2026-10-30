//! The **tree-kill machinery** of a `c3 consult` run: when a turn is killed (its timeout, a
//! stall, an operator `-Kick`), who/what the kill left behind and how its recovery record is
//! written. [`KillSite`] writes the record twice around the kill (before via its hook, after
//! via [`KillSite::record`]); [`KeptKill`] / [`PreKill`] are what a kill kept and what it
//! wrote beforehand; the builders ([`kept_kill`], [`merged_kept`], [`kept_record`],
//! [`pre_kill_record`]), the warning/outcome text ([`kill_check_warning`],
//! [`main_kill_outcome`], [`format_kill_text`]) and the main/secondary kill drivers
//! ([`main_kill_check`], [`secondary_kill`]) live here too.

use super::*;

/// (wave 28e, E1 / E18 / E23) What a tree kill left behind that keeps the recovery record (state
/// `survivors`): its survivors, the descendants it could not verify (`unverified[]` `{pid, why}`),
/// and - a kill that was not confirmed and names no pid - its why (`kill_unconfirmed`).
#[derive(Debug, Clone, Default)]
pub(super) struct KeptKill {
    pub(super) survivors: Vec<u32>,
    pub(super) unverified: Vec<serde_json::Value>,
    pub(super) kill_unconfirmed: String,
    /// (wave 3c, F23-3) The survivors' `{pid, start_time, name}` entries, read AT the kill
    /// (`New-SurvivorEntries` where the plugin calls it); `None` until the kill site reads them.
    pub(super) entries: Option<Vec<serde_json::Value>>,
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
pub(super) fn format_kill_text(c: &c3_core::engine::KillCheck, survivors: &[u32]) -> String {
    // (wave 7b, F38-1) no record anywhere: the tree was left alive
    if c.not_killed {
        return format!("(not killed: {})", c.why);
    }
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
pub(super) fn kill_check_warning(
    c: &c3_core::engine::KillCheck,
    survivors: &[u32],
    turn: &str,
) -> Option<String> {
    if c.confirmed {
        return None;
    }
    // (wave 7b, F38-1) no record anywhere: the tree was left alive
    if c.not_killed {
        return Some(format!(
            "not killed ({turn}): {}; left running: pid {}",
            c.why,
            join_pids(survivors)
        ));
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
pub(super) fn kept_kill(c: &c3_core::engine::KillCheck, survivors: &[u32]) -> Option<KeptKill> {
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
pub(super) fn merged_kept(main: Option<KeptKill>, secondary: Option<KeptKill>) -> Option<KeptKill> {
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
pub(super) fn kept_record(on: &PendingRecord, k: &KeptKill) -> PendingRecord {
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

/// (wave 3c, F23-3; wave 3e, F30-2) Where a turn's kill writes its recovery record - twice. BEFORE
/// the kill ([`KillSite::hook`]: the tree is enumerated, nothing is stopped yet) the record on disk
/// becomes state `survivors` naming the root and every descendant with its start time (one whose
/// start time cannot be read in `unverified[]`) and `kill_unconfirmed` (the kill's result is not
/// recorded yet): a bridge that dies during the kill, or before its result is written, leaves a
/// record the next run scans for that tree. AFTER the kill ([`KillSite::record`]) the record says
/// what the kill kept (survivors, unverified pids, an unknown tree - the plugin's write at its kill
/// sites, `$pendingRecord.state = 'survivors'` ... `Write-PendingFile`) or, a kill that kept nothing,
/// is put back as it was before the kill. A write that fails at either point makes the kill NOT
/// confirmed ([`mark_record_failure`]): warned, no further turn, and no commit unless the end of the
/// run writes the evidence. `on` is the record a kept kill is built on (the run's base record; the
/// format repair's, which names the saved prose). TEST HOOKS (test mode only):
/// `CODEX_CONSULT_TEST_KILL_PAUSE_MS=<ms> | <model>=<ms>[|...]` - a pause held at a kill site (the
/// window a test terminates the bridge in), placed by
/// `CODEX_CONSULT_TEST_KILL_PAUSE_AT=<write|kill>[,<main|continuation|repair>]`: `write` (the
/// default) right after the record's write after the kill, `kill` right after the kill and BEFORE
/// its result is written (the record on disk is the one written before the kill); at every kill
/// site unless a turn is named. (Wave 7, F30-2) The write before the kill that fails is retried ONCE
/// into the record's fallback file ([`crate::liveness::pending::kill_fallback_path`], read by the
/// recovery as any record), and what it came to goes back to the killer
/// ([`crate::engines::subprocess::KillRecord`]); TEST HOOK (test mode only):
/// `CODEX_CONSULT_TEST_KILL_FALLBACK_FAIL=1` - the fallback write fails too. (Wave 7b, F38-1) NO
/// KILL WITHOUT A RECORD: the fallback failing too, the record goes into its third location, the
/// codex home's copy ([`crate::liveness::pending::kill_copy_path`], outside the collab directory;
/// the recovery reads it as any record of the task); all three failing, the killer is told
/// [`crate::engines::subprocess::KillRecord::Lost`] and leaves the tree alive as the evidence.
/// TEST HOOK (test mode only): `CODEX_CONSULT_TEST_KILL_THIRD_FAIL=1` - the copy's write fails too.
#[derive(Clone)]
pub(super) struct KillSite {
    pub(super) store: FilesStore,
    pub(super) pending: PendingRef,
    pub(super) on: PendingRecord,
    /// (wave 7b, F38-1) This run's codex home copy of the kill record
    /// (`<codex home>/c3/pending-kill/<task>-<run id>.json`); `None`: no codex home resolves (the
    /// unit tests: none unless a test sets one).
    pub(super) copy: Option<PathBuf>,
    pub(super) pause_ms: u64,
    /// `true`: the test pause is held right after the kill, before its result is written.
    pub(super) pause_at_kill: bool,
    /// The turn the test pause applies to (`""`: every kill site).
    pub(super) pause_turn: String,
    /// What the current turn's kill wrote before it stopped anything (shared with its hook).
    pub(super) armed: std::sync::Arc<std::sync::Mutex<PreKill>>,
}

/// (wave 3e, F30-2) What a turn's kill wrote BEFORE it stopped anything.
#[derive(Debug, Clone, Default)]
pub(super) struct PreKill {
    /// The turn: `main turn`, `timeout continuation`, `format repair`.
    pub(super) turn: String,
    /// The kill started (its hook heard [`crate::engines::subprocess::KillEvent::Before`]).
    pub(super) fired: bool,
    /// The record on disk before that write (put back after a kill that keeps nothing).
    pub(super) prior: Option<PendingRecord>,
    /// The record written before the kill is on disk; `error`: why it is not.
    pub(super) written: bool,
    pub(super) error: Option<String>,
    /// (wave 7, F30-2) That write failed and the record went into its fallback file - here.
    pub(super) fallback: Option<PathBuf>,
    /// (wave 7b, F38-1) The fallback failed too and the record went into the codex home's copy -
    /// here.
    pub(super) copy: Option<PathBuf>,
}

/// (wave 7, F30-2) The record written before a kill, into the fallback file `path` (the recovery
/// record could not be replaced). `Err`: why not.
fn write_kill_fallback(path: &Path, record: &PendingRecord) -> Result<(), String> {
    if c3_core::test_hooks::hook("CODEX_CONSULT_TEST_KILL_FALLBACK_FAIL")
        .is_some_and(|v| v.trim() == "1")
    {
        return Err(
            "the write failed (test hook CODEX_CONSULT_TEST_KILL_FALLBACK_FAIL)".to_string(),
        );
    }
    let bytes = record
        .to_bytes()
        .map_err(|e| c3_core::one_line(&e.to_string()))?;
    c3_core::store::write_text_atomic(path, &bytes).map_err(|e| c3_core::one_line(&e.to_string()))
}

/// (wave 7b, F38-1) The record written before a kill, into the codex home's copy `path` (neither
/// the recovery record nor its fallback could be written): the fallback's JSON plus `collab_dir`
/// and `task`, the directory created when missing. `Err`: why not.
fn write_kill_copy(
    path: &Path,
    record: &PendingRecord,
    collab_dir: &Path,
    task: &str,
) -> Result<(), String> {
    if c3_core::test_hooks::hook("CODEX_CONSULT_TEST_KILL_THIRD_FAIL")
        .is_some_and(|v| v.trim() == "1")
    {
        return Err("the write failed (test hook CODEX_CONSULT_TEST_KILL_THIRD_FAIL)".to_string());
    }
    let mut r = record.clone();
    r.extra.insert(
        "collab_dir".to_string(),
        serde_json::Value::String(collab_dir.to_string_lossy().to_string()),
    );
    r.extra.insert(
        "task".to_string(),
        serde_json::Value::String(task.to_string()),
    );
    let bytes = r
        .to_bytes()
        .map_err(|e| c3_core::one_line(&e.to_string()))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| c3_core::one_line(&e.to_string()))?;
    }
    c3_core::store::write_text_atomic(path, &bytes).map_err(|e| c3_core::one_line(&e.to_string()))
}

/// (wave 7b, F38-1) This run's codex home copy path: `<codex home>/c3/pending-kill/<task>-<run
/// id>.json`, the run id its consultation id (a fresh one when it has none).
fn kill_copy_of(home: Option<&Path>, pending: &PendingRef, on: &PendingRecord) -> Option<PathBuf> {
    let home = home?;
    let id = on.consult_id.trim();
    let run_id = if !id.is_empty() && uuid::Uuid::parse_str(id).is_ok() {
        id.to_string()
    } else {
        uuid::Uuid::new_v4().to_string()
    };
    Some(crate::liveness::pending::kill_copy_path(
        home,
        pending.task.as_str(),
        &run_id,
    ))
}

/// (wave 3e, F30-2) The `kill_unconfirmed` of the record written before a kill: until the kill's
/// result replaces it, the tree counts as unknown.
fn pre_kill_why(turn: &str, root: u32) -> String {
    format!("the kill of the {turn} (pid {root}) started and its result was never recorded")
}

/// (wave 3e, F30-2) The `note` of the record written before a kill.
fn pre_kill_note(turn: &str, root: u32) -> String {
    format!("the kill of the {turn} (pid {root}) is in progress; its result is not recorded yet")
}

/// (wave 3e, F30-2) The record a kill writes BEFORE it stops anything: `prior` (the record on disk)
/// in state `survivors`, naming the root and every live descendant of the kill's enumeration
/// (`{pid, start_time, name}`; one whose start time could not be read in `unverified[]` `{pid,
/// why}`), with `kill_unconfirmed` ([`pre_kill_why`], unless the record already holds one). A tree
/// that could not be enumerated names the root only - the unknown-tree scan covers the rest.
fn pre_kill_record(
    prior: &PendingRecord,
    turn: &str,
    root: u32,
    tree: &Result<Vec<(u32, String)>, String>,
) -> PendingRecord {
    let mut r = prior.clone();
    r.state = PendingState::Survivors;
    let mut known = vec![root];
    let mut unknown: Vec<u32> = Vec::new();
    if let Ok(descendants) = tree {
        for (pid, start) in descendants {
            match start.as_str() {
                "<gone>" => {}
                "" => unknown.push(*pid),
                _ => known.push(*pid),
            }
        }
    }
    let named = |list: &[serde_json::Value], pid: u32| {
        list.iter()
            .any(|e| e.get("pid").and_then(|p| p.as_u64()) == Some(u64::from(pid)))
    };
    for e in survivor_entries(&known) {
        let pid = e.get("pid").and_then(|p| p.as_u64()).unwrap_or(0) as u32;
        if !named(&r.survivors, pid) {
            r.survivors.push(e);
        }
    }
    if !unknown.is_empty() {
        let why = format!("start time of pid {} unreadable", join_pids(&unknown));
        for pid in &unknown {
            if !named(&r.unverified, *pid) {
                r.unverified
                    .push(serde_json::json!({ "pid": pid, "why": why }));
            }
        }
    }
    if r.kill_unconfirmed.as_deref().unwrap_or("").is_empty() {
        r.kill_unconfirmed = Some(pre_kill_why(turn, root));
    }
    r.note = pre_kill_note(turn, root);
    r
}

impl KillSite {
    /// The site of a run's kills: the test pause from `CODEX_CONSULT_TEST_KILL_PAUSE_MS` (for
    /// `model`) and `CODEX_CONSULT_TEST_KILL_PAUSE_AT`.
    pub(super) fn new(
        store: FilesStore,
        pending: PendingRef,
        on: PendingRecord,
        model: &str,
    ) -> KillSite {
        let pause_ms = test_hook_ms(
            &c3_core::test_hooks::hook("CODEX_CONSULT_TEST_KILL_PAUSE_MS").unwrap_or_default(),
            model,
        )
        .unwrap_or(0);
        let at = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_KILL_PAUSE_AT").unwrap_or_default();
        let mut parts = at.split(',').map(|t| t.trim().to_ascii_lowercase());
        let pause_at_kill = parts.next().is_some_and(|p| p == "kill");
        let pause_turn = match parts.next().as_deref() {
            Some("main") => "main turn",
            Some("continuation") => "timeout continuation",
            Some("repair") => "format repair",
            _ => "",
        }
        .to_string();
        // (wave 7b, F38-1) the unit tests never write into the codex home of the machine
        let home = if cfg!(test) {
            None
        } else {
            crate::liveness::pending::kill_copy_home()
        };
        let copy = kill_copy_of(home.as_deref(), &pending, &on);
        KillSite {
            store,
            pending,
            on,
            copy,
            pause_ms,
            pause_at_kill,
            pause_turn,
            armed: Default::default(),
        }
    }

    /// The test pause, when it is placed here (`at_kill`) for this `turn`.
    fn pause(&self, at_kill: bool, turn: &str) {
        if self.pause_ms > 0
            && self.pause_at_kill == at_kill
            && (self.pause_turn.is_empty() || self.pause_turn == turn)
        {
            std::thread::sleep(Duration::from_millis(self.pause_ms));
        }
    }

    /// (wave 3e, F30-2) The hook of `turn`'s kills (it arms this site for that turn): BEFORE the
    /// kill the record that names the tree is written ([`pre_kill_record`], on the record on disk);
    /// after it, the `kill` test pause.
    pub(super) fn hook(&self, turn: &str) -> crate::engines::subprocess::KillHook {
        if let Ok(mut a) = self.armed.lock() {
            *a = PreKill {
                turn: turn.to_string(),
                ..Default::default()
            };
        }
        let site = self.clone();
        let turn = turn.to_string();
        std::sync::Arc::new(move |ev| match ev {
            crate::engines::subprocess::KillEvent::Before { root, tree } => {
                site.before_kill(&turn, root, tree)
            }
            crate::engines::subprocess::KillEvent::After => {
                site.pause(true, &turn);
                crate::engines::subprocess::KillRecord::NotDue
            }
        })
    }

    /// The write before the kill; (wave 7, F30-2) a failed one retried ONCE into the fallback file,
    /// (wave 7b, F38-1) that failing too, into the codex home's copy, and what it came to returned
    /// to the killer: all three failing, [`crate::engines::subprocess::KillRecord::Lost`] with the
    /// three errors - the killer leaves the tree alive. (Wave 7d, F42-1 sweep) A record on disk
    /// that cannot be inspected is not replaced: it counts as the record's failed write.
    fn before_kill(
        &self,
        turn: &str,
        root: u32,
        tree: &Result<Vec<(u32, String)>, String>,
    ) -> crate::engines::subprocess::KillRecord {
        use crate::engines::subprocess::KillRecord;
        let path = store_pending_path(&self.store, &self.pending);
        let on_run = || {
            let mut r = self.on.clone();
            r.state = PendingState::Running;
            r.child_pid = Some(root);
            r
        };
        // (wave 7d, F42-1 sweep) the record on disk absent: built on the run's record; one that
        // cannot be inspected (or read back as a record) is NOT replaced - it may name what an
        // earlier kill left: the record before the kill (built on the run's record) goes to its
        // fallback, then to the copy, the inspection failure standing for the record's write error
        let rd = crate::liveness::pending::read_pending_file(&path);
        let (prior, uninspected) = match (rd.error, rd.record) {
            (Some(e), _) => (on_run(), Some(e)),
            (None, Some(v)) => match serde_json::from_value::<PendingRecord>(v) {
                Ok(r) => (r, None),
                Err(e) => (
                    on_run(),
                    Some(crate::liveness::pending::record_read_refusal(
                        &path,
                        &std::io::Error::other(format!("it does not read back as a record: {e}")),
                    )),
                ),
            },
            (None, None) => (on_run(), None),
        };
        let record = pre_kill_record(&prior, turn, root, tree);
        let written = match uninspected {
            Some(e) => Err(c3_core::one_line(&e)),
            None => self
                .store
                .write_pending(&self.pending, &record)
                .map_err(|e| c3_core::one_line(&e.to_string())),
        };
        let mut fallback: Option<PathBuf> = None;
        let mut copy: Option<PathBuf> = None;
        let outcome = match &written {
            Ok(()) => KillRecord::Durable { fallback: false },
            Err(e) => {
                let fb = crate::liveness::pending::kill_fallback_path(&path);
                match write_kill_fallback(&fb, &record) {
                    Ok(()) => {
                        fallback = Some(fb);
                        KillRecord::Durable { fallback: true }
                    }
                    Err(f) => {
                        let third = match &self.copy {
                            Some(c) => write_kill_copy(
                                c,
                                &record,
                                &self.store.collab,
                                self.pending.task.as_str(),
                            )
                            .map(|()| c.clone())
                            .map_err(|t| format!("its copy {}: {t}", c.display())),
                            None => Err(
                                "its copy in the codex home: no codex home (CODEX_HOME, HOME and USERPROFILE resolve none)"
                                    .to_string(),
                            ),
                        };
                        match third {
                            Ok(c) => {
                                copy = Some(c);
                                KillRecord::Durable { fallback: true }
                            }
                            Err(t) => KillRecord::Lost(format!(
                                "{e}; its fallback {}: {f}; {t}",
                                fb.display()
                            )),
                        }
                    }
                }
            }
        };
        if let Ok(mut a) = self.armed.lock() {
            a.turn = turn.to_string();
            a.fired = true;
            a.prior = Some(prior);
            a.written = written.is_ok();
            a.error = written.err();
            a.fallback = fallback;
            a.copy = copy;
        }
        outcome
    }

    /// (wave 7b, F38-1) The recovery record is written clean at the end of the run (it names what
    /// the kills left): this run's codex home copy, if a kill wrote one, is not needed any more and
    /// is removed (a failed removal leaves it to the next run's recovery, which consumes it).
    pub(super) fn drop_copy(&self) {
        if let Some(c) = &self.copy {
            if c.exists() {
                if let Err(e) = std::fs::remove_file(c) {
                    eprintln!(
                        "{TOOL}: could not remove the kill record's copy {} ({e}); the next run consumes it",
                        c.display()
                    );
                }
            }
        }
    }

    /// What the current turn's kill wrote before it stopped anything.
    pub(super) fn armed(&self) -> PreKill {
        self.armed.lock().map(|a| a.clone()).unwrap_or_default()
    }

    /// AFTER the kill: the record says what the kill kept (`k`; the survivors' entries are read
    /// now, once), or - a kill that kept nothing - is put back as it was before the kill; then the
    /// `write` test pause. `Err`: that write failed.
    pub(super) fn record(&self, k: Option<&mut KeptKill>) -> Result<(), String> {
        let pre = self.armed();
        let written = match k {
            Some(k) => {
                if k.entries.is_none() {
                    k.entries = Some(survivor_entries(&k.survivors));
                }
                Some(
                    self.store
                        .write_pending(&self.pending, &kept_record(&self.on, k)),
                )
            }
            None if pre.fired && pre.written => pre
                .prior
                .as_ref()
                .map(|r| self.store.write_pending(&self.pending, r)),
            None => None,
        };
        let Some(written) = written else {
            return Ok(());
        };
        self.pause(false, &pre.turn);
        written.map_err(|e| c3_core::one_line(&e.to_string()))
    }
}

/// (wave 3e, F30-2) A kill whose record could not be written (`err`) is NOT confirmed - whatever it
/// stopped, the evidence of it may be missing: its why becomes `the kill's record could not be
/// written (<err>)` (unless it has one) and it keeps the record, an unknown tree.
pub(super) fn mark_record_failure(
    c: &mut c3_core::engine::KillCheck,
    survivors: &[u32],
    kept: &mut Option<KeptKill>,
    err: &str,
) {
    let why = format!("the kill's record could not be written ({err})");
    c.confirmed = false;
    if c.why.is_empty() {
        c.why = why.clone();
    }
    let k = kept.get_or_insert_with(|| KeptKill {
        survivors: survivors.to_vec(),
        ..Default::default()
    });
    if k.kill_unconfirmed.is_empty() {
        k.kill_unconfirmed = why;
    }
}

/// (wave 3e, F30-2) What a kill keeps, as the warning names it (`survivors`, `unverified pids`,
/// `unconfirmed kill`).
pub(super) fn kept_what(c: &c3_core::engine::KillCheck, survivors: &[u32]) -> &'static str {
    if !survivors.is_empty() {
        "survivors"
    } else if !c.unverified.is_empty() {
        "unverified pids"
    } else {
        "unconfirmed kill"
    }
}

/// (wave 3e, F30-2) The warning of a kill whose record could not be written - before the kill
/// (`pre`), after it (`post`) or both - and whether the evidence is LOST (no record of the kill on
/// disk: written neither before nor after it, nor (wave 7) into the fallback file, nor (wave 7b)
/// into the codex home's copy; a kill that found none of the three did not happen). The plugin's
/// form when it is lost: `the survivors|unverified pids|unconfirmed kill could not be recorded
/// (<error>) - <path> still names only child pid <n>`. `None`: both writes succeeded (or none was
/// due).
pub(super) fn kill_record_warning(
    pre: &PreKill,
    post: &Result<(), String>,
    what: &str,
    path: &Path,
    child: u32,
) -> Option<(String, bool)> {
    let path = path.display();
    match (pre.error.as_deref(), post.as_ref().err()) {
        (None, None) => None,
        (_, Some(e)) if pre.written => Some((
            format!(
                "the {what} could not be recorded ({e}) - {path} keeps the record written before the kill (its tree unknown: the next run for this task is refused until a scan finds none of it)"
            ),
            false,
        )),
        (_, Some(e)) if pre.fallback.is_some() || pre.copy.is_some() => Some((
            format!(
                "the {what} could not be recorded ({e}) - {} keeps the record written before the kill (its tree unknown: the next run for this task is refused until a scan finds none of it); {path} still names only child pid {child}",
                pre.fallback
                    .as_deref()
                    .or(pre.copy.as_deref())
                    .unwrap_or(Path::new(""))
                    .display()
            ),
            false,
        )),
        (Some(e), None) => Some((
            format!(
                "the record could not be written before the kill ({e}) - the kill counts as not confirmed; {path} names what it left{}{}",
                pre.fallback
                    .as_deref()
                    .map(|f| format!(" ({} keeps the record written before the kill until the next run consumes it)", f.display()))
                    .unwrap_or_default(),
                pre.copy
                    .as_deref()
                    .map(|c| format!(" ({} keeps the record written before the kill until the end of the run writes the record clean or the next run consumes it)", c.display()))
                    .unwrap_or_default()
            ),
            false,
        )),
        (_, Some(e)) => Some((
            format!(
                "the {what} could not be recorded ({e}) - {path} still names only child pid {child}"
            ),
            true,
        )),
    }
}

/// The main turn's kill check: the tree kill's own (`None` - no process, the http engine: a
/// confirmed kill of the recorded child), then the main-turn test hooks, and survivors - the
/// hook's too - make it unconfirmed. (wave 28e, E1) TEST HOOK (test mode only):
/// `CODEX_CONSULT_TEST_UNVERIFIED=<pid>[,<pid>]` - these pids, when alive, are reported as
/// descendants of this kill whose start time could not be read (only ever stricter). C3's
/// `CODEX_CONSULT_TEST_KILL_UNCONFIRMED` (test mode only) forces the kill unconfirmed.
pub(super) fn main_kill_check(
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
/// refused until it exits)`; else `failed: <stop> <Format-KillText>`. (Wave 7b, F38-1) A kill that
/// did not happen (no record anywhere): `failed: <stop> (not killed: the kill record could not be
/// written anywhere (<errors>): the reviewer tree (pid <root>) is left alive as the evidence; the
/// task is refused until it is gone or recorded)`.
pub(super) fn main_kill_outcome(
    stop: &str,
    c: &c3_core::engine::KillCheck,
    survivors: &[u32],
) -> String {
    // (wave 7b, F38-1) the record of the kill could be written nowhere: no kill - the tree is the
    // evidence, the launch record names its root
    if c.not_killed {
        return format!("failed: {stop} {}", format_kill_text(c, survivors));
    }
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
pub(super) fn secondary_kill(
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
    let mut kept = kept_kill(&c, survivors);
    // (wave 3c, F23-3; wave 3e, F30-2) the record was written BEFORE this kill (its hook); it is
    // updated HERE, at the kill, with what the kill kept (or put back). A write that failed - before
    // or now - makes the kill not confirmed: warned, and the run commits nothing unless the end of
    // the run writes the evidence.
    let mut record_warning: Option<(String, bool)> = None;
    if let Some(site) = sec.kill_site.clone() {
        let pre = site.armed();
        if let Some(e) = &pre.error {
            mark_record_failure(&mut c, survivors, &mut kept, e);
        }
        let mut merged = kept
            .clone()
            .and_then(|k| merged_kept(sec.main_kept.clone(), Some(k)));
        let post = site.record(merged.as_mut());
        if let (Some(k), Some(m)) = (kept.as_mut(), merged) {
            k.entries = m.entries;
        }
        if let Err(e) = &post {
            mark_record_failure(&mut c, survivors, &mut kept, e);
        }
        let child = pre
            .prior
            .as_ref()
            .and_then(|r| r.child_pid)
            .or(site.on.child_pid)
            .unwrap_or(c.root_pid);
        record_warning = kill_record_warning(
            &pre,
            &post,
            kept_what(&c, survivors),
            &store_pending_path(&site.store, &site.pending),
            child,
        );
    }
    if let Some(w) = kill_check_warning(&c, survivors, turn) {
        sec.kill_warnings.push(w);
    }
    if let Some((w, lost)) = record_warning {
        sec.kill_warnings
            .push(format!("kill not recorded ({turn}): {w}"));
        if lost {
            sec.kill_record_lost = Some(w);
        }
    }
    if let Some(k) = kept {
        sec.kept_kill = Some(k);
    }
    let text = format!("{stop} {}", format_kill_text(&c, survivors));
    sec.kill_checks.push((c, survivors.to_vec()));
    text
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
            not_killed: false,
        }
    }

    /// (Wave 7b, F38-1) A kill that did not happen - its record could be written nowhere - is told
    /// as such: the outcome, the kill text and the warning name the tree left alive.
    #[test]
    fn a_kill_that_did_not_happen_is_told_as_such() {
        let why = "the kill record could not be written anywhere (x; its fallback f: y; its copy c: z): the reviewer tree (pid 9) is left alive as the evidence; the task is refused until it is gone or recorded";
        let mut c = check(false, why, 9, &[]);
        c.not_killed = true;
        assert_eq!(
            main_kill_outcome("timeout after 3 s", &c, &[9, 10]),
            format!("failed: timeout after 3 s (not killed: {why})")
        );
        assert_eq!(
            format_kill_text(&c, &[9, 10]),
            format!("(not killed: {why})")
        );
        assert_eq!(
            kill_check_warning(&c, &[9, 10], "format repair"),
            Some(format!(
                "not killed (format repair): {why}; left running: pid 9, 10"
            ))
        );
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
            kill_site: Some(KillSite::new(
                store.clone(),
                pending.clone(),
                base.clone(),
                "",
            )),
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

    /// (wave 3e, F30-2) A store whose task dir holds a pending record in state `running` naming
    /// `child`; returns the store, its pending ref and the record's path.
    fn running_store(tag: &str, child: u32) -> (FilesStore, PendingRef, PathBuf, PendingRecord) {
        let dir = std::env::temp_dir().join(format!(
            "c3-{tag}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(dir.join("t")).unwrap();
        let store = FilesStore::new(dir);
        let pending = PendingRef::single(TaskSlug::new("t").unwrap());
        let path = store_pending_path(&store, &pending);
        let base = PendingRecord {
            state: PendingState::Running,
            n: 1,
            nn: "01".into(),
            child_pid: Some(child),
            note: "the timeout continuation turn is running".into(),
            ..Default::default()
        };
        store.write_pending(&pending, &base).unwrap();
        (store, pending, path, base)
    }

    fn on_disk(path: &Path) -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(path).expect("the record")).unwrap()
    }

    #[test]
    fn a_kill_writes_its_record_before_the_kill_and_puts_it_back_after_a_clean_one() {
        // (wave 3e, F30-2) the hook writes the record that names the tree BEFORE anything is
        // stopped: the root and the live descendants with their start times (this process stands
        // for both), a descendant whose start time cannot be read in unverified[], one that exited
        // meanwhile left out; the unknown tree until the kill's result is written
        let me = std::process::id();
        let (store, pending, path, base) = running_store("prekill", me);
        let site = KillSite::new(store, pending, base.clone(), "");
        let hook = site.hook("timeout continuation");
        let tree: Result<Vec<(u32, String)>, String> = Ok(vec![
            (4_000_001, String::new()),
            (4_000_002, "<gone>".into()),
        ]);
        hook(crate::engines::subprocess::KillEvent::Before {
            root: me,
            tree: &tree,
        });
        let r = on_disk(&path);
        let info = crate::liveness::proc::process_info(me).expect("this process");
        assert_eq!(r["state"], "survivors");
        assert_eq!(r["child_pid"], me);
        assert_eq!(
            r["survivors"],
            serde_json::json!([{ "pid": me, "start_time": info.start, "name": info.name }])
        );
        assert_eq!(
            r["unverified"],
            serde_json::json!([{ "pid": 4_000_001, "why": "start time of pid 4000001 unreadable" }])
        );
        assert_eq!(
            r["kill_unconfirmed"],
            format!("the kill of the timeout continuation (pid {me}) started and its result was never recorded")
        );
        assert_eq!(
            r["note"],
            format!("the kill of the timeout continuation (pid {me}) is in progress; its result is not recorded yet")
        );
        let pre = site.armed();
        assert!(pre.fired && pre.written && pre.error.is_none());
        // a clean kill keeps nothing: the record is put back as it was before the kill
        let mut sec = Secondary {
            kill_site: Some(site.clone()),
            ..Secondary::default()
        };
        let text = secondary_kill(
            "timeout after 30 s",
            Some(KillCheck::confirmed(me)),
            &[],
            "timeout continuation",
            &mut sec,
        );
        assert_eq!(text, "timeout after 30 s (process tree killed)");
        let back: serde_json::Value = serde_json::from_slice(&base.to_bytes().unwrap()).unwrap();
        assert_eq!(on_disk(&path), back);
        assert!(sec.kept_kill.is_none() && sec.kill_warnings.is_empty());
        assert!(sec.kill_record_lost.is_none());
        // a kill that keeps something: the kept record replaces the one written before the kill
        let hook = site.hook("format repair");
        hook(crate::engines::subprocess::KillEvent::Before {
            root: me,
            tree: &Err("denied (test)".into()),
        });
        assert_eq!(
            on_disk(&path)["survivors"].as_array().map(|a| a.len()),
            Some(1)
        );
        let _ = secondary_kill(
            "timeout after 30 s",
            Some(check(false, "start time of pid 5 unreadable", me, &[5])),
            &[],
            "format repair",
            &mut sec,
        );
        let r = on_disk(&path);
        assert_eq!(r["state"], "survivors");
        assert_eq!(r["survivors"], serde_json::json!([]));
        assert_eq!(
            r["unverified"],
            serde_json::json!([{ "pid": 5, "why": "start time of pid 5 unreadable" }])
        );
        assert!(r.get("kill_unconfirmed").is_none(), "{r}");
        assert_eq!(r["note"], base.note.as_str());
    }

    #[test]
    fn a_kill_whose_record_cannot_be_written_is_not_confirmed_and_loses_nothing_silently() {
        // (wave 3e, F30-2) the record path blocked (a directory stands where the file goes) and -
        // wave 7 - its fallback too: the write before the kill, its retry into the fallback and the
        // write after the kill fail - the killer is told (`KillRecord::Lost`), the kill is NOT
        // confirmed, it keeps the record (an unknown tree), the warning names the path, and the
        // evidence is LOST (the run must not commit unless the end of the run writes it)
        let (store, pending, path, base) = running_store("killrec-blocked", 7_000_001);
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir_all(&path).unwrap();
        let fallback = crate::liveness::pending::kill_fallback_path(&path);
        std::fs::create_dir_all(&fallback).unwrap();
        let site = KillSite::new(store, pending, base, "");
        let hook = site.hook("timeout continuation");
        let told = hook(crate::engines::subprocess::KillEvent::Before {
            root: 7_000_002,
            tree: &Ok(Vec::new()),
        });
        match &told {
            crate::engines::subprocess::KillRecord::Lost(why) => assert!(
                why.contains(&format!("; its fallback {}: ", fallback.display())),
                "{why}"
            ),
            other => panic!("the killer must be told the record is lost: {other:?}"),
        }
        assert!(site.armed().error.is_some());
        assert!(site.armed().fallback.is_none());
        let mut sec = Secondary {
            kill_site: Some(site),
            ..Secondary::default()
        };
        let text = secondary_kill(
            "timeout after 30 s",
            Some(KillCheck::confirmed(7_000_002)),
            &[],
            "timeout continuation",
            &mut sec,
        );
        assert!(
            text.starts_with(
                "timeout after 30 s (kill not confirmed: the kill's record could not be written ("
            ),
            "{text}"
        );
        let (c, _) = sec.kill_checks.last().expect("the check");
        assert!(!c.confirmed);
        let kept = sec.kept_kill.clone().expect("the record is kept");
        assert!(kept
            .kill_unconfirmed
            .starts_with("the kill's record could not be written ("));
        let lost = sec.kill_record_lost.clone().expect("the evidence is lost");
        assert!(
            lost.starts_with("the unconfirmed kill could not be recorded (")
                && lost.ends_with(&format!(
                    "{} still names only child pid 7000002",
                    path.display()
                )),
            "{lost}"
        );
        assert!(sec.kill_warnings.iter().any(|w| w.starts_with(
            "kill not confirmed (timeout continuation): the kill's record could not be written ("
        )));
        assert!(sec
            .kill_warnings
            .contains(&format!("kill not recorded (timeout continuation): {lost}")));
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn a_kill_record_written_only_before_or_only_after_the_kill_is_warned_not_lost() {
        // (wave 3e, F30-2) one of the two writes failed: the kill is not confirmed and warned, the
        // evidence stays on disk - the record written before the kill, or the one after it
        let me = std::process::id();
        // the write BEFORE the kill fails, the one after it succeeds: the record says the unknown
        // tree with why
        let (store, pending, path, base) = running_store("killrec-prefail", me);
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir_all(&path).unwrap();
        let site = KillSite::new(store, pending, base, "");
        site.hook("timeout continuation")(crate::engines::subprocess::KillEvent::Before {
            root: me,
            tree: &Ok(Vec::new()),
        });
        std::fs::remove_dir_all(&path).unwrap();
        let mut sec = Secondary {
            kill_site: Some(site),
            ..Secondary::default()
        };
        let _ = secondary_kill(
            "timeout after 30 s",
            Some(KillCheck::confirmed(me)),
            &[],
            "timeout continuation",
            &mut sec,
        );
        assert!(sec.kill_record_lost.is_none());
        let r = on_disk(&path);
        assert_eq!(r["state"], "survivors");
        assert!(r["kill_unconfirmed"]
            .as_str()
            .is_some_and(|w| w.starts_with("the kill's record could not be written (")));
        assert!(sec.kill_warnings.iter().any(|w| w.starts_with(
            "kill not recorded (timeout continuation): the record could not be written before the kill ("
        )));
        // (Windows: a read-only file cannot be replaced) the write before the kill succeeds, the
        // one after it fails: the record written before the kill stays
        if cfg!(windows) {
            let (store, pending, path, base) = running_store("killrec-postfail", me);
            let site = KillSite::new(store, pending, base, "");
            site.hook("format repair")(crate::engines::subprocess::KillEvent::Before {
                root: me,
                tree: &Ok(Vec::new()),
            });
            let before = on_disk(&path);
            let mut perms = std::fs::metadata(&path).unwrap().permissions();
            perms.set_readonly(true);
            std::fs::set_permissions(&path, perms).unwrap();
            let mut sec = Secondary {
                kill_site: Some(site),
                ..Secondary::default()
            };
            let _ = secondary_kill(
                "timeout after 30 s",
                Some(KillCheck::confirmed(me)),
                &[],
                "format repair",
                &mut sec,
            );
            let mut perms = std::fs::metadata(&path).unwrap().permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            std::fs::set_permissions(&path, perms).unwrap();
            assert!(sec.kill_record_lost.is_none());
            assert_eq!(on_disk(&path), before);
            assert!(sec.kill_warnings.iter().any(|w| w.contains(
                "keeps the record written before the kill (its tree unknown: the next run for this task is refused until a scan finds none of it)"
            )));
        }
    }

    /// (Wave 7, F30-2, RC2) The write before the kill fails: it is retried ONCE into the record's
    /// fallback file (`.consult.pending-kill.json`), the killer is told the record is durable there,
    /// the kill is not confirmed and - the write after it failing too - the evidence is NOT lost:
    /// the warning names the fallback that keeps the record written before the kill. The recovery
    /// reads that file as a record of the task.
    #[test]
    fn a_kill_whose_record_cannot_be_written_keeps_it_in_its_fallback() {
        let (store, pending, path, base) = running_store("killrec-fallback", 7_000_011);
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir_all(&path).unwrap();
        let fallback = crate::liveness::pending::kill_fallback_path(&path);
        assert_eq!(
            fallback.file_name().unwrap().to_string_lossy(),
            ".consult.pending-kill.json"
        );
        let site = KillSite::new(store, pending, base, "");
        let told = site.hook("main turn")(crate::engines::subprocess::KillEvent::Before {
            root: 7_000_012,
            tree: &Ok(Vec::new()),
        });
        assert_eq!(
            told,
            crate::engines::subprocess::KillRecord::Durable { fallback: true }
        );
        let pre = site.armed();
        assert!(!pre.written && pre.error.is_some());
        assert_eq!(pre.fallback.as_deref(), Some(fallback.as_path()));
        let r = on_disk(&fallback);
        assert_eq!(r["state"], "survivors", "{r}");
        assert_eq!(r["n"], 1);
        assert_eq!(r["nn"], "01");
        // the record on disk could not be read back (a directory): built on the run's record, the
        // killed root as its child (no live process: no survivor entry)
        assert_eq!(r["child_pid"], 7_000_012, "{r}");
        assert_eq!(
            r["kill_unconfirmed"],
            "the kill of the main turn (pid 7000012) started and its result was never recorded"
        );
        // the kill's handling: not confirmed, the write after it fails too - warned, not lost
        let mut sec = Secondary {
            kill_site: Some(site),
            ..Secondary::default()
        };
        let text = secondary_kill(
            "timeout after 30 s",
            Some(KillCheck::confirmed(7_000_012)),
            &[],
            "main turn",
            &mut sec,
        );
        assert!(text.contains("kill not confirmed"), "{text}");
        assert!(sec.kill_record_lost.is_none());
        let w = sec
            .kill_warnings
            .iter()
            .find(|w| w.starts_with("kill not recorded (main turn): "))
            .expect("the record warning");
        assert!(
            w.contains(&format!(
                "{} keeps the record written before the kill (its tree unknown",
                fallback.display()
            )) && w.ends_with(&format!(
                "{} still names only child pid 7000012",
                path.display()
            )),
            "{w}"
        );
        // the recovery reads the fallback as a record of the task
        let task_dir = path.parent().unwrap();
        assert!(crate::liveness::pending::pending_paths(task_dir)
            .paths
            .contains(&fallback));
        let rd = crate::liveness::pending::read_pending_file(&fallback);
        assert!(rd.error.is_none() && rd.record.is_some());
        let _ = std::fs::remove_dir_all(task_dir.parent().unwrap());
    }

    /// (Wave 7b, F38-1) The recovery record and its fallback both blocked: the record before the
    /// kill goes into the codex home's copy (`<home>/c3/pending-kill/t-<consult id>.json`, the
    /// fallback's JSON plus `collab_dir` and `task`), the killer is told it is durable, the
    /// warning names the copy, nothing is lost, the recovery reads the copy as a record of the
    /// task, and the record written clean at the end of the run removes it. The copy blocked too:
    /// LOST with the three errors (the killer then leaves the tree alive).
    #[test]
    fn a_kill_whose_collab_writes_fail_keeps_its_record_in_the_codex_home() {
        use crate::engines::subprocess::{KillEvent, KillRecord};
        let (store, pending, path, mut base) = running_store("killrec-copy", 7_000_021);
        base.consult_id = "3c0e7a52-5f0e-4b8e-9d1c-2a7b6f4e8d10".into();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir_all(&path).unwrap();
        std::fs::create_dir_all(crate::liveness::pending::kill_fallback_path(&path)).unwrap();
        let task_dir = path.parent().unwrap().to_path_buf();
        let home = task_dir.parent().unwrap().join("codex-home");
        let mut site = KillSite::new(store.clone(), pending.clone(), base.clone(), "");
        assert!(
            site.copy.is_none(),
            "the unit tests write into no codex home"
        );
        site.copy = kill_copy_of(Some(&home), &pending, &base);
        let copy = crate::liveness::pending::kill_copy_path(&home, "t", &base.consult_id);
        assert_eq!(site.copy.as_deref(), Some(copy.as_path()));
        let told = site.hook("main turn")(KillEvent::Before {
            root: 7_000_022,
            tree: &Ok(Vec::new()),
        });
        assert_eq!(told, KillRecord::Durable { fallback: true });
        let pre = site.armed();
        assert!(pre.error.is_some() && pre.fallback.is_none(), "{pre:?}");
        assert_eq!(pre.copy.as_deref(), Some(copy.as_path()));
        let r = on_disk(&copy);
        assert_eq!(r["state"], "survivors", "{r}");
        assert_eq!(r["task"], "t", "{r}");
        assert_eq!(
            r["collab_dir"],
            store.collab.to_string_lossy().as_ref(),
            "{r}"
        );
        assert_eq!(r["consult_id"], base.consult_id.as_str(), "{r}");
        assert_eq!(r["child_pid"], 7_000_022, "{r}");
        assert_eq!(
            r["kill_unconfirmed"],
            "the kill of the main turn (pid 7000022) started and its result was never recorded"
        );
        // the kill's handling: not confirmed, the write after it fails too - warned, not lost
        let mut sec = Secondary {
            kill_site: Some(site.clone()),
            ..Secondary::default()
        };
        let _ = secondary_kill(
            "timeout after 30 s",
            Some(KillCheck::confirmed(7_000_022)),
            &[],
            "main turn",
            &mut sec,
        );
        assert!(sec.kill_record_lost.is_none());
        let w = sec
            .kill_warnings
            .iter()
            .find(|w| w.starts_with("kill not recorded (main turn): "))
            .expect("the record warning");
        assert!(
            w.contains(&format!(
                "{} keeps the record written before the kill (its tree unknown",
                copy.display()
            )),
            "{w}"
        );
        // the recovery reads it as a record of the task; the record written clean removes it
        assert!(
            crate::liveness::pending::pending_paths_with(&task_dir, Some(&home))
                .paths
                .contains(&copy)
        );
        site.drop_copy();
        assert!(!copy.exists(), "the copy is removed");
        // the copy blocked too (a directory where the file goes): lost, the three errors named
        std::fs::create_dir_all(&copy).unwrap();
        let mut site = KillSite::new(store, pending.clone(), base.clone(), "");
        site.copy = kill_copy_of(Some(&home), &pending, &base);
        match site.hook("main turn")(KillEvent::Before {
            root: 7_000_022,
            tree: &Ok(Vec::new()),
        }) {
            KillRecord::Lost(why) => assert!(
                why.contains("; its fallback ")
                    && why.contains(&format!("; its copy {}: ", copy.display())),
                "{why}"
            ),
            other => panic!("the killer must be told the record is nowhere: {other:?}"),
        }
        assert!(site.armed().copy.is_none());
        let _ = std::fs::remove_dir_all(task_dir.parent().unwrap());
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
