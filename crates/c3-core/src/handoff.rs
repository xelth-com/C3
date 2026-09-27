//! The handoff header: the metadata block `codex-consult.ps1` writes above the verbatim
//! reply (the `$headerLines` list, `.ToArray() -join "\n"`, `codex-consult.ps1:3738-3804`).
//!
//! The block is a title, a blank line, the metadata lines in a fixed order, then
//! `Verbatim reply follows.`, a blank line, `---` and a blank line, after which the
//! verbatim reply begins. [`HandoffHeader::render`] reproduces that block exactly, verified
//! in `tests/formats.rs` against the real handoff 15.
//!
//! ## Typed ordered slots (F02-13, F07-6, F08-10, F09-4)
//!
//! The plugin interleaves several *optional* records at exact points around the
//! Effort/Invocation/Bridge-outcome/Timeout/Verdict lines. This module reproduces that
//! order with one named field per record instead of a single append-anywhere bag, so each
//! optional line renders at its exact position. The order, from the plugin literal, is:
//!
//! 1. title, blank
//! 2. `Date:`/`Author:`
//! 3. `Reviewer:`
//! 4. `Preflight:` (with a `WARNING:` suffix folded into the composed line)
//! 5. `Roster:` (optional)
//! 6. `Effort: ... Consultation id: ...`
//! 7. peak warning (optional)
//! 8. recovery-record lines (one per recovered leftover)
//! 9. `Invocation: ... Argv: ...`
//! 10. `Parent thread: ... Result thread: ...`
//! 11. `Brief: ... Reviewed: ...`
//! 12. drift lines (working tree / HEAD / brief / artifacts changed)
//! 13. `Bridge outcome: ... Wall time: ... Tokens: ...`
//! 14. `Engine turns: ...` (optional)
//! 15. `Warnings: ...` (optional)
//! 16. `Denial retry: ...` (optional)
//! 17. `Timeout: ...` (with an optional ` Range: ...` suffix folded into the composed line)
//! 18. `Timeout continuation: ...` (optional)
//! 19. `Partial reply: ...` (optional)
//! 20. `Provider failure: ...` (optional)
//! 21. the structured-status / verdict line (optional; present when a reply parsed)
//! 22. verdict warning (optional)
//! 23. `Format repair: ...` (optional)
//! 24. `Raw event stream: ...` (with an optional `; further turns: ...` suffix)
//! 25. `Verbatim reply follows.`, blank, `---`, blank
//!
//! The lines whose wording is composed by identity/lineage/revision rendering elsewhere in
//! the bridge (reviewer, preflight, roster, parent/result, brief/reviewed, timeout, the
//! optional records) are carried as already-composed `String`s; the lines whose every input
//! is in the ledger (title, date/author, effort, invocation, bridge outcome, raw event
//! stream) are rendered here from their parts.

/// Who authored the reply (the `Date:`/`Author:` line has two branches: Codex, or an
/// engine such as Gemini/Muse).
#[derive(Debug, Clone)]
pub enum Author {
    /// `Author: Codex (model <model>, effort <effort>), Codex CLI <cli_version>.`
    Codex {
        model: String,
        effort: String,
        cli_version: String,
    },
    /// `Author: <label> (model <model>, effort <effort_desc>), <harness>.`
    Engine {
        label: String,
        model: String,
        effort_desc: String,
        harness: String,
    },
}

impl Author {
    fn render(&self, date: &str) -> String {
        match self {
            Author::Codex {
                model,
                effort,
                cli_version,
            } => format!(
                "Date: {date} local. Author: Codex (model {model}, effort {effort}), Codex CLI {cli_version}."
            ),
            Author::Engine {
                label,
                model,
                effort_desc,
                harness,
            } => format!(
                "Date: {date} local. Author: {label} (model {model}, effort {effort_desc}), {harness}."
            ),
        }
    }
}

