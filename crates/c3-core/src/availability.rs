//! The one-truth availability view (wave 24, D14-D17): endpoint groups, one record
//! per roster entry from its preflight verdict, and the one-line summary. Ported from
//! `Get-EndpointGroups`, `ConvertTo-AvailabilityRecord`, `Format-RelativeHint`,
//! `Format-LocalWhen`, `Get-FirstClause`, `Format-AvailabilityLine`,
//! `Format-RosterSkips`. All pure.

use std::collections::HashMap;

use chrono::{DateTime, Duration, FixedOffset, Local, Utc};

use crate::lineage::{format_lineage, format_reviewer_lineage};
use crate::verdict::PreflightVerdict;

/// One availability record.
#[derive(Debug, Clone)]
pub struct AvailabilityRecord {
    pub position: i64,
    pub provider: String,
    pub model: String,
    pub engine: String,
    pub lineage: String,
    pub group: usize,
    /// `available` | `out` | `not checked`.
    pub state: String,
    pub kind: String,
    pub reason: String,
    pub short: String,
    pub hit: Option<DateTime<FixedOffset>>,
    pub until: Option<DateTime<FixedOffset>>,
}

/// `Format-RelativeHint`.
pub fn format_relative_hint(span: Duration) -> String {
    let total_seconds = span.num_milliseconds() as f64 / 1000.0;
    if total_seconds <= 0.0 {
        return "now".into();
    }
    let total_minutes = span.num_milliseconds() as f64 / 60000.0;
    let m = total_minutes.round() as i64;
    if m < 1 {
        return "in <1m".into();
    }
    if m >= 1440 {
        let total_hours = span.num_milliseconds() as f64 / 3_600_000.0;
        let h = total_hours.round() as i64;
        let d = h / 24;
        let hh = h - 24 * d;
        return if hh == 0 {
            format!("in {d}d")
        } else {
            format!("in {d}d {hh}h")
        };
    }
    if m >= 60 {
        let h = m / 60;
        let mm = m - 60 * h;
        return if mm == 0 {
            format!("in {h}h")
        } else {
            format!("in {h}h {mm}m")
        };
    }
    format!("in {m}m")
}

/// `Format-LocalWhen`: a moment in local time for the one-line views.
pub fn format_local_when(when: DateTime<FixedOffset>, now_utc: DateTime<Utc>) -> String {
    let local = when.with_timezone(&Local);
    let now_local = now_utc.with_timezone(&Local);
    let days = (local.date_naive() - now_local.date_naive())
        .num_days()
        .abs();
    if days < 1 {
        local.format("%H:%M").to_string()
    } else if days <= 6 {
        local.format("%a %H:%M").to_string()
    } else {
        local.format("%Y-%m-%d %H:%M").to_string()
    }
}

/// `Get-FirstClause`: up to the first `": "` outside parentheses.
pub fn get_first_clause(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut depth = 0i32;
    if chars.is_empty() {
        return String::new();
    }
    for i in 0..chars.len() - 1 {
        let c = chars[i];
        if c == '(' {
            depth += 1;
        } else if c == ')' && depth > 0 {
            depth -= 1;
        } else if c == ':' && depth == 0 && chars[i + 1] == ' ' {
            return chars[..i].iter().collect();
        }
    }
    text.to_string()
}

