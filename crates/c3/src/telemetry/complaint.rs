//! Complaints and forgetting (README "The complaint", "Delete my data").
//!
//! A complaint is shown to the user in full before it is sent (rule 3): [`complain`]
//! builds the exact payload, passes its pretty JSON to a `confirm` closure, and only
//! sends on a `true`. Every public reference the hub returns is stored under
//! `refs.ndjson` so [`forget`] can prove ownership of the instance when it asks the
//! hub to erase everything this installation ever sent.
//!
//! (wave 2b, F02-3) A DELETE the intake does not confirm deletes NOTHING locally: the salt (the
//! instance id), the spool and the references stay, and the pending deletion - the instance id and
//! the reference a retry needs - is recorded in `forget-pending.json` until a retry is confirmed;
//! meanwhile nothing is spooled or sent. The local files go only after a confirmed DELETE (or on an
//! explicit local-only deletion).
//!
//! (wave 2d, F09-2, F09-3) `forget-pending.json` is a deletion TRANSACTION: written `pending`
//! BEFORE the intake is asked, `confirmed` when it confirmed, `cleaning` when the local cleanup
//! starts, removed only after the cleanup finished (queued data first, the references, the salt
//! last). Every write of it holds the sender lock and the spool lock, and the sender decides on it
//! under the same two; a `confirmed`/`cleaning` record left by an interrupted cleanup blocks
//! spooling and sending and is finished - with its own identity - by the next flush or forget.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::telemetry::event::APP_VERSION;
use crate::telemetry::spool::{
    lock_within, replace_atomic, FLUSH_LOCK, FORGET_PENDING, LAST_FLUSH, SPOOL_FILE, SPOOL_LOCK,
};
use crate::telemetry::{default_hub, instance_id_in, telemetry_dir, Error, Result};

/// Complaint text cap (README: `≤ 8 KiB`).
const MAX_TEXT: usize = 8 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Build the complaint payload, show it through `confirm`, and — only if confirmed — send
/// it. Returns the public reference (`T-XXXX-XXXX`) to quote on the forum, or `None` when
/// the user declined. Uses the default hub and config directory.
pub fn complain(
    text: &str,
    last_run: Option<&str>,
    confirm: impl Fn(&str) -> bool,
) -> Result<Option<String>> {
    complain_to(&default_hub(), &telemetry_dir(), text, last_run, confirm)
}

/// [`complain`] against an explicit hub base and directory (for tests).
pub fn complain_to(
    hub: &str,
    dir: &Path,
    text: &str,
    last_run: Option<&str>,
    confirm: impl Fn(&str) -> bool,
) -> Result<Option<String>> {
    let text: String = text.chars().take(MAX_TEXT).collect();
    let mut context = serde_json::Map::new();
    if let Some(s) = last_run {
        context.insert("last_run".to_string(), json!(s));
    }
    let body = json!({
        "app_id": "c3",
        "app_version": APP_VERSION,
        "instance_id": instance_id_in(dir),
        "text": text,
        "context": Value::Object(context),
    });
    let pretty = serde_json::to_string_pretty(&body)?;
    if !confirm(&pretty) {
        return Ok(None);
    }
    let url = format!("{}/v2/complaints", hub.trim_end_matches('/'));
    let agent = ureq::AgentBuilder::new().timeout(REQUEST_TIMEOUT).build();
    let resp = agent
        .post(&url)
        .send_json(body)
        .map_err(|e| Error::new(format!("complaint POST failed: {e}")))?;
    let value: Value = resp
        .into_json()
        .map_err(|e| Error::new(format!("complaint reply was not JSON: {e}")))?;
    let reference = value
        .get("public_ref")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    if let Some(r) = &reference {
        store_ref(dir, r)?;
    }
    Ok(reference)
}

/// What a forget asks for (`codex-telemetry.ps1 -Forget`): the intake's deletion of this
/// instance (proved by `public_ref`), the local deletion (`local`), or both - then the intake
/// FIRST and the local files only after it confirmed.
#[derive(Debug, Clone, Default)]
pub struct ForgetRequest {
    /// The public reference a delivered complaint printed (the intake's proof of ownership);
    /// `None`: the pending deletion's, when one is recorded.
    pub public_ref: Option<String>,
    /// Remove the local salt, spool, references and counters.
    pub local: bool,
    /// Remove without asking (`-Local` alone asks).
    pub yes: bool,
}

