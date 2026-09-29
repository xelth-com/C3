//! The C3 user configuration file (M11, `docs/port/m11-spec.md` §1).
//!
//! C3's only configuration file lives at the user level, beside the reviewer roster:
//! `<codex home>/c3/config.json` (`CODEX_HOME`, else `~/.codex`), overridable for tests with
//! `C3_CONFIG=<path>`; `C3_CONFIG=none` means "no configuration". Both features it configures
//! — index federation and local embeddings — are OFF unless a valid file turns them on.
//!
//! Validation fails CLOSED: a file that fails any check is not used at all and the command
//! says why in one line. Nothing in the file is a secret and nothing in it may be one — there
//! is no key/token/password field, so any unknown key (a typo, or a smuggled secret field) is
//! refused rather than silently ignored. A refusal never echoes a value other than a peer
//! `name` that already passed the slug check, so a pasted secret in a `conn`, a path or the
//! embedder URL can never leak into an error message.
//!
//! Parsing is manual (a `serde_json::Value` walk, like [`c3_core::roster_ext`]) precisely so
//! no serde type-mismatch error can echo an offending value. This module is feature-independent
//! (it builds with `--no-default-features`): it only parses connection strings and validates the
//! embedder URL; opening a peer or an embedder needs the runtime pieces elsewhere.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::index::federation::{PeerSelection, ResolvedPeer};
use crate::index::{self, embed, Backend};

/// The largest configuration file C3 will read (bytes).
const MAX_CONFIG_BYTES: u64 = 256 * 1024;
/// The most peers a configuration may declare.
const MAX_PEERS: usize = 16;

/// A validated configuration. Absent features are simply empty/`None`.
#[derive(Debug, Clone, Default)]
pub struct C3Config {
    pub peers: Vec<PeerConfig>,
    pub embedder: Option<EmbedderConfig>,
}

/// One validated peer entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerConfig {
    /// A slug, unique within the file — the one value a refusal may echo.
    pub name: String,
    pub backend: Backend,
    pub namespace: String,
    pub database: String,
    /// Canonicalised, case-folded project keys the peer is allowed for (never a wildcard).
    project_keys: Vec<String>,
    pub use_in_packs: bool,
}

/// The validated local-embedder entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbedderConfig {
    pub url: String,
    pub model: String,
    pub dimension: u32,
    project_keys: Vec<String>,
}

impl C3Config {
    /// The peers this project may use, in file order. A peer configured for other projects only
    /// is absent — its existence is not revealed.
    pub fn peers_for_project(&self, repo_root: &Path) -> Vec<&PeerConfig> {
        let key = project_key(repo_root);
        self.peers
            .iter()
            .filter(|p| p.project_keys.contains(&key))
            .collect()
    }

    /// The embedder for this project, when one is configured for it.
    pub fn embedder_for_project(&self, repo_root: &Path) -> Option<&EmbedderConfig> {
        let key = project_key(repo_root);
        self.embedder
            .as_ref()
            .filter(|e| e.project_keys.contains(&key))
    }

    /// Resolve the peers a command's selection names, restricted to those this project may use.
    /// `for_packs` additionally requires `use_in_packs`. A named peer that is not allowed for
    /// the project (or, for packs, not `use_in_packs`) is refused with a one-line message that
    /// echoes only the requested name. `--peers all` yields every allowed (and, for packs,
    /// `use_in_packs`) peer, and never errors on a name.
    pub fn resolve_selection(
        &self,
        repo_root: &Path,
        selection: &PeerSelection,
        for_packs: bool,
    ) -> Result<Vec<ResolvedPeer>, String> {
        let allowed = self.peers_for_project(repo_root);
        if selection.all {
            return Ok(allowed
                .into_iter()
                .filter(|p| !for_packs || p.use_in_packs)
                .map(PeerConfig::resolve)
                .collect());
        }
        let mut out = Vec::new();
        for name in &selection.names {
            let Some(peer) = allowed.iter().find(|p| &p.name == name) else {
                if !is_slug(name) {
                    return Err("no peer of that name is allowed for this project".to_string());
                }
                return Err(format!("no peer named {name} is allowed for this project"));
            };
            if for_packs && !peer.use_in_packs {
                return Err(format!(
                    "peer {name} is configured with use_in_packs: false, so it is not used in packs"
                ));
            }
            out.push(peer.resolve());
        }
        Ok(out)
    }
}

impl PeerConfig {
    fn resolve(&self) -> ResolvedPeer {
        ResolvedPeer {
            name: self.name.clone(),
            backend: self.backend.clone(),
            namespace: self.namespace.clone(),
            database: self.database.clone(),
            use_in_packs: self.use_in_packs,
        }
    }
}

