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

/// The vendor prefix table (`$script:LabVendors`), first match in table order as the plugin walks
/// it. (0.2.5, wave C1) Every model family of the current roster resolves: `grok` (xAI) joined, and
/// the OpenAI `o<digit>` reasoning ids resolve in [`model_lab`] (a bare `o` prefix would catch
/// `opus`). (0.2.5 R4, F16-7 / F17-4) A prefix matches only at a boundary ([`prefix_at_boundary`]),
/// and every maker of the roster has a lab: MiniMax and Meta's Llama ids joined, so the prefix table
/// and [`c3_core::claude::lab_of_host`] name the same lab for every host that function knows.
const LAB_VENDORS: &[(&str, &str)] = &[
    ("qwen", "alibaba"),
    ("deepseek", "deepseek"),
    ("kimi", "moonshot"),
    ("k3", "moonshot"),
    ("glm", "zhipu"),
    ("dola", "bytedance"),
    ("seed", "bytedance"),
    ("mimo", "xiaomi"),
    ("minimax", "minimax"),
    ("gemini", "google"),
    ("muse", "meta"),
    ("meta-llama", "meta"),
    ("llama", "meta"),
    ("grok", "xai"),
    ("gpt", "openai"),
    // (0.6.0, wave 29) the claude engine's ids
    ("claude", "anthropic"),
];

/// (0.6.0, wave 29) The claude engine's model aliases: an id that IS one of them (a `[1m]` suffix
/// aside) is Anthropic's. (0.2.5 R4, F16-7) Never a prefix - `opus-custom` is not Anthropic's model
/// (only a `claude-` id is).
const LAB_ALIASES: &[(&str, &str)] = &[
    ("opus", "anthropic"),
    ("sonnet", "anthropic"),
    ("haiku", "anthropic"),
    ("fable", "anthropic"),
];

/// Whether `id` starts with `prefix` at a boundary: the id ends there, or the next character is
/// `-`, `_`, `.`, `/`, `:` or a digit (`qwen3`, `glm-5`, `k3.1`; not `seedream`, `gptx`).
fn prefix_at_boundary(id: &str, prefix: &str) -> bool {
    id.strip_prefix(prefix).is_some_and(|rest| {
        rest.chars()
            .next()
            .is_none_or(|c| matches!(c, '-' | '_' | '.' | '/' | ':') || c.is_ascii_digit())
    })
}

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
/// (0.2.5, wave C1) An OpenAI `o<digit>` id (`o3`, `o4-mini`) is openai; an id with a `/` that
/// does not resolve whole (an OpenRouter-style `vendor/model`) is tried by its last segment.
/// (0.2.5 R4) Case-folded, a `[1m]` suffix dropped; a prefix counts only at a boundary, an alias of
/// the claude engine only as the whole id.
pub fn model_lab(model: &str) -> String {
    let lower = model.trim().to_lowercase();
    let m = lower.strip_suffix("[1m]").unwrap_or(&lower).trim();
    if m.is_empty() {
        return String::new();
    }
    let by_prefix = |id: &str| -> Option<&'static str> {
        if let Some((_, lab)) = LAB_ALIASES.iter().find(|(a, _)| *a == id) {
            return Some(*lab);
        }
        for (prefix, lab) in LAB_VENDORS {
            if prefix_at_boundary(id, prefix) {
                return Some(*lab);
            }
        }
        let mut chars = id.chars();
        if chars.next() == Some('o') && chars.next().is_some_and(|c| c.is_ascii_digit()) {
            return Some("openai");
        }
        None
    };
    if let Some(lab) = by_prefix(m) {
        return lab.to_string();
    }
    if let Some((_, last)) = m.rsplit_once('/') {
        if let Some(lab) = by_prefix(last) {
            return lab.to_string();
        }
    }
    String::new()
}

