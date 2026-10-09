//! Host markers, the child-environment scrub, coordinator host detection and the
//! `CODEX_CONSULT_COORDINATOR` matcher (wave 27 / 27b). Mirrors `codex-consult-common.ps1`:
//! `$script:HostMarkerNames` / `$script:HostMarkerPrefixes`, `Test-HostMarkerName`,
//! `Get-HostMarkerNames`, `Get-CoordinatorHost`, `ConvertFrom-ReviewerMatcher` /
//! `Resolve-CoordinatorIdentity` and `Format-CoordinatorText`.
//!
//! A reviewer child must never inherit the coordinator's session identity or its messaging
//! socket and token, so these variables are removed from every child the bridge launches. The
//! ledger records only the NAMES removed, never a value.

use crate::ledger::Coordinator;
use crate::roster::RosterEntry;

/// This machine's name, as the records' `host` and the "elsewhere" checks use it
/// (`[Environment]::MachineName` in the plugin): `COMPUTERNAME`, else `HOSTNAME`, else (not on
/// Windows, where `COMPUTERNAME` is always set) the kernel's host name from `/etc/hostname`, since
/// a Linux shell rarely exports `HOSTNAME` and a blank host would make every record look local.
pub fn machine_name() -> String {
    let from_env = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_default();
    if !from_env.trim().is_empty() {
        return from_env;
    }
    #[cfg(not(windows))]
    {
        if let Ok(name) = std::fs::read_to_string("/etc/hostname") {
            let name = name.trim();
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }
    from_env
}

/// The exact host-marker names, in `$script:HostMarkerNames` source order (wave 27 + 27b). These
/// are removed by exact name — never the whole `CLAUDE_CODE_` prefix, so `CLAUDE_CODE_USE_BEDROCK`
/// (and a future claude engine's settings) survive.
pub const HOST_MARKER_NAMES: &[&str] = &[
    "CODEX_SESSION_ID",
    "CODEX_THREAD_ID",
    "CODEX_CI",
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "AI_AGENT",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_BRIDGE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_PID",
    "CLAUDE_EFFORT",
];

/// The wildcard marker prefixes (`$script:HostMarkerPrefixes`): every `CODEX_SANDBOX*` and (wave
/// 27c, D21) the WHOLE `ZCODE_` prefix — it replaces the two exact `ZCODE_SESSION_ID`/
/// `ZCODE_PROJECT_DIR` names and the narrower `ZCODE_PLUGIN` prefix of 27b. Read live inside a Z
/// Code session on 2026-09-29: two `ZCODE_*` names point at the operator's provider-config files,
/// no reviewer engine reads any `ZCODE_` variable, so removing the whole prefix loses nothing and
/// covers a name a later build adds. The `CLAUDE_CODE_` names stay EXACT (a future claude engine's
/// settings must survive).
pub const HOST_MARKER_PREFIXES: &[&str] = &["CODEX_SANDBOX", "ZCODE_"];

/// `Test-HostMarkerName`: on Windows env names are case-insensitive, so the plugin uppercases the
/// name before an ordinal exact/prefix compare against the (uppercase) lists.
pub fn is_host_marker(name: &str) -> bool {
    let up = if cfg!(windows) {
        name.to_uppercase()
    } else {
        name.to_string()
    };
    HOST_MARKER_NAMES.iter().any(|n| *n == up)
        || HOST_MARKER_PREFIXES.iter().any(|p| up.starts_with(p))
}

/// `Get-HostMarkerNames` over an explicit list of the parent env's variable names: keep the ones
/// that are markers, sort ordinal, dedupe. The result is the ledger `child_env_scrubbed` value —
/// the names present in the parent env, never a value.
pub fn host_marker_names_in(env_names: &[String]) -> Vec<String> {
    let mut hit: Vec<String> = env_names
        .iter()
        .filter(|n| is_host_marker(n))
        .cloned()
        .collect();
    hit.sort();
    hit.dedup();
    hit
}

/// `Get-HostMarkerNames` reading THIS process's environment.
pub fn host_marker_names() -> Vec<String> {
    let names: Vec<String> = std::env::vars_os()
        .filter_map(|(k, _)| k.into_string().ok())
        .collect();
    host_marker_names_in(&names)
}

/// The coordinator host, from an env accessor. C3 supports ONE coordinator host, Claude Code
/// (operator decision 2026-09-29, single host): `claude-code` when the Claude Code markers are
/// present, otherwise `unknown`. The multi-host inference (Codex CLI, Z Code, …) stays with the
/// PowerShell bridge. A set-but-empty variable is not a hint (PowerShell truthiness).
pub fn coordinator_host_from(get: &dyn Fn(&str) -> Option<String>) -> &'static str {
    let nonempty = |k: &str| get(k).map(|v| !v.is_empty()).unwrap_or(false);
    let ai = get("AI_AGENT").unwrap_or_default();
    if nonempty("CLAUDECODE")
        || nonempty("CLAUDE_CODE_ENTRYPOINT")
        || ai.to_lowercase().starts_with("claude-code")
    {
        return "claude-code";
    }
    "unknown"
}