/// The `Tokens:` clause of the bridge-outcome line.
#[derive(Debug, Clone)]
pub enum TokenReport {
    /// `in <in> (cached <cached>), out <out>, reasoning <reasoning>` (`Format-Usage`).
    Reported {
        input: i64,
        cached: i64,
        output: i64,
        reasoning: i64,
    },
    /// `not reported by <engine>` (an engine that reports no usage: agy/muse).
    NotReported { engine: String },
    /// `unknown` — the engine reports usage (codex) but this turn produced none (a killed
    /// turn), matching `Format-Usage` of a null usage.
    Unknown,
}

impl TokenReport {
    fn render(&self) -> String {
        match self {
            TokenReport::Reported {
                input,
                cached,
                output,
                reasoning,
            } => format!("in {input} (cached {cached}), out {output}, reasoning {reasoning}"),
            TokenReport::NotReported { engine } => format!("not reported by {engine}"),
            TokenReport::Unknown => "unknown".to_string(),
        }
    }
}

/// The optional records that interleave around the Bridge-outcome/Timeout/Verdict lines,
/// each an already-composed line (its wording is prose composed by the M2 renderer). Kept
/// as one struct so [`HandoffHeader`] stays readable; each field renders at its exact point
/// (see the module docs).
#[derive(Debug, Clone, Default)]
pub struct OptionalRecords {
    /// A peak-window warning, after the Effort line.
    pub peak_warning: Option<String>,
    /// Recovery-record lines, one per recovered leftover, after the peak warning.
    pub recovery_lines: Vec<String>,
    /// Drift lines (tree/HEAD/brief/artifacts changed), after Brief/Reviewed.
    pub drift_lines: Vec<String>,
    /// `Engine turns: ...`, after Bridge outcome.
    pub engine_turns: Option<String>,
    /// `Warnings: ...`.
    pub warnings: Option<String>,
    /// `Denial retry: ...`, immediately before the Timeout line.
    pub denial_retry: Option<String>,
    /// `Timeout continuation: ...`, immediately after the Timeout line.
    pub timeout_continuation: Option<String>,
    /// `Partial reply: ...`.
    pub partial_reply: Option<String>,
    /// `Provider failure: ...`.
    pub provider_failure: Option<String>,
    /// A verdict warning, immediately after the structured-status/verdict line.
    pub verdict_warning: Option<String>,
    /// `Format repair: ...`, after the verdict warning and before the raw event stream.
    pub format_repair: Option<String>,
}

/// The full handoff header, rendered to the exact metadata block.
#[derive(Debug, Clone)]
pub struct HandoffHeader {
    // --- title: `# Handoff <nn> - <engine_label>: <slug>` ---
    pub nn: u32,
    /// The engine's display label in the title (`Codex`, `Gemini`, `Muse`).
    pub engine_label: String,
    pub slug: String,

    // --- typed lines (fixed wording, all inputs in the ledger) ---
    /// The local timestamp `yyyy-MM-dd HH:mm` used in the `Date:` line.
    pub date: String,
    pub author: Author,
    /// `Effort: <sent> sent (requested <req>, mapping <mapping>, by <basis>; not confirmed
    /// by the provider). Consultation id: <consult_id>.`
    pub effort_sent: String,
    pub effort_requested: String,
    pub effort_mapping: String,
    /// The effort basis phrase, e.g. `caps-v1: builtin:openai, any model`.
    pub effort_basis: String,
    pub consult_id: String,
    /// `Invocation: \`codex-consult.ps1\` (mode: <mode>, sandbox: <sandbox>, purpose:
    /// <purpose>). Argv: \`<argv>\` (<prompt_via>).`
    pub mode: String,
    pub sandbox: String,
    pub purpose: String,
    pub argv: String,
    /// How the prompt reached the engine, e.g. `prompt on stdin`.
    pub prompt_via: String,
    /// `Bridge outcome: <outcome>. Wall time: <wall> s. Tokens: <tokens>.`
    pub bridge_outcome: String,
    /// Wall time as it appears (a whole value has no decimal, matching the ledger).
    pub wall_seconds: String,
    pub tokens: TokenReport,
    /// `Raw event stream: \`<events_rel>\`<further>.`
    pub events_rel: String,
    /// Extra event-stream paths for a `; further turns: \`a\`, \`b\`` suffix on the raw
    /// event-stream line; empty for a single-turn consultation.
    pub further_turns: Vec<String>,

