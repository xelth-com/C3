//! The closed telemetry classes (0.6.1 parity): the vendor table, the closed model lists, the
//! coordinator ("judge") classifier of a rating event and the `consult_ref` of a ledger entry.
//!
//! A port of the plugin's `$script:TelemetryVendors`, `Get-TelemetryVendorByHost`,
//! `Get-TelemetryModelToken`, `Get-TelemetryJudgeClass`, `Find-TelemetryRosterEntry`,
//! `Get-TelemetryRosterVendor`, `Test-TelemetryCoordinatorNamed`, `Get-TelemetryRatingActor`,
//! `Resolve-TelemetryJudge`, `ConvertTo-TelemetryJudge` and `Get-TelemetryConsultRef`
//! (`codex-consult-common.ps1` at v0.6.1). Privacy by construction: every value these functions
//! return is the TABLE's own text (a vendor class, an entry of that class's model list) or one of
//! the fixed words `other` / `unknown` - never a roster label, a host, or a model as the operator
//! typed it. The ledger and every local file keep the real ones.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use c3_core::config::CodexConfig;
use c3_core::ledger::{Coordinator, LedgerEntry};
use c3_core::roster::Roster;

/// One row of the vendor table: the class, the endpoint hosts that belong to it (the host equals
/// one or ends with `.` + one), the engine whose runs belong to it (agy, muse, claude), the Codex
/// built-in provider it stands for, and the CLOSED list of the published model names it may send.
#[derive(Debug)]
pub(crate) struct Vendor {
    pub class: &'static str,
    pub hosts: &'static [&'static str],
    pub engine: &'static str,
    pub builtin: &'static str,
    pub models: &'static [&'static str],
}

/// The `claude` engine's model table (`$script:ClaudeModels`) - the `anthropic` class's list.
pub(crate) const CLAUDE_MODELS: [&str; 16] = [
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

/// THE vendor table, in the plugin's order and with its exact names and lists.
pub(crate) static VENDORS: [Vendor; 10] = [
    Vendor {
        class: "openai",
        hosts: &["openai.com", "chatgpt.com"],
        engine: "",
        builtin: "openai",
        models: &["gpt-5.1", "gpt-6-astra", "o4-mini"],
    },
    Vendor {
        class: "zai",
        hosts: &["z.ai", "bigmodel.cn"],
        engine: "",
        builtin: "",
        models: &[
            "glm-5.3",
            "glm-5.3-flash",
            "glm-5.3-flashx",
            "glm-5.2",
            "glm-5.1",
            "glm-5",
            "glm-5-turbo",
            "glm-4.7",
            "glm-4.6",
            "glm-4.5",
            "glm-4.5-air",
        ],
    },
    Vendor {
        class: "xiaomi",
        hosts: &["xiaomimimo.com"],
        engine: "",
        builtin: "",
        models: &[
            "mimo-v2.6-pro",
            "mimo-v2.6-flash",
            "mimo-v2.6-pro-ultraspeed",
            "mimo-v2.5-pro",
            "mimo-v2.5",
        ],
    },
    Vendor {
        class: "byteplus",
        hosts: &["bytepluses.com"],
        engine: "",
        builtin: "",
        models: &[
            "dola-seed-2.0-pro",
            "dola-seed-2.0-lite",
            "dola-seed-2.0-code",
            "bytedance-seed-code",
            "glm-5.3-flash",
            "glm-5.2",
            "glm-5.1",
            "kimi-k2.5",
            "gpt-oss-120b",
            "deepseek-v4.1-flash",
            "deepseek-v4-flash",
            "deepseek-v4-pro",
        ],
    },
    Vendor {
        class: "moonshot",
        hosts: &["kimi.ai", "moonshot.ai"],
        engine: "",
        builtin: "",
        models: &[
            "k3",
            "k3-256k",
            "kimi-for-coding",
            "kimi-for-coding-highspeed",
            "kimi-k2.5",
            "kimi-k3",
        ],
    },
    Vendor {
        class: "alibaba",
        hosts: &["aliyuncs.com"],
        engine: "",
        builtin: "",
        models: &[
            "qwen3.8-max",
            "qwen3.8-flash",
            "qwen3.7-max",
            "qwen3.7-plus",
            "qwen3.6-flash",
            "deepseek-v4.1-flash",
            "deepseek-v4-pro",
            "deepseek-v4-pro-0813",
            "deepseek-v4-flash-0731",
            "glm-5.3",
            "glm-5.2",
        ],
    },
    Vendor {
        class: "minimax",
        hosts: &["api.minimax.io", "api.minimax.cn"],
        engine: "",
        builtin: "",
        models: &["minimax-m3"],
    },
    Vendor {
        class: "google",
        hosts: &[],
        engine: "agy",
        builtin: "",
        models: &[
            "gemini-3.8-flash-high",
            "gemini-3.8-flash-medium",
            "gemini-3.8-flash-low",
            "gemini-3.1-pro-high",
            "gemini-3.1-pro-low",
        ],
    },
    Vendor {
        class: "meta",
        hosts: &[],
        engine: "muse",
        builtin: "",
        models: &["muse-spark-1.3", "muse-spark-1.3-contributor"],
    },
    Vendor {
        class: "anthropic",
        hosts: &[],
        engine: "claude",
        builtin: "",
        models: &CLAUDE_MODELS,
    },
];

/// (C3 extension, wave 2c) The `http` engine's OpenRouter route: the class `openrouter` for the
/// host `openrouter.ai` (and its subdomains). OpenRouter resells other labs' models under
/// `<vendor>/<model>` ids, so its model token is that form - sent only when the vendor part is an
/// OpenRouter vendor slug of [`OPENROUTER_VENDORS`] AND the model part EQUALS an entry of that
/// class's closed list (after lower-casing, `[1m]` stripped), else `other`
/// ([`openrouter_model_token`]). Not a row of the plugin's table: the plugin has no http engine.
pub(crate) static OPENROUTER: Vendor = Vendor {
    class: "openrouter",
    hosts: &["openrouter.ai"],
    engine: "",
    builtin: "",
    models: &[],
};

/// (C3 extension) OpenRouter's vendor slugs and the class of the table whose closed model list
/// their model part is checked against.
pub(crate) const OPENROUTER_VENDORS: [(&str, &str); 9] = [
    ("openai", "openai"),
    ("anthropic", "anthropic"),
    ("google", "google"),
    ("z-ai", "zai"),
    ("moonshotai", "moonshot"),
    ("minimax", "minimax"),
    ("xiaomi", "xiaomi"),
    ("qwen", "alibaba"),
    ("meta-llama", "meta"),
];

/// (C3 extension) The model token of an OpenRouter reviewer: `<slug>/<listed model>` when the
/// vendor slug is one of [`OPENROUTER_VENDORS`] and the model part EQUALS an entry of that
/// class's closed list (the table's own text), else `other`; `unknown` for no model.
pub(crate) fn openrouter_model_token(model: &str) -> String {
    let m = model.trim().to_lowercase();
    if m.is_empty() {
        return "unknown".to_string();
    }
    let m = m.strip_suffix("[1m]").unwrap_or(&m);
    let Some((slug, rest)) = m.split_once('/') else {
        return "other".to_string();
    };
    let Some((slug, class)) = OPENROUTER_VENDORS.iter().find(|(s, _)| *s == slug) else {
        return "other".to_string();
    };
    match model_token(vendor_of_class(class), rest).as_str() {
        "other" | "unknown" => "other".to_string(),
        listed => format!("{slug}/{listed}"),
    }
}

/// The row of a class name (exact) - the plugin's table, plus C3's `openrouter`.
pub(crate) fn vendor_of_class(class: &str) -> Option<&'static Vendor> {
    VENDORS
        .iter()
        .find(|v| v.class == class)
        .or_else(|| (class == OPENROUTER.class).then_some(&OPENROUTER))
}

