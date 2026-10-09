//! The `claude` engine's pure tables and rules (plugin 0.6.0, wave 29 / 29b): Claude Code headless
//! (`claude -p`) as a reviewer engine for the Claude subscription, an API key or a third-party
//! Anthropic-compatible endpoint (`auth: "endpoint"`).
//!
//! Ported from `codex-consult-common.ps1` at v0.6.1: `$script:ClaudeModels` and the model rules
//! (`ConvertTo-ClaudeModelBase`, `Get-ClaudeModelFamily`, `Test-ClaudeModelAlias`,
//! `Test-ClaudeModelMatch`, `Get-ClaudeModelProblem`), the endpoint object
//! (`ConvertFrom-ClaudeEndpointValue`, `Get-ClaudeEndpointProblem`), the ALLOW-listed child
//! environment (`Test-ClaudeChildEnvName`, `Get-ClaudeChildEnvironment` - pure over a variable list
//! here; the runtime passes its own environment), the argv (`Get-ClaudeArgs`,
//! `Get-ClaudeSchemaText`, `ConvertTo-CrtArg`) and `Format-Argv`. Nothing here reads the
//! environment, spawns a process or touches a file except [`schema_text_of_file`].

use serde_json::Value;

/// The aliases of the table (`$script:ClaudeModelAliases`): each floats - a thread is pinned to the
/// id its init event resolves.
pub const CLAUDE_MODEL_ALIASES: &[&str] = &["opus", "sonnet", "haiku", "fable"];

/// The closed model table (`$script:ClaudeModels`, 0.6.0 with `claude-haiku-5-5` of 2026-10-07): a
/// roster entry or `-Model` of auth subscription / api-key names one of them, optionally ending
/// with the 1M-context suffix `[1m]`. The telemetry class `anthropic`'s closed list IS this table.
pub const CLAUDE_MODELS: &[&str] = &[
    "opus",
    "sonnet",
    "haiku",
    "fable",
    "claude-fable-5-1",
    "claude-fable-5",
    "claude-opus-5-5",
    "claude-opus-5",
    "claude-opus-4-8",
    "claude-opus-4-7",
    "claude-opus-4-6",
    "claude-sonnet-5-5",
    "claude-sonnet-5",
    "claude-sonnet-4-6",
    "claude-haiku-5-5",
    "claude-haiku-4-5",
];

/// The roster's `auth` of the engine (`$script:ClaudeAuthModes`); the first is the default.
pub const CLAUDE_AUTH_MODES: &[&str] = &["subscription", "api-key", "endpoint"];

/// (wave 29b, E2) The model id of an endpoint entry (`$script:ClaudeEndpointModelRe`).
pub const ENDPOINT_MODEL_RE: &str = r"^[A-Za-z0-9][A-Za-z0-9._-]{0,63}(\[1m\])?$";
/// (E1) The keys of the `endpoint` object (`$script:ClaudeEndpointKeys`).
pub const ENDPOINT_KEYS: &[&str] = &["base_url", "env_key", "timeout_ms"];
/// (E1) The NAME of the variable that holds the endpoint's token (`$script:ClaudeEndpointEnvKeyRe`).
pub const ENDPOINT_ENV_KEY_RE: &str = r"^[A-Z][A-Z0-9_]{2,}$";
/// (E1) `API_TIMEOUT_MS` of an endpoint: the default and the bounds.
pub const ENDPOINT_TIMEOUT_DEFAULT: i64 = 3_000_000;
pub const ENDPOINT_TIMEOUT_MIN: i64 = 60_000;
pub const ENDPOINT_TIMEOUT_MAX: i64 = 7_200_000;

/// (item 4) The tools a claude turn may have (`$script:ClaudeTools`) - proven by every init event.
pub const CLAUDE_TOOLS: &[&str] = &["Read", "Grep", "Glob", "StructuredOutput"];
/// The `--tools` argument (`$script:ClaudeToolsArg`).
pub const CLAUDE_TOOLS_ARG: &str = "Read,Grep,Glob";
/// The `--permission-mode` (`$script:ClaudePermissionMode`).
pub const CLAUDE_PERMISSION_MODE: &str = "dontAsk";
/// (R22) What a reviewer child runs without (`$script:ClaudeSwitchedOff`, ledger
/// `engine_run.switched_off`).
pub const CLAUDE_SWITCHED_OFF: &[&str] = &[
    "user-settings",
    "project-settings",
    "local-settings",
    "instruction-files",
    "mcp-servers",
    "skills",
    "slash-commands",
    "code-tools",
    "web-tools",
    "write-tools",
    "autoupdater",
];
/// (D9) The largest prompt the engine is given on stdin (the row's `MaxPromptBytes`, 1 MiB).
pub const MAX_PROMPT_BYTES: u64 = 1_048_576;

