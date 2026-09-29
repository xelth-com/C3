//! C3's `ext.c3.reviewers` roster extension (M7b-b, DESIGN §4 API path).
//!
//! The reviewer roster is shared with the PowerShell plugin, whose validator knows only the
//! engines `codex`, `agy` and `muse` and refuses the whole file on any unknown engine — so an
//! `http` reviewer can never live among the plugin-visible `reviewers[]`. Instead it lives under
//! the top-level extension object the plugin validates as "an object" and otherwise ignores
//! (`ext.c3.reviewers`, decision D12 / M7b-b decision 1):
//!
//! ```json
//! { "ext": { "c3": { "reviewers": [
//!   { "provider": "openrouter", "model": "openai/gpt-5", "engine": "http", "lab": "openai",
//!     "weight": 1, "base_url": "https://openrouter.ai/api/v1", "key_env": "OPENROUTER_API_KEY",
//!     "json_object": true, "headers": { "HTTP-Referer": "https://xelth.com", "X-Title": "c3" },
//!     "purposes": ["diff-review"], "roles": ["security"] } ] } } }
//! ```
//!
//! C3 validates these with the same rules and wording style as an ordinary `reviewers[]` entry
//! (a non-empty provider/model without surrounding blanks or the reserved delimiters, a clean
//! `lab`, slug `roles`), plus the API-path additions: `base_url` must be `https://`, `key_env`
//! must be a valid environment-variable NAME, header names/values carry no CR or LF and none is
//! named `Authorization` (the engine sets that itself from the environment). The reviewers are
//! then appended AFTER the plugin's entries, their positions continuing the numbering, so the
//! panel and `c3 providers` see them exactly as they see the plugin's own entries.

use serde_json::Value;

use crate::roster::{convert_to_slug_list, RosterEntry, CONSULT_PURPOSES};

/// One validated `ext.c3.reviewers` entry: everything the `http` engine needs except the key,
/// which is read from the environment at run time and never stored (DESIGN §3 invariant 4).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HttpReviewer {
    /// The roster position, continuing the numbering after the plugin's entries.
    pub position: usize,
    pub provider: String,
    pub model: String,
    /// The lab behind the model, canonical lowercase; `""` when the entry names none.
    pub lab: String,
    /// The panel routing weight (`>= 1`); `1` when the entry names none.
    pub weight: i64,
    /// The API base (no trailing `/chat/completions`); `https://` only.
    pub base_url: String,
    /// The environment variable the key is read from.
    pub key_env: String,
    /// Send `response_format: {"type":"json_object"}`.
    pub json_object: bool,
    /// Extra request headers, in file order (never `Authorization`; no CR/LF).
    pub headers: Vec<(String, String)>,
    /// The consult purposes this reviewer serves (empty = any).
    pub purposes: Vec<String>,
    /// The role slugs this reviewer is willing to take under a panel's `-Roles`.
    pub roles: Vec<String>,
    /// Whether the roster explicitly accepted per-token billing at a lab that also sells a
    /// subscription (`"api_billing": "accepted"`); relaxes the lab-label billing guard.
    pub api_billing_accepted: bool,
    /// (S6) The reviewer pack's periphery budget in tokens (`0..=200000`); `-1` when the entry
    /// names none (the run then takes `--pack-budget`, else [`DEFAULT_PACK_TOKENS`]).
    pub pack_tokens: i64,
}

/// The default OpenRouter API base (mirrors `crate`-side `http_engine::DEFAULT_BASE_URL`).
pub const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";
/// The default key environment variable (OpenRouter's own).
pub const DEFAULT_KEY_ENV: &str = "OPENROUTER_API_KEY";

/// The keys an `ext.c3.reviewers` entry may carry.
const ALLOWED_KEYS: &[&str] = &[
    "provider",
    "model",
    "engine",
    "lab",
    "weight",
    "base_url",
    "key_env",
    "json_object",
    "headers",
    "purposes",
    "roles",
    "api_billing",
    "pack_tokens",
];

/// (S6) The default periphery-token budget for an http reviewer pack when neither the roster nor
/// `--pack-budget` names one. An http reviewer sees only the pack, so a budget of 0 starves it;
/// but every token is billed, so the default is modest.
pub const DEFAULT_PACK_TOKENS: i64 = 12000;
/// (S6) The maximum periphery-token budget an entry / flag may name.
pub const MAX_PACK_TOKENS: i64 = 200000;

/// (S1) Known API-key environment variables, each bound to the host it belongs to (an exact
/// host or a subdomain of it). c3 sends a variable only to its provider; any other endpoint
/// needs a variable the user created for c3 (name starting `C3_KEY_`).
const KNOWN_KEYS: &[(&str, &str)] = &[
    ("OPENROUTER_API_KEY", "openrouter.ai"),
    ("OPENAI_API_KEY", "api.openai.com"),
    ("ANTHROPIC_API_KEY", "api.anthropic.com"),
    ("GEMINI_API_KEY", "generativelanguage.googleapis.com"),
    ("GOOGLE_API_KEY", "generativelanguage.googleapis.com"),
    ("MISTRAL_API_KEY", "api.mistral.ai"),
    ("DEEPSEEK_API_KEY", "api.deepseek.com"),
    ("GROQ_API_KEY", "api.groq.com"),
    ("TOGETHER_API_KEY", "api.together.xyz"),
    ("XAI_API_KEY", "api.x.ai"),
];

/// (S5) Header names c3 controls itself: a roster entry may not set them, case-insensitively.
const RESERVED_HEADERS: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "cookie",
    "host",
    "content-length",
    "content-type",
    "transfer-encoding",
];