/// The row of an engine (agy google, muse meta, claude anthropic); `None` for codex, http and
/// anything else.
pub(crate) fn vendor_of_engine(engine: &str) -> Option<&'static Vendor> {
    if engine.is_empty() || engine == "codex" {
        return None;
    }
    VENDORS
        .iter()
        .find(|v| !v.engine.is_empty() && v.engine == engine)
}

/// `Get-TelemetryVendorByHost`: the row of a base URL's HOST (the host equals one of a row's hosts
/// or ends with `.` + one of them); `None` for any other host, an IPv6 literal or no host.
pub(crate) fn vendor_by_host(base_url: &str) -> Option<&'static Vendor> {
    let (_, host) = c3_core::config::canonical_base_url(base_url);
    let host = host.trim_end_matches('.');
    if host.is_empty() || host.starts_with('[') {
        return None;
    }
    VENDORS
        .iter()
        .chain(std::iter::once(&OPENROUTER))
        .find(|v| {
            v.hosts
                .iter()
                .any(|h| host == *h || host.ends_with(&format!(".{h}")))
        })
}

/// `Get-TelemetryModelToken`: the entry of `vendor`'s closed list the model EQUALS after
/// lower-casing both (and stripping a trailing `[1m]`), as the TABLE spells it; `other` for a
/// model outside the list or without a vendor; `unknown` for no model at all.
pub(crate) fn model_token(vendor: Option<&Vendor>, model: &str) -> String {
    let m = model.trim().to_lowercase();
    if m.is_empty() {
        return "unknown".to_string();
    }
    let Some(v) = vendor else {
        return "other".to_string();
    };
    if v.class == OPENROUTER.class {
        return openrouter_model_token(&m);
    }
    let m = m.strip_suffix("[1m]").unwrap_or(&m);
    v.models
        .iter()
        .find(|known| known.to_lowercase() == m)
        .map(|k| k.to_string())
        .unwrap_or_else(|| "other".to_string())
}

/// `Get-TelemetryVendor` on a reviewer-shaped record: the class of `provider_config.base_url`'s
/// host when the record names one (every engine - the host first), else the engine's row, else
/// (codex) the built-in provider's row (`provider_config.builtin`); `None` (other) otherwise.
pub(crate) fn vendor_of_reviewer(engine: &str, provider_config: &Value) -> Option<&'static Vendor> {
    let engine = if engine.is_empty() { "codex" } else { engine };
    let base_url = provider_config
        .get("base_url")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !base_url.is_empty() {
        return vendor_by_host(base_url);
    }
    if engine != "codex" {
        return vendor_of_engine(engine);
    }
    let builtin = provider_config
        .get("builtin")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if builtin.is_empty() {
        return None;
    }
    VENDORS
        .iter()
        .find(|v| !v.builtin.is_empty() && v.builtin == builtin)
}

// --------------------------------------------------------------------------- the reviewer

/// The engines an event may name (`$script:EngineNames` plus C3's `http`); anything else `other`.
pub(crate) const ENGINES: [&str; 5] = ["codex", "agy", "muse", "claude", "http"];

/// `$script:ConsultPurposes`: the purposes an event may name; anything else `other`, none `none`.
pub(crate) const PURPOSES: [&str; 8] = [
    "framing",
    "decision",
    "checkpoint",
    "core-contract",
    "acceptance",
    "diff-review",
    "stuck",
    "chore",
];

/// `$script:TelemetryFailureClasses`: the failure classes an outcome may name.
pub(crate) const FAILURE_CLASSES: [&str; 10] = [
    "auth",
    "quota",
    "capability",
    "transport",
    "permission",
    "operator",
    "unknown",
    "timeout",
    "stalled",
    "bridge",
];

/// The reviewer of a ledger entry as an event carries it (`Get-TelemetryReviewerClass`) - ONE code
/// path for the consultation and the rating event: `(engine, provider, model)` - the engine (a
/// name of [`ENGINES`], else other), the vendor CLASS of the endpoint (`provider_config.base_url`'s
/// host, else the engine's row, else codex's built-in provider; else other), the model through
/// that class's closed list (`other` outside it, `unknown` without a model). The roster label and
/// the model as typed are never read into the event.
pub fn reviewer_class(entry: &LedgerEntry) -> (String, String, String) {
    let rev = &entry.reviewer;
    let engine = if rev.engine.trim().is_empty() {
        "codex"
    } else {
        rev.engine.trim()
    };
    let engine = if ENGINES.contains(&engine) {
        engine
    } else {
        "other"
    };
    let vendor = if engine == "other" {
        None
    } else {
        vendor_of_reviewer(engine, &rev.provider_config)
    };
    let model_raw = if rev.model.trim().is_empty() {
        entry.model.as_str()
    } else {
        rev.model.as_str()
    };
    (
        engine.to_string(),
        vendor.map(|v| v.class).unwrap_or("other").to_string(),
        model_token(vendor, model_raw),
    )
}

