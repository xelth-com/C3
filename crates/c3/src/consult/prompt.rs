//! Prompt assembly (`codex-consult.ps1:2352-2429`).
//!
//! The structured-mode prompt OPENS with the fixed FINAL OUTPUT CONTRACT paragraph, then the
//! ask, the brief pointer, an optional range line, the purpose paragraph, the open-findings
//! snapshot, the reply-format/schema block (with the schema text inlined when the transport
//! is prompt-only), the constraints line, and always ends with `Consultation id: <id>`. A
//! `-Raw` (or `-Purpose chore`) prompt carries none of the structured scaffolding.
//!
//! Parts are joined by a blank line; the line ending is CRLF (`$nl = "\r\n"`), matching the
//! plugin so the prompt travels byte-for-byte.

/// The per-purpose presets (`$presetEffort`/`$presetWords`/`$presetTimeout`, purpose text).
pub mod presets {
    /// The valid purposes.
    pub const VALID: [&str; 8] = [
        "framing",
        "decision",
        "checkpoint",
        "core-contract",
        "acceptance",
        "diff-review",
        "stuck",
        "chore",
    ];

    /// The default effort for a purpose (`""` = no purpose).
    pub fn effort(purpose: &str) -> &'static str {
        match purpose {
            "framing" | "decision" | "acceptance" | "diff-review" => "high",
            "checkpoint" => "medium",
            "core-contract" | "stuck" => "xhigh",
            "chore" => "low",
            _ => "high",
        }
    }

    /// The default word budget for a purpose.
    pub fn words(purpose: &str) -> u32 {
        match purpose {
            "checkpoint" => 500,
            "core-contract" | "acceptance" => 900,
            "chore" => 400,
            _ => 700,
        }
    }

    /// The default timeout (seconds) for a purpose (`""` = no purpose → 900).
    pub fn timeout(purpose: &str) -> i64 {
        match purpose {
            "framing" | "decision" => 1800,
            "core-contract" | "diff-review" | "stuck" => 2400,
            "acceptance" => 3600,
            "chore" => 600,
            _ => 900,
        }
    }

    /// The purpose paragraph appended to the prompt.
    pub fn text(purpose: &str) -> &'static str {
        match purpose {
            "framing" => "Surface the options the brief does not list and challenge the framing itself before answering inside it. Say what you would need to know to choose between the options.",
            "decision" => "Rank the alternatives. For each one, name the deciding factor and its failure mode.",
            "checkpoint" => "Verify the CURRENT invariants the brief claims against the code as it is now, and report any drift between what is claimed and what exists. Keep it short.",
            "core-contract" => "This review runs BEFORE dependent work is built on the core. Cover the interfaces, recovery and persistence paths and the state machines NAMED IN THE BRIEF and their immediate dependency boundaries - not the whole system. For each state machine: states, transitions, the failure at each transition. Name what the evidence does not show. Expect this review to be re-run whenever recovery, persistence or interfaces change.",
            "acceptance" => "Decide whether the result can be accepted. The verdict, the blockers, the unproven scenarios and an OBSERVABLE first-run checklist (what must be seen in logs or output on the first real run before an exit code 0 is believed) are mandatory.",
            "diff-review" => "Read the change adversarially: what breaks, what it does not cover, what the tests do not prove.",
            "stuck" => "The coordinator is stuck. Look for the angle they are missing and question their assumptions before proposing fixes.",
            "chore" => "This is a chore: a bounded search or extraction task. Report facts with file paths and line numbers, quote what you found, say what you did not find. No verdict, no findings, no recommendations beyond the ask.",
            _ => "",
        }
    }

    /// The verdict rule phrase for a purpose (`acceptance`/`diff-review` allow the full set).
    pub fn verdict_rule(purpose: &str) -> &'static str {
        if purpose == "acceptance" || purpose == "diff-review" {
            "ACCEPT, HOLD or REJECT"
        } else {
            "ADVISE"
        }
    }
}