/// `Get-CoordinatorHost` reading THIS process's environment.
pub fn coordinator_host() -> &'static str {
    coordinator_host_from(&|k| std::env::var(k).ok())
}

/// A parsed `CODEX_CONSULT_COORDINATOR` value (`ConvertFrom-ReviewerMatcher`): the provider, the
/// model (`None` = every model of the provider) and the engine (`None` = any engine).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CoordinatorMatch {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub engine: Option<String>,
    /// (wave 27c, D11) `Some(true)` when the value resolves to a seated reviewer, `Some(false)`
    /// when it parses but no roster entry can match it (a coordinator outside the roster — said,
    /// not refused); `None` when there is no roster to check against.
    pub in_roster: Option<bool>,
    /// (wave 27c, D12) `Some("#n")` when the value is a roster position that names no seat here:
    /// not a refusal — the run goes on with a warning, the coordinator recorded `unresolved`.
    pub unresolved: Option<String>,
}

/// `Get-CodexConfigDefaults` (wave 27c, D9): the provider and the model the bridge would run with -
/// the Codex config's top-level `model_provider` (else `openai`) and its top-level `model` (`""`
/// when none). A model-less codex roster entry and a bare label of the default provider resolve to
/// them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexDefaults {
    pub provider: String,
    pub model: String,
}

impl Default for CodexDefaults {
    fn default() -> Self {
        CodexDefaults {
            provider: "openai".to_string(),
            model: String::new(),
        }
    }
}

/// [`CodexDefaults`] of a read Codex config (`Read-CodexConfigSubset`): a setting counts only when
/// the file and its top level parsed and the value is a non-empty plain string.
pub fn codex_config_defaults(config: &crate::config::CodexConfig) -> CodexDefaults {
    let mut d = CodexDefaults::default();
    if !(config.exists && config.ok) {
        return d;
    }
    let Some(top) = config.top().filter(|t| t.ok) else {
        return d;
    };
    let p = crate::config::get_toml_string(Some(top), "model_provider");
    if p.present && p.reason.is_empty() && !p.value.is_empty() {
        d.provider = p.value;
    }
    let m = crate::config::get_toml_string(Some(top), "model");
    if m.present && m.reason.is_empty() && !m.value.is_empty() {
        d.model = m.value;
    }
    d
}

