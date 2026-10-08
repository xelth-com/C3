//! The panel plan (wave 26): default size, lab derivation, endpoint groups + the concurrency
//! plan, required-reviewer resolution and the seeded seat selection (`Select-PanelRouting`).
//! A port of `Get-PanelDefaultSize`, `Get-ModelLab`/`Get-EntryLab`, `Get-EndpointGroups`,
//! `Get-PanelPlan`, `Resolve-RequiredReviewers` and `Select-PanelRouting`.

use c3_core::lineage::format_reviewer_lineage;
use c3_core::roster::{resolve_reviewer_matcher, Roster, RosterEntry};
use chrono::{DateTime, Utc};

use crate::router::{self, PriorUse, RouterContext};

use super::routing::{
    self, invoke_panel_draw, panel_seed, Candidate, Rating, RoutingScore, ROUTING_EXPLORE,
    ROUTING_MIN_RATINGS, ROUTING_NEUTRAL,
};

/// Purposes on which a framing/decision panel warns below two seats.
const PANEL_FLOOR_PURPOSES: &[&str] = &["framing", "decision"];

/// The vendor prefix table (`$script:LabVendors`), longest-match-by-order as the plugin walks it.
const LAB_VENDORS: &[(&str, &str)] = &[
    ("qwen", "alibaba"),
    ("deepseek", "deepseek"),
    ("kimi", "moonshot"),
    ("k3", "moonshot"),
    ("glm", "zhipu"),
    ("dola", "bytedance"),
    ("seed", "bytedance"),
    ("mimo", "xiaomi"),
    ("gemini", "google"),
    ("muse", "meta"),
    ("gpt", "openai"),
];

/// `Get-PanelDefaultSize` (D6): the default seats for a purpose (`0` = every eligible member).
pub fn default_size(purpose: &str) -> i32 {
    match purpose {
        "" | "chore" | "checkpoint" => 1,
        "diff-review" => 2,
        "framing" | "decision" => 3,
        "core-contract" | "acceptance" => 4,
        "stuck" => 0,
        _ => 1,
    }
}

/// `Get-ModelLab`: the lab a model id belongs to by its vendor prefix; `""` when unknown.
pub fn model_lab(model: &str) -> String {
    let m = model.trim().to_lowercase();
    if m.is_empty() {
        return String::new();
    }
    for (prefix, lab) in LAB_VENDORS {
        if m.starts_with(prefix) {
            return (*lab).to_string();
        }
    }
    String::new()
}

/// `roster` | `vendor` | `singleton`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LabSource {
    Roster,
    Vendor,
    Singleton,
}

impl LabSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            LabSource::Roster => "roster",
            LabSource::Vendor => "vendor",
            LabSource::Singleton => "singleton",
        }
    }
}

/// `Get-EntryLab` (D1): the entry's declared `lab`, else the vendor table on its resolved model,
/// else a singleton lab (its lineage, lowercased). `model`/`lineage` are the resolved values.
pub fn entry_lab(declared_lab: &str, model: &str, lineage: &str) -> (String, LabSource) {
    if !declared_lab.is_empty() {
        return (declared_lab.to_string(), LabSource::Roster);
    }
    let v = model_lab(model);
    if !v.is_empty() {
        return (v, LabSource::Vendor);
    }
    (lineage.to_lowercase(), LabSource::Singleton)
}

// ------------------------------------------------------------------ endpoint groups + concurrency

/// One endpoint group in the concurrency plan.
#[derive(Debug, Clone)]
pub struct PlanGroup {
    pub labels: Vec<String>,
    pub limit: i64,
    pub positions: Vec<i64>,
}

/// `Get-PanelPlan`'s result.
#[derive(Debug, Clone)]
pub struct PanelPlan {
    pub groups: Vec<PlanGroup>,
    /// roster position -> group index.
    pub group_of: Vec<(i64, usize)>,
    pub cap: i64,
    /// The most members that can run at once.
    pub effective: i64,
    /// `at once` | `one after another` | `at most <k> at a time`.
    pub text: String,
    /// Ordered label -> its group's limit, roster order.
    pub limits: Vec<(String, i64)>,
}

/// A runner for the endpoint-group computation: its roster label (provider), position,
/// endpoint fingerprint (empty when the identity is unresolved) and (wave 1b, 0.6.0 E7) its
/// roster entry's plan (empty without one).
#[derive(Debug, Clone, Default)]
pub struct Runner {
    pub position: i64,
    pub label: String,
    pub fingerprint: String,
    pub plan: String,
}

struct EndpointGroups {
    labels: Vec<String>,
    groups: Vec<(Vec<String>, Vec<i64>)>, // (labels, positions)
    group_of: Vec<(i64, usize)>,
    index_of_label: Vec<(String, usize)>,
}