/// The fixed opening paragraph of a structured-mode prompt.
pub const FINAL_OUTPUT_CONTRACT: &str = "FINAL OUTPUT CONTRACT: your ENTIRE final message must be exactly one bare JSON object (schema_version \"1\") - no code fence, no text before or after it. The Markdown answer lives only inside its reply_markdown string; each defect goes in findings[]. A prose final message cannot be ingested, however good the answer is.";

/// One open finding as it appears in the prompt's snapshot.
pub struct OpenFinding {
    pub id: String,
    pub status: String,
    /// Already-rendered `Format-Locations` string.
    pub locations: String,
    pub claim: String,
    pub trigger: String,
    pub verification: String,
}

/// A measured range for the range line.
pub struct Range {
    pub spec: String,
    /// The `<a>..<b>` / `<a>...<b>` human text.
    pub text: String,
    pub insertions: i64,
    pub deletions: i64,
}

/// Everything the prompt needs.
pub struct PromptInputs<'a> {
    pub raw: bool,
    pub purpose: &'a str,
    pub prompt: &'a str,
    /// The repo-relative brief path (empty when none).
    pub brief_ref: &'a str,
    pub range: Option<&'a Range>,
    pub open_findings: &'a [OpenFinding],
    /// `output-schema` | `native` | `prompt-only`.
    pub schema_transport: &'a str,
    /// The reply schema text, inlined only when the transport is prompt-only.
    pub schema_text: &'a str,
    pub max_words: u32,
    pub consult_id: &'a str,
    /// The engine's `Tools:` line (D11), added after the ask/brief/range for a non-codex
    /// engine; empty for codex.
    pub tools_line: &'a str,
}

const NL: &str = "\r\n";

/// Assemble the full prompt text.
pub fn assemble(inp: &PromptInputs) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !inp.raw {
        parts.push(FINAL_OUTPUT_CONTRACT.to_string());
    }
    if !inp.prompt.is_empty() {
        parts.push(inp.prompt.trim().to_string());
    }
    if !inp.brief_ref.is_empty() {
        parts.push(format!(
            "Read the brief at `{}` (path relative to the repository root, which is your working directory) and answer every numbered question in it.",
            inp.brief_ref
        ));
    }
    if let Some(r) = inp.range {
        parts.push(format!(
            "Review range: `{}` - {} ({} insertions, {} deletions; git diff --shortstat). Plan your reading for its size.",
            r.spec, r.text, r.insertions, r.deletions
        ));
    }
    // The engine's own tools line (D11), after the ask/brief/range; empty for codex.
    if !inp.tools_line.is_empty() {
        parts.push(inp.tools_line.to_string());
    }
    if inp.raw {
        if inp.purpose == "chore" {
            parts.push(format!("Review purpose: chore. {}", presets::text("chore")));
        }
        parts.push(format!(
            "Constraints: write NO files and make no edits - this is a read-only consultation; answer in English; keep the answer under {} words.",
            inp.max_words
        ));
    } else {
        if !inp.purpose.is_empty() {
            parts.push(format!(
                "Review purpose: {}. {}",
                inp.purpose,
                presets::text(inp.purpose)
            ));
        }
        if !inp.open_findings.is_empty() {
            parts.push(render_open_findings(inp.open_findings));
        }
        parts.push(schema_block(inp));
        if inp.schema_transport == "prompt-only" {
            parts.push(format!("JSON Schema of the reply:{NL}{}", inp.schema_text));
        }
        parts.push(format!(
            "Constraints: write NO files and make no edits - this is a read-only consultation; answer in English; keep reply_markdown under {} words.",
            inp.max_words
        ));
    }
    parts.push(format!("Consultation id: {}", inp.consult_id));
    parts.join(&format!("{NL}{NL}"))
}