/// The outcome of a forget, for the CLI to report.
#[derive(Debug, Clone, Default)]
pub struct ForgetOutcome {
    /// The exit code: 0 done, 1 refused or not confirmed, 3 the intake did not confirm the
    /// deletion (nothing deleted; the pending deletion recorded for the retry).
    pub exit: i32,
    /// The lines to print, in order.
    pub lines: Vec<String>,
    /// Whether the user confirmed (a `-Local` alone without `-Yes` asks).
    pub confirmed: bool,
    /// Whether a server DELETE was attempted.
    pub server_requested: bool,
    /// Whether the intake confirmed the DELETE.
    pub server_deleted: bool,
    /// The instance id the DELETE named (the pending deletion's identity when one is recorded).
    pub instance_id: String,
    /// The public reference used as proof of ownership, if any.
    pub public_ref: Option<String>,
    /// The local files removed.
    pub removed: Vec<String>,
    /// Whether a deletion transaction is recorded after this call - pending (the identity kept for
    /// a retry) or a cleanup that did not finish (it blocks spooling and sending until finished).
    pub pending: bool,
}

/// A deletion transaction's phase (`forget-pending.json`, F09-3): asked for and not confirmed (the
/// DELETE in flight, failed, or its process gone).
pub const PHASE_PENDING: &str = "pending";
/// The intake confirmed the DELETE; the local cleanup has not started.
pub const PHASE_CONFIRMED: &str = "confirmed";
/// The local cleanup started (a local-only deletion starts here); some files may be gone.
pub const PHASE_CLEANING: &str = "cleaning";

fn phase_pending() -> String {
    PHASE_PENDING.to_string()
}

/// The deletion transaction of this instance (`forget-pending.json`): the identity and the proof a
/// retry needs, kept even when the local salt is gone, and its phase. (F09-3) It is written BEFORE
/// the intake is asked (`pending`), moved to `confirmed` when the intake confirmed, to `cleaning`
/// when the local cleanup starts, and removed only AFTER the cleanup finished - so a process that
/// dies anywhere in between leaves a record that blocks spooling and sending, and the next flush or
/// forget resumes it with the SAME identity. While it exists - in any phase - nothing is spooled
/// or sent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingDeletion {
    pub instance_id: String,
    /// The proof the intake asked for (`""` for a local-only deletion).
    pub public_ref: String,
    /// When the deletion was first asked for.
    pub since: String,
    pub attempts: u32,
    pub last_error: String,
    /// [`PHASE_PENDING`], [`PHASE_CONFIRMED`] or [`PHASE_CLEANING`]; a record written before wave
    /// 2d has none: pending.
    #[serde(default = "phase_pending")]
    pub phase: String,
}

impl PendingDeletion {
    /// Whether only the local cleanup is left (the intake confirmed, or a local-only deletion).
    pub fn cleanup_due(&self) -> bool {
        self.phase == PHASE_CONFIRMED || self.phase == PHASE_CLEANING
    }
}

/// The deletion transaction recorded in `dir`, if any (`None` also for a record that does not
/// parse - [`deletion_state`] counts that one as pending).
pub fn pending_deletion_in(dir: &Path) -> Option<PendingDeletion> {
    let text = fs::read_to_string(dir.join(FORGET_PENDING)).ok()?;
    serde_json::from_str(&text).ok()
}

/// What the deletion transaction asks of a producer and of the sender.
pub(crate) enum DeletionState {
    /// No transaction: spool and send.
    None,
    /// Pending at the intake - or a record that cannot be read (fail closed): spool nothing, send
    /// nothing.
    Pending,
    /// Confirmed (or local-only), the local cleanup not finished: spool nothing, send nothing; the
    /// sender finishes the cleanup with this record's identity.
    Cleanup(PendingDeletion),
}

/// The deletion state of `dir`. Decisions are taken on it under the sender lock and the spool lock
/// (F09-2); every write of the record holds both.
pub(crate) fn deletion_state(dir: &Path) -> DeletionState {
    match fs::read_to_string(dir.join(FORGET_PENDING)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DeletionState::None,
        Err(_) => DeletionState::Pending,
        Ok(t) => match serde_json::from_str::<PendingDeletion>(&t) {
            Ok(r) if r.cleanup_due() => DeletionState::Cleanup(r),
            _ => DeletionState::Pending,
        },
    }
}

