//! The panel member spec — the wire format the panel run hands each member (a port of the
//! plugin's `-PanelSpec`, `Start-PanelMember` at `codex-consult.ps1:2205-2274`).
//!
//! Each seated member is a child process of the same binary. The panel run bakes everything the
//! member needs — its roster position, its numbering (`n`/`NN`), the panel id, the panel-wide
//! member list, the SAME open-findings snapshot (`listed_ids`, computed once before any member
//! starts), the parent's pid + start time (the member's proof of a live parent, D6/D1), the
//! union of every other member's `NN` (the agy tree check, D7), the endpoint plan
//! (`concurrency`/`limits`), the routing record, the role, and an `args` object mirroring the
//! run's CLI options — into a [`MemberSpec`], serialises it to compact UTF-8 JSON and base64s it
//! onto the child's command line.
//!
//! The plugin encodes CLIXML; the port defines its own JSON wire format with the same round-trip
//! guarantee (m4-spec open question 3). The identity/numbering fields the member path reads
//! directly are typed; `limits`, `routing` and `args` stay `serde_json::Value` so the shape can
//! grow with the member run without churning this type.

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One entry of the panel-wide member list (`members[]`, roster order): the same shape the
/// ledger `panel.members[]` record carries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberBrief {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub reason: String,
}

/// The base64-of-JSON spec a panel run hands one member (`-PanelSpec`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemberSpec {
    /// The panel id (guid).
    #[serde(default)]
    pub id: String,
    /// This seat `k` (1-based).
    #[serde(default, deserialize_with = "de_i64")]
    pub position: i64,
    /// The number of seats started.
    #[serde(default, deserialize_with = "de_i64")]
    pub of: i64,
    /// The panel-wide member list, roster order.
    #[serde(default)]
    pub members: Vec<MemberBrief>,
    /// This member's roster position.
    #[serde(default, deserialize_with = "de_i64")]
    pub roster_position: i64,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub engine: String,
    /// The panel's skipped-entries record (roster order), for the member's summary context.
    #[serde(default)]
    pub skipped: Value,
    /// The open findings at panel start — the SAME set for every member (blind within a wave).
    #[serde(default)]
    pub listed_ids: Vec<String>,
    /// This member's consult number.
    #[serde(default, deserialize_with = "de_i64")]
    pub n: i64,
    /// This member's handoff number (`NN`). The panel run writes it as a zero-padded string
    /// (`"01"`) on the wire, so it decodes from a string or a number.
    #[serde(default, deserialize_with = "de_i64")]
    pub nn: i64,
    /// The consult id reserved for this member (the prompt's last line).
    #[serde(default)]
    pub consult_id: String,
    /// The panel run's pid — the member's proof of a live parent (D6).
    #[serde(default, deserialize_with = "de_i64")]
    pub parent_pid: i64,
    /// The panel run's process start time (guards pid reuse).
    #[serde(default)]
    pub parent_start_time: String,
    /// Every OTHER member's `NN` (union), for the agy sibling tree check (D7).
    #[serde(default, deserialize_with = "de_vec_i64")]
    pub sibling_nns: Vec<i64>,
    /// The effective concurrency (`Get-PanelPlan.Effective`).
    #[serde(default, deserialize_with = "de_i64")]
    pub concurrency: i64,
    /// The endpoint-group limits (label -> group limit).
    #[serde(default)]
    pub limits: Value,
    /// The size asked (`= of`).
    #[serde(default, deserialize_with = "de_i64")]
    pub asked: i64,
    /// The `panel.routing` record (object, or null for an un-routed / no-roster panel).
    #[serde(default)]
    pub routing: Option<Value>,
    /// The member's role (empty if none).
    #[serde(default)]
    pub role: String,
    /// The panel-wide roles note (`Select-RoleAssignment`), if any.
    #[serde(default)]
    pub roles_note: String,
    /// The panel-level warnings, surfaced in each member's console/handoff.
    #[serde(default)]
    pub panel_warnings: Vec<String>,
    /// The run's CLI options, mirrored so the member reconstructs the same consultation.
    #[serde(default)]
    pub args: Value,
}

/// Deserialize an `i64` from a JSON number OR a string (the panel run writes `nn` — and, in the
/// plugin's PowerShell wire, other counts — as strings). A blank/absent value is `0`.
fn de_i64<'de, D: serde::Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    match Value::deserialize(d)? {
        Value::Number(n) => Ok(n
            .as_i64()
            .unwrap_or_else(|| n.as_f64().unwrap_or(0.0) as i64)),
        Value::String(s) => Ok(s.trim().parse::<i64>().unwrap_or(0)),
        Value::Null => Ok(0),
        _ => Ok(0),
    }
}

/// Deserialize `Vec<i64>` from an array of numbers or strings (the plugin's `sibling_nns`).
fn de_vec_i64<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<i64>, D::Error> {
    let v = Value::deserialize(d)?;
    let arr = match v {
        Value::Array(a) => a,
        Value::Null => return Ok(Vec::new()),
        _ => return Ok(Vec::new()),
    };
    Ok(arr
        .into_iter()
        .map(|x| match x {
            Value::Number(n) => n
                .as_i64()
                .unwrap_or_else(|| n.as_f64().unwrap_or(0.0) as i64),
            Value::String(s) => s.trim().parse::<i64>().unwrap_or(0),
            _ => 0,
        })
        .collect())
}

