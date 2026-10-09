//! The reviewer roster format (`Read-ReviewerRoster`): fail-closed validation of the
//! roster JSON, plus the `codex_config` / `-CodexConfig` item rules
//! (`ConvertFrom-CodexConfigItems`).
//!
//! Wave 26 (companions decisions D1, D8, D12) adds four keys, all validated here to the
//! plugin's exact refusal wording: the per-entry `lab` (a non-empty string, kept canonical
//! lowercase - D1), `roles` (an array of role slugs the entry is willing to take - D8) and
//! `ext` (an object, the extension point of other implementations; validated only, never
//! read or written - D12), and the top-level `require` (`{"<purpose>": ["<reviewer>", ...]}`:
//! the reviewers a `-Panel` of that purpose must include - D7; every matcher must name an
//! entry at load or the roster is unusable) and `ext`. `roster_version` stays `1`.
//!
//! 0.6.0 (wave 29b, E5/E7 - C3 wave 1b): the per-entry `plan` (a slug naming the coding plan
//! whose quota the entry's route spends, on an entry of ANY engine - [`PLAN_SLUG_RE`]) and the
//! key `endpoint`; a `parallel` key may name a plan.
//!
//! 0.6.0 (wave 29 / 29b - C3 wave 4): the `claude` engine - `auth` `subscription` (the default) |
//! `api-key` | `endpoint`, a model of the engine's closed table (`[1m]` allowed) or, with auth
//! `endpoint`, the provider's own id (never an Anthropic one, E11), and the `endpoint` object
//! `{base_url, env_key, timeout_ms}` (required with auth `endpoint`, refused everywhere else).

use serde_json::Value;

use crate::lineage::{engine_spec, format_lineage, ENGINE_NAMES};

/// The consult purposes (`$script:ConsultPurposes`), the keys `require` may name.
pub const CONSULT_PURPOSES: &[&str] = &[
    "framing",
    "decision",
    "checkpoint",
    "core-contract",
    "acceptance",
    "diff-review",
    "stuck",
    "chore",
];

/// (0.6.0, wave 29b E5) The plan slug: 2 to 32 characters - lowercase letters, digits and `-`,
/// starting with a letter (`$script:PlanSlugRe`, matched case-sensitively).
pub const PLAN_SLUG_RE: &str = r"^[a-z][a-z0-9-]{1,31}$";

/// Whether `value` is a plan slug ([`PLAN_SLUG_RE`]).
pub fn is_plan_slug(value: &str) -> bool {
    regex::Regex::new(PLAN_SLUG_RE).unwrap().is_match(value)
}

/// (0.6.0, wave 29) A claude model's 1M-context suffix `[1m]` (any case) stripped - the rest of the
/// name obeys the roster's string rules.
fn strip_1m_suffix(v: &str) -> String {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"(?i)\[1m\]$").unwrap())
        .replace(v, "")
        .to_string()
}

/// The refusal `why` of a `plan` value that is not a slug (`at` is `entry <n>`; `got` the
/// value as compact JSON) - the plugin's exact wording.
pub fn plan_slug_problem(at: &str, got: &str) -> String {
    format!(
        "{at}: plan must be a slug of 2 to 32 characters - lowercase letters, digits and \"-\", starting with a letter (e.g. \"zai\"; got {got})"
    )
}

/// One validated roster entry.
#[derive(Debug, Clone, Default)]
pub struct RosterEntry {
    pub position: usize,
    pub provider: String,
    pub model: String,
    pub codex_config: Vec<String>,
    pub auth: String,
    /// `always` (the default), `weighty` or (0.6.0) `light`: with -Panel a weighty entry joins
    /// only on the weighty purposes (or -PanelAll); a light one joins on the light purposes and
    /// stands in on a weighty purpose only when no other entry of its provider label runs.
    pub panel: String,
    pub engine: String,
    pub engine_declared: bool,
    /// (wave 26, D1) the lab behind the model, canonical lowercase; `""` when the entry names
    /// none (the panel then derives it from the model id's prefix).
    pub lab: String,
    /// (wave 26, D8) the role slugs the entry is willing to take under a panel's `-Roles`.
    pub roles: Vec<String>,
    /// (wave 26b, D11) the member's own turn timeout in seconds (>= 60); `0` when the entry
    /// names none (the run then takes the purpose default). An explicit `--timeout-sec` still wins.
    pub timeout_sec: i64,
    /// (wave 26b, D12) the member's stall cut in seconds without an event (>= 0; `0` = off); `-1`
    /// when the entry names none (the run then takes `--stall-sec`, else 900).
    pub stall_sec: i64,
    /// (wave 26b, D16) the reviewer's context window in tokens (>= 32000); `0` when the entry
    /// names none (no context budgeting for this reviewer).
    pub context_tokens: i64,
    /// (0.6.0, wave 29b E5) the coding plan whose quota this entry's route spends (a slug,
    /// [`PLAN_SLUG_RE`]); `""` when the entry names none. Every entry of one plan shares its
    /// quota (a usage limit on one route marks the others out) and its scheduling group.
    pub plan: String,
    /// (0.6.0, wave 29b E1) The claude entry's endpoint (auth `endpoint` only; its `plan` is the
    /// entry's).
    pub endpoint: Option<crate::claude::ClaudeEndpoint>,
}

/// The roster (`Read-ReviewerRoster`'s result).
#[derive(Debug, Clone, Default)]
pub struct Roster {
    pub exists: bool,
    pub path: String,
    pub disabled: bool,
    pub entries: Vec<RosterEntry>,
    /// Ordinal, insertion-ordered `parallel` map: label -> n.
    pub parallel: Vec<(String, i64)>,
    /// (wave 26, D7) ordinal, insertion-ordered `require` map: purpose -> the reviewer
    /// matchers a `-Panel` of that purpose must include (each already validated to name an
    /// entry). Empty when the roster names no `require`.
    pub require: Vec<(String, Vec<String>)>,
    /// (M7b-b) The `http` reviewers parsed from the top-level `ext.c3.reviewers` extension
    /// (invisible to the plugin, which validates `ext` as an object only). They are also
    /// appended to [`Roster::entries`] as synthesized `http` entries so the panel and
    /// `c3 providers` see them; this list keeps their full request config, looked up by
    /// [`RosterEntry::position`]. Populated by the `c3` roster loader, never by
    /// [`validate_roster`] (which stays byte-parity with the plugin's validator).
    pub http_reviewers: Vec<crate::roster_ext::HttpReviewer>,
    pub error: String,
}

/// Every roster position of a `(provider, engine)` label, in roster order (`[]` when
/// none). This is the data behind the JSON `roster_positions` (wave 24b) and the
/// table's ROSTER column.
pub fn positions_for(entries: &[RosterEntry], provider: &str, engine: &str) -> Vec<i64> {
    entries
        .iter()
        .filter(|e| e.provider == provider && e.engine == engine)
        .map(|e| e.position as i64)
        .collect()
}

