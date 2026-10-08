//! The `http` engine adapter (DESIGN §4 "API path", D4/D8): one OpenAI-compatible request
//! built from a retained reviewer pack, for OpenRouter or any endpoint that speaks
//! `chat/completions`.
//!
//! Unlike the subprocess engines, this reviewer never receives tools (DESIGN §3 invariant 2):
//! the whole context is the sanitized [`ReviewerPack`], sent as the user message with the reply
//! schema as the system message. Before the request is made, the exact pack is written next to
//! the handoff as `<stem>.pack.md` and `<stem>.pack.json` (the sidecar, extended with a
//! `request` section) so a later reader knows what the reviewer saw and how it was asked
//! ([`crate::pack::reviewer::sidecar_with_request`]); the pack path and content hash become the
//! `reference` a `read-code` finding cites in the v1 reply.
//!
//! Identity (DESIGN §4): the lineage is `provider::model::endpoint`; the conversation is a
//! C3-owned transcript id (never a native thread — [`Capabilities::resume`] is `false`), so a
//! returned conversation is [`ConversationTrust::Candidate`]. A continuation is *replay*
//! ([`Continuation::Replay`]): the retained pack, the prior assistant reply and the new prompt,
//! resent as three messages; a retry reuses the captured inputs without a redraw.
//!
//! Key contract (DESIGN §3 invariant 4): the credential is read from the environment only
//! ([`HttpConfig::key_env`]), never printed, stored, committed or transmitted anywhere but to
//! the provider. Every error string passes through [`scrub`] (the shared [`redact`] pass plus a
//! literal-key scrub), and the [`RequestPlan`]'s `Display` shows the `Authorization` header
//! redacted. A [`precheck`](HttpEngine::precheck) refuses an API key where a subscription engine
//! would otherwise be billed per token (the muse rule, generalized).
//!
//! Proxy (the Linux port): the agent honours the `HTTPS_PROXY` / `HTTP_PROXY` / `ALL_PROXY`
//! environment (`ureq`'s `try_proxy_from_env`), minus the hosts `NO_PROXY` names, and the request
//! event records whether a proxy was used (`proxy: true|false`). A second auth mode exists for a
//! sandbox whose egress proxy attaches the credential itself: when [`AUTH_PROXY_ENV`]
//! (`C3_HTTP_AUTH_PROXY`) lists the endpoint's host, C3 sends NO `Authorization` header and needs
//! NO key in the environment ([`HttpAuth::Proxy`]); the ledger's `provider_config` and the request
//! event carry `auth: proxy`. For every other host the key-to-host binding is unchanged. A
//! user-supplied `Proxy-Authorization` header stays refused (`roster_ext::header_name_problem`).

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use c3_core::engine::{
    AttemptOutcome, Capabilities, Continuation, ConversationId, ConversationTrust, Engine,
    EngineError, EngineKind, LaunchPlan, Reply, Request, SubprocessEngine, TurnRequest,
};
use c3_core::health::provider_failure_class;
use c3_core::ledger::{ProviderFailure, Usage};

use crate::pack::redact;
use crate::pack::reviewer::{self, ReviewerPack};

/// The default OpenRouter API base (DESIGN §4).
pub const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";
/// The default key environment variable (OpenRouter's own).
pub const DEFAULT_KEY_ENV: &str = "OPENROUTER_API_KEY";
/// The environment variable that lists the hosts (comma-separated, e.g. `openrouter.ai`) whose
/// credential an egress proxy attaches itself: for a host on the list C3 sends no `Authorization`
/// header and needs no key in the environment ([`HttpAuth::Proxy`]). A host matches exactly or as
/// a subdomain of a listed name (the same rule the known-key binding uses).
pub const AUTH_PROXY_ENV: &str = "C3_HTTP_AUTH_PROXY";

/// The environment variable naming a PEM file of EXTRA trust anchors for the TLS connection (the
/// Linux port): an egress proxy that re-terminates TLS presents a certificate from its own CA,
/// which the bundled Mozilla roots do not know. The anchors are ADDED to the bundled roots, never
/// replace them; unset means the bundled roots alone (the behaviour before the port). A file that
/// cannot be read or holds no certificate refuses the launch — never a silent fall-back.
pub const CA_BUNDLE_ENV: &str = "C3_HTTP_CA_BUNDLE";

/// The TLS client config for this process: the bundled roots plus, when [`CA_BUNDLE_ENV`] names a
/// file, every certificate in it. `Ok(None)` when the variable is unset (ureq's own default config
/// is used). The path is named in a refusal (it is not a secret); the file's contents never are.
pub fn tls_config_from_env() -> Result<Option<std::sync::Arc<rustls::ClientConfig>>, String> {
    let Some(path) = std::env::var_os(CA_BUNDLE_ENV).filter(|p| !p.is_empty()) else {
        return Ok(None);
    };
    tls_config_with_bundle(Path::new(&path)).map(Some)
}

/// Build a rustls client config from the bundled Mozilla roots plus the certificates of the PEM
/// file at `path` (the same `ring` provider and protocol versions ureq's default config uses).
pub fn tls_config_with_bundle(path: &Path) -> Result<std::sync::Arc<rustls::ClientConfig>, String> {
    use rustls::pki_types::pem::PemObject;
    use rustls::pki_types::CertificateDer;
    let mut roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let certs = CertificateDer::pem_file_iter(path).map_err(|e| {
        format!(
            "{CA_BUNDLE_ENV} names {}, which cannot be read ({})",
            path.display(),
            c3_core::one_line(&e.to_string())
        )
    })?;
    let mut added = 0usize;
    for cert in certs {
        let cert = cert.map_err(|_| {
            format!(
                "{CA_BUNDLE_ENV} names {}, which is not a PEM certificate bundle",
                path.display()
            )
        })?;
        roots.add(cert).map_err(|_| {
            format!(
                "{CA_BUNDLE_ENV} names {}, which holds a certificate that is not a usable trust anchor",
                path.display()
            )
        })?;
        added += 1;
    }
    if added == 0 {
        return Err(format!(
            "{CA_BUNDLE_ENV} names {}, which holds no certificate",
            path.display()
        ));
    }
    let config = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| {
        format!(
            "TLS configuration failed: {}",
            c3_core::one_line(&e.to_string())
        )
    })?
    .with_root_certificates(roots)
    .with_no_client_auth();
    Ok(std::sync::Arc::new(config))
}

/// How the request is authenticated: with the key from `key_env` as a bearer header, or by the
/// egress proxy on the way out (no header, no key read).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpAuth {
    /// `Authorization: Bearer <key from key_env>`.
    Key,
    /// No `Authorization` header: the proxy attaches the credential (`C3_HTTP_AUTH_PROXY`).
    Proxy,
}

impl HttpAuth {
    /// The ledger / events spelling: `key` or `proxy`.
    pub fn as_str(self) -> &'static str {
        match self {
            HttpAuth::Key => "key",
            HttpAuth::Proxy => "proxy",
        }
    }
}

/// The hosts `C3_HTTP_AUTH_PROXY` names: split on commas, trimmed, lower-cased, empties dropped.
pub fn proxy_auth_hosts_from(list: &str) -> Vec<String> {
    list.split(',')
        .map(|h| h.trim().trim_end_matches('.').to_ascii_lowercase())
        .filter(|h| !h.is_empty())
        .collect()
}

/// The hosts `C3_HTTP_AUTH_PROXY` names in this process's environment.
pub fn proxy_auth_hosts() -> Vec<String> {
    std::env::var(AUTH_PROXY_ENV)
        .map(|v| proxy_auth_hosts_from(&v))
        .unwrap_or_default()
}

/// Whether `host` is one of `hosts` (exact) or a subdomain of one. Case-insensitive; a trailing
/// dot on the host is ignored.
pub fn host_uses_proxy_auth(host: &str, hosts: &[String]) -> bool {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return false;
    }
    hosts
        .iter()
        .any(|h| host == *h || host.ends_with(&format!(".{h}")))
}