impl MemberSpec {
    /// Encode to the wire string the panel run puts after `--panel-spec`: base64 (standard, with
    /// padding) of the compact UTF-8 JSON, exactly like the plugin's `[Convert]::ToBase64String`
    /// over `ConvertTo-Json -Compress`.
    pub fn to_wire(&self) -> Result<String, serde_json::Error> {
        let json = serde_json::to_vec(self)?;
        Ok(base64::engine::general_purpose::STANDARD.encode(json))
    }

    /// Decode a `--panel-spec` value (base64 of UTF-8 JSON). `Err` carries a one-line reason,
    /// matching the plugin's "-PanelSpec is internal to -Panel and could not be read (...)".
    pub fn from_wire(wire: &str) -> Result<MemberSpec, String> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(wire.trim())
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
        serde_json::from_str(&text).map_err(|e| e.to_string())
    }

    /// A member accepts its spec only when it names its own numbers and parent (`n`, `nn`,
    /// `parent_pid`) — `codex-consult.ps1:1795`.
    pub fn names_member(&self) -> bool {
        self.n > 0 && self.nn > 0 && self.parent_pid > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> MemberSpec {
        MemberSpec {
            id: "043d5bfe-1111-2222-3333-444455556666".into(),
            position: 3,
            of: 10,
            members: vec![
                MemberBrief {
                    provider: "ZAI".into(),
                    model: "glm-5.3".into(),
                    state: "run".into(),
                    reason: String::new(),
                },
                MemberBrief {
                    provider: "mimo".into(),
                    model: "mimo-1".into(),
                    state: "skipped".into(),
                    reason: "weighty reviewer".into(),
                },
            ],
            roster_position: 4,
            provider: "gemini".into(),
            model: "gemini-3-pro".into(),
            engine: "agy".into(),
            skipped: json!([{"provider": "mimo", "model": "mimo-1", "reason": "weighty"}]),
            listed_ids: vec!["f-001".into(), "f-002".into()],
            n: 12,
            nn: 5,
            consult_id: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".into(),
            parent_pid: 4321,
            parent_start_time: "2026-09-27T10:11:12.3456789Z".into(),
            sibling_nns: vec![2, 3, 4, 6, 7, 8, 9, 10, 11],
            concurrency: 8,
            limits: json!({"ZAI": 1, "gemini": 2, "byteplus": 3}),
            asked: 10,
            routing: Some(json!({"mode": "routed", "order": "routed", "seed": "deadbeef"})),
            role: "adversary".into(),
            roles_note: "roles assigned by seat".into(),
            panel_warnings: vec!["a framing panel seated below 2".into()],
            args: json!({
                "collab_dir": ".collab",
                "purpose": "framing",
                "timeout_sec": 1800,
                "artifact": ["docs/a.md"],
                "raw": false,
                "dry_run": false
            }),
        }
    }

    #[test]
    fn wire_round_trips_every_field() {
        let spec = sample();
        let wire = spec.to_wire().unwrap();
        // The wire is base64 (no JSON punctuation leaks through).
        assert!(!wire.contains('{') && !wire.contains('"'));
        let back = MemberSpec::from_wire(&wire).unwrap();

        assert_eq!(back.id, spec.id);
        assert_eq!(back.position, 3);
        assert_eq!(back.of, 10);
        assert_eq!(back.members.len(), 2);
        assert_eq!(back.members[0].provider, "ZAI");
        assert_eq!(back.members[1].reason, "weighty reviewer");
        assert_eq!(back.roster_position, 4);
        assert_eq!(back.provider, "gemini");
        assert_eq!(back.model, "gemini-3-pro");
        assert_eq!(back.engine, "agy");
        assert_eq!(back.listed_ids, vec!["f-001", "f-002"]);
        assert_eq!(back.n, 12);
        assert_eq!(back.nn, 5);
        assert_eq!(back.consult_id, spec.consult_id);
        assert_eq!(back.parent_pid, 4321);
        assert_eq!(back.parent_start_time, spec.parent_start_time);
        assert_eq!(back.sibling_nns, spec.sibling_nns);
        assert_eq!(back.concurrency, 8);
        assert_eq!(back.limits, spec.limits);
        assert_eq!(back.asked, 10);
        assert_eq!(back.routing, spec.routing);
        assert_eq!(back.role, "adversary");
        assert_eq!(back.roles_note, "roles assigned by seat");
        assert_eq!(back.panel_warnings, spec.panel_warnings);
        assert_eq!(back.args, spec.args);
        assert!(back.names_member());
    }

    #[test]
    fn from_wire_rejects_garbage() {
        assert!(MemberSpec::from_wire("not base64 %%%").is_err());
        // valid base64 of non-JSON.
        let b64 = base64::engine::general_purpose::STANDARD.encode(b"not json");
        assert!(MemberSpec::from_wire(&b64).is_err());
    }

    #[test]
    fn names_member_requires_numbers_and_parent() {
        let mut spec = sample();
        assert!(spec.names_member());
        spec.parent_pid = 0;
        assert!(!spec.names_member());
        spec.parent_pid = 4321;
        spec.n = 0;
        assert!(!spec.names_member());
    }

    #[test]
    fn defaults_absent_fields() {
        // A spec that names only the essentials still decodes (missing fields default).
        let wire = base64::engine::general_purpose::STANDARD
            .encode(br#"{"id":"x","n":1,"nn":1,"parent_pid":9}"#);
        let s = MemberSpec::from_wire(&wire).unwrap();
        assert_eq!(s.id, "x");
        assert!(s.members.is_empty());
        assert_eq!(s.routing, None);
        assert_eq!(s.role, "");
        assert!(s.names_member());
    }
}
