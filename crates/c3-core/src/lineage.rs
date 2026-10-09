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
pub const ENGINE_NAMES: &[&str] = &["codex", "agy", "muse", "claude"];

/// Every engine kind that has an identity/lineage row, including the non-subprocess `http`
/// engine. The single source that keeps `engine.rs`'s `EngineKind` (four) and the lineage
/// table (previously three) in agreement (F08-7).
pub const ALL_ENGINE_NAMES: &[&str] = &["codex", "agy", "muse", "claude", "http"];

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
    // --- display / behaviour fields (`$script:Engines`), used by the consult orchestrator
    // and the dry-run block. Codex leaves the engine-only ones empty (`isCodex` branches).
    /// Header/author label: `Codex`, `Gemini (agy)`, `Meta Muse (muse)`.
    pub label: &'static str,
    /// Handoff file-name prefix (`codex`/`agy`/`muse`).
    pub prefix: &'static str,
    /// The flag the reply schema is passed with (`--output-schema` / `--json-schema`).
    pub schema_flag: &'static str,
    /// Where the reply text comes from (empty for codex).
    pub reply_source: &'static str,
    /// The resume flag (`--conversation` / `--session-id`; empty for codex).
    pub thread_flag: &'static str,
    /// The thread noun (`thread` / `conversation` / `session`).
    pub thread_noun: &'static str,
    /// The `-Sandbox` refusal parenthetical (empty for codex).
    pub read_only_note: &'static str,
    /// The ledger `sandbox` record for a non-codex engine (empty for codex).
    pub sandbox_record: &'static str,
    /// The prompt's `Tools:` line for a non-codex engine (empty for codex).
    pub tools_line: &'static str,
    /// The model-step-cap flag (`--max-model-steps`; empty = the engine has none).
    pub steps_flag: &'static str,
    /// The engine runs a denial-retry turn (agy only).
    pub denial_retry: bool,
    /// The stream reports token usage (codex/agy yes, muse no).
    pub has_usage: bool,
    /// The prompt is delivered through `--prompt-file` (muse), not stdin.
    pub prompt_by_file: bool,
    /// The transports the engine accepts (`["native", "prompt-only"]` for agy/muse).
    pub transports: &'static [&'static str],
    /// The default mode when `-Mode` is empty and no thread applies (`new` for engines).
    pub default_mode: &'static str,
    /// (0.6.0, wave 29) The engine forks a thread (`Modes` holds `fork`: codex, claude).
    pub fork: bool,
    /// (wave 29) The roster `auth` modes the engine takes (`AuthModes`; the first is the default;
    /// empty for an engine without one).
    pub auth_modes: &'static [&'static str],
    /// (wave 29, item 2) A new thread's id is minted by the bridge (`MintsThread`: claude
    /// `--session-id`).
    pub mints_thread: bool,
    /// (wave 29, D9) The largest prompt the engine takes on stdin (`MaxPromptBytes`; 0 = no bound).
    pub max_prompt_bytes: u64,
    /// (wave 29, D5) The members of the engine share ONE scheduling group in a panel
    /// (`ParallelScope 'engine'`).
    pub parallel_scope_engine: bool,
    /// (wave 29, item 1) The directories outside the repository the reviewer must read go to
    /// `--add-dir` (`AddDirs`).
    pub add_dirs: bool,
    /// (wave 29) The argv is quoted by the C runtime rules (`ArgQuote 'crt'`: the schema TEXT
    /// travels in argv).
    pub crt_quote: bool,
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
        "claude" => {
            if win {
                vec!["claude.exe", "claude.cmd", "claude"]
            } else {
                vec!["claude"]
            }
        }
        _ => vec![],
    }
}