/// Whether a request to `url` goes through the proxy the environment names: `ALL_PROXY` or, by
/// the URL's scheme, `HTTPS_PROXY` / `HTTP_PROXY` (upper or lower case), unless `NO_PROXY` lists
/// the host. The agent is then built with `try_proxy_from_env`; the request event records the
/// answer as `proxy`.
pub fn env_proxy_applies(url: &str) -> bool {
    let parsed = match url::Url::parse(url) {
        Ok(u) => u,
        Err(_) => return false,
    };
    let host = parsed.host_str().unwrap_or("");
    let env = |name: &str| {
        std::env::var(name)
            .ok()
            .or_else(|| std::env::var(name.to_ascii_lowercase()).ok())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    proxy_decision(
        parsed.scheme(),
        host,
        env("ALL_PROXY").as_deref(),
        env("HTTPS_PROXY").as_deref(),
        env("HTTP_PROXY").as_deref(),
        env("NO_PROXY").as_deref(),
    )
}

/// The pure proxy rule behind [`env_proxy_applies`]: a non-empty `ALL_PROXY`, else the variable
/// of the scheme, selects a proxy; `NO_PROXY` (`*`, a host, or a domain suffix with or without a
/// leading dot, each optionally with a port) excludes the host. CIDR ranges are not understood.
pub(crate) fn proxy_decision(
    scheme: &str,
    host: &str,
    all_proxy: Option<&str>,
    https_proxy: Option<&str>,
    http_proxy: Option<&str>,
    no_proxy: Option<&str>,
) -> bool {
    let nonempty = |v: Option<&str>| v.map(|s| !s.trim().is_empty()).unwrap_or(false);
    let selected = match scheme {
        "https" => nonempty(all_proxy) || nonempty(https_proxy),
        "http" => nonempty(all_proxy) || nonempty(http_proxy),
        _ => false,
    };
    if !selected {
        return false;
    }
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    // Loopback is never proxied (a test mock, a local server): a proxy cannot reach it anyway.
    if host == "localhost" || host.starts_with("127.") || host == "[::1]" || host == "::1" {
        return false;
    }
    let Some(list) = no_proxy else {
        return true;
    };
    for entry in list.split(',') {
        let e = entry.trim().to_ascii_lowercase();
        if e.is_empty() {
            continue;
        }
        if e == "*" {
            return false;
        }
        // `host:port` → `host`; a bracketed IPv6 literal keeps its brackets.
        let e = if e.starts_with('[') {
            e.split("]:")
                .next()
                .map(|s| s.trim_end_matches(']'))
                .unwrap_or(&e)
                .to_string()
        } else {
            e.rsplit_once(':')
                .filter(|(_, p)| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
                .map(|(h, _)| h.to_string())
                .unwrap_or(e)
        };
        let e = e.trim_start_matches('.').trim_end_matches('.');
        if e.is_empty() {
            continue;
        }
        let bracketed = host.trim_start_matches('[').trim_end_matches(']');
        if host == e || bracketed == e || host.ends_with(&format!(".{e}")) {
            return false;
        }
    }
    true
}

/// Provider labels that name a *subscription* engine: sending an API key to one would bill
/// per token where a subscription (a signed-in CLI) is the intended, already-paid path. The
/// `http` engine refuses these in [`HttpEngine::precheck`] — the muse per-token guard,
/// generalized to the API path (DESIGN §3 invariant 4). Matched case-insensitively as a whole
/// label; `openrouter`, `openai`, `anthropic`, `google`, … (the API concentrators and labs) are
/// deliberately absent.
pub const SUBSCRIPTION_PROVIDERS: [&str; 5] = ["codex", "chatgpt", "muse", "agy", "antigravity"];

/// Configuration for one `http` reviewer. Everything the request needs except the key, which
/// is read from the environment at run time and never stored here.
#[derive(Debug, Clone)]
pub struct HttpConfig {
    /// The API base (no trailing `/chat/completions`); defaults to [`DEFAULT_BASE_URL`].
    pub base_url: String,
    /// The model id sent verbatim (e.g. `openai/gpt-5`).
    pub model: String,
    /// The environment variable the key is read from; defaults to [`DEFAULT_KEY_ENV`].
    pub key_env: String,
    /// Extra request headers (OpenRouter's `HTTP-Referer` / `X-Title` are optional). The
    /// `Authorization` and `content-type` headers are set by the engine and never taken here.
    pub headers: Vec<(String, String)>,
    /// The request timeout (connect and read).
    pub timeout: Duration,
    /// The provider label used for the lineage key and the subscription guard.
    pub provider_label: String,
    /// Send `response_format: {"type":"json_object"}` — set when the endpoint supports it.
    pub json_object: bool,
    /// The repository root, used to make the pack path in `provider_config` repo-relative
    /// (DESIGN §3 invariant 9). `None` falls back to the pack file name.
    pub repo_root: Option<PathBuf>,
}

impl Default for HttpConfig {
    fn default() -> Self {
        HttpConfig {
            base_url: DEFAULT_BASE_URL.to_string(),
            model: String::new(),
            key_env: DEFAULT_KEY_ENV.to_string(),
            headers: Vec::new(),
            timeout: Duration::from_secs(180),
            provider_label: "openrouter".to_string(),
            json_object: true,
            repo_root: None,
        }
    }
}

impl HttpConfig {
    /// The full `chat/completions` URL for this base.
    pub fn completions_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    /// The endpoint's host, lower-cased (empty when the base URL does not parse).
    pub fn host(&self) -> String {
        url::Url::parse(&self.base_url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
            .unwrap_or_default()
    }

    /// The auth mode of this endpoint: [`HttpAuth::Proxy`] when `C3_HTTP_AUTH_PROXY` lists its
    /// host (exactly or as a parent domain), else [`HttpAuth::Key`]. Read from the environment at
    /// call time; never a roster field or a flag, so an agent-writable file cannot switch a host
    /// to the header-less mode.
    pub fn auth_mode(&self) -> HttpAuth {
        if host_uses_proxy_auth(&self.host(), &proxy_auth_hosts()) {
            HttpAuth::Proxy
        } else {
            HttpAuth::Key
        }
    }
}

/// The runtime `http` engine: the resolved config, the retained pack, and the handoff stem the
/// pack files are written from (`<stem>.pack.md`, `<stem>.pack.json`).
#[derive(Debug, Clone)]
pub struct HttpEngine {
    pub config: HttpConfig,
    /// The sanitized reviewer pack this reviewer sees (built by [`crate::pack::reviewer::build`]).
    pub pack: ReviewerPack,
    /// The handoff path without extension; `.pack.md` / `.pack.json` are appended.
    pub handoff_stem: PathBuf,
}

/// A redactable view of the request headers: `Authorization` is shown as `Bearer [REDACTED]` in
/// any `Display`, so a plan can be logged without leaking the key (DESIGN §3 invariant 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestPlan {
    pub url: String,
    pub model: String,
    /// Header names in send order (`content-type`, `authorization`, then any config headers).
    /// The `authorization` value is never stored here — only the header names — so a plan can
    /// never carry the key.
    pub headers: Vec<String>,
    /// A one-line, key-free summary of the request body (`model=…, messages=N, json_object=…`).
    pub body_summary: String,
}

impl fmt::Display for RequestPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "POST {}", self.url)?;
        for h in &self.headers {
            if h.eq_ignore_ascii_case("authorization") {
                writeln!(f, "  {h}: Bearer [REDACTED]")?;
            } else {
                writeln!(f, "  {h}: <set>")?;
            }
        }
        write!(f, "  body: {}", self.body_summary)
    }
}