/// `Get-EndpointGroups -Scheduling`: one group per provider label, merged with any label sharing
/// its provider fingerprint, to a fixed point. Groups are numbered in first-member order.
/// (wave 29b, E7) A runner with a roster `plan` adds the key `plan:<slug>`, so the members of one
/// plan share a group across engines and routes (the availability view does not group by plan:
/// only a quota failure propagates over a plan, E5).
fn endpoint_groups(runners: &[Runner]) -> EndpointGroups {
    let mut labels: Vec<String> = Vec::new();
    // label -> set of fingerprints.
    let mut fps: Vec<(String, Vec<String>)> = Vec::new();
    for m in runners {
        if !labels.contains(&m.label) {
            labels.push(m.label.clone());
            fps.push((m.label.clone(), Vec::new()));
        }
        if !m.fingerprint.is_empty() {
            let e = fps.iter_mut().find(|(l, _)| *l == m.label).unwrap();
            if !e.1.contains(&m.fingerprint) {
                e.1.push(m.fingerprint.clone());
            }
        }
        if !m.plan.is_empty() {
            let key = format!("plan:{}", m.plan);
            let e = fps.iter_mut().find(|(l, _)| *l == m.label).unwrap();
            if !e.1.contains(&key) {
                e.1.push(key);
            }
        }
    }
    let fp_of = |label: &str| -> &[String] { &fps.iter().find(|(l, _)| l == label).unwrap().1 };
    // group id per label, initially its own index; merge labels sharing a fingerprint.
    let mut group_of_label: Vec<usize> = (0..labels.len()).collect();
    let mut changed = true;
    while changed {
        changed = false;
        for i in 0..labels.len() {
            for j in (i + 1)..labels.len() {
                if group_of_label[i] == group_of_label[j] {
                    continue;
                }
                let shared = fp_of(&labels[i])
                    .iter()
                    .any(|f| fp_of(&labels[j]).contains(f));
                if !shared {
                    continue;
                }
                let keep = group_of_label[i].min(group_of_label[j]);
                let drop = group_of_label[i].max(group_of_label[j]);
                for g in group_of_label.iter_mut() {
                    if *g == drop {
                        *g = keep;
                    }
                }
                changed = true;
            }
        }
    }
    // Build groups in first-member order.
    let mut groups: Vec<(Vec<String>, Vec<i64>)> = Vec::new();
    let mut index_of: Vec<(usize, usize)> = Vec::new(); // group-id -> groups index
    let mut group_of: Vec<(i64, usize)> = Vec::new();
    for m in runners {
        let li = labels.iter().position(|l| *l == m.label).unwrap();
        let gid = group_of_label[li];
        let gi = match index_of.iter().find(|(g, _)| *g == gid) {
            Some((_, idx)) => *idx,
            None => {
                let idx = groups.len();
                index_of.push((gid, idx));
                groups.push((Vec::new(), Vec::new()));
                idx
            }
        };
        if !groups[gi].0.contains(&m.label) {
            groups[gi].0.push(m.label.clone());
        }
        groups[gi].1.push(m.position);
        group_of.push((m.position, gi));
    }
    let mut index_of_label: Vec<(String, usize)> = Vec::new();
    for l in &labels {
        let li = labels.iter().position(|x| x == l).unwrap();
        let gid = group_of_label[li];
        let gi = index_of.iter().find(|(g, _)| *g == gid).unwrap().1;
        index_of_label.push((l.clone(), gi));
    }
    EndpointGroups {
        labels,
        groups,
        group_of,
        index_of_label,
    }
}

/// `Get-PanelPlan`: the concurrency plan. `parallel` is the roster's `parallel` map (label or
/// plan -> n); `cap` is `-PanelConcurrency` (0 = no cap). (wave 29b, E7) A group that holds
/// members of a plan is limited by the plan too: the plan's own `parallel` value (default 1)
/// caps it, and a label without a `parallel` value of its own takes its plans' (the smallest) -
/// so `{"parallel": {"zai": 2}}` lets two routes of plan zai run at once, and without it they
/// run one after another.
pub fn panel_plan(runners: &[Runner], parallel: &[(String, i64)], cap: i64) -> PanelPlan {
    let eg = endpoint_groups(runners);
    let mut effective: i64 = 0;
    let mut out: Vec<PlanGroup> = Vec::new();
    let parallel_of =
        |k: &str| -> Option<i64> { parallel.iter().find(|(pl, _)| pl == k).map(|(_, n)| *n) };
    // (E7) the plans of each label among the runners, first-seen order
    let mut plans_of: Vec<(String, Vec<String>)> = Vec::new();
    for m in runners {
        let idx = match plans_of.iter().position(|(l, _)| *l == m.label) {
            Some(i) => i,
            None => {
                plans_of.push((m.label.clone(), Vec::new()));
                plans_of.len() - 1
            }
        };
        if !m.plan.is_empty() && !plans_of[idx].1.contains(&m.plan) {
            plans_of[idx].1.push(m.plan.clone());
        }
    }
    let plan_limit = |p: &str| -> i64 { parallel_of(p).unwrap_or(1) };
    for (labels, positions) in &eg.groups {
        let mut limit: i64 = 0;
        for l in labels {
            let l_plans: &[String] = plans_of
                .iter()
                .find(|(x, _)| x == l)
                .map(|(_, v)| v.as_slice())
                .unwrap_or(&[]);
            let v = match parallel_of(l) {
                Some(n) => n,
                None if !l_plans.is_empty() => {
                    l_plans.iter().map(|p| plan_limit(p)).min().unwrap_or(1)
                }
                None => 1,
            };
            if limit == 0 || v < limit {
                limit = v;
            }
            for p in l_plans {
                let pv = plan_limit(p);
                if pv < limit {
                    limit = pv;
                }
            }
        }
        effective += limit.min(positions.len() as i64);
        out.push(PlanGroup {
            labels: labels.clone(),
            limit,
            positions: positions.clone(),
        });
    }
    let mut limits: Vec<(String, i64)> = Vec::new();
    for l in &eg.labels {
        let gi = eg.index_of_label.iter().find(|(x, _)| x == l).unwrap().1;
        limits.push((l.clone(), out[gi].limit));
    }
    if cap > 0 && cap < effective {
        effective = cap;
    }
    let count = runners.len() as i64;
    let text = if effective >= count {
        "at once".to_string()
    } else if effective <= 1 {
        "one after another".to_string()
    } else {
        format!("at most {effective} at a time")
    };
    PanelPlan {
        groups: out,
        group_of: eg.group_of,
        cap,
        effective,
        text,
        limits,
    }
}

