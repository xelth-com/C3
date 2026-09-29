//! `c3 router replay` (M9 §5): recompute a routed panel's seats from the ledger's own
//! `panel.routing` record and confirm they match `picked`/`explored`.
//!
//! Reads only `sessions.json` — no priors file, no ratings, no network. The draw is fed the
//! record's own seed, size, eligible scores, labs and required flags, so a record the plugin
//! wrote replays exactly like one C3 wrote. The point is auditability: anyone can prove a
//! panel's seats follow from the recorded inputs and the published draw.

use std::path::PathBuf;

use c3_core::ledger::{PanelRouting, SessionsFile};

use crate::panel::routing::{invoke_panel_draw, Candidate, Seat, ROUTING_EXPLORE, ROUTING_NEUTRAL};

/// The result of replaying one or more panels.
#[derive(Debug, Clone, Default)]
pub struct ReplayReport {
    /// Distinct panels replayed.
    pub panels: usize,
    /// The first mismatch, if any (its message is printed and the process exits 1).
    pub first_diff: Option<String>,
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

        if let Some(diff) = replay_one(entry.n, routing) {
            report.first_diff = Some(diff);
            return report;
        }
    }
    report
}

/// Replay a single panel's routing record; `Some(msg)` on the first difference.
fn replay_one(n: i64, routing: &PanelRouting) -> Option<String> {
    let explore = recorded_explore(routing);

    // Reconstruct the draw's candidates from the eligible rows, in record order.
    let cands: Vec<Candidate> = routing
        .eligible
        .iter()
        .map(|e| Candidate {
            position: e.position,
            lab: e.lab.clone(),
            weight: e.score,
            pinned: e.required,
        })
        .collect();

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
        invoke_panel_draw(
            &cands,
            routing.size as usize,
            &seed,
            ROUTING_NEUTRAL,
            explore,
        )
    } else {
        // Roster mode: required first (roster order), then the rest, up to size.
        roster_seats(&cands, routing.size as usize)
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

/// The lineage recorded for a seat's position (for the explored comparison).
fn lineage_at(routing: &PanelRouting, position: i64) -> String {
    routing
        .picked
        .iter()
        .find(|p| p.position == position)
        .map(|p| p.lineage.clone())
        .unwrap_or_default()
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
