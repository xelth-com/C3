//! Reviewer identity and lineage (`Resolve-ReviewerIdentity`, `Resolve-EngineIdentity`,
//! `Format-Lineage`, `Format-ReviewerLineage`) and the engine table (`$script:Engines`).
//!
//! The provider fingerprint is the SHA-256 of the endpoint's compat string, so
//! comments, key order, `name`, headers and secret rotation never change it. An
//! engine's endpoint is the engine itself.

use crate::config::{
    self, canonical_base_url, provider_endpoint, provider_names, provider_set_problem,
    provider_table, CodexConfig,
};
use crate::sha256_hex;

/// The subprocess-launcher engines that have a CLI on PATH. `codex` is the default. This is
/// deliberately the *subprocess* subset: `http` has no launcher, so it is not here (roster
/// validation and launcher discovery iterate this list), but it does have a lineage row -
/// see [`ALL_ENGINE_NAMES`] and [`engine_spec`], which know `http` too (F03-8, F08-7).
pub const ENGINE_NAMES: &[&str] = &["codex", "agy", "muse"];

/// Every engine kind that has an identity/lineage row, including the non-subprocess `http`
/// engine. The single source that keeps `engine.rs`'s `EngineKind` (four) and the lineage
/// table (previously three) in agreement (F08-7).
pub const ALL_ENGINE_NAMES: &[&str] = &["codex", "agy", "muse", "http"];

/// A row of the engine table (the fields providers/preflight need).
#[derive(Debug, Clone)]
pub struct EngineSpec {
    pub name: &'static str,
    pub command: &'static str,
    /// Environment variable that overrides the launcher path.
    pub exe_env: &'static str,
    pub host_name: &'static str,
    pub compat_string: &'static str,
    pub default_provider: &'static str,
    pub model_example: &'static str,
    /// The sign-in check reads local files only (runs under `-NoNetwork` too).
    pub local_sign_in: bool,
}

/// The engine's launcher basenames to look up on PATH, in order (platform-aware).
pub fn launcher_names(engine: &str) -> Vec<&'static str> {
    let win = cfg!(windows);
    match engine {
        "codex" => {
            if win {
                vec!["codex.exe", "codex.cmd", "codex.bat", "codex"]
            } else {
                vec!["codex"]
            }
        }
        "agy" => {
            if win {
                vec!["agy.exe", "agy.cmd", "agy.bat", "agy"]
            } else {
                vec!["agy"]
            }
        }
        "muse" => {
            if win {
                vec!["muse.cmd", "muse.exe", "muse"]
            } else {
                vec!["muse"]
            }
        }
        _ => vec![],
    }
}

/// Vendor install locations tried after PATH: (env var, relative path). muse only.
pub fn install_launchers(engine: &str) -> Vec<(&'static str, &'static str)> {
    if engine == "muse" && cfg!(windows) {
        vec![("LOCALAPPDATA", "Programs\\muse\\muse.cmd")]
    } else {
        vec![]
    }
}

pub fn engine_spec(name: &str) -> Option<EngineSpec> {
    match name {
        "codex" => Some(EngineSpec {
            name: "codex",
            command: "codex",
            exe_env: "CODEX_CONSULT_EXE",
            host_name: "",
            compat_string: "",
            default_provider: "",
            model_example: "gpt-5.1",
            local_sign_in: false,
        }),
        "agy" => Some(EngineSpec {
            name: "agy",
            command: "agy",
            exe_env: "CODEX_CONSULT_AGY_EXE",
            host_name: "engine:agy",
            compat_string: "cc-engine-v1|agy",
            default_provider: "gemini",
            model_example: "gemini-3.8-flash-high",
            local_sign_in: false,
        }),
        "muse" => Some(EngineSpec {
            name: "muse",
            command: "muse",
            exe_env: "CODEX_CONSULT_MUSE_EXE",
            host_name: "engine:muse",
            compat_string: "cc-engine-v1|muse",
            default_provider: "meta",
            model_example: "muse-spark-1.3",
            local_sign_in: true,
        }),
        // The `http` engine has a lineage row but no launcher: it sends an
        // OpenAI-compatible request (M7), so `command`/`exe_env` are empty and it never
        // appears in `ENGINE_NAMES`. Its endpoint is a provider, so its fingerprint folds
        // the provider into the compat string (see `resolve_engine_identity`) rather than
        // collapsing every http reviewer to one identity.
        "http" => Some(EngineSpec {
            name: "http",
            command: "",
            exe_env: "",
            host_name: "engine:http",
            compat_string: "cc-engine-v1|http",
            default_provider: "",
            model_example: "gpt-5.1",
            local_sign_in: false,
        }),
        _ => None,
    }
}

/// `<provider> :: <model>`.
pub fn format_lineage(provider: &str, model: &str) -> String {
    format!("{provider} :: {model}")
}