/// What one `http` attempt produced: the [`AttemptOutcome`], the retained pack file paths, and
/// the `provider_config` value the orchestrator places on the ledger's `reviewer`.
#[derive(Debug, Clone)]
pub struct HttpAttempt {
    pub outcome: AttemptOutcome,
    pub pack_md: PathBuf,
    pub pack_json: PathBuf,
    /// `{engine, base_url, model, pack, pack_sha256}` — built here, placed by the orchestrator.
    pub provider_config: Value,
    /// (item 2) Warnings produced by the attempt — the one `reply normalised: <list>` line when a
    /// near-valid reply was locally repaired into a structured object; empty otherwise.
    pub warnings: Vec<String>,
}

impl HttpEngine {
    /// The `<stem>.pack.md` path.
    pub fn pack_md_path(&self) -> PathBuf {
        append_ext(&self.handoff_stem, "pack.md")
    }

    /// The `<stem>.pack.json` sidecar path.
    pub fn pack_json_path(&self) -> PathBuf {
        append_ext(&self.handoff_stem, "pack.json")
    }

    /// The lineage key delegated to the core planner so it stays byte-identical.
    fn inner(&self) -> SubprocessEngine {
        SubprocessEngine::new(EngineKind::Http)
    }

    /// `env <X> set` / `env <X> not set` — a key-free diagnostic (never the value). In the proxy
    /// auth mode: `proxy (...)`, naming the listing variable and the host, never a value.
    pub fn key_status(&self) -> String {
        if self.config.auth_mode() == HttpAuth::Proxy {
            return format!(
                "proxy ({AUTH_PROXY_ENV} lists {}; no Authorization header is sent and no key is read)",
                self.config.host()
            );
        }
        match self.resolve_key() {
            Some(_) => format!("env {} set", self.config.key_env),
            None => format!("env {} not set", self.config.key_env),
        }
    }

    /// The header NAMES in send order: `content-type`, `authorization` (key mode only), then the
    /// config headers. Never a value.
    fn header_names(&self) -> Vec<String> {
        let mut names = vec!["content-type".to_string()];
        if self.config.auth_mode() == HttpAuth::Key {
            names.push("authorization".to_string());
        }
        for (k, _) in &self.config.headers {
            names.push(k.clone());
        }
        names
    }

    /// The key from the environment, or `None` when unset/empty. Never logged.
    fn resolve_key(&self) -> Option<String> {
        std::env::var(&self.config.key_env)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    }

    /// The richer request plan (url, header names, key-free body summary) used internally and
    /// available for logging. The core [`Engine::plan`] returns the shared
    /// [`c3_core::engine::HttpPlan`]; this
    /// carries the wire shape with the `Authorization` header redacted in any `Display`.
    pub fn request_plan(&self, turn: &TurnRequest) -> RequestPlan {
        let messages = self.messages(turn);
        RequestPlan {
            url: self.config.completions_url(),
            model: self.config.model.clone(),
            headers: self.header_names(),
            body_summary: format!(
                "model={}, messages={}, json_object={}",
                self.config.model,
                messages.len(),
                self.config.json_object
            ),
        }
    }

    /// The message array for this turn: a primary turn is `[system, user(pack)]`; a replay
    /// continuation is the three messages `[user(pack), assistant(prior_reply), user(prompt)]`
    /// (the pack already ends with the reply schema, so the contract travels with it and the
    /// replay needs no separate system message — DESIGN §4 "continuation = replay").
    fn messages(&self, turn: &TurnRequest) -> Vec<Value> {
        match &turn.continuation {
            Some(Continuation::Replay { prior_reply, .. }) => vec![
                json!({ "role": "user", "content": self.pack.content }),
                json!({ "role": "assistant", "content": prior_reply }),
                json!({ "role": "user", "content": turn.request.prompt }),
            ],
            _ => vec![
                json!({ "role": "system", "content": reviewer::system_prompt() }),
                json!({ "role": "user", "content": self.pack.content }),
            ],
        }
    }

    /// The request body for this turn.
    fn body(&self, turn: &TurnRequest, messages: &[Value]) -> Value {
        let mut body = json!({
            "model": self.config.model,
            "messages": messages,
        });
        if self.config.json_object {
            body["response_format"] = json!({ "type": "json_object" });
        }
        if let Some(effort) = turn
            .request
            .effort
            .as_ref()
            .filter(|e| !e.trim().is_empty())
        {
            body["reasoning"] = json!({ "effort": effort });
        }
        body
    }

    /// Redact any string that might carry the key: the shared [`redact`] pass (catches
    /// `sk-or-…`, bearer headers, JWTs, …) plus a literal replacement of this run's key value.
    fn scrub(&self, key: Option<&str>, s: &str) -> String {
        let (mut out, _) = redact::redact(s);
        if let Some(k) = key {
            if k.len() > 8 {
                out = out.replace(k, "[REDACTED:key]");
            }
        }
        out
    }

    /// Build the `provider_config` value for the ledger (`reviewer.provider_config`).
    fn provider_config(&self, pack_md: &Path) -> Value {
        let pack_rel = self
            .config
            .repo_root
            .as_ref()
            .and_then(|root| c3_core::paths::repo_relative(root, pack_md))
            .unwrap_or_else(|| {
                pack_md
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            });
        let mut config = json!({
            "engine": "http",
            "base_url": self.config.base_url,
            "model": self.config.model,
            "pack": pack_rel,
            "pack_sha256": c3_core::sha256_hex(self.pack.content.as_bytes()),
        });
        // The ledger names the header-less mode (`auth: proxy`); the key mode stays as it was.
        if self.config.auth_mode() == HttpAuth::Proxy {
            config["auth"] = Value::String(HttpAuth::Proxy.as_str().to_string());
        }
        config
    }