fn re(pat: &str) -> regex::Regex {
    regex::Regex::new(pat).expect("a valid built-in regex")
}

/// Strip a trailing `[1m]` (any case) - the PowerShell `-replace '\[1m\]$', ''`.
fn strip_1m(model: &str) -> String {
    re(r"(?i)\[1m\]$").replace(model, "").to_string()
}

/// `ConvertTo-ClaudeModelBase`: a claude model name without its 1M-context suffix `[1m]`, trimmed
/// and lower-cased.
pub fn model_base(model: &str) -> String {
    strip_1m(model.trim()).to_lowercase()
}

/// `Get-ClaudeModelFamily`: `opus` | `sonnet` | `haiku` | `fable`, `""` when it names none.
pub fn model_family(model: &str) -> String {
    let b = model_base(model);
    if CLAUDE_MODEL_ALIASES.contains(&b.as_str()) {
        return b;
    }
    if let Some(c) = re(r"^claude-(opus|sonnet|haiku|fable)-").captures(&b) {
        return c[1].to_string();
    }
    String::new()
}

/// `Test-ClaudeModelAlias`: the name is an alias of the table (it floats).
pub fn is_alias(model: &str) -> bool {
    CLAUDE_MODEL_ALIASES.contains(&model_base(model).as_str())
}

/// `Test-ClaudeModelMatch`: `served` is the model `pinned` asks for - equal after the `[1m]` strip,
/// or `pinned` an alias and `served` an id of its family (`claude-<alias>-...`), or the other way
/// round. `exact` (auth endpoint): equality after the strip only - no alias family.
pub fn model_match(pinned: &str, served: &str, exact: bool) -> bool {
    let p = model_base(pinned);
    let s = model_base(served);
    if p.is_empty() || s.is_empty() {
        return false;
    }
    if p == s {
        return true;
    }
    if exact {
        return false;
    }
    if CLAUDE_MODEL_ALIASES.contains(&p.as_str()) {
        return s.starts_with(&format!("claude-{p}-"));
    }
    if CLAUDE_MODEL_ALIASES.contains(&s.as_str()) {
        return p.starts_with(&format!("claude-{s}-"));
    }
    false
}

/// `Get-ClaudeModelProblem`: `""` when a roster or `-Model` value names a model of the table (a
/// trailing `[1m]` allowed), else why not. (E2) auth `endpoint`: the open id pattern instead of
/// the table, and (E11) never an Anthropic id - the billing proof.
pub fn model_problem(model: &str, auth: &str) -> String {
    if model.trim().is_empty() {
        return "is empty".to_string();
    }
    if auth == "endpoint" {
        if !re(ENDPOINT_MODEL_RE).is_match(model) {
            return "is not a model id the endpoint route takes (the id as the provider publishes it: letters, digits, \".\", \"_\", \"-\", at most 64 characters, optionally ending with [1m])".to_string();
        }
        let eb = strip_1m(model).to_lowercase();
        if CLAUDE_MODELS.contains(&eb.as_str()) || eb.starts_with("claude-") {
            return "is an Anthropic model id, which the endpoint route cannot carry: the init event's apiKeySource is 'none' on this route as on the subscription, so only a model the subscription cannot serve proves the billing - name the provider's own model id".to_string();
        }
        return String::new();
    }
    let b = strip_1m(model);
    if !CLAUDE_MODELS.contains(&b.as_str()) {
        return format!(
            "is not in the claude engine's model table ({}; each may end with [1m])",
            CLAUDE_MODELS.join(", ")
        );
    }
    String::new()
}

/// (wave 29b, E1) The endpoint of a roster entry with auth `endpoint` (`ConvertFrom-ClaudeEndpointValue`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClaudeEndpoint {
    /// As written - what `ANTHROPIC_BASE_URL` gets.
    pub base_url: String,
    /// Lower-case scheme and host, the explicit port, the path without a trailing slash - the
    /// route fingerprint's part.
    pub canonical: String,
    pub host_name: String,
    /// The NAME of the variable that holds the token - never its value.
    pub env_key: String,
    /// `API_TIMEOUT_MS`.
    pub timeout_ms: i64,
    /// The entry's plan slug, `""` without one (provider_config only).
    pub plan: String,
}

/// The base-URL refusal (the value is never echoed: it could hold a pasted credential).
pub const ENDPOINT_URL_WHY: &str = "endpoint.base_url must be an absolute https URL without credentials, query or fragment (e.g. \"https://api.z.ai/api/anthropic\"; the value is not shown)";
/// The env_key refusal.
pub const ENDPOINT_KEY_WHY: &str = "endpoint.env_key must be the NAME of the environment variable that holds the token (capital letters, digits and _, at least 3 characters, starting with a letter - e.g. \"ZAI_API_KEY\"), never the token itself (the value is not shown)";