/// `Format-ReviewerLineage`: the lineage plus ` [<engine>]` for a non-codex engine.
pub fn format_reviewer_lineage(provider: &str, model: &str, engine: &str) -> String {
    let mut l = format_lineage(provider, model);
    if !engine.is_empty() && engine != "codex" {
        l.push_str(&format!(" [{engine}]"));
    }
    l
}

/// The resolved (or unresolved) reviewer identity.
#[derive(Debug, Clone)]
pub struct ReviewerIdentity {
    pub provider: String,
    pub provider_source: String,
    pub model: String,
    pub model_source: String,
    pub lineage: String,
    pub resolved: bool,
    pub note: String,
    pub error: String,
    pub fingerprint: String,
    pub compat_string: String,
    pub host: String,
    pub base_url: String,
    pub wire_api: String,
    pub engine: String,
}

impl ReviewerIdentity {
    fn blank(config_path: &str) -> Self {
        let _ = config_path;
        ReviewerIdentity {
            provider: "unknown".into(),
            provider_source: String::new(),
            model: "unknown".into(),
            model_source: "unknown".into(),
            lineage: String::new(),
            resolved: false,
            note: String::new(),
            error: String::new(),
            fingerprint: String::new(),
            compat_string: String::new(),
            host: String::new(),
            base_url: String::new(),
            wire_api: String::new(),
            engine: "codex".into(),
        }
    }
}

/// `Resolve-ReviewerIdentity`.
pub fn resolve_reviewer_identity(
    config: &CodexConfig,
    provider: &str,
    model: &str,
    openai_base_url: &str,
    engine: &str,
    launcher: &str,
) -> ReviewerIdentity {
    let mut id = ReviewerIdentity::blank(&config.path);
    if !engine.is_empty() && engine != "codex" {
        return resolve_engine_identity(id, engine, provider, model, launcher);
    }
    let mut notes: Vec<String> = Vec::new();
    let mut infos: Vec<String> = Vec::new();
    let where_ = if !config.path.is_empty() {
        config.path.clone()
    } else {
        "(no Codex home)".to_string()
    };
    let file_reason = if config.exists && !config.ok {
        config.reason.clone()
    } else {
        String::new()
    };
    let top = if config.exists && config.ok {
        config.top()
    } else {
        None
    };
    let mut top_reason = file_reason.clone();
    if top_reason.is_empty() {
        if let Some(t) = top {
            if !t.ok {
                top_reason = t.reason.clone();
            }
        }
    }
    if !file_reason.is_empty() {
        notes.push(file_reason.clone());
    }
    if let Some(t) = top {
        let pf = config::get_toml_string(Some(t), "profile");
        if pf.present {
            if !pf.reason.is_empty() {
                notes.push(pf.reason);
            } else {
                notes.push(format!(
                    "the Codex config selects profile '{}' (line {}); profiles are not supported - a profile can change the provider, the model and the effort behind the bridge",
                    pf.value, pf.line
                ));
            }
        }
    }
    let cfg_provider = if top.is_some() && top_reason.is_empty() {
        Some(config::get_toml_string(top, "model_provider"))
    } else {
        None
    };
    let cfg_model = if top.is_some() && top_reason.is_empty() {
        Some(config::get_toml_string(top, "model"))
    } else {
        None
    };

    if !provider.is_empty() {
        id.provider = provider.to_string();
        id.provider_source = "-Provider".into();
    } else if !top_reason.is_empty() {
        if !notes.contains(&top_reason) {
            notes.push(top_reason.clone());
        }
    } else if let Some(cp) = &cfg_provider {
        if cp.present {
            if !cp.reason.is_empty() {
                notes.push(cp.reason.clone());
            } else if cp.value.is_empty() {
                notes.push(format!(
                    "model_provider at line {} of {where_} is empty",
                    cp.line
                ));
            } else {
                id.provider = cp.value.clone();
                id.provider_source = "config".into();
            }
        } else {
            id.provider = "openai".into();
            id.provider_source = "codex default".into();
        }
    } else {
        id.provider = "openai".into();
        id.provider_source = "codex default".into();
    }

    if !model.is_empty() {
        id.model = model.to_string();
        id.model_source = "-Model".into();
    } else if !top_reason.is_empty() {
        if !notes.contains(&top_reason) {
            notes.push(top_reason.clone());
        }
    } else if let Some(cm) = &cfg_model {
        if cm.present {
            if !cm.reason.is_empty() {
                notes.push(cm.reason.clone());
            } else if cm.value.is_empty() {
                notes.push(format!("model at line {} of {where_} is empty", cm.line));
            } else {
                id.model = cm.value.clone();
                id.model_source = "config".into();
            }
        } else if config.exists {
            notes.push(format!("no -Model and no top-level model in {where_}"));
        } else {
            notes.push(format!("no -Model and no Codex config at {where_} (the model Codex picks by default is not known to the bridge)"));
        }
    } else if config.exists {
        notes.push(format!("no -Model and no top-level model in {where_}"));
    } else {
        notes.push(format!("no -Model and no Codex config at {where_} (the model Codex picks by default is not known to the bridge)"));
    }

    let mut compat = String::new();
    if !id.provider_source.is_empty() {
        let name = id.provider.clone();
        let table_name = format!(
            "[{}]",
            config::format_toml_path(&["model_providers".into(), name.clone()])
        );
        let mut err = String::new();
        let (pt_found, pt_ok, pt_reason, pt_key) = if config.exists && config.ok {
            let pt = provider_table(config, &name);
            (pt.found, pt.ok, pt.reason, pt.table_key)
        } else {
            (false, false, String::new(), None)
        };
        let set_problem = if config.exists && config.ok {
            provider_set_problem(config, &name)
        } else {
            String::new()
        };
        if !set_problem.is_empty() {
            err = format!("the providers in {where_} could not be established, so {table_name} may be declared there - {set_problem}");
        } else if name == "openai" && !pt_found {
            compat = "cc-provider-v1|builtin:openai".into();
            id.host = "builtin:openai".into();
            id.wire_api = "(built in)".into();
            if !openai_base_url.trim().is_empty() {
                let (curl, chost) = canonical_base_url(openai_base_url);
                compat.push_str(&format!("|base_url={curl}"));
                id.host = chost;
                id.base_url = curl;
            }
        } else if !config.exists {
            err = format!("unknown provider '{name}': there is no Codex config at {where_}, so there is no {table_name} table (built in: openai)");
        } else if !config.ok {
            err = format!("{table_name} cannot be read: {}", config.reason);
        } else if !pt_found {
            let found = provider_names(config);
            let list = if found.is_empty() {
                "(none)".to_string()
            } else {
                found.join(", ")
            };
            err = format!("unknown provider '{name}': {where_} has no {table_name} table; providers found: {list} (built in: openai)");
        } else if !pt_ok {
            err = format!("{table_name} in {where_} is not usable - {pt_reason}");
        } else {
            let ep = provider_endpoint(
                config,
                pt_key.as_deref().unwrap_or(""),
                &table_name,
                &where_,
            );
            if !ep.error.is_empty() {
                err = ep.error;
            } else {
                compat = ep.compat;
                id.host = ep.host;
                id.base_url = ep.base_url;
                id.wire_api = ep.wire_api;
                if name == "openai" {
                    infos.push(
                        "user-defined [model_providers.openai] table used for the identity".into(),
                    );
                }
            }
        }
        if !err.is_empty() {
            if id.provider_source == "-Provider" {
                id.error = err.clone();
            } else if name == "openai" {
                let who = if id.provider_source == "config" {
                    "the config's model_provider"
                } else {
                    "Codex's default provider openai"
                };
                notes.push(format!("{who}: {err}"));
            } else {
                notes.push(format!("the config's model_provider: {err}"));
            }
            compat.clear();
        }
    }
    id.lineage = format_lineage(&id.provider, &id.model);
    id.resolved = !id.provider_source.is_empty()
        && id.model_source != "unknown"
        && !compat.is_empty()
        && id.error.is_empty()
        && notes.is_empty();
    if id.resolved {
        id.compat_string = compat.clone();
        id.fingerprint = sha256_hex(compat.as_bytes());
    }
    let mut all = notes;
    all.extend(infos);
    id.note = all.join("; ");
    id
}