// ---------------------------------------------------------------------- required reviewers (D7)

/// `Resolve-RequiredReviewers`' result.
#[derive(Debug, Clone, Default)]
pub struct RequiredReviewers {
    /// Roster positions, sorted, unique.
    pub positions: Vec<i64>,
    pub matchers: Vec<String>,
    /// `""` | `-Require` | `-Require none` | `roster require.<purpose>`.
    pub source: String,
    pub error: String,
}

/// `Resolve-RequiredReviewers` (D7): `-Require`'s matchers (`explicit`; `none` alone drops the
/// requirement), else the roster's `require` for `purpose` (a panel run only, `use_roster`).
pub fn resolve_required_reviewers(
    roster: &Roster,
    require: &[String],
    purpose: &str,
    explicit: bool,
    use_roster: bool,
) -> RequiredReviewers {
    let mut r = RequiredReviewers::default();
    let mut matchers: Vec<String> = Vec::new();
    if explicit {
        for v in require {
            for p in v.split(',') {
                let t = p.trim();
                if !t.is_empty() {
                    matchers.push(t.to_string());
                }
            }
        }
        if matchers.len() == 1 && matchers[0].eq_ignore_ascii_case("none") {
            r.source = "-Require none".into();
            return r;
        }
        if matchers.iter().any(|m| m.eq_ignore_ascii_case("none")) {
            r.error = "-Require none stands alone (it drops the roster's requirement); do not combine it with reviewers".into();
            return r;
        }
        r.source = "-Require".into();
    } else if use_roster {
        if let Some((_, ms)) = roster.require.iter().find(|(p, _)| p == purpose) {
            for m in ms {
                matchers.push(m.clone());
            }
            r.source = format!("roster require.{purpose}");
        }
    }
    if matchers.is_empty() {
        return r;
    }
    if !roster.exists {
        r.error = "-Require names reviewers of the roster, and there is no reviewer roster".into();
        return r;
    }
    let mut set: Vec<i64> = Vec::new();
    for m in &matchers {
        let (positions, err) = resolve_reviewer_matcher(&roster.entries, m);
        if !err.is_empty() {
            r.error = format!("{}: {}", r.source, err);
            return r;
        }
        for p in positions {
            if !set.contains(&p) {
                set.push(p);
            }
        }
    }
    set.sort_unstable();
    r.positions = set;
    r.matchers = matchers;
    r
}

// ---------------------------------------------------------------------- seat selection (routing)

/// A member fed to `select_panel_routing` — `Select-PanelMembers`' resolved output.
#[derive(Debug, Clone)]
pub struct MemberInput {
    pub entry: RosterEntry,
    /// The resolved identity's provider/model/engine (may differ from the entry's — config model).
    pub provider: String,
    pub model: String,
    pub engine: String,
    /// `run` | `skipped` (with a reason) at input; `select_panel_routing` may add `not-picked`.
    pub state: String,
    pub reason: String,
    /// The `Select-PanelMembers` skip kind (`weighty` / `light` matter: a required entry only
    /// that gate held back rejoins).
    pub skip_kind: String,
}

/// A member after seating.
#[derive(Debug, Clone)]
pub struct SeatedMember {
    pub position: i64,
    pub lineage: String,
    pub lab: String,
    pub lab_source: LabSource,
    pub score: RoutingScore,
    pub required: bool,
    pub state: String,
    pub reason: String,
    pub rule: String,
    pub slot: i32,
}