/// The resolution half of `Resolve-CoordinatorIdentity` for a `CODEX_CONSULT_COORDINATOR` value
/// (`ConvertFrom-ReviewerMatcher`'s grammar: a `#<n>` roster position, a `<provider> :: <model>`
/// lineage, or a bare provider label, either with an optional ` [<engine>]` suffix) - THE one
/// coordinator resolver of the consultation's ledger record and of a rating's actor (F09-6). A
/// RESOLVED triple, through the same rules as a seated reviewer:
///
/// - `#n`: that entry's provider, its model - else, for a codex entry, the model the bridge would
///   run (`defaults.model`) - and its engine; a position naming no seat here is no refusal
///   (`unresolved`);
/// - a lineage: the model as given (a Claude id's `[1m]` suffix stripped); without an engine, its
///   first roster entry's engine, else codex;
/// - a bare label: the model of its roster entries when they name ONE model (a model-less codex
///   entry counting as `defaults.model`) and one engine; several models (or an entry without one)
///   leave the model unnamed; a label no entry carries is - for the config's own provider
///   (`defaults.provider`, ordinal) with a configured model and no other engine - that model on
///   codex.
///
/// (F14-2) "Ordinal" is the plugin's `-ceq`: the provider - against a roster entry's and against
/// `defaults.provider` - and a named model compare CASE-SENSITIVELY (`OpenAI` is not `openai`, as
/// in the roster's `-Require` matcher, `crate::roster::resolve_reviewer_matcher`); an engine
/// compares case-insensitively (`-eq`).
///
/// `roster` is `None` when there is no reviewer roster (then `in_roster` stays unknown). `Err`
/// carries the plugin's exact `<why>` (the caller wraps it, [`coordinator_refusal`]).
pub fn resolve_coordinator_match(
    value: &str,
    roster: Option<&[RosterEntry]>,
    defaults: &CodexDefaults,
) -> Result<CoordinatorMatch, String> {
    let t = value.trim();
    if t.is_empty() {
        return Err("an empty value".to_string());
    }
    // (wave 27c, D10) the grammar of a reviewer matcher is parsed by ONE function that the roster
    // validator shares (`c3_core::roster::parse_reviewer_matcher_grammar`).
    let mut g = crate::roster::parse_reviewer_matcher_grammar(t)?;
    // (wave 29, item 9) a coordinator's Claude model id may carry the 1M-context suffix: stripped
    if let Some(m) = g.model.as_mut() {
        let lower = m.to_ascii_lowercase();
        if lower.starts_with("claude-") && lower.ends_with("[1m]") {
            m.truncate(m.len() - "[1m]".len());
        }
    }
    // (wave 27c, D10 / F30-9) only a value that cannot be PARSED is refused: the grammar above,
    // then the shared character rule (`Get-IdentityStringProblem`) on the provider and the model.
    if g.position.is_none() {
        let p = g.provider.clone().unwrap_or_default();
        if let Some(why) = crate::roster::identity_string_problem(&p) {
            return Err(format!("the provider '{p}' {why}"));
        }
        if let Some(m) = &g.model {
            if let Some(why) = crate::roster::identity_string_problem(m) {
                return Err(format!("the model '{m}' {why}"));
            }
        }
    }
    let model_of = |e: &RosterEntry| -> String {
        if !e.model.is_empty() {
            e.model.clone()
        } else if e.engine.is_empty() || e.engine.eq_ignore_ascii_case("codex") {
            defaults.model.clone()
        } else {
            String::new()
        }
    };
    let engine_of = |e: &RosterEntry| -> String {
        if e.engine.is_empty() {
            "codex".to_string()
        } else {
            e.engine.clone()
        }
    };
    if let Some(pos) = g.position {
        // (wave 27c, D12) naming no seat here is NOT a refusal
        return Ok(match roster {
            None => CoordinatorMatch {
                unresolved: Some(format!("#{pos}")),
                ..Default::default()
            },
            Some(entries) => match entries.iter().find(|e| e.position as i64 == pos) {
                Some(e) => {
                    let m = model_of(e);
                    CoordinatorMatch {
                        provider: Some(e.provider.clone()),
                        model: (!m.is_empty()).then_some(m),
                        engine: Some(engine_of(e)),
                        in_roster: Some(true),
                        unresolved: None,
                    }
                }
                None => CoordinatorMatch {
                    unresolved: Some(format!("#{pos}")),
                    in_roster: Some(false),
                    ..Default::default()
                },
            },
        });
    }
    let provider = g.provider.unwrap_or_default();
    // `Test-ReviewerMatch`: the provider and the model (when named) ordinal (`-ceq`, case-
    // sensitive), the engine when named (`-eq`, case-insensitive) - F14-2
    let hits: Vec<&RosterEntry> = roster
        .unwrap_or(&[])
        .iter()
        .filter(|e| {
            e.provider == provider
                && g.model.as_ref().is_none_or(|m| &e.model == m)
                && g.engine
                    .as_ref()
                    .is_none_or(|en| engine_of(e).eq_ignore_ascii_case(en))
        })
        .collect();
    let mut model = g.model.clone();
    let mut engine = g.engine.clone();
    if g.model.is_some() {
        if engine.is_none() {
            engine = Some(
                hits.first()
                    .map(|e| engine_of(e))
                    .unwrap_or_else(|| "codex".to_string()),
            );
        }
    } else if !hits.is_empty() {
        let mut models: Vec<String> = Vec::new();
        let mut engines: Vec<String> = Vec::new();
        for e in &hits {
            let m = model_of(e);
            if !m.is_empty() && !models.contains(&m) {
                models.push(m);
            }
            let en = engine_of(e);
            if !engines.contains(&en) {
                engines.push(en);
            }
        }
        let every_entry_names_one = hits.iter().all(|e| !model_of(e).is_empty());
        if models.len() == 1 && engines.len() == 1 && every_entry_names_one {
            model = models.pop();
            engine = engines.pop();
        }
    } else if provider == defaults.provider
        && !defaults.model.is_empty()
        && engine
            .as_deref()
            .is_none_or(|e| e.eq_ignore_ascii_case("codex"))
    {
        model = Some(defaults.model.clone());
        engine = Some("codex".to_string());
    }
    // (wave 27c, D11) a coordinator no roster entry matches is SAID (`in_roster: false`), not
    // refused; a run with no roster leaves `in_roster` unknown.
    Ok(CoordinatorMatch {
        provider: Some(provider),
        model,
        engine,
        in_roster: roster.map(|_| !hits.is_empty()),
        unresolved: None,
    })
}

/// `Resolve-CoordinatorIdentity`: the ledger `coordinator` record of a `CODEX_CONSULT_COORDINATOR`
/// value (empty: the host hint alone - [`build_coordinator`] with no identity) resolved by
/// [`resolve_coordinator_match`]; `Err` the full refusal ([`coordinator_refusal`]) of a value that
/// does not parse. The consultation's coordinator and a rating's actor both come from here.
pub fn resolve_coordinator_identity(
    value: &str,
    roster: Option<&[RosterEntry]>,
    defaults: &CodexDefaults,
    host: &str,
) -> Result<Coordinator, String> {
    let t = value.trim();
    if t.is_empty() {
        return Ok(build_coordinator(host, None));
    }
    let m = resolve_coordinator_match(t, roster, defaults)
        .map_err(|why| coordinator_refusal(t, &why))?;
    Ok(build_coordinator(host, Some(&m)))
}

