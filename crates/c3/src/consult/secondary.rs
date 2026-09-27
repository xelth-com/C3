//! The two codex secondary-turn mechanisms of `c3 consult` (`codex-consult.ps1` waves 24/24b/24c):
//! the **timeout continuation** (one `resume <thread>` turn after the main turn was killed on
//! its timeout) and the **format repair** (one `resume <thread>` turn that converts a prose
//! reply into the structured object). This module holds the self-contained pieces the
//! orchestrator drives: the killed-turn salvage (`Read-CodexSalvage` + `Format-PartialBody`),
//! the format-repair drift notes (`Get-FormatRepairDrift`), the repair effort
//! (`Get-RepairEffort`) and the continuation-reply gate (`Test-ContinuationReply`).
//!
//! The turn *launch* (building the request, running the codex process, transitioning the
//! recovery record) lives in [`super::orchestrate`]; everything here is pure text work over an
//! event stream / a reply object, so it is unit-testable without a process.

use c3_core::engine::StructuredReply;
use regex::Regex;

use super::ingest::{self, ProseGate};

// ------------------------------------------------------------------------------- salvage

/// One salvaged item of a killed turn's event stream: an agent message or a reasoning text.
#[derive(Debug, Clone)]
pub struct SalvageItem {
    /// `"reasoning"` or `"message"`.
    pub kind: String,
    pub text: String,
}

/// The salvage of one turn (`New-TurnSalvage`): the agent-message / reasoning items in stream
/// order and the tool calls, each listed once.
#[derive(Debug, Clone, Default)]
pub struct Salvage {
    pub items: Vec<SalvageItem>,
    pub tools: Vec<String>,
}