/// The routing record (the ledger's `panel.routing`).
#[derive(Debug, Clone, Default)]
pub struct RoutingRecord {
    pub mode: String,
    pub order: String,
    pub fallback: String,
    pub seed: String,
    pub nonce: String,
    pub nonce_source: String,
    pub size: i64,
    pub size_asked: i64,
    pub size_source: String,
    pub reserve: i64,
    pub eligible: Vec<EligibleRow>,
    pub picked: Vec<PickedRow>,
    pub explored: Vec<String>,
    pub required: Vec<String>,
    /// (C3, M9) the content of the trailing `ext` key: `{ c3: { router: {...} } }` when the
    /// router used priors or non-default parameters; `None` writes no key, so the record stays
    /// byte-identical to the plugin's.
    pub ext: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct EligibleRow {
    pub position: i64,
    pub lineage: String,
    pub lab: String,
    pub lab_source: String,
    pub score: f64,
    pub basis: String,
    pub ratings: f64,
    pub required: bool,
}

#[derive(Debug, Clone)]
pub struct PickedRow {
    pub slot: i32,
    pub position: i64,
    pub lineage: String,
    pub lab: String,
    pub rule: String,
}

/// `Select-PanelRouting`'s result.
pub struct PanelRoutingResult {
    /// Every member in roster order (state updated: `not-picked` for eligible-but-unseated).
    pub members: Vec<SeatedMember>,
    /// The seated members, in seat order.
    pub picked: Vec<SeatedMember>,
    pub k: i64,
    pub size_asked: i64,
    pub routing: RoutingRecord,
    pub warnings: Vec<String>,
}

/// Round to `d` decimals (banker's rounding matches PowerShell's `[Math]::Round`).
fn round(x: f64, d: i32) -> f64 {
    let f = 10f64.powi(d);
    (x * f).round() / f
}

/// `Select-PanelRouting` (D1, D4-D7) with the plugin's weights: no priors, default parameters.
#[allow(clippy::too_many_arguments)]
pub fn select_panel_routing(
    members: &[MemberInput],
    size: i32,
    size_source: &str,
    order: &str,
    ratings: &[Rating],
    purpose: &str,
    topics: &[String],
    utc_now: DateTime<Utc>,
    task: &str,
    brief_sha: &str,
    nonce: &str,
    nonce_source: &str,
    required: &[i64],
) -> PanelRoutingResult {
    select_panel_routing_with(
        &RouterContext::default(),
        members,
        size,
        size_source,
        order,
        ratings,
        purpose,
        topics,
        utc_now,
        task,
        brief_sha,
        nonce,
        nonce_source,
        required,
    )
}

/// `Select-PanelRouting` with the weight source of router v1 (M9): the same draw, the score
/// from [`router::score`]. With no priors in `ctx` and default parameters every number and
/// every byte of the record equals [`select_panel_routing`]'s.
#[allow(clippy::too_many_arguments)]
pub fn select_panel_routing_with(
    ctx: &RouterContext,
    members: &[MemberInput],
    size: i32,
    size_source: &str,
    order: &str,
    ratings: &[Rating],
    purpose: &str,
    topics: &[String],
    utc_now: DateTime<Utc>,
    task: &str,
    brief_sha: &str,
    nonce: &str,
    nonce_source: &str,
    required: &[i64],
) -> PanelRoutingResult {
    let neutral = ROUTING_NEUTRAL;

    // Enrich each member: a required member that only the weighty gate (0.6.0: or the light gate)
    // held back rejoins as `run`.
    let mut seated: Vec<SeatedMember> = Vec::new();
    let mut prior_uses: Vec<PriorUse> = Vec::new();
    for m in members {
        let mut state = m.state.clone();
        let mut reason = m.reason.clone();
        let is_req = required.contains(&(m.entry.position as i64));
        if is_req && state == "skipped" && (m.skip_kind == "weighty" || m.skip_kind == "light") {
            state = "run".into();
            reason = String::new();
        }
        let engine = if !m.engine.is_empty() {
            m.engine.clone()
        } else if !m.entry.engine.is_empty() {
            m.entry.engine.clone()
        } else {
            "codex".into()
        };
        let lineage = format_reviewer_lineage(&m.provider, &m.model, &engine);
        let (lab, lab_source) = entry_lab(&m.entry.lab, &m.model, &lineage);
        let scored = router::score(
            ratings,
            router::Lineage {
                provider: &m.provider,
                model: &m.model,
                engine: &engine,
            },
            purpose,
            topics,
            utc_now,
            ctx,
        );
        if let Some(mut u) = scored.prior {
            u.position = m.entry.position as i64;
            prior_uses.push(u);
        }
        let score = scored.base;
        seated.push(SeatedMember {
            position: m.entry.position as i64,
            lineage,
            lab,
            lab_source,
            score,
            required: is_req,
            state,
            reason,
            rule: String::new(),
            slot: 0,
        });
    }

    // Eligible = state 'run'.
    let eligible_idx: Vec<usize> = seated
        .iter()
        .enumerate()
        .filter(|(_, m)| m.state == "run")
        .map(|(i, _)| i)
        .collect();
    let eligible_count = eligible_idx.len() as i64;

    let mut k: i64 = size as i64;
    if k <= 0 {
        k = eligible_count;
    }
    let size_asked = k;
    let mut size_source = size_source.to_string();
    let pinned_count = eligible_idx.iter().filter(|&&i| seated[i].required).count() as i64;
    // (wave 26c) the required reviewers raise the size when they outnumber the asked size; the
    // source becomes `required` and a warning is emitted.
    let mut size_raised = false;
    if k < pinned_count {
        k = pinned_count;
        size_raised = true;
        size_source = "required".to_string();
    }
    if k > eligible_count {
        k = eligible_count;
    }

    // The record names the priors of the eligible only.
    prior_uses.retain(|u| {
        eligible_idx
            .iter()
            .any(|&i| seated[i].position == u.position)
    });

    let mut mode = order.to_string();
    let mut fallback = String::new();
    // (M9) a published prior cell is evidence too: a fresh installation with priors routes
    // instead of falling back to the roster order. Without priors this is the plugin's rule.
    let has_prior_cell = prior_uses.iter().any(|u| u.prior_basis != "none");
    if mode == "routed"
        && !has_prior_cell
        && !eligible_idx
            .iter()
            .any(|&i| seated[i].score.all >= ROUTING_MIN_RATINGS as i64)
    {
        mode = "roster".into();
        fallback = "no ratings".into();
    }

    let lineages: Vec<String> = eligible_idx
        .iter()
        .map(|&i| seated[i].lineage.clone())
        .collect();
    let (seed_bytes, seed_hex, _) = panel_seed(task, purpose, brief_sha, &lineages, nonce);

    // Seats: (member index into `seated`, rule).
    let mut seats: Vec<(usize, i32, String)> = Vec::new();
    if mode == "routed" {
        let cands: Vec<Candidate> = eligible_idx
            .iter()
            .map(|&i| Candidate {
                position: seated[i].position,
                lab: seated[i].lab.clone(),
                weight: seated[i].score.score,
                pinned: seated[i].required,
            })
            .collect();
        for s in invoke_panel_draw(&cands, k as usize, &seed_bytes, neutral, ROUTING_EXPLORE) {
            let mi = seated
                .iter()
                .position(|m| m.position == s.position)
                .unwrap();
            seats.push((mi, s.slot, s.rule));
        }
    } else {
        // roster order: required first, then the rest, up to k.
        let mut chosen: Vec<usize> = Vec::new();
        for &i in &eligible_idx {
            if seated[i].required {
                chosen.push(i);
            }
        }
        for &i in &eligible_idx {
            if chosen.len() as i64 >= k {
                break;
            }
            if !seated[i].required {
                chosen.push(i);
            }
        }
        let mut slot = 0;
        for i in chosen {
            slot += 1;
            let rule = if seated[i].required {
                "required"
            } else {
                "roster"
            }
            .to_string();
            seats.push((i, slot, rule));
        }
    }

    // Stamp rule/slot; mark unseated eligible as not-picked.
    for (mi, slot, rule) in &seats {
        seated[*mi].rule = rule.clone();
        seated[*mi].slot = *slot;
    }
    for &i in &eligible_idx {
        if seated[i].slot <= 0 {
            seated[i].state = "not-picked".into();
            seated[i].reason = format!("panel size {k}");
        }
    }

    // Warnings.
    let mut warnings: Vec<String> = Vec::new();
    if size_raised {
        warnings.push(format!(
            "panel size raised: asked {size_asked}, required {pinned_count}"
        ));
    }
    if k < size_asked {
        warnings.push(format!(
            "panel size reduced: asked {size_asked}, eligible {eligible_count}"
        ));
    }
    if mode == "routed" {
        for &i in &eligible_idx {
            if seated[i].lab_source == LabSource::Singleton {
                warnings.push(format!(
                    "routing: no lab known for #{} {} (its model id has no known vendor prefix) - it counts as a lab of its own; give its roster entry a \"lab\"",
                    seated[i].position, seated[i].lineage
                ));
            }
        }
    }
    if size_source != "-PanelSize"
        && PANEL_FLOOR_PURPOSES.contains(&purpose)
        && (seats.len() as i64) < 2
    {
        let n = seats.len();
        warnings.push(format!(
            "panel floor: a {purpose} panel runs {n} member{} - {purpose} questions should hear at least 2 reviewers (pass -PanelSize to choose the size explicitly)",
            if n != 1 { "s" } else { "" }
        ));
    }

    let reserve = seats
        .iter()
        .filter(|(_, _, r)| r.starts_with("lab-"))
        .count() as i64;
    let routing = RoutingRecord {
        mode: mode.clone(),
        order: order.to_string(),
        fallback,
        seed: seed_hex,
        nonce: nonce.to_string(),
        nonce_source: nonce_source.to_string(),
        size: k,
        size_asked,
        size_source: size_source.to_string(),
        reserve,
        eligible: eligible_idx
            .iter()
            .map(|&i| EligibleRow {
                position: seated[i].position,
                lineage: seated[i].lineage.clone(),
                lab: seated[i].lab.clone(),
                lab_source: seated[i].lab_source.as_str().to_string(),
                score: round(seated[i].score.score, 4),
                basis: seated[i].score.basis.clone(),
                ratings: round(seated[i].score.ratings, 2),
                required: seated[i].required,
            })
            .collect(),
        picked: seats
            .iter()
            .map(|(mi, slot, rule)| PickedRow {
                slot: *slot,
                position: seated[*mi].position,
                lineage: seated[*mi].lineage.clone(),
                lab: seated[*mi].lab.clone(),
                rule: rule.clone(),
            })
            .collect(),
        explored: seats
            .iter()
            .filter(|(_, _, r)| r.ends_with("-explore"))
            .map(|(mi, _, _)| seated[*mi].lineage.clone())
            .collect(),
        required: eligible_idx
            .iter()
            .filter(|&&i| seated[i].required)
            .map(|&i| seated[i].lineage.clone())
            .collect(),
        ext: router::routing_ext(ctx, &prior_uses),
    };

    let picked: Vec<SeatedMember> = seats.iter().map(|(mi, _, _)| seated[*mi].clone()).collect();

    PanelRoutingResult {
        members: seated,
        picked,
        k,
        size_asked,
        routing,
        warnings,
    }
}

// re-export the neutral/explore constants used by callers/tests.
pub use routing::{ROUTING_EXPLORE as EXPLORE, ROUTING_NEUTRAL as NEUTRAL};

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(position: usize, provider: &str, model: &str) -> RosterEntry {
        RosterEntry {
            position,
            provider: provider.into(),
            model: model.into(),
            engine: "codex".into(),
            ..Default::default()
        }
    }

