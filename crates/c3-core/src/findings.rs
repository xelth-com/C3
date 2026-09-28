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
//! ## Schema tolerance (F02-6, F07-1/2, F08-1/2)
//!
//! Every struct that mirrors a plugin file carries `#[serde(default)]` on the fields the
//! plugin may omit in an older or a freshly-created file, and a trailing
//! `#[serde(flatten)] extra` map so an unknown member (a field a later plugin wave adds)
//! is preserved *in place* on rewrite instead of being dropped. [`FindingStatus`] is a
//! tolerant token: a status string the bridge writes that C3 does not know becomes
//! [`FindingStatus::Other`] rather than a parse error, and it round-trips unchanged.
//! `ratings` is modelled as `Option<Vec<Rating>>`: a fresh store the plugin writes has no
//! `ratings` member at all (`New-FindingsStore`), so `None` is omitted and a brand-new
//! store round-trips byte-for-byte, while a store that has been rated round-trips its list.
//!
//! ## Status gating (F03-7, F08-11)
//!
//! [`Finding::status`] is private; the only way to move it is [`Finding::set_status`],
//! which runs the [`transition`] gate and, on success, appends one [`HistoryEvent`] and
//! sets the field - the gate and the append-only history are bound together, so a caller
//! cannot change the status without recording it. Acceptance test: `tests/formats.rs`
//! round-trips the real `.collab/c3-design/findings.json` byte-for-byte, and a fresh store
//! with no `ratings` member round-trips too.

use serde::de::Deserializer;
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::ps_json;

/// The whole `findings.json` document.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FindingsFile {
    #[serde(default)]
    pub task_id: String,
    #[serde(default)]
    pub findings: Vec<Finding>,
    /// A fresh store (`New-FindingsStore`) has no `ratings` member; it is added lazily on
    /// the first `-Rate`. `None` is omitted so a fresh store round-trips byte-for-byte.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ratings: Option<Vec<Rating>>,
    /// Unknown top-level members, preserved in place on rewrite.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The tracked lifecycle status of a finding. Known tokens plus a tolerant
/// [`FindingStatus::Other`] for a status string a later plugin wave may add.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum FindingStatus {
    #[default]
    Proposed,
    Implemented,
    Verified,
    Rejected,
    Wontfix,
    Superseded,
    /// A status token C3 does not know; preserved verbatim.
    Other(String),
}

impl FindingStatus {
    /// The on-disk token (`proposed`, ...); an unknown token is returned verbatim.
    pub fn as_str(&self) -> &str {
        match self {
            FindingStatus::Proposed => "proposed",
            FindingStatus::Implemented => "implemented",
            FindingStatus::Verified => "verified",
            FindingStatus::Rejected => "rejected",
            FindingStatus::Wontfix => "wontfix",
            FindingStatus::Superseded => "superseded",
            FindingStatus::Other(s) => s,
        }
    }

    /// Parse a token into a known variant or [`FindingStatus::Other`].
    pub fn from_token(s: &str) -> FindingStatus {
        match s {
            "proposed" => FindingStatus::Proposed,
            "implemented" => FindingStatus::Implemented,
            "verified" => FindingStatus::Verified,
            "rejected" => FindingStatus::Rejected,
            "wontfix" => FindingStatus::Wontfix,
            "superseded" => FindingStatus::Superseded,
            other => FindingStatus::Other(other.to_string()),
        }
    }
}

impl Serialize for FindingStatus {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for FindingStatus {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(FindingStatus::from_token(&s))
    }
}

