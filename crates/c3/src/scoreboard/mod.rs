//! The scoreboard behind `c3 scoreboard` (milestone 3): per (reviewer lineage,
//! purpose) counts over `sessions.json` and `findings.json` — consults, usable,
//! prose, failed, findings by status, verdict letters, Y/P/N marks, median wall
//! time, tokens, HIT% — read-only, no lock.
//!
//! A byte-for-byte port of `codex-scoreboard.ps1`. Stores are read tolerantly as
//! `serde_json::Value` (as the plugin's `ConvertFrom-Json` + `Get-PropertyValue` do), so a
//! store that does not parse is reported and left out rather than failing the run. The
//! `--json` form serializes the row array with the PowerShell-5.1 `ConvertTo-Json`
//! formatter ([`c3_core::ps_json`]) so it matches the plugin's `-Json` output.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Serialize;
use serde_json::Value;

use c3_core::health::is_usable_outcome;
use c3_core::lineage::format_reviewer_lineage;
use c3_core::task_slug;

use crate::providers::{resolve_collab_root, resolve_repo_root};

const TOOL: &str = "codex-scoreboard";

/// Run `c3 scoreboard`; returns the process exit code.
pub fn run(collab_dir: &str, task: &str, json: bool) -> i32 {
    match run_inner(collab_dir, task, json) {
        Ok(code) => code,
        Err(msg) => {
            println!("{TOOL}: {msg}");
            1
        }
    }
}

// ----------------------------------------------------------------------------- accessors

fn prop<'a>(obj: Option<&'a Value>, name: &str) -> Option<&'a Value> {
    // Get-PropertyValue: the value only when present AND not null.
    obj.and_then(|v| v.get(name)).filter(|v| !v.is_null())
}

fn prop_str(obj: Option<&Value>, name: &str) -> String {
    match prop(obj, name) {
        Some(Value::String(s)) => s.clone(),
        Some(v) => value_to_string(v),
        None => String::new(),
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => {
            // PowerShell stringifies booleans as True/False.
            if *b {
                "True".to_string()
            } else {
                "False".to_string()
            }
        }
        Value::Number(n) => n.to_string(),
        Value::Null => String::new(),
        _ => v.to_string(),
    }
}