    fn member(position: usize, provider: &str, model: &str, state: &str) -> MemberInput {
        MemberInput {
            entry: entry(position, provider, model),
            provider: provider.into(),
            model: model.into(),
            engine: "codex".into(),
            state: state.into(),
            reason: String::new(),
            skip_kind: String::new(),
        }
    }

    #[test]
    fn default_sizes_match_the_r14_table() {
        assert_eq!(default_size(""), 1);
        assert_eq!(default_size("chore"), 1);
        assert_eq!(default_size("checkpoint"), 1);
        assert_eq!(default_size("diff-review"), 2);
        assert_eq!(default_size("framing"), 3);
        assert_eq!(default_size("decision"), 3);
        assert_eq!(default_size("core-contract"), 4);
        assert_eq!(default_size("acceptance"), 4);
        assert_eq!(default_size("stuck"), 0);
        assert_eq!(default_size("unknown-purpose"), 1);
    }

    #[test]
    fn model_lab_uses_the_vendor_prefix_table() {
        assert_eq!(model_lab("glm-5.3"), "zhipu");
        assert_eq!(model_lab("gpt-6-astra"), "openai");
        assert_eq!(model_lab("gemini-3-pro"), "google");
        assert_eq!(model_lab("muse-spark"), "meta");
        assert_eq!(model_lab("kimi-k2"), "moonshot");
        assert_eq!(model_lab("mystery-1"), "");
    }

