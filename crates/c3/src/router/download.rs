//! The priors cache and its once-a-day background refresh (M9 spec §3).
//!
//! The refresh runs where the telemetry flush runs and never blocks or fails a
//! consultation. It is fully injectable ([`Fetcher`]) so the tests exercise every branch
//! with no network: an `http://` URL is refused, `CODEX_CONSULT_TELEMETRY=off` and
//! `C3_PRIORS=off` skip the fetch entirely (the injected fetcher is never called), and a
//! signature that does not verify keeps the previous verified copy.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::priors::Priors;
use super::sign;

/// The default hub URL (provisional until the hub serves it; a 404 means "no priors").
pub const DEFAULT_URL: &str = "https://xelth.com/T/v2/priors.json";
/// Do not fetch more than once per 24 h.
const REFRESH_EVERY: chrono::Duration = chrono::Duration::hours(24);
/// A verified copy older than this is ignored (stale).
const MAX_AGE: chrono::Duration = chrono::Duration::days(30);
/// The 3 s total download budget (never blocks a consultation).
const BUDGET: Duration = Duration::from_secs(3);

/// Where priors are configured to come from, resolved from the environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Disabled: `C3_PRIORS=off` or `CODEX_CONSULT_TELEMETRY=off`.
    Off,
    /// A user-supplied local file, loaded without a signature (`C3_PRIORS=file:<path>`).
    File(PathBuf),
    /// The maintainer's signed hub (default, or an `https://` override).
    Hub(String),
    /// An `http://` (or otherwise non-`https`) override, refused.
    Refused(String),
}

/// Resolve the priors mode from the environment (§3 switches). `CODEX_CONSULT_TELEMETRY=off`
/// disables the hub entirely, so a user who turned telemetry off gets no hub traffic.
pub fn resolve_mode() -> Mode {
    if crate::telemetry::env_off() {
        return Mode::Off;
    }
    match std::env::var("C3_PRIORS")
        .ok()
        .map(|s| s.trim().to_string())
    {
        Some(v) if v.eq_ignore_ascii_case("off") => Mode::Off,
        Some(v) if v.is_empty() => Mode::Hub(DEFAULT_URL.to_string()),
        Some(v) => {
            if let Some(path) = v.strip_prefix("file:") {
                Mode::File(PathBuf::from(path))
            } else if v.starts_with("https://") {
                Mode::Hub(v)
            } else {
                Mode::Refused(v)
            }
        }
        None => Mode::Hub(DEFAULT_URL.to_string()),
    }
}

/// The priors cache directory: `<c3 home>/priors`, where the c3 home is the directory the
/// telemetry spool already uses (`<codex home>/c3`).
pub fn priors_dir() -> PathBuf {
    crate::telemetry::telemetry_dir()
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".codex").join("c3"))
        .join("priors")
}

/// `priors.meta.json`: how the cached copy was fetched. `fetched` is the last SUCCESS or 304
/// (the 30-day staleness clock); `attempted` is the last time the network was reached at all
/// (the 24 h cadence gate, F4), written even on a 404/failure so a dead hub is not polled every
/// consultation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Meta {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub fetched: String,
    #[serde(default)]
    pub attempted: String,
    #[serde(default)]
    pub etag: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub key_id: String,
}

impl Meta {
    fn read(dir: &Path) -> Option<Meta> {
        let bytes = std::fs::read(dir.join("priors.meta.json")).ok()?;
        serde_json::from_slice(&bytes).ok()
    }
}

/// A fetched HTTP response (the parts the refresh needs).
pub struct Fetched {
    pub status: u16,
    pub body: Vec<u8>,
    pub etag: Option<String>,
}

/// The network dependency, injected so tests never touch the network.
pub trait Fetcher {
    /// A plain GET with the c3 User-Agent, no cookies, no query string, within `timeout`.
    /// `etag` becomes an `If-None-Match` header. A `304` returns `status = 304`, empty body.
    fn get(&self, url: &str, etag: Option<&str>, timeout: Duration) -> Result<Fetched, String>;
}

/// The real ureq fetcher (`User-Agent: c3/<version>`, no redirects, `https` only). The
/// per-call `timeout` carries the remaining whole-refresh budget (F7).
pub struct UreqFetcher;