fn prop_i64(obj: Option<&Value>, name: &str) -> Option<i64> {
    // [long]::TryParse over the string form (integers only; a fractional string fails).
    match prop(obj, name) {
        Some(Value::Number(n)) => n.as_i64(),
        Some(Value::String(s)) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

fn prop_f64(obj: Option<&Value>, name: &str) -> Option<f64> {
    match prop(obj, name) {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

// ----------------------------------------------------------------------------- accumulator

#[derive(Default, Clone)]
struct Acc {
    lineage: String,
    purpose: String,
    consults: i64,
    usable: i64,
    prose: i64,
    failed: i64,
    raised: i64,
    verified: i64,
    rejected: i64,
    wontfix: i64,
    superseded: i64,
    open: i64,
    accept: i64,
    hold: i64,
    reject: i64,
    advise: i64,
    yes: i64,
    partly: i64,
    no: i64,
    walls: Vec<f64>,
    tokens_in: i64,
    tokens_out: i64,
}

/// One output row (`New-OutRow`): the JSON field order is the plugin's pscustomobject order.
#[derive(Serialize)]
struct OutRow {
    kind: String,
    lineage: String,
    purpose: String,
    consults: i64,
    usable: i64,
    prose: i64,
    failed: i64,
    raised: i64,
    verified: i64,
    rejected: i64,
    wontfix: i64,
    superseded: i64,
    open: i64,
    hit_rate: Option<i64>,
    verdict_accept: i64,
    verdict_hold: i64,
    verdict_reject: i64,
    verdict_advise: i64,
    rated_yes: i64,
    rated_partly: i64,
    rated_no: i64,
    median_wall_seconds: Option<f64>,
    tokens_in: i64,
    tokens_out: i64,
}

struct Board {
    // key "<lineage>\t<purpose>" -> accumulator; insertion order is irrelevant (output is
    // sorted explicitly), so a plain map is enough.
    rows: HashMap<String, Acc>,
}

impl Board {
    fn new() -> Self {
        Board {
            rows: HashMap::new(),
        }
    }

    fn acc(&mut self, lineage: &str, purpose: &str) -> &mut Acc {
        let key = format!("{lineage}\t{purpose}");
        self.rows.entry(key).or_insert_with(|| Acc {
            lineage: lineage.to_string(),
            purpose: purpose.to_string(),
            ..Default::default()
        })
    }
}

fn run_inner(collab_dir: &str, task: &str, json: bool) -> Result<i32, String> {
    if !task.is_empty() && !task_slug::is_slug(task) {
        return Err("-Task must be a slug (letters, digits, dot, dash, underscore).".to_string());
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = resolve_repo_root(&cwd);
    let collab_root = resolve_collab_root(&repo_root, collab_dir);

    let mut task_dirs: Vec<PathBuf> = Vec::new();
    if !task.is_empty() {
        let one = collab_root.join(task);
        if !one.is_dir() {
            return Err(format!("no task '{task}' under {}.", collab_root.display()));
        }
        task_dirs.push(one);
    } else if collab_root.is_dir() {
        let mut names: Vec<PathBuf> = std::fs::read_dir(&collab_root)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.path().is_dir())
                    .map(|e| e.path())
                    .collect()
            })
            .unwrap_or_default();
        names.sort_by(|a, b| {
            a.file_name()
                .unwrap_or_default()
                .cmp(b.file_name().unwrap_or_default())
        });
        task_dirs = names;
    }

    let mut notes: Vec<String> = Vec::new();
    let out = build_rows(&task_dirs, &mut notes);

    if json {
        print_json(&out);
        return Ok(0);
    }
    print_table(&collab_root, task, task_dirs.len(), &notes, &out);
    Ok(0)
}

/// Accumulate every task directory's stores into the sorted output rows (`purpose` rows,
/// each lineage's `(total)`, then the grand `(all) (total)`).
fn build_rows(task_dirs: &[PathBuf], notes: &mut Vec<String>) -> Vec<OutRow> {
    let mut board = Board::new();

    for dir in task_dirs {
        let sessions = read_tolerant(&dir.join("sessions.json"), notes);
        let store = read_tolerant(&dir.join("findings.json"), notes);

        // consult n -> the accumulator key it belongs to.
        let mut key_of: HashMap<i64, String> = HashMap::new();

        let consults = sessions
            .as_ref()
            .and_then(|s| prop(Some(s), "codex"))
            .and_then(|c| prop(Some(c), "consults"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for c in &consults {
            if c.is_null() {
                continue;
            }
            let rev = prop(Some(c), "reviewer");
            let lineage = if rev.is_some() {
                format_reviewer_lineage(
                    &prop_str(rev, "provider"),
                    &prop_str(rev, "model"),
                    &prop_str(rev, "engine"),
                )
            } else {
                "unknown provenance".to_string()
            };
            let purpose = {
                let p = prop_str(Some(c), "purpose");
                if p.is_empty() {
                    "(none)".to_string()
                } else {
                    p
                }
            };
            let key = format!("{lineage}\t{purpose}");
            let acc = board.acc(&lineage, &purpose);
            if let Some(n) = prop_i64(Some(c), "n") {
                key_of.insert(n, key);
            }
            acc.consults += 1;
            let outcome = {
                let o = prop_str(Some(c), "bridge_outcome");
                if o.is_empty() {
                    prop_str(Some(c), "outcome")
                } else {
                    o
                }
            };
            if is_usable_outcome(&outcome) {
                acc.usable += 1;
                let structured = matches!(prop(Some(c), "structured"), Some(Value::Bool(true)));
                if !structured {
                    acc.prose += 1;
                }
            } else {
                acc.failed += 1;
            }
            match prop_str(Some(c), "verdict").as_str() {
                "ACCEPT" => acc.accept += 1,
                "HOLD" => acc.hold += 1,
                "REJECT" => acc.reject += 1,
                "ADVISE" => acc.advise += 1,
                _ => {}
            }
            if let Some(w) = prop_f64(Some(c), "wall_seconds") {
                acc.walls.push(w);
            }
            if let Some(usage) = prop(Some(c), "usage") {
                let usage = Some(usage);
                if let Some(input) = prop_i64(usage, "input_tokens") {
                    let cached = prop_i64(usage, "cached_input_tokens").unwrap_or(0);
                    acc.tokens_in += (input - cached).max(0);
                }
                if let Some(out) = prop_i64(usage, "output_tokens") {
                    acc.tokens_out += out;
                }
            }
        }

        let findings = store
            .as_ref()
            .and_then(|s| prop(Some(s), "findings"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for f in &findings {
            if f.is_null() {
                continue;
            }
            let n = prop_i64(prop(Some(f), "source"), "consult");
            let key = match n.and_then(|n| key_of.get(&n).cloned()) {
                Some(k) => k,
                None => {
                    board.acc("unknown provenance", "(none)");
                    "unknown provenance\t(none)".to_string()
                }
            };
            let acc = board.rows.get_mut(&key).expect("acc present");
            acc.raised += 1;
            match prop_str(Some(f), "status").as_str() {
                "verified" => acc.verified += 1,
                "rejected" => acc.rejected += 1,
                "wontfix" => acc.wontfix += 1,
                "superseded" => acc.superseded += 1,
                "proposed" => acc.open += 1,
                "implemented" => acc.open += 1,
                _ => {}
            }
        }

        let ratings = store
            .as_ref()
            .and_then(|s| prop(Some(s), "ratings"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for rt in &ratings {
            if rt.is_null() {
                continue;
            }
            let n = prop_i64(Some(rt), "n");
            let key = match n.and_then(|n| key_of.get(&n).cloned()) {
                Some(k) => k,
                None => {
                    let lineage = {
                        let l = prop_str(Some(rt), "lineage");
                        if l.is_empty() {
                            "unknown provenance".to_string()
                        } else {
                            l
                        }
                    };
                    let purpose = {
                        let p = prop_str(Some(rt), "purpose");
                        if p.is_empty() {
                            "(none)".to_string()
                        } else {
                            p
                        }
                    };
                    board.acc(&lineage, &purpose);
                    format!("{lineage}\t{purpose}")
                }
            };
            let acc = board.rows.get_mut(&key).expect("acc present");
            match prop_str(Some(rt), "useful").as_str() {
                "yes" => acc.yes += 1,
                "partly" => acc.partly += 1,
                "no" => acc.no += 1,
                _ => {}
            }
        }
    }

    let all: Vec<Acc> = board.rows.values().cloned().collect();

    // lineages: unknown provenance last, then case-insensitive, unique.
    let mut lineages: Vec<String> = Vec::new();
    for a in &all {
        if !lineages.contains(&a.lineage) {
            lineages.push(a.lineage.clone());
        }
    }
    lineages.sort_by(|a, b| {
        let ka = (a == "unknown provenance", a.to_lowercase());
        let kb = (b == "unknown provenance", b.to_lowercase());
        ka.cmp(&kb)
    });

    let mut out: Vec<OutRow> = Vec::new();
    for lin in &lineages {
        let mut mine: Vec<&Acc> = all.iter().filter(|a| &a.lineage == lin).collect();
        mine.sort_by_key(|a| a.purpose.to_lowercase());
        for a in &mine {
            out.push(new_out_row("purpose", lin, &a.purpose, &[a]));
        }
        out.push(new_out_row("lineage", lin, "(total)", &mine));
    }
    if !all.is_empty() {
        let all_refs: Vec<&Acc> = all.iter().collect();
        out.push(new_out_row("total", "(all)", "(total)", &all_refs));
    }
    out
}

fn read_tolerant(path: &PathBuf, notes: &mut Vec<String>) -> Option<Value> {
    if !path.is_file() {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(v) => Some(v),
        Err(e) => {
            notes.push(format!(
                "left out: {} does not parse ({})",
                path.display(),
                c3_core::one_line(&e.to_string())
            ));
            None
        }
    }
}

fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        Some(sorted[mid])
    } else {
        Some((sorted[mid - 1] + sorted[mid]) / 2.0)
    }
}

fn new_out_row(kind: &str, lineage: &str, purpose: &str, parts: &[&Acc]) -> OutRow {
    let mut s = Acc::default();
    let mut walls: Vec<f64> = Vec::new();
    for a in parts {
        s.consults += a.consults;
        s.usable += a.usable;
        s.prose += a.prose;
        s.failed += a.failed;
        s.raised += a.raised;
        s.verified += a.verified;
        s.rejected += a.rejected;
        s.wontfix += a.wontfix;
        s.superseded += a.superseded;
        s.open += a.open;
        s.accept += a.accept;
        s.hold += a.hold;
        s.reject += a.reject;
        s.advise += a.advise;
        s.yes += a.yes;
        s.partly += a.partly;
        s.no += a.no;
        s.tokens_in += a.tokens_in;
        s.tokens_out += a.tokens_out;
        walls.extend(a.walls.iter().copied());
    }
    let hit = if s.verified + s.rejected > 0 {
        Some(round_away(
            100.0 * s.verified as f64 / (s.verified + s.rejected) as f64,
        ))
    } else {
        None
    };
    OutRow {
        kind: kind.to_string(),
        lineage: lineage.to_string(),
        purpose: purpose.to_string(),
        consults: s.consults,
        usable: s.usable,
        prose: s.prose,
        failed: s.failed,
        raised: s.raised,
        verified: s.verified,
        rejected: s.rejected,
        wontfix: s.wontfix,
        superseded: s.superseded,
        open: s.open,
        hit_rate: hit,
        verdict_accept: s.accept,
        verdict_hold: s.hold,
        verdict_reject: s.reject,
        verdict_advise: s.advise,
        rated_yes: s.yes,
        rated_partly: s.partly,
        rated_no: s.no,
        median_wall_seconds: median(&walls),
        tokens_in: s.tokens_in,
        tokens_out: s.tokens_out,
    }
}

/// `[Math]::Round(x, AwayFromZero)` to a whole number.
fn round_away(x: f64) -> i64 {
    x.round() as i64
}

fn print_json(out: &[OutRow]) {
    // `ConvertTo-Json -InputObject <array> -Depth 4`, in the PowerShell-5.1 shape.
    match c3_core::ps_json::to_ps_json(&out) {
        Ok(s) => println!("{s}"),
        Err(_) => println!("[]"),
    }
}

fn print_table(
    collab_root: &std::path::Path,
    task: &str,
    task_count: usize,
    notes: &[String],
    out: &[OutRow],
) {
    let suffix = if !task.is_empty() {
        format!(" (task {task})")
    } else {
        // The plugin prints the literal "task(s)", never a pluralised form.
        format!(" ({task_count} task(s))")
    };
    println!("{TOOL}: {}{}", collab_root.display(), suffix);
    for note in notes {
        println!("{note}");
    }
    if out.is_empty() {
        println!("no consultations recorded.");
        return;
    }

    // Column order and header labels (the plugin's $header ordered map).
    let cols: [&str; 17] = [
        "reviewer",
        "purpose",
        "consults",
        "usable",
        "prose",
        "failed",
        "raised",
        "verified",
        "rejected",
        "wontfix",
        "superseded",
        "open",
        "hit",
        "verdicts",
        "ratings",
        "median",
        "tokens",
    ];
    let headers: [&str; 17] = [
        "REVIEWER",
        "PURPOSE",
        "CONSULTS",
        "USABLE",
        "PROSE",
        "FAILED",
        "RAISED",
        "VERIFIED",
        "REJECTED",
        "WONTFIX",
        "SUPERSEDED",
        "OPEN",
        "HIT%",
        "A/H/R/D",
        "Y/P/N",
        "MEDIAN_S",
        "TOKENS",
    ];
    let numeric: [&str; 12] = [
        "consults",
        "usable",
        "prose",
        "failed",
        "raised",
        "verified",
        "rejected",
        "wontfix",
        "superseded",
        "open",
        "hit",
        "median",
    ];

    // Build every line's per-column cells (header first, then data rows).
    let mut lines: Vec<Vec<String>> = Vec::new();
    lines.push(headers.iter().map(|s| s.to_string()).collect());
    for r in out {
        lines.push(row_cells(r));
    }

    // Column widths: max cell length per column across every line.
    let mut width = [0usize; 17];
    for line in &lines {
        for (i, cell) in line.iter().enumerate() {
            width[i] = width[i].max(cell.chars().count());
        }
    }

    for line in &lines {
        let mut parts: Vec<String> = Vec::with_capacity(17);
        for (i, cell) in line.iter().enumerate() {
            let w = width[i];
            let len = cell.chars().count();
            let pad = w.saturating_sub(len);
            if numeric.contains(&cols[i]) {
                parts.push(format!("{}{}", " ".repeat(pad), cell));
            } else {
                parts.push(format!("{}{}", cell, " ".repeat(pad)));
            }
        }
        println!("{}", parts.join("  ").trim_end());
    }
}

fn row_cells(r: &OutRow) -> Vec<String> {
    vec![
        r.lineage.clone(),
        r.purpose.clone(),
        r.consults.to_string(),
        r.usable.to_string(),
        r.prose.to_string(),
        r.failed.to_string(),
        r.raised.to_string(),
        r.verified.to_string(),
        r.rejected.to_string(),
        r.wontfix.to_string(),
        r.superseded.to_string(),
        r.open.to_string(),
        match r.hit_rate {
            Some(h) => format!("{h}%"),
            None => "-".to_string(),
        },
        format!(
            "{}/{}/{}/{}",
            r.verdict_accept, r.verdict_hold, r.verdict_reject, r.verdict_advise
        ),
        format!("{}/{}/{}", r.rated_yes, r.rated_partly, r.rated_no),
        match r.median_wall_seconds {
            Some(m) => format_one_decimal(m),
            None => "-".to_string(),
        },
        format!("{}/{}", r.tokens_in, r.tokens_out),
    ]
}

/// `.ToString('0.#')`: at most one fractional digit, trailing `.0` dropped. .NET Framework
/// (PowerShell 5.1) first rounds the double to 15 significant digits, then applies the
/// format with round-half-away-from-zero; a value like `150.14999999999998` therefore
/// becomes `150.15` -> `150.2`, not `150.1`. The 15-sig-digit pre-round is reproduced by
/// round-tripping through `%.14e`.
fn format_one_decimal(x: f64) -> String {
    let x = format!("{x:.14e}").parse::<f64>().unwrap_or(x);
    let r = (x * 10.0).round() / 10.0;
    if r.fract() == 0.0 {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn median_and_formatting() {
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[10.0]), Some(10.0));
        assert_eq!(median(&[10.0, 20.0]), Some(15.0));
        assert_eq!(median(&[1.0, 2.0, 3.0]), Some(2.0));
        // .NET ToString('0.#') with the 15-sig-digit pre-round.
        assert_eq!(format_one_decimal(150.14999999999998), "150.2");
        assert_eq!(format_one_decimal(47.0), "47");
        assert_eq!(format_one_decimal(763.1), "763.1");
        assert_eq!(round_away(66.5), 67);
    }

    #[test]
    fn aggregates_one_task_into_three_rows() {
        let dir = std::env::temp_dir().join(format!("c3sb-{}", std::process::id()));
        let task = dir.join("t");
        std::fs::create_dir_all(&task).unwrap();
        let sessions = json!({
            "task_id": "t",
            "codex": {"consults": [{
                "n": 1,
                "purpose": "decision",
                "consult_id": "c1",
                "reviewer": {"provider": "openai", "model": "gpt-5.1"},
                "bridge_outcome": "usable reply",
                "structured": true,
                "verdict": "ACCEPT",
                "wall_seconds": 10,
                "usage": {"input_tokens": 100, "cached_input_tokens": 20, "output_tokens": 50}
            }]}
        });
        let findings = json!({
            "task_id": "t",
            "findings": [{"id": "F02-1", "status": "verified", "source": {"consult": 1}}],
            "ratings": [{"n": 1, "useful": "yes", "lineage": "openai :: gpt-5.1", "purpose": "decision"}]
        });
        std::fs::write(
            task.join("sessions.json"),
            serde_json::to_vec(&sessions).unwrap(),
        )
        .unwrap();
        std::fs::write(
            task.join("findings.json"),
            serde_json::to_vec(&findings).unwrap(),
        )
        .unwrap();

        let mut notes = Vec::new();
        let rows = build_rows(std::slice::from_ref(&task), &mut notes);
        std::fs::remove_dir_all(&dir).ok();

        assert_eq!(rows.len(), 3, "purpose + lineage-total + grand-total");
        let p = &rows[0];
        assert_eq!(p.kind, "purpose");
        assert_eq!(p.lineage, "openai :: gpt-5.1");
        assert_eq!(p.purpose, "decision");
        assert_eq!(p.consults, 1);
        assert_eq!(p.usable, 1);
        assert_eq!(p.prose, 0);
        assert_eq!(p.raised, 1);
        assert_eq!(p.verified, 1);
        assert_eq!(p.hit_rate, Some(100));
        assert_eq!(p.verdict_accept, 1);
        assert_eq!(p.rated_yes, 1);
        assert_eq!(p.tokens_in, 80);
        assert_eq!(p.tokens_out, 50);
        assert_eq!(p.median_wall_seconds, Some(10.0));
        assert_eq!(rows[2].kind, "total");
        assert_eq!(rows[2].lineage, "(all)");
    }
}