/// (0.2.5 D3) The lab ONE token of a model id or a plan slug names, by the same table: the lab of
/// its vendor prefix ([`model_lab`]: `kimi`, `kimi-code` -> moonshot), else the token itself when it
/// is a lab of the table (`moonshot`, `alibaba`); `""` for anything else. `c3 maintain` asks it
/// whether a roster id's leading token names its maker or its plan's vendor.
pub fn lab_named_by(token: &str) -> String {
    let lab = model_lab(token);
    if !lab.is_empty() {
        return lab;
    }
    let t = token.trim().to_lowercase();
    let is_lab =
        LAB_VENDORS.iter().any(|(_, l)| *l == t) || LAB_ALIASES.iter().any(|(_, l)| *l == t);
    if is_lab && !t.is_empty() {
        t
    } else {
        String::new()
    }
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
/// (0.2.5 R4, F16-7) A declared lab is case-folded (`OpenAI` and `openai` are one lab).
pub fn entry_lab(declared_lab: &str, model: &str, lineage: &str) -> (String, LabSource) {
    let declared = declared_lab.trim();
    if !declared.is_empty() {
        return (declared.to_lowercase(), LabSource::Roster);
    }
    let v = model_lab(model);
    if !v.is_empty() {
        return (v, LabSource::Vendor);
    }
    (lineage.to_lowercase(), LabSource::Singleton)
}

/// The lab of a roster entry as the panel seats it: (0.6.0, wave 29b, E5) a claude endpoint entry
/// without a declared `lab` takes the lab of its base URL's host first, else [`entry_lab`] on the
/// resolved `model` / `lineage`.
pub fn member_lab(entry: &RosterEntry, model: &str, lineage: &str) -> (String, LabSource) {
    let host_lab = match (&entry.endpoint, entry.lab.trim().is_empty()) {
        (Some(ep), true) if entry.auth == "endpoint" => c3_core::claude::lab_of_host(&ep.host_name),
        _ => "",
    };
    if !host_lab.is_empty() {
        (host_lab.to_string(), LabSource::Vendor)
    } else {
        entry_lab(&entry.lab, model, lineage)
    }
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
    /// (0.6.0, wave 29, D5) `engine:<name>` for an engine whose members share ONE scheduling
    /// group (claude: `ParallelScope 'engine'`), `""` otherwise.
    pub scope: String,
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
        // (0.6.0, wave 29, D5) the members of a ParallelScope-engine engine share one group
        if !m.scope.is_empty() {
            let e = fps.iter_mut().find(|(l, _)| *l == m.label).unwrap();
            if !e.1.contains(&m.scope) {
                e.1.push(m.scope.clone());
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
        // (ext.c3.inactive) a matcher that names ONLY inactive entries is an explicit request for
        // an inactive reviewer - it fails clearly rather than silently matching nothing.
        let active: Vec<i64> = positions
            .iter()
            .copied()
            .filter(|p| {
                !roster
                    .entries
                    .iter()
                    .any(|e| e.position as i64 == *p && e.inactive)
            })
            .collect();
        if active.is_empty() {
            r.error = format!(
                "reviewer '{}' is inactive (ext.c3.inactive in the roster)",
                m.trim()
            );
            return r;
        }
        for p in active {
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
    /// (wave 9) the budget multiplier of this member's plan (`1.0` = no budget signal); the caller
    /// computes it from [`crate::budget`] so the seating stays a pure, network-free function.
    pub budget_mult: f64,
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
    /// (wave 9) the member's roster `plan` and its budget multiplier (`1.0` = no signal); the
    /// routed draw weighs `score * mult`.
    pub plan: String,
    pub mult: f64,
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
    /// router used priors or non-default parameters, (0.2.5, wave C1) `c3.seating` when the
    /// one-seat-per-lab / upper-tier rules could change the seats; `None` writes no key, so the
    /// record stays byte-identical to the plugin's.
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
    /// (wave 9) the budget multiplier of the member's plan (`1.0` = none); the draw weighs
    /// `score * mult`, and `c3 router replay` reads it back.
    pub mult: f64,
}

#[derive(Debug, Clone)]
pub struct PickedRow {
    pub slot: i32,
    pub position: i64,
    pub lineage: String,
    pub lab: String,
    pub rule: String,
    /// (wave 9) the budget multiplier applied to this seat's draw weight (`1.0` = none).
    pub mult: f64,
}

/// `Select-PanelRouting`'s result.
pub struct PanelRoutingResult {
    /// Every member in roster order (state updated: `not-picked` for eligible-but-unseated).
    pub members: Vec<SeatedMember>,
    /// The seated members, in seat order.
    pub picked: Vec<SeatedMember>,
    /// (C3 panel re-seat (c)) the eligible-but-unseated members, in draw order - the reserve a
    /// before-start preflight failure promotes from, so the panel keeps its planned size.
    pub reserve: Vec<SeatedMember>,
    pub k: i64,
    pub size_asked: i64,
    pub routing: RoutingRecord,
    pub warnings: Vec<String>,
}

/// (0.2.5, wave C1) The not-picked reason of a member kept out because its lab holds a seat.
pub fn lab_seated_reason(lab: &str, by: i64) -> String {
    format!("lab {lab} already seated by #{by}")
}

/// Whether a not-picked reason is [`lab_seated_reason`]'s.
pub fn is_lab_seated_reason(reason: &str) -> bool {
    reason.starts_with("lab ") && reason.contains(" already seated by #")
}

/// Round to `d` decimals (banker's rounding matches PowerShell's `[Math]::Round`).
fn round(x: f64, d: i32) -> f64 {
    let f = 10f64.powi(d);
    (x * f).round() / f
}

/// Inputs shared by [`select_panel_routing`] and [`select_panel_routing_with`]: the members to
/// seat, the panel size and its source, the draw order, the ratings/purpose/topics the routed draw
/// scores on, the clock, the task/brief/nonce identity and the required positions. Borrowed so
/// callers pass without cloning.
pub struct PanelRoutingInput<'a> {
    pub members: &'a [MemberInput],
    pub size: i32,
    pub size_source: &'a str,
    pub order: &'a str,
    pub ratings: &'a [Rating],
    pub purpose: &'a str,
    pub topics: &'a [String],
    pub utc_now: DateTime<Utc>,
    pub task: &'a str,
    pub brief_sha: &'a str,
    pub nonce: &'a str,
    pub nonce_source: &'a str,
    pub required: &'a [i64],
}

/// [`select_panel_routing_with`]'s state up to the draw - shared with [`lab_seat_marks`], so `c3
/// router explain` judges eligibility, labs, pins and the upper tier exactly as the panel does.
struct Prepared {
    /// Every member in roster order, enriched (a required member a soft gate held back rejoined).
    seated: Vec<SeatedMember>,
    prior_uses: Vec<PriorUse>,
    /// Indices into `seated` of the eligible (state `run`).
    eligible_idx: Vec<usize>,
    eligible_count: i64,
    compat: bool,
    /// (0.2.5, wave C1) one seat per lab applies.
    lab_rule: bool,
    panel_all: bool,
    k: i64,
    size_asked: i64,
    size_source: String,
    pinned_count: i64,
    size_raised: bool,
    /// `routed` | `roster` (after the "no ratings" fallback).
    mode: String,
    fallback: String,
    seed_bytes: Vec<u8>,
    seed_hex: String,
    /// The draw candidates: the eligible members in roster order.
    cands: Vec<Candidate>,
    k_before_labs: i64,
    /// The upper-tier positions (empty: the guarantee is not in force).
    tier: Vec<i64>,
}

/// The enrichment, eligibility, size, mode, seed, candidates and upper tier of
/// [`select_panel_routing_with`] - everything before the draw.
fn prepare(ctx: &RouterContext, input: &PanelRoutingInput) -> Prepared {
    let &PanelRoutingInput {
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
        required,
        ..
    } = input;
    // Enrich each member: a required member that only the weighty gate (0.6.0: or the light gate)
    // held back rejoins as `run`.
    let mut seated: Vec<SeatedMember> = Vec::new();
    let mut prior_uses: Vec<PriorUse> = Vec::new();
    for m in members {
        let mut state = m.state.clone();
        let mut reason = m.reason.clone();
        let is_req = required.contains(&(m.entry.position as i64));
        // A required member that only a soft gate held back rejoins: the weighty / light panel
        // gate, or (0.2.4) `request` - an on-request entry a -Require names runs. A held protected
        // entry (`reserve`) never reaches here: the required-outage check refuses the run first.
        if is_req
            && state == "skipped"
            && (m.skip_kind == "weighty" || m.skip_kind == "light" || m.skip_kind == "request")
        {
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
        let (lab, lab_source) = member_lab(&m.entry, &m.model, &lineage);
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
        let score_value = score.score;
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
            plan: m.entry.plan.clone(),
            // (wave 9) the plan multiplier, but never a boost for a below-neutral lineage.
            mult: crate::budget::effective_multiplier(
                score_value,
                ROUTING_NEUTRAL,
                if m.budget_mult.is_finite() && m.budget_mult > 0.0 {
                    m.budget_mult
                } else {
                    1.0
                },
            ),
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

    // (0.2.5, wave C1) A C3 panel seats ONE entry per lab (the model's maker): two models of one
    // vendor repeat each other, so their agreement is not corroboration. Not for the routed bare
    // default (one reviewer), not for -PanelAll (every eligible entry, warned below), and not in the
    // plugin-compat run (`C3_PLUGIN_COMPAT=1`), whose oracle texts are the plugin's draw.
    let compat = c3_core::spelling::plugin();
    let lab_rule = !compat && size_source != "bare-default" && size_source != "-PanelAll";
    let panel_all = !compat && size_source == "-PanelAll";

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

    // The draw candidates: the eligible members in roster order.
    let cands: Vec<Candidate> = eligible_idx
        .iter()
        .map(|&i| Candidate {
            position: seated[i].position,
            lab: seated[i].lab.clone(),
            // (wave 9) budget-aware: the lineage's score scaled by its plan's multiplier.
            weight: seated[i].score.score * seated[i].mult,
            pinned: seated[i].required,
        })
        .collect();
    // (0.2.5, wave C1) one seat per lab caps the panel at every pin plus one seat per other lab.
    let k_before_labs = k;
    if lab_rule {
        k = k.min(routing::lab_seat_cap(&cands) as i64);
    }
    // (0.2.5, wave C1) the upper tier: on a weighty purpose a panel of two or more seats holds at
    // least one `panel: weighty` entry when one is eligible (available, not held, not on-request -
    // every such skip already left it out of the eligible).
    let tier: Vec<i64> = if lab_rule && c3_core::purpose::is_weighty(purpose) && k >= 2 {
        eligible_idx
            .iter()
            .filter(|&&i| members[i].entry.panel == "weighty")
            .map(|&i| seated[i].position)
            .collect()
    } else {
        Vec::new()
    };

    Prepared {
        seated,
        prior_uses,
        eligible_idx,
        eligible_count,
        compat,
        lab_rule,
        panel_all,
        k,
        size_asked,
        size_source,
        pinned_count,
        size_raised,
        mode,
        fallback,
        seed_bytes,
        seed_hex,
        cands,
        k_before_labs,
        tier,
    }
}

/// `Select-PanelRouting` (D1, D4-D7) with the plugin's weights: no priors, default parameters.
pub fn select_panel_routing(input: &PanelRoutingInput) -> PanelRoutingResult {
    select_panel_routing_with(&RouterContext::default(), input)
}

/// `Select-PanelRouting` with the weight source of router v1 (M9): the same draw, the score
/// from [`router::score`]. With no priors in `ctx` and default parameters every number and
/// every byte of the record equals [`select_panel_routing`]'s.
pub fn select_panel_routing_with(
    ctx: &RouterContext,
    input: &PanelRoutingInput,
) -> PanelRoutingResult {
    let &PanelRoutingInput {
        purpose,
        order,
        nonce,
        nonce_source,
        ..
    } = input;
    let neutral = ROUTING_NEUTRAL;
    let Prepared {
        mut seated,
        prior_uses,
        eligible_idx,
        eligible_count,
        compat,
        lab_rule,
        panel_all,
        k,
        size_asked,
        size_source,
        pinned_count,
        size_raised,
        mode,
        fallback,
        seed_bytes,
        seed_hex,
        cands,
        k_before_labs,
        tier,
    } = prepare(ctx, input);

    // Seats: (member index into `seated`, rule).
    let mut seats: Vec<(usize, i32, String)> = Vec::new();
    // (C3 panel re-seat (c)) the eligible-but-unseated members in draw order, for promotion when a
    // seat is lost to a before-start preflight failure.
    let mut reserve_positions: Vec<i64> = Vec::new();
    // (0.2.5, wave C1) the eligible kept out because their lab holds a seat: (position, by).
    let mut kept_out: Vec<(i64, i64)> = Vec::new();
    let seat_index =
        |position: i64| -> usize { seated.iter().position(|m| m.position == position).unwrap() };
    if mode == "routed" {
        let drawn = if lab_rule {
            let d = routing::invoke_lab_draw(
                &cands,
                k as usize,
                &seed_bytes,
                neutral,
                ROUTING_EXPLORE,
                &tier,
            );
            for s in
                routing::invoke_lab_draw_reserve(&cands, &d.seats, &seed_bytes, ROUTING_EXPLORE)
            {
                reserve_positions.push(s.position);
            }
            kept_out = d.lab_seated;
            d.seats
        } else {
            let drawn =
                invoke_panel_draw(&cands, k as usize, &seed_bytes, neutral, ROUTING_EXPLORE);
            // the reserve continues the SAME seeded draw past the panel's seats
            for s in
                routing::invoke_panel_draw_reserve(&cands, &drawn, &seed_bytes, ROUTING_EXPLORE)
            {
                reserve_positions.push(s.position);
            }
            drawn
        };
        for s in drawn {
            seats.push((seat_index(s.position), s.slot, s.rule));
        }
    } else if lab_rule {
        // roster order, one seat per lab (the first entry of each lab in roster order)
        let (d, reserve) = routing::roster_lab_seats(&cands, k as usize, &tier);
        reserve_positions = reserve;
        kept_out = d.lab_seated;
        for s in d.seats {
            seats.push((seat_index(s.position), s.slot, s.rule));
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
        // the reserve is the remaining eligible in roster order
        for &i in &eligible_idx {
            if !chosen.contains(&i) {
                reserve_positions.push(seated[i].position);
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
            seated[i].reason = match kept_out.iter().find(|(p, _)| *p == seated[i].position) {
                Some((_, by)) => lab_seated_reason(&seated[i].lab, *by),
                None => format!("panel size {k}"),
            };
        }
    }

    // Warnings.
    let mut warnings: Vec<String> = Vec::new();
    if size_raised {
        warnings.push(format!(
            "panel size raised: asked {size_asked}, required {pinned_count}"
        ));
    }
    if k < k_before_labs {
        warnings.push(format!(
            "panel size reduced: asked {size_asked}, eligible {eligible_count}, one seat per lab leaves {k} (--panel-all seats every entry)"
        ));
    } else if k < size_asked {
        warnings.push(format!(
            "panel size reduced: asked {size_asked}, eligible {eligible_count}"
        ));
    }
    // (0.2.5, wave C1) two required reviewers of one lab are both seated (an explicit ask), and
    // -PanelAll seats every entry: say once which labs hold more than one seat.
    let shared_labs = |only_pinned: bool| -> Vec<(String, Vec<i64>)> {
        let mut by_lab: Vec<(String, Vec<i64>)> = Vec::new();
        for (mi, _, _) in &seats {
            let m = &seated[*mi];
            if only_pinned && !m.required {
                continue;
            }
            match by_lab.iter_mut().find(|(l, _)| *l == m.lab) {
                Some((_, ps)) => ps.push(m.position),
                None => by_lab.push((m.lab.clone(), vec![m.position])),
            }
        }
        by_lab.retain(|(_, ps)| ps.len() > 1);
        by_lab
    };
    let hash_list = |ps: &[i64]| -> String {
        ps.iter()
            .map(|p| format!("#{p}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    if lab_rule {
        for (lab, ps) in shared_labs(true) {
            warnings.push(format!(
                "required reviewers {} share lab {lab} - each is seated because it is required (a panel otherwise seats one entry per lab)",
                hash_list(&ps)
            ));
        }
    }
    if panel_all {
        let shared = shared_labs(false);
        if !shared.is_empty() {
            let labs: Vec<String> = shared
                .iter()
                .map(|(lab, ps)| format!("lab {lab} holds {} ({})", ps.len(), hash_list(ps)))
                .collect();
            warnings.push(format!(
                "--panel-all seats every eligible entry: {} - a panel otherwise seats one entry per lab",
                labs.join("; ")
            ));
        }
    }
    // (0.2.5 R4, F16-8 / F17-3) the upper-tier guarantee missed because every eligible weighty
    // entry shares a lab with a seat already taken (a required one): said out loud - the one-lab
    // rule is not relaxed.
    if let Some(w) = tier_missed_warning(purpose, &tier, &cands, &seats, &seated) {
        warnings.push(w);
    }
    if mode == "routed" || lab_rule {
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
    // (0.2.5, wave C1) The record says when the C3 seating rules could seat differently from the
    // plugin's draw - two eligible entries share a lab, or an upper-tier seat was drawn - so `c3
    // router replay` replays it with the same rules: `ext.c3.seating = {labs: "one-per-lab", tier:
    // [the upper-tier positions, empty when the guarantee was not in force]}`. Otherwise the draws
    // are the same and the record stays the plugin's.
    let mut ext = router::routing_ext(ctx, &prior_uses);
    let labs_shared = cands
        .iter()
        .enumerate()
        .any(|(i, c)| cands[..i].iter().any(|o| o.lab == c.lab));
    let tier_seated = seats.iter().any(|(_, _, r)| r.starts_with("tier"));
    if lab_rule && (labs_shared || tier_seated) {
        let v = ext.get_or_insert_with(|| serde_json::json!({ "c3": {} }));
        v["c3"]["seating"] = serde_json::json!({ "labs": "one-per-lab", "tier": tier });
    }
    // (0.2.5 R4, F16-6) The record keeps `score` and `budget_mult` rounded (for display, as the
    // plugin writes them); a routed draw weighed the UNROUNDED values, so when rounding lost
    // anything the record also carries them at full precision - `ext.c3.draw = [{position, score,
    // mult}]`, eligible order - and `c3 router replay` weighs those: a near tie replays to the
    // winner it had. Roster order weighs nothing, and a record that rounding did not change stays
    // the plugin's.
    if !compat && mode == "routed" {
        let lossy = eligible_idx.iter().any(|&i| {
            round(seated[i].score.score, 4) != seated[i].score.score
                || round(seated[i].mult, 4) != seated[i].mult
        });
        if lossy {
            let draw: Vec<serde_json::Value> = eligible_idx
                .iter()
                .map(|&i| {
                    serde_json::json!({
                        "position": seated[i].position,
                        "score": seated[i].score.score,
                        "mult": seated[i].mult,
                    })
                })
                .collect();
            let v = ext.get_or_insert_with(|| serde_json::json!({ "c3": {} }));
            v["c3"]["draw"] = serde_json::Value::Array(draw);
        }
    }
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
                mult: round(seated[i].mult, 4),
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
                mult: round(seated[*mi].mult, 4),
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
        ext,
    };

    let picked: Vec<SeatedMember> = seats.iter().map(|(mi, _, _)| seated[*mi].clone()).collect();
    // (C3 panel re-seat (c)) the reserve members (not-picked eligible), in draw order
    let reserve: Vec<SeatedMember> = reserve_positions
        .iter()
        .filter_map(|pos| seated.iter().find(|m| m.position == *pos).cloned())
        .collect();

    PanelRoutingResult {
        members: seated,
        picked,
        reserve,
        k,
        size_asked,
        routing,
        warnings,
    }
}

/// (0.2.5 R4, F16-8 / F17-3) The warning for an upper-tier guarantee the one-lab rule could not
/// keep: the seat was owed (`tier` names the eligible weighty positions and no pin is one of them),
/// no seat went to the tier, and every tier entry's lab already holds a seat. Names the purpose,
/// each lab and the entries it kept out.
fn tier_missed_warning(
    purpose: &str,
    tier: &[i64],
    cands: &[Candidate],
    seats: &[(usize, i32, String)],
    seated: &[SeatedMember],
) -> Option<String> {
    if tier.is_empty()
        || cands.iter().any(|c| c.pinned && tier.contains(&c.position))
        || seats.iter().any(|(_, _, r)| r.starts_with("tier"))
    {
        return None;
    }
    let seated_at = |lab: &str| -> Option<i64> {
        seats
            .iter()
            .map(|(mi, _, _)| &seated[*mi])
            .find(|m| m.lab == lab)
            .map(|m| m.position)
    };
    // (lab, the seat holding it, the tier entries it keeps out)
    let mut parts: Vec<(String, i64, Vec<String>)> = Vec::new();
    for c in cands.iter().filter(|c| tier.contains(&c.position)) {
        let by = seated_at(&c.lab)?;
        let shown = seated
            .iter()
            .find(|m| m.position == c.position)
            .map(|m| format!("#{} {}", m.position, m.lineage))
            .unwrap_or_else(|| format!("#{}", c.position));
        match parts.iter_mut().find(|(l, _, _)| *l == c.lab) {
            Some((_, _, kept)) => kept.push(shown),
            None => parts.push((c.lab.clone(), by, vec![shown])),
        }
    }
    let text: Vec<String> = parts
        .iter()
        .map(|(lab, by, kept)| {
            format!(
                "lab {lab} is held by #{by}, keeping out {}",
                kept.join(", ")
            )
        })
        .collect();
    Some(format!(
        "upper tier missed: purpose {purpose} owes a seat to a weighty entry, but every eligible one shares a lab with a seat already taken ({}) - one seat per lab",
        text.join("; ")
    ))
}

/// (0.2.5 R4, F16-6 / F17-2) What `c3 router explain` marks on each roster entry, by position: why
/// an entry would not run (`not eligible: <reason>` - the panel's own skip reason; an on-request
/// entry and one held by its reserve are left to explain's own marks), and, under the one-seat-per-
/// lab rule, which entry holds its lab's seat when it is not this one - a required entry, the
/// upper-tier entry (certain, or when the routed upper-tier draw falls to that lab), else the lab's
/// representative in the draw. Judged by [`select_panel_routing_with`]'s own preparation and the
/// draw's own pools ([`routing::lab_stands`]), for the inputs a panel of this purpose would get.
pub fn lab_seat_marks(ctx: &RouterContext, input: &PanelRoutingInput) -> Vec<(i64, String)> {
    let p = prepare(ctx, input);
    let mut marks: Vec<(i64, String)> = Vec::new();
    for (i, m) in p.seated.iter().enumerate() {
        let kind = input
            .members
            .get(i)
            .map(|x| x.skip_kind.as_str())
            .unwrap_or("");
        if m.state != "run" && kind != "request" && kind != "reserve" {
            marks.push((m.position, format!("not eligible: {}", m.reason)));
        }
    }
    if !p.lab_rule {
        return marks;
    }
    let stands = routing::lab_stands(&p.cands, p.k.max(0) as usize, &p.tier, p.mode != "routed");
    for c in p.cands.iter().filter(|c| !c.pinned) {
        let Some(st) = stands.iter().find(|s| s.lab == c.lab) else {
            continue;
        };
        let lab = &st.lab;
        let me = c.position;
        let text = if let Some(&pin) = st.pins.first() {
            Some(format!(
                "lab {lab}: #{pin} takes its seat (required) - one seat per lab"
            ))
        } else {
            match (st.tier, st.open) {
                (Some(t), _) if st.tier_certain => (t != me).then(|| {
                    format!("lab {lab}: #{t} takes its seat (upper tier) - one seat per lab")
                }),
                (Some(t), Some(r)) if t != r => Some(if me == r {
                    format!("lab {lab}: #{t} takes its seat instead when the upper-tier draw falls to lab {lab}")
                } else if me == t {
                    format!("lab {lab}: #{r} takes its seat unless the upper-tier draw falls to lab {lab} (then #{me})")
                } else {
                    format!("lab {lab}: #{r} takes its seat, #{t} when the upper-tier draw falls to lab {lab} - one seat per lab")
                }),
                (_, Some(r)) => {
                    (r != me).then(|| format!("lab {lab}: #{r} takes its seat - one seat per lab"))
                }
                _ => None,
            }
        };
        if let Some(t) = text {
            marks.push((me, t));
        }
    }
    marks
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
            budget_mult: 1.0,
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
                scope: String::new(),
            },
            Runner {
                position: 2,
                label: "agy2".into(),
                fingerprint: "google".into(),
                plan: String::new(),
                scope: String::new(),
            },
            Runner {
                position: 3,
                label: "zai".into(),
                fingerprint: "zaifp".into(),
                plan: String::new(),
                scope: String::new(),
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
                scope: String::new(),
            },
            Runner {
                position: 2,
                label: "byteplus".into(),
                fingerprint: "b".into(),
                plan: String::new(),
                scope: String::new(),
            },
            Runner {
                position: 3,
                label: "byteplus".into(),
                fingerprint: "b".into(),
                plan: String::new(),
                scope: String::new(),
            },
            Runner {
                position: 4,
                label: "byteplus".into(),
                fingerprint: "b".into(),
                plan: String::new(),
                scope: String::new(),
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
            scope: String::new(),
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
        let res = select_panel_routing(&PanelRoutingInput {
            members: &members,
            size: 2,
            size_source: "-PanelSize",
            order: "roster",
            ratings: &[],
            purpose: "framing",
            topics: &[],
            utc_now: Utc::now(),
            task: "task",
            brief_sha: "sha",
            nonce: "nonce",
            nonce_source: "-PanelSeed",
            required: &[3],
        });
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

    // (C3 panel re-seat (c)) the reserve is the eligible-but-unseated members in draw order - the
    // list a before-start preflight failure promotes from, so the panel keeps its planned size.
    #[test]
    fn the_reserve_is_the_unseated_eligible_in_draw_order() {
        let members = vec![
            member(1, "openai", "gpt-6", "run"),
            member(2, "zai", "glm-5.3", "run"),
            member(3, "google", "gemini-3", "run"),
            member(4, "moonshot", "kimi-k2", "run"),
        ];
        let res = select_panel_routing(&PanelRoutingInput {
            members: &members,
            size: 2,
            size_source: "-PanelSize",
            order: "roster",
            ratings: &[],
            purpose: "framing",
            topics: &[],
            utc_now: Utc::now(),
            task: "task",
            brief_sha: "sha",
            nonce: "nonce",
            nonce_source: "-PanelSeed",
            required: &[3],
        });
        // seats: required #3, then #1 (roster order)
        let picked: Vec<i64> = res.picked.iter().map(|p| p.position).collect();
        assert_eq!(picked, vec![3, 1]);
        // reserve: the remaining eligible, in roster order, none of them a seated one
        let reserve: Vec<i64> = res.reserve.iter().map(|m| m.position).collect();
        assert_eq!(reserve, vec![2, 4]);
        assert!(res.reserve.iter().all(|m| !picked.contains(&m.position)));
    }

    // a member skipped at selection (e.g. the model is not supported on this login) is neither
    // seated nor in the reserve - only eligible `run` members can be promoted.
    #[test]
    fn a_skipped_member_is_excluded_from_seats_and_reserve() {
        let mut m3 = member(3, "google", "gemini-3", "skipped");
        m3.reason = "model not supported on this login".into();
        m3.skip_kind = "unavailable".into();
        let members = vec![
            member(1, "openai", "gpt-6", "run"),
            member(2, "zai", "glm-5.3", "run"),
            m3,
            member(4, "moonshot", "kimi-k2", "run"),
        ];
        let res = select_panel_routing(&PanelRoutingInput {
            members: &members,
            size: 2,
            size_source: "-PanelSize",
            order: "roster",
            ratings: &[],
            purpose: "framing",
            topics: &[],
            utc_now: Utc::now(),
            task: "task",
            brief_sha: "sha",
            nonce: "nonce",
            nonce_source: "-PanelSeed",
            required: &[],
        });
        let picked: Vec<i64> = res.picked.iter().map(|p| p.position).collect();
        assert_eq!(picked, vec![1, 2]);
        let reserve: Vec<i64> = res.reserve.iter().map(|m| m.position).collect();
        // #4 remains eligible for promotion; the skipped #3 never appears
        assert_eq!(reserve, vec![4]);
        assert!(!reserve.contains(&3));
    }

    #[test]
    fn routed_falls_back_to_roster_without_ratings() {
        let members = vec![
            member(1, "openai", "gpt-6", "run"),
            member(2, "zai", "glm-5.3", "run"),
        ];
        let res = select_panel_routing(&PanelRoutingInput {
            members: &members,
            size: 2,
            size_source: "-PanelSize",
            order: "routed",
            ratings: &[],
            purpose: "framing",
            topics: &[],
            utc_now: Utc::now(),
            task: "task",
            brief_sha: "sha",
            nonce: "nonce",
            nonce_source: "date",
            required: &[],
        });
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
            &PanelRoutingInput {
                members,
                size: 2,
                size_source: "-PanelSize",
                order: "routed",
                ratings: &[],
                purpose: "framing",
                topics: &[],
                utc_now: Utc::now(),
                task: "task",
                brief_sha: "sha",
                nonce: "nonce",
                nonce_source: "date",
                required: &[],
            },
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

    // ------------------------------------------------ (0.2.5, wave C1) one seat per lab, the tier

    fn input<'a>(
        members: &'a [MemberInput],
        size: i32,
        size_source: &'a str,
        purpose: &'a str,
        nonce: &'a str,
        required: &'a [i64],
    ) -> PanelRoutingInput<'a> {
        PanelRoutingInput {
            members,
            size,
            size_source,
            order: "routed",
            ratings: &[],
            purpose,
            topics: &[],
            utc_now: Utc::now(),
            task: "task",
            brief_sha: "sha",
            nonce,
            nonce_source: "-PanelSeed",
            required,
        }
    }

    fn weighty(mut m: MemberInput) -> MemberInput {
        m.entry.panel = "weighty".into();
        m
    }

    /// A claude endpoint entry on a made-up host (no lab of its own).
    fn endpoint_member(position: usize, provider: &str, model: &str) -> MemberInput {
        let mut m = member(position, provider, model, "run");
        m.engine = "claude".into();
        m.entry.engine = "claude".into();
        m.entry.auth = "endpoint".into();
        m.entry.endpoint = Some(c3_core::claude::ClaudeEndpoint {
            base_url: "https://llm.example.test/anthropic".into(),
            canonical: "https://llm.example.test/anthropic".into(),
            host_name: "llm.example.test".into(),
            env_key: "EXAMPLE_KEY".into(),
            ..Default::default()
        });
        m
    }

    /// Ratings that put `model` (provider `prov`, codex engine) over the routed threshold, so the
    /// draw routes.
    fn ratings_for(prov: &str, model: &str, purpose: &str) -> Vec<Rating> {
        ratings_on(prov, model, "codex", purpose)
    }

    fn ratings_on(prov: &str, model: &str, engine: &str, purpose: &str) -> Vec<Rating> {
        let now = Utc::now().fixed_offset();
        (0..3)
            .map(|_| Rating {
                provider: prov.into(),
                model: model.into(),
                engine: engine.into(),
                purpose: purpose.into(),
                topics: vec![],
                consult_when: Some(now),
                useful: "yes".into(),
            })
            .collect()
    }

    #[test]
    fn model_lab_covers_the_roster_families() {
        for (id, lab) in [
            ("grok-4.7", "xai"),
            ("muse-spark-1.3-contributor", "meta"),
            ("muse-wonder-1.0", "meta"),
            ("deepseek-v4.1-flash-1017", "deepseek"),
            ("qwen4-max", "alibaba"),
            ("kimi-code/k3.1-pro", "moonshot"),
            ("kimi-code/kimi-for-coding", "moonshot"),
            ("k3.1-pro", "moonshot"),
            ("glm-5.3-flash", "zhipu"),
            ("mimo-v2.6-pro", "xiaomi"),
            ("gemini-3.8-flash-high", "google"),
            ("gpt-6.2-astra", "openai"),
            ("o3", "openai"),
            ("o4-mini", "openai"),
            ("claude-opus-5-5", "anthropic"),
            ("opus", "anthropic"),
            ("dola-seed-2-0-pro", "bytedance"),
            ("seed-oss", "bytedance"),
            // an OpenRouter-style id resolves by its last segment
            ("x-ai/grok-5", "xai"),
            ("google/gemini-4", "google"),
        ] {
            assert_eq!(model_lab(id), lab, "{id}");
        }
        assert_eq!(model_lab("orca-1"), "");
        // no singleton warning for a roster family any more
        let members = vec![
            member(1, "grok", "grok-4.7", "run"),
            member(2, "meta", "muse-spark-1.3-contributor", "run"),
        ];
        let res = select_panel_routing(&input(&members, 2, "purpose", "framing", "n", &[]));
        assert!(
            !res.warnings.iter().any(|w| w.contains("no lab known")),
            "{:?}",
            res.warnings
        );
    }

    // The 2026-11-20 panel (roster order, no ratings): glm-5.3, glm-5.3-flash, mimo - the second
    // GLM is kept out, the next lab takes its seat.
    #[test]
    fn two_glm_entries_never_sit_in_one_panel() {
        let members = vec![
            endpoint_member(1, "ZAI-claude", "glm-5.3"),
            endpoint_member(2, "ZAI-claude", "glm-5.3-flash"),
            endpoint_member(3, "mimo-claude", "mimo-v2.6-pro"),
            member(4, "gemini", "gemini-3.8-flash-high", "run"),
        ];
        let res = select_panel_routing(&input(&members, 3, "purpose", "framing", "n", &[]));
        assert_eq!(res.routing.mode, "roster");
        let picked: Vec<i64> = res.picked.iter().map(|p| p.position).collect();
        assert_eq!(picked, vec![1, 3, 4]);
        let m2 = res.members.iter().find(|m| m.position == 2).unwrap();
        assert_eq!(m2.state, "not-picked");
        assert_eq!(m2.reason, "lab zhipu already seated by #1");
        assert!(is_lab_seated_reason(&m2.reason));
        // the reserve never holds a seated lab
        assert!(res.reserve.iter().all(|m| m.position != 2));
        // the record says so, for the replay
        let ext = res.routing.ext.expect("seating recorded");
        assert_eq!(ext["c3"]["seating"]["labs"], "one-per-lab");
        // routed (with ratings), over many seeds: never two zhipu seats
        let ratings = ratings_on("ZAI-claude", "glm-5.3-flash", "claude", "framing");
        for n in 1..=200 {
            let nonce = n.to_string();
            let mut inp = input(&members, 3, "purpose", "framing", &nonce, &[]);
            inp.ratings = &ratings;
            let res = select_panel_routing(&inp);
            assert_eq!(res.routing.mode, "routed");
            let labs: Vec<&str> = res.picked.iter().map(|p| p.lab.as_str()).collect();
            assert_eq!(
                labs.iter().filter(|l| **l == "zhipu").count(),
                1,
                "nonce {n}: {labs:?}"
            );
            // the better-rated flash is zhipu's pick
            assert!(res.picked.iter().any(|p| p.position == 2), "nonce {n}");
        }
    }

    // Qwen and DeepSeek behind ONE label are two labs: both seatable.
    #[test]
    fn qwen_and_deepseek_behind_one_label_are_two_labs() {
        let members = vec![
            endpoint_member(1, "alibaba-claude", "qwen4-max"),
            endpoint_member(2, "alibaba-claude", "deepseek-v4.1-flash-1017"),
            endpoint_member(3, "byteplus-claude", "deepseek-v4-1-flash"),
        ];
        let res = select_panel_routing(&input(&members, 3, "-PanelSize", "framing", "n", &[]));
        let picked: Vec<(i64, &str)> = res
            .picked
            .iter()
            .map(|p| (p.position, p.lab.as_str()))
            .collect();
        assert_eq!(picked, vec![(1, "alibaba"), (2, "deepseek")]);
        // the second deepseek (another label) is kept out, and the size says why
        let m3 = res.members.iter().find(|m| m.position == 3).unwrap();
        assert_eq!(m3.reason, "lab deepseek already seated by #2");
        assert!(res
            .warnings
            .iter()
            .any(|w| w.contains("one seat per lab leaves 2")));
    }

    // --require naming two entries of one lab: both seated, with a warning; -PanelAll seats every
    // entry and warns once; the bare default keeps the plugin's draw.
    #[test]
    fn require_two_of_one_lab_seats_both_with_a_warning() {
        let members = vec![
            member(1, "ZAI", "glm-5.3", "run"),
            member(2, "ZAI", "glm-5.3-flash", "run"),
            member(3, "ZAI", "glm-5.2", "run"),
            member(4, "gemini", "gemini-3", "run"),
        ];
        let res = select_panel_routing(&input(&members, 3, "purpose", "framing", "n", &[1, 2]));
        let picked: Vec<(i64, &str)> = res
            .picked
            .iter()
            .map(|p| (p.position, p.rule.as_str()))
            .collect();
        assert_eq!(
            picked,
            vec![(1, "required"), (2, "required"), (4, "roster")]
        );
        assert_eq!(
            res.members.iter().find(|m| m.position == 3).unwrap().reason,
            "lab zhipu already seated by #1"
        );
        let w: Vec<&String> = res
            .warnings
            .iter()
            .filter(|w| w.starts_with("required reviewers #1, #2 share lab zhipu"))
            .collect();
        assert_eq!(w.len(), 1, "{:?}", res.warnings);

        let all = select_panel_routing(&input(&members, 0, "-PanelAll", "framing", "n", &[]));
        assert_eq!(all.picked.len(), 4);
        let w: Vec<&String> = all
            .warnings
            .iter()
            .filter(|w| w.starts_with("--panel-all seats every eligible entry"))
            .collect();
        assert_eq!(w.len(), 1, "{:?}", all.warnings);
        assert!(w[0].contains("lab zhipu holds 3 (#1, #2, #3)"), "{}", w[0]);
        assert!(
            all.routing.ext.is_none(),
            "-PanelAll keeps the plugin's record"
        );

        // the routed bare default (one reviewer) draws over every entry as before: across seeds
        // both GLM entries can be the pick (one-per-lab would always take the first)
        let two = vec![
            member(1, "ZAI", "glm-5.3", "run"),
            member(2, "ZAI", "glm-5.3-flash", "run"),
        ];
        let mut ratings = ratings_for("ZAI", "glm-5.3", "framing");
        ratings.extend(ratings_for("ZAI", "glm-5.3-flash", "framing"));
        let mut seen: Vec<i64> = Vec::new();
        for n in 1..=60 {
            let nonce = n.to_string();
            let mut inp = input(&two, 1, "bare-default", "framing", &nonce, &[]);
            inp.ratings = &ratings;
            let res = select_panel_routing(&inp);
            assert_eq!(res.routing.mode, "routed");
            // the plugin's draw: no seating rules recorded (R4: the draw's unrounded values are)
            let ext = res.routing.ext.as_ref().expect("the unrounded draw values");
            assert!(ext["c3"].get("seating").is_none(), "{ext}");
            assert!(ext["c3"]["draw"].is_array(), "{ext}");
            let p = res.picked[0].position;
            if !seen.contains(&p) {
                seen.push(p);
            }
        }
        seen.sort_unstable();
        assert_eq!(seen, vec![1, 2]);
    }

    // The upper tier: on a weighty purpose a panel of two or more seats holds a `panel: weighty`
    // entry when one is eligible - not on a light purpose, not when the panel has one seat.
    #[test]
    fn a_weighty_purpose_seats_the_upper_tier() {
        let members = vec![
            member(1, "ZAI", "glm-5.3", "run"),
            member(2, "mimo", "mimo-v2.6-pro", "run"),
            member(3, "gemini", "gemini-3", "run"),
            weighty(member(4, "kimi", "kimi-for-coding", "run")),
        ];
        // roster order (no ratings): the tier entry takes the first seat
        let res = select_panel_routing(&input(&members, 3, "purpose", "framing", "n", &[]));
        let picked: Vec<(i64, &str)> = res
            .picked
            .iter()
            .map(|p| (p.position, p.rule.as_str()))
            .collect();
        assert_eq!(picked, vec![(4, "tier"), (1, "roster"), (2, "roster")]);
        assert_eq!(
            res.routing.ext.as_ref().unwrap()["c3"]["seating"]["tier"],
            serde_json::json!([4])
        );
        // routed: the well-rated three would win, yet every panel holds the tier entry
        let mut ratings = ratings_for("ZAI", "glm-5.3", "framing");
        ratings.extend(ratings_for("mimo", "mimo-v2.6-pro", "framing"));
        ratings.extend(ratings_for("gemini", "gemini-3", "framing"));
        for n in 1..=100 {
            let nonce = n.to_string();
            let mut inp = input(&members, 3, "purpose", "framing", &nonce, &[]);
            inp.ratings = &ratings;
            let res = select_panel_routing(&inp);
            assert_eq!(res.routing.mode, "routed");
            assert!(res.picked.iter().any(|p| p.position == 4), "nonce {n}");
        }
        // a light purpose (the weighty gate is passed here only to prove the purpose decides): no
        // tier seat; a one-seat panel: none either
        let res = select_panel_routing(&input(&members, 3, "purpose", "diff-review", "n", &[]));
        assert!(res.picked.iter().all(|p| p.rule != "tier"));
        assert!(res.routing.ext.is_none());
        let res = select_panel_routing(&input(&members, 1, "-PanelSize", "framing", "n", &[]));
        assert_eq!(res.picked.len(), 1);
        assert_eq!(res.picked[0].position, 1);
    }

    // A routed record carrying the seating rules replays identically (`c3 router replay`).
    #[test]
    fn a_one_per_lab_record_replays_identically() {
        let members = vec![
            member(1, "ZAI", "glm-5.3", "run"),
            weighty(member(2, "ZAI", "glm-5.3-flash", "run")),
            member(3, "mimo", "mimo-v2.6-pro", "run"),
            member(4, "gemini", "gemini-3", "run"),
            weighty(member(5, "kimi", "kimi-for-coding", "run")),
        ];
        let mut ratings = ratings_for("ZAI", "glm-5.3", "framing");
        ratings.extend(ratings_for("gemini", "gemini-3", "framing"));
        for n in 1..=40 {
            let nonce = n.to_string();
            for order in ["routed", "roster"] {
                let mut inp = input(&members, 3, "purpose", "framing", &nonce, &[]);
                inp.ratings = &ratings;
                inp.order = order;
                let res = select_panel_routing(&inp);
                let v = crate::panel::run::routing_value(&res.routing);
                let rec: c3_core::ledger::PanelRouting = serde_json::from_value(v).unwrap();
                assert!(
                    crate::router::replay::replay_record(&rec).is_none(),
                    "nonce {n} {order}: {:?}",
                    crate::router::replay::replay_record(&rec)
                );
            }
        }
    }

    #[test]
    fn framing_below_two_seats_warns_unless_explicit_size() {
        // One eligible member, purpose framing, size from purpose default => floor warning.
        let members = vec![member(1, "openai", "gpt-6", "run")];
        let res = select_panel_routing(&PanelRoutingInput {
            members: &members,
            size: 3,
            size_source: "purpose",
            order: "roster",
            ratings: &[],
            purpose: "framing",
            topics: &[],
            utc_now: Utc::now(),
            task: "task",
            brief_sha: "sha",
            nonce: "nonce",
            nonce_source: "date",
            required: &[],
        });
        assert!(res.warnings.iter().any(|w| w.starts_with("panel floor:")));
        // Explicit -PanelSize suppresses the floor warning.
        let res = select_panel_routing(&PanelRoutingInput {
            members: &members,
            size: 1,
            size_source: "-PanelSize",
            order: "roster",
            ratings: &[],
            purpose: "framing",
            topics: &[],
            utc_now: Utc::now(),
            task: "task",
            brief_sha: "sha",
            nonce: "nonce",
            nonce_source: "date",
            required: &[],
        });
        assert!(!res.warnings.iter().any(|w| w.starts_with("panel floor:")));
    }

    // ------------------------------------------------ (0.2.5 R4) labs, explain, tier, precision

    // (R4, F16-7 / F17-4) Every maker of the roster has a lab; a prefix counts only at a boundary;
    // a claude alias only as the whole id; a declared lab is case-folded.
    #[test]
    fn every_maker_has_a_lab_and_prefixes_match_at_a_boundary() {
        for (id, lab) in [
            ("minimax-m3", "minimax"),
            ("MiniMax-M2.5", "minimax"),
            ("meta-llama/llama-4-maverick", "meta"),
            ("llama-3.3-70b", "meta"),
            ("llama4", "meta"),
            ("minimax/minimax-m3", "minimax"),
            ("qwen3-coder", "alibaba"),
            ("glm_5", "zhipu"),
            ("kimi:k2", "moonshot"),
            ("claude-opus-4-8", "anthropic"),
            ("opus", "anthropic"),
            ("opus[1m]", "anthropic"),
            ("Sonnet", "anthropic"),
        ] {
            assert_eq!(model_lab(id), lab, "{id}");
        }
        for id in [
            "opus-custom",
            "opus2",
            "sonnet-mini",
            "fablelike",
            "seedream-3",
            "gptx",
            "glmish",
            "claudette",
            "minimaxer",
            "llamafile-x",
        ] {
            assert_eq!(model_lab(id), "", "{id}");
        }
        // the four entries of F16-7
        let lab = |lab: &str, model: &str| {
            member_lab(
                &RosterEntry {
                    lab: lab.into(),
                    ..entry(1, "p", model)
                },
                model,
                &format!("p :: {model}"),
            )
            .0
        };
        assert_ne!(lab("", "opus-custom"), lab("", "claude-opus-4-8"));
        assert_eq!(lab("OpenAI", "x-1"), lab("", "gpt-6.2-sol"));
        assert_eq!(lab(" OpenAI ", "x-1"), "openai");
        let mut ep = endpoint_member(1, "minimax-claude", "MiniMax-M2").entry;
        ep.endpoint.as_mut().unwrap().host_name = "api.minimax.io".into();
        assert_eq!(
            member_lab(&ep, "MiniMax-M2", "minimax-claude :: MiniMax-M2").0,
            lab("", "minimax-m2")
        );
    }

    // (0.2.5 D3) A token names a lab by its vendor prefix (at a boundary) or by being a lab of the
    // table; anything else names none.
    #[test]
    fn a_token_names_a_lab_by_its_prefix_or_as_the_lab_itself() {
        assert_eq!(lab_named_by("kimi"), "moonshot");
        assert_eq!(lab_named_by("kimi-code"), "moonshot");
        assert_eq!(lab_named_by("Moonshot"), "moonshot");
        assert_eq!(lab_named_by("alibaba"), "alibaba");
        assert_eq!(lab_named_by("qwen"), "alibaba");
        assert_eq!(lab_named_by("supergrok"), "");
        assert_eq!(lab_named_by("zai"), "");
        assert_eq!(lab_named_by(""), "");
    }

    // (R4, F16-7) `lab_of_host` and the prefix table name the same lab for every host the function
    // knows: each of its labs has a prefix, and a model of that maker resolves to it.
    #[test]
    fn lab_of_host_and_the_prefix_table_agree() {
        for (host, model) in [
            ("api.openai.com", "gpt-6.2-sol"),
            ("chatgpt.com", "o4-mini"),
            ("api.z.ai", "glm-5.3"),
            ("open.bigmodel.cn", "glm-5.3-flash"),
            ("api.xiaomimimo.com", "mimo-v2.6-pro"),
            ("api.kimi.ai", "kimi-for-coding"),
            ("api.moonshot.ai", "kimi-k3"),
            ("api.minimax.io", "minimax-m3"),
            ("api.minimax.cn", "MiniMax-M2"),
        ] {
            let host_lab = c3_core::claude::lab_of_host(host);
            assert!(!host_lab.is_empty(), "{host}");
            assert!(
                LAB_VENDORS.iter().any(|(_, l)| *l == host_lab),
                "no prefix names lab {host_lab} ({host})"
            );
            assert_eq!(model_lab(model), host_lab, "{model} at {host}");
        }
    }

    // (R4, F16-8 / F17-3) On a weighty purpose, when every eligible weighty entry shares a lab with
    // a required (non-weighty) entry, no tier seat is drawn - the one-lab rule holds - and the plan
    // says so: the purpose, the lab and the entries kept out.
    #[test]
    fn a_missed_upper_tier_is_warned_about() {
        let members = vec![
            member(1, "kimi", "kimi-for-coding", "run"),
            weighty(member(2, "kimi-claude", "kimi-k3", "run")),
            member(3, "gemini", "gemini-3", "run"),
        ];
        for order in ["roster", "routed"] {
            let mut inp = input(&members, 2, "purpose", "acceptance", "n", &[1]);
            inp.order = order;
            let ratings = ratings_for("gemini", "gemini-3", "acceptance");
            inp.ratings = &ratings;
            let res = select_panel_routing(&inp);
            let picked: Vec<i64> = res.picked.iter().map(|p| p.position).collect();
            assert!(
                !picked.contains(&2),
                "{order}: the one-lab rule was relaxed: {picked:?}"
            );
            assert!(picked.contains(&1));
            let w: Vec<&String> = res
                .warnings
                .iter()
                .filter(|w| w.starts_with("upper tier missed:"))
                .collect();
            assert_eq!(w.len(), 1, "{order}: {:?}", res.warnings);
            assert!(w[0].contains("purpose acceptance"), "{}", w[0]);
            assert!(w[0].contains("lab moonshot is held by #1"), "{}", w[0]);
            assert!(w[0].contains("#2 kimi-claude :: kimi-k3"), "{}", w[0]);
        }
        // a weighty entry of a free lab is seated: no warning; a required weighty entry: none
        let free = vec![
            member(1, "kimi", "kimi-for-coding", "run"),
            weighty(member(2, "gemini", "gemini-3", "run")),
        ];
        let res = select_panel_routing(&input(&free, 2, "purpose", "acceptance", "n", &[1]));
        assert!(res.picked.iter().any(|p| p.position == 2));
        assert!(!res
            .warnings
            .iter()
            .any(|w| w.starts_with("upper tier missed:")));
        let res = select_panel_routing(&input(&members, 2, "purpose", "acceptance", "n", &[2]));
        assert!(!res
            .warnings
            .iter()
            .any(|w| w.starts_with("upper tier missed:")));
        // a light purpose owes no tier seat
        let res = select_panel_routing(&input(&members, 2, "purpose", "diff-review", "n", &[1]));
        assert!(!res
            .warnings
            .iter()
            .any(|w| w.starts_with("upper tier missed:")));
    }

    fn marks_of(members: &[MemberInput], purpose: &str, required: &[i64]) -> Vec<(i64, String)> {
        lab_seat_marks(
            &RouterContext::default(),
            &input(
                members,
                default_size(purpose),
                "purpose",
                purpose,
                "",
                required,
            ),
        )
    }

    fn mark(marks: &[(i64, String)], position: i64) -> Option<String> {
        marks
            .iter()
            .find(|(p, _)| *p == position)
            .map(|(_, m)| m.clone())
    }

    // (R4, F16-6 / F17-2) `c3 router explain` judges a lab's seat with the panel's own eligibility:
    // an unavailable entry is no lab pick (its available sibling is), a required entry holds its
    // lab's seat although a sibling weighs more, and the upper-tier entry holds it on a weighty
    // purpose when the seat is certain to go to its lab.
    #[test]
    fn explain_marks_follow_the_panels_eligibility() {
        // #3 (better rated) is unavailable: #2 represents zhipu, #3 says why it does not run
        let mut down = member(3, "ZAI", "glm-5.3-flash", "skipped");
        down.reason = "unavailable: no key".into();
        down.skip_kind = "unavailable".into();
        let members = vec![
            member(1, "mimo", "mimo-v2.6-pro", "run"),
            member(2, "ZAI", "glm-5.3", "run"),
            down,
            member(4, "ZAI", "glm-5.2", "run"),
        ];
        let marks = marks_of(&members, "diff-review", &[]);
        assert_eq!(
            mark(&marks, 3).as_deref(),
            Some("not eligible: unavailable: no key")
        );
        assert_eq!(mark(&marks, 2), None);
        assert_eq!(
            mark(&marks, 4).as_deref(),
            Some("lab zhipu: #2 takes its seat - one seat per lab")
        );
        assert_eq!(mark(&marks, 1), None);

        // a required zhipu entry lighter than its unpinned sibling (rated better) holds the seat
        let members = vec![
            member(1, "ZAI", "glm-5.2", "run"),
            member(2, "ZAI", "glm-5.3", "run"),
            member(3, "mimo", "mimo-v2.6-pro", "run"),
        ];
        let ratings = ratings_for("ZAI", "glm-5.3", "diff-review");
        let mut inp = input(&members, 2, "purpose", "diff-review", "", &[1]);
        inp.ratings = &ratings;
        let marks = lab_seat_marks(&RouterContext::default(), &inp);
        assert_eq!(
            mark(&marks, 2).as_deref(),
            Some("lab zhipu: #1 takes its seat (required) - one seat per lab")
        );
        assert_eq!(mark(&marks, 1), None);
        // and the real panel agrees, over many seeds
        for n in 1..=40 {
            let nonce = n.to_string();
            let mut inp = input(&members, 2, "purpose", "diff-review", &nonce, &[1]);
            inp.ratings = &ratings;
            let res = select_panel_routing(&inp);
            assert!(!res.picked.iter().any(|p| p.position == 2), "nonce {n}");
        }

        // a weighty purpose: the only weighty entry's lab takes the upper-tier seat with it, its
        // better-rated sibling is marked
        let members = vec![
            member(1, "ZAI", "glm-5.3-flash", "run"),
            weighty(member(2, "ZAI", "glm-5.3", "run")),
            member(3, "mimo", "mimo-v2.6-pro", "run"),
        ];
        let ratings = ratings_for("ZAI", "glm-5.3-flash", "framing");
        let mut inp = input(&members, 3, "purpose", "framing", "", &[]);
        inp.ratings = &ratings;
        let marks = lab_seat_marks(&RouterContext::default(), &inp);
        assert_eq!(
            mark(&marks, 1).as_deref(),
            Some("lab zhipu: #2 takes its seat (upper tier) - one seat per lab")
        );
        assert_eq!(mark(&marks, 2), None);
        for n in 1..=40 {
            let nonce = n.to_string();
            let mut inp = input(&members, 3, "purpose", "framing", &nonce, &[]);
            inp.ratings = &ratings;
            let res = select_panel_routing(&inp);
            assert!(res.picked.iter().any(|p| p.position == 2), "nonce {n}");
            assert!(!res.picked.iter().any(|p| p.position == 1), "nonce {n}");
        }

        // two labs with weighty entries: the routed draw decides which lab gets the tier seat
        let members = vec![
            member(1, "ZAI", "glm-5.3-flash", "run"),
            weighty(member(2, "ZAI", "glm-5.3", "run")),
            weighty(member(3, "gemini", "gemini-3", "run")),
            member(4, "ZAI", "glm-5.2", "run"),
        ];
        let ratings = ratings_for("ZAI", "glm-5.3-flash", "framing");
        let mut inp = input(&members, 3, "purpose", "framing", "", &[]);
        inp.ratings = &ratings;
        let marks = lab_seat_marks(&RouterContext::default(), &inp);
        assert_eq!(
            mark(&marks, 1).as_deref(),
            Some(
                "lab zhipu: #2 takes its seat instead when the upper-tier draw falls to lab zhipu"
            )
        );
        assert_eq!(
            mark(&marks, 2).as_deref(),
            Some("lab zhipu: #1 takes its seat unless the upper-tier draw falls to lab zhipu (then #2)")
        );
        assert_eq!(
            mark(&marks, 4).as_deref(),
            Some("lab zhipu: #1 takes its seat, #2 when the upper-tier draw falls to lab zhipu - one seat per lab")
        );
        // and the panel seats zhipu only through #1 or #2
        for n in 1..=60 {
            let nonce = n.to_string();
            let mut inp = input(&members, 3, "purpose", "framing", &nonce, &[]);
            inp.ratings = &ratings;
            let res = select_panel_routing(&inp);
            assert!(!res.picked.iter().any(|p| p.position == 4), "nonce {n}");
        }
    }

    // (R4, F16-6) A routed record keeps its scores and multipliers rounded for display and carries
    // the draw's unrounded values when rounding changed them; a record rounding did not change stays
    // the plugin's.
    #[test]
    fn a_routed_record_carries_the_draws_full_precision() {
        let mut a = member(1, "ZAI", "glm-5.3", "run");
        a.budget_mult = 0.123456789;
        let members = vec![a, member(2, "mimo", "mimo-v2.6-pro", "run")];
        let mut ratings = ratings_for("ZAI", "glm-5.3", "diff-review");
        ratings.extend(ratings_for("mimo", "mimo-v2.6-pro", "diff-review"));
        let mut inp = input(&members, 2, "-PanelSize", "diff-review", "n", &[]);
        inp.ratings = &ratings;
        let res = select_panel_routing(&inp);
        assert_eq!(res.routing.mode, "routed");
        let seated = res.members.iter().find(|m| m.position == 1).unwrap();
        let draw = res.routing.ext.as_ref().expect("ext")["c3"]["draw"].clone();
        assert_eq!(draw[0]["position"], 1);
        assert_eq!(draw[0]["mult"].as_f64(), Some(seated.mult));
        assert_eq!(draw[0]["score"].as_f64(), Some(seated.score.score));
        assert_eq!(res.routing.eligible[0].mult, round(seated.mult, 4));
        assert!(res.routing.ext.as_ref().unwrap()["c3"]
            .get("seating")
            .is_none());
        // roster order weighs nothing: no draw block
        let res = select_panel_routing(&input(&members, 2, "-PanelSize", "diff-review", "n", &[]));
        assert_eq!(res.routing.mode, "roster");
        assert!(res.routing.ext.is_none());
    }
}
