//! `c3 router replay` (M9 §5): recompute a routed panel's seats from the ledger's own
//! `panel.routing` record and confirm they match `picked`/`explored`.
//!
//! Reads only `sessions.json` — no priors file, no ratings, no network. The draw is fed the
//! record's own seed, size, eligible scores, labs and required flags, so a record the plugin
//! wrote replays exactly like one C3 wrote. The point is auditability: anyone can prove a
//! panel's seats follow from the recorded inputs and the published draw.
//!
//! (0.2.5 R4, F16-6) A routed record's `score` and `budget_mult` are rounded to 4 decimals (the
//! plugin's display form); the draw weighed the unrounded values. A record C3 writes since then
//! carries those at full precision whenever rounding changed them (`ext.c3.draw`), and the replay
//! weighs them - a near tie replays to the winner it had. A routed record without them (written by
//! an older build, or one whose values were exact at 4 decimals) replays from the rounded values as
//! before, and the report says so.

use std::path::PathBuf;

use c3_core::ledger::{PanelRouting, SessionsFile};

use crate::panel::routing::{
    invoke_lab_draw, invoke_panel_draw, roster_lab_seats, Candidate, Seat, ROUTING_EXPLORE,
    ROUTING_NEUTRAL,
};

/// The result of replaying one or more panels.
#[derive(Debug, Clone, Default)]
pub struct ReplayReport {
    /// Distinct panels replayed.
    pub panels: usize,
    /// The first mismatch, if any (its message is printed and the process exits 1).
    pub first_diff: Option<String>,
    /// (R4) The consult numbers of the routed panels replayed from their rounded scores and
    /// multipliers (no `ext.c3.draw`, or - 0.2.5 D1 - one with an invalid row).
    pub rounded: Vec<i64>,
    /// (0.2.5 D1, F30-4) Of those, the panels whose `ext.c3.draw` block was recorded but is not
    /// valid (treated as absent): the consult number and why.
    pub invalid_draw: Vec<(i64, String)>,
}

impl ReplayReport {
    pub fn identical(&self) -> bool {
        self.first_diff.is_none() && self.panels > 0
    }
}

/// Replay every distinct routed panel in `file` (or only consult `nn` when given). Panels
/// are deduplicated by `panel.id`, since every member of one panel carries the same routing
/// record.
pub fn replay_sessions(file: &SessionsFile, nn: Option<i64>) -> ReplayReport {
    let mut report = ReplayReport::default();
    let mut seen: Vec<String> = Vec::new();
    for entry in &file.codex.consults {
        if let Some(n) = nn {
            if entry.n != n {
                continue;
            }
        }
        let panel = match &entry.panel {
            Some(p) => p,
            None => continue,
        };
        let routing = match &panel.routing {
            Some(r) => r,
            None => continue,
        };
        // Deduplicate by panel id (empty id: fall back to the seed, which is per-panel).
        let key = if panel.id.is_empty() {
            format!("seed:{}", routing.seed)
        } else {
            panel.id.clone()
        };
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        report.panels += 1;

        if routing.mode == "routed" {
            match recorded_draw(routing) {
                Draw::Precise(_) => {}
                Draw::Absent => report.rounded.push(entry.n),
                Draw::Invalid(why) => {
                    report.rounded.push(entry.n);
                    report.invalid_draw.push((entry.n, why));
                }
            }
        }
        if let Some(diff) = replay_one(entry.n, routing) {
            report.first_diff = Some(diff);
            return report;
        }
    }
    report
}

/// Replay one routing record on its own (consult number 0 in the message); `Some(msg)` on the
/// first difference.
pub fn replay_record(routing: &PanelRouting) -> Option<String> {
    replay_one(0, routing)
}