/// Load and validate the configuration.
///
/// `Ok(None)` — there is no configuration (no file, or `C3_CONFIG=none`): every command must
/// behave exactly as before M11. `Ok(Some)` — a valid file. `Err(one line)` — a file exists but
/// failed validation (fail closed); the message never echoes a value but a slug-checked peer name.
pub fn load() -> Result<Option<C3Config>, String> {
    let path = match config_path() {
        Some(p) => p,
        None => return Ok(None), // C3_CONFIG=none
    };
    load_from(&path)
}

/// Load and validate from an explicit path (the file need not exist: a missing file is "no
/// configuration"). Used by [`load`] and by tests via `C3_CONFIG`.
pub fn load_from(path: &Path) -> Result<Option<C3Config>, String> {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return Ok(None), // no file → no configuration
    };
    if meta.len() > MAX_CONFIG_BYTES {
        return Err("c3 config: the file is larger than 256 KiB and was not used".to_string());
    }
    let bytes =
        std::fs::read(path).map_err(|_| "c3 config: the file could not be read".to_string())?;
    let root: Value = serde_json::from_str(&String::from_utf8_lossy(&bytes))
        .map_err(|_| "c3 config: the file is not valid JSON and was not used".to_string())?;
    validate(&root).map(Some)
}

/// The configuration path, or `None` for "no configuration" (`C3_CONFIG=none`). `C3_CONFIG`
/// overrides everything; otherwise it is `<codex home>/c3/config.json`.
fn config_path() -> Option<PathBuf> {
    if let Ok(v) = std::env::var("C3_CONFIG") {
        if v.eq_ignore_ascii_case("none") {
            return None;
        }
        if !v.trim().is_empty() {
            return Some(PathBuf::from(v));
        }
    }
    let home = crate::providers::get_codex_home();
    if home.is_empty() {
        return None;
    }
    Some(Path::new(&home).join("c3").join("config.json"))
}

/// The full manual validation over a parsed JSON value.
fn validate(root: &Value) -> Result<C3Config, String> {
    let obj = root
        .as_object()
        .ok_or_else(|| "c3 config: the top level must be a JSON object".to_string())?;
    check_keys(obj.keys(), &["config_version", "index"], "config")?;

    match obj.get("config_version").and_then(|v| v.as_u64()) {
        Some(1) => {}
        _ => return Err("c3 config: config_version must be 1".to_string()),
    }

    let mut cfg = C3Config::default();
    let Some(index) = obj.get("index") else {
        return Ok(cfg);
    };
    let index = index
        .as_object()
        .ok_or_else(|| "c3 config: index must be an object".to_string())?;
    check_keys(index.keys(), &["peers", "embedder"], "config.index")?;

    if let Some(peers) = index.get("peers") {
        cfg.peers = validate_peers(peers)?;
    }
    if let Some(embedder) = index.get("embedder") {
        cfg.embedder = Some(validate_embedder(embedder)?);
    }
    Ok(cfg)
}

fn validate_peers(peers: &Value) -> Result<Vec<PeerConfig>, String> {
    let arr = peers
        .as_array()
        .ok_or_else(|| "c3 config: index.peers must be an array".to_string())?;
    if arr.len() > MAX_PEERS {
        return Err(format!(
            "c3 config: at most {MAX_PEERS} peers are allowed (found {})",
            arr.len()
        ));
    }
    let mut out: Vec<PeerConfig> = Vec::new();
    for item in arr {
        let o = item
            .as_object()
            .ok_or_else(|| "c3 config: each peer must be an object".to_string())?;
        check_keys(
            o.keys(),
            &[
                "name",
                "conn",
                "namespace",
                "database",
                "projects",
                "use_in_packs",
            ],
            "a peer",
        )?;

        // name FIRST (a slug), so every later refusal for this peer may name it.
        let name = match o.get("name").and_then(|v| v.as_str()) {
            Some(s) if is_slug(s) => s.to_string(),
            _ => {
                return Err(
                    "c3 config: each peer needs a name that is a slug ([a-z0-9] and '-')"
                        .to_string(),
                )
            }
        };
        if out.iter().any(|p| p.name == name) {
            return Err(format!("c3 config: two peers share the name {name}"));
        }

        // conn: parsed by index::parse_conn, never `none`, never with userinfo.
        let conn = match o.get("conn").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            _ => return Err(format!("c3 config: peer {name} needs a conn string")),
        };
        let backend = index::parse_conn(&conn)
            .map_err(|_| format!("c3 config: peer {name} has an invalid conn"))?;
        match &backend {
            Backend::None => return Err(format!("c3 config: peer {name} conn must not be none")),
            Backend::Ws { url } => {
                if ws_has_userinfo(url) {
                    return Err(format!(
                        "c3 config: peer {name} conn must not contain a username or password"
                    ));
                }
            }
            Backend::SurrealKv(_) => {}
        }

        let namespace = non_empty_str(o.get("namespace"))
            .ok_or_else(|| format!("c3 config: peer {name} needs a namespace"))?;
        let database = non_empty_str(o.get("database"))
            .ok_or_else(|| format!("c3 config: peer {name} needs a database"))?;

        let project_keys = validate_projects(o.get("projects"))
            .map_err(|_| format!("c3 config: peer {name} needs a non-empty projects list of absolute repository paths"))?;

        let use_in_packs = match o.get("use_in_packs") {
            None => false,
            Some(Value::Bool(b)) => *b,
            Some(_) => {
                return Err(format!(
                    "c3 config: peer {name} use_in_packs must be true or false"
                ))
            }
        };

        out.push(PeerConfig {
            name,
            backend,
            namespace,
            database,
            project_keys,
            use_in_packs,
        });
    }
    Ok(out)
}

