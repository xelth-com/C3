//! The model step of a maintenance pass (`C3_MAINTAIN_MODELS`): collect the available model ids
//! per source, record them (so a later pass can tell which are new), and — in `swap` mode — seat a
//! seated id's successor after it passes the same smoke, editing exactly that roster entry's `model`
//! and nothing else.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::env::{Env, GetPolicy, Redirects, HTTP_BODY_CAP};
use super::version::{parse_model_id, successor, valid_model_id};
use super::web::{is_public_source, strip_vendor};
use c3_core::roster::{Roster, RosterEntry};

/// The model ids collected from one source in a pass. Sources: `grok` / `agy` (the engines' own
/// lists), `endpoint:<canonical endpoint>|<key env name>|<plan>` (an endpoint's authenticated
/// `/v1/models`, which may be PRIVATE - [`endpoint_source`]), `web:<canonical url>` (a public
/// catalog, ids kept as `<vendor>/<model>` - [`super::web::catalog_source`]; an older build wrote
/// `web:<host>`) and `page:<name>` (a configured public page).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceList {
    pub source: String,
    pub ids: Vec<String>,
    /// The catalog's `created` stamp (unix seconds) per id, when it gives one: the first-collection
    /// "new" rule and the date evidence a public candidate needs. Persisted; a state file written
    /// before it existed loads with no dates.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub created: BTreeMap<String, i64>,
    /// (0.2.5 D3) The catalog's list price per id, when it gives one ([`ListPrice`]) - for the
    /// price check of a swap across a major ([`price_hold`]) in the pass that fetched it. Held in
    /// memory only: never written to the state file.
    #[serde(skip)]
    pub prices: BTreeMap<String, ListPrice>,
}

/// (0.2.5 D3) A catalog's list price of one id: `pricing.prompt` and `pricing.completion` (OpenRouter
/// gives USD per token), each kept only when it is a positive number. A list price is a signal of
/// what a model costs elsewhere, not the price the entry's own plan charges.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ListPrice {
    pub prompt: Option<f64>,
    pub completion: Option<f64>,
}

/// The persisted model state: the last-seen id list per source, and the successors a public source
/// once dated older than the seated id.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelsState {
    #[serde(default)]
    pub sources: Vec<SourceList>,
    /// (F04-3) Every `(seated, candidate)` pair a public source dated as not newer than the seated
    /// id. Such a candidate stays rejected for that seated id in every later pass, whatever a source
    /// says then (a catalog that re-stamps an old model later cannot smuggle it in); (F15-2,
    /// F14-2) the cap never evicts a pair whose seated id is still an active roster model
    /// ([`merge_older`]). Persisted; a state written before it existed loads with none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub older: Vec<OlderPair>,
}

/// A successor a public source dated as not newer than the seated id (both lower-cased, the
/// candidate without a vendor part).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OlderPair {
    pub seated: String,
    pub candidate: String,
}

impl OlderPair {
    pub fn new(seated: &str, candidate: &str) -> Self {
        OlderPair {
            seated: seated.to_ascii_lowercase(),
            candidate: strip_vendor(candidate).to_ascii_lowercase(),
        }
    }
}

/// The most recorded [`OlderPair`]s kept - except the protected ones ([`merge_older`]).
pub const MAX_OLDER_PAIRS: usize = 500;

/// `previous` plus the pairs found in this pass, de-duplicated, in the order they were first
/// recorded. (F15-2, F14-2) Over [`MAX_OLDER_PAIRS`], the oldest pairs are evicted - but never a
/// pair whose seated id is still an active roster model (`seated`, compared case-insensitively):
/// its refusal is what keeps a re-stamped older model out of that seat. The cap applies to the
/// other pairs, oldest first; when the protected pairs alone exceed it, they are all kept.
pub fn merge_older(
    previous: &[OlderPair],
    found: &[OlderPair],
    seated: &[String],
) -> Vec<OlderPair> {
    let mut out: Vec<OlderPair> = previous.to_vec();
    for p in found {
        if !out.contains(p) {
            out.push(p.clone());
        }
    }
    let protected = |p: &OlderPair| seated.iter().any(|s| s.eq_ignore_ascii_case(&p.seated));
    let mut excess = out.len().saturating_sub(MAX_OLDER_PAIRS);
    out.retain(|p| {
        if excess > 0 && !protected(p) {
            excess -= 1;
            false
        } else {
            true
        }
    });
    out
}

impl ModelsState {
    fn ids_of(&self, source: &str) -> &[String] {
        self.sources
            .iter()
            .find(|s| s.source == source)
            .map(|s| s.ids.as_slice())
            .unwrap_or(&[])
    }

    fn has_source(&self, source: &str) -> bool {
        self.sources.iter().any(|s| s.source == source)
    }
}

/// The source an entry's models come from (`grok` / `agy` for those engines, [`endpoint_source`]
/// for an Anthropic-compatible endpoint entry). `None` when the entry has no known model source.
pub fn source_for(e: &RosterEntry) -> Option<String> {
    let engine = if e.engine.is_empty() {
        "codex"
    } else {
        e.engine.as_str()
    };
    match engine {
        "grok" => Some("grok".to_string()),
        "agy" => Some("agy".to_string()),
        _ => e.endpoint.as_ref().map(|ep| endpoint_source(ep, &e.plan)),
    }
}

/// (F02-15) The source name of an endpoint's authenticated model list:
/// `endpoint:<scheme>://<host>:<port><path>|<key env name>|<plan>` - the lower-case scheme and host,
/// the port (the scheme's default when none is written), the path without a trailing slash, the
/// NAME of the variable that holds the token, and the entry's plan. Two entries share one list (and
/// its ids are candidates for them) only when all of it agrees - never by host alone, so a model
/// one endpoint path or credential serves is never offered to, or smoked on, another.
pub fn endpoint_source(ep: &c3_core::claude::ClaudeEndpoint, plan: &str) -> String {
    let canonical = match url::Url::parse(ep.base_url.trim()) {
        Ok(u) => format!(
            "{}://{}:{}{}",
            u.scheme().to_ascii_lowercase(),
            u.host_str().unwrap_or("").to_ascii_lowercase(),
            u.port_or_known_default().unwrap_or(0),
            u.path().trim_end_matches('/')
        ),
        Err(_) => ep.canonical.clone(),
    };
    let plan = if plan.is_empty() {
        ep.plan.as_str()
    } else {
        plan
    };
    format!("endpoint:{canonical}|{}|{plan}", ep.env_key)
}

/// The host-only name an endpoint list was recorded under before [`endpoint_source`]
/// (`endpoint:<host>`), so the first pass after the change does not announce the whole list as new.
fn legacy_endpoint_source(source: &str) -> Option<String> {
    let rest = source.strip_prefix("endpoint:")?;
    let (_, after) = rest.split_once("://")?;
    let host = after.split([':', '/', '|']).next()?;
    (!host.is_empty()).then(|| format!("endpoint:{host}"))
}

/// Collect the model ids of every source present in `roster`. `grok models` / `agy models` through
/// the launchers; each distinct endpoint list ([`endpoint_source`]: endpoint, key variable and plan)
/// through `GET <base_url>/v1/models` with its key. Every failure is ignored (an empty list for that
/// source).
pub fn collect(env: &dyn Env, roster: &Roster) -> Vec<SourceList> {
    let mut out: Vec<SourceList> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for e in roster.active_entries() {
        let Some(source) = source_for(e) else {
            continue;
        };
        if !seen.insert(source.clone()) {
            continue;
        }
        let ids = match source.as_str() {
            "grok" => super::engine::resolve_launcher("grok")
                .map(|l| parse_grok_models(&env.run(&l, &["models"], &[], None).combined()))
                .unwrap_or_default(),
            "agy" => super::engine::resolve_launcher("agy")
                .map(|l| parse_agy_models(&env.run(&l, &["models"], &[], None).combined()))
                .unwrap_or_default(),
            _ => e
                .endpoint
                .as_ref()
                .map(|ep| fetch_endpoint_models(env, &ep.base_url, &ep.env_key))
                .unwrap_or_default(),
        };
        if !ids.is_empty() {
            out.push(SourceList {
                source,
                ids,
                ..Default::default()
            });
        }
    }
    out
}

/// Parse `grok models`: lines like `  * grok-4.7 (default)` / `  - grok-4.6`; the id is the first
/// token after the `*`/`-` bullet.
pub fn parse_grok_models(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let t = line.trim_start();
        let rest = t
            .strip_prefix("* ")
            .or_else(|| t.strip_prefix("- "))
            .unwrap_or("");
        if let Some(id) = rest.split_whitespace().next() {
            if valid_model_id(id) {
                out.push(id.to_string());
            }
        }
    }
    out
}

/// Parse `agy models`: tab-separated `id\tDescription`; the id is the first column. A header line
/// (`Fetching available models...`) with no tab is skipped.
pub fn parse_agy_models(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        if let Some((id, _)) = line.split_once('\t') {
            let id = id.trim();
            if valid_model_id(id) {
                out.push(id.to_string());
            }
        }
    }
    out
}

/// GET `<base_url>/v1/models` with the endpoint key and parse `data[].id`. Anthropic- and
/// OpenAI-style auth headers are both sent. 5 s timeout; any failure yields no ids. The key value
/// is never logged.
fn fetch_endpoint_models(env: &dyn Env, base_url: &str, env_key: &str) -> Vec<String> {
    let key = std::env::var(env_key).unwrap_or_default();
    if key.trim().is_empty() {
        return Vec::new();
    }
    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    let bearer = format!("Bearer {key}");
    let headers = [
        ("x-api-key", key.as_str()),
        ("anthropic-version", "2023-06-01"),
        ("Authorization", bearer.as_str()),
    ];
    // never follow a redirect: this request carries the API key.
    let policy = GetPolicy {
        timeout: Duration::from_secs(5),
        redirects: Redirects::Never,
        cap: HTTP_BODY_CAP,
    };
    match env.http_get(&url, &headers, policy) {
        Ok((status, body)) if (200..300).contains(&status) => parse_models_json(&body),
        _ => Vec::new(),
    }
}

/// Parse a `/v1/models` body (`{"data":[{"id":...}]}`) into its ids.
pub fn parse_models_json(body: &str) -> Vec<String> {
    let data: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    data.get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("id").and_then(|v| v.as_str()))
                .filter(|id| valid_model_id(id))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// The ids present in `current` but absent from `previous` (across the engine and endpoint sources),
/// sorted and de-duplicated. Empty when `previous` has never recorded a source (the first pass is a
/// baseline). The public sources (a catalog of hundreds of ids) are left out here: what they bring
/// is announced only as a successor or as "new in the family" of a seated id ([`family_news`]).
pub fn new_ids(previous: &ModelsState, current: &[SourceList]) -> Vec<String> {
    if previous.sources.is_empty() {
        return Vec::new();
    }
    let mut out: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for s in current {
        if is_public_source(&s.source) {
            continue;
        }
        let prev = if previous.has_source(&s.source) {
            previous.ids_of(&s.source)
        } else {
            legacy_endpoint_source(&s.source)
                .map(|l| previous.ids_of(&l))
                .unwrap_or(&[])
        };
        for id in &s.ids {
            if !prev.contains(id) {
                out.insert(id.clone());
            }
        }
    }
    out.into_iter().collect()
}

/// A seated id with a successor available in its own source or in a public source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwapCandidate {
    pub position: usize,
    pub provider: String,
    pub engine: String,
    pub old: String,
    pub new: String,
    /// The successor came only from a public catalog/page, not from the entry's own source: the plan
    /// may not serve it (the smoke decides).
    pub from_public: bool,
    /// The entry is protected (`reserve_percent`) - announced only, never smoked or edited.
    pub protected: bool,
    /// The entry runs only on request - announced only, never smoked or edited.
    pub on_request: bool,
    /// (0.2.5 D3) A successor of a higher major whose catalog list price is more than
    /// [`PRICE_RATIO_LIMIT`] times the seated id's: the reason it is announced only
    /// ([`price_hold`]); `None` otherwise.
    pub price_hold: Option<String>,
}

/// The ids of every public source in `current` with the vendor part stripped (`qwen/qwen4-max` ->
/// `qwen4-max`), de-duplicated.
pub fn public_candidates(current: &[SourceList]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for s in current.iter().filter(|s| is_public_source(&s.source)) {
        for id in &s.ids {
            let m = strip_vendor(id);
            if seen.insert(m.to_string()) {
                out.push(m.to_string());
            }
        }
    }
    out
}

/// An id that the version rule would take for a successor but that the date gate refused (a public
/// id without the date evidence it needs - [`public_verdict`] - or an id a public source dates as
/// not newer - [`own_verdict`]); reported in the notes, never smoked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectedCandidate {
    pub provider: String,
    pub engine: String,
    pub old: String,
    pub new: String,
    pub why: String,
}

/// A public candidate whose seated id is not in the catalog qualifies only when the catalog created
/// it within this many days.
pub const UNANCHORED_MAX_AGE_DAYS: i64 = 180;