    /// Write the pack and its request-augmented sidecar (BEFORE the request is made).
    fn retain_pack(&self, request_info: Value) -> Result<(PathBuf, PathBuf), String> {
        let pack_md = self.pack_md_path();
        let pack_json = self.pack_json_path();
        if let Some(parent) = pack_md.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create {}: {e}", parent.display()))?;
        }
        std::fs::write(&pack_md, self.pack.content.as_bytes())
            .map_err(|e| format!("write {}: {e}", pack_md.display()))?;
        let sidecar = reviewer::sidecar_with_request(&self.pack.sidecar, request_info)?;
        std::fs::write(&pack_json, sidecar.as_bytes())
            .map_err(|e| format!("write {}: {e}", pack_json.display()))?;
        Ok((pack_md, pack_json))
    }

    /// Run (or replay) one attempt: retain the pack, POST the request, map the result. This is
    /// the full-fidelity entry point; the [`Engine`] trait methods return only its `outcome`.
    pub fn attempt(&self, turn: &TurnRequest) -> Result<HttpAttempt, EngineError> {
        // Guard the lineage/model exactly as the core planner does (also rejects an empty model).
        self.inner().plan(&turn.request)?;

        let messages = self.messages(turn);
        let body = self.body(turn, &messages);
        let body_str = serde_json::to_string(&body).unwrap_or_default();
        let url = self.config.completions_url();
        let prompt_sha = c3_core::sha256_hex(
            serde_json::to_string(&messages)
                .unwrap_or_default()
                .as_bytes(),
        );
        let request_info = json!({
            "url": url,
            "model": self.config.model,
            "response_format": if self.config.json_object { "json_object" } else { "none" },
            "prompt_sha256": prompt_sha,
        });

        // (STEP 2) A secondary turn (a format-repair replay or a timeout retry) appends to the same
        // events file and marks its events with the turn label; a primary turn starts it fresh and
        // is the one that (re)writes the retained pack.
        let label = turn_label(turn.kind);
        let fresh = label.is_none();

        // Retain the pack BEFORE the request (primary turn only), so a reader knows what was sent
        // even on a failure; a secondary turn reuses the pack already on disk.
        let (pack_md, pack_json) = if fresh {
            self.retain_pack(request_info)
                .map_err(EngineError::Precheck)?
        } else {
            (self.pack_md_path(), self.pack_json_path())
        };
        let provider_config = self.provider_config(&pack_md);

        // (item 3) The event stream for this attempt.
        let events_path = self.events_path();
        if fresh {
            let _ = std::fs::write(&events_path, b"");
        }

        // The key: read now, from the environment only, never logged. In the proxy auth mode no
        // key is read at all (the egress proxy attaches the credential).
        let auth = self.config.auth_mode();
        let key = match auth {
            HttpAuth::Proxy => None,
            HttpAuth::Key => match self.resolve_key() {
                Some(k) => Some(k),
                None => {
                    self.append_event(
                        &events_path,
                        &tag(
                            label,
                            json!({
                                "event": "error",
                                "class": "auth",
                                "message": format!("env {} not set", self.config.key_env),
                            }),
                        ),
                    );
                    return Ok(HttpAttempt {
                        outcome: AttemptOutcome::LaunchFailed {
                            child_exists: false,
                            message: format!("env {} not set", self.config.key_env),
                        },
                        pack_md,
                        pack_json,
                        provider_config,
                        warnings: Vec::new(),
                    });
                }
            },
        };

        // The TLS roots: the bundled ones, plus the `C3_HTTP_CA_BUNDLE` file when set. A bundle
        // that cannot be used refuses the launch (recorded, never a silent fall-back).
        let tls = match tls_config_from_env() {
            Ok(t) => t,
            Err(message) => {
                self.append_event(
                    &events_path,
                    &tag(
                        label,
                        json!({ "event": "error", "class": "transport", "message": message }),
                    ),
                );
                return Ok(HttpAttempt {
                    outcome: AttemptOutcome::LaunchFailed {
                        child_exists: false,
                        message,
                    },
                    pack_md,
                    pack_json,
                    provider_config,
                    warnings: Vec::new(),
                });
            }
        };

        // (item 3) The request event: header NAMES only, the body size in bytes, the pack hash,
        // the auth mode, whether the environment's proxy is used and whether extra trust anchors
        // are loaded — never the key, never a header value, never the body.
        let proxy = env_proxy_applies(&url);
        self.append_event(
            &events_path,
            &tag(
                label,
                json!({
                    "event": "request",
                    "method": "POST",
                    "url": strip_query(&url),
                    "model": self.config.model,
                    "messages": messages.len(),
                    "body_bytes": body_str.len(),
                    "pack_sha256": c3_core::sha256_hex(self.pack.content.as_bytes()),
                    "headers": self.header_names(),
                    "auth": auth.as_str(),
                    "proxy": proxy,
                    "ca_bundle": tls.is_some(),
                }),
            ),
        );

        let (outcome, warnings) = self.post(
            key.as_deref(),
            proxy,
            tls,
            &url,
            &body_str,
            &events_path,
            label,
        );
        Ok(HttpAttempt {
            outcome,
            pack_md,
            pack_json,
            provider_config,
            warnings,
        })
    }

    /// The `<stem>.events.jsonl` path (item 3): the request/response/error record for this attempt.
    pub fn events_path(&self) -> PathBuf {
        append_ext(&self.handoff_stem, "events.jsonl")
    }

    /// The `<stem>.original.json` path (STEP 1): the model's reply byte for byte, written whenever
    /// the normaliser changed the text so the repaired reply-of-record can still be checked against
    /// what the reviewer actually wrote (the plugin keeps `<stem>.original.md` for the same reason).
    pub fn original_json_path(&self) -> PathBuf {
        append_ext(&self.handoff_stem, "original.json")
    }

    /// Append one event as a JSON line (best-effort; a failed write never fails the run).
    fn append_event(&self, path: &Path, event: &Value) {
        use std::io::Write;
        if let Ok(line) = serde_json::to_string(event) {
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                let _ = writeln!(f, "{line}");
            }
        }
    }

    /// POST the request and map the response/error to an [`AttemptOutcome`] plus any warnings.
    /// (item 1) The wall clock stops only after the response BODY has been read (OpenRouter answers
    /// the headers at once and streams keep-alive whitespace while the model works). Every string
    /// that could carry the key is scrubbed, and each outcome writes its event line.
    #[allow(clippy::too_many_arguments)]
    fn post(
        &self,
        key: Option<&str>,
        proxy: bool,
        tls: Option<std::sync::Arc<rustls::ClientConfig>>,
        url: &str,
        body: &str,
        events_path: &Path,
        label: Option<&str>,
    ) -> (AttemptOutcome, Vec<String>) {
        let mut builder = ureq::AgentBuilder::new()
            .timeout_connect(self.config.timeout)
            .timeout(self.config.timeout)
            // (S4) Never follow a redirect: a 3xx would re-send the pack (project content) to
            // another host. A redirect is reported as a failure below, not chased.
            .redirects(0)
            // The environment's proxy (`HTTPS_PROXY` & co.) when it applies to this URL
            // (`env_proxy_applies`): a sandbox routes all egress through one.
            .try_proxy_from_env(proxy);
        // The bundled roots plus the `C3_HTTP_CA_BUNDLE` anchors, when set.
        if let Some(cfg) = tls {
            builder = builder.tls_config(cfg);
        }
        let agent = builder.build();
        let mut req = agent.post(url).set("content-type", "application/json");
        // The bearer header only in the key mode; the proxy mode sends no credential at all.
        if let Some(key) = key {
            req = req.set("authorization", &format!("Bearer {key}"));
        }
        for (k, v) in &self.config.headers {
            // (S5, defence in depth) Never let a reserved or malformed header name through, even
            // if one somehow reached the config past the roster validator.
            if c3_core::roster_ext::header_name_problem(k).is_none() {
                req = req.set(k, v);
            }
        }

        let started = Instant::now();
        let res = req.send_string(body);

        match res {
            Ok(resp) if (300..=399).contains(&resp.status()) => {
                // (S4) `redirects(0)` returns a 3xx as `Ok`; treat it as an unavailable endpoint.
                let status = resp.status();
                let wall = round1(started.elapsed().as_secs_f64());
                let failure = ProviderFailure {
                    class: "unavailable".to_string(),
                    code: status.to_string(),
                    message: "the endpoint answered with a redirect (not followed)".to_string(),
                    ..Default::default()
                };
                self.append_event(
                    events_path,
                    &tag(
                        label,
                        json!({ "event": "error", "class": failure.class,
                        "code": failure.code, "message": failure.message,
                        "elapsed_seconds": wall }),
                    ),
                );
                (
                    AttemptOutcome::ProviderFailure {
                        failure,
                        exit_code: None,
                    },
                    Vec::new(),
                )
            }
            Ok(resp) => {
                let status = resp.status();
                let req_id = resp
                    .header("x-request-id")
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                // (item 1) The body is read HERE; the clock stops after it.
                let text = resp.into_string().unwrap_or_default();
                let wall = round1(started.elapsed().as_secs_f64());
                self.parse_response(key, &text, wall, status, req_id, events_path, label)
            }
            Err(ureq::Error::Status(code, resp)) => {
                let retry_after = resp
                    .header("retry-after")
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                let body_text = resp.into_string().unwrap_or_default();
                let wall = round1(started.elapsed().as_secs_f64());
                let failure = self.classified_failure(key, Some(code), &body_text, retry_after);
                self.append_event(
                    events_path,
                    &tag(
                        label,
                        json!({ "event": "error", "class": failure.class,
                        "code": failure.code, "message": failure.message,
                        "elapsed_seconds": wall, "retry_after": failure.retry_after }),
                    ),
                );
                (
                    AttemptOutcome::ProviderFailure {
                        failure,
                        exit_code: None,
                    },
                    Vec::new(),
                )
            }
            Err(ureq::Error::Transport(t)) => {
                let wall = round1(started.elapsed().as_secs_f64());
                let message = self.scrub(key, &c3_core::one_line(&t.to_string()));
                if is_timeout(&message) {
                    self.append_event(
                        events_path,
                        &tag(
                            label,
                            json!({ "event": "error", "class": "unavailable",
                            "message": message, "elapsed_seconds": wall }),
                        ),
                    );
                    (
                        AttemptOutcome::TimedOut {
                            partial: None,
                            survivors: Vec::new(),
                            conversation: ConversationTrust::Candidate(new_conversation()),
                            wall_seconds: wall,
                        },
                        Vec::new(),
                    )
                } else {
                    let failure = ProviderFailure {
                        class: "transport".to_string(),
                        message,
                        ..Default::default()
                    };
                    self.append_event(
                        events_path,
                        &tag(
                            label,
                            json!({ "event": "error", "class": failure.class,
                            "message": failure.message, "elapsed_seconds": wall }),
                        ),
                    );
                    (
                        AttemptOutcome::ProviderFailure {
                            failure,
                            exit_code: None,
                        },
                        Vec::new(),
                    )
                }
            }
        }
    }

    /// (item 4) Build a classified [`ProviderFailure`] from an error — either a body
    /// `{"error":{code,message,metadata}}` envelope (which OpenRouter can return under HTTP 200) or
    /// a non-2xx HTTP status. The numeric code is taken from the body when present, else the HTTP
    /// status; the message is scrubbed and one line. A `retry_after` is taken from the header, else
    /// from the envelope's `metadata`.
    fn classified_failure(
        &self,
        key: Option<&str>,
        http_status: Option<u16>,
        body: &str,
        retry_after_header: Option<String>,
    ) -> ProviderFailure {
        let parsed: Option<Value> = serde_json::from_str(body).ok();
        let err = parsed
            .as_ref()
            .and_then(|v| v.get("error"))
            .filter(|e| !e.is_null());
        let body_code = err.and_then(|e| e.get("code")).and_then(json_i64);
        let em = err.and_then(|e| e.get("message")).and_then(Value::as_str);
        let meta_retry = err
            .and_then(|e| e.get("metadata"))
            .and_then(|m| {
                m.get("retry_after")
                    .or_else(|| m.get("retryAfter"))
                    .or_else(|| m.get("retry-after"))
            })
            .map(retry_to_string)
            .filter(|s| !s.is_empty());
        let code_num = body_code.or_else(|| http_status.map(|s| s as i64));
        let raw_msg = match (em, http_status) {
            (Some(e), _) => format!("provider error: {e}"),
            (None, Some(s)) => format!("HTTP {s}: {body}"),
            (None, None) => format!("provider error: {body}"),
        };
        let message = self.scrub(key, &c3_core::one_line(&raw_msg));
        ProviderFailure {
            class: classify_provider_failure(code_num, &message),
            code: code_num.map(|c| c.to_string()).unwrap_or_default(),
            message,
            retry_after: retry_after_header.or(meta_retry).filter(|s| !s.is_empty()),
            ..Default::default()
        }
    }

    /// Parse a 200 body: an `{"error":...}` envelope (OpenRouter returns these with 200) is a
    /// classified [`ProviderFailure`]; otherwise `choices[0].message.content` (falling back to
    /// `.reasoning`) is the reply text, parsed into a [`StructuredReply`] when it is one v1 JSON
    /// object (a fenced object is tolerated). (item 2) When the strict parse fails, a deterministic
    /// LOCAL normaliser runs — the http engine has no enforced output schema — and, when it makes
    /// the reply valid, records a `reply normalised: <list>` warning. Writes the response event and,
    /// for an error envelope, the error event.
    #[allow(clippy::too_many_arguments)]
    fn parse_response(
        &self,
        key: Option<&str>,
        text: &str,
        wall: f64,
        status: u16,
        req_id: Option<String>,
        events_path: &Path,
        label: Option<&str>,
    ) -> (AttemptOutcome, Vec<String>) {
        let json: Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(_) => {
                let message = self.scrub(
                    key,
                    &c3_core::one_line(&format!("non-JSON response: {text}")),
                );
                self.append_event(
                    events_path,
                    &tag(
                        label,
                        json!({ "event": "error",
                        "class": provider_failure_class(&message),
                        "message": message, "elapsed_seconds": wall }),
                    ),
                );
                return (
                    AttemptOutcome::ProviderFailure {
                        failure: ProviderFailure {
                            class: provider_failure_class(&message),
                            message,
                            ..Default::default()
                        },
                        exit_code: None,
                    },
                    Vec::new(),
                );
            }
        };

        // (item 3) The response event: status, the OpenRouter `x-request-id` (else the body `id`),
        // the body size and the elapsed seconds. Never a header value, never the body.
        let id = req_id.or_else(|| {
            json.get("id")
                .and_then(Value::as_str)
                .map(|s| s.to_string())
        });
        self.append_event(
            events_path,
            &tag(
                label,
                json!({ "event": "response", "status": status, "id": id,
                "body_bytes": text.len(), "elapsed_seconds": wall }),
            ),
        );

        if json.get("error").filter(|e| !e.is_null()).is_some() {
            let failure = self.classified_failure(key, Some(status), text, None);
            self.append_event(
                events_path,
                &tag(
                    label,
                    json!({ "event": "error", "class": failure.class,
                    "code": failure.code, "message": failure.message,
                    "elapsed_seconds": wall, "retry_after": failure.retry_after }),
                ),
            );
            return (
                AttemptOutcome::ProviderFailure {
                    failure,
                    exit_code: None,
                },
                Vec::new(),
            );
        }

        let content = json
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .and_then(|c| c.get("message"))
            .map(|m| {
                m.get("content")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .or_else(|| m.get("reasoning").and_then(Value::as_str))
                    .unwrap_or("")
            })
            .unwrap_or("")
            .to_string();

        // (item 2 / N1-N3) Strict parse first; only on failure does the local normaliser run.
        // On a successful repair the repaired JSON becomes the reply-of-record (`raw_text`), so the
        // orchestrator's strict re-parse of the reply succeeds and the finding delta is ingested;
        // the `normalised` event and the `reply normalised: <list>` warning are emitted only when
        // the normaliser actually changed the text (a non-empty note list).
        let mut warnings = Vec::new();
        let mut raw_text = content.clone();
        let structured = match crate::engines::codex::parse_structured(&content) {
            Some(s) => Some(s),
            None => match crate::consult::ingest::normalise_reply(&content) {
                crate::consult::ingest::Normalisation::Repaired { reply, json, notes } => {
                    if notes.is_empty() {
                        // No change was needed (unreachable after a failed strict parse).
                        raw_text = json;
                        Some(reply)
                    } else {
                        // (STEP 1 / F05-1) Preserve the model's EXACT bytes BEFORE the repaired text
                        // becomes the reply-of-record. If they cannot be written, the repaired text
                        // is NOT used: the reply stays as the model wrote it, recorded INVALID with
                        // the reason.
                        let original = self.original_json_path();
                        match std::fs::write(&original, content.as_bytes()) {
                            Ok(()) => {
                                let original_name = original
                                    .file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_default();
                                self.append_event(
                                    events_path,
                                    &tag(
                                        label,
                                        json!({ "event": "normalised", "notes": notes,
                                        "original": original_name }),
                                    ),
                                );
                                warnings.push(format!(
                                    "{}; the reviewer's own text: handoffs/{original_name}",
                                    crate::consult::ingest::normalised_note(&notes)
                                ));
                                raw_text = json;
                                Some(reply)
                            }
                            Err(e) => {
                                warnings.push(format!(
                                    "normalised text not used: the reviewer's own text could not be kept ({})",
                                    e.kind()
                                ));
                                None
                            }
                        }
                    }
                }
                // The reply stays INVALID; the orchestrator's summary keeps the strict error and
                // appends the normaliser's reason (via `ingest::first_validation_error`).
                crate::consult::ingest::Normalisation::Failed { .. } => None,
            },
        };

        (
            AttemptOutcome::Completed(Reply {
                raw_text,
                structured,
                events_path: self.events_path(),
                usage: parse_usage(&json),
                wall_seconds: wall,
                conversation: ConversationTrust::Candidate(new_conversation()),
            }),
            warnings,
        )
    }
}