fn is_json_integer(v: &Value) -> bool {
    if v.is_i64() || v.is_u64() {
        return true;
    }
    if let Some(f) = v.as_f64() {
        return f.fract() == 0.0;
    }
    false
}

fn compact(v: &Value) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

/// `Get-RosterStringProblem` (wave 26b, D3): the first delimiter of the reviewer matcher
/// (`::`, `[`, `]`) or of the seed text (`|`, `,`) - or `#` (a position matcher) - found in a
/// roster string, quoted, else `None`. Blanks around the value are refused by the callers'
/// own checks.
fn roster_string_problem(value: &str) -> Option<&'static str> {
    ["::", "[", "]", "|", ",", "#"]
        .into_iter()
        .find(|d| value.contains(d))
}

/// `Get-IdentityStringProblem` (wave 27c, D10 / F30-9): THE character rule of a provider label and
/// a model id that the roster's validator and `CODEX_CONSULT_COORDINATOR` share - a non-empty
/// string without surrounding blanks and without the matcher's and the seed's delimiters
/// ([`roster_string_problem`]). Interior blanks are allowed. `None`, or why not (`is empty`,
/// `has surrounding blanks`, `must not contain '::'`).
pub fn identity_string_problem(value: &str) -> Option<String> {
    if value.trim().is_empty() {
        return Some("is empty".to_string());
    }
    if value != value.trim() {
        return Some("has surrounding blanks".to_string());
    }
    roster_string_problem(value).map(|d| format!("must not contain '{d}'"))
}

/// `ConvertTo-TomlBasicString`: escape backslash and double quote.
fn to_toml_basic_string(v: &str) -> String {
    v.replace('\\', "\\\\").replace('"', "\\\"")
}

/// `ConvertFrom-CodexConfigItems`: (items, error). `home_dir` supplies `~` expansion.
pub fn convert_from_codex_config_items(
    values: &[String],
    label: &str,
    home_dir: Option<&str>,
) -> (Vec<String>, String) {
    let mut items = Vec::new();
    let key_re = regex::Regex::new(r"^[A-Za-z0-9_.]+=.+$").unwrap();
    let tilde_re = regex::Regex::new(r"^~[\\/]").unwrap();
    let literal_re = regex::Regex::new(r#"^(["'0-9\[{]|true$|false$)"#).unwrap();
    let reserved = [
        "model",
        "model_provider",
        "profile",
        "model_reasoning_effort",
        "model_providers",
    ];
    for cfg_arg in values {
        let trimmed = cfg_arg.trim();
        if trimmed.is_empty() {
            continue;
        }
        for item in split_config_items(trimmed) {
            let item = item.trim();
            if !key_re.is_match(item) {
                return (
                    Vec::new(),
                    format!("{label} '{item}' is malformed: expected key=value (key: letters, digits, _ and .; a non-empty value)."),
                );
            }
            let eq = item.find('=').unwrap();
            let key = &item[..eq];
            let mut value = item[eq + 1..].to_string();
            if reserved.contains(&key) || key.starts_with("model_providers.") {
                return (
                    Vec::new(),
                    format!("{label} '{item}' is refused: {key} is part of the reviewer identity and effort the bridge records (use -Model / -Provider / -Effort; providers belong in the Codex config)."),
                );
            }
            if tilde_re.is_match(&value) {
                let home = home_dir.unwrap_or("");
                let home = home.trim_end_matches(['\\', '/']);
                value = format!("{home}{}", &value[1..]).replace('\\', "/");
            }
            if !literal_re.is_match(&value) {
                value = format!("\"{}\"", to_toml_basic_string(&value));
            }
            items.push(format!("{key}={value}"));
        }
    }
    (items, String::new())
}

/// Split a `-CodexConfig` string at commas that begin the next `key=` (the PowerShell
/// `,(?=\s*[A-Za-z0-9_.]+=)`, without lookahead).
fn split_config_items(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let key_start = regex::Regex::new(r"^\s*[A-Za-z0-9_.]+=").unwrap();
    let mut parts = Vec::new();
    let mut start = 0usize;
    for i in 0..chars.len() {
        if chars[i] == ',' {
            let rest: String = chars[i + 1..].iter().collect();
            if key_start.is_match(&rest) {
                parts.push(chars[start..i].iter().collect());
                start = i + 1;
            }
        }
    }
    parts.push(chars[start..].iter().collect());
    parts
}

/// `ConvertTo-SlugList` (wave 26, D3/D8): split every value at commas, trim, lower-case, drop
/// the empties, dedupe keeping the first, and validate each against the slug pattern
/// `^[a-z0-9][a-z0-9._-]{0,40}$`. Returns `(items, error)`; `error` is the plugin's exact
/// refusal (empty on success).
pub fn convert_to_slug_list(values: &[String], what: &str) -> (Vec<String>, String) {
    let re = regex::Regex::new(r"^[a-z0-9][a-z0-9._-]{0,40}$").unwrap();
    let mut items: Vec<String> = Vec::new();
    for v in values {
        for piece in v.split(',') {
            let trimmed = piece.trim();
            let s = trimmed.to_lowercase();
            if s.is_empty() {
                continue;
            }
            if !re.is_match(&s) {
                return (
                    Vec::new(),
                    format!("{what} '{trimmed}' is not a slug (lowercase letters, digits, dot, dash, underscore; at most 41 characters, starting with a letter or digit)"),
                );
            }
            if !items.contains(&s) {
                items.push(s);
            }
        }
    }
    (items, String::new())
}

/// `Resolve-ReviewerMatcher` (wave 26, D7): resolve one `-Require` / roster `require` matcher
/// against the entries validated so far - `#<n>` (the entry at position n), a bare provider
/// label (every entry of it), or `<provider> :: <model>`, either form with an optional
/// ` [<engine>]` suffix. Compared field by field on what the roster names (provider and model
/// ordinal, the engine), never on a display string. Returns `(positions, error)`; `positions`
/// is in roster order, `error` is the plugin's exact refusal (empty on success).
pub fn resolve_reviewer_matcher(entries: &[RosterEntry], matcher: &str) -> (Vec<i64>, String) {
    let fail = |why: String| (Vec::new(), why);
    let g = match parse_reviewer_matcher_grammar(matcher) {
        Ok(g) => g,
        Err(why) => return fail(why),
    };
    if let Some(pos) = g.position {
        let hit: Vec<i64> = entries
            .iter()
            .filter(|e| e.position as i64 == pos)
            .map(|e| e.position as i64)
            .collect();
        if hit.is_empty() {
            return fail(format!(
                "'{}' names no roster position (the roster has {} entries)",
                matcher.trim(),
                entries.len()
            ));
        }
        return (hit, String::new());
    }
    let provider = g.provider.unwrap_or_default();
    let hits: Vec<i64> = entries
        .iter()
        .filter(|e| {
            e.provider == provider
                && g.model.as_ref().is_none_or(|m| &e.model == m)
                && g.engine.as_ref().is_none_or(|en| &e.engine == en)
        })
        .map(|e| e.position as i64)
        .collect();
    if hits.is_empty() {
        return fail(format!("'{matcher}' matches no roster entry"));
    }
    (hits, String::new())
}

/// The parsed shape of a reviewer matcher, independent of any roster (`parse_reviewer_matcher_grammar`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MatcherGrammar {
    /// A `#<n>` roster position, when the matcher is one.
    pub position: Option<i64>,
    /// The provider label (`None` only for a `#<n>` position).
    pub provider: Option<String>,
    /// The model, when the matcher is `<provider> :: <model>`.
    pub model: Option<String>,
    /// The engine of a ` [<engine>]` suffix.
    pub engine: Option<String>,
}