fn compact(v: &Value) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

fn is_json_integer(v: &Value) -> bool {
    if v.is_i64() || v.is_u64() {
        return true;
    }
    v.as_f64()
        .map(|f| f.is_finite() && f.fract() == 0.0)
        .unwrap_or(false)
}

/// Whether `bu` is an absolute https URL without credentials, query or fragment (the plugin's
/// `^(?i)https://[^/?#@]+` + `[Uri]::TryCreate` rules).
fn base_url_ok(bu: &str) -> bool {
    if bu.trim().is_empty() || bu.chars().any(|c| c.is_whitespace()) {
        return false;
    }
    if !re(r"^(?i)https://[^/?#@]+").is_match(bu) || bu.contains('#') || bu.contains('?') {
        return false;
    }
    match url::Url::parse(bu) {
        Ok(u) => {
            u.scheme() == "https"
                && u.host_str().map(|h| !h.is_empty()).unwrap_or(false)
                && u.username().is_empty()
                && u.password().is_none()
                && u.query().is_none()
                && u.fragment().is_none()
        }
        Err(_) => false,
    }
}

/// `ConvertFrom-ClaudeEndpointValue`: the endpoint object of a roster entry, or the refusal (without
/// the entry prefix). Neither a URL nor an env_key value is ever echoed.
pub fn endpoint_from_value(value: &Value, plan: &str) -> Result<ClaudeEndpoint, String> {
    let obj = match value.as_object() {
        Some(o) => o,
        None => return Err("endpoint must be an object {\"base_url\": \"https://...\", \"env_key\": \"<VARIABLE NAME>\", \"timeout_ms\": <milliseconds, optional>}".to_string()),
    };
    for k in obj.keys() {
        if !ENDPOINT_KEYS.contains(&k.as_str()) {
            return Err(format!(
                "endpoint has an unknown key '{k}' (allowed: {})",
                ENDPOINT_KEYS.join(", ")
            ));
        }
    }
    let bu = match obj.get("base_url").and_then(|v| v.as_str()) {
        Some(s) if base_url_ok(s) => s.to_string(),
        _ => return Err(ENDPOINT_URL_WHY.to_string()),
    };
    let ek = match obj.get("env_key").and_then(|v| v.as_str()) {
        Some(s) if re(ENDPOINT_ENV_KEY_RE).is_match(s) => s.to_string(),
        _ => return Err(ENDPOINT_KEY_WHY.to_string()),
    };
    let mut tm = ENDPOINT_TIMEOUT_DEFAULT;
    if let Some(tv) = obj.get("timeout_ms") {
        let f = tv.as_f64().unwrap_or(f64::NAN);
        if !is_json_integer(tv)
            || f < ENDPOINT_TIMEOUT_MIN as f64
            || f > ENDPOINT_TIMEOUT_MAX as f64
        {
            return Err(format!(
                "endpoint.timeout_ms must be an integer from {ENDPOINT_TIMEOUT_MIN} to {ENDPOINT_TIMEOUT_MAX} (milliseconds - API_TIMEOUT_MS; default {ENDPOINT_TIMEOUT_DEFAULT}; got {})",
                compact(tv)
            ));
        }
        tm = f as i64;
    }
    let (canonical, host_name) = crate::config::canonical_base_url(&bu);
    Ok(ClaudeEndpoint {
        base_url: bu,
        canonical,
        host_name,
        env_key: ek,
        timeout_ms: tm,
        plan: plan.to_string(),
    })
}

/// `Get-ClaudeEndpointProblem`: `""` when an endpoint is usable for a launch, else why not.
pub fn endpoint_problem(ep: Option<&ClaudeEndpoint>) -> String {
    let ep = match ep {
        Some(e) => e,
        None => {
            return "auth endpoint names no endpoint (the roster entry's \"endpoint\" object)"
                .to_string()
        }
    };
    let ok = url::Url::parse(&ep.base_url)
        .map(|u| u.scheme() == "https" && u.host_str().map(|h| !h.is_empty()).unwrap_or(false))
        .unwrap_or(false);
    if ep.base_url.is_empty() || !ok {
        return "endpoint.base_url does not parse as an absolute https URL".to_string();
    }
    if !re(ENDPOINT_ENV_KEY_RE).is_match(&ep.env_key) {
        return "endpoint.env_key is not a variable name".to_string();
    }
    String::new()
}

// ------------------------------------------------------------------------- the child environment

