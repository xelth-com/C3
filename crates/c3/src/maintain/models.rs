//! The model step of a maintenance pass (`C3_MAINTAIN_MODELS`): collect the available model ids
//! per source, record them (so a later pass can tell which are new), and — in `swap` mode — seat a
//! seated id's successor after it passes the same smoke, editing exactly that roster entry's `model`
//! and nothing else.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::env::Env;
use super::version::{parse_model_id, successor, valid_model_id};
use super::web::{is_public_source, strip_vendor};
use c3_core::roster::{Roster, RosterEntry};

/// The model ids collected from one source in a pass. Sources: `grok` / `agy` (the engines' own
/// lists), `endpoint:<host>` (an endpoint's `/v1/models`, which may be PRIVATE), `web:<host>` (a
/// public catalog, ids kept as `<vendor>/<model>`) and `page:<name>` (a configured public page).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceList {
    pub source: String,
    pub ids: Vec<String>,
    /// The catalog's `created` stamp (unix seconds) per id, when it gives one: the first-collection
    /// "new" rule and the date evidence a public candidate needs. Persisted; a state file written
    /// before it existed loads with no dates.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub created: BTreeMap<String, i64>,
}

/// The persisted model state: the last-seen id list per source.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelsState {
    #[serde(default)]
    pub sources: Vec<SourceList>,
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

/// The source an entry's models come from (`grok` / `agy` for those engines, `endpoint:<host>` for
/// an Anthropic-compatible endpoint entry). `None` when the entry has no known model source.
pub fn source_for(e: &RosterEntry) -> Option<String> {
    let engine = if e.engine.is_empty() {
        "codex"
    } else {
        e.engine.as_str()
    };
    match engine {
        "grok" => Some("grok".to_string()),
        "agy" => Some("agy".to_string()),
        _ => e
            .endpoint
            .as_ref()
            .map(|ep| format!("endpoint:{}", ep.host_name)),
    }
}

/// Collect the model ids of every source present in `roster`. `grok models` / `agy models` through
/// the launchers; each distinct Anthropic-compatible endpoint through `GET <base_url>/v1/models`
/// with its key. Every failure is ignored (an empty list for that source).
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
    match env.http_get(&url, &headers, Duration::from_secs(5), false) {
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
        let prev = previous.ids_of(&s.source);
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

/// A public id that the version rule would take for a successor but that lacks the date evidence a
/// public candidate needs ([`public_verdict`]); reported in the notes, never smoked.
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
                    vendor: id.split_once('/').map(|(v, _)| v).unwrap_or(""),
                    created: s.created.get(id).copied(),
                });
            }
        }
    }
    out
}

/// The catalog date of the seated id within ONE source: its entry under the same vendor slug as the
/// candidate when it appears under several, else its first entry. `None` when the seated id is not
/// in that source or carries no date there.
fn seated_created(source: &SourceList, seated: &str, vendor: &str) -> Option<i64> {
    let hits: Vec<&String> = source
        .ids
        .iter()
        .filter(|id| strip_vendor(id).eq_ignore_ascii_case(seated))
        .collect();
    let pick = hits
        .iter()
        .find(|id| id.split_once('/').map(|(v, _)| v).unwrap_or("") == vendor)
        .or_else(|| hits.first())?;
    source.created.get(*pick).copied()
}

/// Whether a public id may be a swap candidate for `seated`: the smoke cannot tell a newer model
/// from an older one the plan still serves (`grok-4.20` is a 2025 model, older than `grok-4.7`), so
/// a catalog id needs DATE evidence. Per occurrence: a page id (no dates) or a catalog entry without
/// `created` never qualifies; when the seated id is in the same catalog with a date, the candidate
/// must be created strictly later; when it is not, the candidate must be created within
/// [`UNANCHORED_MAX_AGE_DAYS`]. Any qualifying occurrence qualifies; otherwise the reason.
fn public_verdict(
    current: &[SourceList],
    seated: &str,
    cand: &str,
    now_unix: i64,
) -> Result<(), &'static str> {
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
        match seated_created(h.source, seated, h.vendor) {
            Some(cs) if c > cs => return Ok(()),
            Some(_) => note(4, "older than the seated model by the catalog's date"),
            None if c >= now_unix - UNANCHORED_MAX_AGE_DAYS * 86_400 => return Ok(()),
            None => note(3, "older than 180 days by the catalog's date"),
        }
    }
    Err(why.1)
}