/// (S2) Parse and validate an http reviewer's base URL, returning its host lowercased (for the
/// S1 key-to-host binding). The scheme must be exactly `https`, the host non-empty, with no
/// userinfo, query or fragment; a port and a path are allowed; no control characters; at most
/// 2048 bytes. The host is the URL parser's host, never a substring of the raw string.
pub fn parse_base_url(raw: &str) -> Result<String, String> {
    if raw.len() > 2048 {
        return Err(format!(
            "base_url is too long ({} bytes; at most 2048)",
            raw.len()
        ));
    }
    if raw.trim() != raw {
        return Err("base_url must not have surrounding blanks".to_string());
    }
    if raw.chars().any(|c| c.is_control()) {
        return Err("base_url must not contain control characters".to_string());
    }
    // (S6) The url crate's parse error can echo the input, so it is dropped: the value may be a
    // pasted secret.
    let u = url::Url::parse(raw).map_err(|_| "base_url is not a valid URL".to_string())?;
    if u.scheme() != "https" {
        return Err("base_url must be https://".to_string());
    }
    if !u.username().is_empty() || u.password().is_some() {
        return Err("base_url must not contain a username or password".to_string());
    }
    if u.query().is_some() {
        return Err("base_url must not contain a query string".to_string());
    }
    if u.fragment().is_some() {
        return Err("base_url must not contain a fragment".to_string());
    }
    match u.host_str() {
        Some(h) if !h.is_empty() => Ok(h.to_ascii_lowercase()),
        _ => Err("base_url must have a host".to_string()),
    }
}

/// (S1) Refuse sending an environment variable to a host it is not bound to. A known key must go
/// to its provider's host (exact or a subdomain). A custom key is bound by its NAME to exactly one
/// host: `C3_KEY_` + the host upper-cased, `.` written `_` and `-` written `__` (no subdomain
/// rule). The variable NAME is what the user created by hand; the roster and the flags are
/// agent-writable, so the name — not the roster — carries the destination host. Names the variable
/// and the rule, never a value.
pub fn check_key_host(key_env: &str, host: &str) -> Result<(), String> {
    let host = host.to_ascii_lowercase();
    // A compile-time known key is bound to its provider's host (exact or a subdomain).
    if let Some((_, bound)) = KNOWN_KEYS.iter().find(|(k, _)| *k == key_env) {
        if host == *bound || host.ends_with(&format!(".{bound}")) {
            return Ok(());
        }
        return Err(format!(
            "{key_env} is bound to {bound}; got host {}",
            cap64(&host)
        ));
    }
    // A custom key: its name must encode this exact host.
    if is_wellformed_c3_key(key_env) {
        return match c3_key_name_for_host(&host) {
            Some(expected) if expected == key_env => Ok(()),
            Some(expected) => Err(format!(
                "{key_env} is bound by its name to a different host; for {} create a variable named {expected}",
                cap64(&host)
            )),
            None => Err(format!(
                "a custom C3_KEY_ variable is bound to a DNS host, but {} is not one (an IP literal, or a host with any other character, cannot use a custom key)",
                cap64(&host)
            )),
        };
    }
    // Neither a known key nor a well-formed custom key name: never echo the name.
    Err(key_env_not_known_message(key_env))
}

/// (S1/S6) Whether `key_env` is a NAME c3 will name in a message and use as a key variable: one of
/// the compile-time known keys, or a well-formed `C3_KEY_` name.
pub fn check_key_env_name(key_env: &str) -> Result<(), String> {
    if KNOWN_KEYS.iter().any(|(k, _)| *k == key_env) || is_wellformed_c3_key(key_env) {
        Ok(())
    } else {
        Err(key_env_not_known_message(key_env))
    }
}

/// A well-formed custom key NAME: `C3_KEY_` + a non-empty suffix, a valid environment-variable
/// name, at most 64 characters (bounds what a refusal may echo).
fn is_wellformed_c3_key(name: &str) -> bool {
    name.starts_with("C3_KEY_")
        && name.len() > "C3_KEY_".len()
        && name.len() <= 64
        && is_env_name(name)
}

/// (S1) The `C3_KEY_` variable name a host is bound to, or `None` when the host is not a DNS name
/// (an IP literal or a host with any character outside a DNS label cannot use a custom key). The
/// encoding is injective over DNS hosts: a `.` becomes one `_`, a `-` becomes `__`, so a maximal
/// run of underscores is either a single dot (odd length 1) or whole hyphens (an even length).
fn c3_key_name_for_host(host: &str) -> Option<String> {
    if !is_dns_host(host) {
        return None;
    }
    let mut s = String::from("C3_KEY_");
    for ch in host.chars() {
        match ch {
            '.' => s.push('_'),
            '-' => s.push_str("__"),
            c if c.is_ascii_alphanumeric() => s.push(c.to_ascii_uppercase()),
            _ => return None,
        }
    }
    Some(s)
}

/// A DNS name: dot-separated labels of `[a-z0-9-]`, none empty, none starting or ending with `-`,
/// and not an IP literal (an IPv4 dotted-decimal or a bracketed/colon IPv6 address is refused: an
/// IP literal cannot use a custom key at all).
fn is_dns_host(host: &str) -> bool {
    if host.is_empty() || host.parse::<std::net::Ipv4Addr>().is_ok() {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    })
}

/// (S6) The refusal for a `key_env` that is neither a known key nor a well-formed `C3_KEY_` name.
/// Never echoes the name (it may itself be a pasted credential); adds a hint when the string looks
/// like a key VALUE rather than a variable name.
fn key_env_not_known_message(key_env: &str) -> String {
    let known = KNOWN_KEYS
        .iter()
        .map(|(k, _)| *k)
        .collect::<Vec<_>>()
        .join(", ");
    let mut m = format!(
        "key_env is not the name of a variable c3 knows (known keys: {known}; for another endpoint create a variable named C3_KEY_<HOST>, e.g. C3_KEY_API_EXAMPLE_COM for api.example.com)"
    );
    if looks_like_credential(key_env) {
        m.push_str(
            ". it looks like a key value: put the key into an environment variable and name that variable here",
        );
    }
    m
}