impl Fetcher for UreqFetcher {
    fn get(&self, url: &str, etag: Option<&str>, timeout: Duration) -> Result<Fetched, String> {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(timeout)
            .timeout(timeout)
            .redirects(0) // never follow a redirect, to any host (F3)
            .build();
        let ua = format!("c3/{}", env!("CARGO_PKG_VERSION"));
        let mut req = agent.get(url).set("User-Agent", &ua);
        if let Some(tag) = etag {
            req = req.set("If-None-Match", tag);
        }
        match req.call() {
            Ok(resp) => {
                let status = resp.status();
                let etag = resp.header("etag").map(|s| s.to_string());
                let mut body = Vec::new();
                use std::io::Read as _;
                resp.into_reader()
                    .take(super::priors::MAX_FILE_BYTES as u64 + 1)
                    .read_to_end(&mut body)
                    .map_err(|e| c3_core::one_line(&e.to_string()))?;
                Ok(Fetched { status, body, etag })
            }
            Err(ureq::Error::Status(code, _)) => Ok(Fetched {
                status: code,
                body: Vec::new(),
                etag: None,
            }),
            Err(e) => Err(c3_core::one_line(&e.to_string())),
        }
    }
}

/// What a refresh did, for the debug log and tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefreshOutcome {
    /// Disabled by a switch; the fetcher was not called.
    Disabled,
    /// A local `file:` source; nothing to download.
    LocalFile,
    /// A non-`https` override; refused, the fetcher was not called.
    Refused,
    /// Fetched too recently (within 24 h); the fetcher was not called.
    Skipped,
    /// The hub returned `304 Not Modified`; the cached copy stays.
    NotModified,
    /// A new verified copy was written to the cache.
    Updated,
    /// The hub returned no priors (a 404 is the normal answer today).
    NoPriors,
    /// The fetch or validation failed; the previous verified copy stays.
    Failed(String),
}

/// Refresh the cache at `dir` if due (§3). Never blocks or fails a consultation: any error
/// leaves the previous verified copy untouched and returns [`RefreshOutcome::Failed`].
pub fn refresh(dir: &Path, now: DateTime<Utc>, fetcher: &dyn Fetcher) -> RefreshOutcome {
    let url = match resolve_mode() {
        Mode::Off => return RefreshOutcome::Disabled,
        Mode::File(_) => return RefreshOutcome::LocalFile,
        Mode::Refused(_) => return RefreshOutcome::Refused,
        Mode::Hub(u) => u,
    };
    let prior = Meta::read(dir);
    let etag = prior.as_ref().and_then(|m| {
        if m.url == url && !m.etag.is_empty() {
            Some(m.etag.clone())
        } else {
            None
        }
    });
    // Cadence gate on the last ATTEMPT of THIS url (F4): a dead hub is not polled every run.
    if let Some(m) = &prior {
        if m.url == url {
            if let Ok(t) = DateTime::parse_from_rfc3339(&m.attempted) {
                if now.signed_duration_since(t.with_timezone(&Utc)) < REFRESH_EVERY {
                    return RefreshOutcome::Skipped;
                }
            }
        }
    }

    // From here, every path that reached the network records an attempt (F4).
    let start = Instant::now();
    let sig_url = format!("{url}.sig");
    let resp = match fetcher.get(&url, etag.as_deref(), BUDGET) {
        Ok(r) => r,
        Err(e) => {
            mark_attempt(dir, &url, now, prior);
            return RefreshOutcome::Failed(e);
        }
    };
    if resp.status == 304 {
        // Still fresh: advance both the success clock and the attempt clock.
        let mut m = prior.filter(|m| m.url == url).unwrap_or_else(|| Meta {
            url: url.clone(),
            ..Default::default()
        });
        let stamp = rfc3339(now);
        m.fetched = stamp.clone();
        m.attempted = stamp;
        let _ = std::fs::create_dir_all(dir);
        let _ = write_atomic(&dir.join("priors.meta.json"), &to_json(&m));
        return RefreshOutcome::NotModified;
    }
    mark_attempt(dir, &url, now, prior.clone());
    if resp.status == 404 {
        return RefreshOutcome::NoPriors;
    }
    if (300..400).contains(&resp.status) {
        return RefreshOutcome::Failed("hub answered with a redirect".to_string());
    }
    if !(200..300).contains(&resp.status) {
        return RefreshOutcome::Failed(format!("hub returned {}", resp.status));
    }
    // Validate before trusting (no downloaded bytes are echoed in the error).
    let priors = match Priors::validate(&resp.body) {
        Ok(p) => p,
        Err(e) => return RefreshOutcome::Failed(e.to_string()),
    };
    // The two requests must together fit the 3 s budget (F7).
    let remaining = match BUDGET.checked_sub(start.elapsed()) {
        Some(r) if !r.is_zero() => r,
        _ => return RefreshOutcome::Failed("budget exhausted".to_string()),
    };
    let sig = match fetcher.get(&sig_url, None, remaining) {
        Ok(r) if (200..300).contains(&r.status) => r.body,
        Ok(r) => return RefreshOutcome::Failed(format!("signature returned {}", r.status)),
        Err(e) => return RefreshOutcome::Failed(e),
    };
    if let Err(e) = sign::verify(&resp.body, &sig) {
        return RefreshOutcome::Failed(e.to_string());
    }
    let key_id = serde_json::from_slice::<sign::SigFile>(&sig)
        .map(|s| s.key_id)
        .unwrap_or_default();

    if std::fs::create_dir_all(dir).is_err() {
        return RefreshOutcome::Failed("could not create the priors cache dir".to_string());
    }
    let stamp = rfc3339(now);
    let meta = Meta {
        url: url.clone(),
        fetched: stamp.clone(),
        attempted: stamp,
        etag: resp.etag.unwrap_or_default(),
        sha256: sha256_hex(&resp.body),
        key_id,
    };
    if write_atomic(&dir.join("priors.json"), &resp.body).is_err()
        || write_atomic(&dir.join("priors.json.sig"), &sig).is_err()
        || write_atomic(&dir.join("priors.meta.json"), &to_json(&meta)).is_err()
    {
        return RefreshOutcome::Failed("could not write the priors cache".to_string());
    }
    let _ = priors; // validated above; the on-disk copy is the source of truth.
    RefreshOutcome::Updated
}