/// (D2) The variables a claude child may inherit (`$script:ClaudeChildEnvNames`).
pub const CHILD_ENV_NAMES: &[&str] = &[
    "SystemRoot",
    "windir",
    "SystemDrive",
    "ComSpec",
    "PATH",
    "PATHEXT",
    "TEMP",
    "TMP",
    "TMPDIR",
    "USERPROFILE",
    "HOME",
    "HOMEDRIVE",
    "HOMEPATH",
    "APPDATA",
    "LOCALAPPDATA",
    "ProgramData",
    "ALLUSERSPROFILE",
    "PUBLIC",
    "PSModulePath",
    "USERNAME",
    "USERDOMAIN",
    "COMPUTERNAME",
    "USER",
    "LOGNAME",
    "SHELL",
    "TERM",
    "PROCESSOR_ARCHITECTURE",
    "PROCESSOR_IDENTIFIER",
    "PROCESSOR_LEVEL",
    "PROCESSOR_REVISION",
    "NUMBER_OF_PROCESSORS",
    "OS",
    "LANG",
    "LANGUAGE",
    "TZ",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "XDG_STATE_HOME",
    "XDG_RUNTIME_DIR",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "ALL_PROXY",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "REQUESTS_CA_BUNDLE",
    "CURL_CA_BUNDLE",
    "NODE_EXTRA_CA_CERTS",
    "CLAUDE_CONFIG_DIR",
];
/// The prefixes that pass too (`$script:ClaudeChildEnvPrefixes`).
pub const CHILD_ENV_PREFIXES: &[&str] = &[
    "ProgramFiles",
    "CommonProgramFiles",
    "ProgramW6432",
    "CommonProgramW6432",
    "LC_",
];
/// (wave 29b, E4) The variables auth endpoint SETS in the child (never inherited).
pub const ENDPOINT_ENV_NAMES: &[&str] = &[
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "API_TIMEOUT_MS",
];

/// `Test-ClaudeChildEnvName`: whether a variable of the parent reaches a claude child.
pub fn child_env_name_allowed(name: &str, auth: &str, pass_prefix: &str) -> bool {
    let u = name.to_uppercase();
    if CHILD_ENV_NAMES.iter().any(|n| n.to_uppercase() == u) {
        return true;
    }
    if CHILD_ENV_PREFIXES
        .iter()
        .any(|p| u.starts_with(&p.to_uppercase()))
    {
        return true;
    }
    if auth == "api-key" && u == "ANTHROPIC_API_KEY" {
        return true;
    }
    !pass_prefix.is_empty() && u.starts_with(&pass_prefix.to_uppercase())
}

/// (test hook) `CODEX_CONSULT_TEST_CHILD_ENV_PASS`'s value as the plugin accepts it: a prefix of 4+
/// characters, never one of ANTHROPIC, CLAUDE or CODEX_CONSULT; `""` otherwise.
pub fn valid_pass_prefix(value: &str) -> String {
    let p = value.trim();
    if p.is_empty()
        || !re(r"^[A-Za-z][A-Za-z0-9_]{3,}$").is_match(p)
        || re(r"^(ANTHROPIC|CLAUDE|CODEX_CONSULT)").is_match(&p.to_uppercase())
    {
        return String::new();
    }
    p.to_string()
}

/// THE child environment of a claude process (`Get-ClaudeChildEnvironment`).
#[derive(Debug, Clone, Default)]
pub struct ChildEnv {
    pub auth: String,
    /// name -> value, every allowed variable plus the set ones (`DISABLE_AUTOUPDATER=1`; with auth
    /// endpoint the route's three variables). Holds a token with auth endpoint: never log it.
    pub env: Vec<(String, String)>,
    /// The names, sorted ordinally - the ledger's `engine_run.child_env_allowed` (never a value).
    pub names: Vec<String>,
    /// The names of the parent's variables the child does not get, sorted ordinally.
    pub removed: Vec<String>,
    /// `""` or why no child may start with it (auth endpoint without a usable endpoint or token).
    pub problem: String,
}