/// Write the transaction record (atomically). The caller holds the sender lock and the spool lock.
fn write_transaction(dir: &Path, record: &PendingDeletion) -> Result<()> {
    fs::create_dir_all(dir)?;
    let text = serde_json::to_string_pretty(record)?;
    replace_atomic(&dir.join(FORGET_PENDING), text.as_bytes())
}

/// What a local cleanup removed, and where it stopped.
#[derive(Debug, Default)]
pub(crate) struct Cleanup {
    /// The files removed, in order.
    pub removed: Vec<String>,
    /// A salt that names another instance (made after the deletion began): kept, said here.
    pub kept: Option<String>,
    /// Why the cleanup stopped (`None`: it finished - with `own_record`, the record is gone).
    pub error: Option<String>,
}

/// The local cleanup of the instance `txn` names, run while the caller holds the sender lock and
/// the spool lock (F09-3). The QUEUED data goes first - the spool, a rewrite's leftover temporary
/// copies, the not-spooled counts, the last flush -, then the proof (the references), the identity
/// (the salt) LAST, and only when it is still `txn`'s instance (a salt made after the deletion began
/// is another instance and stays). `own_record`: the transaction is this cleanup's (confirmed, or a
/// local-only deletion) - it is set to `cleaning` before the first removal and removed after the
/// last, so an interruption leaves a record that blocks spooling and sending and that the next flush
/// or forget resumes (a `cleaning` `txn` is on disk already: a resumed cleanup's, or a local-only
/// deletion's, which its forget records before calling here - F14-1); `false`: a PENDING
/// deletion's record (the retry's identity) is left as it is.
/// `remove` removes one file (tests inject a failure).
pub(crate) fn run_local_cleanup(
    dir: &Path,
    txn: &PendingDeletion,
    own_record: bool,
    remove: &dyn Fn(&Path) -> std::io::Result<()>,
) -> Cleanup {
    let mut c = Cleanup::default();
    if own_record && txn.phase != PHASE_CLEANING {
        let mut cleaning = txn.clone();
        cleaning.phase = PHASE_CLEANING.to_string();
        if let Err(e) = write_transaction(dir, &cleaning) {
            c.error = Some(format!(
                "the deletion record could not be moved to cleaning ({e})"
            ));
            return c;
        }
    }
    let mut targets: Vec<PathBuf> = vec![dir.join(SPOOL_FILE)];
    // a rewrite that died before its rename left a temporary copy of the spool beside it
    let spool_tmp = format!(".{SPOOL_FILE}.");
    if let Ok(rd) = fs::read_dir(dir) {
        let mut tmps: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(&spool_tmp) && n.ends_with(".tmp"))
            })
            .collect();
        tmps.sort();
        targets.extend(tmps);
    }
    targets.extend(
        ["not-spooled.ndjson", LAST_FLUSH, "refs.ndjson"]
            .iter()
            .map(|n| dir.join(n)),
    );
    for p in targets {
        if !p.exists() {
            continue;
        }
        if let Err(e) = remove(&p) {
            c.error = Some(format!(
                "{}: {}",
                p.display(),
                c3_core::one_line(&e.to_string())
            ));
            return c;
        }
        c.removed.push(
            p.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string(),
        );
    }
    let salt = dir.join("salt");
    if salt.exists() {
        let current = crate::telemetry::instance_id_if_exists_in(dir).unwrap_or_default();
        if current == txn.instance_id {
            if let Err(e) = remove(&salt) {
                c.error = Some(format!(
                    "{}: {}",
                    salt.display(),
                    c3_core::one_line(&e.to_string())
                ));
                return c;
            }
            c.removed.push("salt".to_string());
        } else {
            c.kept = Some(format!(
                "the salt names another instance ({}) made after the deletion of {} began: kept",
                if current.is_empty() {
                    "unreadable"
                } else {
                    current.as_str()
                },
                if txn.instance_id.is_empty() {
                    "this machine's data"
                } else {
                    txn.instance_id.as_str()
                }
            ));
        }
    }
    if own_record {
        match fs::remove_file(dir.join(FORGET_PENDING)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                c.error = Some(format!(
                    "the deletion record {} could not be removed ({})",
                    dir.join(FORGET_PENDING).display(),
                    c3_core::one_line(&e.to_string())
                ));
            }
        }
    }
    c
}

