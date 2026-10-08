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
/// process, parsed exactly as the bridge parses it for the ledger's coordinator record (with the
/// reviewer roster; the host hint of this process), classified by [`judge_class`] - source
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
    let Ok(m) = c3_core::host::parse_coordinator_matcher(v, entries) else {
        return Some(none);
    };
    let record = c3_core::host::build_coordinator(c3_core::host::coordinator_host(), Some(&m));
    let (provider, model) = judge_class(&CoordinatorView::of(&record), roster.as_ref(), None);
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
}