/// [`swap_plan`]'s candidates only.
pub fn swap_candidates(
    roster: &Roster,
    current: &[SourceList],
    now_unix: i64,
) -> Vec<SwapCandidate> {
    swap_plan(roster, current, now_unix).0
}

/// The seated ids with an available successor ([`successor`]: same prefix and suffix, a higher
/// version - a higher major included), and the public ids rejected for lack of date evidence. The
/// candidates of an entry are the ids of its own source (if it has one - what the plan offers, no
/// dates needed) PLUS the public catalog ids with the vendor part stripped that pass
/// [`public_verdict`]. When the version rule's pick among ALL ids is a public id that fails the
/// verdict, it is returned as rejected (with the reason) and the best qualifying id, if any, is the
/// candidate. Entries of inactive plans are never touched (iterated via `active_entries`);
/// protected and on-request entries ARE returned (flagged) so the pass can announce them, never
/// smoke them.
pub fn swap_plan(
    roster: &Roster,
    current: &[SourceList],
    now_unix: i64,
) -> (Vec<SwapCandidate>, Vec<RejectedCandidate>) {
    let public = public_candidates(current);
    let mut out = Vec::new();
    let mut rejected = Vec::new();
    for e in roster.active_entries() {
        if e.model.is_empty() {
            continue;
        }
        let engine = if e.engine.is_empty() {
            "codex".to_string()
        } else {
            e.engine.clone()
        };
        let own: &[String] = source_for(e)
            .and_then(|source| current.iter().find(|s| s.source == source))
            .map(|s| s.ids.as_slice())
            .unwrap_or(&[]);
        let others: Vec<&String> = public.iter().filter(|p| !own.contains(p)).collect();
        let mut all: Vec<String> = own.to_vec();
        all.extend(others.iter().map(|p| (*p).clone()));
        let mut cands: Vec<String> = own.to_vec();
        cands.extend(
            others
                .iter()
                .filter(|p| public_verdict(current, &e.model, p, now_unix).is_ok())
                .map(|p| (*p).clone()),
        );
        if let Some(raw) = successor(&e.model, &all) {
            if !own.iter().any(|o| o == raw) {
                if let Err(why) = public_verdict(current, &e.model, raw, now_unix) {
                    rejected.push(RejectedCandidate {
                        provider: e.provider.clone(),
                        engine: engine.clone(),
                        old: e.model.clone(),
                        new: raw.to_string(),
                        why: why.to_string(),
                    });
                }
            }
        }
        if let Some(succ) = successor(&e.model, &cands) {
            out.push(SwapCandidate {
                position: e.position,
                provider: e.provider.clone(),
                engine,
                old: e.model.clone(),
                new: succ.to_string(),
                from_public: !own.iter().any(|o| o == succ),
                protected: e.reserve_percent > 0,
                on_request: e.on_request,
            });
        }
    }
    (out, rejected)
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

/// The published model names the maintain state holds: every id of a PUBLIC source (a catalog or a
/// page - never an endpoint's own list, which may be private), lower-cased, both as recorded
/// (`qwen/qwen4-max`) and with the vendor part stripped (`qwen4-max`). Empty when there is no state.
pub fn published_from(state: &ModelsState) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for s in state.sources.iter().filter(|s| is_public_source(&s.source)) {
        for id in &s.ids {
            out.insert(id.to_ascii_lowercase());
            out.insert(strip_vendor(id).to_ascii_lowercase());
        }
    }
    out
}

/// [`published_from`] of the maintain state on disk, read once per process (telemetry asks per
/// event). Always empty in this crate's unit tests, so they never depend on the machine's state.
pub fn published_ids() -> &'static std::collections::HashSet<String> {
    static P: std::sync::OnceLock<std::collections::HashSet<String>> = std::sync::OnceLock::new();
    P.get_or_init(|| {
        if cfg!(test) {
            return Default::default();
        }
        super::state_dir()
            .map(|d| published_from(&super::read_state(&d).models))
            .unwrap_or_default()
    })
}