    // --- prose lines (composed by identity/lineage/revision rendering; see module docs) ---
    /// `Reviewer: <lineage> (<provenance>; endpoint <host>; provider fingerprint <fp>;
    /// harness <harness>).`
    pub reviewer_line: String,
    /// `Preflight: <preflight>.` (a `WARNING: ...` suffix is folded into this string).
    pub preflight_line: String,
    /// `Roster: <path> - entry <i> of <n> for -Provider <p> (<applied>).`; `None` when no
    /// roster supplied the identity.
    pub roster_line: Option<String>,
    /// `Parent thread: <...>. Result thread: \`<thread>\` (source: <source>).`
    pub parent_result_line: String,
    /// `Brief: \`<brief>\` (sha256 <short>). Reviewed: <...>.`
    pub brief_reviewed_line: String,
    /// `Timeout: <sec> s (<source>); continuation after a timeout kill: up to <cont> s.`
    /// (an optional ` Range: ...` suffix is folded into this string).
    pub timeout_line: String,
    /// The structured-status / verdict line (`Verdict: <v> - <reason>. Findings: ...`);
    /// `None` when no reply parsed (the plugin adds it only `if ($parse)`).
    pub verdict_line: Option<String>,

    /// The optional records that interleave at their exact points.
    pub records: OptionalRecords,
}

