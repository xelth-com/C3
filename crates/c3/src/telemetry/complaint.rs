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
    /// Whether a pending deletion is recorded after this call (the identity kept for a retry).
    pub pending: bool,
}

/// A deletion of this instance the intake has not confirmed yet (`forget-pending.json`): the
/// identity and the proof a retry needs, kept even when the local salt is gone. While it exists
/// nothing is spooled or sent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingDeletion {
    pub instance_id: String,
    pub public_ref: String,
    /// When the deletion was first asked for.
    pub since: String,
    pub attempts: u32,
    pub last_error: String,
}

/// The pending deletion recorded in `dir`, if any.
pub fn pending_deletion_in(dir: &Path) -> Option<PendingDeletion> {
    let text = fs::read_to_string(dir.join(FORGET_PENDING)).ok()?;
    serde_json::from_str(&text).ok()
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

/// `Invoke-TelemetryForget` with C3's pending-deletion record (F02-3): with a public reference
/// the intake is asked FIRST to delete every event and complaint of this instance; a DELETE the
/// intake does not confirm deletes NOTHING here (the salt, the spool, the references stay) and
/// records the pending deletion - the instance id and the reference a retry needs - in
/// `forget-pending.json`, which freezes telemetry (nothing spooled or sent) until a retry is
/// confirmed. `local` removes the local files only after a confirmed DELETE (or, without a
/// reference, after the user confirmed a local-only deletion through `confirm`). `hub`: the
/// resolved intake.
pub fn forget_at(
    hub: &crate::telemetry::Hub,
    dir: &Path,
    req: &ForgetRequest,
    confirm: impl Fn(&str) -> bool,
) -> ForgetOutcome {
    const P: &str = "codex-telemetry";
    let mut out = ForgetOutcome::default();
    let pending = pending_deletion_in(dir);
    let explicit = req
        .public_ref
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(str::to_string);
    let reference = explicit
        .clone()
        .or_else(|| pending.as_ref().map(|p| p.public_ref.clone()));
    let nothing = if req.local {
        "nothing was sent and nothing removed"
    } else {
        "nothing was sent"
    };
    if reference.is_none() && !req.local {
        out.exit = 1;
        out.lines.push(format!("{P}: -Forget needs -PublicRef <ref> (delete the data of this instance at the intake), -Local (remove the local spool and the salt), or both."));
        return out;
    }
    // the identity: the pending deletion's (the one already asked for), else this salt's
    let instance_id = pending
        .as_ref()
        .map(|p| p.instance_id.clone())
        .or_else(|| crate::telemetry::instance_id_if_exists_in(dir))
        .unwrap_or_default();
    out.instance_id = instance_id.clone();
    out.public_ref = reference.clone();
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
        if instance_id.is_empty() {
            out.exit = 1;
            out.lines.push(format!(
                "{P}: this machine has no instance id (no salt {}): the intake holds nothing of it; {nothing}{}.",
                dir.join("salt").display(),
                if req.local { " (-Forget -Local alone removes the local files)" } else { "" }
            ));
            return out;
        }
    } else {
        // (F44-4) -Local alone: the intake keeps what was sent - said, then confirmed
        out.lines.push(format!(
            "{P}: -Local removes the LOCAL data only - the intake still holds what this machine sent{}; to remove it there, run -Forget -PublicRef <ref> BEFORE -Local (c3 telemetry --forget --public-ref <ref>) - the instance id dies with the salt.",
            if instance_id.is_empty() { String::new() } else { format!(" (instance {instance_id})") }
        ));
        if !req.yes && !confirm("remove locally? [y/N] ") {
            out.exit = 1;
            out.lines.push(format!(
                "{P}: nothing removed (no confirmation; -Yes removes without asking)."
            ));
            return out;
        }
    }
    out.confirmed = true;
    // the local deletion holds the sender lock and the spool lock across the DELETE and the
    // removal: no sender posts and no producer appends meanwhile
    let (_sender, _spool) = if req.local {
        let sender = match lock_within(&dir.join(FLUSH_LOCK), Duration::from_secs(10)) {
            Ok(Some(l)) => l,
            _ => {
                out.exit = 1;
                out.lines.push(format!(
                    "{P}: {nothing} - another flush is running (its lock is held); try again when it is done."
                ));
                return out;
            }
        };
        let spool = match lock_within(&dir.join(SPOOL_LOCK), Duration::from_secs(5)) {
            Ok(Some(l)) => l,
            _ => {
                out.exit = 1;
                out.lines.push(format!(
                    "{P}: {nothing} - the spool lock stayed busy for 5 s; try again."
                ));
                return out;
            }
        };
        (Some(sender), Some(spool))
    } else {
        (None, None)
    };
    if let Some(r) = &reference {
        out.server_requested = true;
        out.lines.push(format!(
            "{P}: DELETE {}/v2/instances/{instance_id}?public_ref={r}",
            hub.base
        ));
        match delete_instance(&hub.base, &instance_id, r) {
            Ok(()) => {
                out.server_deleted = true;
                let _ = fs::remove_file(dir.join(FORGET_PENDING));
                out.lines.push(format!(
                    "{P}: the intake deleted the data of instance {instance_id}."
                ));
            }
            Err(why) => {
                // NOT deleted: the identity and the proof are kept for the retry
                let record = PendingDeletion {
                    instance_id: instance_id.clone(),
                    public_ref: r.clone(),
                    since: pending
                        .as_ref()
                        .map(|p| p.since.clone())
                        .unwrap_or_else(now_rfc3339),
                    attempts: pending.as_ref().map(|p| p.attempts).unwrap_or(0) + 1,
                    last_error: why.clone(),
                };
                let recorded = fs::create_dir_all(dir)
                    .map_err(Error::from)
                    .and_then(|()| serde_json::to_string_pretty(&record).map_err(Error::from))
                    .and_then(|t| replace_atomic(&dir.join(FORGET_PENDING), t.as_bytes()));
                out.pending = recorded.is_ok();
                let kept = match &recorded {
                    Ok(()) => format!(
                        "the pending deletion is recorded in {} - c3 forget-me (or this command) retries it with the same instance id and reference; until the intake confirms, nothing is spooled or sent",
                        dir.join(FORGET_PENDING).display()
                    ),
                    Err(e) => format!("the pending deletion could not be recorded ({e}) - run this command again"),
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
    }
    if req.local {
        for name in [
            "salt",
            SPOOL_FILE,
            "refs.ndjson",
            "not-spooled.ndjson",
            LAST_FLUSH,
        ] {
            let p = dir.join(name);
            if p.exists() {
                match fs::remove_file(&p) {
                    Ok(()) => out.removed.push(name.to_string()),
                    Err(e) => {
                        out.exit = 1;
                        out.lines.push(format!(
                            "{P}: the local deletion did not finish ({}: {e}){}; run it again to finish it.",
                            p.display(),
                            if out.removed.is_empty() { String::new() } else { format!("; removed so far: {}", out.removed.join(", ")) }
                        ));
                        return out;
                    }
                }
            }
        }
        out.lines.push(format!(
            "{P}: removed locally - {}; the next event makes a new instance id.",
            if out.removed.is_empty() {
                "nothing (there was no spool and no salt)".to_string()
            } else {
                out.removed.join(", ")
            }
        ));
        if let Some(p) = pending_deletion_in(dir) {
            out.pending = true;
            out.lines.push(format!("{P}: the pending deletion of instance {} at the intake is kept ({}): c3 forget-me retries it; until the intake confirms, nothing is spooled or sent.", p.instance_id, dir.join(FORGET_PENDING).display()));
        }
    }
    out.exit = 0;
    out
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
