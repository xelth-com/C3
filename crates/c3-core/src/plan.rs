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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credential::CredentialResult;
    use crate::lineage::ReviewerIdentity;
    use crate::verdict::verdict_with_credential;
    use chrono::TimeZone;
    use serde_json::json;

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