/// `Read-CodexSalvage`: the `item.completed` agent_message / reasoning texts (in stream order)
/// and the tool calls of a codex `--json` event stream. A tool item is listed once even though
/// it appears as both `item.started` and `item.completed`; an item without an id is paired by
/// type (F07-2). Ported from `codex-consult-common.ps1:4229`.
pub fn read_codex_salvage(events_text: &str) -> Salvage {
    let mut items: Vec<SalvageItem> = Vec::new();
    // Insertion-ordered map id -> tool name (an ordered dict in the plugin).
    let mut tool_keys: Vec<String> = Vec::new();
    let mut tool_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    // item type -> the synthetic keys of its id-less items started and not completed, oldest first.
    let mut open_idless: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let mut k: u64 = 0;

    for line in events_text.split(['\r', '\n']) {
        let t = line.trim();
        if !t.starts_with('{') {
            continue;
        }
        let obj: serde_json::Value = match serde_json::from_str(t) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ty = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if ty != "item.started" && ty != "item.completed" {
            continue;
        }
        let item = match obj.get("item") {
            Some(i) if i.is_object() => i,
            _ => continue,
        };
        let itype = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
        k += 1;
        let mut id = item
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if itype == "agent_message" || itype == "reasoning" {
            let tx = item.get("text").and_then(|v| v.as_str()).unwrap_or("");
            if ty == "item.completed" && !tx.trim().is_empty() {
                items.push(SalvageItem {
                    kind: if itype == "reasoning" {
                        "reasoning".into()
                    } else {
                        "message".into()
                    },
                    text: tx.to_string(),
                });
            }
            continue;
        }
        if itype.is_empty() || itype == "error" {
            continue;
        }
        let name = match itype {
            "command_execution" => format!(
                "shell: {}",
                c3_core::one_line(item.get("command").and_then(|v| v.as_str()).unwrap_or(""))
            ),
            "web_search" => {
                let mut q = item.get("query").and_then(|v| v.as_str()).unwrap_or("");
                if q.is_empty() {
                    q = item
                        .get("action")
                        .and_then(|a| a.get("query"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                }
                format!("web_search: {}", c3_core::one_line(q))
                    .trim_end_matches([' ', ':'])
                    .to_string()
            }
            "mcp_tool_call" => format!(
                "mcp: {}/{}",
                item.get("server").and_then(|v| v.as_str()).unwrap_or(""),
                item.get("tool").and_then(|v| v.as_str()).unwrap_or("")
            ),
            "collab_tool_call" => format!(
                "collab: {}",
                item.get("tool").and_then(|v| v.as_str()).unwrap_or("")
            ),
            other => other.to_string(),
        };
        if id.is_empty() {
            let open = open_idless.entry(itype.to_string()).or_default();
            if ty == "item.completed" && !open.is_empty() {
                let at = open
                    .iter()
                    .position(|kk| tool_map.get(kk).map(|s| s.as_str()) == Some(name.as_str()))
                    .unwrap_or(0);
                id = open.remove(at);
            } else {
                id = format!("#{k}");
                if ty == "item.started" {
                    open.push(id.clone());
                }
            }
        }
        if ty == "item.completed" || !tool_map.contains_key(&id) {
            if !tool_map.contains_key(&id) {
                tool_keys.push(id.clone());
            }
            tool_map.insert(id, name);
        }
    }

    let tools = tool_keys
        .into_iter()
        .map(|kk| tool_map.remove(&kk).unwrap_or_default())
        .collect();
    Salvage { items, tools }
}

/// One turn's contribution to a `.partial.md` body (`Format-PartialBody`'s per-turn object).
pub struct PartialTurn {
    /// e.g. `Turn 1 - the main turn`.
    pub label: String,
    /// e.g. `killed at 902.1 s of 900 s`; empty when there is no note.
    pub note: String,
    pub salvage: Salvage,
}

/// `Format-PartialBody`: per turn a heading, every agent message and reasoning text in stream
/// order, then the tool calls. Markdown, LF line ends (`codex-consult-common.ps1:4379`).
pub fn format_partial_body(turns: &[PartialTurn]) -> String {
    let mut out: Vec<String> = Vec::new();
    for turn in turns {
        out.push(format!(
            "## {}{}",
            turn.label,
            if turn.note.is_empty() {
                String::new()
            } else {
                format!(" - {}", turn.note)
            }
        ));
        out.push(String::new());
        if turn.salvage.items.is_empty() {
            out.push("_(no agent message or reasoning text in this turn's event stream)_".into());
            out.push(String::new());
        }
        let mut n_msg = 0;
        let mut n_rea = 0;
        for it in &turn.salvage.items {
            if it.kind == "reasoning" {
                n_rea += 1;
                out.push(format!("**Reasoning {n_rea}:**"));
            } else {
                n_msg += 1;
                out.push(format!("**Agent message {n_msg}:**"));
            }
            out.push(String::new());
            out.push(it.text.trim().replace("\r\n", "\n"));
            out.push(String::new());
        }
        let tools = &turn.salvage.tools;
        out.push(format!(
            "**Tool calls ({}):**{}",
            tools.len(),
            if tools.is_empty() { " none" } else { "" }
        ));
        if !tools.is_empty() {
            out.push(String::new());
            for tl in tools {
                out.push(format!("- `{}`", tl.replace('`', "'")));
            }
        }
        out.push(String::new());
    }
    out.join("\n")
}

// ------------------------------------------------------------------------------- drift

fn drift_text(text: &str) -> String {
    Regex::new(r"\s+")
        .unwrap()
        .replace_all(text.trim(), " ")
        .to_lowercase()
}

/// `Get-FormatRepairDrift`: what a format repair may have changed — the prose (the first reply)
/// against the repaired object. One note per check that differs
/// (`codex-consult-common.ps1:1784`).
pub fn get_format_repair_drift(prose: &str, reply: &StructuredReply) -> Vec<String> {
    let mut notes: Vec<String> = Vec::new();
    let md = &reply.reply_markdown;

    // 1. the RC<n> ids of the prose vs of reply_markdown.
    let rc_of = |t: &str| -> Vec<String> {
        let mut v: Vec<i64> = Regex::new(r"\bRC([0-9]+)\b")
            .unwrap()
            .captures_iter(t)
            .filter_map(|c| c[1].parse::<i64>().ok())
            .collect();
        v.sort_unstable();
        v.dedup();
        v.into_iter().map(|n| format!("RC{n}")).collect()
    };
    let rc_prose = rc_of(prose);
    let rc_md = rc_of(md);
    if rc_prose.join(",") != rc_md.join(",") {
        notes.push(format!(
            "requested checks differ: prose {}, reply_markdown {}",
            if rc_prose.is_empty() {
                "none".to_string()
            } else {
                rc_prose.join(", ")
            },
            if rc_md.is_empty() {
                "none".to_string()
            } else {
                rc_md.join(", ")
            }
        ));
    }

    // 2. the numbered answers (Q<n>. at a line start, bold or not) of both.
    let q_of = |t: &str| -> Vec<i64> {
        let mut v: Vec<i64> = Regex::new(r"(?m)^\s*(?:\*\*)?Q([0-9]+)\.")
            .unwrap()
            .captures_iter(t)
            .filter_map(|c| c[1].parse::<i64>().ok())
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    };
    let q_prose = q_of(prose);
    let q_md = q_of(md);
    if q_prose.len() != q_md.len() {
        notes.push(format!(
            "numbered answers differ: prose {}, reply_markdown {}",
            q_prose.len(),
            q_md.len()
        ));
    }

    // 3. every F<NN>-<k> id the prose names appears in prior_findings or findings.
    let mut ids_prose: Vec<String> = Regex::new(r"\bF[0-9]{2,}-[0-9]+\b")
        .unwrap()
        .find_iter(prose)
        .map(|m| m.as_str().to_string())
        .collect();
    ids_prose.sort();
    ids_prose.dedup();
    if !ids_prose.is_empty() {
        let mut known: Vec<String> = reply.prior_findings.iter().map(|p| p.id.clone()).collect();
        let findings_text = serde_json::to_string(&reply.findings).unwrap_or_default();
        for m in Regex::new(r"\bF[0-9]{2,}-[0-9]+\b")
            .unwrap()
            .find_iter(&findings_text)
        {
            known.push(m.as_str().to_string());
        }
        let missing: Vec<&String> = ids_prose.iter().filter(|id| !known.contains(id)).collect();
        if !missing.is_empty() {
            notes.push(format!(
                "finding id(s) named in the prose but absent from prior_findings/findings: {}",
                missing
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }

    // 4. a verdict token the prose states equals the JSON verdict.
    let verdict_re =
        Regex::new(r"\b[Vv]erdict\b[^A-Za-z]{0,12}(ACCEPT|HOLD|REJECT|ADVISE)\b").unwrap();
    let bare_re = Regex::new(r"\b(ACCEPT|HOLD|REJECT|ADVISE)\b").unwrap();
    let prose_verdict = verdict_re
        .captures(prose)
        .or_else(|| bare_re.captures(prose))
        .map(|c| c[1].to_string());
    let json_verdict = match reply.verdict {
        c3_core::engine::Verdict::Accept => "ACCEPT",
        c3_core::engine::Verdict::Hold => "HOLD",
        c3_core::engine::Verdict::Reject => "REJECT",
        c3_core::engine::Verdict::Advise => "ADVISE",
    };
    if let Some(pv) = prose_verdict {
        if pv != json_verdict {
            notes.push(format!(
                "verdict differs: prose {pv}, JSON {}",
                if json_verdict.is_empty() {
                    "(none)"
                } else {
                    json_verdict
                }
            ));
        }
    }

    // 5. every prose sentence of >= 60 chars (whitespace-normalised; the 40 longest) appears in
    // reply_markdown (normalised, case-insensitive). Rust's `regex` has no look-behind, so the
    // plugin's `-split '(?<=[.!?])\s+|\r?\n'` is done by hand in `split_sentences`.
    let mut sentences: Vec<String> = split_sentences(prose)
        .into_iter()
        .map(|s| drift_text(&s))
        .filter(|s| s.chars().count() >= 60)
        .collect();
    sentences.sort();
    sentences.dedup();
    sentences.sort_by_key(|s| std::cmp::Reverse(s.chars().count()));
    sentences.truncate(40);
    if !sentences.is_empty() {
        let md_norm = drift_text(md);
        let lost: Vec<&String> = sentences.iter().filter(|s| !md_norm.contains(*s)).collect();
        if !lost.is_empty() {
            let mut first = lost[0].clone();
            if first.chars().count() > 60 {
                first = first.chars().take(60).collect::<String>() + "...";
            }
            notes.push(format!(
                "{} of the {} prose sentences (>= 60 chars) are not in reply_markdown (first: '{}')",
                lost.len(),
                sentences.len(),
                first
            ));
        }
    }

    notes
}

/// Split `text` on sentence enders (`.`, `!`, `?` followed by whitespace) and newlines,
/// mirroring the plugin's `-split '(?<=[.!?])\s+|\r?\n'` without look-behind.
fn split_sentences(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' || c == '\r' {
            out.push(std::mem::take(&mut cur));
            // collapse a CRLF into one break
            if c == '\r' && i + 1 < chars.len() && chars[i + 1] == '\n' {
                i += 1;
            }
            i += 1;
            continue;
        }
        cur.push(c);
        if matches!(c, '.' | '!' | '?') {
            // a run of whitespace after an ender closes the sentence (kept out of the next).
            let mut j = i + 1;
            let mut saw_ws = false;
            while j < chars.len() && (chars[j] == ' ' || chars[j] == '\t') {
                saw_ws = true;
                j += 1;
            }
            if saw_ws {
                out.push(std::mem::take(&mut cur));
                i = j;
                continue;
            }
        }
        i += 1;
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

// ------------------------------------------------------------------------------- gates

/// The reply of a timeout continuation must pass the checks of a first reply before it counts
/// (`Test-ContinuationReply`, F08-5). Structured mode: a valid reply object, else substantive
/// prose; raw/chore: substantive prose. Returns `(usable, reason)`.
pub fn test_continuation_reply(text: &str, raw: bool) -> (bool, String) {
    let t = text.trim();
    if t.is_empty() {
        return (false, "empty reply".to_string());
    }
    let mut invalid = String::new();
    if !raw {
        match crate::engines::codex::parse_structured(t) {
            Some(_) => return (true, String::new()),
            None => {
                // Mirror the plugin's validation_error phrasing via the ingest gate suffix; the
                // detailed schema error is not surfaced by the tolerant parser, so name the shape.
                invalid = "not one bare JSON object satisfying consult-reply v1".to_string();
                if invalid.chars().count() > 120 {
                    invalid = invalid.chars().take(120).collect::<String>() + "...";
                }
            }
        }
    }
    let gate = ingest::prose_gate(t);
    if gate.substantive {
        return (true, String::new());
    }
    let reason = if !invalid.is_empty() {
        format!("not a valid reply object ({invalid}) and {}", gate.reason)
    } else {
        gate.reason.clone()
    };
    (false, reason)
}

/// The prose gate re-exported for the orchestrator's format-repair eligibility check.
pub fn prose_gate(text: &str) -> ProseGate {
    ingest::prose_gate(text)
}

/// Informational codex stderr lines that are never failure evidence (`$script:InfoStderrRe`).
fn is_informational_stderr(line: &str) -> bool {
    let low = line.to_lowercase();
    low.contains("failed to refresh available models")
        || low.contains("failed to decode models response")
        || (low.contains("model metadata for") && low.contains("not found"))
        || low.starts_with("reading prompt from stdin")
}

/// `Get-KilledTurnFailure` (F08-3): does the killed turn's OWN evidence — its event-stream
/// error and its stderr — name a quota or auth failure? A continuation is forbidden after one.
/// Returns `Some((class, one-line text))` for a quota/auth failure, `None` otherwise. (The
/// plugin's full diagnostic-line lexing is reduced here to: the event error, then each stderr
/// line that parses as a provider error or contains an error/status token — informational lines
/// excluded — classified through the one classifier.)
pub fn get_killed_turn_failure(event_error: &str, stderr_text: &str) -> Option<(String, String)> {
    let mut candidates: Vec<String> = Vec::new();
    let et = event_error.trim();
    if !et.is_empty() {
        candidates.push(et.to_string());
    }
    for line in stderr_text.split(['\r', '\n']) {
        let l = line.trim();
        if l.is_empty() || is_informational_stderr(l) {
            continue;
        }
        let (code, msg) = c3_core::health::convert_from_provider_error_text(l);
        if !code.is_empty() || !msg.is_empty() || l.to_lowercase().contains("error") {
            candidates.push(l.to_string());
        }
    }
    for c in candidates {
        let (code, msg) = c3_core::health::convert_from_provider_error_text(&c);
        let class = c3_core::health::provider_failure_class(&format!("{code} {msg}"));
        if class == "quota" || class == "auth" {
            return Some((class, c3_core::one_line(&c)));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use c3_core::engine::{PriorStatus, ReplyPriorFinding, StructuredReply, Verdict};

    const ITEMS_STREAM: &str = concat!(
        r#"{"type":"item.completed","item":{"id":"item_0","type":"reasoning","text":"Reading the brief (first turn)."}}"#,
        "\n",
        r#"{"type":"item.started","item":{"id":"item_1","type":"command_execution","command":"git diff --stat HEAD~1 (first)","status":"in_progress"}}"#,
        "\n",
        r#"{"type":"item.completed","item":{"id":"item_1","type":"command_execution","command":"git diff --stat HEAD~1 (first)","status":"completed"}}"#,
        "\n",
        r#"{"type":"item.completed","item":{"id":"item_2","type":"agent_message","text":"Q1 so far (first turn): the change looks consistent."}}"#,
        "\n"
    );

    #[test]
    fn salvage_collects_items_and_dedups_tools() {
        let s = read_codex_salvage(ITEMS_STREAM);
        assert_eq!(s.items.len(), 2);
        assert_eq!(s.items[0].kind, "reasoning");
        assert_eq!(s.items[1].kind, "message");
        assert_eq!(s.tools.len(), 1, "the command is listed once");
        assert_eq!(s.tools[0], "shell: git diff --stat HEAD~1 (first)");
    }

    #[test]
    fn partial_body_shape() {
        let s = read_codex_salvage(ITEMS_STREAM);
        let body = format_partial_body(&[PartialTurn {
            label: "Turn 1 - the main turn".into(),
            note: "killed at 902.1 s of 900 s".into(),
            salvage: s,
        }]);
        assert!(body.contains("## Turn 1 - the main turn - killed at 902.1 s of 900 s"));
        assert!(body.contains("**Reasoning 1:**"));
        assert!(body.contains("**Agent message 1:**"));
        assert!(body.contains("**Tool calls (1):**"));
        assert!(body.contains("- `shell: git diff --stat HEAD~1 (first)`"));
    }

    #[test]
    fn partial_body_no_items() {
        let body = format_partial_body(&[PartialTurn {
            label: "Turn 1 - the main turn".into(),
            note: "killed at 5 s of 4 s".into(),
            salvage: Salvage::default(),
        }]);
        assert!(body.contains("_(no agent message or reasoning text in this turn's event stream)_"));
        assert!(body.contains("**Tool calls (0):** none"));
    }

    fn reply_with(md: &str, verdict: Verdict) -> StructuredReply {
        StructuredReply {
            verdict,
            verdict_reason: "r".into(),
            reply_markdown: md.into(),
            findings: vec![],
            prior_findings: vec![],
            unproven: vec![],
            first_run_checklist: vec![],
        }
    }

    #[test]
    fn drift_notes_verdict_and_lost_sentence() {
        let prose = "Verdict: REJECT. This is a fairly long sentence about the change that should exceed sixty characters comfortably.";
        let reply = reply_with(
            "A short markdown body that does not contain the sentence.",
            Verdict::Accept,
        );
        let notes = get_format_repair_drift(prose, &reply);
        assert!(notes
            .iter()
            .any(|n| n.starts_with("verdict differs: prose REJECT, JSON ACCEPT")));
        assert!(notes
            .iter()
            .any(|n| n.contains("prose sentences (>= 60 chars) are not in reply_markdown")));
    }

    #[test]
    fn drift_finding_id_missing() {
        let prose = "See F03-2 for details on the lock.";
        let mut reply = reply_with("no ids here", Verdict::Advise);
        // With no prior/finding carrying F03-2, the prose id is flagged.
        let notes = get_format_repair_drift(prose, &reply);
        assert!(notes.iter().any(|n| n.contains("F03-2")));
        // When a prior finding carries it, no such note.
        reply.prior_findings.push(ReplyPriorFinding {
            id: "F03-2".into(),
            status: PriorStatus::Fixed,
            note: String::new(),
        });
        let notes2 = get_format_repair_drift(prose, &reply);
        assert!(!notes2
            .iter()
            .any(|n| n.contains("absent from prior_findings")));
    }

    #[test]
    fn drift_clean_when_prose_matches() {
        let md =
            "Verdict: ACCEPT. The change is consistent with the surrounding module and is safe.";
        let reply = reply_with(md, Verdict::Accept);
        let notes = get_format_repair_drift(md, &reply);
        assert!(notes.is_empty(), "no drift: {notes:?}");
    }

    #[test]
    fn continuation_gate_accepts_valid_object_and_substantive_prose() {
        let json = r#"{"schema_version":"1","verdict":"ADVISE","verdict_reason":"r","reply_markdown":"m","findings":[],"prior_findings":[],"unproven":[],"first_run_checklist":[]}"#;
        assert!(test_continuation_reply(json, false).0);
        // a one-line "Done." is not usable
        let (usable, reason) = test_continuation_reply("Done.", false);
        assert!(!usable);
        assert!(reason.contains("not a valid reply object"));
        // empty
        assert_eq!(test_continuation_reply("   ", false).1, "empty reply");
    }
}
