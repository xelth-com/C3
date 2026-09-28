//! The scoreboard behind `c3 scoreboard` (milestone 3): per (reviewer lineage, purpose) -
//! or, with `--by topic`, per (reviewer lineage, topic) - counts over `sessions.json` and
//! `findings.json` - consults, usable, prose, failed, findings by status, verdict letters,
//! Y/P/N marks, the wave-26 routing SCORE, the UNIQ panel measure, median wall time, tokens,
//! HIT% - read-only, no lock.
//!
//! A byte-for-byte port of `codex-scoreboard.ps1`. Stores are read tolerantly as
//! `serde_json::Value` (as the plugin's `ConvertFrom-Json` + `Get-PropertyValue` do), so a
//! store that does not parse is reported and left out rather than failing the run. The
//! `--json` form serializes the row array with the PowerShell-5.1 `ConvertTo-Json`
//! formatter ([`c3_core::ps_json`]) so it matches the plugin's `-Json` output.
//!
//! ## Wave 26 (D2, D10)
//!
//! * `--by purpose` (the default) | `topic`: `topic` rows one per topic of a consultation's
//!   `topics[]`; a consultation with two topics counts on both, once on its lineage total.
//! * `SCORE` [`score`]: the routing score a routed panel gives this reviewer for this purpose
//!   (a lineage total: its all-purpose score) - [`routing`] over the marks of every task of
//!   the repository from the last 90 days; `-` on a topic row and on the grand total.
//! * `UNIQ` [`unique`/`panel_raised`]: findings this reviewer raised as a panel member that no
//!   other member of the same panel raised at the same location, of all it raised in panels.
//! * A mark joins its consultation by `consult_id` (a mark recorded before wave 26 without
//!   one, by its `n` within the task).

mod routing;

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::Utc;
use serde::Serialize;
use serde_json::Value;

use c3_core::health::is_usable_outcome;
use c3_core::lineage::format_reviewer_lineage;
use c3_core::task_slug;

use crate::providers::{resolve_collab_root, resolve_repo_root};

const TOOL: &str = "codex-scoreboard";