impl Engine for HttpEngine {
    fn capabilities(&self) -> Capabilities {
        self.inner().capabilities()
    }

    fn plan(&self, request: &Request) -> Result<LaunchPlan, EngineError> {
        // Delegate to the core planner so the shared `LaunchPlan::Http(HttpPlan)` stays the
        // contract; `request_plan()` carries the richer, redactable wire view.
        self.inner().plan(request)
    }

    /// The launch guard (DESIGN §3 invariant 4): refuse an API key where a subscription engine
    /// would be billed per token (the muse rule), and refuse a launch with no key in the
    /// environment. Reports only whether the env var is set, never its value.
    fn precheck(&self, _turn: &TurnRequest) -> Result<(), EngineError> {
        let label = self.config.provider_label.trim().to_ascii_lowercase();
        if SUBSCRIPTION_PROVIDERS
            .iter()
            .any(|p| p.eq_ignore_ascii_case(&label))
        {
            return Err(EngineError::Precheck(format!(
                "refusing the http engine for provider `{}`: it is a subscription engine that would be billed per token on the API path; use its own engine instead",
                self.config.provider_label
            )));
        }
        // The proxy auth mode needs no key (the egress proxy attaches it); every other host does.
        if self.config.auth_mode() == HttpAuth::Key && self.resolve_key().is_none() {
            return Err(EngineError::Precheck(format!(
                "env {} not set: the http engine reads its key from the environment only",
                self.config.key_env
            )));
        }
        // An unusable `C3_HTTP_CA_BUNDLE` is refused before any request.
        tls_config_from_env()
            .map(|_| ())
            .map_err(EngineError::Precheck)
    }

