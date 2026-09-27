//! The console summary block and the manual resume command
//! (`codex-consult.ps1:3597-3630` and the `# output` block `3979-4058`).
//!
//! [`build_resume_command`] mirrors the plugin's resume-command builder: it names every
//! replay-relevant option of the killed run so running it reproduces the settings (F08-6),
//! including (wave 24c, F15-5) `-ReplyName` and `-SkipPreflight`. What the purpose resolves
//! the same way on a fresh run (a purpose timeout, the default continuation budget) is left
//! out.
//!
//! [`render_summary`] returns the ordered console lines of a finished run (the usable and
//! the failed branches). It draws only from the ledger-level facts the orchestrator has;
//! colour is not modelled (every line is plain text here).

/// Inputs for the manual resume command (codex; the engine variants land in M2d).
#[derive(Debug, Clone, Default)]
pub struct ResumeInputs {
    pub task: String,
    pub collab_dir: String,
    pub thread: String,
    /// True when no roster supplied the identity (then provider/model must be named).
    pub no_roster: bool,
    pub provider: String,
    pub model: String,
    pub purpose: String,
    pub raw: bool,
    /// The reply name, and whether it was given explicitly (or this is a panel member).
    pub reply_name: String,
    pub reply_name_given: bool,
    pub timeout_source: String,
    pub timeout_sec: i64,
    pub continue_sec: i64,
    pub effort: String,
    pub native_effort: String,
    pub max_words: i64,
    pub transport_override: String,
    /// The `-CodexConfig` items as the user gave them (unexpanded).
    pub codex_config: Vec<String>,
    /// Resolved absolute artifact paths.
    pub artifacts: Vec<String>,
    pub range: String,
    pub sandbox: String,
    pub format_retry: i64,
    pub off_peak_only: bool,
    pub skip_preflight: bool,
    pub codex_exe: String,
}

/// Quote a value that contains a character outside `[A-Za-z0-9._:/\=+@~-]` (`$qa`).
fn qa(v: &str) -> String {
    let safe = !v.is_empty()
        && v.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(
                    c,
                    '.' | '_' | ':' | '/' | '\\' | '=' | '+' | '@' | '~' | '-'
                )
        });
    if safe {
        v.to_string()
    } else {
        format!("\"{}\"", v.replace('"', "\\\""))
    }
}

/// Build the resume argument string (`-Task ... -Prompt "finish your review"`).
pub fn build_resume_command(r: &ResumeInputs) -> String {
    let mut p: Vec<String> = Vec::new();
    p.push(format!("-Task {}", r.task));
    if !r.collab_dir.is_empty() && r.collab_dir != ".collab" {
        p.push(format!(
            "-CollabDir {}",
            if r.collab_dir.contains(char::is_whitespace) {
                format!("\"{}\"", r.collab_dir)
            } else {
                r.collab_dir.clone()
            }
        ));
    }
    p.push(format!("-Mode resume -Thread {}", r.thread));
    if r.no_roster {
        p.push(format!("-Provider {} -Model {}", r.provider, r.model));
    }
    if !r.purpose.is_empty() {
        p.push(format!("-Purpose {}", r.purpose));
    }
    if r.raw && r.purpose != "chore" {
        p.push("-Raw".into());
    }
    // (wave 24c, F15-5) the handoff name and the skipped-preflight decision.
    if r.reply_name_given {
        p.push(format!("-ReplyName {}", qa(&r.reply_name)));
    }
    if r.timeout_source == "explicit" {
        p.push(format!("-TimeoutSec {}", r.timeout_sec));
    }
    if r.continue_sec != r.timeout_sec.min(900) {
        p.push(format!("-ContinueSec {}", r.continue_sec));
    }
    if !r.effort.is_empty() {
        p.push(format!("-Effort {}", r.effort));
    }
    if !r.native_effort.is_empty() {
        p.push(format!("-NativeEffort {}", qa(&r.native_effort)));
    }
    if r.max_words > 0 {
        p.push(format!("-MaxWords {}", r.max_words));
    }
    if !r.transport_override.is_empty() {
        p.push(format!("-SchemaTransport {}", r.transport_override));
    }
    let cfg: Vec<String> = r
        .codex_config
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if !cfg.is_empty() {
        p.push(format!("-CodexConfig {}", qa(&cfg.join(","))));
    }
    if !r.artifacts.is_empty() {
        p.push(format!("-Artifact {}", qa(&r.artifacts.join(","))));
    }
    if !r.range.is_empty() {
        p.push(format!("-Range {}", qa(&r.range)));
    }
    if !r.sandbox.is_empty() && r.sandbox != "read-only" {
        p.push(format!("-Sandbox {}", r.sandbox));
    }
    if r.format_retry == 0 && !r.raw {
        p.push("-FormatRetry 0".into());
    }
    if r.off_peak_only {
        p.push("-OffPeakOnly".into());
    }
    if r.skip_preflight {
        p.push("-SkipPreflight".into());
    }
    if !r.codex_exe.is_empty() {
        p.push(format!("-CodexExe {}", qa(&r.codex_exe)));
    }
    p.push("-Prompt \"finish your review\"".into());
    p.join(" ")
}