/// One occurrence of a (vendor-stripped) id in a public source.
struct PublicHit<'a> {
    source: &'a SourceList,
    /// The id as the source lists it (`<vendor>/<model>`).
    id: &'a str,
    vendor: &'a str,
    created: Option<i64>,
}

/// Every occurrence of `model` (compared after stripping the vendor part, case-insensitive) in the
/// public sources of `current`.
fn public_hits<'a>(current: &'a [SourceList], model: &str) -> Vec<PublicHit<'a>> {
    let mut out = Vec::new();
    for s in current.iter().filter(|s| is_public_source(&s.source)) {
        for id in &s.ids {
            if strip_vendor(id).eq_ignore_ascii_case(model) {
                out.push(PublicHit {
                    source: s,
                    id,
                    vendor: id.split_once('/').map(|(v, _)| v).unwrap_or(""),
                    created: s.created.get(id).copied(),
                });
            }
        }
    }
    out
}

/// The seated id's entry within ONE source: the one under the same vendor slug as the candidate
/// when it appears under several, else its first entry. `None` when the seated id is not in that
/// source.
fn seated_entry<'a>(source: &'a SourceList, seated: &str, vendor: &str) -> Option<&'a String> {
    let hits: Vec<&String> = source
        .ids
        .iter()
        .filter(|id| strip_vendor(id).eq_ignore_ascii_case(seated))
        .collect();
    hits.iter()
        .find(|id| id.split_once('/').map(|(v, _)| v).unwrap_or("") == vendor)
        .or_else(|| hits.first())
        .copied()
}

/// The catalog date of the seated id within ONE source ([`seated_entry`]). `None` when the seated id
/// is not in that source or carries no date there.
fn seated_created(source: &SourceList, seated: &str, vendor: &str) -> Option<i64> {
    source
        .created
        .get(seated_entry(source, seated, vendor)?)
        .copied()
}

/// (0.2.5 D3) A successor of a higher major is announced only when a public catalog lists it at
/// more than this many times the seated id's list price (prompt or completion).
pub const PRICE_RATIO_LIMIT: f64 = 2.0;

/// (0.2.5 D3) The highest ratio of `cand`'s list price to `seated`'s that a public source of
/// `current` gives - per source, the two ids' entries as the date gate pairs them, each field
/// (`prompt`, `completion`) compared only when the source prices it for both. `None` when no source
/// prices both ids.
fn list_price_ratio(current: &[SourceList], seated: &str, cand: &str) -> Option<f64> {
    let mut best: Option<f64> = None;
    for h in public_hits(current, cand) {
        let Some(c) = h.source.prices.get(h.id) else {
            continue;
        };
        let Some(s) =
            seated_entry(h.source, seated, h.vendor).and_then(|id| h.source.prices.get(id))
        else {
            continue;
        };
        for (cv, sv) in [(c.prompt, s.prompt), (c.completion, s.completion)] {
            if let (Some(cv), Some(sv)) = (cv, sv) {
                let r = cv / sv;
                if r.is_finite() && best.is_none_or(|b| r > b) {
                    best = Some(r);
                }
            }
        }
    }
    best
}

/// (0.2.5 D3) Why a successor ACROSS A MAJOR (`qwen3.8-max` -> `qwen4-max`) is only announced: a
/// public catalog prices both ids and the candidate's list price is more than
/// [`PRICE_RATIO_LIMIT`] times the seated id's in either field - the same plan may charge more for
/// it, which no smoke can tell. `None` within one major, or without prices for both ids. A list
/// price is a signal, not the plan's own price.
pub fn price_hold(current: &[SourceList], seated: &str, cand: &str) -> Option<String> {
    let (s, c) = (parse_model_id(seated)?, parse_model_id(cand)?);
    if c.major <= s.major {
        return None;
    }
    let r = list_price_ratio(current, seated, cand)?;
    (r > PRICE_RATIO_LIMIT).then(|| {
        format!(
            "list price {} times higher - seat it by hand",
            ratio_text(r)
        )
    })
}

/// A price ratio as the report says it: one decimal, `.0` dropped (`3`, `2.5`); two decimals when
/// one would round it down to the limit (`2.04`).
fn ratio_text(r: f64) -> String {
    let one = format!("{r:.1}");
    if one.parse::<f64>().is_ok_and(|v| v <= PRICE_RATIO_LIMIT) {
        return format!("{r:.2}");
    }
    one.strip_suffix(".0").map(str::to_string).unwrap_or(one)
}

/// A catalog date later than now by more than this (seconds) is not believed: such a candidate is
/// rejected (F02-14).
pub const FUTURE_SLACK_SECS: i64 = 86_400;

/// (F14-5) The clock skew a qualifying `created` stamp may have: a stamp later than now plus this
/// (seconds) is no evidence - the candidate is announced only, in every branch of the date gate,
/// anchored or not.
pub const FUTURE_SKEW_SECS: i64 = 600;

const WHY_NOT_YET: &str = "dated in the future by the catalog - announce only";

/// Why the date gate refused an id, and whether the refusal is NEGATIVE date evidence that the
/// state records ([`OlderPair`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateRefusal {
    pub why: &'static str,
    pub older: bool,
}

const WHY_OLDER: &str = "older than the seated model by the catalog's date";
const WHY_FUTURE: &str = "dated in the future by the catalog";
const WHY_RECORDED: &str = "recorded as older than the seated model in an earlier pass";

/// Whether the state recorded `cand` as older than `seated` in an earlier pass.
fn recorded_older(older: &[OlderPair], seated: &str, cand: &str) -> bool {
    let probe = OlderPair::new(seated, cand);
    older.contains(&probe)
}

/// (0.2.5 R2, F13-6) Whether the maintain state dates `cand` as NOT newer than `seated`: a public
/// source of `state` dates both ids and the candidate's `created` is not later than the seated
/// id's (the swap gate's own comparison), or an earlier pass recorded the pair as older. `false`
/// without dates for both ids. The repository `AGENTS.md` repair asks this before following a
/// successor, so it never moves to a model the catalog knows to be older.
pub fn dated_older(state: &ModelsState, seated: &str, cand: &str) -> bool {
    recorded_older(&state.older, seated, cand)
        || public_hits(&state.sources, cand).iter().any(|h| {
            matches!(
                (h.created, seated_created(h.source, seated, h.vendor)),
                (Some(c), Some(cs)) if c <= cs
            )
        })
}

/// The NEGATIVE date evidence against `cand` as a successor of `seated` in the public sources of
/// `current`: an occurrence dated in the future (beyond [`FUTURE_SLACK_SECS`]), or one dated not
/// later than the seated id in the same source. With `only_both_dated`, an occurrence counts only
/// when the same source dates the seated id as well (the gate of an id from the entry's own list).
/// "Older" wins over "future" (it is the one the state records).
fn negative_evidence(
    current: &[SourceList],
    seated: &str,
    cand: &str,
    now_unix: i64,
    only_both_dated: bool,
) -> Option<GateRefusal> {
    let mut found: Option<GateRefusal> = None;
    for h in public_hits(current, cand) {
        let Some(c) = h.created else {
            continue;
        };
        let cs = seated_created(h.source, seated, h.vendor);
        if only_both_dated && cs.is_none() {
            continue;
        }
        if matches!(cs, Some(cs) if c <= cs) {
            return Some(GateRefusal {
                why: WHY_OLDER,
                older: true,
            });
        }
        if c > now_unix + FUTURE_SLACK_SECS {
            found = Some(GateRefusal {
                why: WHY_FUTURE,
                older: false,
            });
        }
    }
    found
}

/// Whether a public id may be a swap candidate for `seated`: the smoke cannot tell a newer model
/// from an older one the plan still serves (`grok-4.20` is a 2025 model, older than `grok-4.7`), so
/// a catalog id needs DATE evidence, and negative evidence wins (F02-14, F04-3):
/// * a candidate the state recorded as older than the seated id is refused;
/// * if ANY public source dates it in the future (beyond [`FUTURE_SLACK_SECS`]) or not later than
///   the seated id it lists too, it is refused, whatever another source says;
/// * otherwise one qualifying occurrence qualifies: the seated id in the same catalog with an
///   earlier date, or - when that catalog does not list the seated id - a date within
///   [`UNANCHORED_MAX_AGE_DAYS`]. A page id (no dates), a catalog entry without `created`, and
///   (F14-5) an occurrence dated later than now plus [`FUTURE_SKEW_SECS`] never qualify, anchored
///   or not (beyond [`FUTURE_SLACK_SECS`] such a date is negative evidence as well).
fn public_verdict(
    current: &[SourceList],
    seated: &str,
    cand: &str,
    older: &[OlderPair],
    now_unix: i64,
) -> Result<(), GateRefusal> {
    if recorded_older(older, seated, cand) {
        return Err(GateRefusal {
            why: WHY_RECORDED,
            older: false,
        });
    }
    if let Some(neg) = negative_evidence(current, seated, cand, now_unix, false) {
        return Err(neg);
    }
    // reasons ranked: the most informative one found is reported
    let mut why: (u8, &'static str) = (0, "not in a public source");
    let mut note = |rank: u8, w: &'static str| {
        if rank > why.0 {
            why = (rank, w);
        }
    };
    for h in public_hits(current, cand) {
        if h.source.source.starts_with("page:") {
            note(1, "a page carries no dates - announce only");
            continue;
        }
        let Some(c) = h.created else {
            note(2, "no date in the catalog - announce only");
            continue;
        };
        // (F14-5) a stamp in the future is no evidence, whether the seated id is listed or not
        if c > now_unix + FUTURE_SKEW_SECS {
            note(4, WHY_NOT_YET);
            continue;
        }
        match seated_created(h.source, seated, h.vendor) {
            // (negative evidence was ruled out above: a listed seated id is dated earlier here)
            Some(_) => return Ok(()),
            None if c >= now_unix - UNANCHORED_MAX_AGE_DAYS * 86_400 => return Ok(()),
            None => note(3, "older than 180 days by the catalog's date"),
        }
    }
    Err(GateRefusal {
        why: why.1,
        older: false,
    })
}

/// (F04-2) The gate of an id from the entry's OWN list (engine / endpoint - what the plan offers):
/// no date is needed, but a candidate the state recorded as older is refused, and so is one that a
/// public source dating BOTH ids shows not newer than the seated id (or dates in the future).
fn own_verdict(
    current: &[SourceList],
    seated: &str,
    cand: &str,
    older: &[OlderPair],
    now_unix: i64,
) -> Result<(), GateRefusal> {
    if recorded_older(older, seated, cand) {
        return Err(GateRefusal {
            why: WHY_RECORDED,
            older: false,
        });
    }
    match negative_evidence(current, seated, cand, now_unix, true) {
        Some(neg) => Err(neg),
        None => Ok(()),
    }
}

/// [`swap_plan`]'s candidates only.
pub fn swap_candidates(
    roster: &Roster,
    current: &[SourceList],
    now_unix: i64,
) -> Vec<SwapCandidate> {
    swap_plan(roster, current, now_unix).0
}

/// [`plan_swaps`] with no recorded history: the candidates and the rejected ids.
pub fn swap_plan(
    roster: &Roster,
    current: &[SourceList],
    now_unix: i64,
) -> (Vec<SwapCandidate>, Vec<RejectedCandidate>) {
    let p = plan_swaps(roster, current, &[], now_unix);
    (p.candidates, p.rejected)
}

/// What [`plan_swaps`] found.
#[derive(Debug, Clone, Default)]
pub struct SwapPlan {
    pub candidates: Vec<SwapCandidate>,
    pub rejected: Vec<RejectedCandidate>,
    /// (F04-3) Every `(seated, id)` pair a public source dated as not newer in this pass - the pass
    /// records them in the state, so the id stays rejected later.
    pub older: Vec<OlderPair>,
}

/// The seated ids with an available successor ([`successor`]: same prefix and suffix, a higher
/// version - a higher major included), and the ids the date gate rejected. The candidates of an
/// entry are the ids of its own source (if it has one - what the plan offers - through
/// [`own_verdict`]) PLUS the public catalog ids with the vendor part stripped that pass
/// [`public_verdict`]; `older` is the history the state recorded. When the version rule's pick
/// among ALL ids fails its gate, it is returned as rejected (with the reason) and the best
/// qualifying id, if any, is the candidate. Entries of inactive plans are never touched (iterated
/// via `active_entries`); protected and on-request entries ARE returned (flagged) so the pass can
/// announce them, never smoke them. (0.2.5 D3) A successor across a major that a catalog prices at
/// more than twice the seated id is flagged too ([`price_hold`]); an entry whose id the catalogs
/// spell without its leading lab or plan token is matched through [`catalog_alias`].
pub fn plan_swaps(
    roster: &Roster,
    current: &[SourceList],
    older: &[OlderPair],
    now_unix: i64,
) -> SwapPlan {
    let public = public_candidates(current);
    let mut plan = SwapPlan::default();
    for e in roster.active_entries() {
        if e.model.is_empty() {
            continue;
        }
        match catalog_alias(e, current) {
            Some(alias) => {
                let view = aliased_view(current, &alias);
                let public = public_candidates(&view);
                plan_entry(e, &view, &public, older, now_unix, &mut plan);
            }
            None => plan_entry(e, current, &public, older, now_unix, &mut plan),
        }
    }
    plan
}