/// One stored finding.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Finding {
    #[serde(default)]
    pub id: String,
    /// Private: move it only through [`Finding::set_status`] so the gate and the
    /// append-only `history[]` stay bound together.
    #[serde(default)]
    status: FindingStatus,
    /// `blocker | major | minor | note`.
    #[serde(default)]
    pub severity: String,
    #[serde(default)]
    pub locations: Vec<Location>,
    #[serde(default)]
    pub claim: String,
    #[serde(default)]
    pub trigger: String,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    #[serde(default)]
    pub verification: String,
    #[serde(default)]
    pub remedy: String,
    /// Ids this finding supersedes.
    #[serde(default)]
    pub supersedes: Vec<String>,
    /// Ids that superseded this finding (filled on the OLD finding; bookkeeping).
    #[serde(default)]
    pub superseded_by: Vec<String>,
    #[serde(default)]
    pub source: Source,
    #[serde(default)]
    pub history: Vec<HistoryEvent>,
    #[serde(default)]
    pub reviewer_checks: Vec<ReviewerCheck>,
    /// Unknown members, preserved in place on rewrite.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Finding {
    /// The tracked status (read-only; move it with [`Finding::set_status`]).
    pub fn status(&self) -> &FindingStatus {
        &self.status
    }

    /// Set the status if [`transition`] allows it, appending one [`HistoryEvent`] and then
    /// updating the field. The gate and the append are one operation: a rejected transition
    /// leaves both the status and the history untouched. `note`/`evidence` are the trimmed
    /// argument values; `when`/`by` stamp the history entry. Revision provenance
    /// (`base_commit`/`tree_sha256`) is left empty here and filled by the runtime when it
    /// has a resolved revision (it is not an input to the pure gate).
    pub fn set_status(
        &mut self,
        to: FindingStatus,
        note: &str,
        evidence: &str,
        when: &str,
        by: &str,
    ) -> Result<(), TransitionError> {
        transition(&self.status, &to, note, evidence)?;
        self.history.push(HistoryEvent {
            when: when.to_string(),
            status: to.clone(),
            by: by.to_string(),
            note: note.to_string(),
            evidence: evidence.to_string(),
            base_commit: String::new(),
            tree_sha256: String::new(),
            extra: Map::new(),
        });
        self.status = to;
        Ok(())
    }
}

/// A code location; `line` is `null` when unknown.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Location {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub line: Option<i64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A piece of evidence the reviewer cited.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Evidence {
    /// `read-code | ran-command | inferred | assumed`.
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub reference: String,
    #[serde(default)]
    pub observation: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Where the finding came from.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Source {
    #[serde(default)]
    pub consult: i64,
    #[serde(default)]
    pub reply: String,
    #[serde(default)]
    pub thread: String,
    #[serde(default)]
    pub base_commit: String,
    #[serde(default)]
    pub tree_sha256: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One append-only status-change record.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HistoryEvent {
    #[serde(default)]
    pub when: String,
    #[serde(default)]
    pub status: FindingStatus,
    #[serde(default)]
    pub by: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub evidence: String,
    #[serde(default)]
    pub base_commit: String,
    #[serde(default)]
    pub tree_sha256: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One later reply's report on a finding (the reviewer's own `fixed`/`still-open`/
/// `not-checked`, never the tracked status).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReviewerCheck {
    #[serde(default)]
    pub consult: i64,
    #[serde(default)]
    pub when: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub base_commit: String,
    #[serde(default)]
    pub tree_sha256: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A coordinator usefulness mark, copied from the ledger entry so it survives pruning.
///
/// Field order is the plugin's wave-26 literal `{n, consult_id, lineage, provider, model,
/// engine, purpose, topics, consult_when, useful, note, when}` (`codex-findings.ps1 -Rate`,
/// D2) and MUST NOT change. `engine`, `topics` and `consult_when` are the wave-26 additions
/// (D2); a mark recorded before wave 26 has none of them, so they are omittable (`Option`
/// with `skip_serializing_if`): absent (`None`) is skipped on rewrite, keeping a pre-wave-26
/// store byte-identical, while a fresh mark writes `engine` (a string), `topics` (an array,
/// possibly empty) and `consult_when` (the consultation's own time). A rating that carries a
/// `consult_id` is keyed by it (the routing score joins by `consult_id`); a pre-wave-26 mark
/// without one is keyed by `n` within its task.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Rating {
    #[serde(default)]
    pub n: i64,
    #[serde(default)]
    pub consult_id: String,
    #[serde(default)]
    pub lineage: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    /// (wave 26) the engine that carried the consultation (`codex` | `agy` | `muse`); absent
    /// in a pre-wave-26 mark.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    #[serde(default)]
    pub purpose: String,
    /// (wave 26) the consultation's `-Topic` slugs (a fresh mark writes an array, possibly
    /// empty); absent in a pre-wave-26 mark.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topics: Option<Vec<Value>>,
    /// (wave 26) the consultation's own time (the ledger entry's `when`); `when` is the time
    /// of the mark. Absent in a pre-wave-26 mark; a tri-state so a present `null` (the entry
    /// had no time) round-trips distinct from an absent key.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present_string"
    )]
    pub consult_when: Option<Option<String>>,
    /// `yes | partly | no`.
    #[serde(default)]
    pub useful: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub when: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Deserialize an optional string so a *present* value (including `null`) becomes `Some(..)`