/// `Get-TelemetryPurpose`: one of [`PURPOSES`], `none` without one, else `other`.
pub fn purpose_class(purpose: &str) -> String {
    let p = purpose.trim();
    if p.is_empty() {
        "none".into()
    } else if PURPOSES.contains(&p) {
        p.into()
    } else {
        "other".into()
    }
}

/// `Get-TelemetryOutcome`: `(outcome, severity)` - `usable` (`usable-after-continuation` after a
/// timeout continuation) with `info`; else `failed:<class>` - the provider failure's class, else
/// from the bridge outcome (`timeout`, `stalled`, `operator` - the operator's kick -, else
/// `bridge`), a class outside [`FAILURE_CLASSES`] `unknown` - with `warning` for auth, quota and
/// operator and `error` for every other class.
pub fn outcome_class(entry: &LedgerEntry) -> (String, &'static str) {
    let bo = entry.bridge_outcome.as_str();
    if c3_core::health::is_usable_outcome(bo) {
        let o = if bo.contains("after a timeout continuation") {
            "usable-after-continuation"
        } else {
            "usable"
        };
        return (o.into(), "info");
    }
    let mut class = entry
        .provider_failure
        .as_ref()
        .map(|p| p.class.trim().to_string())
        .unwrap_or_default();
    if class.is_empty() {
        class = if bo.starts_with("failed: timeout") {
            "timeout"
        } else if bo.starts_with("failed: stalled") {
            "stalled"
        } else if bo.contains("stopped by the operator") {
            "operator"
        } else {
            "bridge"
        }
        .into();
    }
    if !FAILURE_CLASSES.contains(&class.as_str()) {
        class = "unknown".into();
    }
    let sev = if ["auth", "quota", "operator"].contains(&class.as_str()) {
        "warning"
    } else {
        "error"
    };
    (format!("failed:{class}"), sev)
}

/// A verdict as a complaint's last-run summary may carry it: one of the reply schema's verdicts,
/// `none` without one, else `other`.
pub(crate) fn verdict_class(verdict: &str) -> String {
    let v = verdict.trim();
    if v.is_empty() {
        "none".into()
    } else if ["ACCEPT", "HOLD", "REJECT", "ADVISE"].contains(&v) {
        v.into()
    } else {
        "other".into()
    }
}

// --------------------------------------------------------------------------- consult_ref

/// `Get-TelemetryConsultRef`: the entry's `consult_ref`, lower-cased, only when it has the shape of
/// a guid; `None` for an entry without one (recorded before 0.6.1) or with anything else.
pub fn consult_ref_of(entry: &LedgerEntry) -> Option<String> {
    let v = entry.consult_ref.as_deref()?.trim().to_ascii_lowercase();
    is_guid(&v).then_some(v)
}

/// `^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$` (lower case).
pub(crate) fn is_guid(v: &str) -> bool {
    let parts: Vec<&str> = v.split('-').collect();
    parts.len() == 5
        && [8, 4, 4, 4, 12]
            .iter()
            .zip(&parts)
            .all(|(n, p)| p.len() == *n && p.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')))
}

// --------------------------------------------------------------------------- the judge

/// Where a rating event's judge came from (`$script:TelemetryJudgeSources`).
pub const JUDGE_SOURCES: [&str; 3] = ["rating_actor", "consult_coordinator", "unknown"];

/// The `judge` of a rating event and of a saved mark: `{provider, model, source}` - classes only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Judge {
    pub provider: String,
    pub model: String,
    pub source: String,
}

impl Judge {
    /// `{other, other, unknown}` - no judge is known.
    pub fn unknown() -> Judge {
        Judge {
            provider: "other".into(),
            model: "other".into(),
            source: "unknown".into(),
        }
    }

    /// The judge as a JSON object `{provider, model, source}` (the mark's and the event's shape).
    pub fn to_value(&self) -> Value {
        serde_json::json!({
            "provider": self.provider,
            "model": self.model,
            "source": self.source,
        })
    }
}

/// A coordinator record's identity fields as the classifier reads them.
#[derive(Debug, Clone, Default)]
pub(crate) struct CoordinatorView {
    pub provider: String,
    pub model: String,
    pub engine: String,
    pub host: String,
}

impl CoordinatorView {
    pub(crate) fn of(c: &Coordinator) -> CoordinatorView {
        CoordinatorView {
            provider: c.provider.clone().unwrap_or_default(),
            model: c.model.clone().unwrap_or_default(),
            engine: c.engine.clone().unwrap_or_default(),
            host: c.host.clone(),
        }
    }
}

/// `Find-TelemetryRosterEntry`: the roster entry a coordinator's provider LABEL names (ordinal);
/// of several, the one of the identity's engine, then the one of its model. Returns the entry's
/// index into `roster.entries`.
fn find_roster_entry(roster: &Roster, provider: &str, model: &str, engine: &str) -> Option<usize> {
    if !roster.exists {
        return None;
    }
    let mut hits: Vec<usize> = roster
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.provider == provider)
        .map(|(i, _)| i)
        .collect();
    if hits.is_empty() {
        return None;
    }
    if !engine.is_empty() {
        let by_engine: Vec<usize> = hits
            .iter()
            .copied()
            .filter(|i| {
                let e = &roster.entries[*i].engine;
                (if e.is_empty() { "codex" } else { e.as_str() }) == engine
            })
            .collect();
        if !by_engine.is_empty() {
            hits = by_engine;
        }
    }
    if !model.is_empty() {
        if let Some(i) = hits
            .iter()
            .copied()
            .find(|i| roster.entries[*i].model == model)
        {
            return Some(i);
        }
    }
    hits.first().copied()
}