impl HandoffHeader {
    /// Render the full metadata block, terminated by the `---` separator and a blank line.
    /// The verbatim reply is appended after this by the caller.
    pub fn render(&self) -> String {
        let r = &self.records;
        let mut lines: Vec<String> = Vec::new();
        // 1. title, blank
        lines.push(format!(
            "# Handoff {:02} - {}: {}",
            self.nn, self.engine_label, self.slug
        ));
        lines.push(String::new());
        // 2-6.
        lines.push(self.author.render(&self.date));
        lines.push(self.reviewer_line.clone());
        lines.push(self.preflight_line.clone());
        if let Some(rl) = &self.roster_line {
            lines.push(rl.clone());
        }
        lines.push(format!(
            "Effort: {} sent (requested {}, mapping {}, by {}; not confirmed by the provider). Consultation id: {}.",
            self.effort_sent, self.effort_requested, self.effort_mapping, self.effort_basis, self.consult_id
        ));
        // 7-8. peak warning, recovery lines
        push_opt(&mut lines, &r.peak_warning);
        lines.extend(r.recovery_lines.iter().cloned());
        // 9-11.
        lines.push(format!(
            "Invocation: `codex-consult.ps1` (mode: {}, sandbox: {}, purpose: {}). Argv: `{}` ({}).",
            self.mode, self.sandbox, self.purpose, self.argv, self.prompt_via
        ));
        lines.push(self.parent_result_line.clone());
        lines.push(self.brief_reviewed_line.clone());
        // 12. drift lines
        lines.extend(r.drift_lines.iter().cloned());
        // 13. bridge outcome
        lines.push(format!(
            "Bridge outcome: {}. Wall time: {} s. Tokens: {}.",
            self.bridge_outcome,
            self.wall_seconds,
            self.tokens.render()
        ));
        // 14-16.
        push_opt(&mut lines, &r.engine_turns);
        push_opt(&mut lines, &r.warnings);
        push_opt(&mut lines, &r.denial_retry);
        // 17-20.
        lines.push(self.timeout_line.clone());
        push_opt(&mut lines, &r.timeout_continuation);
        push_opt(&mut lines, &r.partial_reply);
        push_opt(&mut lines, &r.provider_failure);
        // 21-23.
        if let Some(v) = &self.verdict_line {
            lines.push(v.clone());
        }
        push_opt(&mut lines, &r.verdict_warning);
        push_opt(&mut lines, &r.format_repair);
        // 24. raw event stream (+ further turns)
        let further = if self.further_turns.is_empty() {
            String::new()
        } else {
            let list = self
                .further_turns
                .iter()
                .map(|p| format!("`{p}`"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("; further turns: {list}")
        };
        lines.push(format!(
            "Raw event stream: `{}`{}.",
            self.events_rel, further
        ));
        // 25. tail
        lines.push("Verbatim reply follows.".to_string());
        lines.push(String::new());
        lines.push("---".to_string());
        lines.push(String::new());
        lines.join("\n")
    }
}

fn push_opt(lines: &mut Vec<String>, line: &Option<String>) {
    if let Some(l) = line {
        lines.push(l.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_header() -> HandoffHeader {
        HandoffHeader {
            nn: 3,
            engine_label: "Codex".into(),
            slug: "x".into(),
            date: "2026-09-26 10:00".into(),
            author: Author::Codex {
                model: "m".into(),
                effort: "high".into(),
                cli_version: "0.1".into(),
            },
            effort_sent: "high".into(),
            effort_requested: "high".into(),
            effort_mapping: "openai".into(),
            effort_basis: "caps".into(),
            consult_id: "id".into(),
            mode: "new".into(),
            sandbox: "read-only".into(),
            purpose: "review".into(),
            argv: "codex exec -".into(),
            prompt_via: "prompt on stdin".into(),
            bridge_outcome: "usable reply".into(),
            wall_seconds: "10".into(),
            tokens: TokenReport::NotReported {
                engine: "codex".into(),
            },
            events_rel: "handoffs/03-x.events.jsonl".into(),
            further_turns: vec![],
            reviewer_line: "Reviewer: x.".into(),
            preflight_line: "Preflight: ok.".into(),
            roster_line: None,
            parent_result_line: "Parent thread: none. Result thread: `t`.".into(),
            brief_reviewed_line: "Brief: `b`. Reviewed: HEAD.".into(),
            timeout_line: "Timeout: 1800 s.".into(),
            verdict_line: Some("Verdict: ACCEPT.".into()),
            records: OptionalRecords::default(),
        }
    }

    #[test]
    fn optional_records_render_at_their_points() {
        let mut h = base_header();
        h.records = OptionalRecords {
            denial_retry: Some("Denial retry: succeeded.".into()),
            timeout_continuation: Some("Timeout continuation: usable reply.".into()),
            format_repair: Some("Format repair: succeeded.".into()),
            ..Default::default()
        };
        let out = h.render();
        let lines: Vec<&str> = out.lines().collect();
        let idx = |p: &str| lines.iter().position(|l| l.starts_with(p)).unwrap();
        // At least three optional records rendered, each at its exact point:
        // denial retry BEFORE the Timeout line; timeout continuation right AFTER it;
        // format repair AFTER the verdict line and before the raw event stream.
        assert!(idx("Denial retry:") < idx("Timeout:"));
        assert!(idx("Timeout:") < idx("Timeout continuation:"));
        assert!(idx("Timeout continuation:") < idx("Verdict:"));
        assert!(idx("Verdict:") < idx("Format repair:"));
        assert!(idx("Format repair:") < idx("Raw event stream:"));
        assert!(idx("Raw event stream:") < idx("Verbatim reply follows."));
    }

    #[test]
    fn further_turns_suffix() {
        let mut h = base_header();
        h.further_turns = vec!["handoffs/03-x.cont.jsonl".into()];
        let out = h.render();
        assert!(out.contains("Raw event stream: `handoffs/03-x.events.jsonl`; further turns: `handoffs/03-x.cont.jsonl`."));
    }
}
