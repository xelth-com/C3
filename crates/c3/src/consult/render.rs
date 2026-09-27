//! Rendering the handoff's structured section and the small formatters it shares with the
//! summary (`Format-Locations`, `Format-SeverityCounts`, `Format-IdRange`, `Close-Sentence`,
//! `Format-StructuredSection`; `codex-consult-common.ps1`).
//!
//! The structured section is the `### Findings` / `### Prior findings` / `## Verdict` /
//! `### Blockers` / `### Unproven scenarios` / `### First-run checklist` block appended below
//! the verbatim reply in the handoff and echoed at the end of a successful run.
//!
//! M2c renders the reply's own content faithfully; the prior-blocker *retention* (F04-4:
//! carrying an open prior blocker the reply omitted into `### Blockers`) and the verdict-vs-
//! prior-blocker contradiction check are the ingestion layer's job (deferred with the full
//! findings ingestion) — this renderer draws what the validated reply carries.

use c3_core::engine::{ReplyFinding, ReplyLocation, Severity, StructuredReply, Verdict};
use c3_core::ledger::FindingCounts;

/// `Format-Locations`: `path:line` joined by `, `, each optionally in code ticks; a
/// finding with no location renders `(no location)`.
pub fn format_locations(locations: &[ReplyLocation], code: bool) -> String {
    let mut parts: Vec<String> = Vec::new();
    for l in locations {
        if l.path.is_empty() && l.line.is_none() {
            continue;
        }
        let mut text = l.path.clone();
        if let Some(n) = l.line {
            text = format!("{}:{}", l.path, n);
        }
        if text.is_empty() {
            continue;
        }
        if code {
            text = format!("`{text}`");
        }
        parts.push(text);
    }
    if parts.is_empty() {
        return "(no location)".to_string();
    }
    parts.join(", ")
}

/// `Format-SeverityCounts`.
pub fn format_severity_counts(c: &FindingCounts) -> String {
    format!(
        "{} blocker, {} major, {} minor, {} note",
        c.blocker, c.major, c.minor, c.note
    )
}

/// `Format-IdRange`: `""`, a single id, or `first..last`.
pub fn format_id_range(ids: &[String]) -> String {
    let list: Vec<&String> = ids.iter().filter(|s| !s.is_empty()).collect();
    match list.len() {
        0 => String::new(),
        1 => list[0].clone(),
        _ => format!("{}..{}", list[0], list[list.len() - 1]),
    }
}

/// `Close-Sentence`: one-line, terminated with a period unless it already ends in `.!?`.
pub fn close_sentence(text: &str) -> String {
    let t = c3_core::one_line(text);
    if t.is_empty() {
        return String::new();
    }
    if t.ends_with(['.', '!', '?']) {
        t
    } else {
        format!("{t}.")
    }
}

/// The severity histogram of a reply's findings.
pub fn severity_counts(reply: &StructuredReply) -> FindingCounts {
    let mut c = FindingCounts::default();
    for f in &reply.findings {
        match f.severity {
            Severity::Blocker => c.blocker += 1,
            Severity::Major => c.major += 1,
            Severity::Minor => c.minor += 1,
            Severity::Note => c.note += 1,
        }
    }
    c
}

fn severity_token(s: Severity) -> &'static str {
    match s {
        Severity::Blocker => "blocker",
        Severity::Major => "major",
        Severity::Minor => "minor",
        Severity::Note => "note",
    }
}

fn verdict_token(v: Verdict) -> &'static str {
    match v {
        Verdict::Accept => "ACCEPT",
        Verdict::Hold => "HOLD",
        Verdict::Reject => "REJECT",
        Verdict::Advise => "ADVISE",
    }
}