/// Wrap a `resolve_coordinator_match` `<why>` into the plugin's full refusal (`Stop-WithError`
/// text, exit 1). `value` is the trimmed `CODEX_CONSULT_COORDINATOR` value.
pub fn coordinator_refusal(value: &str, why: &str) -> String {
    format!(
        "CODEX_CONSULT_COORDINATOR='{value}' cannot be used: {why} - give '<provider> :: <model>' (optionally ' [<engine>]'), a roster position '#<n>' or a provider label; nothing was started."
    )
}

/// Build the ledger `coordinator` record (`Resolve-CoordinatorIdentity`). `matched` is `Some` when
/// `CODEX_CONSULT_COORDINATOR` was set and parsed (source `explicit`); `None` means no value —
/// then `source` is `inferred` when the host is known, else `none`.
pub fn build_coordinator(host: &str, matched: Option<&CoordinatorMatch>) -> Coordinator {
    match matched {
        Some(m) => Coordinator {
            provider: m.provider.clone(),
            model: m.model.clone(),
            engine: m.engine.clone(),
            host: host.to_string(),
            source: "explicit".to_string(),
            // (wave 27c, D11/D12) recorded after `source`: `in_roster: false` for a coordinator no
            // reviewer can match, `unresolved: "#n"` for a position naming no seat here.
            in_roster: m.in_roster,
            unresolved: m.unresolved.clone(),
            extra: Default::default(),
        },
        None => Coordinator {
            provider: None,
            model: None,
            engine: None,
            host: host.to_string(),
            source: if host == "unknown" {
                "none"
            } else {
                "inferred"
            }
            .to_string(),
            in_roster: None,
            unresolved: None,
            extra: Default::default(),
        },
    }
}

/// The coordinator's identity as `Format-CoordinatorText` shows it: `<provider> :: <model>` (+
/// ` [<engine>]` when the engine is not codex), `<provider> (model not named)`, a `#n` that names
/// no seat here (`#n (names no roster position here)`), or the no-identity note.
fn coordinator_lineage(c: &Coordinator) -> String {
    if let Some(un) = c.unresolved.as_deref().filter(|u| !u.is_empty()) {
        return format!("{un} (names no roster position here)");
    }
    match &c.provider {
        None => "(no identity given - CODEX_CONSULT_COORDINATOR is not set)".to_string(),
        Some(p) => {
            let mut s = match &c.model {
                Some(m) => format!("{p} :: {m}"),
                None => format!("{p} (model not named)"),
            };
            if let Some(e) = c
                .engine
                .as_deref()
                .filter(|e| !e.eq_ignore_ascii_case("codex"))
            {
                s.push_str(&format!(" [{e}]"));
            }
            s
        }
    }
}

/// `Format-CoordinatorId`: the coordinator's identity alone, as the console line of D11 names it -
/// `<provider> :: <model>` or `<provider>`, ` [<engine>]` when the engine is not codex.
pub fn format_coordinator_id(c: &Coordinator) -> String {
    let p = c.provider.clone().unwrap_or_default();
    let mut who = match c.model.as_deref().filter(|m| !m.is_empty()) {
        Some(m) => format!("{p} :: {m}"),
        None => p,
    };
    if let Some(e) = c
        .engine
        .as_deref()
        .filter(|e| !e.eq_ignore_ascii_case("codex"))
    {
        who.push_str(&format!(" [{e}]"));
    }
    who
}

/// `Format-CoordinatorText`: the dry-run `coordinator :` value -
/// `<identity>; host <host> (inferred, a hint); source <source>`, and (wave 27c, D11) ` (not in
/// the roster - no reviewer can match it)` for a coordinator no reviewer can match.
pub fn format_coordinator_text(c: &Coordinator) -> String {
    let mut text = format!(
        "{}; host {} (inferred, a hint); source {}",
        coordinator_lineage(c),
        c.host,
        c.source
    );
    if c.in_roster == Some(false) && c.unresolved.as_deref().is_none_or(|u| u.is_empty()) {
        text.push_str(" (not in the roster - no reviewer can match it)");
    }
    text
}