/// Replay a single panel's routing record; `Some(msg)` on the first difference.
fn replay_one(n: i64, routing: &PanelRouting) -> Option<String> {
    let explore = recorded_explore(routing);

    // Reconstruct the draw's candidates from the eligible rows, in record order. (0.2.5 D1,
    // F30-4) Precise values only from a fully valid block - never mixed with rounded ones.
    let precise = match recorded_draw(routing) {
        Draw::Precise(rows) => Some(rows),
        Draw::Absent | Draw::Invalid(_) => None,
    };
    let cands: Vec<Candidate> = routing
        .eligible
        .iter()
        .map(|e| Candidate {
            position: e.position,
            lab: e.lab.clone(),
            // (wave 9) the draw weighed `score * budget_mult`; the record carries the multiplier
            // (absent = 1.0) so the replay reproduces the seats without a fresh network read.
            // (R4) The unrounded pair when the record carries it.
            weight: match precise
                .as_ref()
                .and_then(|d| d.iter().find(|(p, _, _)| *p == e.position))
            {
                Some((_, score, mult)) => score * mult,
                None => e.score * recorded_mult(e),
            },
            pinned: e.required,
        })
        .collect();

    // (0.2.5, wave C1) a record that carries `ext.c3.seating` was seated one entry per lab (with
    // the upper-tier seat when its `tier` list is not empty); one without it took the plugin's draw.
    let seating = recorded_seating(routing);
    let seats: Vec<Seat> = if routing.mode == "routed" {
        let seed = match hex_decode(&routing.seed) {
            Some(b) => b,
            None => {
                return Some(format!(
                    "n={n}: the recorded seed '{}' is not hex",
                    routing.seed
                ))
            }
        };
        match &seating {
            Some(tier) => {
                invoke_lab_draw(
                    &cands,
                    routing.size as usize,
                    &seed,
                    ROUTING_NEUTRAL,
                    explore,
                    tier,
                )
                .seats
            }
            None => invoke_panel_draw(
                &cands,
                routing.size as usize,
                &seed,
                ROUTING_NEUTRAL,
                explore,
            ),
        }
    } else {
        match &seating {
            Some(tier) => {
                roster_lab_seats(&cands, routing.size as usize, tier)
                    .0
                    .seats
            }
            // Roster mode: required first (roster order), then the rest, up to size.
            None => roster_seats(&cands, routing.size as usize),
        }
    };

    // Compare seat by seat against the recorded picked rows.
    if seats.len() != routing.picked.len() {
        return Some(format!(
            "n={n}: replayed {} seats, the record has {}",
            seats.len(),
            routing.picked.len()
        ));
    }
    for (i, seat) in seats.iter().enumerate() {
        let rec = &routing.picked[i];
        if seat.slot as i64 != rec.slot || seat.position != rec.position || seat.rule != rec.rule {
            return Some(format!(
                "n={n}: seat {} replayed as slot {} position {} rule {}, the record has slot {} position {} rule {}",
                i + 1,
                seat.slot,
                seat.position,
                seat.rule,
                rec.slot,
                rec.position,
                rec.rule
            ));
        }
    }

    // Compare the explored lineages (a uniform draw), in seat order.
    let replayed_explored: Vec<String> = seats
        .iter()
        .filter(|s| s.rule.ends_with("-explore"))
        .map(|s| lineage_at(routing, s.position))
        .collect();
    let recorded_explored: Vec<String> = routing
        .explored
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    if replayed_explored != recorded_explored {
        return Some(format!(
            "n={n}: replayed explored {replayed_explored:?}, the record has {recorded_explored:?}"
        ));
    }
    None
}

/// Roster-mode seats: required rows first (record order), then the rest, up to `k`.
fn roster_seats(cands: &[Candidate], k: usize) -> Vec<Seat> {
    let mut seats = Vec::new();
    for c in cands.iter().filter(|c| c.pinned) {
        seats.push(Seat {
            slot: seats.len() as i32 + 1,
            position: c.position,
            rule: "required".into(),
        });
    }
    for c in cands.iter().filter(|c| !c.pinned) {
        if seats.len() >= k {
            break;
        }
        seats.push(Seat {
            slot: seats.len() as i32 + 1,
            position: c.position,
            rule: "roster".into(),
        });
    }
    seats
}

/// The recorded budget multiplier of an eligible row (the `budget_mult` key); `1.0` when absent.
fn recorded_mult(e: &c3_core::ledger::RoutingEligible) -> f64 {
    e.extra
        .get("budget_mult")
        .and_then(|v| v.as_f64())
        .filter(|m| m.is_finite() && *m > 0.0)
        .unwrap_or(1.0)
}

