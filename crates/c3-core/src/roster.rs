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

/// One validated roster entry.
#[derive(Debug, Clone, Default)]
pub struct RosterEntry {
    pub position: usize,
    pub provider: String,
    pub model: String,
    pub codex_config: Vec<String>,
    pub auth: String,
    pub panel: String,
    pub engine: String,
    pub engine_declared: bool,
    /// (wave 26, D1) the lab behind the model, canonical lowercase; `""` when the entry names
    /// none (the panel then derives it from the model id's prefix).
    pub lab: String,
    /// (wave 26, D8) the role slugs the entry is willing to take under a panel's `-Roles`.
    pub roles: Vec<String>,
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
    let t = matcher.trim();
    let fail = |why: String| (Vec::new(), why);
    if t.is_empty() {
        return fail("an empty reviewer matcher".to_string());
    }
    let pos_re = regex::Regex::new(r"^#(\d+)$").unwrap();
    if let Some(c) = pos_re.captures(t) {
        let pos: i64 = c[1].parse().unwrap_or(0);
        let hit: Vec<i64> = entries
            .iter()
            .filter(|e| e.position as i64 == pos)
            .map(|e| e.position as i64)
            .collect();
        if hit.is_empty() {
            return fail(format!(
                "'{t}' names no roster position (the roster has {} entries)",
                entries.len()
            ));
        }
        return (hit, String::new());
    }
    let mut rest = t.to_string();
    let mut engine = String::new();
    let eng_re = regex::Regex::new(r"^(.*\S)\s+\[([A-Za-z0-9_-]+)\]$").unwrap();
    if let Some(c) = eng_re.captures(t) {
        rest = c[1].to_string();
        engine = c[2].to_lowercase();
        if !ENGINE_NAMES.contains(&engine.as_str()) {
            return fail(format!(
                "'{matcher}' names the engine '{engine}' (known: {})",
                ENGINE_NAMES.join(", ")
            ));
        }
    }
    let mut provider = rest.clone();
    let mut model: Option<String> = None;
    if let Some(sep) = rest.find(" :: ") {
        provider = rest[..sep].trim().to_string();
        let m = rest[sep + 4..].trim().to_string();
        if provider.is_empty() || m.is_empty() {
            return fail(format!("'{matcher}' is not '<provider> :: <model>'"));
        }
        model = Some(m);
    }
    let hits: Vec<i64> = entries
        .iter()
        .filter(|e| {
            e.provider == provider
                && model.as_ref().is_none_or(|m| &e.model == m)
                && (engine.is_empty() || e.engine == engine)
        })
        .map(|e| e.position as i64)
        .collect();
    if hits.is_empty() {
        return fail(format!("'{matcher}' matches no roster entry"));
    }
    (hits, String::new())
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
                    "panel",
                    "engine",
                    "lab",
                    "roles",
                    "ext",
                    // Accepted by the plugin at HEAD (post wave 26) with runtime semantics not yet
                    // documented: per-entry timeout, stall detection and context budget. C3 accepts
                    // them as opaque so a roster the plugin accepts is never refused here; the
                    // semantics are ported when the plugin documents them.
                    "timeout_sec",
                    "stall_sec",
                    "context_tokens",
                ]
                .contains(&key.as_str())
                {
                    why = format!(
                        "{at} has an unknown key '{key}' (allowed: provider, model, codex_config, auth, panel, engine, lab, roles, ext, timeout_sec, stall_sec, context_tokens)"
                    );
                    break;
                }
            }
            if !why.is_empty() {
                break;
            }
            // (wave 26b, D3 / F22-2, F22-4) the matcher's and the seed's delimiters never
            // inside a provider label, a model or an engine: '::', '[', ']', '|', ',', '#'.
            let mut d3_bad = false;
            for sk in ["provider", "model", "engine"] {
                if let Some(sv) = iobj.get(sk).and_then(|v| v.as_str()) {
                    if let Some(bad) = roster_string_problem(sv) {
                        why = format!("roster entry #{pos}: {sk} must not contain '{bad}'");
                        d3_bad = true;
                        break;
                    }
                }
            }
            if d3_bad {
                break;
            }
            // ext (D12): validated as an object, otherwise ignored (checked first, matching the
            // plugin's per-entry order).
            if let Some(xv) = iobj.get("ext") {
                if !xv.is_object() {
                    why = format!("{at}: ext must be an object (the extension point of other implementations; got {})", compact(xv));
                    break;
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
                match mv.as_str() {
                    Some(s) if !s.trim().is_empty() && s == s.trim() => model = s.to_string(),
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
            // auth
            let mut auth = String::new();
            if let Some(av) = iobj.get("auth") {
                if av.as_str() != Some("none") {
                    why = format!("{at}: auth may only be \"none\" (an endpoint that needs no credential; omit it otherwise)");
                    break;
                }
                auth = "none".into();
            }
            // panel
            let mut panel = "always".to_string();
            if let Some(pv) = iobj.get("panel") {
                match pv.as_str() {
                    Some(s) if s == "always" || s == "weighty" => panel = s.to_string(),
                    _ => {
                        why = format!(
                            "{at}: panel must be \"always\" or \"weighty\" (got {})",
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
                if iobj.contains_key("auth") {
                    why = format!("{at}: auth does not apply to engine {engine} (the {engine} CLI keeps its own sign-in)");
                    break;
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
                        if !entries.iter().any(|e| &e.provider == name) {
                            why = format!("parallel names the provider label '{name}', which no entry of the roster uses");
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
}