/// Record that the network was reached for `url` (F4): advance `attempted` while keeping the
/// last success's fields — but drop them if the cached meta was for a different url.
fn mark_attempt(dir: &Path, url: &str, now: DateTime<Utc>, prior: Option<Meta>) {
    let mut m = match prior {
        Some(m) if m.url == url => m,
        _ => Meta::default(),
    };
    m.url = url.to_string();
    m.attempted = rfc3339(now);
    let _ = std::fs::create_dir_all(dir);
    let _ = write_atomic(&dir.join("priors.meta.json"), &to_json(&m));
}

fn rfc3339(now: DateTime<Utc>) -> String {
    now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Load the verified priors from the cache at `dir` for the current mode (§3). Never fetches
/// and never blocks. Returns the priors and their source descriptor, or `None`.
pub fn load_cached(dir: &Path, now: DateTime<Utc>) -> Option<(Priors, PriorsSource)> {
    match resolve_mode() {
        Mode::Off | Mode::Refused(_) => None,
        Mode::File(path) => {
            // Refuse anything that is not a regular file, and cap the size before reading (F6).
            let md = std::fs::metadata(&path).ok()?;
            if !md.is_file() || md.len() > super::priors::MAX_FILE_BYTES as u64 {
                return None;
            }
            let bytes = std::fs::read(&path).ok()?;
            let priors = Priors::validate(&bytes).ok()?;
            let src = PriorsSource {
                source: "file",
                version: priors.priors_version,
                generated: priors.generated.clone(),
                sha256: sha256_hex(&bytes),
                key_id: String::new(),
            };
            Some((priors, src))
        }
        Mode::Hub(url) => {
            // Ignore a cache with no meta, or one written for a different hub url (F5).
            let meta = Meta::read(dir)?;
            if meta.url != url {
                return None;
            }
            let bytes = std::fs::read(dir.join("priors.json")).ok()?;
            let sig = std::fs::read(dir.join("priors.json.sig")).ok()?;
            let priors = Priors::validate(&bytes).ok()?;
            sign::verify(&bytes, &sig).ok()?;
            // Stale check: a verified copy older than 30 days is ignored.
            if let Ok(t) = DateTime::parse_from_rfc3339(&meta.fetched) {
                if now.signed_duration_since(t.with_timezone(&Utc)) > MAX_AGE {
                    return None;
                }
            } else {
                return None;
            }
            let src = PriorsSource {
                source: "hub",
                version: priors.priors_version,
                generated: priors.generated.clone(),
                sha256: if meta.sha256.is_empty() {
                    sha256_hex(&bytes)
                } else {
                    meta.sha256.clone()
                },
                key_id: meta.key_id.clone(),
            };
            Some((priors, src))
        }
    }
}

/// The provenance of a loaded `priors.json`, recorded in the draw's `ext`.
#[derive(Debug, Clone, Default)]
pub struct PriorsSource {
    pub source: &'static str,
    pub version: i64,
    pub generated: String,
    pub sha256: String,
    pub key_id: String,
}

fn to_json<T: Serialize>(v: &T) -> Vec<u8> {
    serde_json::to_vec_pretty(v).unwrap_or_default()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let d = Sha256::digest(bytes);
    let mut s = String::with_capacity(d.len() * 2);
    for b in d {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Write via a temp file and rename, so a reader never sees a half-written file.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}