    #[test]
    fn entry_lab_prefers_declared_then_vendor_then_singleton() {
        let (l, s) = entry_lab("customlab", "glm-5.3", "zai :: glm-5.3");
        assert_eq!((l.as_str(), s), ("customlab", LabSource::Roster));
        let (l, s) = entry_lab("", "glm-5.3", "zai :: glm-5.3");
        assert_eq!((l.as_str(), s), ("zhipu", LabSource::Vendor));
        let (l, s) = entry_lab("", "mystery", "lab :: mystery");
        assert_eq!((l.as_str(), s), ("lab :: mystery", LabSource::Singleton));
    }

    #[test]
    fn endpoint_groups_merge_shared_fingerprints() {
        // Two labels sharing a fingerprint merge into one group; a distinct one stays apart.
        let runners = vec![
            Runner {
                position: 1,
                label: "agy1".into(),
                fingerprint: "google".into(),
                plan: String::new(),
            },
            Runner {
                position: 2,
                label: "agy2".into(),
                fingerprint: "google".into(),
                plan: String::new(),
            },
            Runner {
                position: 3,
                label: "zai".into(),
                fingerprint: "zaifp".into(),
                plan: String::new(),
            },
        ];
        let plan = panel_plan(&runners, &[], 0);
        assert_eq!(plan.groups.len(), 2);
        // agy1+agy2 share a group.
        assert!(plan.groups[0].labels.contains(&"agy1".to_string()));
        assert!(plan.groups[0].labels.contains(&"agy2".to_string()));
        assert_eq!(plan.groups[0].positions, vec![1, 2]);
    }

    #[test]
    fn concurrency_respects_parallel_and_cap() {
        let runners = vec![
            Runner {
                position: 1,
                label: "zai".into(),
                fingerprint: "a".into(),
                plan: String::new(),
            },
            Runner {
                position: 2,
                label: "byteplus".into(),
                fingerprint: "b".into(),
                plan: String::new(),
            },
            Runner {
                position: 3,
                label: "byteplus".into(),
                fingerprint: "b".into(),
                plan: String::new(),
            },
            Runner {
                position: 4,
                label: "byteplus".into(),
                fingerprint: "b".into(),
                plan: String::new(),
            },
        ];
        // byteplus raised to 3 parallel; zai default 1 => effective = 1 + min(3,3) = 4.
        let plan = panel_plan(&runners, &[("byteplus".into(), 3)], 0);
        assert_eq!(plan.effective, 4);
        assert_eq!(plan.text, "at once");
        // Cap of 2 dominates.
        let plan = panel_plan(&runners, &[("byteplus".into(), 3)], 2);
        assert_eq!(plan.effective, 2);
        assert_eq!(plan.text, "at most 2 at a time");
        // No parallel: byteplus is one-at-a-time => effective = 1 (zai) + 1 (byteplus) = 2.
        let plan = panel_plan(&runners, &[], 0);
        assert_eq!(plan.effective, 2);
    }