/// [`coordinator_reviewer_warning`] with the reviewer's roster `auth` (0.6.0, wave 29, item 9): a
/// reviewer of the claude engine (auth subscription or api-key) - the ENGINE fixes the vendor: the
/// coordinator's provider is compared with `anthropic` (any case) whatever the roster's label, its
/// engine may be unnamed / codex or claude, and the models after normalising
/// ([`crate::claude::model_match`]: `[1m]` stripped, an alias equal to any id of its family).
/// (wave 29b, E5) An endpoint entry is compared as a codex entry is (its label and its model).
pub fn coordinator_reviewer_warning_auth(
    c: &Coordinator,
    reviewer_provider: &str,
    reviewer_model: &str,
    reviewer_engine: &str,
    reviewer_lineage: &str,
    auth: &str,
) -> Option<String> {
    if reviewer_engine == "claude" && auth != "endpoint" {
        if c.source != "explicit" {
            return None;
        }
        let p = c.provider.as_deref()?;
        if !p.eq_ignore_ascii_case("anthropic") {
            return None;
        }
        if let Some(e) = &c.engine {
            if !e.is_empty()
                && !e.eq_ignore_ascii_case("codex")
                && !e.eq_ignore_ascii_case("claude")
            {
                return None;
            }
        }
        return match &c.model {
            Some(m) if crate::claude::model_match(m, reviewer_model, false) => Some(format!(
                "coordinator: {reviewer_lineage} is the coordinator's own model (CODEX_CONSULT_COORDINATOR) - a second opinion from the coordinator's own model, not an independent one"
            )),
            Some(_) => None,
            None => Some(format!(
                "coordinator: {reviewer_lineage} is a reviewer from the coordinator's own provider (model not named) (CODEX_CONSULT_COORDINATOR) - CODEX_CONSULT_COORDINATOR named the provider but not the model"
            )),
        };
    }
    coordinator_reviewer_warning(
        c,
        reviewer_provider,
        reviewer_model,
        reviewer_engine,
        reviewer_lineage,
    )
}

/// `Get-CoordinatorMatch` (the kind only): `own`, `provider` or `""` for a seated reviewer with
/// its roster `auth` - the claude rule of [`coordinator_reviewer_warning_auth`], else the generic
/// rule of [`coordinator_reviewer_warning`].
pub fn coordinator_match_kind_auth(
    c: &Coordinator,
    reviewer_provider: &str,
    reviewer_model: &str,
    reviewer_engine: &str,
    auth: &str,
) -> &'static str {
    match coordinator_reviewer_warning_auth(
        c,
        reviewer_provider,
        reviewer_model,
        reviewer_engine,
        "",
        auth,
    ) {
        Some(w) if w.contains("is the coordinator's own model") => "own",
        Some(_) => "provider",
        None => "",
    }
}

