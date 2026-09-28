//! The panel's seeded routing draw (wave 26, D2-D5): a port of `Get-RoutingScore` (with the
//! per-topic credit the scoreboard's leaner port drops), `Get-PanelSeed`, `ConvertTo-Uniform53`,
//! `Get-SlotUniforms` and `Invoke-PanelDraw` from `codex-consult-common.ps1`.
//!
//! The scoreboard owns its own leaner `routing_score` (purpose only, no topic pooling); this
//! module is the panel's, kept separate because `scoreboard/` is out of this milestone's scope
//! and the panel needs the full `{score, basis, ratings, all}` record plus topic credit.

use chrono::{DateTime, Duration, FixedOffset, Utc};
use sha2::{Digest, Sha256};

pub const ROUTING_WINDOW_DAYS: i64 = 90;
pub const ROUTING_MIN_RATINGS: f64 = 3.0;
pub const ROUTING_PRIOR: f64 = 0.5;
/// `0.25 + 1.75 * 0.5`.
pub const ROUTING_NEUTRAL: f64 = 0.25 + 1.75 * 0.5;
pub const ROUTING_EXPLORE: f64 = 0.2;

/// One normalised mark, ready to score (with the topics the panel's topic pooling needs).
#[derive(Debug, Clone)]
pub struct Rating {
    pub provider: String,
    pub model: String,
    pub engine: String,
    pub purpose: String,
    pub topics: Vec<String>,
    pub consult_when: Option<DateTime<FixedOffset>>,
    pub useful: String,
}

/// `Get-RoutingScore`'s result: the score and the basis behind it.
#[derive(Debug, Clone)]
pub struct RoutingScore {
    pub score: f64,
    /// `purpose+topics` | `purpose` | `all-purpose` | `neutral`.
    pub basis: String,
    /// The (possibly fractional) count behind the basis.
    pub ratings: f64,
    /// The reviewer's rating count of every purpose in the window (the D5 evidence, `>= 3`).
    pub all: i64,
    pub yes: f64,
    pub partly: f64,
    pub no: f64,
}

impl Default for RoutingScore {
    fn default() -> Self {
        RoutingScore {
            score: ROUTING_NEUTRAL,
            basis: "neutral".into(),
            ratings: 0.0,
            all: 0,
            yes: 0.0,
            partly: 0.0,
            no: 0.0,
        }
    }
}

/// `Get-RoutingRate`: `w = (yes + 0.5·partly + 2p) / (n + 4p)` (p = 0.5) scaled into `[0.25, 2]`.
pub fn routing_rate(yes: f64, partly: f64, count: f64) -> f64 {
    let p = ROUTING_PRIOR;
    let w = (yes + 0.5 * partly + 2.0 * p) / (count + 4.0 * p);
    0.25 + 1.75 * w
}

#[derive(Default)]
struct Counts {
    yes: f64,
    partly: f64,
    no: f64,
    n: f64,
}

fn count(list: &[&Rating]) -> Counts {
    let mut c = Counts::default();
    for x in list {
        match x.useful.as_str() {
            "yes" => c.yes += 1.0,
            "partly" => c.partly += 1.0,
            "no" => c.no += 1.0,
            _ => {}
        }
        c.n += 1.0;
    }
    c
}

/// `Get-RoutingScore` (full): hierarchy `(lineage, purpose, topics)` when the pooled topic
/// evidence reaches 3, else `(lineage, purpose)` with `>= 3`, else all-purpose `>= 3`, else
/// neutral. Provider and model compare case-sensitively; the engine case-insensitively.
#[allow(clippy::too_many_arguments)]
pub fn routing_score(
    ratings: &[Rating],
    provider: &str,
    model: &str,
    engine: &str,
    purpose: &str,
    topics: &[String],
    utc_now: DateTime<Utc>,
    all_purpose: bool,
) -> RoutingScore {
    let engine = if engine.is_empty() { "codex" } else { engine };
    let from = utc_now - Duration::days(ROUTING_WINDOW_DAYS);
    let mine: Vec<&Rating> = ratings
        .iter()
        .filter(|r| {
            r.provider == provider
                && r.model == model
                && r.engine.eq_ignore_ascii_case(engine)
                && r.consult_when
                    .map(|w| w.with_timezone(&Utc) >= from)
                    .unwrap_or(false)
        })
        .collect();

    let all = count(&mine);
    let mut res = RoutingScore {
        all: all.n as i64,
        ..Default::default()
    };

    let on_purpose: Vec<&Rating> = if all_purpose {
        Vec::new()
    } else {
        mine.iter()
            .filter(|r| r.purpose == purpose)
            .copied()
            .collect()
    };

    let want: Vec<&String> = topics.iter().filter(|t| !t.is_empty()).collect();
    if !want.is_empty() {
        let mut c = Counts::default();
        for x in &on_purpose {
            let t = x.topics.len();
            if t == 0 {
                continue;
            }
            for topic in &want {
                if x.topics.iter().any(|xt| xt == *topic) {
                    let share = 1.0 / t as f64;
                    match x.useful.as_str() {
                        "yes" => c.yes += share,
                        "partly" => c.partly += share,
                        "no" => c.no += share,
                        _ => {}
                    }
                    c.n += share;
                }
            }
        }
        if c.n >= ROUTING_MIN_RATINGS - 1e-9 {
            res.score = routing_rate(c.yes, c.partly, c.n);
            res.basis = "purpose+topics".into();
            res.ratings = c.n;
            res.yes = c.yes;
            res.partly = c.partly;
            res.no = c.no;
            return res;
        }
    }

    let p = count(&on_purpose);
    if p.n >= ROUTING_MIN_RATINGS {
        res.score = routing_rate(p.yes, p.partly, p.n);
        res.basis = "purpose".into();
        res.ratings = p.n;
        res.yes = p.yes;
        res.partly = p.partly;
        res.no = p.no;
        return res;
    }
    if all.n >= ROUTING_MIN_RATINGS {
        res.score = routing_rate(all.yes, all.partly, all.n);
        res.basis = "all-purpose".into();
        res.ratings = all.n;
        res.yes = all.yes;
        res.partly = all.partly;
        res.no = all.no;
    }
    res
}