/// (wave 27c, D10) The ONE grammar of a reviewer matcher — a `#<n>` position, a bare provider
/// label, or `<provider> :: <model>`, any form with an optional ` [<engine>]` suffix — shared by
/// the roster's `Resolve-ReviewerMatcher` and the `CODEX_CONSULT_COORDINATOR` matcher, so what the
/// roster accepts as a provider/model/engine string the coordinator value accepts. `Err` carries
/// the plugin's exact refusal `<why>` (the caller wraps it). Membership is NOT checked here — that
/// is the roster's (or the coordinator's) job.
pub fn parse_reviewer_matcher_grammar(matcher: &str) -> Result<MatcherGrammar, String> {
    let t = matcher.trim();
    if t.is_empty() {
        return Err("an empty reviewer matcher".to_string());
    }
    let pos_re = regex::Regex::new(r"^#(\d+)$").unwrap();
    if let Some(c) = pos_re.captures(t) {
        return Ok(MatcherGrammar {
            position: Some(c[1].parse().unwrap_or(0)),
            ..Default::default()
        });
    }
    let mut rest = t.to_string();
    let mut engine: Option<String> = None;
    let eng_re = regex::Regex::new(r"^(.*\S)\s+\[([A-Za-z0-9_-]+)\]$").unwrap();
    if let Some(c) = eng_re.captures(t) {
        rest = c[1].to_string();
        let e = c[2].to_lowercase();
        if !ENGINE_NAMES.contains(&e.as_str()) {
            return Err(format!(
                "'{t}' names the engine '{e}' (known: {})",
                ENGINE_NAMES.join(", ")
            ));
        }
        engine = Some(e);
    }
    let mut provider = rest.clone();
    let mut model: Option<String> = None;
    if let Some(sep) = rest.find(" :: ") {
        let p = rest[..sep].trim().to_string();
        let m = rest[sep + 4..].trim().to_string();
        if p.is_empty() || m.is_empty() {
            return Err(format!("'{t}' is not '<provider> :: <model>'"));
        }
        provider = p;
        model = Some(m);
    }
    // (wave 5) no character rule here - `ConvertFrom-ReviewerMatcher` has none: a label or model
    // no roster entry has simply matches nothing (-Require), and `CODEX_CONSULT_COORDINATOR`
    // applies `identity_string_problem` after the parse (`Resolve-CoordinatorIdentity`).
    Ok(MatcherGrammar {
        position: None,
        provider: Some(provider),
        model,
        engine,
    })
}