/// `Get-TelemetryRosterVendor`: the vendor row of a roster ENTRY's endpoint - a codex entry the
/// base_url of its `[model_providers.<label>]` table (the label `openai` without a table: the
/// built-in provider), another engine its endpoint's base URL (C3: an `http` reviewer's
/// `base_url`), else that engine's row. `None` (other) for an unknown host or a label without a
/// table. `config`: the Codex config (read here when a codex entry needs it and none is given).
fn roster_vendor(
    roster: &Roster,
    index: usize,
    config: Option<&CodexConfig>,
) -> Option<&'static Vendor> {
    let entry = &roster.entries[index];
    let engine = if entry.engine.is_empty() {
        "codex"
    } else {
        entry.engine.as_str()
    };
    if engine != "codex" {
        let base_url = roster
            .http_reviewers
            .iter()
            .find(|h| h.provider == entry.provider && h.model == entry.model)
            .map(|h| h.base_url.clone())
            .unwrap_or_default();
        let pc = if base_url.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::json!({ "base_url": base_url })
        };
        return vendor_of_reviewer(engine, &pc);
    }
    let owned;
    let config = match config {
        Some(c) => c,
        None => {
            owned = crate::providers::read_codex_config(&crate::providers::get_codex_config_path());
            &owned
        }
    };
    let id = c3_core::lineage::resolve_reviewer_identity(
        config,
        &entry.provider,
        "unknown",
        "",
        "codex",
        "",
    );
    vendor_of_reviewer("codex", &id.provider_config)
}

/// `Get-TelemetryJudgeClass`: THE classifier of a COORDINATOR identity (its own code path - a
/// coordinator record has no endpoint). The provider NAME decides: `openai` (any case) -> openai;
/// `anthropic` (any case) -> anthropic; a LABEL of the reviewer roster -> the class of that entry's
/// endpoint; any other name -> the row of the engine the identity names, else other. Without a
/// provider: the claude engine or the host `claude-code` -> anthropic, else other (the host is a
/// hint, read only then). The model through the same closed lists (`other` outside its vendor's
/// list, without a model or without a vendor). Returns `(provider class, model)`.
pub(crate) fn judge_class(
    c: &CoordinatorView,
    roster: Option<&Roster>,
    config: Option<&CodexConfig>,
) -> (String, String) {
    let p = c.provider.trim();
    let m = c.model.trim();
    let vendor = if !p.is_empty() {
        if p.eq_ignore_ascii_case("openai") {
            vendor_of_class("openai")
        } else if p.eq_ignore_ascii_case("anthropic") {
            vendor_of_class("anthropic")
        } else {
            match roster.and_then(|r| find_roster_entry(r, p, m, &c.engine).map(|i| (r, i))) {
                Some((r, i)) => roster_vendor(r, i, config),
                None => vendor_of_engine(&c.engine),
            }
        }
    } else if c.engine == "claude" || c.host == "claude-code" {
        vendor_of_class("anthropic")
    } else {
        None
    };
    let mut model = "other".to_string();
    if vendor.is_some() && !m.is_empty() {
        model = model_token(vendor, m);
        if model == "unknown" {
            model = "other".into();
        }
    }
    (
        vendor.map(|v| v.class).unwrap_or("other").to_string(),
        model,
    )
}

/// `Test-TelemetryCoordinatorNamed`: whether a ledger entry's `coordinator` names anyone - a
/// provider, an unresolved `#n` or a host hint other than `unknown`. A record that names nothing
/// is an ABSENT judge (unknown), not an unrecognised one (other).
pub(crate) fn coordinator_named(c: Option<&Coordinator>) -> bool {
    let Some(c) = c else { return false };
    c.provider.as_deref().is_some_and(|p| !p.is_empty())
        || c.unresolved.as_deref().is_some_and(|u| !u.is_empty())
        || (!c.host.is_empty() && c.host != "unknown")
}

/// `Get-TelemetryRatingActor`: the judge of a mark given NOW - `CODEX_CONSULT_COORDINATOR` of THIS
/// process, resolved exactly as the bridge resolves it for the ledger's coordinator record (with the
/// reviewer roster and the Codex config's defaults - `c3_core::host::resolve_coordinator_identity`;
/// the host hint of this process), classified by [`judge_class`] - source
/// `rating_actor`. A value that does not parse names an actor no class fits: `other / other`.
/// `None` when the variable is unset or empty (the consultation's own coordinator decides then).
pub fn rating_actor() -> Option<Judge> {
    let v = std::env::var("CODEX_CONSULT_COORDINATOR").unwrap_or_default();
    let v = v.trim();
    if v.is_empty() {
        return None;
    }
    let none = Judge {
        provider: "other".into(),
        model: "other".into(),
        source: "rating_actor".into(),
    };
    let roster = crate::providers::read_reviewer_roster().ok();
    let entries = roster.as_ref().filter(|r| r.exists).map(|r| &r.entries[..]);
    let config = crate::providers::read_codex_config(&crate::providers::get_codex_config_path());
    // (F09-6) the bridge's own resolver - a bare label takes its roster entries' one model, a
    // model-less entry the configured model - never a second, thinner parse
    let Ok(record) = c3_core::host::resolve_coordinator_identity(
        v,
        entries,
        &c3_core::host::codex_config_defaults(&config),
        c3_core::host::coordinator_host(),
    ) else {
        return Some(none);
    };
    let (provider, model) = judge_class(
        &CoordinatorView::of(&record),
        roster.as_ref(),
        Some(&config),
    );
    Some(Judge {
        provider,
        model,
        source: "rating_actor".into(),
    })
}

/// `Resolve-TelemetryJudge`: the judge of a rating event, resolved AT RATING TIME - `actor` when
/// given ([`rating_actor`]), else the ledger entry's own `coordinator` when it names anyone (source
/// `consult_coordinator`), else `{other, other, unknown}`. `roster`: the reviewer roster (read here
/// when a label needs it and none is given).
pub fn resolve_judge(entry: &LedgerEntry, actor: Option<&Judge>, roster: Option<&Roster>) -> Judge {
    if let Some(a) = actor {
        return a.clone();
    }
    let c = entry.coordinator.as_ref();
    if !coordinator_named(c) {
        return Judge::unknown();
    }
    let view = CoordinatorView::of(c.expect("named implies present"));
    let owned;
    let roster = match roster {
        Some(r) => Some(r),
        None => {
            let p = view.provider.as_str();
            if !p.is_empty()
                && !p.eq_ignore_ascii_case("openai")
                && !p.eq_ignore_ascii_case("anthropic")
            {
                owned = crate::providers::read_reviewer_roster().ok();
                owned.as_ref()
            } else {
                None
            }
        }
    };
    let (provider, model) = judge_class(&view, roster, None);
    Judge {
        provider,
        model,
        source: "consult_coordinator".into(),
    }
}