/// (0.2.5 D3) How a roster id is spelled in the public catalogs when they do not list it as it is:
/// the plan writes a leading token (`kimi-` in `kimi-k3.1-pro`, `kimi-code/` in
/// `kimi-code/k3.1-pro`) that the catalog leaves out (`moonshotai/k3.1-pro`). `prefix` is that
/// token with its separator; `vendors` are the catalog's vendor slugs that list the rest - the
/// maker's ids as the catalog groups them (`moonshotai`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogAlias {
    pub prefix: String,
    pub vendors: Vec<String>,
}

/// (0.2.5 D3) The [`CatalogAlias`] of an entry, or `None`. Tried only when no public source lists
/// the roster id as it is, and only once: the leading token (up to the first `/`, else the first
/// `-`) is dropped when it names the maker of the rest or its plan's vendor - by the panel's lab
/// table ([`crate::panel::plan::lab_named_by`] / [`crate::panel::plan::model_lab`]; the plan's vendor
/// is the lab its plan slug names or its endpoint host's lab) - and the rest must be an id of a
/// known maker that a public source lists under a vendor slug, an id of that same maker. Anything
/// else is no match: nothing is guessed.
pub fn catalog_alias(e: &RosterEntry, current: &[SourceList]) -> Option<CatalogAlias> {
    use crate::panel::plan::{lab_named_by, model_lab};
    let model = e.model.as_str();
    if !public_hits(current, model).is_empty() {
        return None; // the catalog spells it as the plan does
    }
    let cut = model.find('/').or_else(|| model.find('-'))?;
    let (token, rest) = (&model[..cut], &model[cut + 1..]);
    if token.is_empty() || rest.is_empty() || !valid_model_id(rest) {
        return None;
    }
    let lab = model_lab(rest);
    let token_lab = lab_named_by(token);
    if lab.is_empty() || token_lab.is_empty() {
        return None;
    }
    let host_lab = e
        .endpoint
        .as_ref()
        .map(|ep| c3_core::claude::lab_of_host(&ep.host_name))
        .unwrap_or("");
    if token_lab != lab && token_lab != lab_named_by(&e.plan) && token_lab != host_lab {
        return None;
    }
    let mut vendors: Vec<String> = Vec::new();
    for id in current
        .iter()
        .filter(|s| is_public_source(&s.source))
        .flat_map(|s| s.ids.iter())
    {
        if let Some((vendor, m)) = id.split_once('/') {
            if m.eq_ignore_ascii_case(rest)
                && model_lab(id) == lab
                && !vendors.iter().any(|v| v == vendor)
            {
                vendors.push(vendor.to_string());
            }
        }
    }
    (!vendors.is_empty()).then(|| CatalogAlias {
        prefix: model[..=cut].to_string(),
        vendors,
    })
}

/// (0.2.5 D3) `current` as the entry's plan spells it: every id a public source lists under one of
/// the alias's vendor slugs gets the alias prefix put back (`moonshotai/k3.2-pro` ->
/// `moonshotai/kimi-k3.2-pro`), its date and price keyed the same way; every other id and source
/// stays as it is. The date gate, the successor rule and the swap then see the plan's own spelling,
/// exactly as for an id the catalog lists directly.
fn aliased_view(current: &[SourceList], alias: &CatalogAlias) -> Vec<SourceList> {
    let spell = |id: &String| -> String {
        match id.split_once('/') {
            Some((vendor, m)) if alias.vendors.iter().any(|v| v == vendor) => {
                let t = format!("{vendor}/{}{m}", alias.prefix);
                if valid_model_id(&t) {
                    t
                } else {
                    id.clone()
                }
            }
            _ => id.clone(),
        }
    };
    current
        .iter()
        .map(|s| {
            if !is_public_source(&s.source) {
                return s.clone();
            }
            SourceList {
                source: s.source.clone(),
                ids: s.ids.iter().map(spell).collect(),
                created: s.created.iter().map(|(k, v)| (spell(k), *v)).collect(),
                prices: s.prices.iter().map(|(k, v)| (spell(k), *v)).collect(),
            }
        })
        .collect()
}

/// [`plan_swaps`] for one entry, over `current` (the pass's sources, or the entry's
/// [`aliased_view`]) and its public candidates `public`.
fn plan_entry(
    e: &RosterEntry,
    current: &[SourceList],
    public: &[String],
    older: &[OlderPair],
    now_unix: i64,
    plan: &mut SwapPlan,
) {
    let engine = if e.engine.is_empty() {
        "codex".to_string()
    } else {
        e.engine.clone()
    };
    let own: &[String] = source_for(e)
        .and_then(|source| current.iter().find(|s| s.source == source))
        .map(|s| s.ids.as_slice())
        .unwrap_or(&[]);
    let mut all: Vec<String> = own.to_vec();
    all.extend(public.iter().filter(|p| !own.contains(p)).cloned());
    // Only the ids the version rule takes for a successor of the seated id are gated (and can
    // be recorded as older) - each once.
    let succ_ids: Vec<String> = all
        .iter()
        .filter(|id| successor(&e.model, std::slice::from_ref(*id)).is_some())
        .cloned()
        .collect();
    let verdicts: Vec<Result<(), GateRefusal>> = succ_ids
        .iter()
        .map(|id| {
            if own.contains(id) {
                own_verdict(current, &e.model, id, older, now_unix)
            } else {
                public_verdict(current, &e.model, id, older, now_unix)
            }
        })
        .collect();
    let verdict_of = |id: &str| -> Result<(), GateRefusal> {
        succ_ids
            .iter()
            .position(|x| x == id)
            .map(|i| verdicts[i])
            .unwrap_or(Ok(()))
    };
    for (id, v) in succ_ids.iter().zip(&verdicts) {
        if matches!(v, Err(r) if r.older) {
            let pair = OlderPair::new(&e.model, id);
            if !plan.older.contains(&pair) {
                plan.older.push(pair);
            }
        }
    }
    let cands: Vec<String> = succ_ids
        .iter()
        .zip(&verdicts)
        .filter(|(_, v)| v.is_ok())
        .map(|(id, _)| id.clone())
        .collect();
    if let Some(raw) = successor(&e.model, &all) {
        if let Err(r) = verdict_of(raw) {
            plan.rejected.push(RejectedCandidate {
                provider: e.provider.clone(),
                engine: engine.clone(),
                old: e.model.clone(),
                new: raw.to_string(),
                why: r.why.to_string(),
            });
        }
    }
    if let Some(succ) = successor(&e.model, &cands) {
        plan.candidates.push(SwapCandidate {
            position: e.position,
            provider: e.provider.clone(),
            engine,
            old: e.model.clone(),
            new: succ.to_string(),
            from_public: !own.iter().any(|o| o == succ),
            protected: e.reserve_percent > 0,
            on_request: e.on_request,
            price_hold: price_hold(current, &e.model, succ),
        });
    }
}

/// A "new in the family" announcement: public ids not seen in the previous pass that share the
/// seated id's prefix, carry a version not lower than it and a DIFFERENT suffix (seated
/// `gemini-3.8-flash-high`, catalog `gemini-4-hydrogen`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyNews {
    pub provider: String,
    pub seated: String,
    pub ids: Vec<String>,
}

/// The most "new in the family" ids announced per seated entry.
pub const FAMILY_NEWS_PER_ENTRY: usize = 5;
/// On the very first collection of a public source, only ids whose catalog `created` lies within
/// this many days count as new (so the first run does not announce the whole catalog).
pub const FIRST_COLLECTION_DAYS: i64 = 30;

/// The public ids that are new in this pass: per public source, the ids absent from the previous
/// record of that source; on the source's FIRST collection, only those whose `created` stamp is
/// within [`FIRST_COLLECTION_DAYS`] of `now_unix` (an id without a stamp - a page - is not new then).
/// Returned with the vendor part stripped, de-duplicated.
pub fn new_public_ids(
    previous: &ModelsState,
    current: &[SourceList],
    now_unix: i64,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let floor = now_unix - FIRST_COLLECTION_DAYS * 86_400;
    for s in current.iter().filter(|s| is_public_source(&s.source)) {
        let first = !previous.has_source(&s.source);
        let prev = previous.ids_of(&s.source);
        for id in &s.ids {
            let is_new = if first {
                s.created.get(id).is_some_and(|c| *c >= floor)
            } else {
                !prev.contains(id)
            };
            let m = strip_vendor(id);
            if is_new && seen.insert(m.to_string()) {
                out.push(m.to_string());
            }
        }
    }
    out
}

/// The "new in the family" ids per seated, active entry (protected and on-request entries
/// included: an announcement spends nothing). An id is new in the family of a seated id when it
/// is new in this pass ([`new_public_ids`]), has the same prefix, a version not lower and a
/// different suffix - and is not OLDER than the seated id by the catalog's date when both dates are
/// known. At most [`FAMILY_NEWS_PER_ENTRY`] per entry, highest version first; an entry seated twice
/// (same model) is announced once, and an id is announced once per provider: the entries are taken
/// most specific (longest seated id) first, so `glm-5.3-flashx-legal` goes to `glm-5.3-flash`, not
/// also to `glm-5.3`.
pub fn family_news(
    roster: &Roster,
    previous: &ModelsState,
    current: &[SourceList],
    now_unix: i64,
) -> Vec<FamilyNews> {
    let fresh = new_public_ids(previous, current, now_unix);
    let mut out: Vec<FamilyNews> = Vec::new();
    let mut entries: Vec<&RosterEntry> = roster.active_entries().collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.model.len()));
    for e in entries {
        if e.model.is_empty() || out.iter().any(|n| n.seated == e.model) {
            continue;
        }
        let Some(base) = parse_model_id(&e.model) else {
            continue;
        };
        let older_by_date = |id: &str| {
            public_hits(current, id).iter().any(|h| {
                matches!((h.created, seated_created(h.source, &e.model, h.vendor)),
                    (Some(c), Some(cs)) if c < cs)
            })
        };
        let taken = |id: &str| {
            out.iter()
                .any(|n| n.provider == e.provider && n.ids.iter().any(|x| x == id))
        };
        let mut hits: Vec<(&String, (u64, u64))> = fresh
            .iter()
            .filter(|id| **id != e.model)
            .filter_map(|id| parse_model_id(id).map(|p| (id, p)))
            .filter(|(_, p)| {
                p.prefix == base.prefix && p.suffix != base.suffix && p.version() >= base.version()
            })
            .filter(|(id, _)| !older_by_date(id) && !taken(id))
            .map(|(id, p)| (id, p.version()))
            .collect();
        if hits.is_empty() {
            continue;
        }
        hits.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        out.push(FamilyNews {
            provider: e.provider.clone(),
            seated: e.model.clone(),
            ids: hits
                .into_iter()
                .take(FAMILY_NEWS_PER_ENTRY)
                .map(|(id, _)| id.clone())
                .collect(),
        });
    }
    out
}

/// (F02-19) The published model names telemetry may send, each bound to the vendor class its
/// source attributes it to: `(class, lower-case model id)`.
pub type Published = std::collections::HashSet<(String, String)>;

/// The published model names the maintain state holds: every id of a TRUSTED public source
/// ([`super::web::trusted_sources`]: the built-in catalog, or a configured catalog / page declared
/// `"public": true` - never an endpoint's own list, an engine's list or an undeclared configured
/// source, which may be private), bound to a vendor class: a `<vendor>/<model>` id to the class of
/// its OpenRouter vendor slug (`qwen/qwen4-max` -> `(alibaba, qwen4-max)`; an unknown slug binds
/// nowhere), a plain id to the class of the source's own host ([`super::web::TrustedSource`]), or
/// nowhere. Lower-cased. Empty when there is no state. (F16-1, F17-5) A source matches by its full
/// name - a catalog by its exact URL - so an id counts only when the URL it was read from is
/// trusted itself; a legacy host-keyed record of an older build matches nothing until a fetch
/// rewrites it.
pub fn published_from(state: &ModelsState, trusted: &[super::web::TrustedSource]) -> Published {
    let mut out = Published::new();
    for s in state.sources.iter().filter(|s| is_public_source(&s.source)) {
        let Some(t) = trusted.iter().find(|t| t.source == s.source) else {
            continue;
        };
        for id in &s.ids {
            let id = id.to_ascii_lowercase();
            let pair = match id.split_once('/') {
                Some((slug, model)) => crate::telemetry::classes::OPENROUTER_VENDORS
                    .iter()
                    .find(|(s, _)| *s == slug)
                    .map(|(_, class)| (class.to_string(), model.to_string())),
                None => t.class.map(|c| (c.to_string(), id.clone())),
            };
            if let Some(p) = pair.filter(|(_, m)| valid_model_id(m)) {
                // (0.2.5 D3) the catalog spells a Claude id with dots (`anthropic/claude-opus-4.5`),
                // the claude engine with dashes (`claude-opus-4-5`): an id under the `anthropic/`
                // slug is published in its dash form too when that form is of the Claude shape
                if id.starts_with("anthropic/") {
                    if let Some(dashed) = claude_dash_form(&p.1) {
                        out.insert((p.0.clone(), dashed));
                    }
                }
                out.insert(p);
            }
        }
    }
    out
}