/// Validate the roster JSON text of a file known to exist and be readable.
/// `home_dir` is used only for `codex_config` `~` expansion.
pub fn validate_roster(path: &str, text: &str, home_dir: Option<&str>) -> Roster {
    let mut r = Roster {
        exists: true,
        path: path.to_string(),
        ..Default::default()
    };
    let data: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            r.error = roster_refusal(
                path,
                &format!("it does not parse: {}", crate::one_line(&e.to_string())),
            );
            return r;
        }
    };
    let obj = match data.as_object() {
        Some(o) => o,
        None => {
            r.error = roster_refusal(path, "the top level is not a JSON object");
            return r;
        }
    };
    let mut why = String::new();
    for key in obj.keys() {
        if !["roster_version", "reviewers", "parallel", "require", "ext"].contains(&key.as_str()) {
            why = format!("unknown key '{key}' at the top level (allowed: roster_version, reviewers, parallel, require, ext)");
            break;
        }
    }
    // (wave 26, D12) top-level ext: validated as an object only (checked before roster_version,
    // matching the plugin's order so the same refusal wins when several keys are wrong).
    if why.is_empty() {
        if let Some(xv) = obj.get("ext") {
            if !xv.is_object() {
                why = format!(
                    "ext must be an object (the extension point of other implementations; got {})",
                    compact(xv)
                );
            }
        }
    }
    if why.is_empty() {
        match obj.get("roster_version") {
            None => why = "roster_version is missing (expected 1)".into(),
            Some(v) if !is_json_integer(v) || v.as_i64() != Some(1) => {
                why = format!("roster_version must be 1 (got {})", compact(v));
            }
            _ => {}
        }
    }
    if why.is_empty() {
        match obj.get("reviewers") {
            None => why = "reviewers is missing".into(),
            Some(v) if !v.is_array() => why = "reviewers is not an array".into(),
            Some(v) if v.as_array().unwrap().is_empty() => why = "reviewers is empty".into(),
            _ => {}
        }
    }
    let mut entries: Vec<RosterEntry> = Vec::new();
    if why.is_empty() {
        let reviewers = obj["reviewers"].as_array().unwrap();
        for (idx, item) in reviewers.iter().enumerate() {
            let pos = idx + 1;
            let at = format!("entry {pos}");
            let iobj = match item.as_object() {
                Some(o) => o,
                None => {
                    why = format!("{at} is not an object");
                    break;
                }
            };
            for key in iobj.keys() {
                if ![
                    "provider",
                    "model",
                    "codex_config",
                    "auth",
                    // (0.6.0, wave 29b E1/E5) the claude engine's endpoint block (refused below on
                    // every entry C3 can run) and the plan of any entry
                    "endpoint",
                    "plan",
                    "panel",
                    "engine",
                    "lab",
                    "roles",
                    "timeout_sec",
                    "stall_sec",
                    "context_tokens",
                    "ext",
                ]
                .contains(&key.as_str())
                {
                    why = format!(
                        "{at} has an unknown key '{key}' (allowed: provider, model, codex_config, auth, endpoint, plan, panel, engine, lab, roles, timeout_sec, stall_sec, context_tokens, ext)"
                    );
                    break;
                }
            }
            if !why.is_empty() {
                break;
            }
            // (wave 26b, D3 / F22-2, F22-4) the matcher's and the seed's delimiters never
            // inside a provider label, a model or an engine: '::', '[', ']', '|', ',', '#'.
            // (0.6.0, wave 29) a claude model may end with the 1M-context suffix [1m] - the rest
            // obeys the rule.
            let is_claude_item = iobj.get("engine").and_then(|v| v.as_str()) == Some("claude");
            let strip_1m = strip_1m_suffix;
            let mut d3_bad = false;
            for sk in ["provider", "model", "engine"] {
                if let Some(sv) = iobj.get(sk).and_then(|v| v.as_str()) {
                    let sv = if sk == "model" && is_claude_item {
                        strip_1m(sv)
                    } else {
                        sv.to_string()
                    };
                    if let Some(bad) = roster_string_problem(&sv) {
                        why = format!("roster entry #{pos}: {sk} must not contain '{bad}'");
                        d3_bad = true;
                        break;
                    }
                }
            }
            if d3_bad {
                break;
            }
            // (wave 26b, D11) timeout_sec: an integer from 60 to 86400 (seconds). The three numeric
            // keys come first, then ext and the plan - the plugin's per-entry order (0.6.0).
            let mut timeout_sec: i64 = 0;
            if let Some(tv) = iobj.get("timeout_sec") {
                if !is_json_integer(tv)
                    || tv.as_f64().unwrap_or(0.0) < 60.0
                    || tv.as_f64().unwrap_or(0.0) > 86400.0
                {
                    why = format!(
                        "{at}: timeout_sec must be an integer from 60 to 86400 (seconds; got {})",
                        compact(tv)
                    );
                    break;
                }
                timeout_sec = tv.as_i64().unwrap_or(0);
            }
            // (wave 26b, D16) context_tokens: an integer from 32000 to 100000000.
            let mut context_tokens: i64 = 0;
            if let Some(cv) = iobj.get("context_tokens") {
                if !is_json_integer(cv)
                    || cv.as_f64().unwrap_or(0.0) < 32000.0
                    || cv.as_f64().unwrap_or(0.0) > 100_000_000.0
                {
                    why = format!(
                        "{at}: context_tokens must be an integer from 32000 to 100000000 (the reviewer's context window in tokens, e.g. 256000; got {})",
                        compact(cv)
                    );
                    break;
                }
                context_tokens = cv.as_i64().unwrap_or(0);
            }
            // (wave 26b, D12) stall_sec: an integer from 0 (off) to 86400 (seconds without an event).
            let mut stall_sec: i64 = -1;
            if let Some(sv) = iobj.get("stall_sec") {
                if !is_json_integer(sv)
                    || sv.as_f64().unwrap_or(-1.0) < 0.0
                    || sv.as_f64().unwrap_or(-1.0) > 86400.0
                {
                    why = format!(
                        "{at}: stall_sec must be an integer from 0 (off) to 86400 (seconds without an event; got {})",
                        compact(sv)
                    );
                    break;
                }
                stall_sec = sv.as_i64().unwrap_or(-1);
            }
            // ext (D12): validated as an object, otherwise ignored.
            if let Some(xv) = iobj.get("ext") {
                if !xv.is_object() {
                    why = format!("{at}: ext must be an object (the extension point of other implementations; got {})", compact(xv));
                    break;
                }
            }
            // (0.6.0, wave 29b E5) the plan (the quota a route shares with the other routes to the
            // same coding plan): a slug, on any entry of any engine.
            let mut plan = String::new();
            if let Some(pv) = iobj.get("plan") {
                match pv.as_str() {
                    Some(s) if is_plan_slug(s) => plan = s.to_string(),
                    _ => {
                        why = plan_slug_problem(&at, &compact(pv));
                        break;
                    }
                }
            }
            // lab (D1): a non-empty string without surrounding blanks, kept canonical lowercase.
            let mut lab = String::new();
            if let Some(lv) = iobj.get("lab") {
                match lv.as_str() {
                    Some(s) if !s.trim().is_empty() && s == s.trim() => {
                        lab = s.to_lowercase();
                    }
                    _ => {
                        why = format!("{at}: lab must be a non-empty string without surrounding blanks (e.g. \"moonshot\"; omit it to take the lab from the model id)");
                        break;
                    }
                }
            }
            // roles (D8): an array of role slugs the entry is willing to take.
            let mut roles: Vec<String> = Vec::new();
            if let Some(rv) = iobj.get("roles") {
                let all_strings = rv
                    .as_array()
                    .map(|a| a.iter().all(|x| x.is_string()))
                    .unwrap_or(false);
                if !all_strings {
                    why = format!(
                        "{at}: roles must be an array of role names (e.g. [\"security\", \"tests\"])"
                    );
                    break;
                }
                let strs: Vec<String> = rv
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_str().unwrap().to_string())
                    .collect();
                let (parsed, err) = convert_to_slug_list(&strs, "role");
                if !err.is_empty() {
                    why = format!("{at}: roles: {err}");
                    break;
                }
                roles = parsed;
            }
            // provider
            let provider = match iobj.get("provider").and_then(|v| v.as_str()) {
                Some(s) if !s.trim().is_empty() && s == s.trim() => s.to_string(),
                _ => {
                    why = format!(
                        "{at} needs a provider: a non-empty string without surrounding blanks"
                    );
                    break;
                }
            };
            // model
            let mut model = String::new();
            if let Some(mv) = iobj.get("model") {
                // (wave 29) a claude model is checked without its [1m] suffix
                let checked = mv.as_str().map(|s| {
                    if is_claude_item {
                        strip_1m(s)
                    } else {
                        s.to_string()
                    }
                });
                match (mv.as_str(), checked) {
                    (Some(s), Some(c)) if !c.trim().is_empty() && c == c.trim() => {
                        model = s.to_string()
                    }
                    _ => {
                        why = format!("{at}: model must be a non-empty string without surrounding blanks (omit it to use the Codex config's model)");
                        break;
                    }
                }
            }
            // codex_config
            let mut cfg_items: Vec<String> = Vec::new();
            if let Some(cv) = iobj.get("codex_config") {
                let all_strings = cv
                    .as_array()
                    .map(|a| a.iter().all(|x| x.is_string()))
                    .unwrap_or(false);
                if !all_strings {
                    why = format!("{at}: codex_config must be an array of key=value strings");
                    break;
                }
                let strs: Vec<String> = cv
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_str().unwrap().to_string())
                    .collect();
                let (parsed, err) =
                    convert_from_codex_config_items(&strs, "codex_config", home_dir);
                if !err.is_empty() {
                    why = format!("{at}: {}", err.trim_end_matches('.'));
                    break;
                }
                cfg_items = parsed;
            }
            // auth - (0.6.0, wave 29) claude takes subscription | api-key | endpoint (checked below
            // once the engine is known)
            let mut auth = String::new();
            if let Some(av) = iobj.get("auth") {
                if is_claude_item {
                    match av.as_str() {
                        Some(a) if crate::claude::CLAUDE_AUTH_MODES.contains(&a) => {
                            auth = a.to_string()
                        }
                        _ => {
                            why = format!(
                                "{at}: auth of engine claude must be \"subscription\" (the claude.ai login, the default) or \"api-key\" (ANTHROPIC_API_KEY) or \"endpoint\" (a third-party Anthropic-compatible endpoint named by the entry's \"endpoint\") (got {})",
                                compact(av)
                            );
                            break;
                        }
                    }
                } else {
                    if av.as_str() != Some("none") {
                        why = format!("{at}: auth may only be \"none\" (an endpoint that needs no credential; omit it otherwise)");
                        break;
                    }
                    auth = "none".into();
                }
            }
            // panel
            let mut panel = "always".to_string();
            if let Some(pv) = iobj.get("panel") {
                match pv.as_str() {
                    Some(s) if s == "always" || s == "weighty" || s == "light" => {
                        panel = s.to_string()
                    }
                    _ => {
                        why = format!(
                            "{at}: panel must be \"always\", \"weighty\" or \"light\" (got {})",
                            compact(pv)
                        );
                        break;
                    }
                }
            }
            // engine
            let mut engine = "codex".to_string();
            let mut engine_declared = false;
            if let Some(ev) = iobj.get("engine") {
                match ev.as_str() {
                    Some(s) if ENGINE_NAMES.contains(&s) => {
                        engine = s.to_string();
                        engine_declared = true;
                    }
                    _ => {
                        why = format!(
                            "{at}: engine must be one of: {} (got {})",
                            ENGINE_NAMES.join(", "),
                            compact(ev)
                        );
                        break;
                    }
                }
            }
            if engine != "codex" {
                if model.is_empty() {
                    why = format!(
                        "{at}: engine {engine} needs a model (the full model id, e.g. {})",
                        engine_spec(&engine).map(|s| s.model_example).unwrap_or("")
                    );
                    break;
                }
                if iobj.contains_key("codex_config") {
                    why = format!("{at}: codex_config does not apply to engine {engine} (it configures codex exec)");
                    break;
                }
                if iobj.contains_key("auth") && engine != "claude" {
                    why = format!("{at}: auth does not apply to engine {engine} (the {engine} CLI keeps its own sign-in)");
                    break;
                }
            }
            // (0.6.0, wave 29, item 8, D4) a claude entry: a model of the engine's table; auth
            // defaults to subscription. (wave 29b, E1, E2) auth endpoint: the `endpoint` object is
            // REQUIRED (and refused with every other auth and engine), the model the open id pattern.
            let mut entry_endpoint: Option<crate::claude::ClaudeEndpoint> = None;
            if engine == "claude" {
                if auth.is_empty() {
                    auth = "subscription".into();
                }
                let mp = crate::claude::model_problem(&model, &auth);
                if !mp.is_empty() {
                    why = format!("{at}: the claude model '{model}' {mp}");
                    break;
                }
            }
            if iobj.contains_key("endpoint") && !(engine == "claude" && auth == "endpoint") {
                why = format!(
                    "{at}: endpoint applies only to engine claude with auth \"endpoint\" (this entry: engine {engine}{})",
                    if engine == "claude" {
                        format!(", auth {auth}")
                    } else {
                        String::new()
                    }
                );
                break;
            }
            if engine == "claude" && auth == "endpoint" {
                let Some(ev) = iobj.get("endpoint") else {
                    why = format!("{at}: auth \"endpoint\" needs an \"endpoint\" object {{\"base_url\": \"https://...\", \"env_key\": \"<VARIABLE NAME>\"}} - the Anthropic-compatible endpoint and the variable that holds its token");
                    break;
                };
                match crate::claude::endpoint_from_value(ev, &plan) {
                    Ok(ep) => entry_endpoint = Some(ep),
                    Err(e) => {
                        why = format!("{at}: {e}");
                        break;
                    }
                }
            }
            if let Some(other) = entries
                .iter()
                .find(|e| e.provider == provider && e.engine != engine)
            {
                why = format!(
                    "entries {} and {pos} use the provider label '{provider}' with two engines ({}, {engine}); a label names one engine",
                    other.position, other.engine
                );
                break;
            }
            if let Some(dup) = entries
                .iter()
                .find(|e| e.provider == provider && e.model == model)
            {
                let l = if !model.is_empty() {
                    format_lineage(&provider, &model)
                } else {
                    format!("{provider} (no model)")
                };
                why = format!(
                    "entries {} and {pos} are the same reviewer {l}",
                    dup.position
                );
                break;
            }
            entries.push(RosterEntry {
                position: pos,
                provider,
                model,
                codex_config: cfg_items,
                auth,
                panel,
                engine,
                engine_declared,
                lab,
                roles,
                timeout_sec,
                stall_sec,
                context_tokens,
                plan,
                endpoint: entry_endpoint,
            });
        }
    }
    // parallel
    if why.is_empty() {
        if let Some(pv) = obj.get("parallel") {
            match pv.as_object() {
                None => {
                    why = format!(
                        "parallel must be an object {{\"<provider label>\": <n>}} (got {})",
                        compact(pv)
                    )
                }
                Some(pmap) => {
                    for (name, val) in pmap {
                        // (0.6.0, wave 29b E7) a key names a provider label or a plan (the plan's
                        // scheduling group)
                        if !entries
                            .iter()
                            .any(|e| &e.provider == name || (!e.plan.is_empty() && &e.plan == name))
                        {
                            why = format!("parallel names the provider label '{name}', which no entry of the roster uses (as its provider label or its plan)");
                            break;
                        }
                        if !is_json_integer(val) || val.as_f64().unwrap_or(0.0) < 1.0 {
                            why = format!(
                                "parallel.{name} must be an integer >= 1 (got {})",
                                compact(val)
                            );
                            break;
                        }
                        r.parallel.push((name.clone(), val.as_i64().unwrap_or(0)));
                    }
                }
            }
        }
    }
    // require (D7): {"<purpose>": ["<reviewer>", ...]}; every matcher must name an entry at
    // load, else the roster is unusable.
    if why.is_empty() {
        if let Some(qv) = obj.get("require") {
            match qv.as_object() {
                None => {
                    why = format!(
                    "require must be an object {{\"<purpose>\": [\"<reviewer>\", ...]}} (got {})",
                    compact(qv)
                )
                }
                Some(qmap) => {
                    for (name, val) in qmap {
                        if !CONSULT_PURPOSES.contains(&name.as_str()) {
                            why = format!(
                                "require names the purpose '{name}' (known: {})",
                                CONSULT_PURPOSES.join(", ")
                            );
                            break;
                        }
                        let arr = val.as_array();
                        let ok = arr.map(|a| !a.is_empty()).unwrap_or(false)
                            && arr
                                .unwrap()
                                .iter()
                                .all(|x| x.as_str().map(|s| !s.trim().is_empty()).unwrap_or(false));
                        if !ok {
                            why = format!("require.{name} must be a non-empty array of reviewers (\"#<position>\", \"<provider>\" or \"<provider> :: <model>\", optionally with \" [<engine>]\")");
                            break;
                        }
                        let matchers: Vec<String> = arr
                            .unwrap()
                            .iter()
                            .map(|x| x.as_str().unwrap().to_string())
                            .collect();
                        let mut bad = false;
                        for m in &matchers {
                            let (_positions, err) = resolve_reviewer_matcher(&entries, m);
                            if !err.is_empty() {
                                why = format!("require.{name}: {err}");
                                bad = true;
                                break;
                            }
                        }
                        if bad {
                            break;
                        }
                        r.require.push((
                            name.clone(),
                            matchers.iter().map(|m| m.trim().to_string()).collect(),
                        ));
                    }
                }
            }
        }
    }
    if !why.is_empty() {
        r.error = roster_refusal(path, &why);
        return r;
    }
    r.entries = entries;
    r
}

