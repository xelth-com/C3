//! The preflight verdict (`Get-PreflightVerdict`): the availability decision for one
//! identity from local checks (credentials, then the endpoint's recorded health).
//!
//! Split in two so the runtime never runs a credential check for an identity that is
//! already ruled out: [`verdict_pre_credential`] returns the identity-error /
//! unresolved verdict (no credential needed), and [`verdict_with_credential`] takes
//! the credential result and applies the health rules.

use chrono::{DateTime, FixedOffset};

use crate::credential::{CredentialResult, State};
use crate::health::{format_offset_iso, EndpointHealth};
use crate::lineage::ReviewerIdentity;

#[derive(Debug, Clone)]
pub struct PreflightVerdict {
    /// `available` | `unavailable` | `unknown`.
    pub state: String,
    pub preflight: String,
    pub reason: String,
    pub refusal: String,
    pub label: String,
    /// `` | identity | unresolved | credentials | auth | quota | quota-unknown-reset | unknown.
    pub kind: String,
    pub hit: Option<DateTime<FixedOffset>>,
    pub until: Option<DateTime<FixedOffset>>,
    pub credential: Option<CredentialResult>,
}

impl PreflightVerdict {
    fn available() -> Self {
        PreflightVerdict {
            state: "available".into(),
            preflight: String::new(),
            reason: String::new(),
            refusal: String::new(),
            label: String::new(),
            kind: String::new(),
            hit: None,
            until: None,
            credential: None,
        }
    }
}

/// The identity-error / unresolved early returns; `None` means proceed to credentials.
pub fn verdict_pre_credential(id: &ReviewerIdentity) -> Option<PreflightVerdict> {
    if !id.error.is_empty() {
        let mut v = PreflightVerdict::available();
        v.state = "unavailable".into();
        v.kind = "identity".into();
        v.reason = id.error.clone();
        v.preflight = format!("unavailable: {}", id.error);
        v.refusal = id.error.clone();
        v.label = format!("unavailable ({}) - a real run is refused", id.error);
        return Some(v);
    }
    if !id.resolved {
        let reason = format!("reviewer identity unresolved: {}", id.note);
        let mut v = PreflightVerdict::available();
        v.state = "unknown".into();
        v.kind = "unresolved".into();
        v.preflight = format!("unknown: {reason}");
        v.reason = v.preflight.clone();
        v.refusal = format!(
            "provider {}: availability could not be established ({reason}); pass -SkipPreflight to launch anyway, or fix the check",
            id.provider
        );
        v.label = format!("unknown ({reason}) - a real run is refused: {}", v.refusal);
        return Some(v);
    }
    None
}

/// The credential + health branches of `Get-PreflightVerdict`.
pub fn verdict_with_credential(
    id: &ReviewerIdentity,
    health: Option<&EndpointHealth>,
    cred: CredentialResult,
    roster_walk: bool,
) -> PreflightVerdict {
    let p = &id.provider;
    let mut v = PreflightVerdict::available();
    v.preflight = cred.detail.clone();
    v.reason = cred.detail.clone();
    let auth = health.and_then(|h| h.auth.as_ref());
    let quota = health.and_then(|h| h.quota.as_ref());
    let quota_known = health.map(|h| h.quota_known).unwrap_or(false);

    if cred.state == State::Missing {
        v.state = "unavailable".into();
        v.kind = "credentials".into();
        v.refusal = format!(
            "provider {p} is not usable: {}; nothing was started (run codex-providers.ps1 for the full picture)",
            cred.reason
        );
        v.label = format!(
            "unavailable ({}) - a real run is refused: {}",
            cred.reason, v.refusal
        );
    } else if let Some(a) = auth {
        v.state = "unavailable".into();
        v.kind = "auth".into();
        v.hit = Some(a.hit);
        v.reason = format!("auth failed {}: {}", a.when, a.message);
        v.preflight = format!("unavailable: {}", v.reason);
        v.refusal = format!(
            "provider {p} is not usable: the last run on this endpoint was rejected as unauthenticated at {} ({}); if you rotated the credential, pass -SkipPreflight once",
            a.when, a.message
        );
        v.label = format!(
            "unavailable ({}) - a real run is refused: {}",
            v.reason, v.refusal
        );
    } else if let Some(q) = quota.filter(|_| quota_known) {
        v.state = "unavailable".into();
        v.kind = "quota".into();
        v.hit = Some(q.hit);
        v.until = q.retry_after;
        v.reason = format!("usage limit until {}", q.retry_after_iso);
        v.preflight = format!("unavailable: {}", v.reason);
        v.refusal = format!(
            "provider {p} is not usable: its usage limit (hit at {}: {}) lasts until {}; nothing was started (pass -SkipPreflight to launch anyway)",
            q.when, q.message, q.retry_after_iso
        );
        v.label = format!(
            "unavailable ({}) - a real run is refused: {}",
            v.reason, v.refusal
        );
    } else if let Some(q) = quota {
        let until_iso = format_offset_iso(q.until);
        // A reset-less burst 429 (`kind == "burst"`) is out for 10 minutes and names the 429
        // in the parenthetical; a usage-limit-without-reset is out for 60 (wave 24c).
        let is_burst = q.kind == "burst";
        let limit_word = if is_burst { "burst" } else { "usage" };
        let burst_clause = if is_burst {
            " - a 429 that names no usage limit or quota"
        } else {
            ""
        };
        let out_min = (q.until - q.hit).num_minutes();
        v.state = "unavailable".into();
        v.kind = "quota-unknown-reset".into();
        v.hit = Some(q.hit);
        v.until = Some(q.until);
        v.reason = format!(
            "usage limit hit {}, reset unknown; retry after {until_iso}",
            q.hit_iso
        );
        v.preflight = format!("unavailable: {}", v.reason);
        let tail = if roster_walk {
            ""
        } else {
            " (pass -SkipPreflight to launch anyway)"
        };
        v.refusal = format!(
            "provider {p} is not usable: it hit a {limit_word} limit at {} ({}{burst_clause}) and named no reset time - out for {out_min} minutes, until {until_iso}; nothing was started{tail}",
            q.hit_iso, q.message
        );
        v.label = format!(
            "unavailable ({}) - a real run is refused: {}",
            v.reason, v.refusal
        );
    } else if cred.state == State::Unknown {
        v.state = "unknown".into();
        v.kind = "unknown".into();
        v.refusal = format!(
            "provider {p}: availability could not be established ({}); pass -SkipPreflight to launch anyway, or fix the check",
            cred.reason
        );
        v.label = format!(
            "unknown ({}) - a real run is refused: {}",
            cred.reason, v.refusal
        );
    } else {
        v.label = format!("available ({})", cred.detail);
    }
    v.credential = Some(cred);
    v
}