/// (S6) Whether a string given as a variable name looks instead like a key VALUE: it contains a
/// `-` (no environment-variable name does), is longer than 64 characters, or carries a known key
/// prefix such as `sk-`.
fn looks_like_credential(s: &str) -> bool {
    s.contains('-') || s.len() > 64 || s.starts_with("sk-") || s.starts_with("sk_")
}

/// (S5) Why a header name is refused: a reserved name c3 controls, or a name that is not an
/// RFC 7230 token. `None` when the name is acceptable.
pub fn header_name_problem(name: &str) -> Option<String> {
    if RESERVED_HEADERS.contains(&name.to_ascii_lowercase().as_str()) {
        return Some(format!(
            "a header named '{}' is refused; the http engine controls it (it sets Authorization from key_env, and c3 sets content-type/host/length itself)",
            cap64(name)
        ));
    }
    if !is_http_token(name) {
        return Some(format!(
            "header name '{}' is not a valid HTTP token (RFC 7230: letters, digits and !#$%&'*+-.^_`|~)",
            cap64(name)
        ));
    }
    // (S6) A header name that suggests a credential is refused: the roster must not hold secrets.
    if header_name_suggests_credential(name) {
        return Some("a header that carries a credential cannot live in the roster".to_string());
    }
    None
}

/// (S6) Whether a header NAME suggests it carries a credential (case-insensitive, anywhere in the
/// name). `X-Title` and `HTTP-Referer` match none of these and stay allowed.
fn header_name_suggests_credential(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    [
        "key",
        "token",
        "secret",
        "auth",
        "password",
        "credential",
        "session",
    ]
    .iter()
    .any(|w| n.contains(w))
}

/// An RFC 7230 header-name token: `1*tchar`.
fn is_http_token(s: &str) -> bool {
    !s.is_empty()
        && s.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

impl HttpReviewer {
    /// A synthesized [`RosterEntry`] so the panel and `c3 providers` (which iterate
    /// `roster.entries`) see this reviewer exactly as a plugin entry. The full request config
    /// (base_url/key_env/headers/json_object) is kept on the [`HttpReviewer`] and looked up by
    /// position; the entry carries only what seat selection and the lineage need.
    pub fn to_entry(&self) -> RosterEntry {
        RosterEntry {
            position: self.position,
            provider: self.provider.clone(),
            model: self.model.clone(),
            codex_config: Vec::new(),
            auth: String::new(),
            panel: "always".to_string(),
            engine: "http".to_string(),
            engine_declared: true,
            lab: self.lab.clone(),
            roles: self.roles.clone(),
            timeout_sec: 0,
            stall_sec: -1,
            context_tokens: 0,
        }
    }
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

/// (S6) The JSON *type* of a value, for a refusal that must never echo the value's content
/// (it may be a pasted credential). A string's length is safe to report (`a string of N
/// characters`), never its bytes.
fn json_type(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(_) => "a boolean".to_string(),
        Value::Number(_) => "a number".to_string(),
        Value::String(s) => format!("a string of {} characters", s.chars().count()),
        Value::Array(_) => "an array".to_string(),
        Value::Object(_) => "an object".to_string(),
    }
}

/// (S6) Cap an echoed label (a provider/model/host name) at 64 characters so a refusal never
/// carries an unbounded string, even for values the roster string rules already restrict.
fn cap64(s: &str) -> String {
    if s.chars().count() > 64 {
        s.chars().take(64).collect::<String>() + "..."
    } else {
        s.to_string()
    }
}

/// The first reserved roster delimiter found in a string (`::`, `[`, `]`, `|`, `,`, `#`), else
/// `None` (mirrors `roster::roster_string_problem`).
fn string_problem(value: &str) -> Option<&'static str> {
    ["::", "[", "]", "|", ",", "#"]
        .into_iter()
        .find(|d| value.contains(d))
}

