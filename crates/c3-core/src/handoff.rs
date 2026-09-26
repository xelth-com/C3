//! The handoff header: the metadata block `codex-consult.ps1` writes above the verbatim
//! reply (the `$headerLines` list, `.ToArray() -join "\n"`).
//!
//! The block is: a title, a blank line, then the metadata lines in a fixed order, then
//! `Verbatim reply follows.`, a blank line, `---` and a blank line, after which the
//! verbatim reply begins. [`HandoffHeader::render`] reproduces that block exactly. It is
//! verified in `tests/formats.rs` against the real handoff 15
//! (`.collab/c3-design/handoffs/15-codex-merge-framing.md`).
//!
//! Which lines are typed vs. carried as prose, and why: the wording of several lines is a
//! fixed template whose every input is in the ledger entry (author, effort, invocation,
//! bridge outcome, raw event stream); those are typed here and rendered from their parts.
//! Other lines are prose composed by identity resolution, lineage walking and revision
//! fingerprinting done elsewhere in the bridge (reviewer, preflight, roster, parent/result,
//! brief/reviewed, timeout, verdict); the ledger entry does not carry enough to rebuild
//! their wording (see `docs/port/contracts.md` for the exact list of non-ledger inputs),
//! so they are held as `String` and composed by the M2 renderer. The optional records
//! (engine turns, warnings, denial retry, timeout continuation, partial reply, provider
//! failure, format repair) interleave around the timeout/verdict lines; they are modelled
//! as [`HandoffHeader::extra`] appended before the verdict line for now (M2 places each at
//! its exact point) - handoff 15 has none, so the acceptance render is exact.

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
    /// `not reported by <engine>`.
    NotReported { engine: String },
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
        }
    }
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
    /// `Raw event stream: \`<events_rel>\`.`
    pub events_rel: String,

    // --- prose lines (composed by identity/lineage/revision rendering; see module docs) ---
    /// `Reviewer: <lineage> (<provenance>; endpoint <host>; provider fingerprint <fp>;
    /// harness <harness>).`
    pub reviewer_line: String,
    /// `Preflight: <preflight>.`
    pub preflight_line: String,
    /// `Roster: <path> - entry <i> of <n> for -Provider <p> (<applied>).`; `None` when no
    /// roster supplied the identity.
    pub roster_line: Option<String>,
    /// `Parent thread: <...>. Result thread: \`<thread>\` (source: <source>).`
    pub parent_result_line: String,
    /// `Brief: \`<brief>\` (sha256 <short>). Reviewed: <...>.`
    pub brief_reviewed_line: String,
    /// `Timeout: <sec> s (<source>); continuation after a timeout kill: up to <cont> s.`
    pub timeout_line: String,
    /// `Verdict: <verdict> - <reason>. Findings: <counts> (<id-range>, tracked in
    /// \`findings.json\`). Structured reply: \`<reply_json>\`.`
    pub verdict_line: String,

    /// Optional records that interleave around timeout/verdict; empty for handoff 15.
    pub extra: Vec<String>,
}

impl HandoffHeader {
    /// Render the full metadata block, terminated by the `---` separator and a blank line.
    /// The verbatim reply is appended after this by the caller.
    pub fn render(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        lines.push(format!(
            "# Handoff {} - {}: {}",
            self.nn, self.engine_label, self.slug
        ));
        lines.push(String::new());
        lines.push(self.author.render(&self.date));
        lines.push(self.reviewer_line.clone());
        lines.push(self.preflight_line.clone());
        if let Some(r) = &self.roster_line {
            lines.push(r.clone());
        }
        lines.push(format!(
            "Effort: {} sent (requested {}, mapping {}, by {}; not confirmed by the provider). Consultation id: {}.",
            self.effort_sent, self.effort_requested, self.effort_mapping, self.effort_basis, self.consult_id
        ));
        lines.push(format!(
            "Invocation: `codex-consult.ps1` (mode: {}, sandbox: {}, purpose: {}). Argv: `{}` ({}).",
            self.mode, self.sandbox, self.purpose, self.argv, self.prompt_via
        ));
        lines.push(self.parent_result_line.clone());
        lines.push(self.brief_reviewed_line.clone());
        lines.push(format!(
            "Bridge outcome: {}. Wall time: {} s. Tokens: {}.",
            self.bridge_outcome,
            self.wall_seconds,
            self.tokens.render()
        ));
        lines.push(self.timeout_line.clone());
        lines.extend(self.extra.iter().cloned());
        lines.push(self.verdict_line.clone());
        lines.push(format!("Raw event stream: `{}`.", self.events_rel));
        lines.push("Verbatim reply follows.".to_string());
        lines.push(String::new());
        lines.push("---".to_string());
        lines.push(String::new());
        lines.join("\n")
    }
}
