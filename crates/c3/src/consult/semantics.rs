//! Reply semantics (`Test-ReplySemantics`, `Get-PriorBlockerDispositions`,
//! `codex-consult-common.ps1:1173`/`1204`): the verdict-vs-purpose gate (F04-6) and the
//! ACCEPT-vs-blocker / ACCEPT-vs-still-open-prior-blocker contradictions (F04-4).
//!
//! These are pure: they take the validated reply, the run's purpose and the *open* prior
//! blockers (the findings already in `findings.json` at `blocker` severity and `proposed`/
//! `implemented` status), and return the problems (which blank the verdict), the
//! `unchecked_prior_blockers` and the optional operator WARNING.

use c3_core::engine::{PriorStatus, Severity, StructuredReply, Verdict};

/// A prior blocker's disposition according to this reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriorBlocker {
    pub id: String,
    /// `fixed | still-open | not-checked`.
    pub disposition: String,
}

/// The open prior blockers a run carries: `(id, is_blocker, status)` for each finding that is
/// still `proposed`/`implemented` in `findings.json`. Only the `blocker`-severity ones matter.
#[derive(Debug, Clone)]
pub struct OpenPrior {
    pub id: String,
    pub severity: String,
    pub status: String,
}

fn verdict_token(v: Verdict) -> &'static str {
    match v {
        Verdict::Accept => "ACCEPT",
        Verdict::Hold => "HOLD",
        Verdict::Reject => "REJECT",
        Verdict::Advise => "ADVISE",
    }
}

/// The verdict vocabulary a purpose allows: `ACCEPT|HOLD|REJECT` for acceptance/diff-review,
/// else `ADVISE` (`Get-VerdictRule`). Returns `(allowed tokens, expected label)`.
fn purpose_verdicts(purpose: &str) -> (&'static [&'static str], &'static str) {
    match purpose {
        "acceptance" | "diff-review" => (&["ACCEPT", "HOLD", "REJECT"], "ACCEPT|HOLD|REJECT"),
        _ => (&["ADVISE"], "ADVISE"),
    }
}

/// The verdict-fits-purpose error (F04-6), or `None`. `purpose` empty renders as `none`.
pub fn verdict_purpose_error(purpose: &str, verdict: Verdict) -> Option<String> {
    let (allowed, expected) = purpose_verdicts(purpose);
    let v = verdict_token(verdict);
    if allowed.contains(&v) {
        return None;
    }
    let p = if purpose.is_empty() { "none" } else { purpose };
    Some(format!(
        "verdict {v} is not allowed for purpose {p} (expected {expected})"
    ))
}

/// `Get-PriorBlockerDispositions`: for each OPEN prior blocker, this reply's disposition
/// (`fixed`/`still-open`/`not-checked`). A `still-open` report on an id sticks (a later
/// `fixed` for the same id does not override it).
pub fn prior_blocker_dispositions(
    reply: &StructuredReply,
    open: &[OpenPrior],
) -> Vec<PriorBlocker> {
    use std::collections::HashMap;
    let mut reported: HashMap<&str, &str> = HashMap::new();
    for p in &reply.prior_findings {
        let s = match p.status {
            PriorStatus::Fixed => "fixed",
            PriorStatus::StillOpen => "still-open",
            PriorStatus::NotChecked => "not-checked",
            PriorStatus::UnknownId => "unknown-id",
        };
        match reported.get(p.id.as_str()) {
            Some(&"still-open") => {}
            _ => {
                reported.insert(p.id.as_str(), s);
            }
        }
    }
    let mut out = Vec::new();
    for f in open {
        if f.severity != "blocker" {
            continue;
        }
        if f.status != "proposed" && f.status != "implemented" {
            continue;
        }
        let disposition = match reported.get(f.id.as_str()) {
            Some(&"still-open") => "still-open",
            Some(&"fixed") => "fixed",
            _ => "not-checked",
        };
        out.push(PriorBlocker {
            id: f.id.clone(),
            disposition: disposition.to_string(),
        });
    }
    out
}

/// The result of the semantic checks.
#[derive(Debug, Clone, Default)]
pub struct Semantics {
    /// The problems that BLANK the verdict (joined into `validation_error`).
    pub problems: Vec<String>,
    /// The `unchecked_prior_blockers` ids (blockers this ACCEPT did not report on).
    pub unchecked: Vec<String>,
    /// The operator WARNING when ACCEPT is KEPT but leaves unchecked prior blockers.
    pub warning: Option<String>,
}

impl Semantics {
    pub fn verdict_invalid(&self) -> bool {
        !self.problems.is_empty()
    }
    /// The joined `validation_error` for a semantically-invalid reply (empty when valid).
    pub fn validation_error(&self) -> String {
        self.problems.join("; ")
    }
}