/// (0.2.5 D3) The dash form of a catalog's Claude id: `claude-<family>-<version>` with the dots of
/// the version part turned into dashes (`claude-opus-4.5` -> `claude-opus-4-5`), returned only when
/// it differs and is of the published Claude shape ([`c3_core::claude::is_published_claude_shape`]:
/// `claude-(fable|opus|sonnet|haiku)-<digits>[-<digits>]`); `None` otherwise (`claude-3.5-sonnet`,
/// an id already in dash form).
pub fn claude_dash_form(model: &str) -> Option<String> {
    let rest = model.strip_prefix("claude-")?;
    let (family, version) = rest.split_once('-')?;
    let dashed = format!("claude-{family}-{}", version.replace('.', "-"));
    (dashed != model && c3_core::claude::is_published_claude_shape(&dashed)).then_some(dashed)
}

/// [`published_from`] of the maintain state on disk and the C3 config's trusted sources (a config
/// that does not load trusts the built-in catalog only), read once per process (telemetry asks per
/// event). Always empty in this crate's unit tests, so they never depend on the machine's state.
pub fn published_ids() -> &'static Published {
    static P: std::sync::OnceLock<Published> = std::sync::OnceLock::new();
    P.get_or_init(|| {
        if cfg!(test) {
            return Default::default();
        }
        let cfg = crate::c3config::load().ok().flatten();
        let trusted = super::web::trusted_sources(cfg.as_ref());
        super::state_dir()
            .map(|d| published_from(&super::read_state(&d).models, &trusted))
            .unwrap_or_default()
    })
}

/// Seat a successor by editing exactly the `model` of `reviewers[position]` (1-based, the roster
/// position) in the roster file, keeping the file's formatting: copy to
/// `<path>.bak-maintain-<yyyymmdd>`, replace that one string value ([`replace_entry_model`]),
/// re-parse to verify the result still loads, and write it. The backup is left in place; on a
/// verify failure nothing is written and the backup stands. `today` is `yyyymmdd`.
///
/// (R3) `basis` is the roster TEXT as it was read immediately before the candidate's smoke - the
/// same snapshot the protection / saturation gate judged. The edit is computed from THAT text and
/// the on-disk file is required to still equal it, so a concurrent edit during the minutes-long
/// smoke (protecting the entry, or re-pointing its old model to another provider) aborts the swap
/// instead of being applied over a stale pass-start view.
pub fn apply_swap(
    roster_path: &str,
    basis: &str,
    position: usize,
    provider: &str,
    old: &str,
    new: &str,
    today: &str,
) -> Result<(), String> {
    // The successor id is untrusted (it came from a model-list endpoint / CLI): refuse anything
    // outside the strict shape before it is spliced into the roster JSON.
    if !valid_model_id(new) {
        return Err(format!(
            "the successor id {new:?} is not a valid model id; not seated"
        ));
    }
    // Abort if the roster changed since the pre-smoke snapshot the gate judged.
    let current = std::fs::read_to_string(roster_path)
        .map_err(|e| format!("could not read the roster ({e})"))?;
    if current != basis {
        return Err("roster changed during the smoke; the swap was not applied".to_string());
    }
    // The edit is computed from the gated snapshot, so it targets exactly that entry.
    let updated = replace_entry_model(basis, position, provider, old, new)?;
    // Re-parse to verify the edited roster still loads before writing it.
    let home = std::env::var("USERPROFILE")
        .ok()
        .or_else(|| std::env::var("HOME").ok());
    let validated = c3_core::roster::validate_roster(roster_path, &updated, home.as_deref());
    if !validated.error.is_empty() {
        return Err(format!(
            "the edited roster did not load ({}); the backup stands, nothing written",
            validated.error
        ));
    }
    commit_swap(roster_path, basis, &updated, today)
}

/// Commit an edited roster: take the once-per-day backup, re-read the file and abort if it changed
/// since `basis` was read (a concurrent edit during the minutes-long smoke window), then write
/// through a temp file + rename so a crash never leaves a truncated roster. Separated from
/// [`apply_swap`] so the concurrency abort and the atomic write are unit-testable.
fn commit_swap(roster_path: &str, basis: &str, updated: &str, today: &str) -> Result<(), String> {
    // One backup per day, taken before the day's first swap: a second swap of the same pass must
    // not overwrite it with an already edited roster.
    let backup = format!("{roster_path}.bak-maintain-{today}");
    if !std::path::Path::new(&backup).exists() {
        std::fs::write(&backup, basis.as_bytes())
            .map_err(|e| format!("could not write the roster backup ({e})"))?;
    }
    // Re-read immediately before writing: if the roster changed since the edit was computed, abort
    // rather than clobber a concurrent edit (the backup stands, nothing written).
    let current = std::fs::read_to_string(roster_path)
        .map_err(|e| format!("could not re-read the roster ({e})"))?;
    if current != basis {
        return Err("roster changed during the pass; the swap was not applied".to_string());
    }
    c3_core::store::write_text_atomic(std::path::Path::new(roster_path), updated.as_bytes())
        .map_err(|e| format!("could not write the roster ({e})"))
}

/// (F02-17) Replace the `model` of `reviewers[position]` (1-based: the roster position of a
/// plugin-visible entry) with `new`, leaving the rest of the file byte-for-byte unchanged. The
/// value is located by PARSING ([`reviewer_model_span`]: the top-level `reviewers` member, that
/// element, its `model` member - never a text search, so an escaped model, an `ext` object holding
/// the same string, or another entry with the same model cannot be hit). The element must still
/// name `provider` and `old` (decoded). The whole string literal is replaced by `new` serialised as
/// JSON, and the result is verified SEMANTICALLY: re-parsed, it must equal the old roster with only
/// `reviewers[position].model` changed. Any mismatch is an error (the swap is skipped, never
/// guessed).
pub fn replace_entry_model(
    text: &str,
    position: usize,
    provider: &str,
    old: &str,
    new: &str,
) -> Result<String, String> {
    if !valid_model_id(new) {
        return Err(format!("the successor id {new:?} is not a valid model id"));
    }
    let idx = position
        .checked_sub(1)
        .ok_or_else(|| "roster position 0 does not exist".to_string())?;
    let before: serde_json::Value =
        serde_json::from_str(text).map_err(|_| "the roster does not parse".to_string())?;
    let entry = before
        .get("reviewers")
        .and_then(|r| r.as_array())
        .and_then(|a| a.get(idx))
        .ok_or_else(|| format!("the roster has no reviewers entry {position}"))?;
    if entry.get("provider").and_then(|p| p.as_str()) != Some(provider) {
        return Err(format!(
            "reviewers entry {position} is no longer provider {provider}"
        ));
    }
    let (vstart, vend) = reviewer_model_span(text, idx)?;
    let current: String = serde_json::from_str(&text[vstart..vend])
        .map_err(|_| format!("the model of reviewers entry {position} does not decode"))?;
    if current != old {
        return Err(format!(
            "the model of reviewers entry {position} is no longer {old}"
        ));
    }
    let literal = serde_json::to_string(new).map_err(|e| e.to_string())?;
    let mut out = String::with_capacity(text.len() + literal.len());
    out.push_str(&text[..vstart]);
    out.push_str(&literal);
    out.push_str(&text[vend..]);
    // The semantic delta: exactly reviewers[position].model changed, nothing else.
    let mut expected = before.clone();
    expected["reviewers"][idx]["model"] = serde_json::Value::String(new.to_string());
    let after: serde_json::Value = serde_json::from_str(&out)
        .map_err(|_| "the edited roster does not parse; nothing written".to_string())?;
    if after != expected {
        return Err(format!(
            "the edit would change more than the model of reviewers entry {position}; nothing written"
        ));
    }
    Ok(out)
}

/// (F02-17) The byte span (quotes included) of the `model` string of `reviewers[idx]` in a roster
/// text, found by parsing: the top-level object's single `reviewers` member, its element `idx`, that
/// object's single `model` member, which must be a string. A repeated key on that path is refused
/// (a parser would take the last one; the edit must never guess).
fn reviewer_model_span(text: &str, idx: usize) -> Result<(usize, usize), String> {
    let b = text.as_bytes();
    let top = json_ws(b, 0);
    let members = json_object_members(text, top)?;
    let single =
        |members: &[(String, (usize, usize))], key: &str| -> Result<(usize, usize), String> {
            let hits: Vec<(usize, usize)> = members
                .iter()
                .filter(|(k, _)| k == key)
                .map(|(_, s)| *s)
                .collect();
            match hits.as_slice() {
                [one] => Ok(*one),
                [] => Err(format!("no {key} member")),
                _ => Err(format!("the {key} member is repeated")),
            }
        };
    let reviewers = single(&members, "reviewers")?;
    let elements = json_array_elements(text, reviewers.0)?;
    let element = *elements
        .get(idx)
        .ok_or_else(|| format!("the roster has no reviewers entry {}", idx + 1))?;
    let fields = json_object_members(text, element.0)?;
    let model = single(&fields, "model")?;
    if b.get(model.0) != Some(&b'"') {
        return Err(format!(
            "the model of reviewers entry {} is not a string",
            idx + 1
        ));
    }
    Ok(model)
}

/// The deepest nesting the roster scanner follows.
const JSON_MAX_DEPTH: usize = 64;

fn json_ws(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\n' | b'\r') {
        i += 1;
    }
    i
}

/// The end (exclusive) of the JSON string starting at `i` (a `"`).
fn json_string_end(b: &[u8], i: usize) -> Result<usize, String> {
    if b.get(i) != Some(&b'"') {
        return Err("a string was expected".to_string());
    }
    let mut k = i + 1;
    while k < b.len() {
        match b[k] {
            b'\\' => k += 2,
            b'"' => return Ok(k + 1),
            _ => k += 1,
        }
    }
    Err("an unterminated string".to_string())
}