/// (R4, F16-6; 0.2.5 D1, F30-4) What a record's `ext.c3.draw` block gives the replay.
#[derive(Debug, Clone, PartialEq)]
enum Draw {
    /// No block: the rounded values for every candidate.
    Absent,
    /// A block with an invalid row (or not a list): treated as absent - the rounded values for
    /// every candidate - and the reason is reported.
    Invalid(String),
    /// Every row valid: the draw's unrounded `(position, score, mult)`.
    Precise(Vec<(i64, f64, f64)>),
}

/// (R4, F16-6) The draw's unrounded `(position, score, mult)` of the record's `ext.c3.draw`, when
/// it carries them. (0.2.5 D1, F30-4) A block with ANY invalid row - no position or score, a
/// multiplier that is not a positive finite number - is [`Draw::Invalid`], never partly used: a
/// replay never weighs precise values for some candidates and rounded ones for others.
fn recorded_draw(routing: &PanelRouting) -> Draw {
    let Some(block) = routing
        .extra
        .get("ext")
        .and_then(|v| v.get("c3"))
        .and_then(|v| v.get("draw"))
    else {
        return Draw::Absent;
    };
    let Some(rows) = block.as_array() else {
        return Draw::Invalid("its ext.c3.draw block is not a list".to_string());
    };
    let mut out = Vec::with_capacity(rows.len());
    for (i, r) in rows.iter().enumerate() {
        let position = r.get("position").and_then(|v| v.as_i64());
        let score = r
            .get("score")
            .and_then(|v| v.as_f64())
            .filter(|s| s.is_finite());
        let mult = r
            .get("mult")
            .and_then(|v| v.as_f64())
            .filter(|m| m.is_finite() && *m > 0.0);
        match (position, score, mult) {
            (Some(p), Some(s), Some(m)) => out.push((p, s, m)),
            _ => {
                let what = if position.is_none() {
                    "no position"
                } else if score.is_none() {
                    "no score"
                } else {
                    "a multiplier that is not a positive finite number"
                };
                return Draw::Invalid(format!("row {} of its ext.c3.draw block has {what}", i + 1));
            }
        }
    }
    Draw::Precise(out)
}

/// The lineage recorded for a seat's position (for the explored comparison).
fn lineage_at(routing: &PanelRouting, position: i64) -> String {
    routing
        .picked
        .iter()
        .find(|p| p.position == position)
        .map(|p| p.lineage.clone())
        .unwrap_or_default()
}

/// (0.2.5, wave C1) The seating rules of the record: `Some(upper-tier positions)` when it carries
/// `ext.c3.seating.labs = "one-per-lab"`, `None` for the plugin's draw.
fn recorded_seating(routing: &PanelRouting) -> Option<Vec<i64>> {
    let seating = routing.extra.get("ext")?.get("c3")?.get("seating")?;
    if seating.get("labs").and_then(|v| v.as_str()) != Some("one-per-lab") {
        return None;
    }
    Some(
        seating
            .get("tier")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|p| p.as_i64()).collect())
            .unwrap_or_default(),
    )
}

/// The `explore` rate the policy used: `ext.c3.router.params.explore` when the record carries
/// C3's `ext`, else the plugin's `ROUTING_EXPLORE`.
fn recorded_explore(routing: &PanelRouting) -> f64 {
    routing
        .extra
        .get("ext")
        .and_then(|v| v.get("c3"))
        .and_then(|v| v.get("router"))
        .and_then(|v| v.get("params"))
        .and_then(|v| v.get("explore"))
        .and_then(|v| v.as_f64())
        .unwrap_or(ROUTING_EXPLORE)
}

/// Decode a lowercase/uppercase hex string into bytes.
fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    let val = |c: u8| -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            b'A'..=b'F' => Some(c - b'A' + 10),
            _ => None,
        }
    };
    let mut i = 0;
    while i < bytes.len() {
        out.push(val(bytes[i])? << 4 | val(bytes[i + 1])?);
        i += 2;
    }
    Some(out)
}