/// Run `c3 scoreboard`; returns the process exit code.
pub fn run(collab_dir: &str, task: &str, json: bool, by: &str) -> i32 {
    match run_inner(collab_dir, task, json, by) {
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
    unique: i64,
    panel_raised: i64,
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
    /// (wave 26) the routing score; `null` on a topic row and on the grand total.
    score: Option<f64>,
    unique: i64,
    panel_raised: i64,
    median_wall_seconds: Option<f64>,
    tokens_in: i64,
    tokens_out: i64,
}

/// A target accumulator: a `(lineage, key)` row or a lineage's total row.
#[derive(Clone)]
enum Tgt {
    Row(String, String),
    Total(String),
}

struct Board {
    /// key "<lineage>\t<rowkey>" -> the (lineage, key) accumulator.
    rows: HashMap<String, Acc>,
    /// lineage -> its total-row accumulator (a consultation counts here ONCE, whatever rows it
    /// is on).
    total_rows: HashMap<String, Acc>,
    /// lineage -> its reviewer identity `(provider, model, engine)` for the routing score.
    identity_of: HashMap<String, (String, String, String)>,
}

impl Board {
    fn new() -> Self {
        Board {
            rows: HashMap::new(),
            total_rows: HashMap::new(),
            identity_of: HashMap::new(),
        }
    }

    /// Ensure a `(lineage, key)` row and the lineage total exist, and return the targets to
    /// increment for a consultation on those keys (`Get-Accs`).
    fn get_accs(&mut self, lineage: &str, keys: &[String]) -> Vec<Tgt> {
        let mut list = Vec::new();
        for k in keys {
            let rk = format!("{lineage}\t{k}");
            self.rows.entry(rk).or_insert_with(|| Acc {
                lineage: lineage.to_string(),
                purpose: k.clone(),
                ..Default::default()
            });
            list.push(Tgt::Row(lineage.to_string(), k.clone()));
        }
        self.total_rows
            .entry(lineage.to_string())
            .or_insert_with(|| Acc {
                lineage: lineage.to_string(),
                purpose: "(total)".to_string(),
                ..Default::default()
            });
        list.push(Tgt::Total(lineage.to_string()));
        list
    }

    fn apply(&mut self, tgt: &Tgt, f: &mut impl FnMut(&mut Acc)) {
        match tgt {
            Tgt::Row(lin, k) => {
                if let Some(a) = self.rows.get_mut(&format!("{lin}\t{k}")) {
                    f(a);
                }
            }
            Tgt::Total(lin) => {
                if let Some(a) = self.total_rows.get_mut(lin) {
                    f(a);
                }
            }
        }
    }
}

/// The row keys of a consultation (or of a mark without one): its purpose, or (`--by topic`)
/// each of its topics; `(none)` without (`Get-RowKeys`).
fn row_keys(by_topic: bool, purpose: &str, topics: &[Value]) -> Vec<String> {
    if by_topic {
        let mut seen: Vec<String> = Vec::new();
        for t in topics {
            if t.is_null() {
                continue;
            }
            let s = value_to_string(t).to_lowercase();
            if s.is_empty() {
                continue;
            }
            if !seen.contains(&s) {
                seen.push(s);
            }
        }
        if seen.is_empty() {
            return vec!["(none)".to_string()];
        }
        return seen;
    }
    let p = if purpose.is_empty() {
        "(none)"
    } else {
        purpose
    };
    vec![p.to_string()]
}

/// A finding's location keys (`path:line`; `path:` without a line) for the unique measure.
fn location_keys(finding: &Value) -> Vec<String> {
    let mut keys = Vec::new();
    if let Some(locs) = prop(Some(finding), "locations").and_then(|v| v.as_array()) {
        for l in locs {
            if l.is_null() {
                continue;
            }
            let path = prop_str(Some(l), "path");
            if path.is_empty() {
                continue;
            }
            let line = match prop(Some(l), "line") {
                Some(Value::Number(n)) => n.to_string(),
                Some(v) => value_to_string(v),
                None => String::new(),
            };
            keys.push(format!("{}:{}", path.replace('\\', "/"), line));
        }
    }
    keys
}

fn run_inner(collab_dir: &str, task: &str, json: bool, by: &str) -> Result<i32, String> {
    if !task.is_empty() && !task_slug::is_slug(task) {
        return Err("-Task must be a slug (letters, digits, dot, dash, underscore).".to_string());
    }
    let by = by.trim().to_lowercase();
    if by != "purpose" && by != "topic" {
        return Err(format!("-By must be purpose or topic (got '{by}')."));
    }
    let by_topic = by == "topic";
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
    let out = build_rows(&collab_root, &task_dirs, by_topic, &mut notes);

    if json {
        print_json(&out);
        return Ok(0);
    }
    print_table(&collab_root, task, task_dirs.len(), by_topic, &notes, &out);
    Ok(0)
}

/// Accumulate every task directory's stores into the sorted output rows (`purpose`/`topic`
/// rows, each lineage's `(total)`, then the grand `(all) (total)`).
fn build_rows(
    collab_root: &std::path::Path,
    task_dirs: &[PathBuf],
    by_topic: bool,
    notes: &mut Vec<String>,
) -> Vec<OutRow> {
    let mut board = Board::new();

    // (wave 26, D2) every task's findings.json is read ONCE: the shown tasks' rows and the
    // routing scores over the marks of every task of the repository.
    let mut store_of: HashMap<String, Option<Value>> = HashMap::new();
    if collab_root.is_dir() {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(collab_root)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.path().is_dir())
                    .map(|e| e.path())
                    .collect()
            })
            .unwrap_or_default();
        dirs.sort();
        for d in &dirs {
            let name = d
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            store_of.insert(name, read_tolerant(&d.join("findings.json"), notes));
        }
    }

    for dir in task_dirs {
        let name = dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let sessions = read_tolerant(&dir.join("sessions.json"), notes);
        let store = match store_of.get(&name) {
            Some(v) => v.clone(),
            None => read_tolerant(&dir.join("findings.json"), notes),
        };

        // consult n / consult_id -> the accumulator targets it belongs to; its panel id.
        let mut key_of: HashMap<i64, Vec<Tgt>> = HashMap::new();
        let mut key_of_id: HashMap<String, Vec<Tgt>> = HashMap::new();
        let mut panel_of: HashMap<i64, String> = HashMap::new();

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
                let engine = {
                    let e = prop_str(rev, "engine");
                    if e.is_empty() {
                        "codex".to_string()
                    } else {
                        e
                    }
                };
                let l = format_reviewer_lineage(
                    &prop_str(rev, "provider"),
                    &prop_str(rev, "model"),
                    &engine,
                );
                board
                    .identity_of
                    .entry(l.clone())
                    .or_insert_with(|| (prop_str(rev, "provider"), prop_str(rev, "model"), engine));
                l
            } else {
                "unknown provenance".to_string()
            };
            let topics = prop(Some(c), "topics")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let keys = row_keys(by_topic, &prop_str(Some(c), "purpose"), &topics);
            let accs = board.get_accs(&lineage, &keys);
            if let Some(n) = prop_i64(Some(c), "n") {
                key_of.insert(n, accs.clone());
                let panel_id = prop_str(prop(Some(c), "panel"), "id");
                if !panel_id.is_empty() {
                    panel_of.insert(n, panel_id);
                }
            }
            let cid = prop_str(Some(c), "consult_id");
            if !cid.is_empty() {
                key_of_id.insert(cid, accs.clone());
            }

            let outcome = {
                let o = prop_str(Some(c), "bridge_outcome");
                if o.is_empty() {
                    prop_str(Some(c), "outcome")
                } else {
                    o
                }
            };
            let usable = is_usable_outcome(&outcome);
            let prose = usable && !matches!(prop(Some(c), "structured"), Some(Value::Bool(true)));
            let verdict = prop_str(Some(c), "verdict");
            let wall = prop_f64(Some(c), "wall_seconds");
            let (mut tin, mut tout) = (0i64, 0i64);
            if let Some(usage) = prop(Some(c), "usage") {
                let usage = Some(usage);
                if let Some(input) = prop_i64(usage, "input_tokens") {
                    let cached = prop_i64(usage, "cached_input_tokens").unwrap_or(0);
                    tin = (input - cached).max(0);
                }
                if let Some(o) = prop_i64(usage, "output_tokens") {
                    tout = o;
                }
            }
            let mut bump = |a: &mut Acc| {
                a.consults += 1;
                if usable {
                    a.usable += 1;
                    if prose {
                        a.prose += 1;
                    }
                } else {
                    a.failed += 1;
                }
                match verdict.as_str() {
                    "ACCEPT" => a.accept += 1,
                    "HOLD" => a.hold += 1,
                    "REJECT" => a.reject += 1,
                    "ADVISE" => a.advise += 1,
                    _ => {}
                }
                if let Some(w) = wall {
                    a.walls.push(w);
                }
                a.tokens_in += tin;
                a.tokens_out += tout;
            };
            for t in &accs {
                board.apply(t, &mut bump);
            }
        }

        let findings = store
            .as_ref()
            .and_then(|s| prop(Some(s), "findings"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let findings: Vec<Value> = findings.into_iter().filter(|f| !f.is_null()).collect();
        // (wave 26, D10) the unique-findings measure: a finding by a panel member is unique when
        // no finding of ANOTHER member of the same panel shares one of its locations.
        let locs_of: Vec<Vec<String>> = findings.iter().map(location_keys).collect();
        for (i, f) in findings.iter().enumerate() {
            let n = prop_i64(prop(Some(f), "source"), "consult");
            let accs = match n.and_then(|n| key_of.get(&n).cloned()) {
                Some(a) => a,
                None => board.get_accs("unknown provenance", &["(none)".to_string()]),
            };
            let in_panel = n.map(|n| panel_of.contains_key(&n)).unwrap_or(false);
            let mut unique = false;
            if in_panel {
                unique = true;
                let mine = &locs_of[i];
                if !mine.is_empty() {
                    let np = n.unwrap();
                    let my_panel = &panel_of[&np];
                    for (j, _fj) in findings.iter().enumerate() {
                        if j == i {
                            continue;
                        }
                        let nj = prop_i64(prop(Some(&findings[j]), "source"), "consult");
                        let share_panel = match nj {
                            Some(nj) if nj != np => {
                                panel_of.get(&nj).map(|p| p == my_panel).unwrap_or(false)
                            }
                            _ => false,
                        };
                        if !share_panel {
                            continue;
                        }
                        if locs_of[j].iter().any(|k| mine.contains(k)) {
                            unique = false;
                            break;
                        }
                    }
                }
            }
            let status = prop_str(Some(f), "status");
            let mut bump = |a: &mut Acc| {
                a.raised += 1;
                match status.as_str() {
                    "verified" => a.verified += 1,
                    "rejected" => a.rejected += 1,
                    "wontfix" => a.wontfix += 1,
                    "superseded" => a.superseded += 1,
                    "proposed" | "implemented" => a.open += 1,
                    _ => {}
                }
                if in_panel {
                    a.panel_raised += 1;
                    if unique {
                        a.unique += 1;
                    }
                }
            };
            for t in &accs {
                board.apply(t, &mut bump);
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
            // (wave 26, D2) a mark belongs to the consultation whose consult_id it carries; a
            // mark recorded without one, to its n in this task.
            let cid = prop_str(Some(rt), "consult_id");
            let n = prop_i64(Some(rt), "n");
            let accs = if !cid.is_empty() {
                key_of_id.get(&cid).cloned()
            } else {
                n.and_then(|n| key_of.get(&n).cloned())
            };
            let accs = match accs {
                Some(a) => a,
                None => {
                    let lineage = {
                        let l = prop_str(Some(rt), "lineage");
                        if l.is_empty() {
                            "unknown provenance".to_string()
                        } else {
                            l
                        }
                    };
                    let topics = prop(Some(rt), "topics")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();
                    let keys = row_keys(by_topic, &prop_str(Some(rt), "purpose"), &topics);
                    board.get_accs(&lineage, &keys)
                }
            };
            let useful = prop_str(Some(rt), "useful");
            let mut bump = |a: &mut Acc| match useful.as_str() {
                "yes" => a.yes += 1,
                "partly" => a.partly += 1,
                "no" => a.no += 1,
                _ => {}
            };
            for t in &accs {
                board.apply(t, &mut bump);
            }
        }
    }

    // (wave 26, D2) the routing score of every row over the marks of every task, at the consult
    // clock (CODEX_CONSULT_NOW in tests).
    let score_now = match crate::providers::get_consult_clock_peek() {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(_) => Utc::now(),
    };
    let all_ratings = routing::read_all_task_ratings(collab_root, &store_of);
    let identity_of = board.identity_of.clone();
    let row_score = |lineage: &str, key: &str, total: bool| -> Option<f64> {
        let id = identity_of.get(lineage)?;
        if by_topic && !total {
            return None;
        }
        let purpose = if key == "(none)" { "" } else { key };
        let sc =
            routing::routing_score(&all_ratings, &id.0, &id.1, &id.2, purpose, score_now, total);
        Some(round_even_3(sc))
    };

    // lineages: unknown provenance last, then case-insensitive, unique.
    let mut lineages: Vec<String> = Vec::new();
    for a in board.rows.values() {
        if !lineages.contains(&a.lineage) {
            lineages.push(a.lineage.clone());
        }
    }
    lineages.sort_by_key(|a| (a == "unknown provenance", a.to_lowercase()));

    let kind = if by_topic { "topic" } else { "purpose" };
    let mut out: Vec<OutRow> = Vec::new();
    for lin in &lineages {
        let mut mine: Vec<&Acc> = board.rows.values().filter(|a| &a.lineage == lin).collect();
        mine.sort_by_key(|a| a.purpose.to_lowercase());
        for a in &mine {
            let sc = row_score(lin, &a.purpose, false);
            out.push(new_out_row(kind, lin, &a.purpose, &[a], sc));
        }
        let total = board.total_rows.get(lin);
        let total_slice: Vec<&Acc> = total.into_iter().collect();
        let sc = row_score(lin, "", true);
        out.push(new_out_row("lineage", lin, "(total)", &total_slice, sc));
    }
    if !board.total_rows.is_empty() {
        let all_totals: Vec<&Acc> = board.total_rows.values().collect();
        out.push(new_out_row("total", "(all)", "(total)", &all_totals, None));
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

fn new_out_row(
    kind: &str,
    lineage: &str,
    purpose: &str,
    parts: &[&Acc],
    score: Option<f64>,
) -> OutRow {
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
        s.unique += a.unique;
        s.panel_raised += a.panel_raised;
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
        score,
        unique: s.unique,
        panel_raised: s.panel_raised,
        median_wall_seconds: median(&walls),
        tokens_in: s.tokens_in,
        tokens_out: s.tokens_out,
    }
}

/// `[Math]::Round(x, AwayFromZero)` to a whole number.
fn round_away(x: f64) -> i64 {
    x.round() as i64
}

/// `[Math]::Round(x, 3)` - banker's rounding (half to even) to three decimals, the plugin's
/// default `MidpointRounding.ToEven`.
fn round_even_3(x: f64) -> f64 {
    let scaled = x * 1000.0;
    let floor = scaled.floor();
    let diff = scaled - floor;
    let rounded = if (diff - 0.5).abs() < 1e-9 {
        // exact half: to even
        if (floor as i64) % 2 == 0 {
            floor
        } else {
            floor + 1.0
        }
    } else {
        scaled.round()
    };
    rounded / 1000.0
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
    by_topic: bool,
    notes: &[String],
    out: &[OutRow],
) {
    let suffix = if !task.is_empty() {
        format!(" (task {task})")
    } else {
        // The plugin prints the literal "task(s)", never a pluralised form.
        format!(" ({task_count} task(s))")
    };
    let rows_by = if by_topic { "; rows by topic" } else { "" };
    println!("{TOOL}: {}{}{}", collab_root.display(), suffix, rows_by);
    for note in notes {
        println!("{note}");
    }
    if out.is_empty() {
        println!("no consultations recorded.");
        return;
    }

    // Column order and header labels (the plugin's $header ordered map).
    let cols: [&str; 19] = [
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
        "score",
        "uniq",
        "median",
        "tokens",
    ];
    let headers: [&str; 19] = [
        "REVIEWER",
        if by_topic { "TOPIC" } else { "PURPOSE" },
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
        "SCORE",
        "UNIQ",
        "MEDIAN_S",
        "TOKENS",
    ];
    let numeric: [&str; 13] = [
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
        "score",
        "median",
    ];

    // Build every line's per-column cells (header first, then data rows).
    let mut lines: Vec<Vec<String>> = Vec::new();
    lines.push(headers.iter().map(|s| s.to_string()).collect());
    for r in out {
        lines.push(row_cells(r));
    }

    // Column widths: max cell length per column across every line.
    let mut width = [0usize; 19];
    for line in &lines {
        for (i, cell) in line.iter().enumerate() {
            width[i] = width[i].max(cell.chars().count());
        }
    }

    for line in &lines {
        let mut parts: Vec<String> = Vec::with_capacity(19);
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
        match r.score {
            Some(s) => format_three_decimal(s),
            None => "-".to_string(),
        },
        format!("{}/{}", r.unique, r.panel_raised),
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

/// `.ToString('0.###')`: at most three fractional digits, trailing zeros dropped (the SCORE
/// column). The value is already `[Math]::Round(_, 3)`, so no further rounding is needed.
fn format_three_decimal(x: f64) -> String {
    let x = format!("{x:.14e}").parse::<f64>().unwrap_or(x);
    let r = (x * 1000.0).round() / 1000.0;
    if r.fract() == 0.0 {
        format!("{}", r as i64)
    } else {
        // Trim to at most three decimals, dropping trailing zeros.
        let s = format!("{r:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
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
        // SCORE: 0.### trims trailing zeros; 1.125 keeps its three decimals.
        assert_eq!(format_three_decimal(1.125), "1.125");
        assert_eq!(format_three_decimal(1.7), "1.7");
        assert_eq!(format_three_decimal(2.0), "2");
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
            "ratings": [{"n": 1, "consult_id": "c1", "useful": "yes", "lineage": "openai :: gpt-5.1", "purpose": "decision"}]
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
        let rows = build_rows(&dir, std::slice::from_ref(&task), false, &mut notes);
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
        // one mark, below the 3-rating floor: the neutral score.
        assert_eq!(p.score, Some(1.125));
        assert_eq!(rows[1].kind, "lineage");
        assert_eq!(rows[1].score, Some(1.125));
        assert_eq!(rows[2].kind, "total");
        assert_eq!(rows[2].lineage, "(all)");
        assert_eq!(rows[2].score, None);
    }

    #[test]
    fn topic_rows_split_and_total_counts_once() {
        let dir = std::env::temp_dir().join(format!("c3sb-topic-{}", std::process::id()));
        let task = dir.join("t");
        std::fs::create_dir_all(&task).unwrap();
        let sessions = json!({
            "task_id": "t",
            "codex": {"consults": [{
                "n": 1,
                "purpose": "framing",
                "topics": ["security", "tests"],
                "consult_id": "c1",
                "reviewer": {"provider": "openai", "model": "gpt-5.1"},
                "bridge_outcome": "usable reply",
                "structured": true
            }]}
        });
        std::fs::write(
            task.join("sessions.json"),
            serde_json::to_vec(&sessions).unwrap(),
        )
        .unwrap();

        let mut notes = Vec::new();
        let rows = build_rows(&dir, std::slice::from_ref(&task), true, &mut notes);
        std::fs::remove_dir_all(&dir).ok();

        // two topic rows + lineage total + grand total.
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].kind, "topic");
        assert_eq!(rows[0].purpose, "security");
        assert_eq!(rows[0].consults, 1);
        assert_eq!(rows[1].purpose, "tests");
        assert_eq!(rows[1].consults, 1);
        // the lineage total counts the consultation ONCE, not once per topic.
        assert_eq!(rows[2].kind, "lineage");
        assert_eq!(rows[2].consults, 1);
        // topic (non-total) rows carry no score.
        assert_eq!(rows[0].score, None);
    }
}
