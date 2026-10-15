//! The roster `plan` - one coding plan reached on several routes (C3 wave 1b; the plugin's
//! 0.6.0 wave 29b decisions E5, E7 and E16, `.collab/claude-engine-2026-09-30` handoffs 12/21).
//!
//! A roster entry of ANY engine may name the plan whose quota its route spends (`"plan": "zai"`
//! on a codex `ZAI` entry and on another route to z.ai). Three things follow, all shared by every
//! engine (not tied to the claude engine that introduced the key in the plugin):
//!
//! - **Quota propagation (E5).** The health records of every route of the plan (the fingerprints
//!   of the roster's entries with that plan) are read as ONE record set; when its newest
//!   {usable reply, quota failure} is a quota failure that still blocks, every OTHER route of the
//!   plan is out until the same time ([`plan_quota`], [`plan_quota_verdict`]). Auth, transport
//!   and capability failures stay route-local; without a plan nothing propagates.
//! - **The scheduling group (E7).** A panel's members of one plan share one concurrency group
//!   across engines and routes, limited by the plan's own `parallel` value (default 1) - in
//!   `c3::panel::plan`.
//! - **The machine-wide wait (E16).** A run's row in the machine-wide health file's `running[]`
//!   carries the entry's plan; a panel member waits for a live run of its group's plans from any
//!   engine or repository ([`crate::health::machine_running_count`], [`format_machine_wait`]).
//!
//! Everything here is pure: the caller resolves the identities (fingerprints) of the plan's
//! entries and supplies the task consults (with the machine-wide records folded in).

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::health::{endpoint_health_set, format_offset_iso, MachineRunning, Record};
use crate::verdict::{PlanQuotaRef, PreflightVerdict};

/// One route of a plan: the endpoint fingerprint of a roster entry that names the plan and that
/// entry's provider label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanRoute {
    pub fingerprint: String,
    pub label: String,
}

/// `Get-PlanQuota`'s result.
#[derive(Debug, Clone, Default)]
pub struct PlanQuota {
    pub plan: String,
    /// The blocking quota record (with its fingerprint), `None` when the plan is not out.
    pub quota: Option<Record>,
    /// The blocking record names its reset time.
    pub quota_known: bool,
    /// The provider label of the route the record was recorded on (the first roster entry of
    /// the plan on that fingerprint); `""` when none names it.
    pub label: String,
}

/// The routes of a plan in roster order, one per fingerprint (the first entry's label wins):
/// `entries` are `(plan, fingerprint, label)` of every roster entry whose identity resolved.
pub fn plan_routes(plan: &str, entries: &[(String, String, String)]) -> Vec<PlanRoute> {
    let mut out: Vec<PlanRoute> = Vec::new();
    if plan.is_empty() {
        return out;
    }
    for (p, fp, label) in entries {
        if p != plan || fp.is_empty() {
            continue;
        }
        if out.iter().any(|r| &r.fingerprint == fp) {
            continue;
        }
        out.push(PlanRoute {
            fingerprint: fp.clone(),
            label: label.clone(),
        });
    }
    out
}

/// `Get-PlanQuota` (E5): the records of every route of the plan as one record set; the plan is
/// out while that set's newest {usable reply, quota failure} is a quota failure that still blocks
/// (a usage limit with a reset ahead, or without one for 60 minutes, a burst for 10).
pub fn plan_quota(
    plan: &str,
    routes: &[PlanRoute],
    consults: &[Value],
    now_utc: DateTime<Utc>,
) -> PlanQuota {
    let mut r = PlanQuota {
        plan: plan.to_string(),
        ..Default::default()
    };
    if routes.is_empty() {
        return r;
    }
    let fps: Vec<String> = routes.iter().map(|x| x.fingerprint.clone()).collect();
    let h = endpoint_health_set(consults, &fps, now_utc);
    if let Some(q) = h.quota {
        r.label = routes
            .iter()
            .find(|x| x.fingerprint == q.fingerprint)
            .map(|x| x.label.clone())
            .unwrap_or_default();
        r.quota_known = h.quota_known;
        r.quota = Some(q);
    }
    r
}

/// `Get-PlanQuotaVerdict` (E5): `verdict` (the entry's own preflight verdict) with the plan quota
/// applied. An entry whose own verdict is `available` or `unknown` of kind `unknown` (sign-in not
/// checked) is OUT when its plan is out on ANOTHER route:
/// `plan <slug> (usage limit on <label> until <iso>)`, or without a reset time
/// `plan <slug> (usage limit on <label> hit <iso>, reset unknown; retry after <iso>)` (a burst:
/// `burst limit (429) on ...`); kind `quota` | `quota-unknown-reset`. Any other verdict, an entry
/// without a plan, a plan that is not out, or one out on this entry's OWN route (its own verdict
/// says it): `verdict` unchanged. `roster_walk` drops the `-SkipPreflight` hint from the refusal.
pub fn plan_quota_verdict(
    verdict: PreflightVerdict,
    plan: &str,
    own_fingerprint: &str,
    provider: &str,
    pq: &PlanQuota,
    roster_walk: bool,
) -> PreflightVerdict {
    if plan.is_empty() {
        return verdict;
    }
    if !(verdict.state == "available" || (verdict.state == "unknown" && verdict.kind == "unknown"))
    {
        return verdict;
    }
    let q = match &pq.quota {
        Some(q) => q,
        None => return verdict,
    };
    if !own_fingerprint.is_empty() && q.fingerprint == own_fingerprint {
        return verdict;
    }
    let on = if pq.label.is_empty() {
        "another route".to_string()
    } else {
        pq.label.clone()
    };
    let tail = if roster_walk {
        ""
    } else {
        " (pass -SkipPreflight to launch anyway)"
    };
    let mut v = PreflightVerdict::available();
    v.state = "unavailable".into();
    v.hit = Some(q.hit);
    v.credential = verdict.credential;
    v.plan_quota = Some(PlanQuotaRef {
        plan: plan.to_string(),
        label: on.clone(),
    });
    if pq.quota_known {
        v.kind = "quota".into();
        v.until = q.retry_after;
        v.reason = format!(
            "plan {plan} (usage limit on {on} until {})",
            q.retry_after_iso
        );
        v.refusal = format!(
            "provider {provider} is not usable: its plan {plan} hit a usage limit on {on} at {} ({}) that lasts until {}; nothing was started{tail}",
            q.when, q.message, q.retry_after_iso
        );
    } else {
        v.kind = "quota-unknown-reset".into();
        v.burst = q.kind == "burst";
        v.until = Some(q.until);
        let until_iso = format_offset_iso(q.until);
        let what = if v.burst {
            "burst limit (429)"
        } else {
            "usage limit"
        };
        v.reason = format!(
            "plan {plan} ({what} on {on} hit {}, reset unknown; retry after {until_iso})",
            q.hit_iso
        );
        v.refusal = format!(
            "provider {provider} is not usable: its plan {plan} hit a {} limit on {on} at {} ({}) and named no reset time - out until {until_iso}; nothing was started{tail}",
            if v.burst { "burst" } else { "usage" },
            q.hit_iso,
            q.message
        );
    }
    v.preflight = format!("unavailable: {}", v.reason);
    v.label = format!(
        "unavailable ({}) - a real run is refused: {}",
        v.reason, v.refusal
    );
    v
}

