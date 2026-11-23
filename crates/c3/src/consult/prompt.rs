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

    /// The purpose paragraph appended to the prompt. (wave 8 = plugin wave 30, T15) The paragraphs
    /// of the reviewing purposes (checkpoint, core-contract, acceptance, diff-review) end with the
    /// reading discipline ([`c3_core::quota::READING_DISCIPLINE`]): a reviewer that prints whole
    /// files replays them on every later tool call of its thread.
    pub fn text(purpose: &str) -> String {
        let base = base_text(purpose);
        if c3_core::quota::READING_DISCIPLINE_PURPOSES.contains(&purpose) {
            format!("{base} {}", c3_core::quota::READING_DISCIPLINE)
        } else {
            base.to_string()
        }
    }

    /// The purpose paragraph without the reading discipline.
    fn base_text(purpose: &str) -> &'static str {
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
#[derive(Debug, Clone, Default)]
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
#[derive(Debug, Clone, Default)]
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
    /// (0.2.4) The brief's text, inlined after the "Read the brief at" line for an engine whose
    /// spec says `inline_brief` (grok); empty otherwise.
    pub brief_text: &'a str,
    pub range: Option<&'a Range>,
    /// (B1) The `--diff` line ("The change under review is the patch at <path> ..."), after the
    /// brief and the range; empty without `--diff`.
    pub diff_line: &'a str,
    pub open_findings: &'a [OpenFinding],
    /// (0.2.5 R5, F18-1 / F19-6) How many open findings of the task are NOT listed because they
    /// were recorded against another revision than the one under review (`--rev`); `0` otherwise.
    /// With `open_findings` empty and this > 0, one sentence says so and the reply format asks for
    /// an empty `prior_findings`.
    pub withheld_findings: usize,
    /// `output-schema` | `native` | `prompt-only`.
    pub schema_transport: &'a str,
    /// The reply schema text, inlined only when the transport is prompt-only.
    pub schema_text: &'a str,
    pub max_words: u32,
    pub consult_id: &'a str,
    /// The engine's `Tools:` line (D11), added after the ask/brief/range for a non-codex
    /// engine; empty for codex.
    pub tools_line: &'a str,
    /// (A1) A worktree-isolated reviewer's (kimi) `Isolation:` paragraph - the live tree's
    /// uncommitted paths at launch, which its throwaway worktree of HEAD does not hold
    /// ([`crate::engines::worktree::isolation_prompt_line`]); added after the tools line. Empty for
    /// every other engine and for a clean tree.
    pub isolation_line: &'a str,
    /// A panel member's role paragraph (`role_prompt_line`), added after the ask and before the
    /// brief (`codex-consult.ps1:3555`); empty when the run has no role.
    pub role_line: &'a str,
    /// (wave 26b, D16) the reviewer's context-window line, added right after the ask when its
    /// roster entry names a `context_tokens`; empty otherwise.
    pub context_line: &'a str,
    /// (wave 26b, D16) the "your previous reply is <path>" line, added when a fork/resume was
    /// downgraded to a new thread because the previous one is too large; empty otherwise.
    pub prev_reply_line: &'a str,
    /// (wave 28c D11, 28d D7, 28e E4) the re-read anchor ([`reread_line`]), the last line before
    /// the consultation id; empty otherwise.
    pub reread_line: &'a str,
    /// (C3) An engine that COUNTS tool calls (grok, kimi) on a reviewing purpose: one extra sentence
    /// after the purpose paragraph makes the tool-reading mandatory (a zero-tool-call reply is
    /// rejected and re-asked). Never set for codex (byte-parity) or an engine that counts no tools.
    pub tool_reading_required: bool,
    /// (C3) The grok reading-duty paragraph ([`GROK_READING_DUTY`]): added for engine `grok` on a
    /// non-raw, NON-review purpose (the four review purposes already carry the reading discipline).
    /// A zero-tool-call reply is not failed there, but the orchestrator soft-re-asks once.
    pub reading_duty: bool,
    /// (C3) The grok answer-once paragraph ([`GROK_ANSWER_ONCE`]): added for engine `grok` on any
    /// non-raw purpose (review or not). Under `--json-schema` Grok may emit a schema-shaped PROGRESS
    /// note before it reads and end the turn on it; this tells it to write the answer exactly once,
    /// last, so a placeholder is never taken as the final reply.
    pub grok_answer_once: bool,
}