/// `ConvertTo-TelemetryJudge`: the `judge` object through its own allowlist - `source` one of
/// [`JUDGE_SOURCES`] (else unknown), `provider` a class of the table (else other), `model` an
/// entry of THAT class's list exactly (else other); an unknown source is always other / other.
pub fn closed_judge(provider: &str, model: &str, source: &str) -> Judge {
    let source = if JUDGE_SOURCES.contains(&source) {
        source
    } else {
        "unknown"
    };
    let vendor = if source != "unknown" && !provider.is_empty() {
        vendor_of_class(provider)
    } else {
        None
    };
    let model = match vendor {
        Some(v) if !model.is_empty() && v.models.contains(&model) => model,
        Some(v) if v.class == OPENROUTER.class && openrouter_model_token(model) == model => model,
        _ => "other",
    };
    Judge {
        provider: vendor.map(|v| v.class).unwrap_or("other").to_string(),
        model: model.to_string(),
        source: source.to_string(),
    }
}

/// [`closed_judge`] of a [`Judge`].
pub fn close_judge(j: &Judge) -> Judge {
    closed_judge(&j.provider, &j.model, &j.source)
}

/// `Get-RatingMarkJudge`: the judge a mark saved (read back through [`closed_judge`]), or `None`
/// for a mark without one (an object with a `source` key) - the caller then takes the
/// consultation's coordinator.
pub fn mark_judge(judge: Option<&Value>) -> Option<Judge> {
    let j = judge?;
    let o = j.as_object()?;
    if !o.contains_key("source") {
        return None;
    }
    let s = |k: &str| o.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    Some(closed_judge(&s("provider"), &s("model"), &s("source")))
}

// --------------------------------------------------------------------------- a queued event (F09-5)

/// The `os` values an event may carry: the three the client names, or this build's own label.
fn closed_os(os: &str) -> String {
    let own = crate::telemetry::event::os_label();
    if ["Windows", "Linux", "macOS"].contains(&os) || os == own {
        os.to_string()
    } else {
        "other".to_string()
    }
}

/// A version of this client as an event carries it (`app_version`, `bridge_version`): three
/// numbers and an optional pre-release tag of letters, digits, dots and dashes.
fn is_version(v: &str) -> bool {
    let (core, pre) = match v.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (v, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 6 && p.chars().all(|c| c.is_ascii_digit()))
        && pre.is_none_or(|p| {
            !p.is_empty()
                && p.len() <= 32
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        })
}

/// An instance id of this client's shape: the 64 lower-case hex digits of a SHA-256.
fn is_instance_id(v: &str) -> bool {
    v.len() == 64 && v.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
}

/// (F09-5) The body a QUEUED event is sent with: the event re-built field by field through the
/// closed classes, in the constructors' key order - the allowlist of
/// [`crate::telemetry::Event`] (a consultation) or [`crate::telemetry::RatingEvent`] (a rating),
/// every string a closed value: the engine one of [`ENGINES`]; the provider a CLASS of the table
/// (a roster label - `customer-acme` - is `other`); the model an entry of that class's list (else
/// `other`, `unknown` kept); the purpose, the outcome, the mark, the judge, `os` and the versions
/// through their own closed sets; ids only of their own shapes (`consult_ref` a guid, the instance
/// a SHA-256). An event C3 queued before wave 2 (the raw event, its labels as typed) leaves with
/// `other` where its labels were; an event of the current constructors closes to itself. `None` -
/// the event is discarded, with a local diagnostic - for a body that is no C3 consultation or rating
/// event, or that has no instance id or no time of this client's shape (nothing could attribute it).
///
/// (wave 2g, F19-3) What leaves is ALWAYS the serialised reconstruction, never the queued bytes: a
/// body can hide a value the parse dropped - a duplicate key (`"title":"customer-acme"` before the
/// real `title`: the parse keeps the last one), whitespace, escapes - so even a body whose parsed
/// value equals its reconstruction is re-serialised. The reconstruction keeps the constructors' key
/// order (`preserve_order`), so a current event still leaves byte for byte as it was built.
pub(crate) fn close_event_body(body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body).ok()?;
    close_event(&v).map(|closed| closed.to_string())
}