fn validate_embedder(embedder: &Value) -> Result<EmbedderConfig, String> {
    let o = embedder
        .as_object()
        .ok_or_else(|| "c3 config: index.embedder must be an object".to_string())?;
    check_keys(
        o.keys(),
        &["url", "model", "dimension", "projects"],
        "the embedder",
    )?;

    let url = match o.get("url").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        _ => return Err("c3 config: the embedder needs a url".to_string()),
    };
    // Loopback rule; the refusal never echoes the URL.
    embed::validate_embedder_url(&url).map_err(|why| format!("c3 config: {why}"))?;

    let model = non_empty_str(o.get("model"))
        .ok_or_else(|| "c3 config: the embedder needs a model name".to_string())?;

    let dimension = match o.get("dimension").and_then(|v| v.as_u64()) {
        Some(d) if (1..=100_000).contains(&d) => d as u32,
        _ => {
            return Err("c3 config: the embedder dimension must be a positive integer".to_string())
        }
    };

    let project_keys = validate_projects(o.get("projects")).map_err(|_| {
        "c3 config: the embedder needs a non-empty projects list of absolute repository paths"
            .to_string()
    })?;

    Ok(EmbedderConfig {
        url,
        model,
        dimension,
        project_keys,
    })
}

/// Validate a `projects` value: a non-empty array of absolute path strings, returned as
/// canonicalised, case-folded keys (a value never echoed on failure).
fn validate_projects(projects: Option<&Value>) -> Result<Vec<String>, ()> {
    let arr = projects.and_then(|v| v.as_array()).ok_or(())?;
    if arr.is_empty() {
        return Err(());
    }
    let mut keys = Vec::with_capacity(arr.len());
    for item in arr {
        let s = item.as_str().ok_or(())?;
        let p = Path::new(s);
        if !p.is_absolute() {
            return Err(());
        }
        keys.push(project_key(p));
    }
    Ok(keys)
}

/// The comparison key for a repository root: canonicalised when possible (else lexically
/// normalised), and case-folded on Windows. There is no wildcard: two keys match only when they
/// name the same directory.
fn project_key(path: &Path) -> String {
    let base = std::fs::canonicalize(path)
        .map(|c| c.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string_lossy().to_string());
    // Normalise separators and strip a Windows extended-length prefix so a canonicalised path
    // and a config literal compare equal.
    let mut s = base.replace('\\', "/");
    if let Some(rest) = s.strip_prefix("//?/") {
        s = rest.to_string();
    }
    while s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}

/// A `ws://`/`wss://` connection string that carries userinfo (`ws://user:pass@host`).
fn ws_has_userinfo(url: &str) -> bool {
    match url::Url::parse(url) {
        Ok(u) => !u.username().is_empty() || u.password().is_some(),
        // If it does not parse as a URL, fall back to spotting an `@` in the authority.
        Err(_) => url
            .trim_start_matches("wss://")
            .trim_start_matches("ws://")
            .split('/')
            .next()
            .map(|auth| auth.contains('@'))
            .unwrap_or(false),
    }
}