/// The panel's wait line (E16, `codex-consult.ps1`'s scheduler): a member of panel slot `k` of
/// `total` waits while `rows` (live runs elsewhere on the machine - another repository, another
/// panel, a single run) use its group's endpoints or plans up to `limit`:
/// `  panel member <k> of <n> waits: <c> run(s) elsewhere on this machine use <what> (parallel
/// limit <l>): <label> in <repo> task <t> handoff <nn> ([plan <p>, ]pid <pid>); ...` - `<what>`
/// is `its endpoint`, `its plan <p>[, <q>]` (every row counts only through its plan) or `its
/// endpoint or its plan <p>`; `(plan <p>, ` marks the rows that count only through their plan.
pub fn format_machine_wait(
    k: i64,
    total: usize,
    rows: &[MachineRunning],
    by_plan: &[MachineRunning],
    limit: i64,
) -> String {
    let by_plan_pids: Vec<u32> = by_plan.iter().map(|r| r.pid).collect();
    let who = rows
        .iter()
        .map(|row| {
            let plan_part = if !row.plan.is_empty() && by_plan_pids.contains(&row.pid) {
                format!("plan {}, ", row.plan)
            } else {
                String::new()
            };
            format!(
                "{} in {} task {} handoff {} ({plan_part}pid {})",
                row.label, row.repo, row.task, row.nn, row.pid
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    let mut used_plans: Vec<&str> = Vec::new();
    for r in by_plan {
        if !r.plan.is_empty() && !used_plans.contains(&r.plan.as_str()) {
            used_plans.push(r.plan.as_str());
        }
    }
    let what = if by_plan.is_empty() {
        "its endpoint".to_string()
    } else if by_plan.len() == rows.len() {
        format!("its plan {}", used_plans.join(", "))
    } else {
        format!("its endpoint or its plan {}", used_plans.join(", "))
    };
    format!(
        "  panel member {k} of {total} waits: {} run(s) elsewhere on this machine use {what} (parallel limit {limit}): {who}",
        rows.len()
    )
}

// ------------------------------------------------------------ the declared plan cap (wave 9, D4)

/// The next instant a declared plan cap resets after `after`, from the roster's `ext.c3.reset_hint`:
/// `monthly:<DD>T<HH:MM><offset>` (day of the month, clamped to the month's length),
/// `weekly:<Mon..Sun>T<HH:MM><offset>` or `daily:<HH:MM><offset>` (`offset` like `+08:00`). `None`
/// when the hint does not parse.
pub fn next_cap_reset(hint: &str, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
    use chrono::{Datelike, Duration, FixedOffset, NaiveDate, NaiveTime, TimeZone, Weekday};
    // the grammar is ASCII; anything else is malformed (and never reaches a byte-offset split)
    if !hint.is_ascii() {
        return None;
    }
    let (kind, rest) = hint.split_once(':')?;
    // the clock and the offset: "HH:MM+08:00" (the last 6 characters are the offset)
    let split_clock = |s: &str| -> Option<(NaiveTime, FixedOffset)> {
        if !s.is_ascii() || s.len() < 11 {
            return None;
        }
        let (clock, off) = s.split_at(s.len() - 6);
        let t = NaiveTime::parse_from_str(clock, "%H:%M").ok()?;
        let sign = match off.as_bytes()[0] {
            b'+' => 1,
            b'-' => -1,
            _ => return None,
        };
        let (oh, om) = off[1..].split_once(':')?;
        let secs = sign * (oh.parse::<i32>().ok()? * 3600 + om.parse::<i32>().ok()? * 60);
        Some((t, FixedOffset::east_opt(secs)?))
    };
    match kind {
        "monthly" => {
            let (day, clock) = rest.split_once('T')?;
            let day: u32 = day.parse().ok().filter(|d| (1..=31).contains(d))?;
            let (t, off) = split_clock(clock)?;
            let local = after.with_timezone(&off);
            let (mut y, mut m) = (local.year(), local.month());
            for _ in 0..14 {
                let last = (28..=31)
                    .rev()
                    .find(|d| NaiveDate::from_ymd_opt(y, m, *d).is_some())?;
                let d = NaiveDate::from_ymd_opt(y, m, day.min(last))?;
                let cand = off.from_local_datetime(&d.and_time(t)).single()?;
                if cand.with_timezone(&Utc) > after {
                    return Some(cand.with_timezone(&Utc));
                }
                if m == 12 {
                    y += 1;
                    m = 1;
                } else {
                    m += 1;
                }
            }
            None
        }
        "weekly" => {
            let (wd, clock) = rest.split_once('T')?;
            let wd: Weekday = wd.parse().ok()?;
            let (t, off) = split_clock(clock)?;
            let local = after.with_timezone(&off);
            for add in 0..=7 {
                let d = local.date_naive() + Duration::days(add);
                if d.weekday() != wd {
                    continue;
                }
                let cand = off.from_local_datetime(&d.and_time(t)).single()?;
                if cand.with_timezone(&Utc) > after {
                    return Some(cand.with_timezone(&Utc));
                }
            }
            None
        }
        "daily" => {
            let (t, off) = split_clock(rest)?;
            let local = after.with_timezone(&off);
            for add in 0..=1 {
                let d = local.date_naive() + Duration::days(add);
                let cand = off.from_local_datetime(&d.and_time(t)).single()?;
                if cand.with_timezone(&Utc) > after {
                    return Some(cand.with_timezone(&Utc));
                }
            }
            None
        }
        _ => None,
    }
}

/// (wave 9, D4) The verdict of a roster entry that declares a plan cap (`ext.c3.plan_caps`): an
/// instant 429 with no window text - `n >= 2` of them within 60 minutes of each other, newer than
/// the endpoint's newest usable reply - marks the entry `unavailable (plan quota: 429 twice in <n>
/// min, no window text; retry after <next reset>)` until the cap's next reset (the entry's
/// `reset_hint`, else 24 hours after the first hit of the pair), instead of the 10-minute burst. An entry
/// without a declared cap keeps `verdict` as it is.
pub fn plan_cap_verdict(
    verdict: PreflightVerdict,
    entry: &crate::roster::RosterEntry,
    health: Option<&crate::health::EndpointHealth>,
    now: DateTime<Utc>,
) -> PreflightVerdict {
    use chrono::Duration;
    if entry.plan_caps.is_empty()
        || !(verdict.kind.is_empty() || verdict.kind == "quota-unknown-reset")
    {
        return verdict;
    }
    let Some(all_hits) = health.map(|h| &h.instant_429s) else {
        return verdict;
    };
    // (wave 9e, F76-1) a hit is pair evidence only while it lies inside the entry's CURRENT cap
    // period: for a hinted entry iff the reset that follows the hit has not passed yet
    // (`next_cap_reset(hint, hit) > now`), for an unhinted entry iff it is younger than 24 h at read
    // time. `endpoint_health` retains id-carrying hits far longer (35 days), so this per-entry bound
    // is applied here, where the hint is known, and every reader gets the same verdict regardless of
    // retention. A pair lying entirely before the last reset is no evidence.
    let hits: Vec<DateTime<chrono::FixedOffset>> = all_hits
        .iter()
        .copied()
        .filter(|hit| {
            let hit_utc = hit.with_timezone(&Utc);
            if entry.reset_hint.is_empty() {
                now - hit_utc <= Duration::hours(24)
            } else {
                next_cap_reset(&entry.reset_hint, hit_utc).is_none_or(|r| r > now)
            }
        })
        .collect();
    if hits.len() < 2 {
        return verdict;
    }
    // (wave 9f, F78-1) the newest QUALIFYING pair: ANY two distinct-id hits inside the period within
    // 60 min of each other. Fixing `second = hits[0]` (the single newest hit) and pairing only with
    // it let a later isolated instant 429 hide an established earlier pair, so the exhausted entry
    // read available before its declared reset. We scan newest-first and take the first `second` that
    // has a partner (the newest older hit) within the hour.
    let mut sorted = hits;
    sorted.sort_by(|a, b| b.cmp(a));
    let mut found = None;
    for (i, &second) in sorted.iter().enumerate() {
        if let Some(&first) = sorted[i + 1..]
            .iter()
            .find(|h| second - **h <= Duration::minutes(60))
        {
            found = Some((second, first));
            break;
        }
    }
    let Some((second, first)) = found else {
        return verdict;
    };
    let gap_min = ((second - first).num_seconds() as f64 / 60.0).ceil() as i64;
    let second_utc = second.with_timezone(&Utc);
    let until_utc = if entry.reset_hint.is_empty() {
        // the pair ages out of the read-time 24 h bound with its first hit
        first.with_timezone(&Utc) + Duration::hours(24)
    } else {
        next_cap_reset(&entry.reset_hint, second_utc).unwrap_or(second_utc + Duration::hours(24))
    };
    if until_utc <= now {
        return verdict;
    }
    let until = until_utc.with_timezone(second.offset());
    let until_iso = format_offset_iso(until);
    let mut v = verdict;
    v.state = "unavailable".into();
    v.kind = "quota".into();
    v.burst = false;
    v.hit = Some(second);
    v.until = Some(until);
    v.reason =
        format!("plan quota: 429 twice in {gap_min} min, no window text; retry after {until_iso}");
    v.preflight = format!("unavailable: {}", v.reason);
    v.refusal = format!(
        "provider {} is not usable: {} (the plan declares a {} cap); nothing was started (pass -SkipPreflight to launch anyway)",
        entry.provider,
        v.reason,
        entry.plan_caps.join("/")
    );
    v.label = format!(
        "unavailable ({}) - a real run is refused: {}",
        v.reason, v.refusal
    );
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credential::CredentialResult;
    use crate::lineage::ReviewerIdentity;
    use crate::verdict::verdict_with_credential;
    use chrono::TimeZone;
    use serde_json::json;

    // UNIT (wave 9, D4): the declared cap's reset rule and the two-instant-429 verdict.
    #[test]
    fn the_cap_reset_hint_names_the_next_reset() {
        let at = |s: &str| s.parse::<DateTime<Utc>>().unwrap();
        // monthly:27T00:00+08:00 = the 26th, 16:00 UTC
        assert_eq!(
            next_cap_reset("monthly:27T00:00+08:00", at("2026-10-14T20:10:00Z")),
            Some(at("2026-10-26T16:00:00Z"))
        );
        // already past this month: the next one
        assert_eq!(
            next_cap_reset("monthly:27T00:00+08:00", at("2026-10-27T01:00:00Z")),
            Some(at("2026-11-26T16:00:00Z"))
        );
        // the 31st clamps to the month's length
        assert_eq!(
            next_cap_reset("monthly:31T00:00+00:00", at("2026-11-02T00:00:00Z")),
            Some(at("2026-11-30T00:00:00Z"))
        );
        assert_eq!(
            next_cap_reset("daily:04:00+00:00", at("2026-10-14T05:00:00Z")),
            Some(at("2026-10-15T04:00:00Z"))
        );
        assert_eq!(
            next_cap_reset("weekly:MonT00:00+00:00", at("2026-10-14T05:00:00Z")),
            Some(at("2026-10-19T00:00:00Z"))
        );
        assert_eq!(
            next_cap_reset("monthly:99T00:00+08:00", at("2026-10-14T05:00:00Z")),
            None
        );
        assert_eq!(next_cap_reset("hourly:1", at("2026-10-14T05:00:00Z")), None);
    }

    // UNIT (F66-3, F68-1, F72-1/2): the pair is built from id-carrying rows only, counted by exact
    // consult id, and only from hits younger than 24 h at read time.
    #[test]
    fn hits_are_counted_by_consultation_identity_not_by_time() {
        use crate::health::endpoint_health;
        use crate::roster::RosterEntry;
        let now = Utc.with_ymd_and_hms(2026, 10, 14, 20, 10, 0).unwrap();
        let row = |id: &str, n: i64, msg: &str, when: &str, fin: &str, machine: bool| {
            let mut v = json!({"n": n, "when": when, "finished_at": fin,
                "bridge_outcome": format!("failed: {msg}"),
                "reviewer": {"provider_fingerprint": FP_A},
                "provider_failure": {"class": "quota", "code": "429",
                    "message": msg, "when": when, "retry_after": null}});
            if !id.is_empty() {
                v["consult_id"] = json!(id);
            }
            if machine {
                v["machine"] = json!(true);
            }
            v
        };
        let capped = RosterEntry {
            provider: "ali".into(),
            plan_caps: vec!["monthly".into()],
            reset_hint: "monthly:27T00:00+08:00".into(),
            ..Default::default()
        };
        let verdict = |rows: &[serde_json::Value], at: DateTime<Utc>| {
            let h = endpoint_health(rows, FP_A, at);
            (
                h.instant_429s.len(),
                plan_cap_verdict(PreflightVerdict::available(), &capped, Some(&h), at).state,
            )
        };
        const M: &str = "429 Too Many Requests (request id r-1)";
        let w = "2026-10-14T20:06:00+00:00";
        let f = "2026-10-14T20:06:01+00:00";
        let ledger = row("c-1", 1, M, w, f, false);
        let avail = (1, "available".to_string());
        let out = (2, "unavailable".to_string());
        // the same attempt mirrored on the machine file, with its consult id: one hit
        assert_eq!(
            verdict(&[ledger.clone(), row("c-1", 0, M, f, f, true)], now),
            avail
        );
        // a legacy mirror WITHOUT an id takes no part: the ledger failure alone, one hit
        assert_eq!(
            verdict(&[ledger.clone(), row("", 0, M, f, f, true)], now),
            avail
        );
        // ... and a legacy row never completes a pair, whatever it says
        let other = "429 Too Many Requests (request id r-9)";
        let f3 = "2026-10-14T20:06:04+00:00";
        assert_eq!(
            verdict(&[ledger.clone(), row("", 0, other, f3, f3, true)], now),
            avail
        );
        assert_eq!(
            verdict(
                &[row("", 1, M, w, f, false), row("", 2, M, f3, f3, true)],
                now
            )
            .0,
            0
        );
        // two DIFFERENT ids at one instant, identical texts: two consultations -> the plan verdict
        assert_eq!(
            verdict(&[ledger.clone(), row("c-2", 2, M, w, f, false)], now),
            out
        );
        assert_eq!(
            verdict(&[ledger.clone(), row("c-2", 0, M, f, f, true)], now),
            out
        );
        // two id-carrying hits 10 min apart: the plan verdict
        let earlier = row(
            "c-3",
            0,
            M,
            "2026-10-14T19:56:00+00:00",
            "2026-10-14T19:56:00+00:00",
            true,
        );
        assert_eq!(verdict(&[ledger.clone(), earlier], now), out);
        // two id-carrying hits 25 h old, read a day later. The per-entry cap-period rule (wave 9e)
        // decides: the UNHINTED entry no longer counts them (24 h bound) -> no cap verdict; the
        // HINTED entry, whose declared reset is still ahead, still gets the plan verdict.
        let old = Utc.with_ymd_and_hms(2026, 10, 15, 21, 0, 0).unwrap();
        let aged = [
            ledger,
            row(
                "c-3",
                0,
                M,
                "2026-10-14T19:56:00+00:00",
                "2026-10-14T19:56:00+00:00",
                true,
            ),
        ];
        let state = |rows: &[serde_json::Value], e: &RosterEntry, at: DateTime<Utc>| {
            let h = endpoint_health(rows, FP_A, at);
            plan_cap_verdict(PreflightVerdict::available(), e, Some(&h), at).state
        };
        let unhinted = RosterEntry {
            reset_hint: String::new(),
            ..capped.clone()
        };
        assert_eq!(state(&aged, &unhinted, old), "available");
        assert_eq!(state(&aged, &capped, old), "unavailable");
    }

    // UNIT (F66-6): the chosen policy, pinned with a controlled clock. The pair is INFERRED
    // exhaustion, not evidence: (a) a transient pair on a capped plan costs the time to the
    // declared reset (the operator clears it with a successful explicit run or by editing the
    // hint); (b) without a hint the entry is out for 24 h and then PROBED - eligible again, which
    // is not evidence that capacity returned (a new instant 429 marks it out again).
    #[test]
    fn the_cap_policy_is_inferred_exhaustion_with_a_24_hour_probe() {
        use crate::health::endpoint_health;
        use crate::roster::RosterEntry;
        let hit = |n: i64, when: &str| {
            json!({"consult_id": format!("c-{n}"), "n": n, "when": when, "finished_at": when,
                "bridge_outcome": "failed: 429 Too Many Requests",
                "reviewer": {"provider_fingerprint": FP_A},
                "provider_failure": {"class": "quota", "code": "429",
                    "message": "429 Too Many Requests", "when": when, "retry_after": null}})
        };
        let rows = [
            hit(1, "2026-10-14T20:00:00+00:00"),
            hit(2, "2026-10-14T20:20:00+00:00"),
        ];
        let hinted = RosterEntry {
            provider: "ali".into(),
            plan_caps: vec!["monthly".into()],
            reset_hint: "monthly:27T00:00+08:00".into(),
            ..Default::default()
        };
        let unhinted = RosterEntry {
            reset_hint: String::new(),
            ..hinted.clone()
        };
        let state = |e: &RosterEntry, now: DateTime<Utc>| {
            let h = endpoint_health(&rows, FP_A, now);
            plan_cap_verdict(PreflightVerdict::available(), e, Some(&h), now).state
        };
        // (a) a transient pair, the endpoint recovers an hour later - the hint still holds it out
        let soon = Utc.with_ymd_and_hms(2026, 10, 14, 21, 30, 0).unwrap();
        assert_eq!(state(&hinted, soon), "unavailable");
        // (F76-1, RC1) two days later - well past 24 h - the hinted entry is STILL out: its declared
        // reset (the 26th, 16:00 UTC) has not arrived, so the 24 h read-time bound must not free it.
        let rc1 = Utc.with_ymd_and_hms(2026, 10, 16, 20, 30, 0).unwrap();
        assert_eq!(state(&hinted, rc1), "unavailable");
        let at_reset = Utc.with_ymd_and_hms(2026, 10, 26, 16, 0, 1).unwrap();
        assert_eq!(state(&hinted, at_reset), "available");
        // (F76-1) a hinted pair lying entirely BEFORE the last reset is no evidence once that reset
        // has passed: read the following day, the entry is available again.
        let after_reset = Utc.with_ymd_and_hms(2026, 10, 27, 0, 0, 0).unwrap();
        assert_eq!(state(&hinted, after_reset), "available");
        // (F78-1, RC1) a later ISOLATED instant 429 (a third distinct consultation at 22:00Z, no
        // partner within the hour) must not hide the established 20:00/20:20 pair: read at 22:11Z the
        // monthly refusal remains, until the declared reset (the 26th, 16:00 UTC).
        let rows3 = [
            hit(1, "2026-10-14T20:00:00+00:00"),
            hit(2, "2026-10-14T20:20:00+00:00"),
            hit(3, "2026-10-14T22:00:00+00:00"),
        ];
        let at3 = Utc.with_ymd_and_hms(2026, 10, 14, 22, 11, 0).unwrap();
        let h3 = endpoint_health(&rows3, FP_A, at3);
        let v3 = plan_cap_verdict(PreflightVerdict::available(), &hinted, Some(&h3), at3);
        assert_eq!(v3.state, "unavailable");
        assert_eq!(
            v3.until.map(|u| u.with_timezone(&Utc)),
            Some(Utc.with_ymd_and_hms(2026, 10, 26, 16, 0, 0).unwrap())
        );
        // (b) no hint: out until the pair ages out at READ time (24 h after its first hit), then a probe
        let day = Utc.with_ymd_and_hms(2026, 10, 15, 19, 59, 0).unwrap();
        assert_eq!(state(&unhinted, day), "unavailable");
        let probe = Utc.with_ymd_and_hms(2026, 10, 15, 20, 1, 0).unwrap();
        assert_eq!(state(&unhinted, probe), "available");
        // a successful explicit run after the pair clears it (the operator's override)
        let ok = json!({"n": 3, "when": "2026-10-14T21:00:00+00:00",
            "finished_at": "2026-10-14T21:00:10+00:00", "bridge_outcome": "usable reply",
            "reviewer": {"provider_fingerprint": FP_A}});
        let mut with_ok = rows.to_vec();
        with_ok.push(ok);
        let h = endpoint_health(&with_ok, FP_A, soon);
        assert_eq!(
            plan_cap_verdict(PreflightVerdict::available(), &hinted, Some(&h), soon).state,
            "available"
        );
    }

    #[test]
    fn two_instant_429s_inside_an_hour_mark_a_capped_entry_out_until_the_reset() {
        use crate::health::endpoint_health;
        use crate::roster::RosterEntry;
        let now = Utc.with_ymd_and_hms(2026, 10, 14, 20, 10, 0).unwrap();
        let hit = |n: i64, when: &str, fin: &str| {
            json!({"consult_id": format!("c-{n}"), "n": n, "when": when, "finished_at": fin,
                "bridge_outcome": "failed: 429 Too Many Requests",
                "reviewer": {"provider_fingerprint": FP_A},
                "provider_failure": {"class": "quota", "code": "429",
                    "message": "429 Too Many Requests", "when": when, "retry_after": null}})
        };
        let one = hit(1, "2026-10-14T19:29:00+00:00", "2026-10-14T19:29:01+00:00");
        let two = hit(2, "2026-10-14T20:06:00+00:00", "2026-10-14T20:06:00+00:00");
        let capped = RosterEntry {
            provider: "ali".into(),
            plan_caps: vec!["monthly".into()],
            reset_hint: "monthly:27T00:00+08:00".into(),
            ..Default::default()
        };
        let plain = RosterEntry {
            provider: "ali".into(),
            ..Default::default()
        };
        let avail = || PreflightVerdict::available();
        let h = endpoint_health(&[one.clone(), two.clone()], FP_A, now);
        assert_eq!(h.instant_429s.len(), 2);
        let v = plan_cap_verdict(avail(), &capped, Some(&h), now);
        assert_eq!(
            (v.state.as_str(), v.kind.as_str()),
            ("unavailable", "quota")
        );
        assert_eq!(
            v.until.map(|u| u.with_timezone(&Utc)),
            Some(Utc.with_ymd_and_hms(2026, 10, 26, 16, 0, 0).unwrap())
        );
        assert!(
            v.reason.starts_with(
                "plan quota: 429 twice in 37 min, no window text; retry after 2026-10-26"
            ),
            "{}",
            v.reason
        );
        // without the declaration, nothing changes (the 10-minute burst stays the verdict's job)
        assert_eq!(
            plan_cap_verdict(avail(), &plain, Some(&h), now).state,
            "available"
        );
        // the capped entry without a hint: +24 h after the first hit (the pair ages out)
        let no_hint = RosterEntry {
            reset_hint: String::new(),
            ..capped.clone()
        };
        let v24 = plan_cap_verdict(avail(), &no_hint, Some(&h), now);
        assert_eq!(
            v24.until.map(|u| u.with_timezone(&Utc)),
            Some(Utc.with_ymd_and_hms(2026, 10, 15, 19, 29, 0).unwrap())
        );
        // more than an hour apart: no cap verdict
        let far = hit(0, "2026-10-14T18:00:00+00:00", "2026-10-14T18:00:01+00:00");
        let h_far = endpoint_health(&[far, two.clone()], FP_A, now);
        assert_eq!(
            plan_cap_verdict(avail(), &capped, Some(&h_far), now).state,
            "available"
        );
        // not instant (a 5 s turn): not counted
        let slow = hit(1, "2026-10-14T19:29:00+00:00", "2026-10-14T19:29:05+00:00");
        let h_slow = endpoint_health(&[slow, two.clone()], FP_A, now);
        assert_eq!(
            plan_cap_verdict(avail(), &capped, Some(&h_slow), now).state,
            "available"
        );
        // after the cap's reset: available again
        let later = Utc.with_ymd_and_hms(2026, 10, 26, 17, 0, 0).unwrap();
        let h_late = endpoint_health(&[one, two], FP_A, later);
        assert_eq!(
            plan_cap_verdict(avail(), &capped, Some(&h_late), later).state,
            "available"
        );
    }

    const FP_A: &str = "fp-zai-codex";
    const FP_B: &str = "fp-zai-other";

    fn quota_failure(fp: &str, when: &str, retry_after: Option<&str>, msg: &str) -> Value {
        json!({
            "n": 1,
            "when": when,
            "finished_at": when,
            "bridge_outcome": "failed: provider error",
            "reviewer": { "provider_fingerprint": fp },
            "provider_failure": {
                "class": "quota", "code": "429", "message": msg, "when": when,
                "retry_after": retry_after
            }
        })
    }

    fn other_failure(fp: &str, when: &str, class: &str, msg: &str) -> Value {
        json!({
            "n": 1,
            "when": when,
            "finished_at": when,
            "bridge_outcome": "failed: provider error",
            "reviewer": { "provider_fingerprint": fp },
            "provider_failure": { "class": class, "code": "", "message": msg, "when": when }
        })
    }

    fn usable(fp: &str, when: &str, n: i64) -> Value {
        json!({
            "n": n,
            "when": when,
            "finished_at": when,
            "bridge_outcome": "usable reply",
            "reviewer": { "provider_fingerprint": fp }
        })
    }

    fn routes() -> Vec<PlanRoute> {
        plan_routes(
            "zai",
            &[
                ("zai".into(), FP_A.into(), "ZAI".into()),
                ("".into(), "fp-openai".into(), "openai".into()),
                ("zai".into(), FP_B.into(), "ZAI-b".into()),
                ("zai".into(), FP_A.into(), "ZAI-second-entry".into()),
            ],
        )
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 8, 12, 0, 0).unwrap()
    }

    fn ident(provider: &str) -> ReviewerIdentity {
        let cfg = crate::config::scan_config_text(
            "",
            "model = \"gpt-5.1\"
",
        );
        let mut id =
            crate::lineage::resolve_reviewer_identity(&cfg, "openai", "gpt-5.1", "", "codex", "");
        id.provider = provider.into();
        id
    }

    fn available_verdict(provider: &str) -> PreflightVerdict {
        verdict_with_credential(
            &ident(provider),
            None,
            CredentialResult::ok("env K set"),
            true,
        )
    }

    #[test]
    fn plan_routes_one_per_fingerprint_in_roster_order() {
        let r = routes();
        assert_eq!(
            r,
            vec![
                PlanRoute {
                    fingerprint: FP_A.into(),
                    label: "ZAI".into()
                },
                PlanRoute {
                    fingerprint: FP_B.into(),
                    label: "ZAI-b".into()
                },
            ]
        );
        assert!(plan_routes("", &[("".into(), "x".into(), "y".into())]).is_empty());
    }

    #[test]
    fn a_quota_failure_on_one_route_marks_the_other_out_until_the_same_reset() {
        let consults = vec![quota_failure(
            FP_A,
            "2026-10-08T13:30:00+02:00",
            Some("2026-10-08T17:00:00+02:00"),
            "usage limit reached for the 5 hour window",
        )];
        let pq = plan_quota("zai", &routes(), &consults, now());
        assert!(pq.quota_known);
        assert_eq!(pq.label, "ZAI");
        let v = plan_quota_verdict(available_verdict("ZAI-b"), "zai", FP_B, "ZAI-b", &pq, true);
        assert_eq!(v.state, "unavailable");
        assert_eq!(v.kind, "quota");
        assert_eq!(
            v.reason,
            "plan zai (usage limit on ZAI until 2026-10-08T17:00:00+02:00)"
        );
        assert_eq!(
            v.refusal,
            "provider ZAI-b is not usable: its plan zai hit a usage limit on ZAI at 2026-10-08T13:30:00+02:00 (usage limit reached for the 5 hour window) that lasts until 2026-10-08T17:00:00+02:00; nothing was started"
        );
        assert_eq!(v.preflight, format!("unavailable: {}", v.reason));
        assert_eq!(
            v.plan_quota,
            Some(PlanQuotaRef {
                plan: "zai".into(),
                label: "ZAI".into()
            })
        );
        assert_eq!(
            v.until.map(format_offset_iso).as_deref(),
            Some("2026-10-08T17:00:00+02:00")
        );
        // a direct run keeps the -SkipPreflight hint
        let d = plan_quota_verdict(available_verdict("ZAI-b"), "zai", FP_B, "ZAI-b", &pq, false);
        assert!(d
            .refusal
            .ends_with("nothing was started (pass -SkipPreflight to launch anyway)"));
        assert!(d
            .label
            .starts_with("unavailable (plan zai (usage limit on ZAI until "));
        // the route that recorded it: its own verdict says it (unchanged here)
        let own = plan_quota_verdict(available_verdict("ZAI"), "zai", FP_A, "ZAI", &pq, true);
        assert_eq!(own.state, "available");
        // an entry without a plan: unchanged
        let none = plan_quota_verdict(available_verdict("x"), "", "fp-x", "x", &pq, true);
        assert_eq!(none.state, "available");
    }

    #[test]
    fn the_plan_works_both_ways_and_only_for_quota() {
        // a usage limit on the other route marks the first one out
        let back = vec![quota_failure(
            FP_B,
            "2026-10-08T13:00:00+02:00",
            Some("2026-10-08T16:00:00+02:00"),
            "usage limit reached",
        )];
        let pq = plan_quota("zai", &routes(), &back, now());
        let v = plan_quota_verdict(available_verdict("ZAI"), "zai", FP_A, "ZAI", &pq, true);
        assert!(v
            .reason
            .starts_with("plan zai (usage limit on ZAI-b until "));
        // an AUTH failure never propagates
        let auth = vec![other_failure(
            FP_A,
            "2026-10-08T13:30:00+02:00",
            "auth",
            "invalid api key (401)",
        )];
        let pq = plan_quota("zai", &routes(), &auth, now());
        assert!(pq.quota.is_none());
        let v = plan_quota_verdict(available_verdict("ZAI-b"), "zai", FP_B, "ZAI-b", &pq, true);
        assert_eq!(v.state, "available");
        // nor a transport failure
        let tr = vec![other_failure(
            FP_A,
            "2026-10-08T13:30:00+02:00",
            "transport",
            "connection reset",
        )];
        assert!(plan_quota("zai", &routes(), &tr, now()).quota.is_none());
        // a usable reply on the other route AFTER the limit clears the plan (one record set)
        let cleared = vec![
            quota_failure(
                FP_A,
                "2026-10-08T13:30:00+02:00",
                Some("2026-10-08T17:00:00+02:00"),
                "usage limit reached",
            ),
            usable(FP_B, "2026-10-08T13:40:00+02:00", 2),
        ];
        assert!(plan_quota("zai", &routes(), &cleared, now())
            .quota
            .is_none());
        // ... but a usable reply BEFORE it does not
        let before = vec![
            usable(FP_B, "2026-10-08T13:20:00+02:00", 2),
            quota_failure(
                FP_A,
                "2026-10-08T13:30:00+02:00",
                Some("2026-10-08T17:00:00+02:00"),
                "usage limit reached",
            ),
        ];
        assert!(plan_quota("zai", &routes(), &before, now()).quota.is_some());
        // the reset passed: nothing blocks
        let later = Utc.with_ymd_and_hms(2026, 10, 8, 15, 30, 0).unwrap();
        assert!(plan_quota("zai", &routes(), &cleared[..1], later)
            .quota
            .is_none());
    }

    #[test]
    fn a_reset_less_limit_and_a_burst_name_their_window() {
        let consults = vec![quota_failure(
            FP_A,
            "2026-10-08T13:45:00+02:00",
            None,
            "You've hit your usage limit; try again later.",
        )];
        let pq = plan_quota("zai", &routes(), &consults, now());
        assert!(!pq.quota_known);
        let v = plan_quota_verdict(available_verdict("ZAI-b"), "zai", FP_B, "ZAI-b", &pq, true);
        assert_eq!(v.kind, "quota-unknown-reset");
        assert_eq!(
            v.reason,
            "plan zai (usage limit on ZAI hit 2026-10-08T13:45:00+02:00, reset unknown; retry after 2026-10-08T14:45:00+02:00)"
        );
        assert_eq!(
            v.refusal,
            "provider ZAI-b is not usable: its plan zai hit a usage limit on ZAI at 2026-10-08T13:45:00+02:00 (You've hit your usage limit; try again later.) and named no reset time - out until 2026-10-08T14:45:00+02:00; nothing was started"
        );
        assert!(!v.burst);
        let burst = vec![quota_failure(
            FP_A,
            "2026-10-08T13:55:00+02:00",
            None,
            "429 Too Many Requests",
        )];
        let pq = plan_quota("zai", &routes(), &burst, now());
        let v = plan_quota_verdict(available_verdict("ZAI-b"), "zai", FP_B, "ZAI-b", &pq, false);
        assert!(v.burst);
        assert_eq!(
            v.reason,
            "plan zai (burst limit (429) on ZAI hit 2026-10-08T13:55:00+02:00, reset unknown; retry after 2026-10-08T14:05:00+02:00)"
        );
        assert!(v
            .refusal
            .contains("its plan zai hit a burst limit on ZAI at "));
        assert!(v
            .refusal
            .ends_with("out until 2026-10-08T14:05:00+02:00; nothing was started (pass -SkipPreflight to launch anyway)"));
    }

    #[test]
    fn only_an_available_or_not_checked_verdict_is_replaced() {
        let consults = vec![quota_failure(
            FP_A,
            "2026-10-08T13:30:00+02:00",
            Some("2026-10-08T17:00:00+02:00"),
            "usage limit reached",
        )];
        let pq = plan_quota("zai", &routes(), &consults, now());
        let id = ident("ZAI-b");
        // missing credentials stay the entry's own verdict
        let missing =
            verdict_with_credential(&id, None, CredentialResult::missing("env K not set"), true);
        let v = plan_quota_verdict(missing, "zai", FP_B, "ZAI-b", &pq, true);
        assert_eq!(v.kind, "credentials");
        // "sign-in not checked" (unknown/unknown) is replaced, its credential kept
        let unknown = verdict_with_credential(
            &id,
            None,
            CredentialResult::unknown("sign-in not checked"),
            true,
        );
        let v = plan_quota_verdict(unknown, "zai", FP_B, "ZAI-b", &pq, true);
        assert_eq!(v.kind, "quota");
        assert_eq!(
            v.credential.map(|c| c.reason).as_deref(),
            Some("sign-in not checked")
        );
        // no route of the plan resolved: nothing to read
        let pq0 = plan_quota("zai", &[], &consults, now());
        assert!(pq0.quota.is_none());
        // a label-less record (its fingerprint is no roster route) reads "another route"
        let pq2 = PlanQuota {
            label: String::new(),
            ..pq.clone()
        };
        let v = plan_quota_verdict(available_verdict("ZAI-b"), "zai", FP_B, "ZAI-b", &pq2, true);
        assert!(v.reason.contains("usage limit on another route until"));
    }

    #[test]
    fn a_quota_mark_on_a_usable_reply_counts_as_a_quota_failure_after_it() {
        // (E15) a usable reply carrying engine_run.quota_mark: the route is out until the mark's
        // reset, and the plan with it; a later usable reply clears it
        let marked = json!({
            "n": 3,
            "when": "2026-10-08T13:30:00+02:00",
            "finished_at": "2026-10-08T13:31:00+02:00",
            "bridge_outcome": "usable reply",
            "reviewer": { "provider_fingerprint": FP_B },
            "engine_run": { "quota_mark": {
                "class": "quota", "kind": "", "code": "", "message": "rate limit event: rejected",
                "when": "2026-10-08T13:31:00+02:00", "retry_after": "2026-10-08T18:00:00+02:00",
                "until": "2026-10-08T18:00:00+02:00"
            } }
        });
        let pq = plan_quota("zai", &routes(), std::slice::from_ref(&marked), now());
        assert!(pq.quota_known);
        assert_eq!(pq.label, "ZAI-b");
        let v = plan_quota_verdict(available_verdict("ZAI"), "zai", FP_A, "ZAI", &pq, true);
        assert_eq!(
            v.reason,
            "plan zai (usage limit on ZAI-b until 2026-10-08T18:00:00+02:00)"
        );
        let h = crate::health::endpoint_health(std::slice::from_ref(&marked), FP_B, now());
        assert!(h.quota.is_some() && h.recent_usable.is_some());
        let cleared = vec![marked, usable(FP_B, "2026-10-08T13:40:00+02:00", 4)];
        assert!(plan_quota("zai", &routes(), &cleared, now())
            .quota
            .is_none());
    }

    fn row(label: &str, pid: u32, plan: &str, endpoint: &str) -> MachineRunning {
        MachineRunning {
            endpoint: endpoint.into(),
            label: label.into(),
            pid,
            start_time: String::new(),
            repo: "C:\\r\\e".into(),
            task: "t".into(),
            nn: "01".into(),
            panel: String::new(),
            since: String::new(),
            plan: plan.into(),
        }
    }

    #[test]
    fn the_wait_line_names_the_endpoint_or_the_plan() {
        let by_plan = vec![row("ZAI", 4242, "zai", FP_A)];
        assert_eq!(
            format_machine_wait(1, 2, &by_plan, &by_plan, 1),
            "  panel member 1 of 2 waits: 1 run(s) elsewhere on this machine use its plan zai (parallel limit 1): ZAI in C:\\r\\e task t handoff 01 (plan zai, pid 4242)"
        );
        let ep = vec![row("ZAI", 7, "zai", FP_A)];
        assert_eq!(
            format_machine_wait(2, 3, &ep, &[], 1),
            "  panel member 2 of 3 waits: 1 run(s) elsewhere on this machine use its endpoint (parallel limit 1): ZAI in C:\\r\\e task t handoff 01 (pid 7)"
        );
        let both = vec![row("ZAI", 7, "", FP_A), row("ZAI-b", 8, "zai", FP_B)];
        assert_eq!(
            format_machine_wait(1, 1, &both, &both[1..], 2),
            "  panel member 1 of 1 waits: 2 run(s) elsewhere on this machine use its endpoint or its plan zai (parallel limit 2): ZAI in C:\\r\\e task t handoff 01 (pid 7); ZAI-b in C:\\r\\e task t handoff 01 (plan zai, pid 8)"
        );
    }
}