/// The full refusal message wrapper for a `why` (used by the runtime for the
/// not-a-file / empty cases too).
pub fn roster_refusal(path: &str, why: &str) -> String {
    format!(
        "the reviewer roster '{path}' is not usable: {why}. Fix it or move it aside - an existing roster is never ignored (CODEX_CONSULT_ROSTER names another file)."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(json: &str) -> Roster {
        let r = validate_roster("R.json", json, None);
        assert!(r.error.is_empty(), "unexpected refusal: {}", r.error);
        r
    }

    #[test]
    fn lab_roles_require_are_exposed() {
        let r = ok(r##"{
            "roster_version": 1,
            "reviewers": [
                {"provider": "openai", "model": "gpt-6-astra", "lab": "OpenAI", "roles": ["security", "tests"]},
                {"provider": "ZAI", "model": "glm-5.3"}
            ],
            "require": {"framing": ["#1", "ZAI"]},
            "ext": {"note": "ignored"}
        }"##);
        assert_eq!(r.entries[0].lab, "openai", "lab kept canonical lowercase");
        assert_eq!(r.entries[0].roles, vec!["security", "tests"]);
        assert!(r.entries[1].lab.is_empty());
        assert_eq!(
            r.require,
            vec![(
                "framing".to_string(),
                vec!["#1".to_string(), "ZAI".to_string()]
            )]
        );
    }

    #[test]
    fn lab_must_be_a_clean_string() {
        let r = validate_roster(
            "R.json",
            r##"{"roster_version":1,"reviewers":[{"provider":"openai","model":"m","lab":" moon "}]}"##,
            None,
        );
        assert!(
            r.error
                .contains("entry 1: lab must be a non-empty string without surrounding blanks"),
            "{}",
            r.error
        );
    }

    #[test]
    fn roles_must_be_string_array_of_slugs() {
        let bad_type = validate_roster(
            "R.json",
            r##"{"roster_version":1,"reviewers":[{"provider":"openai","model":"m","roles":"security"}]}"##,
            None,
        );
        assert!(
            bad_type
                .error
                .contains("entry 1: roles must be an array of role names"),
            "{}",
            bad_type.error
        );
        let bad_slug = validate_roster(
            "R.json",
            r##"{"roster_version":1,"reviewers":[{"provider":"openai","model":"m","roles":["Bad Role"]}]}"##,
            None,
        );
        assert!(
            bad_slug
                .error
                .contains("entry 1: roles: role 'Bad Role' is not a slug"),
            "{}",
            bad_slug.error
        );
    }

    #[test]
    fn require_validation() {
        let unknown_purpose = validate_roster(
            "R.json",
            r##"{"roster_version":1,"reviewers":[{"provider":"openai","model":"m"}],"require":{"nope":["#1"]}}"##,
            None,
        );
        assert!(
            unknown_purpose
                .error
                .contains("require names the purpose 'nope' (known: framing, decision"),
            "{}",
            unknown_purpose.error
        );
        let no_entry = validate_roster(
            "R.json",
            r##"{"roster_version":1,"reviewers":[{"provider":"openai","model":"m"}],"require":{"framing":["#9"]}}"##,
            None,
        );
        assert!(
            no_entry.error.contains(
                "require.framing: '#9' names no roster position (the roster has 1 entries)"
            ),
            "{}",
            no_entry.error
        );
        let empty = validate_roster(
            "R.json",
            r##"{"roster_version":1,"reviewers":[{"provider":"openai","model":"m"}],"require":{"framing":[]}}"##,
            None,
        );
        assert!(
            empty
                .error
                .contains("require.framing must be a non-empty array of reviewers"),
            "{}",
            empty.error
        );
    }

    #[test]
    fn ext_must_be_an_object() {
        let r = validate_roster(
            "R.json",
            r##"{"roster_version":1,"reviewers":[{"provider":"openai","model":"m"}],"ext":[]}"##,
            None,
        );
        assert!(
            r.error.contains(
                "ext must be an object (the extension point of other implementations; got [])"
            ),
            "{}",
            r.error
        );
        let entry = validate_roster(
            "R.json",
            r##"{"roster_version":1,"reviewers":[{"provider":"openai","model":"m","ext":5}]}"##,
            None,
        );
        assert!(entry.error.contains("entry 1: ext must be an object (the extension point of other implementations; got 5)"), "{}", entry.error);
    }

    #[test]
    fn roster_string_problem_finds_each_forbidden_delimiter() {
        assert_eq!(roster_string_problem("a::b"), Some("::"));
        assert_eq!(roster_string_problem("a[b"), Some("["));
        assert_eq!(roster_string_problem("a]b"), Some("]"));
        assert_eq!(roster_string_problem("a|b"), Some("|"));
        assert_eq!(roster_string_problem("a,b"), Some(","));
        assert_eq!(roster_string_problem("a#1"), Some("#"));
        assert_eq!(roster_string_problem("clean-value"), None);
    }

    #[test]
    fn roster_rejects_forbidden_delimiters_in_provider_model_and_engine() {
        let cases = [
            (
                "provider",
                "a::b",
                "roster entry #1: provider must not contain '::'",
            ),
            (
                "provider",
                "a[b",
                "roster entry #1: provider must not contain '['",
            ),
            (
                "provider",
                "a]b",
                "roster entry #1: provider must not contain ']'",
            ),
            (
                "provider",
                "a#1",
                "roster entry #1: provider must not contain '#'",
            ),
            (
                "model",
                "glm|5",
                "roster entry #1: model must not contain '|'",
            ),
            (
                "model",
                "glm,5",
                "roster entry #1: model must not contain ','",
            ),
            (
                "model",
                "glm]5",
                "roster entry #1: model must not contain ']'",
            ),
            (
                "engine",
                "co::dex",
                "roster entry #1: engine must not contain '::'",
            ),
        ];
        for (key, value, expected) in cases {
            let text = format!(
                r#"{{"roster_version":1,"reviewers":[{{"provider":"openai","model":"m","{key}":{value:?}}}]}}"#
            );
            let r = validate_roster("R.json", &text, None);
            assert!(
                r.error.contains(expected),
                "{key}={value}: expected {expected:?} in {}",
                r.error
            );
        }
    }

    #[test]
    fn roster_accepts_clean_provider_model_and_engine_strings() {
        let r = ok(r##"{
            "roster_version": 1,
            "reviewers": [
                {"provider": "openai", "model": "gpt-6-astra"},
                {"provider": "gemini", "engine": "agy", "model": "gemini-3.8-flash-high"}
            ]
        }"##);
        assert_eq!(r.entries[0].provider, "openai");
        assert_eq!(r.entries[1].engine, "agy");
    }

    #[test]
    fn matcher_forms_resolve() {
        let entries = ok(r##"{"roster_version":1,"reviewers":[
            {"provider":"gemini","engine":"agy","model":"gemini-3.8-flash-high"},
            {"provider":"gemini","engine":"agy","model":"gemini-3.1-pro-high"}
        ]}"##)
        .entries;
        assert_eq!(resolve_reviewer_matcher(&entries, "gemini").0, vec![1, 2]);
        assert_eq!(
            resolve_reviewer_matcher(&entries, "gemini :: gemini-3.1-pro-high [agy]").0,
            vec![2]
        );
        assert!(!resolve_reviewer_matcher(&entries, "gemini :: nope")
            .1
            .is_empty());
        assert!(
            resolve_reviewer_matcher(&entries, "gemini :: gemini-3.8-flash-high [muse]")
                .1
                .contains("matches no roster entry")
        );
    }

    #[test]
    fn panel_light_is_the_third_weight() {
        let r = ok(
            r##"{"roster_version":1,"reviewers":[{"provider":"ZAI","model":"glm-5.3","panel":"weighty"},{"provider":"ZAI","model":"glm-5.3-flash","panel":"light"},{"provider":"openai","model":"gpt-5.1"}]}"##,
        );
        let panels: Vec<&str> = r.entries.iter().map(|e| e.panel.as_str()).collect();
        assert_eq!(panels, vec!["weighty", "light", "always"]);
        let bad = validate_roster(
            "R.json",
            r##"{"roster_version":1,"reviewers":[{"provider":"openai","model":"m","panel":"heavy"}]}"##,
            None,
        );
        assert!(
            bad.error
                .contains(r#"entry 1: panel must be "always", "weighty" or "light" (got "heavy")"#),
            "{}",
            bad.error
        );
    }

    fn why(json: &str) -> String {
        let r = validate_roster("R.json", json, None);
        assert!(!r.error.is_empty(), "expected a refusal: {json}");
        let pre = "the reviewer roster 'R.json' is not usable: ";
        let s = r.error.strip_prefix(pre).unwrap_or(&r.error);
        s.split(". Fix it or move it aside")
            .next()
            .unwrap()
            .to_string()
    }

    #[test]
    fn plan_is_a_slug_on_any_engine() {
        // (0.6.0, wave 29b E5) a plan on a codex entry and on an agy/muse entry; omitted = ""
        let r = ok(
            r##"{"roster_version":1,"reviewers":[{"provider":"ZAI","model":"glm-5.3","plan":"zai"},{"provider":"gemini","engine":"agy","model":"gemini-3-pro","plan":"google-ai-pro"},{"provider":"openai","model":"gpt-5.1"},{"provider":"ZAI2","model":"glm-5.3-flash","plan":"zai"}]}"##,
        );
        let plans: Vec<&str> = r.entries.iter().map(|e| e.plan.as_str()).collect();
        assert_eq!(plans, vec!["zai", "google-ai-pro", "", "zai"]);
        // the slug rule (case-sensitive, 2..32, a letter first, letters/digits/-)
        for good in [
            "zai",
            "ab",
            "a1",
            "kimi-code",
            "a234567890123456789012345678901b",
        ] {
            assert!(is_plan_slug(good), "{good}");
        }
        for bad in [
            "",
            "z",
            "Zai",
            "1zai",
            "-zai",
            "zai_x",
            "zai.x",
            "zai x",
            "a2345678901234567890123456789012c",
        ] {
            assert!(!is_plan_slug(bad), "{bad}");
        }
        let text = |v: &str| {
            format!(
                "entry 1: plan must be a slug of 2 to 32 characters - lowercase letters, digits and \"-\", starting with a letter (e.g. \"zai\"; got {v})"
            )
        };
        assert_eq!(
            why(r##"{"roster_version":1,"reviewers":[{"provider":"ZAI","plan":"ZAI"}]}"##),
            text("\"ZAI\"")
        );
        assert_eq!(
            why(r##"{"roster_version":1,"reviewers":[{"provider":"ZAI","plan":"z"}]}"##),
            text("\"z\"")
        );
        assert_eq!(
            why(r##"{"roster_version":1,"reviewers":[{"provider":"ZAI","plan":5}]}"##),
            text("5")
        );
        assert_eq!(
            why(r##"{"roster_version":1,"reviewers":[{"provider":"ZAI","plan":["zai"]}]}"##),
            text("[\"zai\"]")
        );
        assert_eq!(
            why(r##"{"roster_version":1,"reviewers":[{"provider":"ZAI","plan":null}]}"##),
            text("null")
        );
        // the plugin's per-entry order: timeout/context/stall, ext, plan, then lab ...
        assert_eq!(
            why(r##"{"roster_version":1,"reviewers":[{"provider":"ZAI","plan":"Z","lab":""}]}"##),
            text("\"Z\"")
        );
        assert!(why(
            r##"{"roster_version":1,"reviewers":[{"provider":"ZAI","plan":"Z","timeout_sec":5}]}"##
        )
        .starts_with("entry 1: timeout_sec must be"));
        assert!(why(
            r##"{"roster_version":1,"reviewers":[{"provider":"ZAI","plan":"Z","ext":1}]}"##
        )
        .starts_with("entry 1: ext must be an object"));
    }

    #[test]
    fn the_unknown_key_list_and_the_endpoint_key_are_the_plugins() {
        assert_eq!(
            why(r##"{"roster_version":1,"reviewers":[{"provider":"ZAI","modle":"glm-5.3"}]}"##),
            "entry 1 has an unknown key 'modle' (allowed: provider, model, codex_config, auth, endpoint, plan, panel, engine, lab, roles, timeout_sec, stall_sec, context_tokens, ext)"
        );
        // endpoint is the claude engine's (auth "endpoint", wave 4): refused on every engine C3 runs
        assert_eq!(
            why(r##"{"roster_version":1,"reviewers":[{"provider":"ZAI","endpoint":{"base_url":"https://api.z.ai/api/anthropic","env_key":"K"},"plan":"zai"}]}"##),
            "entry 1: endpoint applies only to engine claude with auth \"endpoint\" (this entry: engine codex)"
        );
        assert_eq!(
            why(r##"{"roster_version":1,"reviewers":[{"provider":"gemini","engine":"agy","model":"gemini-3-pro","endpoint":{}}]}"##),
            "entry 1: endpoint applies only to engine claude with auth \"endpoint\" (this entry: engine agy)"
        );
    }

    #[test]
    fn parallel_may_name_a_plan() {
        // (0.6.0, wave 29b E7) a parallel key names a provider label or a plan
        let r = ok(
            r##"{"roster_version":1,"reviewers":[{"provider":"ZAI","model":"glm-5.3","plan":"zai"},{"provider":"openai","model":"gpt-5.1"}],"parallel":{"zai":2,"openai":1}}"##,
        );
        assert_eq!(
            r.parallel,
            vec![("zai".to_string(), 2), ("openai".to_string(), 1)]
        );
        assert_eq!(
            why(r##"{"roster_version":1,"reviewers":[{"provider":"ZAI","model":"glm-5.3"}],"parallel":{"zai":2}}"##),
            "parallel names the provider label 'zai', which no entry of the roster uses (as its provider label or its plan)"
        );
        assert_eq!(
            why(
                r##"{"roster_version":1,"reviewers":[{"provider":"ZAI","model":"glm-5.3","plan":"zai"}],"parallel":{"zai":0}}"##
            ),
            "parallel.zai must be an integer >= 1 (got 0)"
        );
    }

    #[test]
    fn the_claude_engine_keys() {
        // (0.6.0, wave 29 / 29b) the claude entry: the model table ([1m] allowed), auth
        // subscription (default) | api-key | endpoint, the endpoint block and its refusals
        let r = ok(
            r##"{"roster_version":1,"reviewers":[{"provider":"anthropic","engine":"claude","model":"claude-opus-5-5[1m]","auth":"subscription","panel":"weighty"},{"provider":"anthropic","engine":"claude","model":"sonnet","auth":"api-key"},{"provider":"anthropic","engine":"claude","model":"haiku"},{"provider":"ZAI-claude","engine":"claude","model":"glm-5.3","auth":"endpoint","endpoint":{"base_url":"https://api.z.ai/api/anthropic","env_key":"ZAI_KEY"},"plan":"zai"}],"parallel":{"anthropic":2,"zai":2}}"##,
        );
        assert_eq!(r.entries[0].model, "claude-opus-5-5[1m]");
        assert_eq!(r.entries[0].auth, "subscription");
        assert_eq!(r.entries[1].auth, "api-key");
        assert_eq!(r.entries[2].auth, "subscription");
        let ep = r.entries[3].endpoint.as_ref().unwrap();
        assert_eq!(ep.env_key, "ZAI_KEY");
        assert_eq!(ep.timeout_ms, 3_000_000);
        assert_eq!(ep.plan, "zai");
        assert!(r.entries[2].endpoint.is_none());
        let cases = [
            (r##"{"roster_version":1,"reviewers":[{"provider":"anthropic","engine":"claude","model":"opus","auth":"key"}]}"##, "entry 1: auth of engine claude must be \"subscription\" (the claude.ai login, the default) or \"api-key\" (ANTHROPIC_API_KEY) or \"endpoint\" (a third-party Anthropic-compatible endpoint named by the entry's \"endpoint\") (got \"key\")"),
            (r##"{"roster_version":1,"reviewers":[{"provider":"anthropic","engine":"claude","model":"claude-opus-9"}]}"##, "entry 1: the claude model 'claude-opus-9' is not in the claude engine's model table (opus, sonnet, haiku, fable, claude-fable-5-1, claude-fable-5, claude-opus-5-5, claude-opus-5, claude-opus-4-8, claude-opus-4-7, claude-opus-4-6, claude-sonnet-5-5, claude-sonnet-5, claude-sonnet-4-6, claude-haiku-5-5, claude-haiku-4-5; each may end with [1m])"),
            (r##"{"roster_version":1,"reviewers":[{"provider":"anthropic","engine":"claude"}]}"##, "entry 1: engine claude needs a model (the full model id, e.g. claude-sonnet-5-5)"),
            (r##"{"roster_version":1,"reviewers":[{"provider":"anthropic","engine":"claude","model":"opus","codex_config":["a=b"]}]}"##, "entry 1: codex_config does not apply to engine claude (it configures codex exec)"),
            (r##"{"roster_version":1,"reviewers":[{"provider":"g","engine":"agy","model":"gemini-3.8-flash-high","auth":"api-key"}]}"##, "entry 1: auth may only be \"none\" (an endpoint that needs no credential; omit it otherwise)"),
            (r##"{"roster_version":1,"reviewers":[{"provider":"openai","model":"gpt-5.1[1m]"}]}"##, "roster entry #1: model must not contain '['"),
            (r##"{"roster_version":1,"reviewers":[{"provider":"x","engine":"claude","model":"glm-5.3","auth":"endpoint"}]}"##, "entry 1: auth \"endpoint\" needs an \"endpoint\" object {\"base_url\": \"https://...\", \"env_key\": \"<VARIABLE NAME>\"} - the Anthropic-compatible endpoint and the variable that holds its token"),
            (r##"{"roster_version":1,"reviewers":[{"provider":"x","engine":"claude","model":"sonnet","auth":"api-key","endpoint":{"base_url":"https://a.example/v","env_key":"ABC"}}]}"##, "entry 1: endpoint applies only to engine claude with auth \"endpoint\" (this entry: engine claude, auth api-key)"),
            (r##"{"roster_version":1,"reviewers":[{"provider":"x","engine":"claude","model":"FABLE","auth":"endpoint","endpoint":{"base_url":"https://a.example/v","env_key":"ABC"}}]}"##, "entry 1: the claude model 'FABLE' is an Anthropic model id, which the endpoint route cannot carry"),
            (r##"{"roster_version":1,"reviewers":[{"provider":"x","engine":"claude","model":"glm-5.3","auth":"endpoint","endpoint":{"base_url":"https://user:SECRETPW@api.z.ai/x","env_key":"ABC"}}]}"##, "entry 1: endpoint.base_url must be an absolute https URL without credentials, query or fragment"),
            (r##"{"roster_version":1,"reviewers":[{"provider":"ZAI","model":"glm-5.3","plan":"zai"},{"provider":"ZAI","engine":"claude","model":"glm-5.3","auth":"endpoint","endpoint":{"base_url":"https://api.z.ai/api/anthropic","env_key":"ABC"},"plan":"zai"}]}"##, "entries 1 and 2 use the provider label 'ZAI' with two engines (codex, claude); a label names one engine"),
        ];
        for (json, want) in cases {
            let w = why(json);
            assert!(w.contains(want), "{json}\n{w}");
            assert!(!w.contains("SECRETPW"));
        }
    }
}