/// `Get-ClaudeChildEnvironment` over the parent's variables `vars` (name, value), for `auth`
/// (`subscription` when not one of [`CLAUDE_AUTH_MODES`]) and - auth endpoint - `endpoint`, with
/// the (already validated) test pass prefix. `case_insensitive`: Windows variable names. `token`:
/// the endpoint's token, read by the caller from the variable `endpoint.env_key` names at the
/// moment the environment is built (`""` when unset or blank).
pub fn child_environment(
    vars: &[(String, String)],
    auth: &str,
    endpoint: Option<&ClaudeEndpoint>,
    pass_prefix: &str,
    case_insensitive: bool,
    token: &str,
) -> ChildEnv {
    let auth = if CLAUDE_AUTH_MODES.contains(&auth) {
        auth
    } else {
        "subscription"
    };
    let key_of = |n: &str| -> String {
        if case_insensitive {
            n.to_uppercase()
        } else {
            n.to_string()
        }
    };
    let mut env: Vec<(String, String)> = Vec::new();
    let mut removed: Vec<String> = Vec::new();
    let has =
        |env: &Vec<(String, String)>, n: &str| env.iter().any(|(k, _)| key_of(k) == key_of(n));
    for (n, v) in vars {
        if n.is_empty() || n.contains('=') || has(&env, n) {
            continue;
        }
        if child_env_name_allowed(n, auth, pass_prefix) {
            env.push((n.clone(), v.clone()));
        } else if !n.eq_ignore_ascii_case("DISABLE_AUTOUPDATER")
            && !(auth == "endpoint" && ENDPOINT_ENV_NAMES.contains(&n.to_uppercase().as_str()))
            && !removed.contains(n)
        {
            removed.push(n.clone());
        }
    }
    let set = |env: &mut Vec<(String, String)>, k: &str, v: String| {
        env.retain(|(x, _)| key_of(x) != key_of(k));
        env.push((k.to_string(), v));
    };
    set(&mut env, "DISABLE_AUTOUPDATER", "1".to_string());
    let mut problem = String::new();
    if auth == "endpoint" {
        problem = endpoint_problem(endpoint);
        if problem.is_empty() {
            let ep = endpoint.expect("checked by endpoint_problem");
            set(&mut env, "ANTHROPIC_BASE_URL", ep.base_url.clone());
            set(&mut env, "API_TIMEOUT_MS", ep.timeout_ms.to_string());
            if !token.trim().is_empty() {
                set(&mut env, "ANTHROPIC_AUTH_TOKEN", token.to_string());
            } else {
                problem = format!("env {} not set (the token of auth endpoint)", ep.env_key);
            }
        }
    }
    env.sort_by(|a, b| a.0.cmp(&b.0));
    let mut names: Vec<String> = env.iter().map(|(k, _)| k.clone()).collect();
    names.sort();
    removed.sort();
    ChildEnv {
        auth: auth.to_string(),
        env,
        names,
        removed,
        problem,
    }
}

// ------------------------------------------------------------------------- the argv

/// `ConvertTo-CrtArg`: one argument of a CLI that parses its command line by the C runtime rules -
/// a bare token when safe, else double-quoted with every quote as `\"` and the backslashes before
/// a quote (and at the end) doubled.
pub fn crt_arg(value: &str) -> String {
    if !value.is_empty() && re(r"^[A-Za-z0-9_.\-:\\/=]+$").is_match(value) {
        return value.to_string();
    }
    let mut out = String::from("\"");
    let mut bs = 0usize;
    for ch in value.chars() {
        if ch == '\\' {
            bs += 1;
            continue;
        }
        if ch == '"' {
            out.push_str(&"\\".repeat(2 * bs + 1));
            out.push('"');
            bs = 0;
            continue;
        }
        if bs > 0 {
            out.push_str(&"\\".repeat(bs));
            bs = 0;
        }
        out.push(ch);
    }
    if bs > 0 {
        out.push_str(&"\\".repeat(2 * bs));
    }
    out.push('"');
    out
}

