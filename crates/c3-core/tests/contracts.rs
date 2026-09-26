//! Acceptance tests for the M3 core contracts, against the real design evidence in
//! `.collab/c3-design/`. The headline test is byte-identity: parse a real store into the
//! typed model and re-serialize; the bytes must match what the plugin wrote.

use std::fs;
use std::path::PathBuf;

use c3_core::findings::FindingsFile;
use c3_core::handoff::{Author, HandoffHeader, TokenReport};
use c3_core::ledger::SessionsFile;
use c3_core::ps_json;

fn design_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.collab/c3-design")
        .canonicalize()
        .expect("the .collab/c3-design evidence directory exists")
}

/// Report the first differing byte offset with a little context, for a readable failure.
fn assert_bytes_eq(got: &[u8], want: &[u8], what: &str) {
    if got == want {
        return;
    }
    let n = got.len().min(want.len());
    let mut at = n;
    for i in 0..n {
        if got[i] != want[i] {
            at = i;
            break;
        }
    }
    let lo = at.saturating_sub(60);
    let g = String::from_utf8_lossy(&got[lo..(at + 60).min(got.len())]);
    let w = String::from_utf8_lossy(&want[lo..(at + 60).min(want.len())]);
    panic!(
        "{what}: bytes differ at offset {at} (got {} bytes, want {} bytes)\n  got : {:?}\n  want: {:?}",
        got.len(),
        want.len(),
        g,
        w
    );
}

#[test]
fn ps_json_formatter_reproduces_sessions_byte_for_byte() {
    // Pure formatter proof, independent of the typed model: parse to a Value and format.
    let bytes = fs::read(design_dir().join("sessions.json")).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let mut out = ps_json::format_value_root(&value);
    out.push('\n');
    assert_bytes_eq(out.as_bytes(), &bytes, "sessions.json (formatter on Value)");
}

#[test]
fn ps_json_formatter_reproduces_findings_byte_for_byte() {
    let bytes = fs::read(design_dir().join("findings.json")).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let mut out = ps_json::format_value_root(&value);
    out.push('\n');
    assert_bytes_eq(out.as_bytes(), &bytes, "findings.json (formatter on Value)");
}

#[test]
fn sessions_typed_round_trip_byte_for_byte() {
    let bytes = fs::read(design_dir().join("sessions.json")).unwrap();
    let sessions = SessionsFile::read(&bytes).expect("parse sessions.json");
    let out = sessions.to_bytes().expect("serialize sessions.json");
    assert_bytes_eq(&out, &bytes, "sessions.json (typed round-trip)");
}

#[test]
fn findings_typed_round_trip_byte_for_byte() {
    let bytes = fs::read(design_dir().join("findings.json")).unwrap();
    let findings = FindingsFile::read(&bytes).expect("parse findings.json");
    let out = findings.to_bytes().expect("serialize findings.json");
    assert_bytes_eq(&out, &bytes, "findings.json (typed round-trip)");
}

#[test]
fn handoff_15_header_renders_exactly() {
    let md = fs::read_to_string(design_dir().join("handoffs/15-codex-merge-framing.md")).unwrap();
    // The header is everything up to and including the `---` separator line.
    let (head, _body) = md
        .split_once("\n---\n")
        .expect("handoff has a --- separator");
    let header_region = format!("{head}\n---\n");
    let lines: Vec<&str> = header_region.lines().collect();
    let line_by = |p: &str| {
        lines
            .iter()
            .find(|l| l.starts_with(p))
            .unwrap_or_else(|| panic!("no line starting with {p:?}"))
            .to_string()
    };
    // The prose lines (composed elsewhere by identity/lineage/revision rendering) are fed
    // as given; render() must reproduce every other line's wording and the scaffold.
    let inv = line_by("Invocation:");
    let argv = inv
        .split_once("Argv: `")
        .unwrap()
        .1
        .rsplit_once("` (")
        .unwrap()
        .0
        .to_string();

    let header = HandoffHeader {
        nn: 15,
        engine_label: "Codex".into(),
        slug: "merge-framing".into(),
        date: "2026-09-26 18:02".into(),
        author: Author::Codex {
            model: "gpt-6-astra".into(),
            effort: "high".into(),
            cli_version: "0.155.1".into(),
        },
        effort_sent: "high".into(),
        effort_requested: "high".into(),
        effort_mapping: "openai".into(),
        effort_basis: "caps-v1: builtin:openai, any model".into(),
        consult_id: "d8c2b550-5b84-4908-8c63-e6d071322dbc".into(),
        mode: "new".into(),
        sandbox: "read-only".into(),
        purpose: "framing".into(),
        argv,
        prompt_via: "prompt on stdin".into(),
        bridge_outcome: "usable reply".into(),
        wall_seconds: "252.1".into(),
        tokens: TokenReport::Reported {
            input: 209515,
            cached: 152960,
            output: 7328,
            reasoning: 1238,
        },
        events_rel: "handoffs/15-codex-merge-framing.events.jsonl".into(),
        reviewer_line: line_by("Reviewer:"),
        preflight_line: line_by("Preflight:"),
        roster_line: Some(line_by("Roster:")),
        parent_result_line: line_by("Parent thread:"),
        brief_reviewed_line: line_by("Brief:"),
        timeout_line: line_by("Timeout:"),
        verdict_line: line_by("Verdict:"),
        extra: vec![],
    };

    assert_eq!(header.render(), header_region);
}