/// Vendor install locations tried after PATH: (env var, relative path). muse: the vendor's
/// `%LOCALAPPDATA%\Programs\muse\muse.cmd`; (wave 29) claude: the native installer's
/// `%USERPROFILE%\.local\bin\claude.exe` (`$HOME/.local/bin/claude` elsewhere) - a bridge
/// started before the install may not see it on PATH.
pub fn install_launchers(engine: &str) -> Vec<(&'static str, &'static str)> {
    match engine {
        "muse" if cfg!(windows) => vec![("LOCALAPPDATA", "Programs\\muse\\muse.cmd")],
        "claude" if cfg!(windows) => vec![("USERPROFILE", ".local\\bin\\claude.exe")],
        "claude" => vec![("HOME", ".local/bin/claude")],
        _ => vec![],
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
            label: "Codex",
            prefix: "codex",
            schema_flag: "--output-schema",
            reply_source: "",
            thread_flag: "",
            thread_noun: "thread",
            read_only_note: "",
            sandbox_record: "",
            tools_line: "",
            steps_flag: "",
            denial_retry: false,
            has_usage: true,
            prompt_by_file: false,
            transports: &["output-schema", "prompt-only"],
            default_mode: "",
            fork: true,
            auth_modes: &[],
            mints_thread: false,
            max_prompt_bytes: 0,
            parallel_scope_engine: false,
            add_dirs: false,
            crt_quote: false,
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
            label: "Gemini (agy)",
            prefix: "agy",
            schema_flag: "--json-schema",
            reply_source: "the result event's structured_output (else its response text)",
            thread_flag: "--conversation",
            thread_noun: "conversation",
            read_only_note: "its --sandbox restricts the terminal only; the bridge's tree check fails a run that writes",
            sandbox_record: "read-only (requested; enforced by evidence for tracked and untracked files and the collab directory, not for gitignored paths, submodules or files outside the repository; agy --sandbox restricts the terminal only)",
            tools_line: "Tools: you may read files of the repository; you have NO permission to run commands in this consultation - never call run_command; make NO file changes; a check that needs a command belongs under `## Requested checks`.",
            steps_flag: "",
            denial_retry: true,
            has_usage: true,
            prompt_by_file: false,
            transports: &["native", "prompt-only"],
            default_mode: "new",
            fork: false,
            auth_modes: &[],
            mints_thread: false,
            max_prompt_bytes: 0,
            parallel_scope_engine: false,
            add_dirs: false,
            crt_quote: false,
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
            label: "Meta Muse (muse)",
            prefix: "muse",
            schema_flag: "--output-schema",
            reply_source: "the run_terminal record's text (run.terminal.completed)",
            thread_flag: "--session-id",
            thread_noun: "session",
            read_only_note: "muse runs with --disable-write --disable-shell --disable-web-tools and the bridge's tree check fails a run that changed anything",
            sandbox_record: "read-only (requested; muse --disable-write --disable-shell --disable-web-tools --approval-mode never; checked by evidence for tracked and untracked files and the collab directory, not for gitignored paths, submodules, files outside the repository or what the reviewer reads)",
            tools_line: "Tools: you may read files of the repository (read_file); writing files, the shell and the web tools are disabled in this consultation (--disable-write --disable-shell --disable-web-tools) - do not try them; make NO file changes; a check that needs a command belongs under `## Requested checks`.",
            steps_flag: "--max-model-steps",
            denial_retry: false,
            has_usage: false,
            prompt_by_file: true,
            transports: &["native", "prompt-only"],
            default_mode: "new",
            fork: false,
            auth_modes: &[],
            mints_thread: false,
            max_prompt_bytes: 0,
            parallel_scope_engine: false,
            add_dirs: false,
            crt_quote: false,
        }),
        // (0.6.0, wave 29) Claude Code headless (`claude -p`): the subscription, an API key or a
        // third-party Anthropic-compatible endpoint (wave 29b). Its replies are
        // `NN-claudecode-<slug>.*` - the prefix `claude` stays the coordinator's brief prefix.
        "claude" => Some(EngineSpec {
            name: "claude",
            command: "claude",
            exe_env: "CODEX_CONSULT_CLAUDE_EXE",
            host_name: "engine:claude",
            compat_string: "cc-engine-v1|claude",
            default_provider: "anthropic",
            model_example: "claude-sonnet-5-5",
            local_sign_in: false,
            label: "Claude (claude)",
            prefix: "claudecode",
            schema_flag: "--json-schema",
            reply_source: "the result event's structured_output (else its result text)",
            thread_flag: "--resume",
            thread_noun: "session",
            read_only_note: "claude runs with --restricted and the read tools only (Read, Grep, Glob), each turn's init event must prove it, and the bridge's tree check fails a run that changed anything",
            sandbox_record: "read-only (requested; claude --restricted --tools Read,Grep,Glob --permission-mode dontAsk --strict-mcp-config, proven by each turn's init event; checked by evidence for tracked and untracked files and the collab directory, not for gitignored paths, submodules or files outside the repository; managed settings and their hooks still apply)",
            tools_line: "Tools: you may read files of the repository (Read, Grep, Glob); no shell, web or write tool exists in this consultation; make NO file changes; a check that needs a command belongs under `## Requested checks`.",
            steps_flag: "--max-turns",
            denial_retry: true,
            has_usage: true,
            prompt_by_file: false,
            transports: &["native", "prompt-only"],
            default_mode: "new",
            fork: true,
            auth_modes: crate::claude::CLAUDE_AUTH_MODES,
            mints_thread: true,
            max_prompt_bytes: crate::claude::MAX_PROMPT_BYTES,
            parallel_scope_engine: true,
            add_dirs: true,
            crt_quote: true,
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
            label: "HTTP",
            prefix: "http",
            schema_flag: "--output-schema",
            reply_source: "",
            thread_flag: "",
            thread_noun: "thread",
            read_only_note: "",
            sandbox_record: "",
            tools_line: "",
            steps_flag: "",
            denial_retry: false,
            has_usage: true,
            prompt_by_file: false,
            transports: &["output-schema", "prompt-only"],
            default_mode: "new",
            fork: false,
            auth_modes: &[],
            mints_thread: false,
            max_prompt_bytes: 0,
            parallel_scope_engine: false,
            add_dirs: false,
            crt_quote: false,
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
    /// The ledger's `reviewer.provider_config`: `{builtin:"openai"}` (built-in, optionally with
    /// `base_url`/`base_url_source` when `OPENAI_BASE_URL` is set), or the raw provider-table
    /// echo for a user table (`Get-ProviderEndpoint`'s `Config`). `Null` until resolved.
    pub provider_config: serde_json::Value,
    /// (0.6.0, wave 29) The roster `auth` of an engine that takes one (claude: `subscription` |
    /// `api-key` | `endpoint`; the first is the default); `""` for every other engine.
    pub auth: String,
    /// (wave 29b, E1) The claude entry's endpoint (auth `endpoint` only).
    pub endpoint: Option<crate::claude::ClaudeEndpoint>,
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
            provider_config: serde_json::Value::Null,
            auth: String::new(),
            endpoint: None,
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
    resolve_reviewer_identity_auth(
        config,
        provider,
        model,
        openai_base_url,
        engine,
        launcher,
        "",
        None,
    )
}

/// `Resolve-ReviewerIdentity -Auth -Endpoint` (0.6.0, wave 29/29b): the identity of an engine that
/// takes a roster `auth` (claude) - `auth` `""` takes the engine's default (subscription), and
/// auth `endpoint` takes the roster entry's `endpoint`. Every other engine ignores both.
#[allow(clippy::too_many_arguments)]
pub fn resolve_reviewer_identity_auth(
    config: &CodexConfig,
    provider: &str,
    model: &str,
    openai_base_url: &str,
    engine: &str,
    launcher: &str,
    auth: &str,
    endpoint: Option<&crate::claude::ClaudeEndpoint>,
) -> ReviewerIdentity {
    let mut id = ReviewerIdentity::blank(&config.path);
    if !engine.is_empty() && engine != "codex" {
        return resolve_engine_identity(id, engine, provider, model, launcher, auth, endpoint);
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
            let mut pc = serde_json::Map::new();
            pc.insert("builtin".into(), serde_json::Value::String("openai".into()));
            if !openai_base_url.trim().is_empty() {
                let (curl, chost) = canonical_base_url(openai_base_url);
                let audit = config::audit_base_url(&curl);
                compat.push_str(&format!("|base_url={curl}"));
                id.host = chost;
                id.base_url = curl;
                pc.insert("base_url".into(), serde_json::Value::String(audit));
                pc.insert(
                    "base_url_source".into(),
                    serde_json::Value::String("OPENAI_BASE_URL".into()),
                );
            }
            id.provider_config = serde_json::Value::Object(pc);
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
                id.provider_config = ep.provider_config;
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
    auth: &str,
    endpoint: Option<&crate::claude::ClaudeEndpoint>,
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
    // (wave 29) an engine that takes a roster `auth` (claude): kept on the identity - the default
    // its first mode; (wave 29b, E5) auth endpoint takes the entry's endpoint (an error without one)
    if !spec.auth_modes.is_empty() {
        if !auth.is_empty() && !spec.auth_modes.contains(&auth) {
            id.error = format!(
                "the {engine} engine takes auth {} (got '{auth}')",
                spec.auth_modes.join(" or ")
            );
        }
        id.auth = if spec.auth_modes.contains(&auth) {
            auth.to_string()
        } else {
            spec.auth_modes[0].to_string()
        };
        if id.auth == "endpoint" {
            id.endpoint = endpoint.cloned();
            let why = crate::claude::endpoint_problem(endpoint);
            if !why.is_empty() && id.error.is_empty() {
                id.error = format!("the {engine} engine's auth endpoint is not usable: {why}");
            }
        }
    }
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
    // The ledger's `reviewer.provider_config` for a CLI engine (`Get-*IdentityConfig`): the
    // engine name and the resolved launcher, and — for muse (D4) — the credential mechanism the
    // billing guard requires (oauth). The http engine keeps a null config (its endpoint is a
    // provider table). Built via an insertion-ordered map (serde_json preserve_order) so the
    // muse keys stay `engine, launcher, credential_mechanism`.
    if engine == "agy" || engine == "muse" {
        let mut pc = serde_json::Map::new();
        pc.insert(
            "engine".into(),
            serde_json::Value::String(engine.to_string()),
        );
        pc.insert(
            "launcher".into(),
            serde_json::Value::String(launcher.to_string()),
        );
        if engine == "muse" {
            pc.insert(
                "credential_mechanism".into(),
                serde_json::Value::String("oauth".into()),
            );
        }
        id.provider_config = serde_json::Value::Object(pc);
    }
    // (wave 29, item 6) claude: `credential_mechanism` = the roster's auth, `auth_method` and
    // `api_provider` as `claude auth status` reported them (null here: the runtime fills them in
    // after its preflight, `Get-ClaudeIdentityConfig`); (wave 29b, E5) auth endpoint:
    // `credential_mechanism endpoint`, `base_url` (as written), `env_key` (the NAME) and `plan` (null
    // without one) - never the token.
    if engine == "claude" {
        let mut pc = serde_json::Map::new();
        pc.insert(
            "engine".into(),
            serde_json::Value::String(engine.to_string()),
        );
        pc.insert(
            "launcher".into(),
            serde_json::Value::String(launcher.to_string()),
        );
        pc.insert(
            "credential_mechanism".into(),
            serde_json::Value::String(id.auth.clone()),
        );
        if id.auth == "endpoint" {
            let ep = id.endpoint.as_ref();
            let s = |v: Option<String>| {
                v.map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null)
            };
            pc.insert("base_url".into(), s(ep.map(|e| e.base_url.clone())));
            pc.insert("env_key".into(), s(ep.map(|e| e.env_key.clone())));
            pc.insert(
                "plan".into(),
                s(ep.map(|e| e.plan.clone()).filter(|p| !p.is_empty())),
            );
        } else {
            pc.insert("auth_method".into(), serde_json::Value::Null);
            pc.insert("api_provider".into(), serde_json::Value::Null);
        }
        id.provider_config = serde_json::Value::Object(pc);
    }
    if id.error.is_empty() {
        id.resolved = true;
        // The http engine's endpoint is a provider, so its fingerprint must distinguish one
        // http provider from another; the CLI engines have a single endpoint (the engine
        // itself), so their compat string is fixed.
        let compat = if engine == "http" {
            format!("{}|provider={}", spec.compat_string, id.provider)
        } else if !spec.auth_modes.is_empty() && id.auth == "endpoint" {
            // (wave 29b, E5) the ROUTE: the canonical base URL and the token variable's NAME
            let ep = id.endpoint.as_ref();
            format!(
                "{}|endpoint|{}|{}",
                spec.compat_string,
                ep.map(|e| e.canonical.as_str()).unwrap_or(""),
                ep.map(|e| e.env_key.as_str()).unwrap_or("")
            )
        } else if !spec.auth_modes.is_empty() {
            // (D7) the engine + the auth mode + the model family (an Opus limit never marks
            // Sonnet or the API key out)
            let family = crate::claude::model_family(&id.model);
            format!(
                "{}|{}|{}",
                spec.compat_string,
                id.auth,
                if family.is_empty() {
                    crate::claude::model_base(&id.model)
                } else {
                    family
                }
            )
        } else {
            spec.compat_string.to_string()
        };
        id.compat_string = compat.clone();
        id.fingerprint = sha256_hex(compat.as_bytes());
    }
    id
}