    fn runner(position: i64, label: &str, fingerprint: &str, plan: &str) -> Runner {
        Runner {
            position,
            label: label.into(),
            fingerprint: fingerprint.into(),
            plan: plan.into(),
        }
    }

    #[test]
    fn a_plan_is_one_scheduling_group_across_routes() {
        // (wave 29b, E7 - harness-claude's "E7: every plan is a scheduling group across engines",
        // with two routes C3 runs) a ZAI member and a ZAI-b member of plan zai on two endpoints
        // share one group beside an openai member: "at most 2 at a time".
        let runners = vec![
            runner(1, "ZAI", "fp-zai", "zai"),
            runner(2, "ZAI-b", "fp-zai-b", "zai"),
            runner(3, "openai", "fp-openai", ""),
        ];
        let plan = panel_plan(&runners, &[], 0);
        assert_eq!(plan.groups.len(), 2);
        assert_eq!(plan.groups[0].positions, vec![1, 2]);
        assert_eq!(plan.groups[0].limit, 1);
        assert_eq!(plan.text, "at most 2 at a time");
        // "parallel": {"zai": 2} -> at once (both labels take their plan's value)
        let plan = panel_plan(&runners, &[("zai".into(), 2)], 0);
        assert_eq!(plan.groups[0].limit, 2);
        assert_eq!(plan.text, "at once");
        // raising both labels but not the plan keeps them one after another
        let plan = panel_plan(&runners, &[("ZAI".into(), 2), ("ZAI-b".into(), 2)], 0);
        assert_eq!(plan.groups[0].limit, 1);
        assert_eq!(plan.text, "at most 2 at a time");
        // a label's own value below the plan's wins
        let plan = panel_plan(&runners, &[("zai".into(), 3), ("ZAI".into(), 1)], 0);
        assert_eq!(plan.groups[0].limit, 1);
        // without the plan they run at once (two endpoints)
        let no_plan = vec![
            runner(1, "ZAI", "fp-zai", ""),
            runner(2, "ZAI-b", "fp-zai-b", ""),
            runner(3, "openai", "fp-openai", ""),
        ];
        let plan = panel_plan(&no_plan, &[], 0);
        assert_eq!(plan.groups.len(), 3);
        assert_eq!(plan.text, "at once");
        // two plans stay apart
        let two = vec![
            runner(1, "ZAI", "fp-zai", "zai"),
            runner(2, "mimo", "fp-mimo", "mimo"),
        ];
        assert_eq!(panel_plan(&two, &[], 0).text, "at once");
    }

    #[test]
    fn required_none_drops_the_requirement() {
        let roster = Roster {
            exists: true,
            ..Default::default()
        };
        let r = resolve_required_reviewers(&roster, &["none".into()], "framing", true, false);
        assert_eq!(r.source, "-Require none");
        assert!(r.positions.is_empty());
        assert!(r.error.is_empty());
    }

    #[test]
    fn required_none_cannot_combine() {
        let roster = Roster {
            exists: true,
            ..Default::default()
        };
        let r = resolve_required_reviewers(
            &roster,
            &["#1".into(), "none".into()],
            "framing",
            true,
            false,
        );
        assert!(r.error.contains("stands alone"));
    }

    #[test]
    fn required_resolves_positions_from_matchers() {
        let roster = Roster {
            exists: true,
            entries: vec![entry(1, "openai", "gpt-6"), entry(2, "zai", "glm-5.3")],
            ..Default::default()
        };
        let r = resolve_required_reviewers(&roster, &["#2".into()], "framing", true, false);
        assert_eq!(r.positions, vec![2]);
        assert_eq!(r.source, "-Require");
    }

    #[test]
    fn roster_order_seats_required_first_then_roster() {
        let members = vec![
            member(1, "openai", "gpt-6", "run"),
            member(2, "zai", "glm-5.3", "run"),
            member(3, "google", "gemini-3", "run"),
        ];
        let res = select_panel_routing(
            &members,
            2,
            "-PanelSize",
            "roster",
            &[],
            "framing",
            &[],
            Utc::now(),
            "task",
            "sha",
            "nonce",
            "-PanelSeed",
            &[3],
        );
        // required #3 takes seat 1, then roster order fills seat 2 with #1.
        assert_eq!(res.picked.len(), 2);
        assert_eq!(res.picked[0].position, 3);
        assert_eq!(res.picked[0].rule, "required");
        assert_eq!(res.picked[1].position, 1);
        assert_eq!(res.picked[1].rule, "roster");
        // #2 is eligible but not picked.
        let m2 = res.members.iter().find(|m| m.position == 2).unwrap();
        assert_eq!(m2.state, "not-picked");
        assert_eq!(m2.reason, "panel size 2");
    }