/// `Test-ReplySemantics`: the verdict-purpose gate plus, for ACCEPT, the new-blocker and
/// still-open-prior-blocker contradictions and the unchecked-prior-blocker bookkeeping.
pub fn test_reply_semantics(
    reply: &StructuredReply,
    purpose: &str,
    open: &[OpenPrior],
) -> Semantics {
    let mut s = Semantics::default();
    if let Some(e) = verdict_purpose_error(purpose, reply.verdict) {
        s.problems.push(e);
    }
    if reply.verdict == Verdict::Accept {
        let new_blockers = reply
            .findings
            .iter()
            .filter(|f| f.severity == Severity::Blocker)
            .count();
        if new_blockers > 0 {
            s.problems.push(format!(
                "verdict ACCEPT contradicts {new_blockers} blocker finding(s)"
            ));
        }
        let dispositions = prior_blocker_dispositions(reply, open);
        let still_open: Vec<String> = dispositions
            .iter()
            .filter(|d| d.disposition == "still-open")
            .map(|d| d.id.clone())
            .collect();
        if !still_open.is_empty() {
            s.problems.push(format!(
                "verdict ACCEPT contradicts still-open prior blocker {}",
                still_open.join(", ")
            ));
        }
        s.unchecked = dispositions
            .iter()
            .filter(|d| d.disposition == "not-checked")
            .map(|d| d.id.clone())
            .collect();
        // ACCEPT is KEPT (no problem) yet left prior blockers unchecked: an operator WARNING.
        if s.problems.is_empty() && !s.unchecked.is_empty() {
            s.warning = Some(format!(
                "ACCEPT with {} unchecked prior blocker(s) ({}).",
                s.unchecked.len(),
                s.unchecked.join(", ")
            ));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use c3_core::engine::{ReplyFinding, ReplyLocation, ReplyPriorFinding};

    fn reply(verdict: Verdict) -> StructuredReply {
        StructuredReply {
            verdict,
            verdict_reason: "r".into(),
            reply_markdown: "m".into(),
            findings: vec![],
            prior_findings: vec![],
            unproven: vec![],
            first_run_checklist: vec![],
        }
    }

    #[test]
    fn verdict_purpose_gate() {
        assert_eq!(
            verdict_purpose_error("acceptance", Verdict::Advise).as_deref(),
            Some("verdict ADVISE is not allowed for purpose acceptance (expected ACCEPT|HOLD|REJECT)")
        );
        assert!(verdict_purpose_error("diff-review", Verdict::Reject).is_none());
        assert_eq!(
            verdict_purpose_error("framing", Verdict::Hold).as_deref(),
            Some("verdict HOLD is not allowed for purpose framing (expected ADVISE)")
        );
        assert!(verdict_purpose_error("", Verdict::Accept)
            .unwrap()
            .contains("purpose none"));
        assert!(verdict_purpose_error("", Verdict::Advise).is_none());
    }

    #[test]
    fn accept_contradicts_new_blocker() {
        let mut r = reply(Verdict::Accept);
        r.findings.push(ReplyFinding {
            severity: Severity::Blocker,
            locations: vec![ReplyLocation {
                path: "a".into(),
                line: None,
            }],
            claim: "c".into(),
            trigger: String::new(),
            evidence: vec![],
            verification: String::new(),
            remedy: String::new(),
            supersedes: vec![],
        });
        let s = test_reply_semantics(&r, "acceptance", &[]);
        assert!(s.verdict_invalid());
        assert!(s
            .validation_error()
            .contains("verdict ACCEPT contradicts 1 blocker finding(s)"));
    }

    #[test]
    fn accept_contradicts_still_open_prior_blocker() {
        let mut r = reply(Verdict::Accept);
        r.prior_findings.push(ReplyPriorFinding {
            id: "F02-1".into(),
            status: PriorStatus::StillOpen,
            note: String::new(),
        });
        let open = vec![OpenPrior {
            id: "F02-1".into(),
            severity: "blocker".into(),
            status: "proposed".into(),
        }];
        let s = test_reply_semantics(&r, "acceptance", &open);
        assert_eq!(
            s.validation_error(),
            "verdict ACCEPT contradicts still-open prior blocker F02-1"
        );
    }

    #[test]
    fn accept_with_unchecked_prior_blocker_keeps_verdict_but_warns() {
        // The prior blocker is not reported at all -> ACCEPT kept, WARNING + unchecked recorded.
        let r = reply(Verdict::Accept);
        let open = vec![OpenPrior {
            id: "F02-1".into(),
            severity: "blocker".into(),
            status: "proposed".into(),
        }];
        let s = test_reply_semantics(&r, "diff-review", &open);
        assert!(!s.verdict_invalid());
        assert_eq!(s.unchecked, vec!["F02-1".to_string()]);
        assert_eq!(
            s.warning.as_deref(),
            Some("ACCEPT with 1 unchecked prior blocker(s) (F02-1).")
        );
    }

    #[test]
    fn fixed_prior_blocker_is_neither_unchecked_nor_a_problem() {
        let mut r = reply(Verdict::Accept);
        r.prior_findings.push(ReplyPriorFinding {
            id: "F02-1".into(),
            status: PriorStatus::Fixed,
            note: String::new(),
        });
        let open = vec![OpenPrior {
            id: "F02-1".into(),
            severity: "blocker".into(),
            status: "implemented".into(),
        }];
        let s = test_reply_semantics(&r, "diff-review", &open);
        assert!(!s.verdict_invalid());
        assert!(s.unchecked.is_empty());
        assert!(s.warning.is_none());
    }
}