fn render_open_findings(open: &[OpenFinding]) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push("Findings from earlier consultations in this task that are still open (id - status - locations - claim - trigger - verify):".to_string());
    for f in open {
        let mut line = format!(
            "- {} - {} - {} - {}",
            f.id,
            f.status,
            f.locations,
            c3_core::one_line(&f.claim)
        );
        let trigger = c3_core::one_line(&f.trigger);
        if !trigger.is_empty() {
            line.push_str(&format!(" - trigger: {trigger}"));
        }
        let verify = c3_core::one_line(&f.verification);
        if !verify.is_empty() {
            line.push_str(&format!(" - verify: {verify}"));
        }
        lines.push(line);
    }
    lines.push("Report each listed id in `prior_findings` as fixed, still-open or not-checked (unknown-id if you cannot find it). Do not file a still-open one again as a new finding unless the claim changed; when a new finding replaces a listed one (a split, a merge, a corrected claim), name the old id in its `supersedes`.".to_string());
    lines.join(NL)
}

/// The `$schemaLines` block on its own, for a secondary turn (the timeout continuation on a
/// prompt-only transport re-sends the reply format, `codex-consult.ps1:3695`). `has_open` picks
/// the `prior_findings` line; `prompt_only` picks the first line's wording.
pub(crate) fn schema_lines(purpose: &str, has_open: bool, prompt_only: bool) -> String {
    // A minimal PromptInputs is enough: `schema_block` reads only `purpose`, whether
    // `open_findings` is empty, and `schema_transport`.
    let one = [OpenFinding {
        id: String::new(),
        status: String::new(),
        locations: String::new(),
        claim: String::new(),
        trigger: String::new(),
        verification: String::new(),
    }];
    let inp = PromptInputs {
        raw: false,
        purpose,
        prompt: "",
        brief_ref: "",
        range: None,
        open_findings: if has_open { &one } else { &[] },
        schema_transport: if prompt_only {
            "prompt-only"
        } else {
            "output-schema"
        },
        schema_text: "",
        max_words: 0,
        consult_id: "",
        tools_line: "",
    };
    schema_block(&inp)
}

