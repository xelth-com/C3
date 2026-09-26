//! The reviewer roster format (`Read-ReviewerRoster`): fail-closed validation of the
//! roster JSON, plus the `codex_config` / `-CodexConfig` item rules
//! (`ConvertFrom-CodexConfigItems`).
//!
//! Divergence from the PowerShell bridge (companions decision D12, 2026-09-26): an
//! optional `ext` object is accepted at the top level and per entry, validated only
//! as an object and otherwise ignored (never read, never written). The current
//! bridge still rejects `ext` as an unknown key; C3 is the implementation D12 was
//! written for. `roster_version` stays 1.

use serde_json::Value;

use crate::lineage::{engine_spec, format_lineage, ENGINE_NAMES};

/// One validated roster entry.
#[derive(Debug, Clone)]
pub struct RosterEntry {
    pub position: usize,
    pub provider: String,
    pub model: String,
    pub codex_config: Vec<String>,
    pub auth: String,
    pub panel: String,
    pub engine: String,
    pub engine_declared: bool,
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
    pub error: String,
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
        if !["roster_version", "reviewers", "parallel", "ext"].contains(&key.as_str()) {
            why = format!("unknown key '{key}' at the top level (allowed: roster_version, reviewers, parallel, ext)");
            break;
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
                    "ext",
                ]
                .contains(&key.as_str())
                {
                    why = format!(
                        "{at} has an unknown key '{key}' (allowed: provider, model, codex_config, auth, panel, engine, ext)"
                    );
                    break;
                }
            }
            if !why.is_empty() {
                break;
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
            // ext (D12): validated as an object, otherwise ignored
            if let Some(xv) = iobj.get("ext") {
                if !xv.is_object() {
                    why = format!("{at}: ext must be a JSON object (reserved for other implementations; the bridge ignores it)");
                    break;
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
    // top-level ext (D12)
    if why.is_empty() {
        if let Some(xv) = obj.get("ext") {
            if !xv.is_object() {
                why = "ext must be a JSON object (reserved for other implementations; the bridge ignores it)".into();
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