    #[test]
    fn routed_falls_back_to_roster_without_ratings() {
        let members = vec![
            member(1, "openai", "gpt-6", "run"),
            member(2, "zai", "glm-5.3", "run"),
        ];
        let res = select_panel_routing(
            &members,
            2,
            "-PanelSize",
            "routed",
            &[],
            "framing",
            &[],
            Utc::now(),
            "task",
            "sha",
            "nonce",
            "date",
            &[],
        );
        assert_eq!(res.routing.mode, "roster");
        assert_eq!(res.routing.fallback, "no ratings");
    }

    fn priors_ctx(cells: &str) -> RouterContext {
        let body = format!(
            r#"{{"priors_version":1,"generated":"2026-09-29T00:00:00Z","window_days":90,"cells":[{cells}]}}"#
        );
        RouterContext {
            params: Default::default(),
            priors: Some(crate::router::Priors::validate(body.as_bytes()).unwrap()),
            source: Default::default(),
        }
    }

    fn routed(ctx: &RouterContext, members: &[MemberInput]) -> PanelRoutingResult {
        select_panel_routing_with(
            ctx,
            members,
            2,
            "-PanelSize",
            "routed",
            &[],
            "framing",
            &[],
            Utc::now(),
            "task",
            "sha",
            "nonce",
            "date",
            &[],
        )
    }

    #[test]
    fn priors_route_a_fresh_installation_and_are_recorded() {
        let members = vec![
            member(1, "openai", "gpt-6", "run"),
            member(2, "zai", "glm-5.3", "run"),
            member(3, "google", "gemini-3", "run"),
        ];
        let ctx = priors_ctx(
            r#"{"provider":"zai","model":"glm-5.3","engine":"codex","purpose":"framing","topic":"","mean":0.9,"n":40}"#,
        );
        let res = routed(&ctx, &members);
        // A published cell is evidence: no roster fallback although nobody has a local mark.
        assert_eq!(res.routing.mode, "routed");
        assert_eq!(res.routing.fallback, "");
        let glm = res
            .routing
            .eligible
            .iter()
            .find(|e| e.position == 2)
            .unwrap();
        assert_eq!(glm.basis, "prior");
        assert!(glm.score > ROUTING_NEUTRAL);
        let other = res
            .routing
            .eligible
            .iter()
            .find(|e| e.position == 1)
            .unwrap();
        assert_eq!(other.basis, "neutral");
        assert_eq!(other.score, ROUTING_NEUTRAL);
        // The record says which priors shaped the draw, under ONE trailing object.
        let ext = res
            .routing
            .ext
            .expect("ext is written when priors are loaded");
        assert_eq!(ext["c3"]["router"]["router"], "v1");
        let uses = ext["c3"]["router"]["prior_use"].as_array().unwrap();
        assert_eq!(uses.len(), 3);
        assert_eq!(uses[1]["position"], 2);
        assert_eq!(uses[1]["prior_basis"], "lineage+purpose");
    }

    #[test]
    fn without_priors_the_record_is_the_plugins() {
        let members = vec![
            member(1, "openai", "gpt-6", "run"),
            member(2, "zai", "glm-5.3", "run"),
        ];
        let res = routed(&RouterContext::default(), &members);
        assert_eq!(res.routing.mode, "roster");
        assert_eq!(res.routing.fallback, "no ratings");
        assert!(res.routing.ext.is_none());
        for e in &res.routing.eligible {
            assert_eq!(e.score, ROUTING_NEUTRAL);
            assert_eq!(e.basis, "neutral");
        }
    }

    #[test]
    fn priors_without_a_cell_for_anyone_keep_the_roster_fallback() {
        let members = vec![
            member(1, "openai", "gpt-6", "run"),
            member(2, "zai", "glm-5.3", "run"),
        ];
        let ctx = priors_ctx(
            r#"{"provider":"xiaomi","model":"mimo-v2.6-pro","engine":"codex","purpose":"","topic":"","mean":0.8,"n":12}"#,
        );
        let res = routed(&ctx, &members);
        assert_eq!(res.routing.mode, "roster");
        assert_eq!(res.routing.fallback, "no ratings");
        // Priors were loaded, so the record still names them.
        assert!(res.routing.ext.is_some());
    }

    #[test]
    fn framing_below_two_seats_warns_unless_explicit_size() {
        // One eligible member, purpose framing, size from purpose default => floor warning.
        let members = vec![member(1, "openai", "gpt-6", "run")];
        let res = select_panel_routing(
            &members,
            3,
            "purpose",
            "roster",
            &[],
            "framing",
            &[],
            Utc::now(),
            "task",
            "sha",
            "nonce",
            "date",
            &[],
        );
        assert!(res.warnings.iter().any(|w| w.starts_with("panel floor:")));
        // Explicit -PanelSize suppresses the floor warning.
        let res = select_panel_routing(
            &members,
            1,
            "-PanelSize",
            "roster",
            &[],
            "framing",
            &[],
            Utc::now(),
            "task",
            "sha",
            "nonce",
            "date",
            &[],
        );
        assert!(!res.warnings.iter().any(|w| w.starts_with("panel floor:")));
    }
}