    fn run(&self, turn: &TurnRequest) -> Result<AttemptOutcome, EngineError> {
        Ok(self.attempt(turn)?.outcome)
    }

    fn continue_turn(&self, turn: &TurnRequest) -> Result<AttemptOutcome, EngineError> {
        // Continuation is replay (the retained pack + the prior reply + the new prompt); the
        // message shaping is decided by `turn.continuation` in `messages()`.
        Ok(self.attempt(turn)?.outcome)
    }
}

// --------------------------------------------------------------------------- free helpers

/// Append a compound extension (`pack.md`) to a stem that has none.
fn append_ext(stem: &Path, ext: &str) -> PathBuf {
    let mut s = stem.as_os_str().to_os_string();
    s.push(".");
    s.push(ext);
    PathBuf::from(s)
}

/// A fresh client-owned conversation (transcript) id; `http` has no native thread.
fn new_conversation() -> ConversationId {
    ConversationId(uuid::Uuid::new_v4().to_string())
}

/// (item 4) Classify a provider failure by the numeric code (from the body envelope when present,
/// else the HTTP status) and the scrubbed message. 401/403 → auth; 402 → quota; 429 → burst; 408
/// and 5xx → unavailable; 400 with a context-length message → the `capability` class the other
/// engines use for an oversized brief; messages that say overloaded / unavailable / timeout →
/// unavailable. Everything else stays `unknown`.
fn classify_provider_failure(code: Option<i64>, message: &str) -> String {
    if let Some(c) = code {
        match c {
            401 | 403 => return "auth".to_string(),
            402 => return "quota".to_string(),
            429 => return "burst".to_string(),
            408 => return "unavailable".to_string(),
            500..=599 => return "unavailable".to_string(),
            400 if c3_core::health::is_context_overflow(message) => {
                return "capability".to_string()
            }
            _ => {}
        }
    }
    let m = message.to_ascii_lowercase();
    if m.contains("overloaded")
        || m.contains("unavailable")
        || m.contains("temporarily")
        || m.contains("timeout")
        || m.contains("timed out")
    {
        return "unavailable".to_string();
    }
    if c3_core::health::is_context_overflow(message) {
        return "capability".to_string();
    }
    "unknown".to_string()
}

/// Round to one decimal place (the ledger's wall-time precision).
fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// A JSON code as an i64: a number directly, or a numeric string (`"429"`).
fn json_i64(v: &Value) -> Option<i64> {
    v.as_i64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

/// A `retry_after` value as a string (a number of seconds, or a string), else empty.
fn retry_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

/// A URL with any query or fragment removed (the events file records the path only).
fn strip_query(url: &str) -> String {
    url.split(['?', '#']).next().unwrap_or(url).to_string()
}

/// (STEP 2) The events-file turn label for a turn kind: `None` for the primary turn (which starts
/// the events file fresh), a marker for a secondary turn (which appends and tags its events).
fn turn_label(kind: c3_core::engine::TurnKind) -> Option<&'static str> {
    match kind {
        c3_core::engine::TurnKind::Primary => None,
        c3_core::engine::TurnKind::FormatRepair => Some("format-repair"),
        c3_core::engine::TurnKind::TimeoutContinuation => Some("retry"),
        c3_core::engine::TurnKind::DenialRetry => Some("denial-retry"),
    }
}

/// (STEP 2) The pause before a timeout RETRY, or `None` when the failure is not retryable. Only an
/// `unavailable` failure (a request timeout, a 5xx, or an overloaded/unavailable answer — item 4)
/// is retried; `auth`, `quota` and `burst` are not. The pause is the provider's `retry_after` when
/// given (a burst 429 above 120 s is not retried, but that class is already excluded), else 20 s,
/// and never more than 120 s.
pub fn retry_pause(class: &str, retry_after: Option<&str>) -> Option<Duration> {
    if class != "unavailable" {
        return None;
    }
    let secs = retry_after
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(20)
        .min(120);
    Some(Duration::from_secs(secs))
}

/// Add the `turn` label to an event object when this is a secondary turn (a no-op for the primary).
fn tag(label: Option<&str>, mut v: Value) -> Value {
    if let (Some(l), Some(o)) = (label, v.as_object_mut()) {
        o.insert("turn".to_string(), Value::String(l.to_string()));
    }
    v
}

/// Whether a (already scrubbed) transport error message names a timeout. Matches the English
/// wording and, because the OS text is localized, the locale-independent OS error numbers:
/// `10060` (WSAETIMEDOUT, Windows), `110` (ETIMEDOUT, Linux), `60` (ETIMEDOUT, macOS).
fn is_timeout(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    m.contains("timed out")
        || m.contains("timeout")
        || m.contains("os error 10060")
        || m.contains("os error 110")
        || m.contains("os error 60")
}