fn schema_block(inp: &PromptInputs) -> String {
    let prior_line = if inp.open_findings.is_empty() {
        "- prior_findings: an empty array (no earlier findings are open in this task)."
    } else {
        "- prior_findings: one entry {id, status, note} per id listed above; status fixed | still-open | not-checked | unknown-id."
    };
    let first = if inp.schema_transport == "prompt-only" {
        "Reply format: your final message must be exactly one JSON object - no code fence, no text before or after it - that satisfies the JSON Schema given at the end of this section (schema_version \"1\"). Field meaning:"
    } else {
        "Reply format: your final message must be exactly one JSON object matching the output schema you were given (schema_version \"1\"). Field meaning:"
    };
    let verdict_line = format!(
        "- verdict: ACCEPT, HOLD or REJECT for acceptance and diff-review consultations, ADVISE for every other consultation; this one takes {}. verdict_reason: one sentence.",
        presets::verdict_rule(inp.purpose)
    );
    let lines: Vec<&str> = vec![
        first,
        "- reply_markdown: your full answer in Markdown, answering every numbered question by number. This is what people read - a complete Markdown answer, but it lives INSIDE the JSON string, never as the message itself. The word limit below applies to reply_markdown only - never shorten, merge or drop findings to fit it.",
        "  If you want evidence you cannot obtain read-only, end reply_markdown with a section `## Requested checks` listing at most 5 items `RC1`..`RCn`, each ONE runnable command or procedure with its working directory, the permission it needs (read-only / workspace-write), the observation that would settle it, and a budget (time or scope); refer to a finding by its position in your findings array (`finding #2`), by an earlier id (`F04-1`) or by the invariant name. \"Investigate X\" is not a check. Omit the section if you need nothing.",
        "- findings: one item per concrete defect or risk you assert; an empty array is a valid answer.",
        "  - severity: blocker (must be fixed before acceptance) | major | minor | note.",
        "  - locations: every place the finding concerns, each {path, line} with the path relative to the repository root and line null when no single line applies; an empty array when the finding is not tied to a place (e.g. a missing interface).",
        "  - claim: the assertion, self-contained. trigger: the input or state that exposes it.",
        "  - evidence: what you ALREADY did to support the claim, one entry per source: kind (read-code: you read the code there | ran-command: you ran something and saw the result | inferred: deduced from other evidence | assumed: not checked), reference (the file, command or log you looked at), observation (what you saw there). At least one entry; use kind \"assumed\" when you checked nothing.",
        "  - verification: one step the coordinator can run next to confirm the claim (prospective - not what you already did).",
        "  - remedy: the fix you propose.",
        "  - supersedes: ids of earlier findings this one replaces (a split, a merge, a corrected claim); otherwise an empty array.",
        &verdict_line,
        prior_line,
        "- unproven: scenarios the evidence does not cover (an empty array if none).",
        "- first_run_checklist: for an acceptance consultation, what must be observed in logs or output on the first real run before an exit code 0 is believed; an empty array for other purposes.",
        "- schema_version: always \"1\".",
    ];
    lines.join(NL)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base<'a>() -> PromptInputs<'a> {
        PromptInputs {
            raw: false,
            purpose: "framing",
            prompt: "x",
            brief_ref: ".collab/t/handoffs/01.md",
            range: None,
            open_findings: &[],
            schema_transport: "output-schema",
            schema_text: "",
            max_words: 700,
            consult_id: "abc-123",
            tools_line: "",
        }
    }

    #[test]
    fn structured_opens_with_contract_and_ends_with_id() {
        let p = assemble(&base());
        assert!(p.starts_with(FINAL_OUTPUT_CONTRACT));
        assert!(p.ends_with("Consultation id: abc-123"));
        assert!(p.contains("Read the brief at `.collab/t/handoffs/01.md`"));
        assert!(p.contains("Review purpose: framing."));
        assert!(p.contains("keep reply_markdown under 700 words"));
        // output-schema transport: the schema text is NOT inlined.
        assert!(!p.contains("JSON Schema of the reply:"));
    }

    #[test]
    fn raw_has_no_contract_or_schema() {
        let mut i = base();
        i.raw = true;
        i.purpose = "";
        let p = assemble(&i);
        assert!(!p.starts_with(FINAL_OUTPUT_CONTRACT));
        assert!(!p.contains("Reply format:"));
        assert!(p.contains("keep the answer under 700 words"));
        assert!(p.ends_with("Consultation id: abc-123"));
    }

    #[test]
    fn chore_raw_carries_purpose_paragraph() {
        let mut i = base();
        i.raw = true;
        i.purpose = "chore";
        i.max_words = 400;
        let p = assemble(&i);
        assert!(p.contains("Review purpose: chore. This is a chore"));
    }

    #[test]
    fn prompt_only_inlines_schema_text() {
        let mut i = base();
        i.schema_transport = "prompt-only";
        i.schema_text = "{\"$schema\":\"x\"}";
        let p = assemble(&i);
        assert!(p.contains("JSON Schema of the reply:"));
        assert!(p.contains("{\"$schema\":\"x\"}"));
        assert!(p.contains("satisfies the JSON Schema given at the end of this section"));
    }

    #[test]
    fn open_findings_snapshot_rendered() {
        let of = vec![OpenFinding {
            id: "F01-1".into(),
            status: "proposed".into(),
            locations: "`a.rs:5`".into(),
            claim: "a claim".into(),
            trigger: "on start".into(),
            verification: "run it".into(),
        }];
        let mut i = base();
        i.open_findings = &of;
        let p = assemble(&i);
        assert!(p.contains("Findings from earlier consultations in this task that are still open"));
        assert!(p.contains(
            "- F01-1 - proposed - `a.rs:5` - a claim - trigger: on start - verify: run it"
        ));
        assert!(p.contains("one entry {id, status, note} per id listed above"));
    }

    #[test]
    fn crlf_paragraph_joins() {
        let p = assemble(&base());
        assert!(p.contains("\r\n\r\n"));
    }
}