/// The end (exclusive) of the JSON value starting at `i`.
fn json_value_end(text: &str, i: usize, depth: usize) -> Result<usize, String> {
    let b = text.as_bytes();
    if depth > JSON_MAX_DEPTH {
        return Err("the roster nests too deeply".to_string());
    }
    match b.get(i) {
        Some(b'"') => json_string_end(b, i),
        Some(b'{') => json_object_members_at(text, i, depth).map(|(_, end)| end),
        Some(b'[') => json_array_elements_at(text, i, depth).map(|(_, end)| end),
        Some(_) => {
            let mut k = i;
            while k < b.len() && !matches!(b[k], b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r')
            {
                k += 1;
            }
            if k == i {
                Err("a value was expected".to_string())
            } else {
                Ok(k)
            }
        }
        None => Err("the roster ends early".to_string()),
    }
}

/// The members (decoded key, value span) of the object starting at `i`.
fn json_object_members(text: &str, i: usize) -> Result<Members, String> {
    json_object_members_at(text, i, 0).map(|(m, _)| m)
}

type Members = Vec<(String, (usize, usize))>;

fn json_object_members_at(text: &str, i: usize, depth: usize) -> Result<(Members, usize), String> {
    let b = text.as_bytes();
    if b.get(i) != Some(&b'{') {
        return Err("an object was expected".to_string());
    }
    let mut out = Vec::new();
    let mut k = json_ws(b, i + 1);
    if b.get(k) == Some(&b'}') {
        return Ok((out, k + 1));
    }
    loop {
        let ks = k;
        let ke = json_string_end(b, ks)?;
        let key: String =
            serde_json::from_str(&text[ks..ke]).map_err(|_| "a key does not decode".to_string())?;
        k = json_ws(b, ke);
        if b.get(k) != Some(&b':') {
            return Err("a ':' was expected".to_string());
        }
        let vs = json_ws(b, k + 1);
        let ve = json_value_end(text, vs, depth + 1)?;
        out.push((key, (vs, ve)));
        k = json_ws(b, ve);
        match b.get(k) {
            Some(b',') => k = json_ws(b, k + 1),
            Some(b'}') => return Ok((out, k + 1)),
            _ => return Err("a ',' or '}' was expected".to_string()),
        }
    }
}

/// The element spans of the array starting at `i`.
fn json_array_elements(text: &str, i: usize) -> Result<Vec<(usize, usize)>, String> {
    json_array_elements_at(text, i, 0).map(|(e, _)| e)
}

fn json_array_elements_at(
    text: &str,
    i: usize,
    depth: usize,
) -> Result<(Vec<(usize, usize)>, usize), String> {
    let b = text.as_bytes();
    if b.get(i) != Some(&b'[') {
        return Err("an array was expected".to_string());
    }
    let mut out = Vec::new();
    let mut k = json_ws(b, i + 1);
    if b.get(k) == Some(&b']') {
        return Ok((out, k + 1));
    }
    loop {
        let ve = json_value_end(text, k, depth + 1)?;
        out.push((k, ve));
        k = json_ws(b, ve);
        match b.get(k) {
            Some(b',') => k = json_ws(b, k + 1),
            Some(b']') => return Ok((out, k + 1)),
            _ => return Err("a ',' or ']' was expected".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_grok_and_agy_model_lists() {
        let grok = "You are logged in.\n\nDefault model: grok-4.7\n\nAvailable models:\n  * grok-4.7 (default)\n  - grok-4.6\n  - grok-4.5\n";
        assert_eq!(
            parse_grok_models(grok),
            vec!["grok-4.7", "grok-4.6", "grok-4.5"]
        );
        let agy = "Fetching available models...\ngemini-3.8-flash-high\tGemini 3.8 Flash (High)\nclaude-sonnet-4-6\tClaude Sonnet 4.6\n";
        assert_eq!(
            parse_agy_models(agy),
            vec!["gemini-3.8-flash-high", "claude-sonnet-4-6"]
        );
    }

    #[test]
    fn parses_models_endpoint_json() {
        let body = r#"{"data":[{"id":"glm-5.3"},{"id":"glm-5.4"}],"object":"list"}"#;
        assert_eq!(parse_models_json(body), vec!["glm-5.3", "glm-5.4"]);
        assert!(parse_models_json("not json").is_empty());
    }

    fn sl(source: &str, ids: &[&str]) -> SourceList {
        SourceList {
            source: source.into(),
            ids: ids.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn new_ids_are_baseline_then_diff() {
        let prev = ModelsState::default();
        let cur = vec![sl("grok", &["grok-4.7"])];
        assert!(new_ids(&prev, &cur).is_empty(), "first pass is a baseline");
        let prev = ModelsState {
            sources: vec![sl("grok", &["grok-4.7"])],
            ..Default::default()
        };
        let cur = vec![
            sl("grok", &["grok-4.7", "grok-5.0"]),
            // a public catalog's new ids are not listed here (only as successor / family news)
            sl("web:openrouter.ai", &["x-ai/grok-9"]),
        ];
        assert_eq!(new_ids(&prev, &cur), vec!["grok-5.0"]);
    }

    fn entry(provider: &str, engine: &str, model: &str) -> RosterEntry {
        RosterEntry {
            provider: provider.into(),
            engine: engine.into(),
            model: model.into(),
            ..Default::default()
        }
    }

    fn alibaba_endpoint_entry() -> RosterEntry {
        RosterEntry {
            position: 1,
            endpoint: Some(c3_core::claude::ClaudeEndpoint {
                base_url: "https://alibaba.example/apps/anthropic".into(),
                host_name: "alibaba.example".into(),
                env_key: "NO_KEY".into(),
                ..Default::default()
            }),
            ..entry("alibaba-claude", "claude", "qwen3.8-max")
        }
    }

    #[test]
    fn a_catalog_offers_a_successor_to_an_entry_without_its_own_list() {
        let mut protected = entry("astra", "codex", "gpt-6-astra");
        protected.reserve_percent = 15;
        let roster = Roster {
            exists: true,
            entries: vec![alibaba_endpoint_entry(), protected],
            ..Default::default()
        };
        let now = 1_795_000_000;
        let cur = vec![dated(
            "web:openrouter.ai",
            &[
                ("qwen/qwen4-max", Some(at(1, now))),
                ("qwen/qwen4-plus", Some(at(1, now))),
                ("openai/gpt-7-astra", Some(at(5, now))),
            ],
        )];
        let c = swap_candidates(&roster, &cur, now);
        assert_eq!(c.len(), 2, "{c:?}");
        assert_eq!(c[0].old, "qwen3.8-max");
        assert_eq!(c[0].new, "qwen4-max");
        assert!(c[0].from_public);
        assert!(!c[0].protected);
        // the protected entry is returned flagged (announced, never smoked)
        assert_eq!(c[1].new, "gpt-7-astra");
        assert!(c[1].protected);
        // the entry's own list wins the origin when it lists the successor too
        let own = source_for(&alibaba_endpoint_entry()).unwrap();
        let cur = vec![
            sl(&own, &["qwen4-max"]),
            sl("web:openrouter.ai", &["qwen/qwen4-max"]),
        ];
        let c = swap_candidates(&roster, &cur, now);
        assert!(!c[0].from_public);
    }

    fn at(days_ago: i64, now: i64) -> i64 {
        now - days_ago * 86_400
    }

    /// A source with a `created` date per id (`None`: the catalog gives none).
    fn dated(source: &str, ids: &[(&str, Option<i64>)]) -> SourceList {
        let mut s = sl(source, &ids.iter().map(|(i, _)| *i).collect::<Vec<_>>());
        for (id, c) in ids {
            if let Some(c) = c {
                s.created.insert(id.to_string(), *c);
            }
        }
        s
    }

    fn roster_of(entries: Vec<RosterEntry>) -> Roster {
        Roster {
            exists: true,
            entries,
            ..Default::default()
        }
    }

    #[test]
    fn a_catalog_id_older_than_the_seated_one_by_date_is_never_a_candidate() {
        // the live case: the vendor's grok-4.20 (2025) is numerically above grok-4.7 but older
        let now = 1_795_000_000;
        let roster = roster_of(vec![entry("grok", "grok", "grok-4.7")]);
        let cur = vec![dated(
            "web:openrouter.ai",
            &[
                ("x-ai/grok-4.7", Some(at(30, now))),
                ("x-ai/grok-4.20", Some(at(300, now))),
                ("x-ai/grok-4.8", Some(at(2, now))),
            ],
        )];
        let (c, rejected) = swap_plan(&roster, &cur, now);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].new, "grok-4.8");
        assert_eq!(rejected.len(), 1, "{rejected:?}");
        assert_eq!(rejected[0].new, "grok-4.20");
        assert_eq!(
            rejected[0].why,
            "older than the seated model by the catalog's date"
        );
        // without grok-4.8, nothing qualifies and grok-4.20 is still only reported
        let cur = vec![dated(
            "web:openrouter.ai",
            &[
                ("x-ai/grok-4.7", Some(at(30, now))),
                ("x-ai/grok-4.20", Some(at(300, now))),
            ],
        )];
        let (c, rejected) = swap_plan(&roster, &cur, now);
        assert!(c.is_empty());
        assert_eq!(rejected.len(), 1);
        // the entry's OWN list keeps today's rule (no dates needed) while no public source dates
        // both ids
        let cur = vec![
            sl("grok", &["grok-4.7", "grok-4.20"]),
            dated("web:openrouter.ai", &[("x-ai/grok-4.7", Some(at(30, now)))]),
        ];
        assert_eq!(swap_candidates(&roster, &cur, now)[0].new, "grok-4.20");
    }

    /// (F04-2) An id from the entry's OWN list goes through the date gate when a public source
    /// dates both ids: `grok models` listing `grok-4.20` beside `grok-4.7` no longer seats it when
    /// the catalog dates it older; a newer own id still wins without a date.
    #[test]
    fn an_own_list_id_a_catalog_dates_older_is_not_a_candidate() {
        let now = 1_795_000_000;
        let roster = roster_of(vec![entry("grok", "grok", "grok-4.7")]);
        let cur = vec![
            sl("grok", &["grok-4.7", "grok-4.20", "grok-4.8"]),
            dated(
                "web:openrouter.ai",
                &[
                    ("x-ai/grok-4.7", Some(at(30, now))),
                    ("x-ai/grok-4.20", Some(at(300, now))),
                ],
            ),
        ];
        let p = plan_swaps(&roster, &cur, &[], now);
        assert_eq!(p.candidates.len(), 1, "{:?}", p.candidates);
        assert_eq!(p.candidates[0].new, "grok-4.8");
        assert!(!p.candidates[0].from_public);
        assert_eq!(p.rejected.len(), 1, "{:?}", p.rejected);
        assert_eq!(p.rejected[0].new, "grok-4.20");
        assert_eq!(p.rejected[0].why, WHY_OLDER);
        assert_eq!(p.older, vec![OlderPair::new("grok-4.7", "grok-4.20")]);
        // only the older own id: no candidate at all
        let cur = vec![
            sl("grok", &["grok-4.7", "grok-4.20"]),
            dated(
                "web:openrouter.ai",
                &[
                    ("x-ai/grok-4.7", Some(at(30, now))),
                    ("x-ai/grok-4.20", Some(at(300, now))),
                ],
            ),
        ];
        assert!(plan_swaps(&roster, &cur, &[], now).candidates.is_empty());
    }

    /// (F02-14) A candidate dated in the future (beyond a day) is rejected, whether the seated id is
    /// in the catalog or not; (F14-5) a date within the day is no evidence either (announced only),
    /// only a stamp within ten minutes of now is believed.
    #[test]
    fn a_future_dated_candidate_is_rejected() {
        let now = 1_795_000_000;
        let roster = roster_of(vec![entry("grok", "grok", "grok-4.7")]);
        for cur in [
            vec![dated(
                "web:openrouter.ai",
                &[
                    ("x-ai/grok-4.7", Some(at(30, now))),
                    ("x-ai/grok-4.8", Some(now + 3 * 86_400)),
                ],
            )],
            vec![dated(
                "web:openrouter.ai",
                &[("x-ai/grok-4.8", Some(now + 3 * 86_400))],
            )],
        ] {
            let p = plan_swaps(&roster, &cur, &[], now);
            assert!(p.candidates.is_empty(), "{:?}", p.candidates);
            assert_eq!(p.rejected[0].why, WHY_FUTURE);
            assert!(p.older.is_empty(), "a future date is not recorded as older");
        }
        let cur = vec![dated(
            "web:openrouter.ai",
            &[
                ("x-ai/grok-4.7", Some(at(30, now))),
                ("x-ai/grok-4.8", Some(now + 300)),
            ],
        )];
        assert_eq!(swap_candidates(&roster, &cur, now)[0].new, "grok-4.8");
    }

    /// (F14-5) A `created` stamp later than now plus ten minutes never qualifies a candidate, in
    /// every branch: anchored (the seated id dated earlier in the same catalog) and unanchored (the
    /// seated id absent) - the reviewer's +23 h case included. It is announced only, never recorded
    /// as older.
    #[test]
    fn a_candidate_dated_in_the_future_is_not_evidence_in_any_branch() {
        let now = 1_795_000_000;
        let grok = roster_of(vec![entry("grok", "grok", "grok-4.7")]);
        let alibaba = roster_of(vec![alibaba_endpoint_entry()]);
        for ahead in [FUTURE_SKEW_SECS + 1, 3_600, 23 * 3_600] {
            let anchored = vec![dated(
                "web:openrouter.ai",
                &[
                    ("x-ai/grok-4.7", Some(at(30, now))),
                    ("x-ai/grok-4.8", Some(now + ahead)),
                ],
            )];
            let unanchored = vec![dated(
                "web:openrouter.ai",
                &[("qwen/qwen4-max", Some(now + ahead))],
            )];
            for (roster, cur) in [(&grok, &anchored), (&alibaba, &unanchored)] {
                let p = plan_swaps(roster, cur, &[], now);
                assert!(p.candidates.is_empty(), "+{ahead}s: {:?}", p.candidates);
                assert_eq!(p.rejected[0].why, WHY_NOT_YET, "+{ahead}s");
                assert!(p.older.is_empty());
            }
        }
        // within the skew it is evidence, anchored or not
        let unanchored = vec![dated(
            "web:openrouter.ai",
            &[("qwen/qwen4-max", Some(now + FUTURE_SKEW_SECS))],
        )];
        assert_eq!(
            swap_candidates(&alibaba, &unanchored, now)[0].new,
            "qwen4-max"
        );
        // a future copy is no evidence, but it does not veto a qualifying copy in another catalog
        let cur = vec![
            dated(
                "web:other.example",
                &[("qwen/qwen4-max", Some(now + 3_600))],
            ),
            dated("web:openrouter.ai", &[("qwen/qwen4-max", Some(at(2, now)))]),
        ];
        assert_eq!(swap_candidates(&alibaba, &cur, now)[0].new, "qwen4-max");
    }

    /// (F02-14) Negative date evidence wins: one catalog dates the candidate older than the seated
    /// id, a second catalog lists only the candidate with a recent date (which alone would qualify
    /// it, unanchored) - it is not a candidate, in either source order.
    #[test]
    fn negative_date_evidence_in_any_source_wins() {
        let now = 1_795_000_000;
        let roster = roster_of(vec![alibaba_endpoint_entry()]);
        let older = dated(
            "web:openrouter.ai",
            &[
                ("qwen/qwen3.8-max", Some(at(10, now))),
                ("qwen/qwen4-max", Some(at(300, now))),
            ],
        );
        let recent = dated("web:other.example", &[("qwen/qwen4-max", Some(at(2, now)))]);
        for cur in [
            vec![older.clone(), recent.clone()],
            vec![recent.clone(), older.clone()],
        ] {
            let p = plan_swaps(&roster, &cur, &[], now);
            assert!(p.candidates.is_empty(), "{:?}", p.candidates);
            assert_eq!(p.rejected[0].why, WHY_OLDER);
            assert_eq!(p.older, vec![OlderPair::new("qwen3.8-max", "qwen4-max")]);
        }
        // the recent copy alone qualifies (unanchored, within 180 days)
        assert_eq!(swap_candidates(&roster, &[recent], now)[0].new, "qwen4-max");
    }

    /// (F04-3) A candidate once recorded as older stays rejected, even when a later catalog re-dates
    /// it after the seated id - for a public id and for an own-list id alike; the record is keyed by
    /// the seated id, so another seated id is not affected.
    #[test]
    fn a_candidate_recorded_as_older_stays_rejected() {
        let now = 1_795_000_000;
        let roster = roster_of(vec![entry("grok", "grok", "grok-4.7")]);
        let recorded = vec![OlderPair::new("grok-4.7", "x-ai/grok-4.20")];
        let restamped = dated(
            "web:openrouter.ai",
            &[
                ("x-ai/grok-4.7", Some(at(30, now))),
                ("x-ai/grok-4.20", Some(at(1, now))),
            ],
        );
        let p = plan_swaps(&roster, std::slice::from_ref(&restamped), &recorded, now);
        assert!(p.candidates.is_empty(), "{:?}", p.candidates);
        assert_eq!(p.rejected[0].why, WHY_RECORDED);
        // the same id in the entry's own list
        let cur = vec![sl("grok", &["grok-4.7", "grok-4.20"])];
        let p = plan_swaps(&roster, &cur, &recorded, now);
        assert!(p.candidates.is_empty());
        assert_eq!(p.rejected[0].why, WHY_RECORDED);
        // another seated id is judged afresh
        let roster2 = roster_of(vec![entry("grok", "grok", "grok-4.5")]);
        assert_eq!(
            plan_swaps(&roster2, std::slice::from_ref(&restamped), &recorded, now).candidates[0]
                .new,
            "grok-4.20"
        );
        // the merge keeps old records, adds new ones once, and is bounded
        let merged = merge_older(
            &recorded,
            &[OlderPair::new("a-1", "a-2"), recorded[0].clone()],
            &[],
        );
        assert_eq!(merged.len(), 2);
        let many: Vec<OlderPair> = (0..MAX_OLDER_PAIRS + 7)
            .map(|i| OlderPair::new("m-1", &format!("m-{i}")))
            .collect();
        let capped = merge_older(&[], &many, &[]);
        assert_eq!(capped.len(), MAX_OLDER_PAIRS);
        assert_eq!(capped.last(), many.last(), "the newest are kept");
        // a state with records round-trips; an older state loads with none
        let st = ModelsState {
            sources: vec![],
            older: recorded.clone(),
        };
        let back: ModelsState = serde_json::from_str(&serde_json::to_string(&st).unwrap()).unwrap();
        assert_eq!(back.older, recorded);
        let old: ModelsState = serde_json::from_str(r#"{"sources":[]}"#).unwrap();
        assert!(old.older.is_empty());
    }

    /// (F15-2, F14-2) The reviewers' check: the refusal of a pair whose seated id is still in the
    /// roster survives the cap - 500 other pairs recorded after it, and more found in this pass -
    /// so a later catalog that re-dates the evicted candidate newer still cannot make it a
    /// candidate. The cap applies to the other pairs, oldest first; when the protected pairs alone
    /// exceed it, they are all kept.
    #[test]
    fn an_older_refusal_survives_the_cap_while_its_seated_model_is_in_the_roster() {
        let now = 1_795_000_000;
        let roster = roster_of(vec![entry("grok", "grok", "grok-4.7")]);
        let seated: Vec<String> = roster
            .active_entries()
            .map(|e| e.model.to_ascii_lowercase())
            .collect();
        // the protected refusal is the OLDEST record, then 500 others
        let mut previous = vec![OlderPair::new("GROK-4.7", "x-ai/grok-4.20")];
        previous
            .extend((0..MAX_OLDER_PAIRS).map(|i| OlderPair::new("gone-1", &format!("gone-{i}"))));
        let found: Vec<OlderPair> = (0..3)
            .map(|i| OlderPair::new("gone-2", &format!("new-{i}")))
            .collect();
        let merged = merge_older(&previous, &found, &seated);
        assert_eq!(merged.len(), MAX_OLDER_PAIRS);
        assert_eq!(merged[0], OlderPair::new("grok-4.7", "grok-4.20"), "kept");
        assert!(
            !merged.contains(&OlderPair::new("gone-1", "gone-0"))
                && !merged.contains(&OlderPair::new("gone-1", "gone-3")),
            "the oldest other pairs went first"
        );
        assert!(merged.contains(&OlderPair::new("gone-1", "gone-4")));
        assert!(
            found.iter().all(|p| merged.contains(p)),
            "the new pairs are kept"
        );
        // the re-stamped candidate is still refused
        let restamped = dated(
            "web:openrouter.ai",
            &[
                ("x-ai/grok-4.7", Some(at(30, now))),
                ("x-ai/grok-4.20", Some(at(1, now))),
            ],
        );
        let p = plan_swaps(&roster, std::slice::from_ref(&restamped), &merged, now);
        assert!(p.candidates.is_empty(), "{:?}", p.candidates);
        assert_eq!(p.rejected[0].why, WHY_RECORDED);
        // without the protection (the seat left the roster) the oldest pair is evicted as before
        let unprotected = merge_older(&previous, &found, &[]);
        assert!(!unprotected.contains(&OlderPair::new("grok-4.7", "grok-4.20")));
        // the protected pairs alone exceed the cap: all of them are kept, every other pair goes
        let mut many: Vec<OlderPair> = (0..MAX_OLDER_PAIRS + 5)
            .map(|i| OlderPair::new("grok-4.7", &format!("grok-4.{}", 100 + i)))
            .collect();
        many.push(OlderPair::new("gone-9", "gone-10"));
        let kept = merge_older(&many, &[], &seated);
        assert_eq!(kept.len(), MAX_OLDER_PAIRS + 5);
        assert!(kept.iter().all(|p| p.seated == "grok-4.7"));
    }

    /// (F02-15) An endpoint's authenticated list is keyed by the canonical endpoint (scheme, host,
    /// port, path) plus the key variable and the plan: two entries on one host with another path or
    /// another key do not share it, so a successor only one of them lists is never offered to the
    /// other; the same endpoint written with the explicit default port and a trailing slash does.
    #[test]
    fn an_endpoint_list_is_keyed_by_endpoint_key_and_plan_not_host() {
        let ep = |base: &str, key: &str, plan: &str, model: &str, pos: usize| RosterEntry {
            position: pos,
            plan: plan.into(),
            endpoint: Some(c3_core::claude::ClaudeEndpoint {
                base_url: base.into(),
                host_name: "api.example".into(),
                env_key: key.into(),
                ..Default::default()
            }),
            ..entry(&format!("e{pos}"), "claude", model)
        };
        let a = ep("https://api.example/one", "KEY_A", "pa", "glm-5.3", 1);
        let b = ep("https://api.example/two", "KEY_A", "pa", "glm-5.3-air", 2);
        let c = ep("https://api.example/one", "KEY_C", "pa", "glm-5.3", 3);
        let d = ep("https://API.example:443/one/", "KEY_A", "pa", "glm-5.3", 4);
        let e2 = ep("https://api.example/one", "KEY_A", "pb", "glm-5.3", 5);
        let sa = source_for(&a).unwrap();
        assert_eq!(sa, "endpoint:https://api.example:443/one|KEY_A|pa");
        assert_ne!(sa, source_for(&b).unwrap());
        assert_ne!(sa, source_for(&c).unwrap());
        assert_ne!(sa, source_for(&e2).unwrap());
        assert_eq!(sa, source_for(&d).unwrap());
        // the first entry's list holds a private successor; only the entries with its key get it
        let roster = roster_of(vec![a, b, c, d, e2]);
        let cur = vec![sl(
            &sa,
            &["glm-5.3", "glm-5.4", "glm-5.3-air", "glm-5.4-air"],
        )];
        let got: Vec<(usize, String)> = swap_candidates(&roster, &cur, 0)
            .into_iter()
            .map(|c| (c.position, c.new))
            .collect();
        assert_eq!(
            got,
            vec![(1, "glm-5.4".to_string()), (4, "glm-5.4".to_string())]
        );
        // the first pass after the key change reads the old host-only record (no flood of "new")
        let prev = ModelsState {
            sources: vec![sl("endpoint:api.example", &["glm-5.3", "glm-5.4"])],
            ..Default::default()
        };
        let cur = vec![sl(&sa, &["glm-5.3", "glm-5.4", "glm-5.5"])];
        assert_eq!(new_ids(&prev, &cur), vec!["glm-5.5"]);
    }

    #[test]
    fn a_seated_id_absent_from_the_catalog_needs_a_recent_candidate() {
        let now = 1_795_000_000;
        let roster = roster_of(vec![alibaba_endpoint_entry()]);
        let old = vec![dated(
            "web:openrouter.ai",
            &[("qwen/qwen4-max", Some(at(400, now)))],
        )];
        let (c, rejected) = swap_plan(&roster, &old, now);
        assert!(c.is_empty());
        assert_eq!(rejected[0].why, "older than 180 days by the catalog's date");
        let recent = vec![dated(
            "web:openrouter.ai",
            &[("qwen/qwen4-max", Some(at(20, now)))],
        )];
        assert_eq!(swap_candidates(&roster, &recent, now)[0].new, "qwen4-max");
    }

    #[test]
    fn undated_and_page_ids_are_never_candidates() {
        let now = 1_795_000_000;
        let roster = roster_of(vec![alibaba_endpoint_entry()]);
        let undated = vec![dated("web:openrouter.ai", &[("qwen/qwen4-max", None)])];
        let (c, rejected) = swap_plan(&roster, &undated, now);
        assert!(c.is_empty());
        assert_eq!(rejected[0].why, "no date in the catalog - announce only");
        let page = vec![sl("page:qwen-docs", &["qwen4-max"])];
        let (c, rejected) = swap_plan(&roster, &page, now);
        assert!(c.is_empty());
        assert_eq!(rejected[0].why, "a page carries no dates - announce only");
    }

    #[test]
    fn an_old_state_without_dates_loads_and_dates_round_trip() {
        let old: ModelsState = serde_json::from_str(
            r#"{"sources":[{"source":"web:openrouter.ai","ids":["qwen/qwen4-max"]}]}"#,
        )
        .unwrap();
        assert!(old.sources[0].created.is_empty());
        let now = 1_795_000_000;
        let st = ModelsState {
            sources: vec![dated("web:openrouter.ai", &[("qwen/qwen4-max", Some(now))])],
            ..Default::default()
        };
        let back: ModelsState = serde_json::from_str(&serde_json::to_string(&st).unwrap()).unwrap();
        assert_eq!(back.sources[0].created.get("qwen/qwen4-max"), Some(&now));
    }

    #[test]
    fn family_news_skips_older_by_date_and_dedupes_per_provider() {
        let now = 1_795_000_000;
        let roster = roster_of(vec![
            entry("ZAI", "claude", "glm-5.3"),
            entry("ZAI", "claude", "glm-5.3-flash"),
            entry("gemini", "agy", "gemini-3.8-flash-high"),
        ]);
        let cur = vec![dated(
            "web:openrouter.ai",
            &[
                ("z-ai/glm-5.3-flashx-legal", Some(at(2, now))),
                ("google/gemini-3.8-flash-high", Some(at(10, now))),
                // numerically not lower, but created before the seated id: not announced
                ("google/gemini-3.80-pro", Some(at(200, now))),
                ("google/gemini-4-hydrogen", Some(at(3, now))),
            ],
        )];
        let prev = ModelsState {
            sources: vec![sl("web:openrouter.ai", &[])],
            ..Default::default()
        };
        let news = family_news(&roster, &prev, &cur, now);
        let zai: Vec<&FamilyNews> = news.iter().filter(|n| n.provider == "ZAI").collect();
        assert_eq!(zai.len(), 1, "{news:?}");
        assert_eq!(zai[0].seated, "glm-5.3-flash");
        assert_eq!(zai[0].ids, vec!["glm-5.3-flashx-legal"]);
        let g = news.iter().find(|n| n.provider == "gemini").unwrap();
        assert_eq!(g.ids, vec!["gemini-4-hydrogen"]);
    }

    #[test]
    fn family_news_first_collection_and_later_diff() {
        let now = 1_795_000_000;
        let roster = Roster {
            exists: true,
            entries: vec![entry("gemini", "agy", "gemini-3.8-flash-high")],
            ..Default::default()
        };
        let mut cur = sl(
            "web:openrouter.ai",
            &[
                "google/gemini-4-hydrogen",
                "google/gemini-3.9-pro",
                "google/gemini-3.5-ultra",
                "google/gemini-4.1-flash-high",
                "google/gemma-5",
            ],
        );
        cur.created
            .insert("google/gemini-4-hydrogen".into(), at(3, now));
        cur.created
            .insert("google/gemini-3.9-pro".into(), at(90, now));
        cur.created
            .insert("google/gemini-3.5-ultra".into(), at(1, now));
        cur.created
            .insert("google/gemini-4.1-flash-high".into(), at(2, now));
        let cur = vec![cur];
        // first collection: only the ids created within 30 days count; a lower version
        // (gemini-3.5-ultra) and the same suffix (a successor, not family news) are excluded
        let news = family_news(&roster, &ModelsState::default(), &cur, now);
        assert_eq!(news.len(), 1);
        assert_eq!(news[0].seated, "gemini-3.8-flash-high");
        assert_eq!(news[0].ids, vec!["gemini-4-hydrogen"]);
        // a later pass: an id absent from the previous record is new whatever its stamp
        let prev = ModelsState {
            sources: vec![sl("web:openrouter.ai", &["google/gemini-4-hydrogen"])],
            ..Default::default()
        };
        let news = family_news(&roster, &prev, &cur, now);
        assert_eq!(news[0].ids, vec!["gemini-3.9-pro"]);
        // nothing new -> nothing announced
        let prev = ModelsState {
            sources: cur.clone(),
            ..Default::default()
        };
        assert!(family_news(&roster, &prev, &cur, now).is_empty());
    }

    #[test]
    fn family_news_is_capped_per_entry() {
        let roster = Roster {
            exists: true,
            entries: vec![entry("g", "agy", "gemini-3.8-flash-high")],
            ..Default::default()
        };
        let ids: Vec<String> = (0..9).map(|i| format!("google/gemini-4-v{i}")).collect();
        let refs: Vec<&str> = ids.iter().map(|s| s.as_str()).collect();
        let cur = vec![sl("web:openrouter.ai", &refs)];
        let prev = ModelsState {
            sources: vec![sl("web:openrouter.ai", &[])],
            ..Default::default()
        };
        let news = family_news(&roster, &prev, &cur, 0);
        assert_eq!(news[0].ids.len(), FAMILY_NEWS_PER_ENTRY);
    }

    #[test]
    fn published_names_come_from_public_sources_only() {
        let st = ModelsState {
            sources: vec![
                sl(
                    "web:https://openrouter.ai:443/api/v1/models",
                    &["qwen/Qwen4-Max"],
                ),
                sl("page:zai-docs", &["glm-6"]),
                sl("endpoint:api.z.ai", &["glm-9-secret"]),
                sl(
                    "endpoint:https://api.z.ai:443/api/anthropic|ZAI_API_KEY|zai",
                    &["glm-9-x"],
                ),
                sl("grok", &["grok-4.7"]),
            ],
            ..Default::default()
        };
        let has = |p: &Published, class: &str, id: &str| p.contains(&(class.into(), id.into()));
        // the built-in catalog only (no config): its ids, bound to their vendor slug's class
        let p = published_from(&st, &crate::maintain::web::trusted_sources(None));
        assert!(has(&p, "alibaba", "qwen4-max"));
        assert!(!has(&p, "zai", "qwen4-max"), "bound to its own vendor only");
        assert!(
            !has(&p, "zai", "glm-6"),
            "a configured page is not trusted unless declared public"
        );
        assert!(!p.iter().any(|(_, m)| m == "glm-9-secret" || m == "glm-9-x"));
        assert!(
            !p.iter().any(|(_, m)| m == "grok-4.7"),
            "an engine's own list stays private"
        );
        assert_eq!(p.len(), 1, "{p:?}");
        // the page declared public: its plain ids bind to its host's class (docs.z.ai -> zai)
        let cfg = crate::c3config::C3Config {
            model_pages: vec![crate::c3config::ModelPageConfig {
                name: "zai-docs".into(),
                url: "https://docs.z.ai/models".into(),
                pattern: "(glm-[0-9.]+)".into(),
                public: true,
            }],
            ..Default::default()
        };
        let p = published_from(&st, &crate::maintain::web::trusted_sources(Some(&cfg)));
        assert!(has(&p, "zai", "glm-6"));
        assert!(!has(&p, "alibaba", "glm-6"));
        assert!(
            !p.iter().any(|(_, m)| m == "glm-9-secret" || m == "glm-9-x"),
            "a keyed endpoint list stays private"
        );
        // no state -> nothing published
        assert!(published_from(
            &ModelsState::default(),
            &crate::maintain::web::trusted_sources(Some(&cfg))
        )
        .is_empty());
    }

    /// (F16-1, F17-5) A state file of an older build keys a catalog by its host (`web:<host>`): it
    /// loads without error, and its ids are NOT published - which of the host's URLs they came from
    /// is unknown - not even for the built-in catalog's host, until a fetch rewrites the record
    /// under the catalog's URL.
    #[test]
    fn a_legacy_host_keyed_catalog_record_is_read_but_not_trusted() {
        let old: ModelsState = serde_json::from_str(
            r#"{"sources":[{"source":"web:openrouter.ai","ids":["qwen/qwen4-max"],"created":{"qwen/qwen4-max":1}},
                {"source":"web:pub.example.com","ids":["z-ai/glm-9-trusted"]}]}"#,
        )
        .unwrap();
        assert_eq!(old.sources.len(), 2);
        let cfg = crate::c3config::C3Config {
            model_catalogs: Some(vec![crate::c3config::ModelCatalogConfig {
                url: "https://pub.example.com/v1/models".into(),
                public: true,
            }]),
            ..Default::default()
        };
        for trusted in [
            crate::maintain::web::trusted_sources(None),
            crate::maintain::web::trusted_sources(Some(&cfg)),
        ] {
            assert!(published_from(&old, &trusted).is_empty());
        }
        // the same ids under their URL are published
        let rewritten = ModelsState {
            sources: vec![
                sl(
                    "web:https://openrouter.ai:443/api/v1/models",
                    &["qwen/qwen4-max"],
                ),
                sl(
                    "web:https://pub.example.com:443/v1/models",
                    &["z-ai/glm-9-trusted"],
                ),
            ],
            ..Default::default()
        };
        let p = published_from(
            &rewritten,
            &crate::maintain::web::trusted_sources(Some(&cfg)),
        );
        assert!(p.contains(&("alibaba".to_string(), "qwen4-max".to_string())));
        assert!(p.contains(&("zai".to_string(), "glm-9-trusted".to_string())));
    }

    #[test]
    fn replace_single_occurrence_keeps_formatting() {
        let text = r#"{
  "reviewers": [
    { "provider": "grok", "engine": "grok", "model": "grok-4.7", "plan": "supergrok" }
  ]
}"#;
        let out = replace_entry_model(text, 1, "grok", "grok-4.7", "grok-4.8").unwrap();
        assert!(out.contains("\"model\": \"grok-4.8\""));
        assert_eq!(out, text.replace("grok-4.7", "grok-4.8"));
    }

    #[test]
    fn replace_targets_the_position_when_model_repeats() {
        let text = r#"{"reviewers":[
          {"provider":"a","model":"m1"},
          {"provider":"b","model":"m1"}
        ]}"#;
        // same model string in two entries -> the position decides
        let out = replace_entry_model(text, 2, "b", "m1", "m2").unwrap();
        assert_eq!(out.matches("\"model\":\"m2\"").count(), 1);
        assert!(out.contains("{\"provider\":\"b\",\"model\":\"m2\"}"));
        assert!(out.contains("{\"provider\":\"a\",\"model\":\"m1\"}"));
        // the position must still name the provider and the old model
        assert!(replace_entry_model(text, 1, "b", "m1", "m2").is_err());
        assert!(replace_entry_model(text, 3, "b", "m1", "m2").is_err());
        assert!(replace_entry_model(text, 0, "a", "m1", "m2").is_err());
    }

    #[test]
    fn replace_errors_when_absent() {
        let text = r#"{"reviewers":[{"provider":"a","model":"m1"}]}"#;
        assert!(replace_entry_model(text, 1, "a", "nope", "x").is_err());
    }

    #[test]
    fn replace_refuses_a_hostile_successor_id() {
        // a quote-bearing successor can never be spliced: refused, the roster text is not consulted
        let text = r#"{"reviewers":[{"provider":"a","model":"m1"}]}"#;
        assert!(replace_entry_model(text, 1, "a", "m1", "m2\",\"x\":\"y").is_err());
        assert!(replace_entry_model(text, 1, "a", "m1", "m2\nextra").is_err());
    }

    /// (F02-17) The writer edits the parsed `reviewers[position].model` string: a JSON-escaped model
    /// (`m-1` is `m-1`) is found and replaced whole, while an `ext` object holding the same
    /// decoded id as a plain `"model"` (the old text search's only literal match) is untouched; the
    /// re-parsed roster differs from the old one in that one field only.
    #[test]
    fn replace_edits_the_parsed_entry_model_not_a_literal_elsewhere() {
        let text = r#"{"roster_version":1,"reviewers":[{"provider":"a","engine":"claude","model":"m-1","ext":{"note":{"model":"m-1"}}}],"ext":{"c3":{"model":"m-1"}}}"#;
        let out = replace_entry_model(text, 1, "a", "m-1", "m-2").unwrap();
        assert_eq!(
            out,
            r#"{"roster_version":1,"reviewers":[{"provider":"a","engine":"claude","model":"m-2","ext":{"note":{"model":"m-1"}}}],"ext":{"c3":{"model":"m-1"}}}"#
        );
        let before: serde_json::Value = serde_json::from_str(text).unwrap();
        let mut after: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(after["reviewers"][0]["model"], "m-2");
        assert_eq!(after["reviewers"][0]["ext"]["note"]["model"], "m-1");
        after["reviewers"][0]["model"] = serde_json::Value::String("m-1".into());
        assert_eq!(after, before, "nothing else changed");
        // a repeated model key in the entry is refused (a parser takes the last one; never guess)
        let dup = r#"{"reviewers":[{"provider":"a","model":"m-1","model":"m-1"}]}"#;
        assert!(replace_entry_model(dup, 1, "a", "m-1", "m-2").is_err());
        // a non-string model is refused
        let num = r#"{"reviewers":[{"provider":"a","model":7}]}"#;
        assert!(replace_entry_model(num, 1, "a", "7", "m-2").is_err());
    }

    /// (F02-17) The span scanner walks nested values, strings with escaped quotes and brackets, and
    /// whitespace, and lands on the right element.
    #[test]
    fn the_roster_scanner_finds_the_element_model_span() {
        let text = "{ \"ext\": {\"x\": [1, {\"model\": \"no\"}], \"s\": \"a\\\"]}\"},\n  \"reviewers\" : [ {\"model\":\"first\"} ,\n {\"provider\":\"p\", \"model\" : \"se\\\"cond\", \"n\": null, \"b\": true} ] }";
        let (s, e) = reviewer_model_span(text, 1).unwrap();
        assert_eq!(&text[s..e], "\"se\\\"cond\"");
        let (s, e) = reviewer_model_span(text, 0).unwrap();
        assert_eq!(&text[s..e], "\"first\"");
        assert!(reviewer_model_span(text, 2).is_err());
        assert!(reviewer_model_span("{\"reviewers\": [ {\"model\": \"x\" ", 0).is_err());
        assert!(reviewer_model_span("[]", 0).is_err());
    }

    #[test]
    fn id_shape_filter_drops_malformed_ids_at_collection() {
        let grok = "  * grok-4.7 (default)\n  - grok\"-4.6\n  - grok-4.5\n";
        assert_eq!(parse_grok_models(grok), vec!["grok-4.7", "grok-4.5"]);
        let body =
            "{\"data\":[{\"id\":\"glm-5.3\"},{\"id\":\"glm-5.4\\nignore\"},{\"id\":\"glm-5.5\"}]}";
        assert_eq!(parse_models_json(body), vec!["glm-5.3", "glm-5.5"]);
    }

    #[test]
    fn commit_swap_aborts_on_a_concurrent_edit_and_writes_atomically() {
        let dir = std::env::temp_dir().join(format!(
            "c3-mnt-swap-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("roster.json");
        let path_s = path.to_string_lossy().to_string();

        // on-disk content matches the basis -> the atomic write lands
        std::fs::write(&path, b"A").unwrap();
        assert!(commit_swap(&path_s, "A", "B", "20261115").is_ok());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "B");
        assert!(dir.join("roster.json.bak-maintain-20261115").exists());

        // the file changed since the edit was computed -> abort, the file is left as it is
        std::fs::write(&path, b"CHANGED").unwrap();
        let err = commit_swap(&path_s, "A", "B2", "20261116").unwrap_err();
        assert!(err.contains("roster changed during the pass"), "{err}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "CHANGED");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_swap_aborts_when_the_roster_changed_since_the_pre_smoke_snapshot() {
        // (R3) The swap edit is computed from the basis the protection gate judged BEFORE the smoke,
        // and apply_swap refuses to write when the on-disk roster no longer equals that basis - so a
        // concurrent edit during the minutes-long smoke (protecting the entry, or re-pointing its old
        // model to another provider) aborts the swap instead of being applied over a stale view.
        let dir = std::env::temp_dir().join(format!(
            "c3-mnt-applyswap-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("roster.json");
        let path_s = path.to_string_lossy().to_string();
        let basis = r#"{"roster_version":1,"reviewers":[{"provider":"grok","engine":"grok","model":"grok-4.7","plan":"supergrok"}]}"#;

        // unchanged on disk since the snapshot -> the swap lands
        std::fs::write(&path, basis).unwrap();
        apply_swap(
            &path_s, basis, 1, "grok", "grok-4.7", "grok-4.8", "20261115",
        )
        .unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("grok-4.8"));

        // the operator PROTECTS the entry mid-smoke: on disk the roster now differs from the basis
        // the gate judged, so a swap computed from that stale basis aborts and writes nothing.
        let protected = r#"{"roster_version":1,"reviewers":[{"provider":"grok","engine":"grok","model":"grok-4.8","plan":"supergrok","ext":{"reserve_percent":15}}]}"#;
        std::fs::write(&path, protected).unwrap();
        let err = apply_swap(
            &path_s, basis, 1, "grok", "grok-4.7", "grok-4.9", "20261116",
        )
        .unwrap_err();
        assert!(err.contains("roster changed during the smoke"), "{err}");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            protected,
            "the protected roster was left untouched"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn swap_candidates_finds_successor() {
        let roster = Roster {
            exists: true,
            entries: vec![RosterEntry {
                position: 3,
                provider: "ZAI-claude".into(),
                engine: "claude".into(),
                model: "glm-5.3".into(),
                endpoint: Some(c3_core::claude::ClaudeEndpoint {
                    base_url: "https://api.z.ai/api/anthropic".into(),
                    host_name: "api.z.ai".into(),
                    env_key: "ZAI_API_KEY".into(),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        };
        let own = source_for(&roster.entries[0]).unwrap();
        let cur = vec![sl(&own, &["glm-5.3", "glm-5.4"])];
        let c = swap_candidates(&roster, &cur, 0);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].old, "glm-5.3");
        assert_eq!(c[0].new, "glm-5.4");
        assert_eq!(c[0].provider, "ZAI-claude");
        assert_eq!(c[0].price_hold, None);
    }

    /// One catalog row of [`priced`]: the id, its date and (optionally) its `(prompt, completion)`.
    type PricedRow<'a> = (&'a str, i64, Option<(f64, f64)>);

    /// A public source with a date and (optionally) a `(prompt, completion)` list price per id.
    fn priced(source: &str, ids: &[PricedRow]) -> SourceList {
        let mut s = dated(
            source,
            &ids.iter()
                .map(|(i, c, _)| (*i, Some(*c)))
                .collect::<Vec<_>>(),
        );
        for (id, _, p) in ids {
            if let Some((prompt, completion)) = p {
                s.prices.insert(
                    id.to_string(),
                    ListPrice {
                        prompt: Some(*prompt),
                        completion: Some(*completion),
                    },
                );
            }
        }
        s
    }

    /// (0.2.5 D3) A successor ACROSS A MAJOR whose catalog list price is more than twice the seated
    /// id's (either field) is flagged - announced only, "seat it by hand"; within one major, without
    /// prices for both ids, or at no more than twice, nothing changes.
    #[test]
    fn a_successor_across_a_major_priced_over_twice_the_seated_one_is_announced_only() {
        let now = 1_795_000_000;
        let roster = roster_of(vec![alibaba_endpoint_entry()]);
        let src = "web:https://openrouter.ai:443/api/v1/models";
        let hold = |cur: Vec<SourceList>| -> Vec<(String, Option<String>)> {
            swap_candidates(&roster, &cur, now)
                .into_iter()
                .map(|c| (c.new, c.price_hold))
                .collect()
        };
        // the prompt price three times higher
        let cur = vec![priced(
            src,
            &[
                ("qwen/qwen3.8-max", at(30, now), Some((0.000001, 0.000004))),
                ("qwen/qwen4-max", at(1, now), Some((0.000003, 0.000006))),
            ],
        )];
        assert_eq!(
            hold(cur),
            vec![(
                "qwen4-max".to_string(),
                Some("list price 3 times higher - seat it by hand".to_string())
            )]
        );
        // only the completion price over twice
        let cur = vec![priced(
            src,
            &[
                ("qwen/qwen3.8-max", at(30, now), Some((0.000002, 0.000004))),
                ("qwen/qwen4-max", at(1, now), Some((0.000003, 0.00001))),
            ],
        )];
        assert_eq!(
            hold(cur)[0].1.as_deref(),
            Some("list price 2.5 times higher - seat it by hand")
        );
        // exactly twice: not more than twice
        let cur = vec![priced(
            src,
            &[
                ("qwen/qwen3.8-max", at(30, now), Some((0.000001, 0.000004))),
                ("qwen/qwen4-max", at(1, now), Some((0.000002, 0.000008))),
            ],
        )];
        assert_eq!(hold(cur), vec![("qwen4-max".to_string(), None)]);
        // no price for the seated id: nothing changes
        let cur = vec![priced(
            src,
            &[
                ("qwen/qwen3.8-max", at(30, now), None),
                ("qwen/qwen4-max", at(1, now), Some((0.00003, 0.00006))),
            ],
        )];
        assert_eq!(hold(cur), vec![("qwen4-max".to_string(), None)]);
        // within one major: ten times the price changes nothing
        let cur = vec![priced(
            src,
            &[
                ("qwen/qwen3.8-max", at(30, now), Some((0.000001, 0.000004))),
                ("qwen/qwen3.9-max", at(1, now), Some((0.00001, 0.00004))),
            ],
        )];
        assert_eq!(hold(cur), vec![("qwen3.9-max".to_string(), None)]);
        // the prices of an id from the entry's own list come from the catalog too
        let own = source_for(&alibaba_endpoint_entry()).unwrap();
        let cur = vec![
            sl(&own, &["qwen3.8-max", "qwen4-max"]),
            priced(
                src,
                &[
                    ("qwen/qwen3.8-max", at(30, now), Some((0.000001, 0.000001))),
                    ("qwen/qwen4-max", at(1, now), Some((0.000005, 0.000001))),
                ],
            ),
        ];
        assert_eq!(
            hold(cur)[0].1.as_deref(),
            Some("list price 5 times higher - seat it by hand")
        );
        assert_eq!(ratio_text(2.04), "2.04");
        assert_eq!(ratio_text(2.0000001), "2.00");
        assert_eq!(ratio_text(12.345), "12.3");
    }

    /// (0.2.5 D3) A roster id the plan spells with a leading token naming its maker or its plan's
    /// vendor (`kimi-code/k3.1-pro`, `kimi-k3.1-pro`), which the catalog leaves out
    /// (`moonshotai/k3.1-pro`), is matched once more without it: dates and successors work as for
    /// a direct match, and the successor keeps the plan's spelling (the prefix put back).
    #[test]
    fn a_roster_id_with_a_lab_token_is_matched_through_the_catalog_spelling() {
        let now = 1_795_000_000;
        let src = "web:https://openrouter.ai:443/api/v1/models";
        let mut kimi = entry("kimi", "kimi", "kimi-code/k3.1-pro");
        kimi.plan = "kimi".into();
        kimi.position = 1;
        let mut short = entry("kimi-claude", "claude", "kimi-k3.1-pro");
        short.position = 2;
        let roster = roster_of(vec![kimi.clone(), short.clone()]);
        let cur = vec![dated(
            src,
            &[
                ("moonshotai/k3.1-pro", Some(at(30, now))),
                ("moonshotai/k3.2-pro", Some(at(1, now))),
                ("qwen/k3.9-pro", Some(at(1, now))),
            ],
        )];
        assert_eq!(
            catalog_alias(&kimi, &cur),
            Some(CatalogAlias {
                prefix: "kimi-code/".into(),
                vendors: vec!["moonshotai".into()]
            })
        );
        let c = swap_candidates(&roster, &cur, now);
        let pairs: Vec<(String, String, bool)> = c
            .iter()
            .map(|c| (c.old.clone(), c.new.clone(), c.from_public))
            .collect();
        assert_eq!(
            pairs,
            vec![
                (
                    "kimi-code/k3.1-pro".to_string(),
                    "kimi-code/k3.2-pro".to_string(),
                    true
                ),
                (
                    "kimi-k3.1-pro".to_string(),
                    "kimi-k3.2-pro".to_string(),
                    true
                ),
            ]
        );
        // the dates of the catalog spelling count: a successor dated older is rejected and recorded
        let older = vec![dated(
            src,
            &[
                ("moonshotai/k3.1-pro", Some(at(30, now))),
                ("moonshotai/k3.2-pro", Some(at(300, now))),
            ],
        )];
        let plan = plan_swaps(&roster_of(vec![kimi.clone()]), &older, &[], now);
        assert!(plan.candidates.is_empty(), "{:?}", plan.candidates);
        assert_eq!(plan.rejected.len(), 1);
        assert_eq!(plan.rejected[0].new, "kimi-code/k3.2-pro");
        assert_eq!(plan.rejected[0].why, WHY_OLDER);
        assert_eq!(
            plan.older,
            vec![OlderPair::new("kimi-code/k3.1-pro", "kimi-code/k3.2-pro")]
        );
        // nothing is guessed: the seated id itself must be listed in the catalog spelling
        let only_succ = vec![dated(src, &[("moonshotai/k3.2-pro", Some(at(1, now)))])];
        assert_eq!(catalog_alias(&kimi, &only_succ), None);
        assert!(swap_candidates(&roster_of(vec![kimi.clone()]), &only_succ, now).is_empty());
        // a token that names no maker is never dropped
        let mut odd = entry("x", "claude", "acme-k3.1-pro");
        odd.position = 1;
        assert_eq!(catalog_alias(&odd, &cur), None);
        // a direct match wins: the catalog spells the roster id as it is
        let direct = vec![dated(
            src,
            &[
                ("moonshotai/kimi-k3.1-pro", Some(at(30, now))),
                ("moonshotai/kimi-k3.2-pro", Some(at(1, now))),
            ],
        )];
        assert_eq!(catalog_alias(&short, &direct), None);
        assert_eq!(
            swap_candidates(&roster_of(vec![short.clone()]), &direct, now)[0].new,
            "kimi-k3.2-pro"
        );
        // a token naming the plan's vendor (another maker's model on that plan)
        let mut resold = entry("alibaba-claude", "claude", "alibaba-deepseek-v4.1-flash");
        resold.plan = "alibaba".into();
        resold.position = 1;
        let ds = vec![dated(
            src,
            &[
                ("deepseek/deepseek-v4.1-flash", Some(at(30, now))),
                ("deepseek/deepseek-v4.2-flash", Some(at(1, now))),
            ],
        )];
        assert_eq!(
            swap_candidates(&roster_of(vec![resold.clone()]), &ds, now)[0].new,
            "alibaba-deepseek-v4.2-flash"
        );
        // ... but not on a plan of another vendor
        resold.plan = "byteplus".into();
        assert_eq!(catalog_alias(&resold, &ds), None);
        // the prices are read in the plan's spelling too
        let mut priced_cur = priced(
            src,
            &[
                (
                    "moonshotai/k3.1-pro",
                    at(30, now),
                    Some((0.000001, 0.000001)),
                ),
                ("moonshotai/k4-pro", at(1, now), Some((0.000004, 0.000001))),
            ],
        );
        priced_cur.ids.push("acme/other-1".into());
        let c = swap_candidates(&roster_of(vec![kimi]), &[priced_cur], now);
        assert_eq!(c[0].new, "kimi-code/k4-pro");
        assert_eq!(
            c[0].price_hold.as_deref(),
            Some("list price 4 times higher - seat it by hand")
        );
    }

    /// (0.2.5 D3) A Claude id a trusted catalog spells with dots under the `anthropic/` slug is
    /// published in its dash form too (the claude engine's spelling), only when that form is of the
    /// Claude shape.
    #[test]
    fn a_dotted_claude_id_of_a_trusted_catalog_is_published_in_its_dash_form() {
        assert_eq!(
            claude_dash_form("claude-opus-4.5").as_deref(),
            Some("claude-opus-4-5")
        );
        assert_eq!(
            claude_dash_form("claude-sonnet-5.1").as_deref(),
            Some("claude-sonnet-5-1")
        );
        assert_eq!(claude_dash_form("claude-opus-4-5"), None, "already dashed");
        assert_eq!(claude_dash_form("claude-3.5-sonnet"), None, "not the shape");
        assert_eq!(claude_dash_form("claude-opus-4.5.1"), None, "not the shape");
        assert_eq!(claude_dash_form("claude-opus-4.5-thinking"), None);
        assert_eq!(claude_dash_form("gpt-6.2"), None);
        let st = ModelsState {
            sources: vec![
                sl(
                    "web:https://openrouter.ai:443/api/v1/models",
                    &[
                        "anthropic/claude-opus-6.1",
                        "anthropic/claude-3.5-sonnet",
                        "z-ai/claude-opus-6.2",
                    ],
                ),
                sl("page:untrusted", &["claude-opus-6.3"]),
            ],
            ..Default::default()
        };
        let p = published_from(&st, &crate::maintain::web::trusted_sources(None));
        let has = |class: &str, id: &str| p.contains(&(class.to_string(), id.to_string()));
        assert!(has("anthropic", "claude-opus-6-1"), "{p:?}");
        assert!(
            has("anthropic", "claude-opus-6.1"),
            "the catalog's own form stays"
        );
        assert!(!has("anthropic", "claude-3-5-sonnet"));
        assert!(
            !p.iter().any(|(_, m)| m == "claude-opus-6-2"),
            "another slug: {p:?}"
        );
        assert!(
            !p.iter().any(|(_, m)| m == "claude-opus-6-3"),
            "untrusted: {p:?}"
        );
    }
}