/// (0.2.5 R5) The prompt's inputs, owned: the orchestrator assembles the prompt from them when it
/// plans the run and again once the run's throwaway worktree and `--diff` patch exist - the patch's
/// content-named path and redaction count, what was staged and which links the checkout lost are
/// known only then; nothing else changes.
#[derive(Debug, Clone, Default)]
pub struct PromptPlan {
    pub raw: bool,
    pub purpose: String,
    pub prompt: String,
    pub brief_ref: String,
    pub brief_text: String,
    pub range: Option<Range>,
    pub diff_line: String,
    pub open_findings: Vec<OpenFinding>,
    pub withheld_findings: usize,
    pub schema_transport: String,
    pub schema_text: String,
    pub max_words: u32,
    pub consult_id: String,
    pub tools_line: String,
    pub isolation_line: String,
    pub role_line: String,
    pub context_line: String,
    pub prev_reply_line: String,
    pub reread_line: String,
    pub tool_reading_required: bool,
    pub reading_duty: bool,
    pub grok_answer_once: bool,
}

impl PromptPlan {
    /// The borrowed inputs of [`assemble`].
    pub fn inputs(&self) -> PromptInputs<'_> {
        PromptInputs {
            raw: self.raw,
            purpose: &self.purpose,
            prompt: &self.prompt,
            brief_ref: &self.brief_ref,
            brief_text: &self.brief_text,
            range: self.range.as_ref(),
            diff_line: &self.diff_line,
            open_findings: &self.open_findings,
            withheld_findings: self.withheld_findings,
            schema_transport: &self.schema_transport,
            schema_text: &self.schema_text,
            max_words: self.max_words,
            consult_id: &self.consult_id,
            tools_line: &self.tools_line,
            isolation_line: &self.isolation_line,
            role_line: &self.role_line,
            context_line: &self.context_line,
            prev_reply_line: &self.prev_reply_line,
            reread_line: &self.reread_line,
            tool_reading_required: self.tool_reading_required,
            reading_duty: self.reading_duty,
            grok_answer_once: self.grok_answer_once,
        }
    }

    /// The prompt text.
    pub fn assemble(&self) -> String {
        assemble(&self.inputs())
    }
}

/// (0.2.5 R5, F18-1 / F19-6) The one sentence of a prompt whose task's open findings are not listed
/// because they were recorded against another revision than the one under review.
pub fn withheld_findings_line(n: usize) -> String {
    format!(
        "{n} open finding(s) of this task were recorded against another revision than the one under review, so they are not listed here and are not re-checked in this consultation."
    )
}

/// (C3) The mandatory tool-reading sentence appended to a reviewing-purpose prompt for an engine
/// that counts tool calls (grok, kimi): a reply that read no file is rejected and re-asked.
pub const TOOL_READING_MANDATORY: &str = "Mandatory: this reply is accepted only if you read the code with your tools first; a reply without tool calls is rejected and re-asked.";

/// (C3) The grok reading-duty paragraph for a NON-review purpose: a separate paragraph pushing Grok
/// to open the brief (referenced by path, never inlined) before answering. Measured: a no-read grok
/// reply answers from the brief alone and INVENTS code and citations, so the zero-tool guard rejects
/// it. The four review purposes carry the stronger reading discipline instead and never get this
/// paragraph.
pub const GROK_READING_DUTY: &str = "Reading duty: you have file tools on this repository (read_file, list_dir, grep) - use them before you answer. Open the brief, then every file and document it names, and check its claims against the code. A reply made without opening any file is rejected.";