/// `c3 router replay`: read the task's `sessions.json`, replay, and return the exit code.
pub fn run(collab_dir: &str, task: &str, nn: Option<i64>) -> i32 {
    if task.is_empty() {
        eprintln!("c3 router replay: --task is required");
        return 2;
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = crate::providers::resolve_repo_root(&cwd);
    let collab_root = crate::providers::resolve_collab_root(&repo_root, collab_dir);
    let sessions = collab_root.join(task).join("sessions.json");
    let bytes = match std::fs::read(&sessions) {
        Ok(b) => b,
        Err(e) => {
            eprintln!(
                "c3 router replay: cannot read {}: {}",
                sessions.display(),
                c3_core::one_line(&e.to_string())
            );
            return 2;
        }
    };
    let file = match SessionsFile::read(&bytes) {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "c3 router replay: {} did not parse: {}",
                sessions.display(),
                c3_core::one_line(&e.to_string())
            );
            return 2;
        }
    };
    let report = replay_sessions(&file, nn);
    if report.panels == 0 {
        println!("replay: no routed panel found");
        return 2;
    }
    let absent: Vec<String> = report
        .rounded
        .iter()
        .filter(|n| !report.invalid_draw.iter().any(|(m, _)| m == *n))
        .map(|n| format!("n={n}"))
        .collect();
    if !absent.is_empty() {
        println!(
            "replay: note - {} replayed from scores and multipliers rounded to 4 decimals (the record carries no full-precision values: written by a build before 0.2.5, or exact at that precision); a near tie can replay to another winner",
            absent.join(", ")
        );
    }
    // (0.2.5 D1, F30-4) a full-precision block with an invalid row is not used at all
    for (n, why) in &report.invalid_draw {
        println!(
            "replay: note - n={n} replayed from scores and multipliers rounded to 4 decimals for every candidate ({why}, so none of its full-precision values is used); a near tie can replay to another winner"
        );
    }
    match &report.first_diff {
        None => {
            println!("replay: identical");
            0
        }
        Some(diff) => {
            println!("replay: DIFFERS");
            println!("{diff}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest;

    /// A routed two-seat-pool record of size 1 whose rounded scores and multipliers are equal, with
    /// the draw's unrounded values (`ext.c3.draw`) when `precise`; `picked` is the winner the
    /// UNROUNDED weights give for `seed`.
    fn record(seed: &[u8], precise: bool) -> PanelRouting {
        let exact = [(1i64, 0.25004f64, 0.20004f64), (2, 0.24996, 0.19996)];
        let cands: Vec<Candidate> = exact
            .iter()
            .map(|(p, s, m)| Candidate {
                position: *p,
                lab: format!("lab{p}"),
                weight: s * m,
                pinned: false,
            })
            .collect();
        let seats = invoke_panel_draw(&cands, 1, seed, ROUTING_NEUTRAL, ROUTING_EXPLORE);
        let seed_hex: String = seed.iter().map(|b| format!("{b:02x}")).collect();
        let mut v = serde_json::json!({
            "mode": "routed", "order": "routed", "fallback": "", "seed": seed_hex,
            "nonce": "n", "nonce_source": "-PanelSeed", "size": 1, "size_asked": 1,
            "size_source": "-PanelSize", "reserve": 0,
            "eligible": exact.iter().map(|(p, _, _)| serde_json::json!({
                "position": p, "lineage": format!("p{p} :: m{p}"), "lab": format!("lab{p}"),
                "lab_source": "vendor", "score": 0.25, "basis": "rated", "ratings": 3.0,
                "required": false, "budget_mult": 0.2,
            })).collect::<Vec<_>>(),
            "picked": seats.iter().map(|s| serde_json::json!({
                "slot": s.slot, "position": s.position, "lineage": format!("p{0} :: m{0}", s.position),
                "lab": format!("lab{}", s.position), "rule": s.rule, "budget_mult": 0.2,
            })).collect::<Vec<_>>(),
            "explored": [],
            "required": [],
        });
        if precise {
            v["ext"] = serde_json::json!({ "c3": { "draw": exact.iter().map(|(p, s, m)| serde_json::json!({
                "position": p, "score": s, "mult": m,
            })).collect::<Vec<_>>() } });
        }
        serde_json::from_value(v).unwrap()
    }

    // (R4, F16-6) A near tie the rounded values cannot tell apart: the record with the draw's
    // unrounded values replays identically; the same record without them (an older build's) can
    // replay to the other winner, and the report says it was replayed from rounded values.
    #[test]
    fn a_near_tie_replays_from_full_precision() {
        let mut found = None;
        for i in 0u32..200_000 {
            let seed = sha2::Sha256::digest(i.to_be_bytes()).to_vec();
            let precise = record(&seed, true);
            if precise.picked[0].rule.ends_with("-explore") {
                continue;
            }
            if replay_record(&record(&seed, false)).is_some() {
                found = Some(seed);
                break;
            }
        }
        let seed = found.expect("a seed whose rounded replay differs");
        assert_eq!(replay_record(&record(&seed, true)), None);
        assert!(replay_record(&record(&seed, false))
            .unwrap()
            .contains("seat 1 replayed as"));
        // the report names a routed panel replayed from rounded values
        let mk = |precise: bool| -> SessionsFile {
            let routing = serde_json::to_value(record(&seed, precise)).unwrap();
            serde_json::from_value(serde_json::json!({
                "codex": { "consults": [ { "n": 7, "panel": { "id": "p1", "routing": routing } } ] }
            }))
            .unwrap()
        };
        let old = replay_sessions(&mk(false), None);
        assert_eq!(old.rounded, vec![7]);
        assert!(old.first_diff.is_some());
        let new = replay_sessions(&mk(true), None);
        assert!(new.rounded.is_empty());
        assert!(new.identical());
    }

    // (0.2.5 D1, F30-4) A draw block with ANY invalid row - no position, no score, a multiplier
    // that is not a positive finite number, or a block that is not a list - is treated as absent:
    // the replay weighs the rounded values for EVERY candidate (it replays exactly like the record
    // without the block) and the report names the panel with the reason; a valid block is used.
    #[test]
    fn a_replay_never_mixes_precise_and_rounded_values() {
        let seed = sha2::Sha256::digest(7u32.to_be_bytes()).to_vec();
        let without = record(&seed, false);
        let with = record(&seed, true);
        assert!(matches!(recorded_draw(&with), Draw::Precise(ref r) if r.len() == 2));
        assert_eq!(recorded_draw(&without), Draw::Absent);
        let broken = |row: serde_json::Value| -> PanelRouting {
            let mut v = serde_json::to_value(record(&seed, true)).unwrap();
            v["ext"]["c3"]["draw"][1] = row;
            serde_json::from_value(v).unwrap()
        };
        for (row, why) in [
            (serde_json::json!({"score": 0.24996, "mult": 0.19996}), "row 2 of its ext.c3.draw block has no position"),
            (serde_json::json!({"position": 2, "mult": 0.19996}), "row 2 of its ext.c3.draw block has no score"),
            (serde_json::json!({"position": 2, "score": 0.24996, "mult": 0.0}), "row 2 of its ext.c3.draw block has a multiplier that is not a positive finite number"),
            (serde_json::json!({"position": 2, "score": 0.24996, "mult": -1.0}), "row 2 of its ext.c3.draw block has a multiplier that is not a positive finite number"),
        ] {
            let rec = broken(row);
            assert_eq!(recorded_draw(&rec), Draw::Invalid(why.to_string()));
            // the seats are those of the record without any full-precision value
            assert_eq!(
                replay_record(&rec).is_some(),
                replay_record(&without).is_some(),
                "{why}"
            );
        }
        let mut v = serde_json::to_value(record(&seed, true)).unwrap();
        v["ext"]["c3"]["draw"] = serde_json::json!({"position": 1});
        let not_list: PanelRouting = serde_json::from_value(v).unwrap();
        assert_eq!(
            recorded_draw(&not_list),
            Draw::Invalid("its ext.c3.draw block is not a list".to_string())
        );
        // the report names the panel replayed from rounded values, with the reason
        let routing = serde_json::to_value(broken(serde_json::json!({"position": 2}))).unwrap();
        let file: SessionsFile = serde_json::from_value(serde_json::json!({
            "codex": { "consults": [ { "n": 9, "panel": { "id": "p9", "routing": routing } } ] }
        }))
        .unwrap();
        let rep = replay_sessions(&file, None);
        assert_eq!(rep.rounded, vec![9]);
        assert_eq!(
            rep.invalid_draw,
            vec![(9, "row 2 of its ext.c3.draw block has no score".to_string())]
        );
    }
}