/// `ConvertTo-LengthPrefixed`: `<utf8-byte-count>:<value>`.
pub fn length_prefixed(value: &str) -> String {
    format!("{}:{}", value.len(), value)
}

/// `Get-PanelSeed`: SHA-256 over `<task>|<purpose>|<brief sha>|<lineages>|<nonce>`, every field
/// length-prefixed and `<lineages>` the eligible lineages sorted ordinally, each length-prefixed,
/// joined by `,` (itself then length-prefixed). Returns `(seed_bytes, hex, text)`.
pub fn panel_seed(
    task: &str,
    purpose: &str,
    brief_sha: &str,
    lineages: &[String],
    nonce: &str,
) -> (Vec<u8>, String, String) {
    let mut sorted: Vec<String> = lineages.to_vec();
    sorted.sort(); // ordinal (byte) order — Rust String Ord is byte-lexicographic
    let joined = sorted
        .iter()
        .map(|s| length_prefixed(s))
        .collect::<Vec<_>>()
        .join(",");
    let text = [
        length_prefixed(task),
        length_prefixed(purpose),
        length_prefixed(brief_sha),
        length_prefixed(&joined),
        length_prefixed(nonce),
    ]
    .join("|");
    let bytes = Sha256::digest(text.as_bytes()).to_vec();
    let hex = hex_lower(&bytes);
    (bytes, hex, text)
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// `ConvertTo-Uniform53`: the top 53 bits (big-endian) of 8 bytes at `offset`, / 2^53.
pub fn uniform53(hash: &[u8], offset: usize) -> f64 {
    let mut v: i64 = 0;
    for i in 0..6 {
        v = v * 256 + hash[offset + i] as i64;
    }
    v = v * 32 + ((hash[offset + 6] as i64) >> 3);
    v as f64 / 9007199254740992.0
}

/// The two uniforms of draw slot `slot` (1-based): SHA-256(seed || slot as 4 BE bytes); bytes
/// 0..7 pick, bytes 8..15 decide exploration.
pub fn slot_uniforms(seed: &[u8], slot: i32) -> (f64, f64) {
    let mut buf = Vec::with_capacity(seed.len() + 4);
    buf.extend_from_slice(seed);
    buf.push(((slot >> 24) & 255) as u8);
    buf.push(((slot >> 16) & 255) as u8);
    buf.push(((slot >> 8) & 255) as u8);
    buf.push((slot & 255) as u8);
    let h = Sha256::digest(&buf);
    (uniform53(&h, 0), uniform53(&h, 8))
}

/// A draw candidate (roster order).
#[derive(Debug, Clone)]
pub struct Candidate {
    pub position: i64,
    pub lab: String,
    pub weight: f64,
    pub pinned: bool,
}

/// A seated slot.
#[derive(Debug, Clone)]
pub struct Seat {
    pub slot: i32,
    pub position: i64,
    pub rule: String,
}

/// `Invoke-PanelDraw`: the seats of a routed panel. Pinned candidates take the first seats
/// (roster order); then the lab reserve draws from un-seated labs weighing `>= neutral`; else
/// the rank draw over every remaining candidate. Exploration (uniform pick) when the second
/// uniform is `< explore`.
pub fn invoke_panel_draw(
    candidates: &[Candidate],
    k: usize,
    seed: &[u8],
    neutral: f64,
    explore: f64,
) -> Vec<Seat> {
    let mut seats: Vec<Seat> = Vec::new();
    let mut left: Vec<Candidate> = Vec::new();
    for c in candidates {
        if c.pinned {
            seats.push(Seat {
                slot: seats.len() as i32 + 1,
                position: c.position,
                rule: "required".into(),
            });
        } else {
            left.push(c.clone());
        }
    }
    // Distinct labs weighing >= neutral (over ALL candidates), and the labs the pins seated.
    let good_labs: Vec<String> = {
        let mut seen: Vec<String> = Vec::new();
        for c in candidates {
            if c.weight >= neutral && !seen.contains(&c.lab) {
                seen.push(c.lab.clone());
            }
        }
        seen
    };
    let pinned_labs: Vec<String> = {
        let mut seen: Vec<String> = Vec::new();
        for c in candidates.iter().filter(|c| c.pinned) {
            if !seen.contains(&c.lab) {
                seen.push(c.lab.clone());
            }
        }
        seen
    };
    let reserve = std::cmp::min(
        k.saturating_sub(seats.len()),
        good_labs
            .iter()
            .filter(|l| !pinned_labs.contains(l))
            .count(),
    );
    let mut reserve_taken = 0usize;

    while seats.len() < k && !left.is_empty() {
        let slot = seats.len() as i32 + 1;
        let seated_labs: Vec<String> = {
            let mut seen: Vec<String> = Vec::new();
            for s in &seats {
                let lab = candidates
                    .iter()
                    .find(|c| c.position == s.position)
                    .map(|c| c.lab.clone())
                    .unwrap_or_default();
                if !seen.contains(&lab) {
                    seen.push(lab);
                }
            }
            seen
        };
        let mut kind = "rank";
        // Indices into `left`.
        let mut pool: Vec<usize> = Vec::new();
        if reserve_taken < reserve {
            pool = left
                .iter()
                .enumerate()
                .filter(|(_, c)| !seated_labs.contains(&c.lab) && c.weight >= neutral)
                .map(|(i, _)| i)
                .collect();
            if !pool.is_empty() {
                kind = "lab";
            }
        }
        if pool.is_empty() {
            pool = (0..left.len()).collect();
            kind = "rank";
        }
        let (pick, expl) = slot_uniforms(seed, slot);
        let winner_idx: usize; // index into `left`
        let rule: String;
        if expl < explore {
            let mut idx = (pick * pool.len() as f64).floor() as usize;
            if idx >= pool.len() {
                idx = pool.len() - 1;
            }
            winner_idx = pool[idx];
            rule = format!("{kind}-explore");
        } else {
            let total: f64 = pool.iter().map(|&i| left[i].weight).sum();
            let target = pick * total;
            let mut acc = 0.0;
            let mut chosen: Option<usize> = None;
            for &i in &pool {
                acc += left[i].weight;
                if target < acc {
                    chosen = Some(i);
                    break;
                }
            }
            winner_idx = chosen.unwrap_or(*pool.last().unwrap());
            rule = format!("{kind}-draw");
        }
        if kind == "lab" {
            reserve_taken += 1;
        }
        let winner = left.remove(winner_idx);
        seats.push(Seat {
            slot,
            position: winner.position,
            rule,
        });
    }
    seats
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_prefixed_counts_utf8_bytes() {
        assert_eq!(length_prefixed("abc"), "3:abc");
        // 'é' is 2 UTF-8 bytes.
        assert_eq!(length_prefixed("é"), "2:é");
        assert_eq!(length_prefixed(""), "0:");
    }

    #[test]
    fn seed_is_deterministic_and_order_independent_in_lineages() {
        let a = panel_seed(
            "task-x",
            "framing",
            "deadbeef",
            &["zai :: glm-5.3".into(), "openai :: gpt-6".into()],
            "2026-09-28",
        );
        let b = panel_seed(
            "task-x",
            "framing",
            "deadbeef",
            &["openai :: gpt-6".into(), "zai :: glm-5.3".into()],
            "2026-09-28",
        );
        // The lineages are sorted before hashing, so input order does not matter.
        assert_eq!(a.1, b.1);
        assert_eq!(a.0.len(), 32);
        // A different nonce changes the seed.
        let c = panel_seed(
            "task-x",
            "framing",
            "deadbeef",
            &["zai :: glm-5.3".into(), "openai :: gpt-6".into()],
            "2026-09-29",
        );
        assert_ne!(a.1, c.1);
    }

    #[test]
    fn uniform53_bounds() {
        let all_zero = [0u8; 16];
        assert_eq!(uniform53(&all_zero, 0), 0.0);
        let all_ff = [0xffu8; 16];
        let u = uniform53(&all_ff, 0);
        assert!(u < 1.0 && u > 0.999);
    }

    #[test]
    fn draw_is_deterministic_for_a_fixed_seed() {
        let cands = vec![
            Candidate {
                position: 1,
                lab: "openai".into(),
                weight: 1.5,
                pinned: false,
            },
            Candidate {
                position: 2,
                lab: "zhipu".into(),
                weight: 1.0,
                pinned: false,
            },
            Candidate {
                position: 3,
                lab: "google".into(),
                weight: 1.2,
                pinned: false,
            },
        ];
        let (seed, _, _) = panel_seed("t", "framing", "sha", &["a".into()], "n");
        let s1 = invoke_panel_draw(&cands, 2, &seed, ROUTING_NEUTRAL, ROUTING_EXPLORE);
        let s2 = invoke_panel_draw(&cands, 2, &seed, ROUTING_NEUTRAL, ROUTING_EXPLORE);
        assert_eq!(s1.len(), 2);
        let p1: Vec<i64> = s1.iter().map(|s| s.position).collect();
        let p2: Vec<i64> = s2.iter().map(|s| s.position).collect();
        assert_eq!(p1, p2, "the draw is deterministic for a fixed seed");
        // Slots are numbered 1..k in order.
        assert_eq!(s1[0].slot, 1);
        assert_eq!(s1[1].slot, 2);
    }

    #[test]
    fn required_take_the_first_seats_in_roster_order() {
        let cands = vec![
            Candidate {
                position: 1,
                lab: "openai".into(),
                weight: 0.5,
                pinned: false,
            },
            Candidate {
                position: 2,
                lab: "zhipu".into(),
                weight: 2.0,
                pinned: true,
            },
            Candidate {
                position: 3,
                lab: "google".into(),
                weight: 1.2,
                pinned: true,
            },
        ];
        let (seed, _, _) = panel_seed("t", "framing", "sha", &["a".into()], "n");
        let seats = invoke_panel_draw(&cands, 3, &seed, ROUTING_NEUTRAL, ROUTING_EXPLORE);
        assert_eq!(seats[0].rule, "required");
        assert_eq!(seats[0].position, 2);
        assert_eq!(seats[1].rule, "required");
        assert_eq!(seats[1].position, 3);
    }

    #[test]
    fn lab_diversity_seats_distinct_labs_first() {
        // Two openai candidates weigh most; a lab reserve still seats zhipu (a distinct lab).
        let cands = vec![
            Candidate {
                position: 1,
                lab: "openai".into(),
                weight: 2.0,
                pinned: false,
            },
            Candidate {
                position: 2,
                lab: "openai".into(),
                weight: 1.9,
                pinned: false,
            },
            Candidate {
                position: 3,
                lab: "zhipu".into(),
                weight: 1.2,
                pinned: false,
            },
        ];
        let (seed, _, _) = panel_seed("t", "framing", "sha", &["a".into()], "n");
        let seats = invoke_panel_draw(&cands, 2, &seed, ROUTING_NEUTRAL, ROUTING_EXPLORE);
        let labs: std::collections::HashSet<String> = seats
            .iter()
            .map(|s| {
                cands
                    .iter()
                    .find(|c| c.position == s.position)
                    .unwrap()
                    .lab
                    .clone()
            })
            .collect();
        assert_eq!(
            labs.len(),
            2,
            "the two seats are distinct labs (lab reserve)"
        );
    }

    #[test]
    fn routing_score_neutral_without_evidence() {
        let s = routing_score(
            &[],
            "openai",
            "gpt-6",
            "codex",
            "framing",
            &[],
            Utc::now(),
            false,
        );
        assert_eq!(s.basis, "neutral");
        assert!((s.score - ROUTING_NEUTRAL).abs() < 1e-9);
        assert_eq!(s.all, 0);
    }

    #[test]
    fn routing_score_purpose_basis_at_three() {
        let now = Utc::now();
        let cw = Some(now.fixed_offset());
        let mk = |u: &str| Rating {
            provider: "openai".into(),
            model: "gpt-6".into(),
            engine: "codex".into(),
            purpose: "framing".into(),
            topics: vec![],
            consult_when: cw,
            useful: u.into(),
        };
        let ratings = vec![mk("yes"), mk("yes"), mk("partly")];
        let s = routing_score(
            &ratings,
            "openai",
            "gpt-6",
            "codex",
            "framing",
            &[],
            now,
            false,
        );
        assert_eq!(s.basis, "purpose");
        assert_eq!(s.all, 3);
        // w = (2 + 0.5 + 1) / (3 + 2) = 3.5/5 = 0.7; score = 0.25 + 1.75*0.7 = 1.475.
        assert!((s.score - 1.475).abs() < 1e-9);
    }
}