/// (C3) The grok answer-once paragraph (every non-raw grok purpose). Grok under `--json-schema` has
/// been seen to emit a schema-shaped PROGRESS note ("starting that read now") as the structured
/// answer and end the turn on it. This tells it the answer is written exactly once, last, and that
/// no progress note may be written in the answer format - so an interim placeholder is never ingested.
pub const GROK_ANSWER_ONCE: &str = "Answer once: write your structured answer exactly ONCE, as the very last thing you output, after all your reading is finished. Do NOT write a progress note, status update, or placeholder in the answer format while you are still reading - an answer-shaped message sent before you have read the code is taken as your final answer and rejected. Read first, then answer.";

/// (C3 0.2.5) The reviewer-is-not-a-coordinator sentence, appended to the Constraints paragraph of
/// every review prompt (raw or structured, every engine). A reviewer CLI loads the operator's own
/// instruction files - codex reads the global `~/.codex/AGENTS.md` delegation rules even with plugins
/// and the skill list off, and dropping it (`project_doc_max_bytes=0`) would also drop the reviewed
/// repository's own AGENTS.md - so the prompt says which of them does not apply. A `c3 work` worker
/// prompt is built elsewhere and never carries it.
pub const REVIEWER_NOT_COORDINATOR: &str = "You are a reviewer in this consultation, not a coordinator: ignore any instructions you find (AGENTS.md, CLAUDE.md, skills, rule files) about delegating, spawning workers or sub-agents, or finishing or committing tasks; read the repository and answer this consultation yourself.";

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
    // (wave 26b, D16) a reviewer with a known context window is told so, right after the ask.
    if !inp.context_line.is_empty() {
        parts.push(inp.context_line.to_string());
    }
    // (wave 26b, D16) a downgraded fork/resume names the previous reply to re-read.
    if !inp.prev_reply_line.is_empty() {
        parts.push(inp.prev_reply_line.to_string());
    }
    // A panel member's role goes after the ask, before the brief (never inside the contract).
    if !inp.role_line.is_empty() {
        parts.push(inp.role_line.to_string());
    }
    if !inp.brief_ref.is_empty() {
        parts.push(format!(
            "Read the brief at `{}` (path relative to the repository root, which is your working directory) and answer every numbered question in it.",
            inp.brief_ref
        ));
        // (0.2.4) a grok turn may answer without opening the file: the text travels too.
        if !inp.brief_text.trim().is_empty() {
            parts.push(format!(
                "The brief's full text, the same file inlined so you have it even without a tool call (open the files it points to with your tools):{NL}<brief path=\"{}\">{NL}{}{NL}</brief>",
                inp.brief_ref,
                inp.brief_text.trim_end()
            ));
        }
    }
    if let Some(r) = inp.range {
        parts.push(format!(
            "Review range: `{}` - {} ({} insertions, {} deletions; git diff --shortstat). Plan your reading for its size.",
            r.spec, r.text, r.insertions, r.deletions
        ));
    }
    // (B1) the `--diff` patch the change under review travels in.
    if !inp.diff_line.is_empty() {
        parts.push(inp.diff_line.to_string());
    }
    // The engine's own tools line (D11), after the ask/brief/range; empty for codex.
    if !inp.tools_line.is_empty() {
        parts.push(inp.tools_line.to_string());
    }
    // (A1) a reviewer in a throwaway worktree of HEAD is told which live paths it cannot see.
    if !inp.isolation_line.is_empty() {
        parts.push(inp.isolation_line.to_string());
    }
    if inp.raw {
        if inp.purpose == "chore" {
            parts.push(format!("Review purpose: chore. {}", presets::text("chore")));
        }
        parts.push(format!(
            "Constraints: write NO files and make no edits - this is a read-only consultation; answer in English; keep the answer under {} words. {REVIEWER_NOT_COORDINATOR}",
            inp.max_words
        ));
    } else {
        if !inp.purpose.is_empty() {
            let mut p = format!(
                "Review purpose: {}. {}",
                inp.purpose,
                presets::text(inp.purpose)
            );
            // (C3) an engine that counts tool calls is told the tool-reading is mandatory.
            if inp.tool_reading_required {
                p.push(' ');
                p.push_str(TOOL_READING_MANDATORY);
            }
            parts.push(p);
        }
        // (C3) grok on a non-review purpose gets the reading-duty paragraph (a separate paragraph):
        // it answers from the brief alone otherwise. The review purposes carry the stronger reading
        // discipline and are excluded by the caller.
        if inp.reading_duty {
            parts.push(GROK_READING_DUTY.to_string());
        }
        // (C3) grok (review or not): write the answer exactly once, last - no schema-shaped progress
        // note that would be taken as the final reply.
        if inp.grok_answer_once {
            parts.push(GROK_ANSWER_ONCE.to_string());
        }
        if !inp.open_findings.is_empty() {
            parts.push(render_open_findings(inp.open_findings));
        } else if inp.withheld_findings > 0 {
            parts.push(withheld_findings_line(inp.withheld_findings));
        }
        parts.push(schema_block(inp));
        if inp.schema_transport == "prompt-only" {
            parts.push(format!("JSON Schema of the reply:{NL}{}", inp.schema_text));
        }
        parts.push(format!(
            "Constraints: write NO files and make no edits - this is a read-only consultation; answer in English; keep reply_markdown under {} words. {REVIEWER_NOT_COORDINATOR}",
            inp.max_words
        ));
    }
    // (wave 28c, D11) the re-read anchor: the last line before the consultation id.
    if !inp.reread_line.is_empty() {
        parts.push(inp.reread_line.to_string());
    }
    parts.push(format!("Consultation id: {}", inp.consult_id));
    parts.join(&format!("{NL}{NL}"))
}