/// A slug: non-empty, at most 64 chars, `[a-z0-9-]`, starting with an alphanumeric.
fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Reject any key not in `allowed`. A typo (or a smuggled secret field) fails closed. The
/// message names the offending KEY (not a value), like [`c3_core::roster_ext`].
fn check_keys<'a>(
    keys: impl Iterator<Item = &'a String>,
    allowed: &[&str],
    at: &str,
) -> Result<(), String> {
    for k in keys {
        if !allowed.contains(&k.as_str()) {
            return Err(format!(
                "c3 config: {at} has an unknown key '{k}' (allowed: {})",
                allowed.join(", ")
            ));
        }
    }
    Ok(())
}

/// A non-empty string value without surrounding blanks.
fn non_empty_str(v: Option<&Value>) -> Option<String> {
    match v.and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() && s == s.trim() => Some(s.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, body: &str) -> PathBuf {
        let p = dir.join("config.json");
        std::fs::write(&p, body).unwrap();
        p
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("c3-cfg-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn missing_file_and_none_are_no_config() {
        let d = scratch("none");
        assert!(load_from(&d.join("does-not-exist.json")).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn valid_peer_and_embedder_parse() {
        let d = scratch("valid");
        let proj = d.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        let projs = proj.to_string_lossy().replace('\\', "/");
        let body = format!(
            r#"{{"config_version":1,"index":{{
              "peers":[{{"name":"hub","conn":"ws://127.0.0.1:8000","namespace":"ns","database":"idx",
                        "projects":["{projs}"],"use_in_packs":true}}],
              "embedder":{{"url":"http://127.0.0.1:11434/v1/embeddings","model":"nomic","dimension":768,
                          "projects":["{projs}"]}}
            }}}}"#
        );
        let p = write(&d, &body);
        let cfg = load_from(&p).unwrap().unwrap();
        assert_eq!(cfg.peers.len(), 1);
        assert_eq!(cfg.peers[0].name, "hub");
        assert!(cfg.peers[0].use_in_packs);
        assert_eq!(cfg.peers_for_project(&proj).len(), 1);
        assert!(cfg.embedder_for_project(&proj).is_some());
        // A different project sees neither peer nor embedder.
        let other = d.join("other");
        std::fs::create_dir_all(&other).unwrap();
        assert!(cfg.peers_for_project(&other).is_empty());
        assert!(cfg.embedder_for_project(&other).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn unknown_key_is_refused() {
        let d = scratch("unknown");
        let p = write(
            &d,
            r#"{"config_version":1,"index":{"peers":[],"secret":"x"}}"#,
        );
        let e = load_from(&p).unwrap_err();
        assert!(e.contains("unknown key 'secret'"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn bad_version_userinfo_and_projects() {
        let d = scratch("bad");
        // version
        let p = write(&d, r#"{"config_version":2,"index":{}}"#);
        assert!(load_from(&p)
            .unwrap_err()
            .contains("config_version must be 1"));
        // userinfo conn — the refusal must not echo the secret in the URL.
        let p = write(
            &d,
            r#"{"config_version":1,"index":{"peers":[{"name":"h","conn":"ws://user:sk-secret@127.0.0.1:8000","namespace":"n","database":"d","projects":["/abs/x"]}]}}"#,
        );
        let e = load_from(&p).unwrap_err();
        assert!(e.contains("username or password"), "{e}");
        assert!(!e.contains("sk-secret"), "secret leaked: {e}");
        // projects missing / empty / relative.
        let p = write(
            &d,
            r#"{"config_version":1,"index":{"peers":[{"name":"h","conn":"ws://127.0.0.1:8000","namespace":"n","database":"d","projects":[]}]}}"#,
        );
        assert!(load_from(&p).unwrap_err().contains("projects"));
        let p = write(
            &d,
            r#"{"config_version":1,"index":{"peers":[{"name":"h","conn":"ws://127.0.0.1:8000","namespace":"n","database":"d","projects":["relative/path"]}]}}"#,
        );
        assert!(load_from(&p).unwrap_err().contains("projects"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn cloud_embedder_refused_without_echo() {
        let d = scratch("cloud");
        let p = write(
            &d,
            r#"{"config_version":1,"index":{"embedder":{"url":"http://sk-leak.evil.example/v1/embeddings","model":"m","dimension":8,"projects":["/abs/x"]}}}"#,
        );
        let e = load_from(&p).unwrap_err();
        assert!(e.contains("cloud embedder is not supported"), "{e}");
        assert!(!e.contains("sk-leak"), "the url leaked: {e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn slug_rule() {
        assert!(is_slug("hub"));
        assert!(is_slug("xelixir-2"));
        assert!(!is_slug("Hub"));
        assert!(!is_slug("has space"));
        assert!(!is_slug(""));
        assert!(!is_slug("-lead"));
    }
}