fn close_event(v: &Value) -> Option<Value> {
    use serde_json::{json, Map};
    let o = v.as_object()?;
    if o.get("app_id")?.as_str()? != "c3" {
        return None;
    }
    let instance_id = o.get("instance_id")?.as_str()?;
    if !is_instance_id(instance_id) {
        return None;
    }
    let client_time = chrono::DateTime::parse_from_rfc3339(o.get("client_time")?.as_str()?)
        .ok()?
        .with_timezone(&chrono::Utc)
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let event_type = o.get("event_type")?.as_str()?;
    if event_type != "consultation" && event_type != "rating" {
        return None;
    }
    let empty = Map::new();
    let d = o
        .get("details")
        .and_then(|d| d.as_object())
        .unwrap_or(&empty);
    let text = |m: &Map<String, Value>, k: &str| -> String {
        m.get(k)
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let engine = text(d, "engine");
    let engine = if ENGINES.contains(&engine.as_str()) {
        engine
    } else {
        "other".to_string()
    };
    // the provider: a CLASS of the table as it stands, else other (a label is never resolved here)
    let vendor = vendor_of_class(&text(d, "provider"));
    let raw_model = text(d, "model");
    let model = if raw_model.is_empty() || raw_model == "unknown" {
        "unknown".to_string()
    } else {
        match model_token(vendor, &raw_model).as_str() {
            "unknown" => "other".to_string(),
            m => m.to_string(),
        }
    };
    let provider = vendor.map(|v| v.class).unwrap_or("other").to_string();
    let raw_purpose = text(d, "purpose");
    let purpose = if raw_purpose == "none" {
        raw_purpose
    } else {
        purpose_class(&raw_purpose)
    };
    let os = closed_os(&text(o, "os"));
    let runtime = text(o, "runtime");
    let runtime = match runtime.strip_prefix("rust ") {
        Some(ver) if is_version(ver) => runtime.clone(),
        _ => "other".to_string(),
    };
    let app_version = text(o, "app_version");
    let app_version = if is_version(&app_version) {
        app_version
    } else {
        "unknown".to_string()
    };
    let consult_ref = Some(text(d, "consult_ref").to_ascii_lowercase()).filter(|r| is_guid(r));
    let mut details = Map::new();
    details.insert("engine".into(), json!(engine));
    details.insert("provider".into(), json!(provider));
    details.insert("model".into(), json!(model));
    details.insert("purpose".into(), json!(purpose));
    let (severity, title) = if event_type == "consultation" {
        let raw = text(d, "outcome");
        let outcome = match raw.as_str() {
            "usable" | "usable-after-continuation" => raw.clone(),
            _ => match raw.strip_prefix("failed:") {
                Some(c) if FAILURE_CLASSES.contains(&c) => raw.clone(),
                _ => "failed:unknown".to_string(),
            },
        };
        let severity = if outcome.starts_with("usable") {
            "info"
        } else if ["failed:auth", "failed:quota", "failed:operator"].contains(&outcome.as_str()) {
            "warning"
        } else {
            "error"
        };
        let num = |k: &str| d.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
        let small = |k: &str, dflt: u64| {
            d.get(k)
                .and_then(|x| x.as_u64())
                .filter(|n| *n <= u32::MAX as u64)
                .unwrap_or(dflt)
        };
        let flag = |k: &str| d.get(k).and_then(|x| x.as_bool()).unwrap_or(false);
        let wall = d
            .get("wall_seconds")
            .and_then(|x| x.as_f64())
            .filter(|w| w.is_finite() && *w >= 0.0)
            .unwrap_or(0.0);
        details.insert("outcome".into(), json!(outcome));
        details.insert("wall_seconds".into(), json!(wall));
        details.insert("tokens_in".into(), json!(num("tokens_in")));
        details.insert("tokens_out".into(), json!(num("tokens_out")));
        details.insert("findings".into(), json!(num("findings")));
        details.insert("structured".into(), json!(flag("structured")));
        details.insert("format_retry".into(), json!(flag("format_retry")));
        details.insert("panel_size".into(), json!(small("panel_size", 1)));
        details.insert("peers_used".into(), json!(small("peers_used", 0)));
        details.insert("os".into(), json!(closed_os(&text(d, "os"))));
        let druntime = text(d, "runtime");
        details.insert(
            "runtime".into(),
            json!(match druntime.strip_prefix("rust ") {
                Some(ver) if is_version(ver) => druntime.clone(),
                _ => "other".to_string(),
            }),
        );
        let topic = text(d, "topic_tag");
        if crate::telemetry::event::TOPIC_VOCAB.contains(&topic.as_str()) {
            details.insert("topic_tag".into(), json!(topic));
        }
        let useful = text(d, "useful");
        if ["yes", "partly", "no"].contains(&useful.as_str()) {
            details.insert("useful".into(), json!(useful));
        }
        for k in ["verified", "rejected"] {
            if let Some(n) = d.get(k).and_then(|x| x.as_i64()) {
                details.insert(k.into(), json!(n));
            }
        }
        if let Some(r) = &consult_ref {
            details.insert("consult_ref".into(), json!(r));
        }
        (severity, purpose.clone())
    } else {
        let mark = text(d, "mark");
        let mark = if ["yes", "partly", "no"].contains(&mark.as_str()) {
            mark
        } else {
            "other".to_string()
        };
        let bridge = text(d, "bridge_version");
        let bridge = if is_version(&bridge) {
            bridge
        } else {
            app_version.clone()
        };
        let judge = match d.get("judge").and_then(|j| j.as_object()) {
            Some(j) => closed_judge(&text(j, "provider"), &text(j, "model"), &text(j, "source")),
            None => Judge::unknown(),
        };
        details.insert("mark".into(), json!(mark));
        details.insert(
            "age_days".into(),
            json!(d
                .get("age_days")
                .and_then(|x| x.as_i64())
                .filter(|n| *n >= 0)
                .unwrap_or(0)),
        );
        details.insert("bridge_version".into(), json!(bridge));
        details.insert("os".into(), json!(closed_os(&text(d, "os"))));
        details.insert("ps_version".into(), json!("unknown"));
        details.insert("judge".into(), judge.to_value());
        if let Some(n) = d
            .get("rating_rev")
            .and_then(|x| x.as_i64())
            .filter(|n| *n >= 1)
        {
            details.insert("rating_rev".into(), json!(n));
        }
        if let Some(r) = &consult_ref {
            details.insert("consult_ref".into(), json!(r));
        }
        ("info", mark)
    };
    let mut out = Map::new();
    out.insert("app_id".into(), json!("c3"));
    out.insert("app_version".into(), json!(app_version));
    out.insert("instance_id".into(), json!(instance_id));
    out.insert("event_type".into(), json!(event_type));
    out.insert("severity".into(), json!(severity));
    out.insert("title".into(), json!(title));
    out.insert("details".into(), Value::Object(details));
    out.insert("tags".into(), json!([provider, model]));
    out.insert("client_time".into(), json!(client_time));
    out.insert("os".into(), json!(os));
    out.insert("runtime".into(), json!(runtime));
    Some(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_table_and_closed_models() {
        assert_eq!(
            vendor_by_host("https://api.z.ai/api/v1").unwrap().class,
            "zai"
        );
        assert_eq!(
            vendor_by_host("https://open.bigmodel.cn/x").unwrap().class,
            "zai"
        );
        assert_eq!(
            vendor_by_host("https://api.kimi.ai/coding/v1")
                .unwrap()
                .class,
            "moonshot"
        );
        assert_eq!(
            vendor_by_host("https://API.OPENAI.COM/v1").unwrap().class,
            "openai"
        );
        assert!(vendor_by_host("https://llm.acmecorp-internal.example/v1").is_none());
        // A look-alike host is no vendor's: the host must END with "." + the listed name.
        assert!(vendor_by_host("https://evilz.ai/v1").is_none());
        assert!(vendor_by_host("http://[::1]:8080/v1").is_none());
        let zai = vendor_of_class("zai");
        assert_eq!(model_token(zai, "GLM-5.3"), "glm-5.3");
        assert_eq!(model_token(zai, "glm-5.3[1m]"), "glm-5.3");
        assert_eq!(model_token(zai, "glm-9-secret"), "other");
        assert_eq!(model_token(None, "glm-5.3"), "other");
        assert_eq!(model_token(zai, "  "), "unknown");
        assert_eq!(
            model_token(vendor_of_class("minimax"), "MiniMax-M3"),
            "minimax-m3"
        );
    }

    #[test]
    fn judge_classifier_by_name_engine_and_host() {
        let v = |p: &str, m: &str, e: &str, h: &str| CoordinatorView {
            provider: p.into(),
            model: m.into(),
            engine: e.into(),
            host: h.into(),
        };
        let none = Roster::default();
        assert_eq!(
            judge_class(
                &v("OpenAI", "gpt-6-astra", "", "unknown"),
                Some(&none),
                None
            ),
            ("openai".into(), "gpt-6-astra".into())
        );
        assert_eq!(
            judge_class(&v("anthropic", "claude-opus-5-5", "", ""), None, None),
            ("anthropic".into(), "claude-opus-5-5".into())
        );
        // A name outside the roster: its engine's class, else other.
        assert_eq!(
            judge_class(&v("gem", "gemini-3.1-pro-high", "agy", ""), None, None),
            ("google".into(), "gemini-3.1-pro-high".into())
        );
        assert_eq!(
            judge_class(
                &v("customer-acme", "acme-7b", "", "claude-code"),
                None,
                None
            ),
            ("other".into(), "other".into())
        );
        // No provider: the host claude-code -> anthropic (the model is not named: other).
        assert_eq!(
            judge_class(&v("", "", "", "claude-code"), None, None),
            ("anthropic".into(), "other".into())
        );
        assert_eq!(
            judge_class(&v("", "", "", "unknown"), None, None),
            ("other".into(), "other".into())
        );
    }

    #[test]
    fn closed_judge_allowlist() {
        assert_eq!(
            closed_judge("moonshot", "k3", "rating_actor"),
            Judge {
                provider: "moonshot".into(),
                model: "k3".into(),
                source: "rating_actor".into()
            }
        );
        // An unknown source is always other/other.
        assert_eq!(closed_judge("openai", "gpt-5.1", "who"), Judge::unknown());
        // A class outside the table, a model outside its list.
        assert_eq!(closed_judge("acme", "x", "rating_actor").provider, "other");
        assert_eq!(
            closed_judge("openai", "gpt-9", "rating_actor").model,
            "other"
        );
        assert_eq!(mark_judge(Some(&serde_json::json!("x"))), None);
        assert_eq!(
            mark_judge(Some(&serde_json::json!({"provider": "openai"}))),
            None
        );
    }

    #[test]
    fn openrouter_classes_are_closed() {
        assert_eq!(
            vendor_by_host("https://openrouter.ai/api/v1")
                .unwrap()
                .class,
            "openrouter"
        );
        let or = vendor_of_class("openrouter");
        assert_eq!(model_token(or, "openai/GPT-5.1"), "openai/gpt-5.1");
        assert_eq!(model_token(or, "z-ai/glm-5.3"), "z-ai/glm-5.3");
        assert_eq!(model_token(or, "moonshotai/kimi-k3"), "moonshotai/kimi-k3");
        // the model part outside its class's list, an unknown vendor slug, no slug at all
        assert_eq!(model_token(or, "openai/customer-acme-ft"), "other");
        assert_eq!(model_token(or, "customer-acme/gpt-5.1"), "other");
        assert_eq!(model_token(or, "gpt-5.1"), "other");
        assert_eq!(
            closed_judge("openrouter", "openai/gpt-5.1", "rating_actor").model,
            "openai/gpt-5.1"
        );
        assert_eq!(
            closed_judge("openrouter", "acme/x", "rating_actor").model,
            "other"
        );
    }

    #[test]
    fn reviewer_purpose_and_outcome_classes() {
        let mut e = LedgerEntry::default();
        e.reviewer.provider = "customer-acme".into();
        e.reviewer.model = "customer-acme-7b".into();
        e.reviewer.engine = "codex".into();
        e.reviewer.provider_config =
            serde_json::json!({"base_url": "https://llm.customer-acme.example/v1"});
        assert_eq!(
            reviewer_class(&e),
            ("codex".into(), "other".into(), "other".into())
        );
        e.reviewer.provider_config = serde_json::json!({"base_url": "https://api.z.ai/api/v1"});
        e.reviewer.model = "glm-5.3".into();
        assert_eq!(
            reviewer_class(&e),
            ("codex".into(), "zai".into(), "glm-5.3".into())
        );
        e.reviewer.provider_config = serde_json::json!({"builtin": "openai"});
        e.reviewer.model = "gpt-6-astra".into();
        assert_eq!(reviewer_class(&e).1, "openai");
        e.reviewer.engine = "muse".into();
        e.reviewer.provider_config = Value::Null;
        e.reviewer.model = String::new();
        e.model = String::new();
        assert_eq!(
            reviewer_class(&e),
            ("muse".into(), "meta".into(), "unknown".into())
        );
        e.reviewer.engine = "customer-acme".into();
        assert_eq!(reviewer_class(&e).0, "other");
        assert_eq!(purpose_class("customer-acme"), "other");
        assert_eq!(purpose_class(""), "none");
        assert_eq!(purpose_class("diff-review"), "diff-review");
        e.bridge_outcome = "usable reply".into();
        assert_eq!(outcome_class(&e), ("usable".into(), "info"));
        e.bridge_outcome = "usable reply (after a timeout continuation)".into();
        assert_eq!(outcome_class(&e).0, "usable-after-continuation");
        e.bridge_outcome = "failed: timeout after 600 s".into();
        assert_eq!(outcome_class(&e), ("failed:timeout".into(), "error"));
        e.provider_failure = Some(c3_core::ledger::ProviderFailure {
            class: "quota".into(),
            ..Default::default()
        });
        assert_eq!(outcome_class(&e), ("failed:quota".into(), "warning"));
        e.provider_failure.as_mut().unwrap().class = "customer-acme".into();
        assert_eq!(outcome_class(&e).0, "failed:unknown");
        assert_eq!(verdict_class("customer-acme"), "other");
    }

    #[test]
    fn http_reviewers_get_c3_classes() {
        let mut e = LedgerEntry::default();
        e.reviewer.engine = "http".into();
        e.reviewer.provider = "or-label".into();
        e.reviewer.provider_config = serde_json::json!({"engine": "http", "base_url": "https://openrouter.ai/api/v1", "model": "x"});
        e.reviewer.model = "openai/gpt-5.1".into();
        assert_eq!(
            reviewer_class(&e),
            ("http".into(), "openrouter".into(), "openai/gpt-5.1".into())
        );
        e.reviewer.model = "customer-acme/private-7b".into();
        assert_eq!(reviewer_class(&e).2, "other");
        // an OpenAI-compatible endpoint of a listed vendor: that vendor's class and list
        e.reviewer.provider_config =
            serde_json::json!({"engine": "http", "base_url": "https://api.z.ai/api/paas/v4"});
        e.reviewer.model = "glm-5.3".into();
        assert_eq!(reviewer_class(&e).1, "zai");
        assert_eq!(reviewer_class(&e).2, "glm-5.3");
        // a private host: other / other
        e.reviewer.provider_config =
            serde_json::json!({"engine": "http", "base_url": "http://localhost:8080/v1"});
        assert_eq!(
            reviewer_class(&e),
            ("http".into(), "other".into(), "other".into())
        );
    }

    #[test]
    fn consult_ref_shape() {
        let mut e = LedgerEntry::default();
        assert_eq!(consult_ref_of(&e), None);
        e.consult_ref = Some(" 6F1C2A9E-4B7D-4E2A-9C3F-0D8E5B7A1C24 ".into());
        assert_eq!(
            consult_ref_of(&e).as_deref(),
            Some("6f1c2a9e-4b7d-4e2a-9c3f-0d8e5b7a1c24")
        );
        e.consult_ref = Some("not-a-guid".into());
        assert_eq!(consult_ref_of(&e), None);
    }

    /// (F09-5) Closing is the identity on what the current constructors build: a queued event of
    /// this build is sent with its EXACT bytes (consultations usable and failed, an OpenRouter
    /// model, no model, no purpose; ratings with every judge source). (wave 2g, F19-3) Those bytes
    /// are the SERIALISED reconstruction, not the queued body - the comparison checks the parsed
    /// value and the constructors' key order at once.
    #[test]
    fn a_current_event_closes_to_its_exact_bytes() {
        use crate::telemetry::{Event, RatingEvent, RatingInput};
        use c3_core::ledger::ProviderFailure;
        let iid = "ab".repeat(32);
        let mut e = LedgerEntry {
            purpose: "diff-review".into(),
            bridge_outcome: "usable reply".into(),
            wall_seconds: 3.0,
            consult_ref: Some("6f1c2a9e-4b7d-4e2a-9c3f-0d8e5b7a1c24".into()),
            ..Default::default()
        };
        e.reviewer.engine = "codex".into();
        e.reviewer.model = "glm-5.3".into();
        e.reviewer.provider_config = serde_json::json!({"base_url": "https://api.z.ai/api/v1"});
        let mut bodies = Vec::new();
        bodies.push(serde_json::to_string(&Event::from_ledger(&e, Some(2), &iid)).unwrap());
        e.bridge_outcome = "failed: quota".into();
        e.provider_failure = Some(ProviderFailure {
            class: "quota".into(),
            ..Default::default()
        });
        e.wall_seconds = 12.75;
        e.purpose = String::new();
        bodies.push(serde_json::to_string(&Event::from_ledger(&e, None, &iid)).unwrap());
        e.reviewer.engine = "http".into();
        e.reviewer.provider_config =
            serde_json::json!({"base_url": "https://openrouter.ai/api/v1"});
        e.reviewer.model = "openai/gpt-5.1".into();
        e.purpose = "my-own-purpose".into();
        bodies.push(serde_json::to_string(&Event::from_ledger(&e, Some(1), &iid)).unwrap());
        e.reviewer.model = String::new();
        e.consult_ref = None;
        bodies.push(serde_json::to_string(&Event::from_ledger(&e, Some(1), &iid)).unwrap());
        let rated = chrono::DateTime::parse_from_rfc3339("2026-10-09T10:00:00+02:00").unwrap();
        for (judge, rev) in [
            (closed_judge("moonshot", "k3", "rating_actor"), Some(2)),
            (
                closed_judge("openai", "gpt-6-astra", "consult_coordinator"),
                None,
            ),
            (Judge::unknown(), Some(1)),
        ] {
            let input = RatingInput {
                entry: &e,
                mark: "partly",
                rated_at: rated,
                consult_when: None,
                judge: &judge,
                rating_rev: rev,
            };
            bodies.push(serde_json::to_string(&RatingEvent::from_rating(&input, &iid)).unwrap());
        }
        for b in &bodies {
            assert_eq!(close_event_body(b).as_deref(), Some(b.as_str()), "{b}");
            // the same event in other bytes (whitespace) leaves as the constructors built it
            let spaced = serde_json::to_string_pretty(
                &serde_json::from_str::<serde_json::Value>(b).unwrap(),
            )
            .unwrap();
            assert_ne!(&spaced, b);
            assert_eq!(
                close_event_body(&spaced).as_deref(),
                Some(b.as_str()),
                "{b}"
            );
        }
        // what cannot be attributed is discarded: another app, no instance id, no time
        assert!(close_event_body(&bodies[0].replace("\"c3\"", "\"other-app\"")).is_none());
        assert!(close_event_body(&bodies[0].replace(&iid, "testinstance")).is_none());
        assert!(close_event_body(r#"{"app_id":"c3","instance_id":"x"}"#).is_none());
        assert!(close_event_body("not json").is_none());
    }

    /// (wave 2g, F19-3) A duplicate key hides a private value from the parse (the last one wins):
    /// a current event with `"title":"customer-acme",` inserted before its real title closes to the
    /// event without it - the queued bytes never leave.
    #[test]
    fn a_duplicate_key_never_carries_its_hidden_value_out() {
        use crate::telemetry::Event;
        let mut e = LedgerEntry {
            purpose: "diff-review".into(),
            bridge_outcome: "usable reply".into(),
            ..Default::default()
        };
        e.reviewer.engine = "codex".into();
        let body =
            serde_json::to_string(&Event::from_ledger(&e, Some(2), &"ab".repeat(32))).unwrap();
        let forged = body.replacen("\"title\":", "\"title\":\"customer-acme\",\"title\":", 1);
        assert!(forged.contains("customer-acme"));
        let parsed: Value = serde_json::from_str(&forged).unwrap();
        assert_eq!(parsed, serde_json::from_str::<Value>(&body).unwrap());
        let closed = close_event_body(&forged).expect("a valid event");
        assert!(!closed.contains("customer-acme"), "{closed}");
        assert_eq!(closed, body);
    }
}