/// and an *absent* key stays `None` (the [`Rating::consult_when`] tri-state). With
/// `#[serde(default)]`, an absent key never calls this and defaults to `None`.
fn deserialize_present_string<'de, D>(d: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<String>::deserialize(d)?))
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
/// `superseded` requires neither. `note`/`evidence` are the trimmed argument values. A
/// tolerant [`FindingStatus::Other`] token is treated as any other target: no gate fires.
pub fn transition(
    from: &FindingStatus,
    to: &FindingStatus,
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
        FindingStatus::Proposed if *from != FindingStatus::Proposed && note.trim().is_empty() => {
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
            transition(&Implemented, &Verified, "", ""),
            Err(TransitionError::EvidenceRequired)
        );
        assert!(transition(&Implemented, &Verified, "", "ran cargo test: 21 passed").is_ok());
    }

    #[test]
    fn rejected_needs_note() {
        assert_eq!(
            transition(&Proposed, &Rejected, "", ""),
            Err(TransitionError::NoteRequiredReject)
        );
        assert!(transition(&Proposed, &Rejected, "not a real bug", "").is_ok());
    }

    #[test]
    fn reopen_needs_note_but_superseded_does_not() {
        assert_eq!(
            transition(&Verified, &Proposed, "", ""),
            Err(TransitionError::NoteRequiredReopen)
        );
        assert!(transition(&Verified, &Proposed, "regressed", "").is_ok());
        // proposed -> proposed is not a reopen.
        assert!(transition(&Proposed, &Proposed, "", "").is_ok());
        // superseded requires neither.
        assert!(transition(&Verified, &Superseded, "", "").is_ok());
    }

    #[test]
    fn set_status_gates_and_appends_history() {
        let mut f = Finding {
            status: Implemented,
            ..Default::default()
        };
        // A gated transition changes nothing.
        assert!(f.set_status(Verified, "", "", "t0", "me").is_err());
        assert_eq!(f.status(), &Implemented);
        assert!(f.history.is_empty());
        // An allowed transition appends exactly one history entry and moves the status.
        f.set_status(Verified, "", "ran cargo test", "t1", "me")
            .unwrap();
        assert_eq!(f.status(), &Verified);
        assert_eq!(f.history.len(), 1);
        assert_eq!(f.history[0].status, Verified);
        assert_eq!(f.history[0].evidence, "ran cargo test");
    }

    #[test]
    fn status_token_is_tolerant() {
        let s = FindingStatus::from_token("archived");
        assert_eq!(s, FindingStatus::Other("archived".into()));
        assert_eq!(s.as_str(), "archived");
        // round-trips as a bare string
        let j = serde_json::to_string(&s).unwrap();
        assert_eq!(j, "\"archived\"");
        let back: FindingStatus = serde_json::from_str(&j).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn fresh_store_has_no_ratings_member() {
        let fresh = r#"{"task_id":"x","findings":[]}"#;
        let f = FindingsFile::read(fresh.as_bytes()).unwrap();
        assert!(f.ratings.is_none());
        let v: Value = serde_json::from_slice(&f.to_bytes().unwrap()).unwrap();
        assert!(
            !v.as_object().unwrap().contains_key("ratings"),
            "a fresh store keeps no ratings member"
        );
    }

    #[test]
    fn unknown_member_is_preserved() {
        let src = r#"{"task_id":"x","findings":[],"future_field":{"a":1}}"#;
        let f = FindingsFile::read(src.as_bytes()).unwrap();
        assert!(f.extra.contains_key("future_field"));
        let v: Value = serde_json::from_slice(&f.to_bytes().unwrap()).unwrap();
        assert_eq!(v["future_field"]["a"], serde_json::json!(1));
    }

    #[test]
    fn id_format() {
        assert_eq!(finding_id(2, 1), "F02-1");
        assert_eq!(finding_id(15, 4), "F15-4");
        assert_eq!(finding_id(100, 3), "F100-3");
    }
}