fn resolve_engine_identity(
    mut id: ReviewerIdentity,
    engine: &str,
    provider: &str,
    model: &str,
    launcher: &str,
) -> ReviewerIdentity {
    id.engine = engine.to_string();
    let spec = match engine_spec(engine) {
        Some(s) => s,
        None => {
            id.error = format!(
                "unknown engine '{engine}' (engines: {})",
                ENGINE_NAMES.join(", ")
            );
            id.lineage = format_lineage(&id.provider, &id.model);
            return id;
        }
    };
    if !provider.is_empty() {
        id.provider = provider.to_string();
        id.provider_source = "-Provider".into();
    } else {
        id.provider = spec.default_provider.to_string();
        id.provider_source = "engine default".into();
    }
    if !model.is_empty() {
        id.model = model.to_string();
        id.model_source = "-Model".into();
    } else {
        id.error = format!(
            "the {engine} engine needs a model: pass -Model <id> (the full model id, e.g. {}) or name it in the roster entry",
            spec.model_example
        );
    }
    id.host = spec.host_name.to_string();
    id.wire_api = String::new();
    id.base_url = String::new();
    id.lineage = format_lineage(&id.provider, &id.model);
    let _ = launcher;
    if id.error.is_empty() {
        id.resolved = true;
        // The http engine's endpoint is a provider, so its fingerprint must distinguish one
        // http provider from another; the CLI engines have a single endpoint (the engine
        // itself), so their compat string is fixed.
        let compat = if engine == "http" {
            format!("{}|provider={}", spec.compat_string, id.provider)
        } else {
            spec.compat_string.to_string()
        };
        id.compat_string = compat.clone();
        id.fingerprint = sha256_hex(compat.as_bytes());
    }
    id
}