/// The success/failure facts the summary renders.
#[derive(Debug, Clone, Default)]
pub struct SummaryInputs {
    pub bridge_outcome: String,
    pub usable: bool,
    pub wall_seconds: String,
    pub lineage_shown: String,
    pub mode: String,
    pub thread: String,
    pub thread_source: String,
    /// An unverified rollout candidate (`Find-ThreadInRollouts`): the newest rollout uuid that
    /// did NOT contain this run's consultation id. Renders the `thread      : unknown - rollout
    /// candidate ...` line (usable codex run only). Empty otherwise.
    pub thread_candidate: String,
    /// This run's consultation id (for the rollout-candidate line).
    pub consult_id: String,
    /// A failure's operator hint (context-window etc.), when the bridge can explain it.
    pub failure_hint: String,
    /// The `continued  : ...` line when a timeout continuation ran (empty otherwise).
    pub continue_line: String,
    /// The `format repair: ...` console line when a repair ran (empty otherwise).
    pub repair_console: String,
    /// The repair drift notes, each rendered as `  drift: <note>`.
    pub repair_drift: Vec<String>,
    /// The `partial    :` and `resume     :` lines when a turn was killed.
    pub partial_path: String,
    pub partial_footer: String,
    pub resume_command: String,
    /// The structured verdict line (`verdict    : ACCEPT - ...`) when a reply parsed.
    pub verdict_line: String,
    /// The findings line (`findings   : ...`).
    pub findings_line: String,
    /// The `prior      : <id status>, ...` line (empty when no prior findings reported).
    pub prior_line: String,
    /// The `unknown ids: ...` line (prior ids not in findings.json).
    pub unknown_ids_line: String,
    /// The `supersedes : ...` line (supersedes targets not in findings.json).
    pub supersedes_line: String,
    /// The `structured : INVALID (...)` line when a reply was ingested but is not structured
    /// (prose kept); empty otherwise. Replaces the verdict/findings lines.
    pub structured_invalid: String,
    pub reply_path: String,
    pub reply_json_path: String,
    pub events_path: String,
    /// The verbatim reply body echoed at the end (usable only).
    pub reply_body: String,
    pub section: String,
    /// stderr tail (failed only).
    pub stderr_tail: String,
}

