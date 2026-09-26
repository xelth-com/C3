//! Findings: `findings.json`, the finding lifecycle and ratings.
//!
//! Mirrors the stored record `codex-findings.ps1` keeps (README "Findings: ids, status,
//! ratings"): the id `F<NN>-<k>` (`NN` the handoff number of the reply that raised it,
//! `k` its 1-based position in that reply's `findings[]`), the tracked `status`, the
//! severity, locations, the reviewer's claim/trigger/evidence, `supersedes`/`superseded_by`
//! bookkeeping, `source`, an append-only `history[]` (one entry per status change) and
//! `reviewer_checks[]` (one entry per later reply that reported on it). Field order matches
//! the on-disk record and is preserved by [`crate::ps_json`].
//!
//! [`transition`] is the pure status-change rule (README): `verified` requires evidence, a
//! `rejected` and a reopen (`-> proposed` from a non-`proposed` state) require a note, and
//! `superseded` requires neither; any status may follow any other. Acceptance test:
//! `tests/formats.rs` round-trips the real `.collab/c3-design/findings.json` byte-for-byte.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ps_json;

/// The whole `findings.json` document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingsFile {
    pub task_id: String,
    pub findings: Vec<Finding>,
    pub ratings: Vec<Rating>,
}

/// The tracked lifecycle status of a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FindingStatus {
    Proposed,
    Implemented,
    Verified,
    Rejected,
    Wontfix,
    Superseded,
}

impl FindingStatus {
    /// The on-disk token (`proposed`, ...).
    pub fn as_str(self) -> &'static str {
        match self {
            FindingStatus::Proposed => "proposed",
            FindingStatus::Implemented => "implemented",
            FindingStatus::Verified => "verified",
            FindingStatus::Rejected => "rejected",
            FindingStatus::Wontfix => "wontfix",
            FindingStatus::Superseded => "superseded",
        }
    }
}

/// One stored finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub status: FindingStatus,
    /// `blocker | major | minor | note`.
    pub severity: String,
    pub locations: Vec<Location>,
    pub claim: String,
    pub trigger: String,
    pub evidence: Vec<Evidence>,
    pub verification: String,
    pub remedy: String,
    /// Ids this finding supersedes.
    pub supersedes: Vec<Value>,
    /// Ids that superseded this finding (filled on the OLD finding; bookkeeping).
    pub superseded_by: Vec<Value>,
    pub source: Source,
    pub history: Vec<HistoryEvent>,
    pub reviewer_checks: Vec<ReviewerCheck>,
}

/// A code location; `line` is `null` when unknown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub path: String,
    pub line: Option<i64>,
}

/// A piece of evidence the reviewer cited.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    /// `read-code | ran-command | inferred | assumed`.
    pub kind: String,
    pub reference: String,
    pub observation: String,
}

/// Where the finding came from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub consult: i64,
    pub reply: String,
    pub thread: String,
    pub base_commit: String,
    pub tree_sha256: String,
}

/// One append-only status-change record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEvent {
    pub when: String,
    pub status: FindingStatus,
    pub by: String,
    pub note: String,
    pub evidence: String,
    pub base_commit: String,
    pub tree_sha256: String,
}

/// One later reply's report on a finding (the reviewer's own `fixed`/`still-open`/
/// `not-checked`, never the tracked status).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewerCheck {
    pub consult: i64,
    pub when: String,
    pub status: String,
    pub note: String,
    pub base_commit: String,
    pub tree_sha256: String,
}

/// A coordinator usefulness mark, copied from the ledger entry so it survives pruning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rating {
    pub n: i64,
    pub consult_id: String,
    pub lineage: String,
    pub provider: String,
    pub model: String,
    pub purpose: String,
    /// `yes | partly | no`.
    pub useful: String,
    pub note: String,
    pub when: String,
}

/// Why a status change was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionError {
    /// `verified` needs `-Evidence`.
    EvidenceRequired,
    /// `rejected` needs `-Note`.
    NoteRequiredReject,
    /// A reopen (`-> proposed` from a non-`proposed` state) needs `-Note`.
    NoteRequiredReopen,
}

impl std::fmt::Display for TransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransitionError::EvidenceRequired => write!(
                f,
                "verified requires -Evidence (what you ran, not that you believe it)"
            ),
            TransitionError::NoteRequiredReject => write!(f, "rejected requires -Note"),
            TransitionError::NoteRequiredReopen => {
                write!(f, "reopening a finding (-Status proposed) requires -Note")
            }
        }
    }
}

impl std::error::Error for TransitionError {}

/// The status-transition rule (README). Any status may follow any other; the gates are:
/// `verified` requires non-empty `evidence`; `rejected` requires a non-empty `note`; a
/// reopen (`to == proposed` while `from != proposed`) requires a non-empty `note`;
/// `superseded` requires neither. `note`/`evidence` are the trimmed argument values.
pub fn transition(
    from: FindingStatus,
    to: FindingStatus,
    note: &str,
    evidence: &str,
) -> Result<(), TransitionError> {
    match to {
        FindingStatus::Verified if evidence.trim().is_empty() => {
            Err(TransitionError::EvidenceRequired)
        }
        FindingStatus::Rejected if note.trim().is_empty() => {
            Err(TransitionError::NoteRequiredReject)
        }
        FindingStatus::Proposed if from != FindingStatus::Proposed && note.trim().is_empty() => {
            Err(TransitionError::NoteRequiredReopen)
        }
        _ => Ok(()),
    }
}

/// `F<NN>-<k>` for a reply's handoff number `nn` and 1-based finding position `k`.
pub fn finding_id(nn: u32, k: usize) -> String {
    format!("F{nn:02}-{k}")
}

impl FindingsFile {
    /// Parse `findings.json` bytes.
    pub fn read(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    /// Serialize to the exact on-disk byte stream `Write-JsonFile` would produce.
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        ps_json::to_ps_json_bytes(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use FindingStatus::*;

    #[test]
    fn verified_needs_evidence() {
        assert_eq!(
            transition(Implemented, Verified, "", ""),
            Err(TransitionError::EvidenceRequired)
        );
        assert!(transition(Implemented, Verified, "", "ran cargo test: 21 passed").is_ok());
    }

    #[test]
    fn rejected_needs_note() {
        assert_eq!(
            transition(Proposed, Rejected, "", ""),
            Err(TransitionError::NoteRequiredReject)
        );
        assert!(transition(Proposed, Rejected, "not a real bug", "").is_ok());
    }

    #[test]
    fn reopen_needs_note_but_superseded_does_not() {
        assert_eq!(
            transition(Verified, Proposed, "", ""),
            Err(TransitionError::NoteRequiredReopen)
        );
        assert!(transition(Verified, Proposed, "regressed", "").is_ok());
        // proposed -> proposed is not a reopen.
        assert!(transition(Proposed, Proposed, "", "").is_ok());
        // superseded requires neither.
        assert!(transition(Verified, Superseded, "", "").is_ok());
    }

    #[test]
    fn id_format() {
        assert_eq!(finding_id(2, 1), "F02-1");
        assert_eq!(finding_id(15, 4), "F15-4");
        assert_eq!(finding_id(100, 3), "F100-3");
    }
}
