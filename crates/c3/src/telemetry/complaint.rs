//! Complaints and `forget-me` (README "The complaint", "Delete my data").
//!
//! A complaint is shown to the user in full before it is sent (rule 3): [`complain`]
//! builds the exact payload, passes its pretty JSON to a `confirm` closure, and only
//! sends on a `true`. Every public reference the hub returns is stored under
//! `refs.ndjson` so [`forget_me`] can prove ownership of the instance when it asks the
//! hub to erase everything this installation ever sent.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

use crate::telemetry::event::APP_VERSION;
use crate::telemetry::{debug_log, default_hub, instance_id_in, telemetry_dir, Error, Result};

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

/// The outcome of a [`forget_me`], for the CLI to report.
#[derive(Debug, Clone, Default)]
pub struct ForgetOutcome {
    /// Whether the user confirmed the deletion.
    pub confirmed: bool,
    /// Whether a server DELETE was attempted (a public reference was stored).
    pub server_requested: bool,
    /// Whether the server accepted the DELETE.
    pub server_deleted: bool,
    /// The instance id that was erased.
    pub instance_id: String,
    /// The public reference used as proof of ownership, if any.
    pub public_ref: Option<String>,
}

/// Describe and, on `confirm`, perform a full erase: ask the hub to delete every event and
/// complaint of this instance (proved by the newest stored public reference), then remove
/// the local salt, spool and refs. Uses the default hub and directory.
pub fn forget_me(confirm: impl Fn(&str) -> bool) -> Result<ForgetOutcome> {
    forget_me_at(&default_hub(), &telemetry_dir(), confirm)
}

/// [`forget_me`] against an explicit hub base and directory (for tests).
pub fn forget_me_at(
    hub: &str,
    dir: &Path,
    confirm: impl Fn(&str) -> bool,
) -> Result<ForgetOutcome> {
    let instance_id = instance_id_in(dir);
    let public_ref = newest_ref(dir);
    let plan = match &public_ref {
        Some(r) => format!(
            "This deletes the local telemetry salt, spool and complaint references under {}, and asks {} to erase every event and complaint of instance {} (proof: {}).",
            dir.display(),
            hub.trim_end_matches('/'),
            instance_id,
            r
        ),
        None => format!(
            "This deletes the local telemetry salt, spool and complaint references under {}. No complaint reference is stored, so the server cannot verify ownership; only local data is removed (instance {}).",
            dir.display(),
            instance_id
        ),
    };
    if !confirm(&plan) {
        return Ok(ForgetOutcome {
            confirmed: false,
            instance_id,
            public_ref,
            ..Default::default()
        });
    }
    let mut server_deleted = false;
    if let Some(r) = &public_ref {
        let url = format!(
            "{}/v2/instances/{}?public_ref={}",
            hub.trim_end_matches('/'),
            instance_id,
            r
        );
        let agent = ureq::AgentBuilder::new().timeout(REQUEST_TIMEOUT).build();
        match agent.request("DELETE", &url).call() {
            Ok(_) => server_deleted = true,
            Err(e) => debug_log(&format!("forget-me DELETE failed: {e}")),
        }
    }
    for name in ["salt", "spool.ndjson", "refs.ndjson"] {
        let _ = fs::remove_file(dir.join(name));
    }
    Ok(ForgetOutcome {
        confirmed: true,
        server_requested: public_ref.is_some(),
        server_deleted,
        instance_id,
        public_ref,
    })
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
fn newest_ref(dir: &Path) -> Option<String> {
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