/// A valid POSIX-ish environment-variable NAME (`^[A-Za-z_][A-Za-z0-9_]*$`).
pub fn is_env_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Parse and validate `ext.c3.reviewers` from the whole roster JSON `data`. `plugin_count` is the
/// number of already-validated plugin entries (the positions continue from there). Returns the
/// validated reviewers (empty when the extension is absent), or the refusal `why` (unwrapped;
/// the caller wraps it with `roster::roster_refusal`, exactly as for a plugin entry).
pub fn parse_ext_reviewers(data: &Value, plugin_count: usize) -> Result<Vec<HttpReviewer>, String> {
    let Some(reviewers) = data
        .get("ext")
        .and_then(|e| e.get("c3"))
        .and_then(|c| c.get("reviewers"))
    else {
        return Ok(Vec::new());
    };
    let arr = match reviewers.as_array() {
        Some(a) => a,
        None => {
            return Err(format!(
                "ext.c3.reviewers must be an array (got {})",
                json_type(reviewers)
            ))
        }
    };
    let mut out: Vec<HttpReviewer> = Vec::new();
    for (idx, item) in arr.iter().enumerate() {
        let i = idx + 1;
        let at = format!("ext.c3.reviewers entry {i}");
        let iobj = match item.as_object() {
            Some(o) => o,
            None => return Err(format!("{at} is not an object")),
        };
        for key in iobj.keys() {
            if !ALLOWED_KEYS.contains(&key.as_str()) {
                return Err(format!(
                    "{at} has an unknown key '{key}' (allowed: {})",
                    ALLOWED_KEYS.join(", ")
                ));
            }
        }
        // engine: present it must be "http" (the extension is for the http engine only).
        if let Some(ev) = iobj.get("engine") {
            if ev.as_str() != Some("http") {
                return Err(format!(
                    "{at}: engine must be \"http\" (ext.c3.reviewers is the http engine's extension; got {})",
                    json_type(ev)
                ));
            }
        }
        // provider (same rule as an ordinary entry).
        let provider = match iobj.get("provider").and_then(|v| v.as_str()) {
            Some(s) if !s.trim().is_empty() && s == s.trim() => s.to_string(),
            _ => {
                return Err(format!(
                    "{at} needs a provider: a non-empty string without surrounding blanks"
                ))
            }
        };
        if let Some(bad) = string_problem(&provider) {
            return Err(format!("{at}: provider must not contain '{bad}'"));
        }
        // model (required for the http engine, like the plugin's non-codex entries).
        let model = match iobj.get("model").and_then(|v| v.as_str()) {
            Some(s) if !s.trim().is_empty() && s == s.trim() => s.to_string(),
            _ => {
                return Err(format!(
                    "{at}: engine http needs a model: a non-empty string without surrounding blanks (the full model id, e.g. openai/gpt-5)"
                ))
            }
        };
        if let Some(bad) = string_problem(&model) {
            return Err(format!("{at}: model must not contain '{bad}'"));
        }
        // lab (same rule as an ordinary entry).
        let mut lab = String::new();
        if let Some(lv) = iobj.get("lab") {
            match lv.as_str() {
                Some(s) if !s.trim().is_empty() && s == s.trim() => lab = s.to_lowercase(),
                _ => {
                    return Err(format!(
                        "{at}: lab must be a non-empty string without surrounding blanks (e.g. \"openai\"; omit it to take the lab from the model id)"
                    ))
                }
            }
        }
        // weight (>= 1; 1 when omitted).
        let mut weight = 1i64;
        if let Some(wv) = iobj.get("weight") {
            if !is_json_integer(wv) || wv.as_f64().unwrap_or(0.0) < 1.0 {
                return Err(format!(
                    "{at}: weight must be an integer >= 1 (the panel routing weight; got {})",
                    json_type(wv)
                ));
            }
            weight = wv.as_i64().unwrap_or(1);
        }
        // base_url (S2: strict parse; default OpenRouter's own). `host` (the parsed host,
        // lowercased) drives the S1 key-to-host binding below.
        let mut base_url = DEFAULT_BASE_URL.to_string();
        if let Some(bv) = iobj.get("base_url") {
            match bv.as_str() {
                Some(s) => match parse_base_url(s) {
                    Ok(_) => base_url = s.to_string(),
                    Err(why) => return Err(format!("{at}: {why}")),
                },
                None => {
                    return Err(format!(
                        "{at}: base_url must be a string (got {})",
                        json_type(bv)
                    ))
                }
            }
        }
        // The host cannot fail to parse here (a default is a valid https URL, and any value was
        // just validated), so an Err is an internal invariant, surfaced rather than silently
        // allowing the S1 check to be skipped.
        let host = parse_base_url(&base_url).map_err(|why| format!("{at}: {why}"))?;
        // key_env (a valid environment-variable NAME; default OPENROUTER_API_KEY). The key value
        // itself is NEVER a roster field — only the name of the variable it is read from.
        let mut key_env = DEFAULT_KEY_ENV.to_string();
        if let Some(kv) = iobj.get("key_env") {
            match kv.as_str() {
                Some(s) => {
                    // (S6) The value is a string: it must be the NAME of a variable c3 knows. A
                    // pasted key value is refused here and never echoed.
                    check_key_env_name(s).map_err(|why| format!("{at}: {why}"))?;
                    key_env = s.to_string();
                }
                None => {
                    return Err(format!(
                        "{at}: key_env must be a string (got {}). The key value is never stored in the roster.",
                        json_type(kv)
                    ))
                }
            }
        }
        // (S1) The key may go only to the host it belongs to.
        if let Err(why) = check_key_host(&key_env, &host) {
            return Err(format!("{at}: {why}"));
        }
        // json_object (default true).
        let mut json_object = true;
        if let Some(jv) = iobj.get("json_object") {
            match jv.as_bool() {
                Some(b) => json_object = b,
                None => {
                    return Err(format!(
                        "{at}: json_object must be true or false (got {})",
                        json_type(jv)
                    ))
                }
            }
        }
        // headers (object of string->string; no CR/LF; never named Authorization).
        let mut headers: Vec<(String, String)> = Vec::new();
        if let Some(hv) = iobj.get("headers") {
            let hobj = match hv.as_object() {
                Some(o) => o,
                None => {
                    return Err(format!(
                        "{at}: headers must be an object of string values (e.g. {{\"X-Title\": \"c3\"}}; got {})",
                        json_type(hv)
                    ))
                }
            };
            for (name, val) in hobj {
                // (S6) Vet the name first (a reserved / malformed / credential-suggesting name is
                // refused regardless of its value), then the value's type — the value-type refusal
                // never echoes the header name, which may itself be an attacker-chosen string.
                if let Some(why) = header_name_problem(name) {
                    return Err(format!("{at}: {why}"));
                }
                let value = match val.as_str() {
                    Some(s) => s,
                    None => {
                        return Err(format!(
                            "{at}: a header value must be a string (got {})",
                            json_type(val)
                        ))
                    }
                };
                if value.contains(['\r', '\n']) {
                    return Err(format!(
                        "{at}: header '{}' value must not contain a carriage return or line feed",
                        cap64(name)
                    ));
                }
                headers.push((name.clone(), value.to_string()));
            }
        }
        // purposes (consult purposes, validated as slugs; each must be a known purpose).
        let mut purposes: Vec<String> = Vec::new();
        if let Some(pv) = iobj.get("purposes") {
            let all_strings = pv
                .as_array()
                .map(|a| a.iter().all(|x| x.is_string()))
                .unwrap_or(false);
            if !all_strings {
                return Err(format!(
                    "{at}: purposes must be an array of purpose names (e.g. [\"diff-review\", \"acceptance\"])"
                ));
            }
            let strs: Vec<String> = pv
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_str().unwrap().to_string())
                .collect();
            let (parsed, err) = convert_to_slug_list(&strs, "purpose");
            if !err.is_empty() {
                return Err(format!("{at}: purposes: {err}"));
            }
            for p in &parsed {
                if !CONSULT_PURPOSES.contains(&p.as_str()) {
                    return Err(format!(
                        "{at}: purposes names '{p}' (known: {})",
                        CONSULT_PURPOSES.join(", ")
                    ));
                }
            }
            purposes = parsed;
        }
        // roles (same rule as an ordinary entry).
        let mut roles: Vec<String> = Vec::new();
        if let Some(rv) = iobj.get("roles") {
            let all_strings = rv
                .as_array()
                .map(|a| a.iter().all(|x| x.is_string()))
                .unwrap_or(false);
            if !all_strings {
                return Err(format!(
                    "{at}: roles must be an array of role names (e.g. [\"security\", \"tests\"])"
                ));
            }
            let strs: Vec<String> = rv
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_str().unwrap().to_string())
                .collect();
            let (parsed, err) = convert_to_slug_list(&strs, "role");
            if !err.is_empty() {
                return Err(format!("{at}: roles: {err}"));
            }
            roles = parsed;
        }
        // api_billing (only "accepted" allowed; relaxes the lab-label billing guard).
        let mut api_billing_accepted = false;
        if let Some(av) = iobj.get("api_billing") {
            match av.as_str() {
                Some("accepted") => api_billing_accepted = true,
                _ => {
                    return Err(format!(
                        "{at}: api_billing may only be \"accepted\" (it accepts per-token billing at a lab that also sells a subscription; omit it otherwise; got {})",
                        json_type(av)
                    ))
                }
            }
        }
        // (S6) pack_tokens: the periphery budget in tokens (0..=200000); -1 when omitted.
        let mut pack_tokens: i64 = -1;
        if let Some(tv) = iobj.get("pack_tokens") {
            if !is_json_integer(tv)
                || tv.as_f64().unwrap_or(-1.0) < 0.0
                || tv.as_f64().unwrap_or(-1.0) > MAX_PACK_TOKENS as f64
            {
                return Err(format!(
                    "{at}: pack_tokens must be an integer from 0 to {MAX_PACK_TOKENS} (the reviewer pack's periphery budget; got {})",
                    json_type(tv)
                ));
            }
            pack_tokens = tv.as_i64().unwrap_or(-1);
        }
        // A duplicate reviewer (provider + model) within the extension, matching the plugin's
        // duplicate-entry refusal wording.
        if let Some(dup) = out
            .iter()
            .find(|e| e.provider == provider && e.model == model)
        {
            return Err(format!(
                "ext.c3.reviewers entries {} and {i} are the same reviewer {} :: {} [http]",
                dup.position - plugin_count,
                cap64(&provider),
                cap64(&model)
            ));
        }
        out.push(HttpReviewer {
            position: plugin_count + out.len() + 1,
            provider,
            model,
            lab,
            weight,
            base_url,
            key_env,
            json_object,
            headers,
            purposes,
            roles,
            api_billing_accepted,
            pack_tokens,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str, plugin_count: usize) -> Result<Vec<HttpReviewer>, String> {
        let data: Value = serde_json::from_str(json).unwrap();
        parse_ext_reviewers(&data, plugin_count)
    }

    #[test]
    fn absent_extension_is_no_reviewers() {
        assert!(parse(r#"{"roster_version":1,"reviewers":[]}"#, 1)
            .unwrap()
            .is_empty());
        // ext without c3.reviewers is also empty (the plugin ignores ext content).
        assert!(parse(r#"{"ext":{"note":"ignored"}}"#, 1)
            .unwrap()
            .is_empty());
        assert!(parse(r#"{"ext":{"c3":{"other":1}}}"#, 1)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn full_reviewer_parses_with_defaults_and_position() {
        let rs = parse(
            r##"{"ext":{"c3":{"reviewers":[
                {"provider":"openrouter","model":"openai/gpt-5","engine":"http","lab":"OpenAI",
                 "weight":2,"base_url":"https://openrouter.ai/api/v1","key_env":"OPENROUTER_API_KEY",
                 "json_object":true,"headers":{"HTTP-Referer":"https://xelth.com","X-Title":"c3"},
                 "purposes":["diff-review"],"roles":["security","tests"],"api_billing":"accepted"},
                {"provider":"or2","model":"anthropic/claude"}
            ]}}}"##,
            3,
        )
        .unwrap();
        assert_eq!(rs.len(), 2);
        assert_eq!(rs[0].position, 4, "continues after 3 plugin entries");
        assert_eq!(rs[1].position, 5);
        assert_eq!(rs[0].lab, "openai", "lab canonical lowercase");
        assert_eq!(rs[0].weight, 2);
        assert_eq!(
            rs[0].headers,
            vec![
                ("HTTP-Referer".to_string(), "https://xelth.com".to_string()),
                ("X-Title".to_string(), "c3".to_string()),
            ]
        );
        assert_eq!(rs[0].purposes, vec!["diff-review"]);
        assert_eq!(rs[0].roles, vec!["security", "tests"]);
        assert!(rs[0].api_billing_accepted);
        // Defaults on the minimal second reviewer.
        assert_eq!(rs[1].base_url, DEFAULT_BASE_URL);
        assert_eq!(rs[1].key_env, DEFAULT_KEY_ENV);
        assert!(rs[1].json_object);
        assert_eq!(rs[1].weight, 1);
        // The synthesized entry is visible to the panel/providers as an http entry.
        let e = rs[0].to_entry();
        assert_eq!(e.engine, "http");
        assert_eq!(e.position, 4);
        assert_eq!(e.provider, "openrouter");
        assert_eq!(e.roles, vec!["security", "tests"]);
    }

    fn err(json: &str) -> String {
        parse(json, 0).unwrap_err()
    }

    fn one(inner: &str) -> String {
        err(&format!(r#"{{"ext":{{"c3":{{"reviewers":[{inner}]}}}}}}"#))
    }

    #[test]
    fn every_refusal() {
        assert!(err(r#"{"ext":{"c3":{"reviewers":{}}}}"#).contains("must be an array"));
        assert!(one("5").contains("is not an object"));
        assert!(one(r#"{"provider":"or","model":"m","nope":1}"#).contains("unknown key 'nope'"));
        assert!(one(r#"{"provider":"or","model":"m","engine":"agy"}"#)
            .contains("engine must be \"http\""));
        assert!(one(r#"{"model":"m"}"#).contains("needs a provider"));
        assert!(one(r#"{"provider":" or ","model":"m"}"#).contains("needs a provider"));
        assert!(
            one(r#"{"provider":"o::r","model":"m"}"#).contains("provider must not contain '::'")
        );
        assert!(one(r#"{"provider":"or"}"#).contains("engine http needs a model"));
        assert!(one(r#"{"provider":"or","model":"m|x"}"#).contains("model must not contain '|'"));
        assert!(
            one(r#"{"provider":"or","model":"m","lab":" x "}"#).contains("lab must be a non-empty")
        );
        assert!(one(r#"{"provider":"or","model":"m","weight":0}"#)
            .contains("weight must be an integer >= 1"));
        assert!(
            one(r#"{"provider":"or","model":"m","base_url":"http://x.y"}"#)
                .contains("base_url must be https://")
        );
        assert!(one(r#"{"provider":"or","model":"m","base_url":"https://"}"#).contains("base_url"));
        assert!(one(r#"{"provider":"or","model":"m","key_env":"1BAD"}"#)
            .contains("key_env is not the name of a variable c3 knows"));
        // A non-string key_env is a type refusal, never echoing the value.
        assert!(one(r#"{"provider":"or","model":"m","key_env":123}"#)
            .contains("key_env must be a string (got a number)"));
        assert!(one(r#"{"provider":"or","model":"m","json_object":"yes"}"#)
            .contains("json_object must be true or false"));
        assert!(one(r#"{"provider":"or","model":"m","headers":[]}"#)
            .contains("headers must be an object"));
        assert!(one(r#"{"provider":"or","model":"m","headers":{"X":1}}"#)
            .contains("a header value must be a string"));
        assert!(
            one(r#"{"provider":"or","model":"m","headers":{"Authorization":"Bearer x"}}"#)
                .contains("a header named 'Authorization' is refused")
        );
        assert!(
            one("{\"provider\":\"or\",\"model\":\"m\",\"headers\":{\"X\":\"a\\nb\"}}")
                .contains("must not contain a carriage return or line feed")
        );
        assert!(one(r#"{"provider":"or","model":"m","purposes":["nope"]}"#)
            .contains("purposes names 'nope'"));
        assert!(
            one(r#"{"provider":"or","model":"m","purposes":"diff-review"}"#)
                .contains("purposes must be an array")
        );
        assert!(one(r#"{"provider":"or","model":"m","roles":"security"}"#)
            .contains("roles must be an array"));
        assert!(one(r#"{"provider":"or","model":"m","roles":["Bad Role"]}"#)
            .contains("roles: role 'Bad Role' is not a slug"));
        assert!(one(r#"{"provider":"or","model":"m","api_billing":"yes"}"#)
            .contains("api_billing may only be \"accepted\""));
        // Duplicate reviewer within the extension.
        assert!(err(
            r#"{"ext":{"c3":{"reviewers":[{"provider":"or","model":"m"},{"provider":"or","model":"m"}]}}}"#
        )
        .contains("are the same reviewer"));
    }

    #[test]
    fn env_name_rule() {
        assert!(is_env_name("OPENROUTER_API_KEY"));
        assert!(is_env_name("_X1"));
        assert!(!is_env_name("1X"));
        assert!(!is_env_name("A-B"));
        assert!(!is_env_name(""));
    }

    // ------------------------------------------------------------------ S1 key-to-host binding

    #[test]
    fn s1_every_known_pair_accepted() {
        for (key, host) in KNOWN_KEYS {
            assert!(
                check_key_host(key, host).is_ok(),
                "{key} -> {host} should be accepted"
            );
            // A subdomain of the bound host is accepted too.
            assert!(check_key_host(key, &format!("eu.{host}")).is_ok());
        }
    }

    #[test]
    fn s1_known_key_foreign_host_refused() {
        let e = check_key_host("OPENAI_API_KEY", "evil.example").unwrap_err();
        assert!(
            e.contains("OPENAI_API_KEY is bound to api.openai.com"),
            "{e}"
        );
        assert!(e.contains("got host evil.example"), "{e}");
    }

    #[test]
    fn s1_look_alike_hosts_refused() {
        // A suffix that only *contains* the bound host, and a look-alike that shares a substring.
        assert!(check_key_host("OPENAI_API_KEY", "api.openai.com.evil.example").is_err());
        assert!(check_key_host("OPENROUTER_API_KEY", "evilopenrouter.ai").is_err());
        assert!(check_key_host("OPENROUTER_API_KEY", "openrouter.ai.evil.example").is_err());
    }

    #[test]
    fn s1_foreign_secrets_to_openrouter_refused() {
        for key in ["GITHUB_TOKEN", "AWS_SECRET_ACCESS_KEY", "ANTHROPIC_API_KEY"] {
            let e = check_key_host(key, "openrouter.ai").unwrap_err();
            // ANTHROPIC is a known key bound elsewhere; the others are unknown, non-C3_KEY_.
            assert!(
                e.contains("bound to") || e.contains("is not the name of a variable c3 knows"),
                "{key}: {e}"
            );
        }
    }

    #[test]
    fn s1_custom_key_bound_to_its_encoded_host() {
        // The two worked examples from the brief.
        assert!(check_key_host("C3_KEY_API_EXAMPLE_COM", "api.example.com").is_ok());
        assert!(
            check_key_host("C3_KEY_MY__LLM_INTERNAL_EXAMPLE", "my-llm.internal.example").is_ok()
        );
        // The same variable sent to a different host is refused and names the right variable.
        let e = check_key_host("C3_KEY_API_EXAMPLE_COM", "evil.example").unwrap_err();
        assert!(
            e.contains("create a variable named C3_KEY_EVIL_EXAMPLE"),
            "{e}"
        );
        // A near-miss host: api-example.com (one hyphen) does not match C3_KEY_API_EXAMPLE_COM.
        assert!(check_key_host("C3_KEY_API_EXAMPLE_COM", "api-example.com").is_err());
        // An IP literal cannot use a custom key at all.
        let e = check_key_host("C3_KEY_10_0_0_1", "10.0.0.1").unwrap_err();
        assert!(e.contains("is not one"), "{e}");
        let e = check_key_host("C3_KEY_X", "[::1]").unwrap_err();
        assert!(e.contains("is not one"), "{e}");
        // The bare prefix with no suffix is not a well-formed C3 key.
        assert!(check_key_host("C3_KEY_", "anything.example").is_err());
    }

    #[test]
    fn s1_host_encoding_round_trips_and_is_injective() {
        let hosts = [
            "api.example.com",
            "my-llm.internal.example",
            "openrouter.ai",
            "eu.api.openai.com",
            "a--b.example",
            "api-example.com",
            "x.y.z",
        ];
        let mut names = std::collections::HashSet::new();
        for h in hosts {
            let name = c3_key_name_for_host(h).unwrap_or_else(|| panic!("{h} should encode"));
            // Round-trip: the encoded variable is accepted for its own host only.
            assert!(check_key_host(&name, h).is_ok(), "{name} for {h}");
            for other in hosts {
                if other != h {
                    assert!(
                        check_key_host(&name, other).is_err(),
                        "{name} must not be accepted for {other}"
                    );
                }
            }
            // Injective: no two distinct hosts share a variable name.
            assert!(names.insert(name.clone()), "duplicate encoding {name}");
        }
        // An IP literal and a host with an illegal character do not encode.
        assert!(
            c3_key_name_for_host("10.0.0.1").is_none(),
            "IPv4 literal is not a DNS host"
        );
        assert!(c3_key_name_for_host("under_score.example").is_none());
        assert!(c3_key_name_for_host("-bad.example").is_none());
        assert!(c3_key_name_for_host("bad-.example").is_none());
    }

    #[test]
    fn s6_key_env_value_that_looks_like_a_key_is_not_echoed() {
        // A pasted key value as key_env: refused, hinted, never echoed.
        let secret = "sk-or-v1-0123456789abcdef0123456789abcdef";
        let e = check_key_env_name(secret).unwrap_err();
        assert!(!e.contains(secret), "the value leaked: {e}");
        assert!(
            e.contains("key_env is not the name of a variable c3 knows"),
            "{e}"
        );
        assert!(e.contains("it looks like a key value"), "{e}");
    }

    // ------------------------------------------------------------------ S2 base URL parsing

    #[test]
    fn s2_base_url_parsing() {
        assert_eq!(
            parse_base_url("https://openrouter.ai/api/v1").unwrap(),
            "openrouter.ai"
        );
        assert_eq!(
            parse_base_url("https://EU.OpenRouter.AI:8443/x").unwrap(),
            "eu.openrouter.ai",
            "host lowercased, port and path allowed"
        );
        assert!(parse_base_url("http://openrouter.ai").is_err(), "scheme");
        assert!(parse_base_url("https://").is_err(), "no host");
        assert!(
            parse_base_url("https://user:pw@openrouter.ai").is_err(),
            "userinfo"
        );
        assert!(
            parse_base_url("https://openrouter.ai/?a=b").is_err(),
            "query"
        );
        assert!(
            parse_base_url("https://openrouter.ai/#frag").is_err(),
            "fragment"
        );
        assert!(
            parse_base_url("https://openrouter.ai/\u{0007}").is_err(),
            "control char"
        );
        assert!(parse_base_url(&format!("https://{}", "a".repeat(3000))).is_err());
    }

    #[test]
    fn s1_s2_wired_into_the_roster_parser() {
        // A key sent to a foreign host is refused at parse time.
        assert!(one(
            r#"{"provider":"or","model":"m","key_env":"GITHUB_TOKEN","base_url":"https://openrouter.ai"}"#
        )
        .contains("key_env is not the name of a variable c3 knows"));
        assert!(one(
            r#"{"provider":"or","model":"m","key_env":"OPENAI_API_KEY","base_url":"https://evil.example"}"#
        )
        .contains("OPENAI_API_KEY is bound to api.openai.com"));
        // A C3_KEY_ variable is accepted only for the host its name encodes.
        assert!(parse(
            r#"{"ext":{"c3":{"reviewers":[{"provider":"local","model":"m","key_env":"C3_KEY_MY_HOST","base_url":"https://my.host/v1"}]}}}"#,
            0
        )
        .is_ok());
        // The same variable at a different host is refused at parse time.
        assert!(one(
            r#"{"provider":"local","model":"m","key_env":"C3_KEY_MY_HOST","base_url":"https://other.host/v1"}"#
        )
        .contains("create a variable named C3_KEY_OTHER_HOST"));
        // A well-formed openai pair is accepted.
        assert!(parse(
            r#"{"ext":{"c3":{"reviewers":[{"provider":"openai","model":"gpt-5","key_env":"OPENAI_API_KEY","base_url":"https://api.openai.com/v1"}]}}}"#,
            0
        )
        .is_ok());
    }

    // ------------------------------------------------------------------ S5 reserved headers

    #[test]
    fn s5_reserved_and_malformed_headers_refused() {
        for h in RESERVED_HEADERS {
            assert!(header_name_problem(h).is_some(), "{h} must be refused");
            // Case-insensitive.
            assert!(header_name_problem(&h.to_uppercase()).is_some());
        }
        assert!(header_name_problem("X-Title").is_none());
        assert!(header_name_problem("HTTP-Referer").is_none());
        // (S6) A header name that suggests a credential is refused (the roster holds no secrets).
        for h in [
            "X-Api-Key",
            "X-Auth-Token",
            "My-Secret",
            "Session-Id",
            "X-Password",
            "X-Credential",
        ] {
            let p = header_name_problem(h).unwrap_or_else(|| panic!("{h} must be refused"));
            assert!(p.contains("carries a credential"), "{h}: {p}");
        }
        assert!(
            header_name_problem("Bad Header").is_some(),
            "space not a token"
        );
        assert!(
            header_name_problem("bad:name").is_some(),
            "colon not a token"
        );
        assert!(header_name_problem("").is_some());
        // Wired into the roster parser.
        assert!(
            one(r#"{"provider":"or","model":"m","headers":{"Cookie":"x"}}"#)
                .contains("a header named 'Cookie' is refused")
        );
        assert!(
            one(r#"{"provider":"or","model":"m","headers":{"Bad Name":"x"}}"#)
                .contains("not a valid HTTP token")
        );
    }

    // ------------------------------------------------------------------ S6 no-secret canary

    #[test]
    fn s6_no_refusal_echoes_a_field_value() {
        // A unique marker fed into each protected field, in a wrong type or an invalid value:
        // no returned refusal may contain the marker (it could be a pasted secret). base_url and
        // key_env, a header value, and every wrong-typed field are covered; a header NAME that
        // carries the marker but suggests a credential is refused with a fixed message too.
        const M: &str = "qzcanary-7f3a9c-secretmarker";
        let cases = [
            format!(r#"{{"provider":"p","model":"m","weight":"{M}"}}"#),
            format!(r#"{{"provider":"p","model":"m","json_object":"{M}"}}"#),
            format!(r#"{{"provider":"p","model":"m","headers":"{M}"}}"#),
            format!(r#"{{"provider":"p","model":"m","headers":{{"X-Data":{{"k":"{M}"}}}}}}"#),
            format!(r#"{{"provider":"p","model":"m","headers":{{"X-{M}-Key":"v"}}}}"#),
            format!(r#"{{"provider":"p","model":"m","key_env":"{M}"}}"#),
            format!(r#"{{"provider":"p","model":"m","key_env":{{"k":"{M}"}}}}"#),
            format!(r#"{{"provider":"p","model":"m","base_url":"{M}"}}"#),
            format!(r#"{{"provider":"p","model":"m","base_url":{{"k":"{M}"}}}}"#),
            format!(r#"{{"provider":"p","model":"m","engine":"{M}"}}"#),
            format!(r#"{{"provider":"p","model":"m","api_billing":"{M}"}}"#),
            format!(r#"{{"provider":"p","model":"m","pack_tokens":"{M}"}}"#),
        ];
        for inner in cases {
            let e = one(&inner);
            assert!(!e.is_empty(), "expected a refusal for: {inner}");
            assert!(!e.contains(M), "the marker leaked into a refusal: {e}");
        }
        // check_key_host and check_key_env_name never echo the key_env name either.
        assert!(!check_key_env_name(M).unwrap_err().contains(M));
        assert!(!check_key_host(M, "openrouter.ai").unwrap_err().contains(M));
    }

    // ------------------------------------------------------------------ S6 pack_tokens

    #[test]
    fn s6_pack_tokens() {
        let rs = parse(
            r#"{"ext":{"c3":{"reviewers":[{"provider":"openrouter","model":"m","pack_tokens":8000}]}}}"#,
            0,
        )
        .unwrap();
        assert_eq!(rs[0].pack_tokens, 8000);
        // Omitted -> -1 (the run applies --pack-budget, else the default).
        let rs2 = parse(
            r#"{"ext":{"c3":{"reviewers":[{"provider":"openrouter","model":"m"}]}}}"#,
            0,
        )
        .unwrap();
        assert_eq!(rs2[0].pack_tokens, -1);
        assert!(
            one(r#"{"provider":"openrouter","model":"m","pack_tokens":-5}"#)
                .contains("pack_tokens must be an integer from 0 to 200000")
        );
        assert!(
            one(r#"{"provider":"openrouter","model":"m","pack_tokens":300000}"#)
                .contains("pack_tokens must be an integer from 0 to 200000")
        );
    }
}