/// `Format-CoordinatorWarning`: the "the reviewer is the coordinator's own model" warning body
/// (without the `coordinator: ` prefix conventions of the caller), or `None` when the coordinator
/// is not an explicit reviewer identity that matches the reviewer being consulted.
pub fn coordinator_reviewer_warning(
    c: &Coordinator,
    reviewer_provider: &str,
    reviewer_model: &str,
    reviewer_engine: &str,
    reviewer_lineage: &str,
) -> Option<String> {
    if c.source != "explicit" {
        return None;
    }
    let p = c.provider.as_deref()?;
    if p != reviewer_provider {
        return None;
    }
    if let Some(e) = &c.engine {
        if e != reviewer_engine {
            return None;
        }
    }
    // (wave 27c, D9) "own model" is said only when provider, model AND engine are equal. A bare
    // provider label without a resolvable model gives the WEAKER warning — the reviewer comes from
    // the coordinator's own provider, but which model the coordinator ran was not named.
    match &c.model {
        Some(m) if m == reviewer_model => Some(format!(
            "coordinator: {reviewer_lineage} is the coordinator's own model (CODEX_CONSULT_COORDINATOR) - a second opinion from the coordinator's own model, not an independent one"
        )),
        Some(_) => None,
        None => Some(format!(
            "coordinator: {reviewer_lineage} is a reviewer from the coordinator's own provider (model not named) (CODEX_CONSULT_COORDINATOR) - CODEX_CONSULT_COORDINATOR named the provider but not the model"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The resolver with the bare defaults (`openai`, no configured model).
    fn parse(value: &str, roster: Option<&[RosterEntry]>) -> Result<CoordinatorMatch, String> {
        resolve_coordinator_match(value, roster, &CodexDefaults::default())
    }

    #[test]
    fn host_single() {
        // C3 supports one coordinator host: claude-code, else unknown (operator decision, single
        // host). The Claude Code markers name it; nothing else is inferred.
        let cc1 = |k: &str| (k == "CLAUDECODE").then(|| "1".to_string());
        assert_eq!(coordinator_host_from(&cc1), "claude-code");
        let cc2 = |k: &str| (k == "AI_AGENT").then(|| "claude-code/1".to_string());
        assert_eq!(coordinator_host_from(&cc2), "claude-code");
        // A Codex or Z Code marker is NOT inferred (single host): unknown.
        let codex = |k: &str| (k == "CODEX_THREAD_ID").then(|| "t".to_string());
        assert_eq!(coordinator_host_from(&codex), "unknown");
        let zcode = |k: &str| (k == "ZCODE_SESSION_ID").then(|| "z".to_string());
        assert_eq!(coordinator_host_from(&zcode), "unknown");
        let none = |_: &str| None;
        assert_eq!(coordinator_host_from(&none), "unknown");
    }

    #[test]
    fn markers_and_kept() {
        assert!(is_host_marker("CODEX_SESSION_ID"));
        assert!(is_host_marker("CLAUDE_CODE_MESSAGING_TOKEN"));
        assert!(is_host_marker("CODEX_SANDBOX_NETWORK"));
        assert!(is_host_marker("ZCODE_PLUGIN_DATA"));
        // (wave 27c, D21) the whole ZCODE_ prefix is a marker: the former exact names, the plugin
        // roots, and the provider-config / build / process names read live in a Z Code session.
        assert!(is_host_marker("ZCODE_SESSION_ID"));
        assert!(is_host_marker("ZCODE_PROJECT_DIR"));
        assert!(is_host_marker("ZCODE_APP_VERSION"));
        assert!(is_host_marker("ZCODE_BASE_URL"));
        assert!(is_host_marker("ZCODE_PERSONAL_PROVIDER_CONFIG_FILE"));
        assert!(is_host_marker("ZCODE_ANYTHING_A_LATER_BUILD_ADDS"));
        // The CLAUDE_CODE_ names stay EXACT — a future claude engine's settings survive.
        assert!(!is_host_marker("CLAUDE_CODE_USE_BEDROCK"));
        assert!(!is_host_marker("CLAUDE_PLUGIN_ROOT"));
        assert!(!is_host_marker("CODEX_HOME"));
        // A different prefix is never swept by ZCODE_.
        assert!(!is_host_marker("ZCODEX_SOMETHING"));
    }

    #[test]
    fn matcher_refusals() {
        // (wave 5, harness-host REFUSE D3) the plugin's character rule: the matcher's and the
        // seed's delimiters are refused; interior blanks are not
        assert_eq!(
            parse("open::ai", None).unwrap_err(),
            "the provider 'open::ai' must not contain '::'"
        );
        assert_eq!(
            parse("openai :: gpt|5", None).unwrap_err(),
            "the model 'gpt|5' must not contain '|'"
        );
        assert_eq!(
            parse("open,ai :: gpt-5", None).unwrap_err(),
            "the provider 'open,ai' must not contain ','"
        );
        assert_eq!(
            parse("openai :: gpt#5", None).unwrap_err(),
            "the model 'gpt#5' must not contain '#'"
        );
        let blank = parse("open ai :: gpt 5", None).unwrap();
        assert_eq!(blank.provider.as_deref(), Some("open ai"));
        assert_eq!(blank.model.as_deref(), Some("gpt 5"));
        assert_eq!(
            parse("openai :: gpt-5.1 [bad]", None).unwrap_err(),
            "'openai :: gpt-5.1 [bad]' names the engine 'bad' (known: codex, agy, muse, claude)"
        );
        let ok = parse("openai :: gpt-5.1", None).unwrap();
        assert_eq!(ok.provider.as_deref(), Some("openai"));
        assert_eq!(ok.model.as_deref(), Some("gpt-5.1"));
    }

    #[test]
    fn coordinator_position_and_roster_membership() {
        use crate::roster::RosterEntry;
        let roster = [RosterEntry {
            position: 1,
            provider: "openai".into(),
            model: "gpt-5.1".into(),
            engine: "codex".into(),
            ..Default::default()
        }];
        // (D12) `#n` that names no seat here is NOT a refusal: recorded unresolved, run goes on.
        let m = parse("#5", Some(&roster)).unwrap();
        assert_eq!(m.unresolved.as_deref(), Some("#5"));
        assert_eq!(m.in_roster, Some(false));
        // `#n` with no roster at all is also unresolved (not a refusal).
        let m = parse("#1", None).unwrap();
        assert_eq!(m.unresolved.as_deref(), Some("#1"));
        // (D12) `#n` naming a seat resolves through the seated reviewer's lineage.
        let m = parse("#1", Some(&roster)).unwrap();
        assert_eq!(m.provider.as_deref(), Some("openai"));
        assert_eq!(m.model.as_deref(), Some("gpt-5.1"));
        assert_eq!(m.in_roster, Some(true));
        assert!(m.unresolved.is_none());
        // (D11) a coordinator no reviewer can match is said, not refused.
        let m = parse("anthropic :: opus", Some(&roster)).unwrap();
        assert_eq!(m.in_roster, Some(false));
        assert!(m.unresolved.is_none());
        // (F09-6) A bare provider label that the roster seats with ONE model: in_roster true, the
        // model of that entry and its engine (`Resolve-CoordinatorIdentity`).
        let m = parse("openai", Some(&roster)).unwrap();
        assert_eq!(m.in_roster, Some(true));
        assert_eq!(m.model.as_deref(), Some("gpt-5.1"));
        assert_eq!(m.engine.as_deref(), Some("codex"));
    }

    /// (F09-6) The plugin's resolution rules: a bare label takes its roster entries' one model (a
    /// model-less codex entry counting as the configured model), several models leave it unnamed, a
    /// label outside the roster takes the configured model only for the configured provider, `#n`
    /// of a model-less entry the configured model, a lineage without an engine its entry's engine
    /// (else codex), a Claude id's `[1m]` is stripped.
    #[test]
    fn coordinator_resolution_infers_models_like_the_plugin() {
        use crate::roster::RosterEntry;
        let e = |pos: usize, p: &str, m: &str, en: &str| RosterEntry {
            position: pos,
            provider: p.into(),
            model: m.into(),
            engine: en.into(),
            ..Default::default()
        };
        let roster = [
            e(1, "JudgeLabel-Kimi", "k3", "codex"),
            e(2, "ZAI", "glm-5.3", "codex"),
            e(3, "ZAI", "glm-5.1", "codex"),
            e(4, "openai", "", "codex"),
            e(5, "gem", "gemini-3-pro", "agy"),
        ];
        let d = CodexDefaults {
            provider: "openai".into(),
            model: "gpt-5.1".into(),
        };
        let r = |v: &str| resolve_coordinator_match(v, Some(&roster), &d).unwrap();
        // the bare label of a sole entry: its model and engine
        let m = r("JudgeLabel-Kimi");
        assert_eq!(
            (m.model.as_deref(), m.engine.as_deref(), m.in_roster),
            (Some("k3"), Some("codex"), Some(true))
        );
        // ambiguous: two models of one label - the model is not named, the engine not inferred
        let m = r("ZAI");
        assert_eq!((m.model.as_deref(), m.engine.as_deref()), (None, None));
        // a model-less codex entry: the configured model
        let m = r("openai");
        assert_eq!(m.model.as_deref(), Some("gpt-5.1"));
        let m = r("#4");
        assert_eq!(
            (
                m.provider.as_deref(),
                m.model.as_deref(),
                m.engine.as_deref()
            ),
            (Some("openai"), Some("gpt-5.1"), Some("codex"))
        );
        // another engine's entry keeps its own engine; the label's engine suffix filters
        let m = r("gem");
        assert_eq!(
            (m.model.as_deref(), m.engine.as_deref()),
            (Some("gemini-3-pro"), Some("agy"))
        );
        assert_eq!(r("gem [codex]").in_roster, Some(false));
        // a lineage without an engine: its roster entry's, else codex
        assert_eq!(r("gem :: gemini-3-pro").engine.as_deref(), Some("agy"));
        let m = r("anthropic :: claude-opus-5-5[1m]");
        assert_eq!(
            (m.model.as_deref(), m.engine.as_deref(), m.in_roster),
            (Some("claude-opus-5-5"), Some("codex"), Some(false))
        );
        // a label outside the roster: the configured model only for the configured provider
        let none = resolve_coordinator_match("openai", None, &d).unwrap();
        assert_eq!(
            (
                none.model.as_deref(),
                none.engine.as_deref(),
                none.in_roster
            ),
            (Some("gpt-5.1"), Some("codex"), None)
        );
        let other = resolve_coordinator_match("anthropic", None, &d).unwrap();
        assert_eq!(
            (other.model.as_deref(), other.engine.as_deref()),
            (None, None)
        );
        let agy = resolve_coordinator_match("openai [agy]", None, &d).unwrap();
        assert!(agy.model.is_none());
        let no_model =
            resolve_coordinator_match("openai", None, &CodexDefaults::default()).unwrap();
        assert!(no_model.model.is_none());
        // the full record: an empty value is the host hint alone, a bad one the plugin's refusal
        let rec = resolve_coordinator_identity("JudgeLabel-Kimi", Some(&roster), &d, "claude-code")
            .unwrap();
        assert_eq!(
            (
                rec.provider.as_deref(),
                rec.model.as_deref(),
                rec.source.as_str()
            ),
            (Some("JudgeLabel-Kimi"), Some("k3"), "explicit")
        );
        assert_eq!(
            resolve_coordinator_identity("  ", Some(&roster), &d, "unknown")
                .unwrap()
                .source,
            "none"
        );
        let bad = resolve_coordinator_identity("open::ai", None, &d, "unknown").unwrap_err();
        assert_eq!(
            bad,
            "CODEX_CONSULT_COORDINATOR='open::ai' cannot be used: the provider 'open::ai' must not contain '::' - give '<provider> :: <model>' (optionally ' [<engine>]'), a roster position '#<n>' or a provider label; nothing was started."
        );
    }

    /// (F14-2) The plugin's comparisons (`Test-ReviewerMatch`, `Resolve-CoordinatorIdentity`): the
    /// provider `-ceq` - a label whose case differs from the config's provider or a roster entry's
    /// is ANOTHER provider (no model inferred, not in the roster) -, the engine `-eq`.
    #[test]
    fn coordinator_provider_is_case_sensitive_the_engine_is_not() {
        use crate::roster::RosterEntry;
        let d = CodexDefaults {
            provider: "openai".into(),
            model: "gpt-6-astra".into(),
        };
        let m = resolve_coordinator_match("OpenAI", None, &d).unwrap();
        assert_eq!(
            (
                m.provider.as_deref(),
                m.model.as_deref(),
                m.engine.as_deref()
            ),
            (Some("OpenAI"), None, None)
        );
        let m = resolve_coordinator_match("openai", None, &d).unwrap();
        assert_eq!(
            (m.model.as_deref(), m.engine.as_deref()),
            (Some("gpt-6-astra"), Some("codex"))
        );
        // a roster entry: the label's case is the entry's or it is another provider; its engine
        // (written in another case) still matches a `[codex]` suffix and counts as codex
        let roster = [RosterEntry {
            position: 1,
            provider: "kimi".into(),
            model: String::new(),
            engine: "Codex".into(),
            ..Default::default()
        }];
        let m = resolve_coordinator_match("Kimi", Some(&roster), &d).unwrap();
        assert_eq!((m.model.as_deref(), m.in_roster), (None, Some(false)));
        let m = resolve_coordinator_match("kimi [codex]", Some(&roster), &d).unwrap();
        assert_eq!(
            (m.model.as_deref(), m.in_roster),
            (Some("gpt-6-astra"), Some(true))
        );
    }

    /// `Get-CodexConfigDefaults`: the top-level `model_provider` (else openai) and `model`.
    #[test]
    fn codex_defaults_from_the_config() {
        let c = crate::config::scan_config_text(
            "config.toml",
            "model = \"gpt-6\"\nmodel_provider = \"ZAI\"\n\n[model_providers.ZAI]\nbase_url = \"https://api.z.ai/api/v1\"\n",
        );
        assert_eq!(
            codex_config_defaults(&c),
            CodexDefaults {
                provider: "ZAI".into(),
                model: "gpt-6".into()
            }
        );
        let empty = crate::config::scan_config_text("config.toml", "");
        assert_eq!(codex_config_defaults(&empty), CodexDefaults::default());
        assert_eq!(
            codex_config_defaults(&crate::config::config_not_found("x")),
            CodexDefaults::default()
        );
    }

    #[test]
    fn coordinator_own_model_vs_own_provider_warning() {
        // Own model: provider + model equal → the strong warning.
        let full = build_coordinator(
            "claude-code",
            Some(&parse("openai :: gpt-5.1", None).unwrap()),
        );
        let w =
            coordinator_reviewer_warning(&full, "openai", "gpt-5.1", "codex", "openai :: gpt-5.1")
                .unwrap();
        assert!(w.contains("the coordinator's own model"));
        // Bare provider label → the weaker "own provider (model not named)" warning.
        let bare = build_coordinator("claude-code", Some(&parse("openai", None).unwrap()));
        let w =
            coordinator_reviewer_warning(&bare, "openai", "gpt-5.1", "codex", "openai :: gpt-5.1")
                .unwrap();
        assert!(w.contains("the coordinator's own provider (model not named)"));
        // A named-but-different model does not warn at all.
        assert!(
            coordinator_reviewer_warning(&full, "openai", "gpt-6", "codex", "openai :: gpt-6")
                .is_none()
        );
    }

    #[test]
    fn coordinator_text() {
        let c = build_coordinator(
            "codex",
            Some(&CoordinatorMatch {
                provider: Some("openai".into()),
                model: Some("gpt-5.1".into()),
                engine: None,
                ..Default::default()
            }),
        );
        assert_eq!(
            format_coordinator_text(&c),
            "openai :: gpt-5.1; host codex (inferred, a hint); source explicit"
        );
        let inferred = build_coordinator("zcode", None);
        assert_eq!(
            format_coordinator_text(&inferred),
            "(no identity given - CODEX_CONSULT_COORDINATOR is not set); host zcode (inferred, a hint); source inferred"
        );
        let none = build_coordinator("unknown", None);
        assert_eq!(none.source, "none");
        // (wave 5) the plugin's text: the engine only when it is not codex, a model-less label,
        // an unresolved `#n`, a coordinator no reviewer can match
        let codex = build_coordinator(
            "unknown",
            Some(&CoordinatorMatch {
                provider: Some("openai".into()),
                model: Some("gpt-5.1".into()),
                engine: Some("codex".into()),
                in_roster: Some(true),
                ..Default::default()
            }),
        );
        assert_eq!(
            format_coordinator_text(&codex),
            "openai :: gpt-5.1; host unknown (inferred, a hint); source explicit"
        );
        let agy = build_coordinator(
            "claude-code",
            Some(&CoordinatorMatch {
                provider: Some("google".into()),
                model: None,
                engine: Some("agy".into()),
                in_roster: Some(false),
                ..Default::default()
            }),
        );
        assert_eq!(
            format_coordinator_text(&agy),
            "google (model not named) [agy]; host claude-code (inferred, a hint); source explicit (not in the roster - no reviewer can match it)"
        );
        let un = build_coordinator(
            "unknown",
            Some(&CoordinatorMatch {
                unresolved: Some("#5".into()),
                in_roster: Some(false),
                ..Default::default()
            }),
        );
        assert_eq!(
            format_coordinator_text(&un),
            "#5 (names no roster position here); host unknown (inferred, a hint); source explicit"
        );
        assert_eq!(format_coordinator_id(&codex), "openai :: gpt-5.1");
        assert_eq!(format_coordinator_id(&agy), "google [agy]");
    }
}