/// Map an OpenAI-compatible `usage` object to [`Usage`].
fn parse_usage(json: &Value) -> Option<Usage> {
    let u = json.get("usage")?;
    let get = |name: &str| u.get(name).and_then(Value::as_i64).unwrap_or(0);
    let reasoning = u
        .get("completion_tokens_details")
        .and_then(|d| d.get("reasoning_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    Some(Usage {
        input_tokens: get("prompt_tokens"),
        cached_input_tokens: 0,
        output_tokens: get("completion_tokens"),
        reasoning_output_tokens: reasoning,
        total_tokens: u.get("total_tokens").and_then(Value::as_i64),
        extra: Default::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscription_guard_refuses_muse_label() {
        assert!(SUBSCRIPTION_PROVIDERS
            .iter()
            .any(|p| p.eq_ignore_ascii_case("MUSE")));
        assert!(!SUBSCRIPTION_PROVIDERS
            .iter()
            .any(|p| p.eq_ignore_ascii_case("openrouter")));
    }

    #[test]
    fn proxy_auth_host_list_is_parsed_and_matched_exactly_or_by_subdomain() {
        let hosts = proxy_auth_hosts_from(" openrouter.ai, ,API.Example.COM., ");
        assert_eq!(hosts, vec!["openrouter.ai", "api.example.com"]);
        assert!(host_uses_proxy_auth("openrouter.ai", &hosts));
        assert!(host_uses_proxy_auth("OpenRouter.AI.", &hosts));
        assert!(host_uses_proxy_auth("eu.openrouter.ai", &hosts));
        assert!(host_uses_proxy_auth("api.example.com", &hosts));
        // A suffix without the dot boundary, a look-alike and an empty host never match.
        assert!(!host_uses_proxy_auth("evilopenrouter.ai", &hosts));
        assert!(!host_uses_proxy_auth("openrouter.ai.evil.example", &hosts));
        assert!(!host_uses_proxy_auth("example.com", &hosts));
        assert!(!host_uses_proxy_auth("", &hosts));
        assert!(!host_uses_proxy_auth("openrouter.ai", &[]));
        assert!(proxy_auth_hosts_from("").is_empty());
    }

    #[test]
    fn auth_mode_follows_the_listing_variable_for_the_endpoint_host() {
        // The listing is read from the environment at call time; a host that is not listed stays
        // in the key mode, so the key-to-host binding is unchanged for every other endpoint.
        let listed = HttpConfig {
            base_url: "https://proxy-auth-unit.test/v1".to_string(),
            model: "m".to_string(),
            ..Default::default()
        };
        let other = HttpConfig {
            base_url: "https://keyed-unit.test/v1".to_string(),
            model: "m".to_string(),
            ..Default::default()
        };
        assert_eq!(listed.host(), "proxy-auth-unit.test");
        let prev = std::env::var(AUTH_PROXY_ENV).ok();
        std::env::set_var(AUTH_PROXY_ENV, "proxy-auth-unit.test");
        assert_eq!(listed.auth_mode(), HttpAuth::Proxy);
        assert_eq!(other.auth_mode(), HttpAuth::Key);
        match prev {
            Some(v) => std::env::set_var(AUTH_PROXY_ENV, v),
            None => std::env::remove_var(AUTH_PROXY_ENV),
        }
        assert_eq!(HttpAuth::Proxy.as_str(), "proxy");
        assert_eq!(HttpAuth::Key.as_str(), "key");
    }

    /// A public root (ISRG Root X1), used only to prove a PEM bundle loads; not a secret.
    const PUBLIC_ROOT_PEM: &str = "-----BEGIN CERTIFICATE-----
MIIFazCCA1OgAwIBAgIRAIIQz7DSQONZRGPgu2OCiwAwDQYJKoZIhvcNAQELBQAw
TzELMAkGA1UEBhMCVVMxKTAnBgNVBAoTIEludGVybmV0IFNlY3VyaXR5IFJlc2Vh
cmNoIEdyb3VwMRUwEwYDVQQDEwxJU1JHIFJvb3QgWDEwHhcNMTUwNjA0MTEwNDM4
WhcNMzUwNjA0MTEwNDM4WjBPMQswCQYDVQQGEwJVUzEpMCcGA1UEChMgSW50ZXJu
ZXQgU2VjdXJpdHkgUmVzZWFyY2ggR3JvdXAxFTATBgNVBAMTDElTUkcgUm9vdCBY
MTCCAiIwDQYJKoZIhvcNAQEBBQADggIPADCCAgoCggIBAK3oJHP0FDfzm54rVygc
h77ct984kIxuPOZXoHj3dcKi/vVqbvYATyjb3miGbESTtrFj/RQSa78f0uoxmyF+
0TM8ukj13Xnfs7j/EvEhmkvBioZxaUpmZmyPfjxwv60pIgbz5MDmgK7iS4+3mX6U
A5/TR5d8mUgjU+g4rk8Kb4Mu0UlXjIB0ttov0DiNewNwIRt18jA8+o+u3dpjq+sW
T8KOEUt+zwvo/7V3LvSye0rgTBIlDHCNAymg4VMk7BPZ7hm/ELNKjD+Jo2FR3qyH
B5T0Y3HsLuJvW5iB4YlcNHlsdu87kGJ55tukmi8mxdAQ4Q7e2RCOFvu396j3x+UC
B5iPNgiV5+I3lg02dZ77DnKxHZu8A/lJBdiB3QW0KtZB6awBdpUKD9jf1b0SHzUv
KBds0pjBqAlkd25HN7rOrFleaJ1/ctaJxQZBKT5ZPt0m9STJEadao0xAH0ahmbWn
OlFuhjuefXKnEgV4We0+UXgVCwOPjdAvBbI+e0ocS3MFEvzG6uBQE3xDk3SzynTn
jh8BCNAw1FtxNrQHusEwMFxIt4I7mKZ9YIqioymCzLq9gwQbooMDQaHWBfEbwrbw
qHyGO0aoSCqI3Haadr8faqU9GY/rOPNk3sgrDQoo//fb4hVC1CLQJ13hef4Y53CI
rU7m2Ys6xt0nUW7/vGT1M0NPAgMBAAGjQjBAMA4GA1UdDwEB/wQEAwIBBjAPBgNV
HRMBAf8EBTADAQH/MB0GA1UdDgQWBBR5tFnme7bl5AFzgAiIyBpY9umbbjANBgkq
hkiG9w0BAQsFAAOCAgEAVR9YqbyyqFDQDLHYGmkgJykIrGF1XIpu+ILlaS/V9lZL
ubhzEFnTIZd+50xx+7LSYK05qAvqFyFWhfFQDlnrzuBZ6brJFe+GnY+EgPbk6ZGQ
3BebYhtF8GaV0nxvwuo77x/Py9auJ/GpsMiu/X1+mvoiBOv/2X/qkSsisRcOj/KK
NFtY2PwByVS5uCbMiogziUwthDyC3+6WVwW6LLv3xLfHTjuCvjHIInNzktHCgKQ5
ORAzI4JMPJ+GslWYHb4phowim57iaztXOoJwTdwJx4nLCgdNbOhdjsnvzqvHu7Ur
TkXWStAmzOVyyghqpZXjFaH3pO3JLF+l+/+sKAIuvtd7u+Nxe5AW0wdeRlN8NwdC
jNPElpzVmbUq4JUagEiuTDkHzsxHpFKVK7q4+63SM1N95R1NbdWhscdCb+ZAJzVc
oyi3B43njTOQ5yOf+1CceWxG1bQVs5ZufpsMljq4Ui0/1lvh+wjChP4kqKOJ2qxq
4RgqsahDYVvTH9w7jXbyLeiNdd8XM2w9U/t7y0Ff/9yi0GE44Za4rF2LN9d11TPA
mRGunUHBcnWEvgJBQl9nJEiU0Zsnvgc/ubhPgXRR4Xq37Z0j4r7g1SgEEzwxA57d
emyPxgcYxn/eR44/KJ4EBs+lVDR3veyJm+kXQ99b21/+jh5Xos1AnX5iItreGCc=
-----END CERTIFICATE-----
";

    #[test]
    fn ca_bundle_adds_anchors_and_refuses_an_unusable_file() {
        let dir = std::env::temp_dir().join(format!("c3-ca-bundle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // A PEM bundle with one public root loads on top of the bundled roots.
        let good = dir.join("roots.pem");
        std::fs::write(&good, PUBLIC_ROOT_PEM).unwrap();
        assert!(tls_config_with_bundle(&good).is_ok());
        // A missing file, an empty file and a file without a certificate are refused, naming the
        // variable and the path (never the contents).
        let missing = dir.join("missing.pem");
        let err = tls_config_with_bundle(&missing).unwrap_err();
        assert!(
            err.contains(CA_BUNDLE_ENV) && err.contains("cannot be read"),
            "{err}"
        );
        let empty = dir.join("empty.pem");
        std::fs::write(&empty, "").unwrap();
        let err = tls_config_with_bundle(&empty).unwrap_err();
        assert!(err.contains("holds no certificate"), "{err}");
        let text = dir.join("text.pem");
        std::fs::write(&text, "not a certificate\n").unwrap();
        assert!(tls_config_with_bundle(&text).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn proxy_decision_follows_scheme_and_no_proxy() {
        let p = Some("http://127.0.0.1:3128");
        // The scheme picks the variable; ALL_PROXY covers both.
        assert!(proxy_decision(
            "https",
            "openrouter.ai",
            None,
            p,
            None,
            None
        ));
        assert!(!proxy_decision(
            "https",
            "openrouter.ai",
            None,
            None,
            p,
            None
        ));
        assert!(proxy_decision("http", "mock.test", None, None, p, None));
        assert!(!proxy_decision("http", "mock.test", None, p, None, None));
        assert!(proxy_decision("http", "mock.test", p, None, None, None));
        assert!(!proxy_decision(
            "https",
            "openrouter.ai",
            Some("  "),
            None,
            None,
            None
        ));
        assert!(!proxy_decision("ftp", "x", p, p, p, None));
        // NO_PROXY: a wildcard, an exact host, a domain suffix (with or without the dot), a port.
        for no in [
            "*",
            "openrouter.ai",
            ".openrouter.ai",
            "OPENROUTER.AI:443",
            "localhost,openrouter.ai",
        ] {
            assert!(
                !proxy_decision("https", "openrouter.ai", None, p, None, Some(no)),
                "NO_PROXY={no}"
            );
        }
        assert!(!proxy_decision(
            "https",
            "eu.openrouter.ai",
            None,
            p,
            None,
            Some("openrouter.ai")
        ));
        assert!(proxy_decision(
            "https",
            "openrouter.ai",
            None,
            p,
            None,
            Some("localhost,127.0.0.1")
        ));
        assert!(proxy_decision(
            "https",
            "evilopenrouter.ai",
            None,
            p,
            None,
            Some("openrouter.ai")
        ));
        assert!(!proxy_decision(
            "http",
            "127.0.0.1",
            p,
            None,
            None,
            Some("127.0.0.1:8080")
        ));
        assert!(!proxy_decision(
            "http",
            "[::1]",
            p,
            None,
            None,
            Some("[::1]:80")
        ));
        assert!(!proxy_decision(
            "http",
            "example.test",
            p,
            None,
            None,
            Some("example.test:8080")
        ));
        // Loopback is never proxied, whatever the variables say.
        for h in ["localhost", "127.0.0.1", "127.0.0.2", "[::1]"] {
            assert!(!proxy_decision("http", h, p, p, p, None), "{h}");
            assert!(!proxy_decision("https", h, p, p, p, None), "{h}");
        }
    }

    #[test]
    fn classify_provider_failure_maps_codes_and_messages() {
        assert_eq!(classify_provider_failure(Some(401), ""), "auth");
        assert_eq!(classify_provider_failure(Some(403), ""), "auth");
        assert_eq!(classify_provider_failure(Some(402), ""), "quota");
        assert_eq!(classify_provider_failure(Some(429), ""), "burst");
        assert_eq!(classify_provider_failure(Some(408), ""), "unavailable");
        assert_eq!(classify_provider_failure(Some(503), ""), "unavailable");
        // 400 with a context-length message is the oversized-brief class the other engines use.
        assert_eq!(
            classify_provider_failure(Some(400), "prompt is too long for this model"),
            "capability"
        );
        // A plain 400 is not context overflow.
        assert_eq!(
            classify_provider_failure(Some(400), "bad request"),
            "unknown"
        );
        // Message-based fallback: an overloaded/unavailable/timeout text with no useful code.
        assert_eq!(
            classify_provider_failure(Some(200), "Upstream error: Service temporarily overloaded"),
            "unavailable"
        );
        assert_eq!(classify_provider_failure(None, "nothing useful"), "unknown");
    }

    #[test]
    fn request_plan_display_redacts_authorization() {
        let plan = RequestPlan {
            url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
            model: "openai/gpt-5".to_string(),
            headers: vec!["content-type".into(), "authorization".into()],
            body_summary: "model=openai/gpt-5, messages=2, json_object=true".to_string(),
        };
        let shown = plan.to_string();
        assert!(shown.contains("Bearer [REDACTED]"));
        assert!(!shown.to_lowercase().contains("sk-or-"));
    }

    #[test]
    fn append_ext_builds_compound_extension() {
        assert_eq!(
            append_ext(Path::new("/t/01-http-slug"), "pack.md"),
            PathBuf::from("/t/01-http-slug.pack.md")
        );
        assert_eq!(
            append_ext(Path::new("/t/01-http-slug"), "pack.json"),
            PathBuf::from("/t/01-http-slug.pack.json")
        );
    }

    #[test]
    fn reserved_or_malformed_headers_are_skipped_by_the_adapter() {
        // (S5, defence in depth) `post` sets a config header only when
        // `roster_ext::header_name_problem` clears it — so a reserved or malformed name never
        // reaches the wire even if it somehow got past the roster validator.
        for h in [
            "authorization",
            "Proxy-Authorization",
            "Cookie",
            "Host",
            "Content-Length",
            "content-type",
            "Transfer-Encoding",
            "Bad Header",
            "bad:name",
        ] {
            assert!(
                c3_core::roster_ext::header_name_problem(h).is_some(),
                "{h} must be skipped by the adapter"
            );
        }
        for h in ["X-Title", "HTTP-Referer", "X-Custom"] {
            assert!(c3_core::roster_ext::header_name_problem(h).is_none());
        }
    }

    #[test]
    fn parse_usage_maps_openai_fields() {
        let v = json!({"usage":{"prompt_tokens":100,"completion_tokens":40,"total_tokens":140,
            "completion_tokens_details":{"reasoning_tokens":12}}});
        let u = parse_usage(&v).unwrap();
        assert_eq!(u.input_tokens, 100);
        assert_eq!(u.output_tokens, 40);
        assert_eq!(u.total_tokens, Some(140));
        assert_eq!(u.reasoning_output_tokens, 12);
    }

    #[test]
    fn retry_pause_only_for_unavailable_and_capped() {
        // Only `unavailable` is retried; auth/quota/burst are not.
        assert!(retry_pause("auth", None).is_none());
        assert!(retry_pause("quota", Some("30")).is_none());
        assert!(retry_pause("burst", Some("5")).is_none());
        // `unavailable` retries: the provider's retry_after when given, else 20 s, capped at 120 s.
        assert_eq!(
            retry_pause("unavailable", None),
            Some(Duration::from_secs(20))
        );
        assert_eq!(
            retry_pause("unavailable", Some("0")),
            Some(Duration::from_secs(0))
        );
        assert_eq!(
            retry_pause("unavailable", Some("45")),
            Some(Duration::from_secs(45))
        );
        assert_eq!(
            retry_pause("unavailable", Some("999")),
            Some(Duration::from_secs(120))
        );
    }

    #[test]
    fn turn_label_marks_only_secondary_turns() {
        use c3_core::engine::TurnKind;
        assert_eq!(turn_label(TurnKind::Primary), None);
        assert_eq!(turn_label(TurnKind::FormatRepair), Some("format-repair"));
        assert_eq!(turn_label(TurnKind::TimeoutContinuation), Some("retry"));
        // A secondary event is tagged; a primary event is untouched.
        let ev = tag(Some("retry"), json!({"event": "response"}));
        assert_eq!(ev["turn"], "retry");
        let ev = tag(None, json!({"event": "response"}));
        assert!(ev.get("turn").is_none());
    }
}
