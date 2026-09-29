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
use crate::lineage::ENGINE_NAMES;
use crate::roster::RosterEntry;

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
    "ZCODE_SESSION_ID",
    "ZCODE_PROJECT_DIR",
];

/// The wildcard marker prefixes (`$script:HostMarkerPrefixes`): every `CODEX_SANDBOX*` and every
/// `ZCODE_PLUGIN*` (`ZCODE_PLUGIN_ROOT`/`ZCODE_PLUGIN_DATA` included).
pub const HOST_MARKER_PREFIXES: &[&str] = &["CODEX_SANDBOX", "ZCODE_PLUGIN"];

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
    let pos_re = regex::Regex::new(r"^#(\d+)$").unwrap();
    if let Some(c) = pos_re.captures(t) {
        let pos: i64 = c[1].parse().unwrap_or(0);
        return match roster {
            None => Err(format!(
                "'{t}' names no roster position (there is no reviewer roster)"
            )),
            Some(entries) => match entries.iter().find(|e| e.position as i64 == pos) {
                Some(e) => Ok(CoordinatorMatch {
                    provider: Some(e.provider.clone()),
                    model: if e.model.is_empty() {
                        None
                    } else {
                        Some(e.model.clone())
                    },
                    engine: if e.engine.is_empty() {
                        None
                    } else {
                        Some(e.engine.clone())
                    },
                }),
                None => Err(format!(
                    "'{t}' names no roster position (the roster has {} entries)",
                    entries.len()
                )),
            },
        };
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
    let label_re = regex::Regex::new(r"^[A-Za-z0-9._-]+$").unwrap();
    if !label_re.is_match(&provider) {
        return Err(format!(
            "the provider '{provider}' is not a provider label (letters, digits, dot, dash, underscore)"
        ));
    }
    if let Some(m) = &model {
        if m.chars().any(|c| c.is_whitespace()) {
            return Err(format!("the model '{m}' contains white space"));
        }
    }
    Ok(CoordinatorMatch {
        provider: Some(provider),
        model,
        engine,
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
    if let Some(m) = &c.model {
        if m != reviewer_model {
            return None;
        }
    }
    if let Some(e) = &c.engine {
        if e != reviewer_engine {
            return None;
        }
    }
    // The warning names the REVIEWER's lineage (the model actually being consulted), which the
    // coordinator matched — not the coordinator's own (possibly bare-label) spelling.
    Some(format!(
        "coordinator: {reviewer_lineage} is the coordinator's own model (CODEX_CONSULT_COORDINATOR) - a second opinion from the coordinator's own model, not an independent one"
    ))
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
        assert!(!is_host_marker("CLAUDE_CODE_USE_BEDROCK"));
        assert!(!is_host_marker("CLAUDE_PLUGIN_ROOT"));
        assert!(!is_host_marker("CODEX_HOME"));
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
            parse_coordinator_matcher("#5", Some(&[])).unwrap_err(),
            "'#5' names no roster position (the roster has 0 entries)"
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
    fn coordinator_text() {
        let c = build_coordinator(
            "codex",
            Some(&CoordinatorMatch {
                provider: Some("openai".into()),
                model: Some("gpt-5.1".into()),
                engine: None,
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
