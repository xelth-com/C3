//! The routing score behind the scoreboard's `SCORE` column (wave 26, D2-D5): a port of
//! `Read-AllTaskRatings` and `Get-RoutingScore` from `codex-consult-common.ps1`.
//!
//! The score reads the judge's usefulness marks (`codex-findings.ps1 -Rate`) of EVERY task
//! of the repository from the last 90 days, keyed by the consultation's `consult_id` (a mark
//! recorded before wave 26 without one is keyed by its `n` within its task); the latest mark
//! of a consultation wins. A mark that lacks the wave-26 fields (`engine`, `topics`,
//! `consult_when`, or a `provider`) is completed by joining to its consultation entry by
//! `consult_id`. The rate is `w = (yes + 0.5·partly + 1) / (n + 2)` scaled into `[0.25, 2]`
//! (neutral `1.125`), for the reviewer's `(lineage, purpose)` once it has at least three
//! marks, else its all-purpose rate with at least three, else neutral.

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Duration, FixedOffset, Utc};
use serde_json::Value;

const ROUTING_WINDOW_DAYS: i64 = 90;
const ROUTING_MIN_RATINGS: f64 = 3.0;
const ROUTING_PRIOR: f64 = 0.5;
/// `0.25 + 1.75 * 0.5`.
const ROUTING_NEUTRAL: f64 = 0.25 + 1.75 * 0.5;

/// One normalised mark, ready to score. (The scoreboard's `SCORE` scores by purpose, so the
/// per-topic credit the plugin's `Get-RoutingScore` also computes is not carried here; the
/// panel milestone that needs it lives elsewhere.)
#[derive(Debug, Clone)]
pub struct AllRating {
    pub provider: String,
    pub model: String,
    pub engine: String,
    pub purpose: String,
    pub consult_when: Option<DateTime<FixedOffset>>,
    pub rated_when: Option<DateTime<FixedOffset>>,
    pub useful: String,
}

fn pv<'a>(v: Option<&'a Value>, key: &str) -> Option<&'a Value> {
    v.and_then(|o| o.get(key)).filter(|x| !x.is_null())
}

fn pv_str(v: Option<&Value>, key: &str) -> String {
    match pv(v, key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => (if *b { "True" } else { "False" }).to_string(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

fn has_key(v: Option<&Value>, key: &str) -> bool {
    v.and_then(|o| o.as_object())
        .map(|o| o.contains_key(key))
        .unwrap_or(false)
}

/// `ConvertTo-WhenOffset`: a lenient ISO-8601-with-offset parse; `None` when nothing parses.
fn parse_when(v: Option<&Value>) -> Option<DateTime<FixedOffset>> {
    let s = match v {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(_)) | Some(Value::Bool(_)) => return None,
        _ => return None,
    };
    if s.is_empty() {
        return None;
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(&s) {
        return Some(dt);
    }
    // A space instead of the 'T' separator, or a trailing 'Z' variant.
    let alt = s.replacen(' ', "T", 1);
    if let Ok(dt) = DateTime::parse_from_rfc3339(&alt) {
        return Some(dt);
    }
    None
}

/// Every consultation entry of every task under `collab_root` (`Read-AllTaskConsults`).
fn read_all_task_consults(collab_root: &Path) -> Vec<Value> {
    let mut out = Vec::new();
    let mut dirs: Vec<_> = match std::fs::read_dir(collab_root) {
        Ok(rd) => rd
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.path())
            .collect(),
        Err(_) => return out,
    };
    dirs.sort();
    for d in dirs {
        let f = d.join("sessions.json");
        if let Ok(bytes) = std::fs::read(&f) {
            if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
                if let Some(arr) = pv(pv(Some(&v), "codex"), "consults").and_then(|c| c.as_array())
                {
                    for c in arr {
                        if !c.is_null() {
                            out.push(c.clone());
                        }
                    }
                }
            }
        }
    }
    out
}

/// `Read-AllTaskRatings`: normalise every task's marks into scorable records. `stores` maps a
/// task directory name to its already-parsed `findings.json` (`None` = unreadable/absent), so
/// each store is read once per run. The consultation join (for pre-wave-26 marks) reads the
/// sessions lazily.
pub fn read_all_task_ratings(
    collab_root: &Path,
    stores: &HashMap<String, Option<Value>>,
) -> Vec<AllRating> {
    let mut out: Vec<AllRating> = Vec::new();
    let mut by_key: HashMap<String, usize> = HashMap::new();
    let mut by_id: Option<HashMap<String, Value>> = None;
    let eng_re = regex::Regex::new(r"\s\[([a-z]+)\]$").unwrap();

    let mut dirs: Vec<_> = match std::fs::read_dir(collab_root) {
        Ok(rd) => rd
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.path())
            .collect(),
        Err(_) => return out,
    };
    dirs.sort();

    for d in &dirs {
        let name = d
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let store = match stores.get(&name) {
            Some(Some(v)) => v.clone(),
            _ => continue,
        };
        let ratings = match pv(Some(&store), "ratings").and_then(|v| v.as_array()) {
            Some(a) => a.clone(),
            None => continue,
        };
        for rt in &ratings {
            if rt.is_null() {
                continue;
            }
            let rt = Some(rt);
            let useful = pv_str(rt, "useful");
            if !["yes", "partly", "no"].contains(&useful.as_str()) {
                continue;
            }
            let cid = pv_str(rt, "consult_id");
            let need_join = !cid.is_empty()
                && (!has_key(rt, "consult_when")
                    || !has_key(rt, "engine")
                    || !has_key(rt, "topics")
                    || pv_str(rt, "provider").is_empty());
            let mut entry: Option<Value> = None;
            if need_join {
                if by_id.is_none() {
                    let mut map = HashMap::new();
                    for c in read_all_task_consults(collab_root) {
                        let ci = pv_str(Some(&c), "consult_id");
                        if !ci.is_empty() {
                            map.entry(ci.to_lowercase()).or_insert(c);
                        }
                    }
                    by_id = Some(map);
                }
                entry = by_id
                    .as_ref()
                    .and_then(|m| m.get(&cid.to_lowercase()))
                    .cloned();
            }
            let rev = entry
                .as_ref()
                .and_then(|e| pv(Some(e), "reviewer").cloned());
            let rev_ref = rev.as_ref();

            let mut provider = pv_str(rt, "provider");
            if provider.is_empty() {
                provider = pv_str(rev_ref, "provider");
            }
            if provider.is_empty() {
                continue;
            }
            let mut model = pv_str(rt, "model");
            if model.is_empty() {
                model = pv_str(rev_ref, "model");
            }
            let mut engine = pv_str(rt, "engine");
            if engine.is_empty() {
                engine = pv_str(rev_ref, "engine");
            }
            if engine.is_empty() {
                // The lineage's ` [<engine>]` suffix, if any.
                let lineage = pv_str(rt, "lineage");
                if let Some(caps) = eng_re.captures(&lineage) {
                    engine = caps[1].to_string();
                }
            }
            if engine.is_empty() {
                engine = "codex".to_string();
            }
            let mut purpose = pv_str(rt, "purpose");
            if !has_key(rt, "purpose") {
                if let Some(e) = entry.as_ref() {
                    purpose = pv_str(Some(e), "purpose");
                }
            }
            let mut consult_when = parse_when(pv(rt, "consult_when"));
            if consult_when.is_none() {
                if let Some(e) = entry.as_ref() {
                    consult_when = parse_when(pv(Some(e), "when"));
                }
            }
            let rated_when = parse_when(pv(rt, "when"));
            if consult_when.is_none() {
                consult_when = rated_when;
            }
            let n = pv_str(rt, "n");
            let key = if !cid.is_empty() {
                cid.to_lowercase()
            } else {
                format!("{name}#n{n}")
            };
            let rec = AllRating {
                provider,
                model,
                engine,
                purpose,
                consult_when,
                rated_when,
                useful,
            };
            if let Some(&idx) = by_key.get(&key) {
                // The latest mark of a consultation wins (newer RatedWhen).
                let newer = match (rec.rated_when, out[idx].rated_when) {
                    (Some(new_w), Some(old_w)) => new_w >= old_w,
                    (Some(_), None) => true,
                    _ => false,
                };
                if newer {
                    out[idx] = rec;
                }
                continue;
            }
            by_key.insert(key, out.len());
            out.push(rec);
        }
    }
    out
}