/// The longest first line of an ask the re-read anchor repeats whole (characters).
const REREAD_ASK_MAX: usize = 300;

/// (wave 28c, D11 / F43-3, F44-6) A reviewer with a context window (`context_tokens` > 0) may
/// compact it mid-review, and a summary may lose the brief: its prompt ends by naming the brief
/// again - "Before you answer, re-read the brief: `<brief>`." - the last line before the
/// consultation id. (wave 28d, D7 / F50-2) Without a brief file the ask itself is repeated there.
/// (wave 28e, E4 / F54-4) A multi-line ask is not folded into one line: its FIRST line is repeated
/// whole (whitespace inside it folded; longer than 300 characters it is cut and points to the top
/// of the prompt), followed by the count of the remaining (non-blank) lines - " (+<n> more
/// lines)"; a one-line ask as before, cut at 300. `""` without a context window or an ask.
pub fn reread_line(context_tokens: i64, brief_ref: &str, prompt: &str) -> String {
    if context_tokens <= 0 {
        return String::new();
    }
    if !brief_ref.is_empty() {
        return format!("Before you answer, re-read the brief: `{brief_ref}`.");
    }
    let ask_lines: Vec<&str> = prompt
        .trim()
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .filter(|l| !l.trim().is_empty())
        .collect();
    let Some(first) = ask_lines.first() else {
        return String::new();
    };
    let mut ask = c3_core::one_line(first);
    if ask.chars().count() > REREAD_ASK_MAX {
        ask = ask.chars().take(REREAD_ASK_MAX).collect::<String>()
            + "... (cut here: the whole ask is at the top of this prompt)";
    }
    let more = ask_lines.len() - 1;
    if more > 0 {
        ask.push_str(&format!(
            " (+{more} more {})",
            if more == 1 { "line" } else { "lines" }
        ));
    }
    format!("Before you answer, re-read the ask: {ask}")
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
        brief_text: "",
        range: None,
        diff_line: "",
        open_findings: if has_open { &one } else { &[] },
        withheld_findings: 0,
        schema_transport: if prompt_only {
            "prompt-only"
        } else {
            "output-schema"
        },
        schema_text: "",
        max_words: 0,
        consult_id: "",
        tools_line: "",
        isolation_line: "",
        role_line: "",
        context_line: "",
        prev_reply_line: "",
        reread_line: "",
        tool_reading_required: false,
        reading_duty: false,
        grok_answer_once: false,
    };
    schema_block(&inp)
}