/// `ConvertTo-AvailabilityRecord`.
#[allow(clippy::too_many_arguments)]
pub fn convert_to_availability_record(
    position: i64,
    provider: &str,
    model: &str,
    engine: &str,
    group: usize,
    verdict: Option<&PreflightVerdict>,
    block: &str,
    now_utc: DateTime<Utc>,
) -> AvailabilityRecord {
    let engine = if engine.is_empty() { "codex" } else { engine };
    let lineage = if !model.is_empty() {
        format_lineage(provider, model)
    } else {
        provider.to_string()
    };
    let mut rec = AvailabilityRecord {
        position,
        provider: provider.to_string(),
        model: model.to_string(),
        engine: engine.to_string(),
        lineage,
        group,
        state: "available".into(),
        kind: String::new(),
        reason: String::new(),
        short: String::new(),
        hit: None,
        until: None,
    };
    if !block.is_empty() {
        rec.state = "out".into();
        rec.kind = "refused".into();
        rec.reason = format!("refused: {block}");
        rec.short = format!("refused: {}", get_first_clause(block));
        return rec;
    }
    let v = match verdict {
        Some(v) if v.state != "available" => v,
        _ => return rec,
    };
    rec.state = if v.state == "unavailable" {
        "out".into()
    } else {
        "not checked".into()
    };
    rec.kind = v.kind.clone();
    rec.reason = v.reason.clone();
    rec.hit = v.hit;
    rec.until = v.until;
    // (wave 29b, E5) out because its plan is out on another route: "plan zai (usage limit on ZAI
    // until 15:00, in 3h)"; (wave 24c) a burst names itself
    let pqv = v.plan_quota.as_ref();
    if rec.kind == "quota" {
        if let Some(until) = rec.until {
            let when = format!(
                "until {}, {}",
                format_local_when(until, now_utc),
                format_relative_hint(until.with_timezone(&Utc) - now_utc)
            );
            rec.short = match pqv {
                Some(p) => format!("plan {} (usage limit on {} {when})", p.plan, p.label),
                None => when,
            };
        }
    } else if rec.kind == "quota-unknown-reset" {
        if let Some(until) = rec.until {
            let hit = rec.hit.unwrap_or(until);
            let window = format!(
                "reset unknown; retry after {}, {}",
                format_local_when(until, now_utc),
                format_relative_hint(until.with_timezone(&Utc) - now_utc)
            );
            rec.short = match pqv {
                Some(p) => format!(
                    "plan {} ({} hit on {} {}, {window})",
                    p.plan,
                    if v.burst { "burst limit" } else { "limit" },
                    p.label,
                    format_local_when(hit, now_utc)
                ),
                None => format!(
                    "{} {}, {window}",
                    if v.burst {
                        "burst limit hit"
                    } else {
                        "limit hit"
                    },
                    format_local_when(hit, now_utc)
                ),
            };
        }
    } else if rec.kind == "auth" {
        rec.short = format!(
            "auth failed {}",
            rec.hit
                .map(|h| format_local_when(h, now_utc))
                .unwrap_or_else(|| "recently".into())
        );
    } else if rec.kind == "unresolved" {
        rec.short = "identity unresolved".into();
    } else if let Some(cred) = v
        .credential
        .as_ref()
        .filter(|c| !c.reason.is_empty() && (rec.kind == "credentials" || rec.kind == "unknown"))
    {
        rec.short = cred.reason.clone();
    } else {
        rec.short = v.reason.clone();
    }
    rec
}

/// The endpoint groups of a set of members (`Get-EndpointGroups`).
#[derive(Debug, Clone)]
pub struct EndpointGroups {
    pub group_of: HashMap<i64, usize>,
    pub groups: Vec<GroupInfo>,
}

#[derive(Debug, Clone)]
pub struct GroupInfo {
    pub labels: Vec<String>,
    pub positions: Vec<i64>,
}