fn finding_line(id: &str, f: &ReplyFinding) -> String {
    let mut line = format!(
        "- **{}** [{}] {} - {}",
        id,
        severity_token(f.severity),
        format_locations(&f.locations, true),
        close_sentence(&f.claim)
    );
    if !f.trigger.is_empty() {
        line.push_str(&format!(" Trigger: {}", close_sentence(&f.trigger)));
    }
    let ev: Vec<String> = f
        .evidence
        .iter()
        .map(|e| {
            format!(
                "{}: {}",
                evidence_kind(e),
                c3_core::one_line(&e.observation)
            )
        })
        .collect();
    if !ev.is_empty() {
        line.push_str(&format!(" Evidence: {}", close_sentence(&ev.join("; "))));
    }
    if !f.verification.is_empty() {
        line.push_str(&format!(" Verify: {}", close_sentence(&f.verification)));
    }
    if !f.remedy.is_empty() {
        line.push_str(&format!(" Remedy: {}", close_sentence(&f.remedy)));
    }
    let sup: Vec<&String> = f.supersedes.iter().filter(|s| !s.is_empty()).collect();
    if !sup.is_empty() {
        let joined = sup
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        line.push_str(&format!(" Supersedes: {joined}."));
    }
    line
}

fn evidence_kind(e: &c3_core::engine::ReplyEvidence) -> &'static str {
    use c3_core::engine::EvidenceKind::*;
    match e.kind {
        ReadCode => "read-code",
        RanCommand => "ran-command",
        Inferred => "inferred",
        Assumed => "assumed",
    }
}

fn prior_status_token(s: c3_core::engine::PriorStatus) -> &'static str {
    use c3_core::engine::PriorStatus::*;
    match s {
        Fixed => "fixed",
        StillOpen => "still-open",
        NotChecked => "not-checked",
        UnknownId => "unknown-id",
    }
}

