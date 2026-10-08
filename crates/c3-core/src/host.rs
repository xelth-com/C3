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

/// `ConvertFrom-ReviewerMatcher` for `CODEX_CONSULT_COORDINATOR`: a `#<n>` roster position, a
/// `<provider> :: <model>` lineage, or a bare provider label, either with an optional ` [<engine>]`
/// suffix. `roster` is `None` when there is no reviewer roster at all. `Err` carries the plugin's
/// exact `<why>` (the caller wraps it into the full refusal).
pub fn parse_coordinator_matcher(
    value: &str,
    roster: Option<&[RosterEntry]>,
) -> Result<CoordinatorMatch, String> {
    let t = value.trim();
    if t.is_empty() {
        return Err("an empty value".to_string());
    }
    // (wave 27c, D10) the grammar of a reviewer matcher — `#<n>`, a bare provider label, or
    // `<provider> :: <model>`, any with an optional ` [<engine>]` — is parsed by ONE function that
    // the roster validator shares (`c3_core::roster::parse_reviewer_matcher_grammar`); what the
    // roster accepts as a provider/model/engine string, the coordinator value accepts.
    let g = crate::roster::parse_reviewer_matcher_grammar(t)?;
    if let Some(pos) = g.position {
        // (wave 27c, D12) a `#<n>` resolves through the same code as a seated reviewer: the entry's
        // model, else the default the bridge would run. Naming no seat here is NOT a refusal.
        return match roster {
            None => Ok(CoordinatorMatch {
                unresolved: Some(format!("#{pos}")),
                ..Default::default()
            }),
            Some(entries) => match entries.iter().find(|e| e.position as i64 == pos) {
                Some(e) => Ok(CoordinatorMatch {
                    provider: Some(e.provider.clone()),
                    model: (!e.model.is_empty()).then(|| e.model.clone()),
                    engine: (!e.engine.is_empty()).then(|| e.engine.clone()),
                    in_roster: Some(true),
                    unresolved: None,
                }),
                None => Ok(CoordinatorMatch {
                    unresolved: Some(format!("#{pos}")),
                    in_roster: Some(false),
                    ..Default::default()
                }),
            },
        };
    }
    let provider = g.provider.unwrap_or_default();
    // (wave 27c, D11) a coordinator whose provider (and model, when named) matches no roster entry
    // is SAID (`in_roster: false`), not refused; a run with no roster leaves `in_roster` unknown.
    let in_roster = roster.map(|entries| {
        entries.iter().any(|e| {
            e.provider == provider
                && g.model.as_ref().is_none_or(|m| &e.model == m)
                && g.engine.as_ref().is_none_or(|en| &e.engine == en)
        })
    });
    Ok(CoordinatorMatch {
        provider: Some(provider),
        model: g.model,
        engine: g.engine,
        in_roster,
        unresolved: None,
    })
}

/// Wrap a `parse_coordinator_matcher` `<why>` into the plugin's full refusal (`Stop-WithError`
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

/// The coordinator's lineage as a display string (`Format-CoordinatorText`'s identity half):
/// `<provider> :: <model>` (+ ` [<engine>]`), or `<provider> (every model of it)`, or the
/// no-identity note.
fn coordinator_lineage(c: &Coordinator) -> String {
    match &c.provider {
        None => "(no identity given - CODEX_CONSULT_COORDINATOR is not set)".to_string(),
        Some(p) => {
            let mut s = match &c.model {
                Some(m) => format!("{p} :: {m}"),
                None => format!("{p} (every model of it)"),
            };
            if let Some(e) = &c.engine {
                s.push_str(&format!(" [{e}]"));
            }
            s
        }
    }
}

/// `Format-CoordinatorText`: the dry-run `coordinator :` value —
/// `<lineage>; host <host> (inferred, a hint); source <source>`.
pub fn format_coordinator_text(c: &Coordinator) -> String {
    format!(
        "{}; host {} (inferred, a hint); source {}",
        coordinator_lineage(c),
        c.host,
        c.source
    )
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
        assert_eq!(
            parse_coordinator_matcher("open ai", None).unwrap_err(),
            "the provider 'open ai' is not a provider label (letters, digits, dot, dash, underscore)"
        );
        assert_eq!(
            parse_coordinator_matcher("openai :: gpt 5", None).unwrap_err(),
            "the model 'gpt 5' contains white space"
        );
        assert_eq!(
            parse_coordinator_matcher("openai :: gpt-5.1 [bad]", None).unwrap_err(),
            "'openai :: gpt-5.1 [bad]' names the engine 'bad' (known: codex, agy, muse)"
        );
        let ok = parse_coordinator_matcher("openai :: gpt-5.1", None).unwrap();
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
        let m = parse_coordinator_matcher("#5", Some(&roster)).unwrap();
        assert_eq!(m.unresolved.as_deref(), Some("#5"));
        assert_eq!(m.in_roster, Some(false));
        // `#n` with no roster at all is also unresolved (not a refusal).
        let m = parse_coordinator_matcher("#1", None).unwrap();
        assert_eq!(m.unresolved.as_deref(), Some("#1"));
        // (D12) `#n` naming a seat resolves through the seated reviewer's lineage.
        let m = parse_coordinator_matcher("#1", Some(&roster)).unwrap();
        assert_eq!(m.provider.as_deref(), Some("openai"));
        assert_eq!(m.model.as_deref(), Some("gpt-5.1"));
        assert_eq!(m.in_roster, Some(true));
        assert!(m.unresolved.is_none());
        // (D11) a coordinator no reviewer can match is said, not refused.
        let m = parse_coordinator_matcher("anthropic :: opus", Some(&roster)).unwrap();
        assert_eq!(m.in_roster, Some(false));
        assert!(m.unresolved.is_none());
        // A bare provider label that the roster seats: in_roster true, model not named.
        let m = parse_coordinator_matcher("openai", Some(&roster)).unwrap();
        assert_eq!(m.in_roster, Some(true));
        assert!(m.model.is_none());
    }

    #[test]
    fn coordinator_own_model_vs_own_provider_warning() {
        // Own model: provider + model equal → the strong warning.
        let full = build_coordinator(
            "claude-code",
            Some(&parse_coordinator_matcher("openai :: gpt-5.1", None).unwrap()),
        );
        let w =
            coordinator_reviewer_warning(&full, "openai", "gpt-5.1", "codex", "openai :: gpt-5.1")
                .unwrap();
        assert!(w.contains("the coordinator's own model"));
        // Bare provider label → the weaker "own provider (model not named)" warning.
        let bare = build_coordinator(
            "claude-code",
            Some(&parse_coordinator_matcher("openai", None).unwrap()),
        );
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
    }
}