/// members: (position, provider label, resolved, fingerprint).
pub fn endpoint_groups(members: &[(i64, String, bool, String)]) -> EndpointGroups {
    let mut labels: Vec<String> = Vec::new();
    let mut fps: HashMap<String, Vec<String>> = HashMap::new();
    for (_, label, resolved, fp) in members {
        if !fps.contains_key(label) {
            fps.insert(label.clone(), Vec::new());
            labels.push(label.clone());
        }
        if *resolved && !fp.is_empty() {
            let v = fps.get_mut(label).unwrap();
            if !v.contains(fp) {
                v.push(fp.clone());
            }
        }
    }
    let mut group_of_label: HashMap<String, usize> = HashMap::new();
    for (i, l) in labels.iter().enumerate() {
        group_of_label.insert(l.clone(), i);
    }
    let mut changed = true;
    while changed {
        changed = false;
        for i in 0..labels.len() {
            for j in (i + 1)..labels.len() {
                let gi = group_of_label[&labels[i]];
                let gj = group_of_label[&labels[j]];
                if gi == gj {
                    continue;
                }
                let fi = &fps[&labels[i]];
                let fj = &fps[&labels[j]];
                let shared = fi.iter().any(|f| fj.contains(f));
                if !shared {
                    continue;
                }
                let keep = gi.min(gj);
                let drop = gi.max(gj);
                let keys: Vec<String> = labels.clone();
                for l in keys {
                    if group_of_label[&l] == drop {
                        group_of_label.insert(l, keep);
                    }
                }
                changed = true;
            }
        }
    }
    let mut groups: Vec<GroupInfo> = Vec::new();
    let mut index_of: HashMap<usize, usize> = HashMap::new();
    let mut group_of: HashMap<i64, usize> = HashMap::new();
    for (pos, label, _, _) in members {
        let gid = group_of_label[label];
        let idx = *index_of.entry(gid).or_insert_with(|| {
            let n = groups.len();
            groups.push(GroupInfo {
                labels: Vec::new(),
                positions: Vec::new(),
            });
            n
        });
        if !groups[idx].labels.contains(label) {
            groups[idx].labels.push(label.clone());
        }
        groups[idx].positions.push(*pos);
        group_of.insert(*pos, idx);
    }
    EndpointGroups { group_of, groups }
}

/// `Format-AvailabilityLine`.
pub fn format_availability_line(
    records: &[AvailabilityRecord],
    noun: &str,
    prefix: &str,
    suffix: &str,
) -> String {
    let n = records.len();
    let noun_word = if n == 1 {
        noun.trim_end_matches('s')
    } else {
        noun
    };
    if n == 0 {
        return format!("{prefix}no {noun} to check{suffix}");
    }
    let a = records.iter().filter(|r| r.state == "available").count();
    let o = records.iter().filter(|r| r.state == "out").count();
    let c = records.iter().filter(|r| r.state == "not checked").count();
    if o == 0 && c == 0 {
        return format!("{prefix}all {n} {noun_word} available{suffix}");
    }
    let mut parts: Vec<String> = Vec::new();
    for state in ["out", "not checked"] {
        let these: Vec<&AvailabilityRecord> = records.iter().filter(|r| r.state == state).collect();
        if these.is_empty() {
            continue;
        }
        let mut clauses: Vec<String> = Vec::new();
        let mut done: std::collections::HashSet<usize> = std::collections::HashSet::new();
        for r in &these {
            if done.contains(&r.group) {
                continue;
            }
            let group: Vec<&AvailabilityRecord> =
                records.iter().filter(|x| x.group == r.group).collect();
            let same: Vec<&&AvailabilityRecord> = group
                .iter()
                .filter(|x| x.state == state && x.short == r.short)
                .collect();
            if group.len() >= 2 && same.len() == group.len() {
                done.insert(r.group);
                let mut group_labels: Vec<String> = Vec::new();
                for x in &group {
                    if !group_labels.contains(&x.provider) {
                        group_labels.push(x.provider.clone());
                    }
                }
                clauses.push(format!("{} :: * ({})", group_labels.join("+"), r.short));
            } else {
                clauses.push(format!("{} ({})", r.lineage, r.short));
            }
        }
        parts.push(format!("{state} - {}", clauses.join(", ")));
    }
    let mut tail = format!("{a} of {n} {noun_word} available");
    if c > 0 {
        tail.push_str(&format!(", {o} out, {c} not checked"));
    }
    parts.push(tail);
    format!("{prefix}{}{suffix}", parts.join("; "))
}

/// `Format-RosterSkips`: skipped entries as `<lineage> (<reason>)`, comma-joined.
pub fn format_roster_skips(skipped: &[(String, String, String, String)]) -> String {
    skipped
        .iter()
        .map(|(provider, model, engine, reason)| {
            format!(
                "{} ({reason})",
                format_reviewer_lineage(provider, model, engine)
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}