/// Seat a successor by editing exactly the target entry's `model` in the roster file, keeping the
/// file's formatting: copy to `<path>.bak-maintain-<yyyymmdd>`, replace that one `model` value,
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
    let updated = replace_entry_model(basis, provider, old, new)?;
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

/// Replace the `model` value of exactly the entry whose provider is `provider` and whose model is
/// `old` with `new`, leaving the rest of the file byte-for-byte unchanged. A single `"model":
/// "old"` occurrence is replaced directly; several are disambiguated by the one whose enclosing
/// object also names `provider`. Zero matches, or an ambiguity the provider cannot resolve, is an
/// error (the swap is then skipped, never guessed).
pub fn replace_entry_model(
    text: &str,
    provider: &str,
    old: &str,
    new: &str,
) -> Result<String, String> {
    if !valid_model_id(new) {
        return Err(format!("the successor id {new:?} is not a valid model id"));
    }
    let needle_positions = find_model_values(text, old);
    let chosen = match needle_positions.as_slice() {
        [] => return Err(format!("the model {old} was not found in the roster")),
        [only] => *only,
        many => {
            let matching: Vec<(usize, usize)> = many
                .iter()
                .copied()
                .filter(|&(vs, _)| object_names_provider(text, vs, provider))
                .collect();
            match matching.as_slice() {
                [one] => *one,
                _ => {
                    return Err(format!(
                        "the model {old} is ambiguous in the roster (provider {provider})"
                    ))
                }
            }
        }
    };
    let (vstart, vend) = chosen;
    let mut out = String::with_capacity(text.len() + new.len());
    out.push_str(&text[..vstart]);
    out.push_str(new);
    out.push_str(&text[vend..]);
    Ok(out)
}