fn schema_block(inp: &PromptInputs) -> String {
    let prior_line = if inp.open_findings.is_empty() && inp.withheld_findings > 0 {
        "- prior_findings: an empty array (no earlier findings are listed for the revision under review)."
    } else if inp.open_findings.is_empty() {
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
            brief_text: "",
            range: None,
            diff_line: "",
            open_findings: &[],
            withheld_findings: 0,
            schema_transport: "output-schema",
            schema_text: "",
            max_words: 700,
            consult_id: "abc-123",
            tools_line: "",
            isolation_line: "",
            role_line: "",
            context_line: "",
            prev_reply_line: "",
            reread_line: "",
            tool_reading_required: false,
            reading_duty: false,
            grok_answer_once: false,
        }
    }

    // (A1) the isolation paragraph follows the tools line and precedes the purpose; empty adds
    // nothing.
    #[test]
    fn isolation_line_follows_the_tools_line() {
        let mut i = base();
        i.tools_line = "Tools: read files.";
        let plain = assemble(&i);
        i.isolation_line = "Isolation: your working directory is a throwaway worktree.";
        let with = assemble(&i);
        assert!(!plain.contains("Isolation:"));
        let t = with.find("Tools: read files.").unwrap();
        let iso = with.find("Isolation: your working directory").unwrap();
        let purpose = with.find("Review purpose: framing.").unwrap();
        assert!(t < iso && iso < purpose, "{with}");
    }

    // (B1) the `--diff` line follows the brief line and precedes the tools line; empty adds nothing.
    #[test]
    fn diff_line_follows_the_brief_and_precedes_the_tools_line() {
        let mut i = base();
        i.tools_line = "Tools: read files.";
        assert!(!assemble(&i).contains("The change under review"));
        i.diff_line = "The change under review is the patch at `.collab/t/handoffs/01.patch`.";
        let p = assemble(&i);
        let brief = p.find("Read the brief at").unwrap();
        let diff = p.find("The change under review is the patch at").unwrap();
        let tools = p.find("Tools: read files.").unwrap();
        assert!(brief < diff && diff < tools, "{p}");
    }

    #[test]
    fn role_line_after_ask_before_brief() {
        let mut i = base();
        i.role_line = "Your role in this review: adversary. Focus on what it asks for; the reply format, the verdict rules and the read-only rule stay as stated.\r\nAttack it.";
        let p = assemble(&i);
        let role_at = p.find("Your role in this review").unwrap();
        let ask_at = p.find("\r\n\r\nx\r\n\r\n").map(|n| n + 4).unwrap_or(0);
        let brief_at = p.find("Read the brief at").unwrap();
        assert!(ask_at < role_at && role_at < brief_at);
    }

    // (R5, F18-1 / F19-6) a review of another revision lists no prior findings: one sentence says
    // how many were recorded against another revision, and the reply format asks for an empty
    // prior_findings; the owned plan assembles the same text as the borrowed inputs.
    #[test]
    fn withheld_findings_are_named_in_one_sentence_and_not_listed() {
        let mut i = base();
        i.withheld_findings = 34;
        let p = assemble(&i);
        assert!(
            p.contains("34 open finding(s) of this task were recorded against another revision than the one under review, so they are not listed here"),
            "{p}"
        );
        assert!(p.contains(
            "- prior_findings: an empty array (no earlier findings are listed for the revision under review)."
        ));
        assert!(!p.contains("Findings from earlier consultations"));
        assert!(!assemble(&base()).contains("open finding(s) of this task"));
        let plan = PromptPlan {
            purpose: "framing".into(),
            prompt: "x".into(),
            brief_ref: ".collab/t/handoffs/01.md".into(),
            withheld_findings: 34,
            schema_transport: "output-schema".into(),
            max_words: 700,
            consult_id: "abc-123".into(),
            ..Default::default()
        };
        assert_eq!(plan.assemble(), p);
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

    // (C3) The grok reading-duty paragraph: present for grok + a non-review purpose, a separate
    // paragraph after the purpose line; absent for raw, for the review purposes, and for the engines
    // that do not set the flag.
    #[test]
    fn reading_duty_paragraph_only_for_grok_non_review() {
        // grok + framing: the caller sets reading_duty -> the paragraph is present, after the
        // purpose line and before the consultation id.
        // (base() has an empty brief_text: orchestrate now passes the grok brief by reference, never
        // inlined on a non-raw purpose - so there is no <brief> block, only the pointer line.)
        let mut i = base();
        i.reading_duty = true;
        let p = assemble(&i);
        assert!(p.contains(GROK_READING_DUTY), "framing must carry it");
        assert!(
            !p.contains("<brief"),
            "the brief is referenced, not inlined"
        );
        assert!(
            p.contains("Read the brief at `.collab/t/handoffs/01.md`"),
            "the brief is still referenced by path"
        );
        let purpose_at = p.find("Review purpose:").unwrap();
        let duty_at = p.find(GROK_READING_DUTY).unwrap();
        let id_at = p.find("Consultation id:").unwrap();
        assert!(purpose_at < duty_at && duty_at < id_at);
        // It is its own paragraph (double CRLF before it), not glued to the purpose line.
        assert!(p.contains(&format!("\r\n\r\n{GROK_READING_DUTY}")));

        // grok + acceptance (a review purpose): the caller leaves reading_duty false -> absent.
        let mut acc = base();
        acc.purpose = "acceptance";
        acc.reading_duty = false;
        assert!(!assemble(&acc).contains(GROK_READING_DUTY));

        // another engine + framing: the caller leaves reading_duty false -> absent.
        let other = base();
        assert!(!assemble(&other).contains(GROK_READING_DUTY));

        // raw: the paragraph never appears, even if the flag were set (the raw branch omits it).
        let mut raw = base();
        raw.raw = true;
        raw.purpose = "";
        raw.reading_duty = true;
        assert!(!assemble(&raw).contains(GROK_READING_DUTY));
    }

    // Case 1: a non-blank brief_text inlines the brief after the pointer, before the purpose.
    #[test]
    fn brief_text_inlined_after_read_line_before_purpose() {
        let mut i = base();
        i.brief_text = "1. Is the range real?\r\n2. What did the tests prove?";
        let p = assemble(&i);
        assert!(p.contains("<brief path=\".collab/t/handoffs/01.md\">"));
        assert!(p.contains("1. Is the range real?\r\n2. What did the tests prove?"));
        assert!(p.contains("</brief>"));
        let read_at = p.find("Read the brief at").unwrap();
        let open_at = p.find("<brief path=").unwrap();
        let close_at = p.find("</brief>").unwrap();
        let purpose_at = p.find("Review purpose:").unwrap();
        assert!(read_at < open_at && open_at < close_at && close_at < purpose_at);
    }

    // Case 2: an empty or whitespace-only brief_text adds nothing.
    #[test]
    fn blank_brief_text_adds_nothing() {
        let mut i = base();
        i.brief_text = "";
        assert!(!assemble(&i).contains("<brief"));
        i.brief_text = "  \r\n \n\t";
        let p = assemble(&i);
        assert!(!p.contains("<brief"));
        assert!(p.contains("Read the brief at `.collab/t/handoffs/01.md`"));
    }

    // Case 3: the text's trailing whitespace is trimmed before </brief>.
    #[test]
    fn brief_text_trailing_whitespace_trimmed() {
        let mut i = base();
        i.brief_text = "1. One.\r\n2. Two.\r\n\r\n   \r\n";
        let p = assemble(&i);
        assert!(p.contains(
            "<brief path=\".collab/t/handoffs/01.md\">\r\n1. One.\r\n2. Two.\r\n</brief>"
        ));
    }

    // Case 5: raw mode drops the scaffolding, not the inline: the block still follows the pointer.
    #[test]
    fn raw_mode_still_inlines_brief_text() {
        let mut i = base();
        i.raw = true;
        i.purpose = "";
        i.brief_text = "1. One.";
        let p = assemble(&i);
        assert!(!p.starts_with(FINAL_OUTPUT_CONTRACT));
        assert!(p.find("Read the brief at").unwrap() < p.find("<brief path=").unwrap());
        assert!(p.contains("1. One."));
        assert!(p.contains("</brief>"));
    }

    // Case 6: the inline is nested under the pointer - no brief_ref, no inline, whatever the text.
    #[test]
    fn brief_text_without_brief_ref_adds_nothing() {
        let mut i = base();
        i.brief_ref = "";
        i.brief_text = "1. Orphaned text.";
        let p = assemble(&i);
        assert!(!p.contains("<brief"));
        assert!(!p.contains("Read the brief at"));
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

    /// (C3 0.2.5) Every review prompt - structured and raw, whatever the engine - tells the reviewer
    /// it is not a coordinator, at the end of its Constraints paragraph; a `c3 work` worker prompt
    /// (the work preamble + brief + ask) never carries it.
    #[test]
    fn a_review_prompt_says_the_reviewer_is_not_a_coordinator() {
        let structured = assemble(&base());
        assert!(structured.contains(&format!(
            "keep reply_markdown under 700 words. {REVIEWER_NOT_COORDINATOR}\r\n\r\nConsultation id: abc-123"
        )));
        let mut i = base();
        i.raw = true;
        i.purpose = "";
        i.tools_line = "Tools: read_file, grep.";
        let raw = assemble(&i);
        assert!(raw.contains(&format!(
            "keep the answer under 700 words. {REVIEWER_NOT_COORDINATOR}\r\n\r\nConsultation id: abc-123"
        )));
        assert!(!c3_core::claude::WORK_PREAMBLE.contains(REVIEWER_NOT_COORDINATOR));
        assert!(!c3_core::claude::WORK_PREAMBLE.contains("not a coordinator"));
    }

    #[test]
    fn crlf_paragraph_joins() {
        let p = assemble(&base());
        assert!(p.contains("\r\n\r\n"));
    }

    /// The plugin's `harness-fixes28e.ps1` ANCHOR (E4) asks: the first line whole, the rest
    /// counted; a first line over 300 characters cut with the pointer; the re-read line right
    /// before the consultation id.
    #[test]
    fn reread_anchor_keeps_the_first_line_and_counts_the_rest() {
        let ask1 = "Review app.txt for   typos.\nThen check the README.\n\n   And the CHANGELOG.  ";
        assert_eq!(
            reread_line(256000, "", ask1),
            "Before you answer, re-read the ask: Review app.txt for typos. (+2 more lines)"
        );
        assert_eq!(
            reread_line(256000, "", "Fix the typo.\r\nNothing else."),
            "Before you answer, re-read the ask: Fix the typo. (+1 more line)"
        );
        let long_first = format!("{} {}", "c".repeat(200), "d".repeat(200));
        let cut = format!(
            "Before you answer, re-read the ask: {}... (cut here: the whole ask is at the top of this prompt)",
            &long_first[..300]
        );
        assert_eq!(
            reread_line(
                256000,
                "",
                &format!("{long_first}\nsecond line\nthird line")
            ),
            format!("{cut} (+2 more lines)")
        );
        assert_eq!(reread_line(256000, "", &long_first), cut);
        // a brief file is named instead; no context window, no anchor
        assert_eq!(
            reread_line(256000, ".collab/t/brief.md", ask1),
            "Before you answer, re-read the brief: `.collab/t/brief.md`."
        );
        assert_eq!(reread_line(0, "", ask1), "");
        assert_eq!(reread_line(256000, "", "  \n "), "");
        // the last line before the consultation id; the whole ask stays at the top
        let mut i = base();
        i.prompt = ask1;
        let line = reread_line(256000, "", ask1);
        i.reread_line = &line;
        let p = assemble(&i);
        assert!(
            p.ends_with(&format!("{line}\r\n\r\nConsultation id: abc-123")),
            "{p}"
        );
        assert!(p.contains("Then check the README.") && p.contains("And the CHANGELOG."));
    }
}