/// `^[A-Za-z0-9._-]{1,128}$`.
fn is_public_ref(r: &str) -> bool {
    !r.is_empty()
        && r.len() <= 128
        && r.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// The intake's DELETE of an instance: `Ok(())` when it confirmed (a 2xx answer that is not a
/// JSON object with `ok: false`), else why not (`HTTP 404: unknown public_ref`, the transport
/// error).
fn delete_instance(
    base: &str,
    instance_id: &str,
    public_ref: &str,
) -> std::result::Result<(), String> {
    let url = format!(
        "{}/v2/instances/{instance_id}?public_ref={public_ref}",
        base.trim_end_matches('/')
    );
    let agent = ureq::AgentBuilder::new().timeout(REQUEST_TIMEOUT).build();
    let err_of = |code: u16, body: String| {
        let why = serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
            .filter(|s| !s.is_empty());
        match why {
            Some(w) => format!("HTTP {code}: {}", c3_core::one_line(&w)),
            None => format!("HTTP {code}"),
        }
    };
    match agent.request("DELETE", &url).call() {
        Ok(resp) => {
            let code = resp.status();
            let body = resp.into_string().unwrap_or_default();
            match serde_json::from_str::<Value>(&body) {
                Ok(v) if v.get("ok") == Some(&Value::Bool(false)) => Err(err_of(code, body)),
                _ => Ok(()),
            }
        }
        Err(ureq::Error::Status(code, resp)) => {
            Err(err_of(code, resp.into_string().unwrap_or_default()))
        }
        Err(e) => Err(format!(
            "the intake could not be reached: {}",
            c3_core::one_line(&e.to_string())
        )),
    }
}

/// Forget with the default hub and directory ([`forget_at`]).
pub fn forget(req: &ForgetRequest, confirm: impl Fn(&str) -> bool) -> ForgetOutcome {
    forget_at(&crate::telemetry::hub(), &telemetry_dir(), req, confirm)
}

/// `Invoke-TelemetryForget` with C3's deletion transaction (F02-3, F09-2, F09-3): with a public
/// reference the intake is asked FIRST to delete every event and complaint of this instance; a
/// DELETE the intake does not confirm deletes NOTHING here (the salt, the spool, the references
/// stay) and leaves the transaction `pending` - the instance id and the reference a retry needs -
/// in `forget-pending.json`, which freezes telemetry (nothing spooled or sent) until a retry is
/// confirmed. `local` removes the local files only after a confirmed DELETE (or, without a
/// reference, after the user confirmed a local-only deletion through `confirm`). A transaction
/// whose cleanup did not finish is finished first. `hub`: the resolved intake.
///
/// Every forget holds the sender lock and the spool lock from its first write of the transaction
/// to its last (F09-2): the record is written BEFORE the DELETE, so a sender never passes its check
/// while a deletion is under way and a crash during the DELETE leaves a pending deletion (F09-3).
pub fn forget_at(
    hub: &crate::telemetry::Hub,
    dir: &Path,
    req: &ForgetRequest,
    confirm: impl Fn(&str) -> bool,
) -> ForgetOutcome {
    forget_with(hub, dir, req, confirm, &|p| fs::remove_file(p))
}

/// [`forget_at`] with an injected file removal (tests interrupt the local cleanup with it).
#[doc(hidden)]
pub fn forget_with(
    hub: &crate::telemetry::Hub,
    dir: &Path,
    req: &ForgetRequest,
    confirm: impl Fn(&str) -> bool,
    remove: &dyn Fn(&Path) -> std::io::Result<()>,
) -> ForgetOutcome {
    const P: &str = "codex-telemetry";
    let mut out = ForgetOutcome::default();
    // read without the locks for the request's checks and the question; decided again under them
    let seen = pending_deletion_in(dir);
    let resume = seen.as_ref().is_some_and(|p| p.cleanup_due());
    let explicit = req
        .public_ref
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(str::to_string);
    let pending_ref = |p: &Option<PendingDeletion>| {
        p.as_ref()
            .filter(|p| p.phase == PHASE_PENDING && !p.public_ref.is_empty())
            .map(|p| p.public_ref.clone())
    };
    let reference = explicit.clone().or_else(|| pending_ref(&seen));
    let nothing = if req.local {
        "nothing was sent and nothing removed"
    } else {
        "nothing was sent"
    };
    if !resume {
        if reference.is_none() && !req.local {
            out.exit = 1;
            out.lines.push(format!("{P}: -Forget needs -PublicRef <ref> (delete the data of this instance at the intake), -Local (remove the local spool and the salt), or both."));
            return out;
        }
        if let Some(r) = &reference {
            if !is_public_ref(r) {
                out.exit = 1;
                out.lines.push(format!("{P}: -PublicRef '{r}' is not a public_ref (letters, digits, dot, dash, underscore); {nothing}."));
                return out;
            }
            if !hub.error.is_empty() {
                out.exit = 1;
                out.lines.push(format!("{P}: {}; {nothing}.", hub.error));
                return out;
            }
        } else {
            // (F44-4) -Local alone: the intake keeps what was sent - said, then confirmed
            let id = seen
                .as_ref()
                .map(|p| p.instance_id.clone())
                .or_else(|| crate::telemetry::instance_id_if_exists_in(dir))
                .unwrap_or_default();
            out.lines.push(format!(
                "{P}: -Local removes the LOCAL data only - the intake still holds what this machine sent{}; to remove it there, run -Forget -PublicRef <ref> BEFORE -Local (c3 telemetry --forget --public-ref <ref>) - the instance id dies with the salt.",
                if id.is_empty() { String::new() } else { format!(" (instance {id})") }
            ));
            if !req.yes && !confirm("remove locally? [y/N] ") {
                out.exit = 1;
                out.lines.push(format!(
                    "{P}: nothing removed (no confirmation; -Yes removes without asking)."
                ));
                return out;
            }
        }
    }
    out.confirmed = true;
    // (F09-2) the sender lock and the spool lock for the whole forget: no sender passes its
    // deletion check and no producer appends while the transaction is written, the intake asked
    // and the local files removed
    let Ok(Some(_sender)) = lock_within(&dir.join(FLUSH_LOCK), Duration::from_secs(10)) else {
        out.exit = 1;
        out.lines.push(format!(
            "{P}: {nothing} - another flush is running (its lock is held); try again when it is done."
        ));
        return out;
    };
    let Ok(Some(_spool)) = lock_within(&dir.join(SPOOL_LOCK), Duration::from_secs(5)) else {
        out.exit = 1;
        out.lines.push(format!(
            "{P}: {nothing} - the spool lock stayed busy for 5 s; try again."
        ));
        return out;
    };
    // the transaction as it is NOW
    let current = pending_deletion_in(dir);
    if let Some(txn) = current.as_ref().filter(|t| t.cleanup_due()) {
        // (F09-3) a confirmed deletion whose local cleanup did not finish: only that is left
        out.instance_id = txn.instance_id.clone();
        out.public_ref = Some(txn.public_ref.clone()).filter(|r| !r.is_empty());
        out.lines.push(format!(
            "{P}: finishing the local deletion of {} ({}) - nothing more is asked of the intake.",
            instance_label(&txn.instance_id),
            if txn.public_ref.is_empty() {
                "a local-only deletion that did not finish".to_string()
            } else {
                format!("the intake confirmed its deletion; the local cleanup did not finish (phase {})", txn.phase)
            }
        ));
        finish_cleanup(&mut out, dir, txn, true, remove);
        return out;
    }
    if resume {
        // finished by another process meanwhile
        out.exit = 0;
        out.lines.push(format!(
            "{P}: the local deletion was finished meanwhile (by a flush or another forget); nothing left to do."
        ));
        return out;
    }
    // the identity: the pending deletion's (the one already asked for), else this salt's
    let reference = explicit.or_else(|| pending_ref(&current));
    let instance_id = current
        .as_ref()
        .map(|p| p.instance_id.clone())
        .or_else(|| crate::telemetry::instance_id_if_exists_in(dir))
        .unwrap_or_default();
    out.instance_id = instance_id.clone();
    out.public_ref = reference.clone();
    let Some(r) = reference else {
        // -Local alone: a pending deletion keeps its record (the retry's identity); otherwise the
        // cleanup is its own local-only transaction
        match current {
            Some(p) => {
                finish_cleanup(&mut out, dir, &p, false, remove);
                if pending_deletion_in(dir).is_some() {
                    out.pending = true;
                    out.lines.push(format!("{P}: the pending deletion of instance {} at the intake is kept ({}): c3 forget-me retries it; until the intake confirms, nothing is spooled or sent.", p.instance_id, dir.join(FORGET_PENDING).display()));
                }
            }
            None => {
                // (F14-1) the local-only transaction is RECORDED, `cleaning`, before the first
                // removal: an interruption leaves the record that blocks spooling and sending and
                // that the next flush or forget resumes with this identity
                let txn = PendingDeletion {
                    instance_id,
                    public_ref: String::new(),
                    since: now_rfc3339(),
                    attempts: 0,
                    last_error: String::new(),
                    phase: PHASE_CLEANING.to_string(),
                };
                if let Err(e) = write_transaction(dir, &txn) {
                    out.exit = 1;
                    out.lines.push(format!(
                        "{P}: {nothing} - the local deletion could not be recorded before its cleanup ({e})."
                    ));
                    return out;
                }
                finish_cleanup(&mut out, dir, &txn, true, remove);
            }
        }
        return out;
    };
    if instance_id.is_empty() {
        out.exit = 1;
        out.lines.push(format!(
            "{P}: this machine has no instance id (no salt {}): the intake holds nothing of it; {nothing}{}.",
            dir.join("salt").display(),
            if req.local { " (-Forget -Local alone removes the local files)" } else { "" }
        ));
        return out;
    }
    // (F09-3) the transaction BEFORE the remote call: a process that dies during or after the DELETE
    // leaves a pending deletion that blocks spooling and sending until a retry is confirmed
    let mut txn = PendingDeletion {
        instance_id: instance_id.clone(),
        public_ref: r.clone(),
        since: current
            .as_ref()
            .map(|p| p.since.clone())
            .unwrap_or_else(now_rfc3339),
        attempts: current.as_ref().map(|p| p.attempts).unwrap_or(0) + 1,
        last_error: current
            .as_ref()
            .map(|p| p.last_error.clone())
            .unwrap_or_default(),
        phase: PHASE_PENDING.to_string(),
    };
    if let Err(e) = write_transaction(dir, &txn) {
        out.exit = 1;
        out.lines.push(format!(
            "{P}: {nothing} - the deletion could not be recorded before asking the intake ({e})."
        ));
        return out;
    }
    out.pending = true;
    out.server_requested = true;
    out.lines.push(format!(
        "{P}: DELETE {}/v2/instances/{instance_id}?public_ref={r}",
        hub.base
    ));
    match delete_instance(&hub.base, &instance_id, &r) {
        Ok(()) => {
            out.server_deleted = true;
            out.lines.push(format!(
                "{P}: the intake deleted the data of instance {instance_id}."
            ));
        }
        Err(why) => {
            // NOT deleted: the identity and the proof are kept for the retry
            txn.last_error = why.clone();
            let recorded = write_transaction(dir, &txn);
            let kept = match &recorded {
                Ok(()) => format!(
                    "the pending deletion is recorded in {} - c3 forget-me (or this command) retries it with the same instance id and reference; until the intake confirms, nothing is spooled or sent",
                    dir.join(FORGET_PENDING).display()
                ),
                Err(e) => format!("the pending deletion is recorded in {} (its last error could not be added: {e}) - c3 forget-me retries it; until the intake confirms, nothing is spooled or sent", dir.join(FORGET_PENDING).display()),
            };
            out.exit = 3;
            if req.local {
                out.lines.push(format!("{P}: the intake did not confirm the deletion ({why}); NOTHING was deleted - not there and not here: the salt (instance {instance_id}), the spool and the references are kept; {kept}."));
            } else {
                out.lines.push(format!("{P}: the intake did not confirm the deletion ({why}); nothing is deleted there - try again later (with the right -PublicRef); {kept}."));
            }
            return out;
        }
    }
    if !req.local {
        // the intake's part was all that was asked: the transaction is complete
        match fs::remove_file(dir.join(FORGET_PENDING)) {
            Ok(()) => out.pending = false,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => out.pending = false,
            Err(e) => {
                out.exit = 1;
                out.lines.push(format!(
                    "{P}: the deletion record {} could not be removed ({}): nothing is spooled or sent until it is - run this command again.",
                    dir.join(FORGET_PENDING).display(),
                    c3_core::one_line(&e.to_string())
                ));
                return out;
            }
        }
        out.exit = 0;
        return out;
    }
    // confirmed: recorded so, then the local cleanup under the same locks
    txn.phase = PHASE_CONFIRMED.to_string();
    if let Err(e) = write_transaction(dir, &txn) {
        out.exit = 1;
        out.lines.push(format!("{P}: the confirmation could not be recorded ({e}); nothing local was removed - the pending deletion blocks spooling and sending; run this command again."));
        return out;
    }
    finish_cleanup(&mut out, dir, &txn, true, remove);
    out
}

/// Run [`run_local_cleanup`] for a forget and report it in `out` (exit 0 when it finished, else 1
/// with where it stopped and how it resumes).
fn finish_cleanup(
    out: &mut ForgetOutcome,
    dir: &Path,
    txn: &PendingDeletion,
    own_record: bool,
    remove: &dyn Fn(&Path) -> std::io::Result<()>,
) {
    const P: &str = "codex-telemetry";
    let c = run_local_cleanup(dir, txn, own_record, remove);
    out.removed = c.removed.clone();
    out.pending = pending_deletion_in(dir).is_some();
    if let Some(e) = &c.error {
        out.exit = 1;
        let so_far = if c.removed.is_empty() {
            String::new()
        } else {
            format!("; removed so far: {}", c.removed.join(", "))
        };
        // (F14-1) the record is said only when it is there (in any form it blocks spooling and
        // sending); a cleanup that kept none says so
        if own_record && dir.join(FORGET_PENDING).exists() {
            out.lines.push(format!(
                "{P}: the local deletion did not finish ({e}){so_far}; the deletion record {} {} - nothing is spooled or sent until the next flush or c3 forget-me finishes it.",
                dir.join(FORGET_PENDING).display(),
                if txn.instance_id.is_empty() {
                    "stays (begun without a salt, it names no instance)".to_string()
                } else {
                    format!("keeps instance {}", txn.instance_id)
                }
            ));
        } else if own_record {
            out.lines.push(format!(
                "{P}: the local deletion did not finish ({e}){so_far}; no deletion record is kept ({} is gone), so nothing blocks spooling and sending - run c3 forget-me --local again to finish it.",
                dir.join(FORGET_PENDING).display()
            ));
        } else {
            out.lines.push(format!(
                "{P}: the local deletion did not finish ({e}){so_far}; run it again to finish it."
            ));
        }
        return;
    }
    out.exit = 0;
    out.lines.push(format!(
        "{P}: removed locally - {}; the next event makes a new instance id.",
        if c.removed.is_empty() {
            "nothing (there was no spool and no salt)".to_string()
        } else {
            c.removed.join(", ")
        }
    ));
    if let Some(k) = &c.kept {
        out.lines.push(format!("{P}: {k}."));
    }
}

/// `instance <id>`, or `this machine's data` for a deletion begun without an instance id (no salt).
fn instance_label(instance_id: &str) -> String {
    if instance_id.is_empty() {
        "this machine's data".to_string()
    } else {
        format!("instance {instance_id}")
    }
}

/// Append a received public reference to `refs.ndjson`.
fn store_ref(dir: &Path, reference: &str) -> Result<()> {
    fs::create_dir_all(dir)?;
    let line = json!({ "public_ref": reference, "when": now_rfc3339() }).to_string();
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(refs_path(dir))?;
    writeln!(f, "{line}")?;
    Ok(())
}

/// The newest stored public reference, if any.
pub fn newest_ref(dir: &Path) -> Option<String> {
    let text = fs::read_to_string(refs_path(dir)).ok()?;
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter_map(|v| {
            v.get("public_ref")
                .and_then(|r| r.as_str())
                .map(str::to_string)
        })
        .next_back()
}

fn refs_path(dir: &Path) -> PathBuf {
    dir.join("refs.ndjson")
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}