/// The (start, end) byte ranges of the VALUE string of every `"model": "<value>"` whose value is
/// exactly `value` (the range covers the value text only, inside the quotes).
fn find_model_values(text: &str, value: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while let Some(rel) = text[i..].find("\"model\"") {
        let key = i + rel;
        // skip to the opening quote of the value: past "model", optional ws, ':', optional ws, '"'
        let mut j = key + "\"model\"".len();
        while j < bytes.len() && (bytes[j] as char).is_whitespace() {
            j += 1;
        }
        if j < bytes.len() && bytes[j] == b':' {
            j += 1;
            while j < bytes.len() && (bytes[j] as char).is_whitespace() {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'"' {
                let vstart = j + 1;
                // the value ends at the next unescaped quote
                let mut k = vstart;
                while k < bytes.len() {
                    if bytes[k] == b'\\' {
                        k += 2;
                        continue;
                    }
                    if bytes[k] == b'"' {
                        break;
                    }
                    k += 1;
                }
                if k <= bytes.len() && &text[vstart..k] == value {
                    out.push((vstart, k));
                }
            }
        }
        i = key + "\"model\"".len();
    }
    out
}

/// Whether the JSON object enclosing byte `pos` contains a `"provider": "<provider>"` member.
fn object_names_provider(text: &str, pos: usize, provider: &str) -> bool {
    let bytes = text.as_bytes();
    // scan back to the enclosing '{' (depth-aware)
    let mut depth = 0i32;
    let mut start = None;
    let mut i = pos;
    loop {
        if i == 0 {
            break;
        }
        i -= 1;
        match bytes[i] {
            b'}' => depth += 1,
            b'{' => {
                if depth == 0 {
                    start = Some(i);
                    break;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    let Some(start) = start else {
        return false;
    };
    // scan forward to the matching '}'
    let mut depth = 0i32;
    let mut end = text.len();
    let mut j = start;
    while j < bytes.len() {
        match bytes[j] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = j + 1;
                    break;
                }
            }
            _ => {}
        }
        j += 1;
    }
    let slice = &text[start..end];
    let marker = format!("\"{provider}\"");
    slice.contains("\"provider\"") && slice.contains(&marker)
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
        let cur = vec![
            sl("endpoint:alibaba.example", &["qwen4-max"]),
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
        // the entry's OWN list keeps today's rule (no dates needed)
        let cur = vec![
            sl("grok", &["grok-4.7", "grok-4.20"]),
            dated("web:openrouter.ai", &[("x-ai/grok-4.7", Some(at(30, now)))]),
        ];
        assert_eq!(swap_candidates(&roster, &cur, now)[0].new, "grok-4.20");
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
        };
        let news = family_news(&roster, &prev, &cur, now);
        assert_eq!(news[0].ids, vec!["gemini-3.9-pro"]);
        // nothing new -> nothing announced
        let prev = ModelsState {
            sources: cur.clone(),
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
        };
        let news = family_news(&roster, &prev, &cur, 0);
        assert_eq!(news[0].ids.len(), FAMILY_NEWS_PER_ENTRY);
    }

    #[test]
    fn published_names_come_from_public_sources_only() {
        let st = ModelsState {
            sources: vec![
                sl("web:openrouter.ai", &["qwen/Qwen4-Max"]),
                sl("page:zai-docs", &["glm-6"]),
                sl("endpoint:api.z.ai", &["glm-9-secret"]),
                sl("grok", &["grok-4.7"]),
            ],
        };
        let p = published_from(&st);
        assert!(p.contains("qwen/qwen4-max"));
        assert!(p.contains("qwen4-max"));
        assert!(p.contains("glm-6"));
        assert!(!p.contains("glm-9-secret"));
        assert!(!p.contains("grok-4.7"));
    }

    #[test]
    fn replace_single_occurrence_keeps_formatting() {
        let text = r#"{
  "reviewers": [
    { "provider": "grok", "engine": "grok", "model": "grok-4.7", "plan": "supergrok" }
  ]
}"#;
        let out = replace_entry_model(text, "grok", "grok-4.7", "grok-4.8").unwrap();
        assert!(out.contains("\"model\": \"grok-4.8\""));
        assert_eq!(out, text.replace("grok-4.7", "grok-4.8"));
    }

    #[test]
    fn replace_disambiguates_by_provider_when_model_repeats() {
        let text = r#"{"reviewers":[
          {"provider":"a","model":"m1"},
          {"provider":"b","model":"m1"}
        ]}"#;
        // same model string in two entries → provider disambiguates
        let out = replace_entry_model(text, "b", "m1", "m2").unwrap();
        assert_eq!(out.matches("\"model\":\"m2\"").count(), 1);
        assert!(out.contains("{\"provider\":\"b\",\"model\":\"m2\"}"));
        assert!(out.contains("{\"provider\":\"a\",\"model\":\"m1\"}"));
    }

    #[test]
    fn replace_errors_when_absent() {
        let text = r#"{"reviewers":[{"provider":"a","model":"m1"}]}"#;
        assert!(replace_entry_model(text, "a", "nope", "x").is_err());
    }

    #[test]
    fn replace_refuses_a_hostile_successor_id() {
        // a quote-bearing successor can never be spliced: refused, the roster text is not consulted
        let text = r#"{"reviewers":[{"provider":"a","model":"m1"}]}"#;
        assert!(replace_entry_model(text, "a", "m1", "m2\",\"x\":\"y").is_err());
        assert!(replace_entry_model(text, "a", "m1", "m2\nextra").is_err());
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
        apply_swap(&path_s, basis, "grok", "grok-4.7", "grok-4.8", "20261115").unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("grok-4.8"));

        // the operator PROTECTS the entry mid-smoke: on disk the roster now differs from the basis
        // the gate judged, so a swap computed from that stale basis aborts and writes nothing.
        let protected = r#"{"roster_version":1,"reviewers":[{"provider":"grok","engine":"grok","model":"grok-4.8","plan":"supergrok","ext":{"reserve_percent":15}}]}"#;
        std::fs::write(&path, protected).unwrap();
        let err =
            apply_swap(&path_s, basis, "grok", "grok-4.7", "grok-4.9", "20261116").unwrap_err();
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
        let cur = vec![sl("endpoint:api.z.ai", &["glm-5.3", "glm-5.4"])];
        let c = swap_candidates(&roster, &cur, 0);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].old, "glm-5.3");
        assert_eq!(c[0].new, "glm-5.4");
        assert_eq!(c[0].provider, "ZAI-claude");
    }
}