/// `Get-RoutingRate`: `w = (yes + 0.5·partly + 2p) / (n + 4p)` (p = 0.5) scaled into
/// `[0.25, 2]`.
fn routing_rate(yes: f64, partly: f64, count: f64) -> f64 {
    let p = ROUTING_PRIOR;
    let w = (yes + 0.5 * partly + 2.0 * p) / (count + 4.0 * p);
    0.25 + 1.75 * w
}

struct Counts {
    yes: f64,
    partly: f64,
    n: f64,
}

fn count(list: &[&AllRating]) -> Counts {
    let mut c = Counts {
        yes: 0.0,
        partly: 0.0,
        n: 0.0,
    };
    for x in list {
        match x.useful.as_str() {
            "yes" => c.yes += 1.0,
            "partly" => c.partly += 1.0,
            _ => {}
        }
        c.n += 1.0;
    }
    c
}

/// `Get-RoutingScore` (the `Score` only; the scoreboard needs no basis). Provider and model
/// are compared case-sensitively, the engine case-insensitively (as the plugin's `-ceq` /
/// `-eq`). `all_purpose` scores every purpose in the window (the lineage total).
pub fn routing_score(
    ratings: &[AllRating],
    provider: &str,
    model: &str,
    engine: &str,
    purpose: &str,
    utc_now: DateTime<Utc>,
    all_purpose: bool,
) -> f64 {
    let engine = if engine.is_empty() { "codex" } else { engine };
    let from = utc_now - Duration::days(ROUTING_WINDOW_DAYS);
    let mine: Vec<&AllRating> = ratings
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
    let on_purpose: Vec<&AllRating> = if all_purpose {
        Vec::new()
    } else {
        mine.iter()
            .filter(|r| r.purpose == purpose)
            .copied()
            .collect()
    };

    let p = count(&on_purpose);
    if p.n >= ROUTING_MIN_RATINGS {
        return routing_rate(p.yes, p.partly, p.n);
    }
    if all.n >= ROUTING_MIN_RATINGS {
        return routing_rate(all.yes, all.partly, all.n);
    }
    ROUTING_NEUTRAL
}