/// Render the ordered console lines of a finished run (no colour).
pub fn render_summary(s: &SummaryInputs) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if !s.usable {
        out.push(format!(
            "codex-consult: {} (wall {} s)",
            s.bridge_outcome, s.wall_seconds
        ));
        if !s.failure_hint.is_empty() {
            out.push(format!("hint       : {}", s.failure_hint));
        }
        if !s.continue_line.is_empty() {
            out.push(s.continue_line.clone());
        }
        if !s.partial_path.is_empty() {
            out.push(format!(
                "partial    : {} ({})",
                s.partial_path, s.partial_footer
            ));
            if !s.resume_command.is_empty() {
                out.push(format!("resume     : {}", s.resume_command));
            } else {
                out.push("resume     : not possible - the thread of the killed turn is not known (start again with -Mode new)".into());
            }
        }
        out.push(format!("reply file : {}", s.reply_path));
        if !s.reply_json_path.is_empty() {
            out.push(format!("reply json : {}", s.reply_json_path));
        }
        out.push(format!("events file: {}", s.events_path));
        if !s.stderr_tail.trim().is_empty() {
            out.push("--- codex stderr (tail) ---".into());
            out.push(s.stderr_tail.trim().to_string());
        }
        return out;
    }

    out.push(format!(
        "codex-consult: {} - {}, mode {}, thread {} (source: {}), wall {} s",
        s.bridge_outcome, s.lineage_shown, s.mode, s.thread, s.thread_source, s.wall_seconds
    ));
    if !s.continue_line.is_empty() {
        out.push(s.continue_line.clone());
    }
    if !s.partial_path.is_empty() {
        out.push(format!(
            "partial    : {} ({})",
            s.partial_path, s.partial_footer
        ));
        if !s.resume_command.is_empty() {
            out.push(format!("resume     : {}", s.resume_command));
        }
    }
    if !s.repair_console.is_empty() {
        out.push(s.repair_console.clone());
        for d in &s.repair_drift {
            out.push(format!("  drift: {d}"));
        }
    }
    if !s.thread_candidate.is_empty() {
        out.push(format!(
            "thread     : unknown - rollout candidate {} did not contain consultation id {} (not used as a thread or a parent)",
            s.thread_candidate, s.consult_id
        ));
    }
    if !s.verdict_line.is_empty() {
        out.push(s.verdict_line.clone());
    }
    if !s.findings_line.is_empty() {
        out.push(s.findings_line.clone());
    }
    if !s.prior_line.is_empty() {
        out.push(s.prior_line.clone());
    }
    if !s.unknown_ids_line.is_empty() {
        out.push(s.unknown_ids_line.clone());
    }
    if !s.supersedes_line.is_empty() {
        out.push(s.supersedes_line.clone());
    }
    if !s.structured_invalid.is_empty() {
        out.push(s.structured_invalid.clone());
    }
    out.push(format!("reply file : {}", s.reply_path));
    if !s.reply_json_path.is_empty() {
        out.push(format!("reply json : {}", s.reply_json_path));
    }
    out.push(format!("events file: {}", s.events_path));
    out.push(String::new());
    out.push(s.reply_body.clone());
    if !s.section.is_empty() {
        out.push(String::new());
        out.push(s.section.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_resume() -> ResumeInputs {
        ResumeInputs {
            task: "t".into(),
            collab_dir: ".collab".into(),
            thread: "abc-123".into(),
            no_roster: true,
            provider: "openai".into(),
            model: "gpt-6-astra".into(),
            purpose: "acceptance".into(),
            timeout_source: "purpose".into(),
            timeout_sec: 3600,
            continue_sec: 900,
            sandbox: "read-only".into(),
            format_retry: 1,
            ..Default::default()
        }
    }

    #[test]
    fn resume_command_names_replay_options_and_skip_preflight() {
        let mut r = base_resume();
        r.reply_name = "review".into();
        r.reply_name_given = true;
        r.skip_preflight = true;
        r.effort = "high".into();
        r.timeout_source = "explicit".into();
        r.timeout_sec = 120;
        r.range = "a..b".into();
        let cmd = build_resume_command(&r);
        assert!(cmd.starts_with("-Task t -Mode resume -Thread abc-123"));
        assert!(cmd.contains("-Provider openai -Model gpt-6-astra"));
        assert!(cmd.contains("-Purpose acceptance"));
        assert!(cmd.contains("-ReplyName review"));
        assert!(cmd.contains("-SkipPreflight"));
        assert!(cmd.contains("-TimeoutSec 120"));
        assert!(cmd.contains("-Effort high"));
        assert!(cmd.contains("-Range a..b"));
        assert!(cmd.ends_with("-Prompt \"finish your review\""));
    }

    #[test]
    fn resume_command_omits_defaults() {
        let cmd = build_resume_command(&base_resume());
        // purpose timeout / default continuation are NOT replayed.
        assert!(!cmd.contains("-TimeoutSec"));
        assert!(!cmd.contains("-ContinueSec"));
        assert!(!cmd.contains("-ReplyName"));
        assert!(!cmd.contains("-SkipPreflight"));
        assert!(!cmd.contains("-Sandbox")); // read-only default
    }

    #[test]
    fn failed_summary_has_hint_and_partial() {
        let s = SummaryInputs {
            bridge_outcome: "failed: timeout after 900 s".into(),
            usable: false,
            wall_seconds: "901".into(),
            failure_hint: "context too long".into(),
            partial_path: "handoffs/03-codex-review.partial.md".into(),
            partial_footer: "killed at 901 s of 900 s".into(),
            resume_command: "pwsh ...".into(),
            reply_path: "handoffs/03-codex-review.md".into(),
            events_path: "handoffs/03-codex-review.events.jsonl".into(),
            ..Default::default()
        };
        let lines = render_summary(&s);
        assert!(lines[0].starts_with("codex-consult: failed: timeout"));
        assert!(lines.iter().any(|l| l.starts_with("hint       :")));
        assert!(lines.iter().any(|l| l.starts_with("partial    :")));
        assert!(lines.iter().any(|l| l.starts_with("resume     :")));
    }

    #[test]
    fn usable_summary_header_and_verdict() {
        let s = SummaryInputs {
            bridge_outcome: "usable reply".into(),
            usable: true,
            wall_seconds: "12".into(),
            lineage_shown: "openai :: gpt-6-astra".into(),
            mode: "new".into(),
            thread: "t-1".into(),
            thread_source: "events".into(),
            verdict_line: "verdict    : ACCEPT - looks good".into(),
            findings_line: "findings   : none".into(),
            reply_path: "handoffs/03-codex-review.md".into(),
            events_path: "handoffs/03-codex-review.events.jsonl".into(),
            reply_body: "the answer".into(),
            ..Default::default()
        };
        let lines = render_summary(&s);
        assert!(lines[0].contains(
            "usable reply - openai :: gpt-6-astra, mode new, thread t-1 (source: events)"
        ));
        assert!(lines
            .iter()
            .any(|l| l == "verdict    : ACCEPT - looks good"));
    }
}