/// Render the handoff's structured section (`Format-StructuredSection`) for a validated
/// reply, given the finding ids assigned to its findings (`ids[i]` is `findings[i]`'s id).
/// Lines join with `\n`, matching the plugin.
pub fn format_structured_section(reply: &StructuredReply, ids: &[String]) -> String {
    let mut out: Vec<String> = Vec::new();

    out.push("### Findings".into());
    out.push(String::new());
    if reply.findings.is_empty() {
        out.push("_(none)_".into());
    }
    for (i, f) in reply.findings.iter().enumerate() {
        let id = ids.get(i).map(|s| s.as_str()).unwrap_or("?");
        out.push(finding_line(id, f));
    }
    out.push(String::new());

    out.push("### Prior findings".into());
    out.push(String::new());
    if reply.prior_findings.is_empty() {
        out.push("_(none)_".into());
    }
    for p in &reply.prior_findings {
        let mut line = format!("- {} - {}", p.id, prior_status_token(p.status));
        let note = c3_core::one_line(&p.note);
        if !note.is_empty() {
            line.push_str(&format!(" - {note}"));
        }
        out.push(line);
    }
    out.push(String::new());

    out.push(format!("## Verdict: {}", verdict_token(reply.verdict)));
    out.push(String::new());
    let reason = c3_core::one_line(&reply.verdict_reason);
    out.push(if reason.is_empty() {
        "_(no reason given)_".into()
    } else {
        reason
    });
    out.push(String::new());

    out.push("### Blockers".into());
    out.push(String::new());
    let mut nb = 0;
    for (i, f) in reply.findings.iter().enumerate() {
        if f.severity != Severity::Blocker {
            continue;
        }
        nb += 1;
        let id = ids.get(i).map(|s| s.as_str()).unwrap_or("?");
        let mut line = format!(
            "- **{}** {} - {}",
            id,
            format_locations(&f.locations, true),
            close_sentence(&f.claim)
        );
        if !f.verification.is_empty() {
            line.push_str(&format!(" Verify: {}", close_sentence(&f.verification)));
        }
        if !f.remedy.is_empty() {
            line.push_str(&format!(" Remedy: {}", close_sentence(&f.remedy)));
        }
        out.push(line);
    }
    if nb == 0 {
        out.push("_(none)_".into());
    }
    out.push(String::new());

    out.push("### Unproven scenarios".into());
    out.push(String::new());
    let un: Vec<&String> = reply.unproven.iter().filter(|s| !s.is_empty()).collect();
    if un.is_empty() {
        out.push("_(none)_".into());
    }
    for u in un {
        out.push(format!("- {}", c3_core::one_line(u)));
    }
    out.push(String::new());

    out.push("### First-run checklist (observable)".into());
    out.push(String::new());
    let cl: Vec<&String> = reply
        .first_run_checklist
        .iter()
        .filter(|s| !s.is_empty())
        .collect();
    if cl.is_empty() {
        out.push("_(none)_".into());
    }
    for c in cl {
        out.push(format!("- [ ] {}", c3_core::one_line(c)));
    }

    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use c3_core::engine::{
        EvidenceKind, PriorStatus, ReplyEvidence, ReplyFinding, ReplyLocation, ReplyPriorFinding,
    };

    fn reply() -> StructuredReply {
        StructuredReply {
            verdict: Verdict::Reject,
            verdict_reason: "a blocker remains".into(),
            reply_markdown: "body".into(),
            findings: vec![ReplyFinding {
                severity: Severity::Blocker,
                locations: vec![ReplyLocation {
                    path: "a.rs".into(),
                    line: Some(10),
                }],
                claim: "the lock is not released".into(),
                trigger: "on error".into(),
                evidence: vec![ReplyEvidence {
                    kind: EvidenceKind::ReadCode,
                    reference: "a.rs:10".into(),
                    observation: "no drop".into(),
                }],
                verification: "run the test".into(),
                remedy: "drop the guard".into(),
                supersedes: vec![],
            }],
            prior_findings: vec![ReplyPriorFinding {
                id: "F01-1".into(),
                status: PriorStatus::Fixed,
                note: "done".into(),
            }],
            unproven: vec!["the race under load".into()],
            first_run_checklist: vec![],
        }
    }

    #[test]
    fn locations_and_counts() {
        let l = vec![
            ReplyLocation {
                path: "a.rs".into(),
                line: Some(5),
            },
            ReplyLocation {
                path: "b.rs".into(),
                line: None,
            },
        ];
        assert_eq!(format_locations(&l, false), "a.rs:5, b.rs");
        assert_eq!(format_locations(&l, true), "`a.rs:5`, `b.rs`");
        assert_eq!(format_locations(&[], true), "(no location)");
        assert_eq!(
            format_severity_counts(&severity_counts(&reply())),
            "1 blocker, 0 major, 0 minor, 0 note"
        );
    }

    #[test]
    fn id_range_forms() {
        assert_eq!(format_id_range(&[]), "");
        assert_eq!(format_id_range(&["F03-1".into()]), "F03-1");
        assert_eq!(
            format_id_range(&["F03-1".into(), "F03-2".into(), "F03-3".into()]),
            "F03-1..F03-3"
        );
    }

    #[test]
    fn close_sentence_adds_period() {
        assert_eq!(close_sentence("hello"), "hello.");
        assert_eq!(close_sentence("done."), "done.");
        assert_eq!(close_sentence("  a  b "), "a b.");
    }

    #[test]
    fn structured_section_shape() {
        let s = format_structured_section(&reply(), &["F03-1".to_string()]);
        assert!(s.contains("### Findings"));
        assert!(s.contains("- **F03-1** [blocker] `a.rs:10` - the lock is not released."));
        assert!(s.contains("Trigger: on error."));
        assert!(s.contains("Evidence: read-code: no drop."));
        assert!(s.contains("## Verdict: REJECT"));
        assert!(s.contains("a blocker remains"));
        assert!(s.contains("### Blockers"));
        assert!(s.contains("### First-run checklist (observable)"));
        assert!(s.contains("- F01-1 - fixed - done"));
    }
}