/// `Format-Argv`: the human-readable argv of the record - an argument with white space in double
/// quotes, every other one as it is.
pub fn format_argv(argv: &[String]) -> String {
    argv.iter()
        .map(|a| {
            if a.chars().any(|c| c.is_whitespace()) {
                format!("\"{a}\"")
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `Get-ClaudeSchemaText` on a schema's TEXT: whitespace outside strings removed (one line; the
/// strings keep theirs), a BOM dropped.
pub fn schema_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_str = false;
    let mut esc = false;
    for ch in text.chars() {
        if in_str {
            out.push(ch);
            if esc {
                esc = false;
            } else if ch == '\\' {
                esc = true;
            } else if ch == '"' {
                in_str = false;
            }
            continue;
        }
        if ch == '"' {
            in_str = true;
            out.push(ch);
            continue;
        }
        if ch.is_whitespace() || ch == '\u{FEFF}' {
            continue;
        }
        out.push(ch);
    }
    out
}

/// `Get-ClaudeSchemaText` on a file: `""` when it cannot be read.
pub fn schema_text_of_file(path: &std::path::Path) -> String {
    match std::fs::read(path) {
        Ok(b) => schema_text(&String::from_utf8_lossy(&b)),
        Err(_) => String::new(),
    }
}

/// The options of one claude turn (`New-EngineTurnOptions` as `Get-ClaudeArgs` reads it).
#[derive(Debug, Clone, Default)]
pub struct ClaudeArgs {
    /// The model: the roster's on a new thread, the RESOLVED id on every later turn (D4).
    pub model: String,
    /// The effort to send (`--effort`), when one is sent.
    pub effort: Option<String>,
    /// The schema TEXT (`--json-schema`), when the transport is native.
    pub schema_text: Option<String>,
    /// `--max-turns` (`-MaxModelSteps`; 0 = not sent).
    pub max_turns: u32,
    /// `--add-dir` directories (outside the repository).
    pub add_dirs: Vec<String>,
    /// The thread to continue (`--resume`); `""` for a new one.
    pub thread: String,
    /// A fork of `thread` (`--fork-session`).
    pub fork: bool,
    /// The minted id of a new thread (`--session-id`).
    pub new_thread: String,
}

/// `Get-ClaudeArgs`: the argv of one claude turn after the launcher. The prompt travels on stdin.
pub fn claude_args(t: &ClaudeArgs) -> Vec<String> {
    let mut a: Vec<String> = [
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--restricted",
        "--strict-mcp-config",
        "--disable-slash-commands",
        "--tools",
        CLAUDE_TOOLS_ARG,
        "--permission-mode",
        CLAUDE_PERMISSION_MODE,
        "--model",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    a.push(t.model.clone());
    if let Some(e) = t.effort.as_ref().filter(|e| !e.trim().is_empty()) {
        a.push("--effort".into());
        a.push(e.clone());
    }
    if let Some(s) = &t.schema_text {
        a.push("--json-schema".into());
        a.push(s.clone());
    }
    if t.max_turns > 0 {
        a.push("--max-turns".into());
        a.push(t.max_turns.to_string());
    }
    for d in t.add_dirs.iter().filter(|d| !d.is_empty()) {
        a.push("--add-dir".into());
        a.push(d.clone());
    }
    if !t.thread.is_empty() {
        a.push("--resume".into());
        a.push(t.thread.clone());
        if t.fork {
            a.push("--fork-session".into());
        }
    } else if !t.new_thread.is_empty() {
        a.push("--session-id".into());
        a.push(t.new_thread.clone());
    }
    a
}

/// `ConvertTo-ClaudeToken`: a short identifier as the CLI reported it (authMethod, apiProvider,
/// apiKeySource, a permission mode), `""` when absent, `unrecognized` for anything else.
pub fn token(value: Option<&Value>) -> String {
    match value.and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => {
            if re(r"^[A-Za-z][A-Za-z0-9_.-]{0,39}$").is_match(s) {
                s.to_string()
            } else {
                "unrecognized".to_string()
            }
        }
        _ => String::new(),
    }
}

/// The lab of an endpoint entry by its base URL's host (`Get-EntryLab` through
/// `Get-TelemetryVendorByHost`, wave 29b E5): the vendor table's `Lab` of the class whose host the
/// URL's host equals or ends with (`.` + it) - openai.com / chatgpt.com openai, z.ai / bigmodel.cn
/// zhipu, xiaomimimo.com xiaomi, kimi.ai / moonshot.ai moonshot, api.minimax.io / .cn minimax;
/// `""` for another host (or a class without a lab: byteplus, alibaba).
pub fn lab_of_host(host: &str) -> &'static str {
    let h = host.to_lowercase();
    let is = |d: &str| h == d || h.ends_with(&format!(".{d}"));
    if is("openai.com") || is("chatgpt.com") {
        "openai"
    } else if is("z.ai") || is("bigmodel.cn") {
        "zhipu"
    } else if is("xiaomimimo.com") {
        "xiaomi"
    } else if is("kimi.ai") || is("moonshot.ai") {
        "moonshot"
    } else if is("api.minimax.io") || is("api.minimax.cn") {
        "minimax"
    } else {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_model_table_and_the_family_rule() {
        assert!(model_problem("claude-opus-5-5[1m]", "subscription").is_empty());
        assert!(model_problem("claude-haiku-5-5", "api-key").is_empty());
        assert!(model_problem("sonnet", "").is_empty());
        assert_eq!(
            model_problem("gpt-5.1", "subscription"),
            "is not in the claude engine's model table (opus, sonnet, haiku, fable, claude-fable-5-1, claude-fable-5, claude-opus-5-5, claude-opus-5, claude-opus-4-8, claude-opus-4-7, claude-opus-4-6, claude-sonnet-5-5, claude-sonnet-5, claude-sonnet-4-6, claude-haiku-5-5, claude-haiku-4-5; each may end with [1m])"
        );
        assert!(!model_problem("claude-opus-9", "subscription").is_empty());
        assert_eq!(model_family("claude-opus-5-5[1m]"), "opus");
        assert_eq!(model_family("Sonnet"), "sonnet");
        assert_eq!(model_family("glm-5.3"), "");
        assert!(is_alias("haiku[1m]"));
        assert!(!is_alias("claude-haiku-5-5"));
        assert!(model_match("sonnet", "claude-sonnet-5-5", false));
        assert!(model_match("claude-sonnet-5-5", "sonnet", false));
        assert!(model_match("claude-opus-5-5[1m]", "claude-opus-5-5", true));
        assert!(!model_match("sonnet", "claude-sonnet-5-5", true));
        assert!(!model_match("sonnet", "claude-opus-5-5", false));
    }

    #[test]
    fn the_endpoint_route_takes_the_providers_own_ids_only() {
        assert!(model_problem("glm-5.3", "endpoint").is_empty());
        assert!(model_problem("mimo-v2.6-pro[1m]", "endpoint").is_empty());
        assert!(model_problem("glm 5.3", "endpoint").starts_with("is not a model id"));
        for m in [
            "claude-sonnet-5-5",
            "sonnet",
            "Claude-Opus-5-5[1m]",
            "claude-haiku-9",
            "FABLE",
        ] {
            assert!(
                model_problem(m, "endpoint").starts_with("is an Anthropic model id"),
                "{m}"
            );
        }
    }

    #[test]
    fn the_endpoint_object() {
        let ep = endpoint_from_value(
            &json!({"base_url": "https://API.Z.AI/api/anthropic/", "env_key": "ZAI_KEY"}),
            "zai",
        )
        .unwrap();
        assert_eq!(ep.canonical, "https://api.z.ai/api/anthropic");
        assert_eq!(ep.host_name, "api.z.ai");
        assert_eq!(ep.timeout_ms, 3_000_000);
        assert_eq!(ep.plan, "zai");
        let bad = |v: Value| endpoint_from_value(&v, "").unwrap_err();
        assert_eq!(bad(json!("https://x")), "endpoint must be an object {\"base_url\": \"https://...\", \"env_key\": \"<VARIABLE NAME>\", \"timeout_ms\": <milliseconds, optional>}");
        assert_eq!(
            bad(json!({"base_url":"https://a.example/v","env_key":"ABC","model":"x"})),
            "endpoint has an unknown key 'model' (allowed: base_url, env_key, timeout_ms)"
        );
        for u in [
            "http://api.z.ai/api/anthropic",
            "https://user:SECRET@api.z.ai/x",
            "https://api.z.ai/x?key=SECRET",
            "/api/anthropic",
        ] {
            assert_eq!(
                bad(json!({"base_url": u, "env_key": "ABC"})),
                ENDPOINT_URL_WHY
            );
        }
        assert_eq!(
            bad(json!({"base_url":"https://a.example/v","env_key":"sk-SECRET"})),
            ENDPOINT_KEY_WHY
        );
        assert_eq!(
            bad(json!({"base_url":"https://a.example/v","env_key":"ABC","timeout_ms":5})),
            "endpoint.timeout_ms must be an integer from 60000 to 7200000 (milliseconds - API_TIMEOUT_MS; default 3000000; got 5)"
        );
        assert_eq!(
            endpoint_problem(None),
            "auth endpoint names no endpoint (the roster entry's \"endpoint\" object)"
        );
    }

    #[test]
    fn the_child_environment_is_an_allow_list() {
        let vars: Vec<(String, String)> = [
            ("Path", "C:\\x"),
            ("USERPROFILE", "C:\\u"),
            ("CLAUDE_CONFIG_DIR", "C:\\c"),
            ("ANTHROPIC_BASE_URL", "http://127.0.0.1:9/"),
            ("ANTHROPIC_API_KEY", "k"),
            ("CLAUDE_CODE_EFFORT_LEVEL", "max"),
            ("W29_OPERATOR_VAR", "x"),
            ("CODEX_CONSULT_TEST_MODE", "1"),
            ("FAKE_CLAUDE_REPLY", "r"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        let s = child_environment(&vars, "subscription", None, "FAKE_CLAUDE_", true, "");
        assert!(s.names.contains(&"Path".to_string()));
        assert!(s.names.contains(&"CLAUDE_CONFIG_DIR".to_string()));
        assert!(s.names.contains(&"FAKE_CLAUDE_REPLY".to_string()));
        assert!(s.names.contains(&"DISABLE_AUTOUPDATER".to_string()));
        for absent in [
            "ANTHROPIC_BASE_URL",
            "ANTHROPIC_API_KEY",
            "CLAUDE_CODE_EFFORT_LEVEL",
            "W29_OPERATOR_VAR",
            "CODEX_CONSULT_TEST_MODE",
        ] {
            assert!(!s.names.iter().any(|n| n == absent), "{absent}");
        }
        assert!(s.removed.contains(&"W29_OPERATOR_VAR".to_string()));
        let a = child_environment(&vars, "api-key", None, "", true, "");
        assert!(a.names.contains(&"ANTHROPIC_API_KEY".to_string()));
        assert!(!a.names.iter().any(|n| n == "ANTHROPIC_BASE_URL"));
        assert_eq!(valid_pass_prefix("ANTHROPIC_"), "");
        assert_eq!(valid_pass_prefix("FAKE_CLAUDE_"), "FAKE_CLAUDE_");
        let ep = endpoint_from_value(
            &json!({"base_url":"https://api.z.ai/api/anthropic","env_key":"W29B_TOKEN"}),
            "zai",
        )
        .unwrap();
        let e = child_environment(&vars, "endpoint", Some(&ep), "", true, "tok");
        let pref: Vec<&String> = e
            .names
            .iter()
            .filter(|n| {
                n.starts_with("ANTHROPIC_") || n.starts_with("API_") || n.starts_with("CLAUDE")
            })
            .collect();
        assert_eq!(
            pref,
            vec![
                "ANTHROPIC_AUTH_TOKEN",
                "ANTHROPIC_BASE_URL",
                "API_TIMEOUT_MS",
                "CLAUDE_CONFIG_DIR"
            ]
        );
        assert!(e.problem.is_empty());
        let n = child_environment(&vars, "endpoint", Some(&ep), "", true, "");
        assert_eq!(
            n.problem,
            "env W29B_TOKEN not set (the token of auth endpoint)"
        );
        assert!(!n.names.iter().any(|x| x == "ANTHROPIC_AUTH_TOKEN"));
    }

    #[test]
    fn the_route_identities() {
        // (D7) engine + auth + model family; (E5) an endpoint ROUTE: the canonical base URL and the
        // token variable's NAME - the label, the model and the plan are no part of it
        let cfg = crate::config::config_not_found("C:/x/config.toml");
        let id = |model: &str, auth: &str, ep: Option<&ClaudeEndpoint>| {
            crate::lineage::resolve_reviewer_identity_auth(
                &cfg,
                "",
                model,
                "",
                "claude",
                "C:/x/claude.exe",
                auth,
                ep,
            )
        };
        let opus = id("opus", "", None);
        assert_eq!(opus.compat_string, "cc-engine-v1|claude|subscription|opus");
        assert_eq!(opus.provider, "anthropic");
        assert_eq!(
            opus.fingerprint,
            id("claude-opus-5-5[1m]", "subscription", None).fingerprint
        );
        assert_eq!(
            id("sonnet", "api-key", None).compat_string,
            "cc-engine-v1|claude|api-key|sonnet"
        );
        assert_eq!(
            opus.provider_config["credential_mechanism"],
            serde_json::json!("subscription")
        );
        let ep = endpoint_from_value(
            &json!({"base_url":"https://API.Z.AI/api/anthropic/","env_key":"ZAI_KEY"}),
            "zai",
        )
        .unwrap();
        let r = id("glm-5.3", "endpoint", Some(&ep));
        assert_eq!(
            r.compat_string,
            "cc-engine-v1|claude|endpoint|https://api.z.ai/api/anthropic|ZAI_KEY"
        );
        let keys: Vec<&String> = r.provider_config.as_object().unwrap().keys().collect();
        assert_eq!(
            keys,
            vec![
                "engine",
                "launcher",
                "credential_mechanism",
                "base_url",
                "env_key",
                "plan"
            ]
        );
        assert!(id("glm-5.3", "endpoint", None)
            .error
            .contains("auth endpoint is not usable: auth endpoint names no endpoint"));
        assert!(id("", "", None)
            .error
            .starts_with("the claude engine needs a model"));
    }

    #[test]
    fn the_argv_and_its_quoting() {
        let a = claude_args(&ClaudeArgs {
            model: "sonnet".into(),
            effort: Some("high".into()),
            schema_text: Some("{\"a\":1}".into()),
            max_turns: 12,
            add_dirs: vec!["D:\\outside dir".into()],
            new_thread: "u".into(),
            ..Default::default()
        });
        assert_eq!(
            a.join(" "),
            "-p --output-format stream-json --verbose --restricted --strict-mcp-config --disable-slash-commands --tools Read,Grep,Glob --permission-mode dontAsk --model sonnet --effort high --json-schema {\"a\":1} --max-turns 12 --add-dir D:\\outside dir --session-id u"
        );
        let f = claude_args(&ClaudeArgs {
            model: "m".into(),
            thread: "t".into(),
            fork: true,
            new_thread: "ignored".into(),
            ..Default::default()
        });
        assert!(f.join(" ").ends_with("--resume t --fork-session"));
        assert_eq!(crt_arg("C:\\a b\\"), "\"C:\\a b\\\\\"");
        assert_eq!(crt_arg(""), "\"\"");
        assert_eq!(crt_arg("{\"a\":\"b\"}"), "\"{\\\"a\\\":\\\"b\\\"}\"");
        assert_eq!(schema_text("{\n  \"a\": \"x y\"\n}"), "{\"a\":\"x y\"}");
        assert_eq!(format_argv(&["a".into(), "b c".into()]), "a \"b c\"");
    }
}
