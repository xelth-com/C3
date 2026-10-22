//! `c3 update` — C3 checks for its own newer release and offers it (wave 11, hardened in 11b).
//!
//! Source of truth: the git tags of the C3 repository, read through the GitHub tags API as an
//! unauthenticated JSON array `[{"name":"v0.2.3","commit":{"sha":...}},...]`. The newest release is
//! the highest semantic version among tag names of the form `v<major>.<minor>.<patch>` (anything
//! else is ignored) and is compared with the running binary's `CARGO_PKG_VERSION`. The API's
//! `commit.sha` for that tag is the release's **identity**: `--apply` installs and synchronises
//! that exact commit, never a tag name (11b F97-1/F97-2).
//!
//! Nothing here ever changes anything on its own. The SessionStart hook only READS the cache,
//! refreshing it at most once a day within a 3 s budget and silently ([`hook_line`]); `c3 update`
//! reports ([`run_check_report`]); `c3 update --apply` is the only path that installs
//! ([`run_apply`]). It never follows a redirect, never sends anything but the GET, and never writes
//! outside `<codex home>/c3/`.
//!
//! The network is the injectable [`Fetcher`] (reused from the priors download, §2): the real ureq
//! implementation in production, a mock in the unit tests, and in `CODEX_CONSULT_TEST_MODE=1` a
//! reader of `C3_UPDATE_TEST_TAGS_FILE` that never opens a socket.

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::router::download::{Fetched, Fetcher, UreqFetcher};

/// The owner/repo the tags and the changelog come from.
pub const REPO: &str = "xelth-com/C3";
/// The unauthenticated GitHub tags API for [`REPO`].
pub const TAGS_URL: &str = "https://api.github.com/repos/xelth-com/C3/tags";
/// The canonical git remote `--apply` installs from and fetches the release tag from. Always this
/// literal, never a checkout's `origin` (11b F97-2).
pub const GIT_URL: &str = "https://github.com/xelth-com/C3.git";
/// The source descriptor recorded in the cache.
pub const SOURCE: &str = "github-tags";

/// No automatic attempt within 24 h of a successful check (`c3 update` forces one regardless).
const SUCCESS_EVERY: chrono::Duration = chrono::Duration::hours(24);
/// No automatic attempt within 1 h of a FAILED attempt (11b F97-3 backoff).
const FAIL_BACKOFF: chrono::Duration = chrono::Duration::hours(1);
/// The hook offers only from a successful response younger than this (older: the release may be
/// gone, so stay silent — 11b F97-3).
const OFFER_MAX_AGE: chrono::Duration = chrono::Duration::days(7);
/// The whole-check network budget (the hook never blocks past this).
const BUDGET: Duration = Duration::from_secs(3);

// --------------------------------------------------------------------------- version

/// A `<major>.<minor>.<patch>` release version. Field order is the comparison order, so the derived
/// `Ord` is semantic-version ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    /// The running binary's version (`CARGO_PKG_VERSION`, always a valid `x.y.z`).
    pub fn installed() -> Version {
        parse_core(env!("CARGO_PKG_VERSION")).unwrap_or(Version {
            major: 0,
            minor: 0,
            patch: 0,
        })
    }

    /// Render without a leading `v` (e.g. `0.2.3`).
    pub fn bare(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }

    /// Render with a leading `v` (e.g. `v0.2.3`), the tag spelling.
    pub fn tag(&self) -> String {
        format!("v{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Parse a tag NAME: `v<major>.<minor>.<patch>`, all parts numeric and nothing else. A `v`-less
/// name, a pre-release suffix, extra parts or non-digits yield `None` and are ignored when picking
/// the newest tag.
pub fn parse_tag(name: &str) -> Option<Version> {
    parse_core(name.strip_prefix('v')?)
}

fn parse_core(core: &str) -> Option<Version> {
    let mut it = core.split('.');
    let major = it.next()?.parse::<u64>().ok()?;
    let minor = it.next()?.parse::<u64>().ok()?;
    let patch = it.next()?.parse::<u64>().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some(Version {
        major,
        minor,
        patch,
    })
}

// --------------------------------------------------------------------------- the tags API

/// One entry of the GitHub tags array (only the fields we read).
#[derive(Debug, Clone, Deserialize)]
struct Tag {
    name: String,
    #[serde(default)]
    commit: TagCommit,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct TagCommit {
    #[serde(default)]
    sha: String,
}

/// The newest release among tag names of the form `v<x>.<y>.<z>` (others ignored): its version,
/// exact tag name and commit sha, or `None` when no tag parses.
fn pick_latest(tags: &[Tag]) -> Option<(Version, String, String)> {
    tags.iter()
        .filter_map(|t| parse_tag(&t.name).map(|v| (v, t.name.clone(), t.commit.sha.clone())))
        .max_by(|a, b| a.0.cmp(&b.0))
}

// --------------------------------------------------------------------------- status

/// What a check (fresh fetch or the cache) says about the installed vs the latest release.
#[derive(Debug, Clone)]
pub struct Status {
    pub installed: Version,
    pub latest: Option<Version>,
    pub latest_tag: Option<String>,
    pub latest_sha: Option<String>,
}

impl Status {
    /// A strictly newer release than the running binary exists.
    pub fn update_available(&self) -> bool {
        matches!(self.latest, Some(l) if l > self.installed)
    }

    /// The `v<latest>` tag to install/compare (falls back to the installed tag when no newer tag).
    fn latest_tag_str(&self) -> String {
        self.latest_tag
            .clone()
            .unwrap_or_else(|| self.installed.tag())
    }
}

/// A fetch failure, with a one-line reason safe to print (a status code or "offline"): never a
/// secret, never any downloaded bytes.
#[derive(Debug, Clone)]
pub struct FetchFail {
    pub reason: String,
}

/// Fetch the tags and compute the status (no cache write; the caller decides).
fn fetch_status(fetcher: &dyn Fetcher) -> Result<Status, FetchFail> {
    let resp = fetcher.get(TAGS_URL, None, BUDGET).map_err(|e| FetchFail {
        reason: if e.trim().is_empty() {
            "offline".to_string()
        } else {
            e
        },
    })?;
    if !(200..300).contains(&resp.status) {
        return Err(FetchFail {
            reason: format!("GitHub returned {}", resp.status),
        });
    }
    let tags: Vec<Tag> = serde_json::from_slice(&resp.body).map_err(|_| FetchFail {
        reason: "the tags response could not be parsed".to_string(),
    })?;
    let latest = pick_latest(&tags);
    Ok(Status {
        installed: Version::installed(),
        latest: latest.as_ref().map(|(v, _, _)| *v),
        latest_tag: latest.as_ref().map(|(_, n, _)| n.clone()),
        latest_sha: latest
            .as_ref()
            .map(|(_, _, s)| s.clone())
            .filter(|s| !s.is_empty()),
    })
}

/// The fetcher for the current environment: a no-socket reader in test mode, else real ureq.
fn default_fetcher() -> Box<dyn Fetcher> {
    if c3_core::test_hooks::mode_on() {
        Box::new(TestFetcher)
    } else {
        Box::new(UreqFetcher)
    }
}

/// In `CODEX_CONSULT_TEST_MODE=1` the network is replaced: this reads the tags-API JSON from
/// `C3_UPDATE_TEST_TAGS_FILE` when set, else reports "offline". No socket is ever opened.
struct TestFetcher;

impl Fetcher for TestFetcher {
    fn get(&self, _url: &str, _etag: Option<&str>, _timeout: Duration) -> Result<Fetched, String> {
        match std::env::var("C3_UPDATE_TEST_TAGS_FILE") {
            Ok(p) if !p.trim().is_empty() => {
                let body = std::fs::read(&p).map_err(|e| c3_core::one_line(&e.to_string()))?;
                Ok(Fetched {
                    status: 200,
                    body,
                    etag: None,
                })
            }
            _ => Err("offline".to_string()),
        }
    }
}

// --------------------------------------------------------------------------- the cache

/// `<codex home>/c3/update-check.json` (§3). `checked_at` is the last SUCCESS; `attempted_at` is
/// the last ATTEMPT (success or failure) — the two are separate so a failing network backs off
/// without discarding the last good answer (11b F97-3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCache {
    /// The last successful check (the 24 h cadence and the 7-day offer window).
    pub checked_at: String,
    /// The last attempt, success or failure (the 1 h failure backoff). Defaulted for old caches.
    #[serde(default)]
    pub attempted_at: String,
    /// The latest release as a bare version (`0.2.4`); empty when none was found.
    pub latest: String,
    pub latest_sha: String,
    /// The installed version when the cache was written (a bare version).
    pub installed: String,
    pub source: String,
}

/// `<codex home>/c3/update-check.json`. `None` when no codex home resolves.
fn cache_path() -> Option<PathBuf> {
    let home = codex_home();
    if home.as_os_str().is_empty() {
        return None;
    }
    Some(home.join("c3").join("update-check.json"))
}

/// The codex home, matching `providers`/`telemetry`: `CODEX_HOME`, else `~/.codex`.
fn codex_home() -> PathBuf {
    if let Ok(h) = std::env::var("CODEX_HOME") {
        if !h.is_empty() {
            return PathBuf::from(h);
        }
    }
    let home = std::env::var("HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("USERPROFILE").ok().filter(|s| !s.is_empty()));
    match home {
        Some(h) => PathBuf::from(h).join(".codex"),
        None => PathBuf::from(".codex"),
    }
}

fn read_cache() -> Option<UpdateCache> {
    let bytes = std::fs::read(cache_path()?).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Write the cache via a temp file and rename, so a reader never sees a half-written file.
/// Best-effort: a failure is silent (the check simply has no cache next time).
fn write_cache(c: &UpdateCache) {
    let Some(path) = cache_path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
    }
    let Ok(bytes) = serde_json::to_vec_pretty(c) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, &bytes).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

fn rfc3339(now: DateTime<Utc>) -> String {
    now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// A successful check: both `checked_at` and `attempted_at` advance to `now`.
fn cache_success(status: &Status, now: DateTime<Utc>) -> UpdateCache {
    let stamp = rfc3339(now);
    UpdateCache {
        checked_at: stamp.clone(),
        attempted_at: stamp,
        latest: status.latest.map(|v| v.bare()).unwrap_or_default(),
        latest_sha: status.latest_sha.clone().unwrap_or_default(),
        installed: status.installed.bare(),
        source: SOURCE.to_string(),
    }
}

/// A failed attempt: keep the previous success's fields (if any) and only advance `attempted_at`.
fn cache_attempt_failed(prev: Option<&UpdateCache>, now: DateTime<Utc>) -> UpdateCache {
    let mut c = prev.cloned().unwrap_or_else(|| UpdateCache {
        checked_at: String::new(),
        attempted_at: String::new(),
        latest: String::new(),
        latest_sha: String::new(),
        installed: Version::installed().bare(),
        source: SOURCE.to_string(),
    });
    c.attempted_at = rfc3339(now);
    c
}

fn status_from_cache(c: &UpdateCache) -> Status {
    let latest = parse_core(&c.latest);
    Status {
        installed: Version::installed(),
        latest,
        latest_tag: latest.map(|v| v.tag()),
        latest_sha: if c.latest_sha.is_empty() {
            None
        } else {
            Some(c.latest_sha.clone())
        },
    }
}

/// `now - stamp < dur` for an RFC3339 `stamp` (false when the stamp is empty or unparsable).
fn within(stamp: &str, now: DateTime<Utc>, dur: chrono::Duration) -> bool {
    DateTime::parse_from_rfc3339(stamp)
        .map(|t| now.signed_duration_since(t.with_timezone(&Utc)) < dur)
        .unwrap_or(false)
}

/// Whether the hook should attempt a network refresh now (11b F97-3): never within 24 h of a
/// success, never within 1 h of any attempt; otherwise yes.
fn should_attempt(cache: Option<&UpdateCache>, now: DateTime<Utc>) -> bool {
    let Some(c) = cache else {
        return true;
    };
    if within(&c.checked_at, now, SUCCESS_EVERY) {
        return false;
    }
    if within(&c.attempted_at, now, FAIL_BACKOFF) {
        return false;
    }
    true
}

/// Whether a cached SUCCESS is young enough to offer from (11b F97-3): the last success under 7
/// days old.
fn offerable(c: &UpdateCache, now: DateTime<Utc>) -> bool {
    within(&c.checked_at, now, OFFER_MAX_AGE)
}

// --------------------------------------------------------------------------- the off switch

/// The automatic check is ON by default; `C3_UPDATE_CHECK=off` or `"update_check": false` in the
/// C3 config turns it off. The environment wins; the config is consulted only when the environment
/// is unset. An unreadable/invalid config leaves the check enabled (its error surfaces elsewhere).
pub fn check_enabled() -> bool {
    if let Ok(v) = std::env::var("C3_UPDATE_CHECK") {
        if v.trim().eq_ignore_ascii_case("off") {
            return false;
        }
        return true;
    }
    match crate::c3config::load() {
        Ok(Some(cfg)) => cfg.update_check,
        _ => true,
    }
}

// --------------------------------------------------------------------------- the hook line

/// The hook's throttled daily check (item 5, 11b F97-3). Returns the update line to print, or
/// `None` (disabled, nothing on record, offer too old, up to date, or any error). Never panics,
/// never blocks past the 3 s budget, never writes outside `<codex home>/c3/`.
pub fn hook_line(now: DateTime<Utc>) -> Option<String> {
    if !check_enabled() {
        return None;
    }
    let cache = read_cache();
    // Refresh only when due; a failure records the attempt and keeps the previous success.
    let refreshed = if should_attempt(cache.as_ref(), now) {
        match fetch_status(default_fetcher().as_ref()) {
            Ok(s) => {
                let c = cache_success(&s, now);
                write_cache(&c);
                Some(c)
            }
            Err(_) => {
                write_cache(&cache_attempt_failed(cache.as_ref(), now));
                None
            }
        }
    } else {
        None
    };
    // Offer from the fresh success if we got one, else from the stored cache — but only when that
    // success is young enough (a stale offer may name a release that no longer exists).
    let effective = refreshed.or(cache);
    let c = effective.as_ref()?;
    if !offerable(c, now) {
        return None;
    }
    let status = status_from_cache(c);
    if status.update_available() {
        Some(format_hook_line(&status))
    } else {
        None
    }
}

/// The exact hook line (item 5).
pub fn format_hook_line(status: &Status) -> String {
    let latest = status.latest_tag_str();
    format!(
        "c3: update available {latest} (installed {}) - review: https://github.com/{REPO}/blob/{latest}/CHANGELOG.md ; apply: c3 update --apply",
        status.installed.bare()
    )
}

// --------------------------------------------------------------------------- install-kind + plan

/// The identity of a source checkout, captured at inspection and revalidated before the merge
/// (11b F97-1). `path` is git's own work-tree root (stable and git-accepted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckoutId {
    pub path: PathBuf,
    pub branch: String,
    pub head: String,
}

/// Whether a known source checkout can be fast-forwarded (item 7b), decided WITHOUT the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checkout {
    /// A clean working tree on a branch: a fast-forward will be attempted (carrying its identity).
    Ready { id: CheckoutId },
    /// Known but not fast-forwardable now, with the reason to print.
    Skip { path: PathBuf, why: String },
    /// No source checkout is known.
    Unknown,
}

/// Resolve the source checkout (item 7b): config `source_checkout`, else `C3_SOURCE_CHECKOUT`, else
/// the parent of `CLAUDE_PLUGIN_ROOT` when that parent is a git work tree whose
/// `plugin/.claude-plugin/plugin.json` names "c3". `None` when none is known.
pub fn source_checkout_path() -> Option<PathBuf> {
    if let Ok(Some(cfg)) = crate::c3config::load() {
        if let Some(p) = cfg.source_checkout {
            if !p.trim().is_empty() {
                return Some(PathBuf::from(p));
            }
        }
    }
    if let Ok(p) = std::env::var("C3_SOURCE_CHECKOUT") {
        if !p.trim().is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    if let Ok(root) = std::env::var("CLAUDE_PLUGIN_ROOT") {
        let parent = PathBuf::from(root).parent().map(Path::to_path_buf)?;
        if looks_like_c3_tree(&parent) {
            return Some(parent);
        }
    }
    None
}

/// A git work tree whose `plugin/.claude-plugin/plugin.json` names the "c3" plugin.
fn looks_like_c3_tree(dir: &Path) -> bool {
    if !dir.join(".git").exists() {
        return false;
    }
    let manifest = dir
        .join("plugin")
        .join(".claude-plugin")
        .join("plugin.json");
    let Ok(bytes) = std::fs::read(&manifest) else {
        return false;
    };
    serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(|s| s == "c3"))
        .unwrap_or(false)
}

/// Inspect a known checkout WITHOUT the network: clean + on a branch → [`Checkout::Ready`] with its
/// {path, branch, HEAD}; else [`Checkout::Skip`] with the reason (not a git repo / dirty / detached).
pub fn inspect_checkout(path: &Path) -> Checkout {
    // git's own work-tree root: stable, git-accepted, and it fails cleanly when this is not a repo.
    let root = match git(path, &["rev-parse", "--show-toplevel"]) {
        Ok(s) if !s.trim().is_empty() => PathBuf::from(s.trim()),
        _ => {
            return Checkout::Skip {
                path: path.to_path_buf(),
                why: "it is not a git work tree".into(),
            }
        }
    };
    match git(&root, &["status", "--porcelain"]) {
        Ok(s) if s.trim().is_empty() => {}
        Ok(_) => {
            return Checkout::Skip {
                path: root,
                why: "its working tree is not clean".into(),
            }
        }
        Err(_) => {
            return Checkout::Skip {
                path: root,
                why: "it is not a git work tree".into(),
            }
        }
    }
    let branch = match git(&root, &["symbolic-ref", "--short", "-q", "HEAD"]) {
        Ok(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => {
            return Checkout::Skip {
                path: root,
                why: "its HEAD is detached".into(),
            }
        }
    };
    let head = match git(&root, &["rev-parse", "HEAD"]) {
        Ok(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => {
            return Checkout::Skip {
                path: root,
                why: "its HEAD could not be read".into(),
            }
        }
    };
    Checkout::Ready {
        id: CheckoutId {
            path: root,
            branch,
            head,
        },
    }
}

/// Re-inspect a checkout and require it to be the same identity captured at planning: same path,
/// same branch, same HEAD, still clean (11b F97-1). `Err` names the precondition that changed.
fn revalidate(expect: &CheckoutId) -> Result<(), String> {
    match inspect_checkout(&expect.path) {
        Checkout::Ready { id } => {
            if id.path != expect.path {
                return Err(format!(
                    "the checkout path changed (was {}, now {})",
                    expect.path.display(),
                    id.path.display()
                ));
            }
            if id.branch != expect.branch {
                return Err(format!(
                    "the branch changed (was {}, now {})",
                    expect.branch, id.branch
                ));
            }
            if id.head != expect.head {
                return Err(format!(
                    "HEAD moved since the check (was {}, now {})",
                    expect.head, id.head
                ));
            }
            Ok(())
        }
        Checkout::Skip { why, .. } => Err(why),
        Checkout::Unknown => Err("the checkout is no longer usable".into()),
    }
}

/// One step `--apply` would run, in order (item 7). A pure value, so the plan is testable without
/// running anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// (a) `cargo install --git <GIT_URL> --rev <sha> --locked [--features index-surreal] c3-cli`
    /// — the API commit, never the tag name (11b F97-2).
    CargoInstall { rev: String, index_surreal: bool },
    /// (b) fetch the release tag from the canonical URL, verify its commit against the API sha,
    /// revalidate the checkout identity, then fast-forward to that commit.
    GitSync {
        id: CheckoutId,
        tag: String,
        rev: String,
    },
    /// (b) a known checkout that is NOT fast-forwardable now — printed and skipped, never fails.
    GitSyncSkipped { path: PathBuf, why: String },
    /// (c) `claude plugin update c3@c3` when `claude` is on PATH.
    PluginUpdate,
    /// (c) `claude` is not on PATH — print the command to run by hand.
    PluginUpdateManual,
}

impl Step {
    /// The command line this step runs (for the report and the apply log).
    pub fn command(&self) -> String {
        match self {
            Step::CargoInstall { rev, index_surreal } => {
                let feat = if *index_surreal {
                    " --features index-surreal"
                } else {
                    ""
                };
                format!("cargo install --git {GIT_URL} --rev {rev} --locked{feat} c3-cli")
            }
            Step::GitSync { id, tag, rev } => format!(
                "git -C {p} fetch {GIT_URL} refs/tags/{tag}:refs/c3-update/{tag} (verify == {rev}), then git -C {p} merge --ff-only {rev}",
                p = id.path.display()
            ),
            Step::GitSyncSkipped { path, why } => {
                format!("(skip the source checkout at {} - {why})", path.display())
            }
            Step::PluginUpdate => "claude plugin update c3@c3".to_string(),
            Step::PluginUpdateManual => {
                "claude plugin update c3@c3   (claude is not on PATH - run this by hand)"
                    .to_string()
            }
        }
    }
}

/// Build the ordered plan for a release (tag + API commit) and environment (item 7). Pure: all the
/// environment probing (the feature bundle, the checkout, `claude` on PATH) is passed in.
pub fn plan(
    tag: &str,
    rev: &str,
    index_surreal: bool,
    checkout: &Checkout,
    claude_on_path: bool,
) -> Vec<Step> {
    let mut steps = vec![Step::CargoInstall {
        rev: rev.to_string(),
        index_surreal,
    }];
    match checkout {
        Checkout::Ready { id } => steps.push(Step::GitSync {
            id: id.clone(),
            tag: tag.to_string(),
            rev: rev.to_string(),
        }),
        Checkout::Skip { path, why } => steps.push(Step::GitSyncSkipped {
            path: path.clone(),
            why: why.clone(),
        }),
        Checkout::Unknown => {}
    }
    steps.push(if claude_on_path {
        Step::PluginUpdate
    } else {
        Step::PluginUpdateManual
    });
    steps
}

/// Whether the running binary was built with the embedded index (§7a: the extra `--features`).
pub fn built_with_index_surreal() -> bool {
    cfg!(feature = "index-surreal")
}

/// A one-line description of the install kind for the report (item 6).
fn describe_install_kind(checkout: &Checkout, index_surreal: bool) -> String {
    let bundle = if index_surreal {
        "index-surreal bundle"
    } else {
        "light bundle"
    };
    match checkout {
        Checkout::Ready { id } => format!(
            "cargo install ({bundle}) + fast-forward the source checkout at {} (on {})",
            id.path.display(),
            id.branch
        ),
        Checkout::Skip { path, why } => format!(
            "cargo install ({bundle}); the source checkout at {} will be skipped ({why})",
            path.display()
        ),
        Checkout::Unknown => format!("cargo install ({bundle}); no source checkout known"),
    }
}

// --------------------------------------------------------------------------- git helpers / runner

/// Run git in `dir`, returning trimmed stdout on success (exit 0), `Err(stderr)` otherwise.
fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| c3_core::one_line(&e.to_string()))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(c3_core::one_line(&String::from_utf8_lossy(&out.stderr)))
    }
}

/// `git merge --ff-only <commit>` in `dir`: `Ok` on a fast-forward, `Err(reason)` otherwise.
fn git_ff_merge(dir: &Path, commit: &str) -> Result<(), String> {
    git(dir, &["merge", "--ff-only", commit]).map(|_| ())
}

/// Two commit ids name the same commit (full or abbreviated, case-insensitive).
fn commit_eq(a: &str, b: &str) -> bool {
    let a = a.trim().to_ascii_lowercase();
    let b = b.trim().to_ascii_lowercase();
    !a.is_empty() && !b.is_empty() && (a == b || a.starts_with(&b) || b.starts_with(&a))
}

/// Synchronise a checkout to the release commit (11b F97-1/F97-2), WITHOUT touching its `origin`:
/// fetch the release tag from `canonical` into a dedicated ref, verify the peeled commit equals
/// `rev` (the API sha), revalidate the checkout identity, then fast-forward to `rev`. `Ok(())` on a
/// completed fast-forward; `Err(reason)` on any skip (non-fatal). The temporary ref is deleted
/// afterwards (best effort). `canonical` is injectable only so the tests can stand up a local
/// "canonical" repo; production always passes [`GIT_URL`].
fn sync_checkout(id: &CheckoutId, canonical: &str, tag: &str, rev: &str) -> Result<(), String> {
    let temp_ref = format!("refs/c3-update/{tag}");
    let refspec = format!("refs/tags/{tag}:{temp_ref}");
    println!(
        "c3 update: $ git -C {} fetch {canonical} {refspec}",
        id.path.display()
    );
    let result = sync_after_fetch(id, canonical, &temp_ref, &refspec, rev);
    let _ = git(&id.path, &["update-ref", "-d", &temp_ref]); // best-effort cleanup
    result
}

fn sync_after_fetch(
    id: &CheckoutId,
    canonical: &str,
    temp_ref: &str,
    refspec: &str,
    rev: &str,
) -> Result<(), String> {
    // 1. fetch the release from the CANONICAL repository into a dedicated ref (never origin).
    git(&id.path, &["fetch", canonical, refspec]).map_err(|e| format!("git fetch failed ({e})"))?;
    // 2. peel the fetched tag to its commit.
    let peeled = git(&id.path, &["rev-parse", &format!("{temp_ref}^{{commit}}")])
        .map_err(|e| format!("could not resolve the fetched release tag ({e})"))?;
    // 3. bind to the API commit: a mismatch is never merged and never falls back to origin.
    if !commit_eq(peeled.trim(), rev) {
        return Err("release tag does not match the GitHub API commit".to_string());
    }
    // 4. revalidate the checkout identity immediately before the merge.
    revalidate(id)?;
    // 5. fast-forward to the verified commit (the sha, not the tag name).
    git_ff_merge(&id.path, rev).map_err(|e| format!("not a fast-forward ({e})"))?;
    Ok(())
}

/// `claude` resolvable on PATH (no execution).
fn claude_on_path() -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    let exts: &[&str] = if cfg!(windows) {
        &["exe", "cmd", "bat", ""]
    } else {
        &[""]
    };
    for dir in std::env::split_paths(&paths) {
        for ext in exts {
            let cand = if ext.is_empty() {
                dir.join("claude")
            } else {
                dir.join(format!("claude.{ext}"))
            };
            if cand.is_file() {
                return true;
            }
        }
    }
    false
}

/// Run a child, inheriting stdio so its output streams to the user.
fn stream(program: &str, args: &[&str], dir: Option<&Path>) -> Result<(), String> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    println!("c3 update: $ {program} {}", args.join(" "));
    let status = cmd
        .status()
        .map_err(|e| c3_core::one_line(&e.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "{program} exited with {}",
            status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "a signal".to_string())
        ))
    }
}

/// Execute one step, streaming the child's output. `Ok(true)` ran and succeeded; `Ok(false)` was
/// skipped (non-fatal: a known-dirty/detached checkout, a sha/identity mismatch, a non-ff, or
/// `claude` not on PATH); `Err` is a real command failure that stops the apply.
fn run_step(step: &Step) -> Result<bool, String> {
    match step {
        Step::CargoInstall { rev, index_surreal } => {
            let mut args = vec![
                "install",
                "--git",
                GIT_URL,
                "--rev",
                rev.as_str(),
                "--locked",
            ];
            if *index_surreal {
                args.push("--features");
                args.push("index-surreal");
            }
            args.push("c3-cli");
            stream("cargo", &args, None)?;
            Ok(true)
        }
        Step::GitSync { id, tag, rev } => {
            // (b) never fails (a): any skip reason is printed and the step is skipped.
            match sync_checkout(id, GIT_URL, tag, rev) {
                Ok(()) => {
                    println!(
                        "c3 update: fast-forwarded the source checkout at {} to {rev}",
                        id.path.display()
                    );
                    Ok(true)
                }
                Err(why) => {
                    println!("c3 update: skipped the source checkout fast-forward - {why}");
                    Ok(false)
                }
            }
        }
        Step::GitSyncSkipped { path, why } => {
            println!(
                "c3 update: skipped the source checkout at {} ({why})",
                path.display()
            );
            Ok(false)
        }
        Step::PluginUpdate => {
            stream("claude", &["plugin", "update", "c3@c3"], None)?;
            Ok(true)
        }
        Step::PluginUpdateManual => {
            println!("c3 update: claude is not on PATH - run `claude plugin update c3@c3` in a session to update the plugin");
            Ok(false)
        }
    }
}

// --------------------------------------------------------------------------- the subcommand

/// Resolve the checkout for a report/apply (probes git when a checkout is known).
fn resolve_checkout() -> Checkout {
    match source_checkout_path() {
        Some(p) => inspect_checkout(&p),
        None => Checkout::Unknown,
    }
}

/// Print the report block for a status, with an optional note appended to the `latest:` line (used
/// when a forced refresh failed and the data is from the cache). Returns whether an update exists.
fn print_report(status: &Status, note: Option<&str>) -> bool {
    let available = status.update_available();
    let tag = status.latest_tag_str();
    let index = built_with_index_surreal();
    let checkout = resolve_checkout();
    let suffix = note.map(|n| format!(" {n}")).unwrap_or_default();

    println!("c3 update");
    println!("  installed: {}", status.installed.bare());
    match status.latest {
        Some(l) => println!("  latest:    {} ({SOURCE}){suffix}", l.tag()),
        None => println!("  latest:    unknown (no release tag found){suffix}"),
    }
    if available {
        println!("  status:    an update is available");
        println!("  changelog: https://github.com/{REPO}/blob/{tag}/CHANGELOG.md");
    } else {
        println!("  status:    up to date");
    }
    println!("  install:   {}", describe_install_kind(&checkout, index));
    if available {
        let rev = status.latest_sha.clone().unwrap_or_default();
        println!("  --apply would run, stopping at the first failure:");
        for step in plan(&tag, &rev, index, &checkout, claude_on_path()) {
            println!("      {}", step.command());
        }
    }
    println!("  nothing was changed.");
    available
}

/// `c3 update` / `c3 update --check` (item 6): force a check now (ignoring the throttle), write the
/// cache, print the report, change nothing. Exit 0 up to date, 3 an update exists, 1 on a fetch
/// failure. On failure with a prior cache, the cached data is printed with a "refresh failed" note
/// (11b F97-3).
pub fn run_check_report(now: DateTime<Utc>) -> i32 {
    match fetch_status(default_fetcher().as_ref()) {
        Ok(s) => {
            write_cache(&cache_success(&s, now));
            if print_report(&s, None) {
                3
            } else {
                0
            }
        }
        Err(e) => {
            let prev = read_cache();
            write_cache(&cache_attempt_failed(prev.as_ref(), now));
            if let Some(c) = prev.as_ref().filter(|c| !c.checked_at.is_empty()) {
                let note = format!("(checked {}, refresh failed: {})", c.checked_at, e.reason);
                print_report(&status_from_cache(c), Some(&note));
            }
            eprintln!("c3 update: could not check for updates - {}", e.reason);
            1
        }
    }
}

/// `c3 update --apply` (item 7): force a check, then install. Refuse when up to date (exit 0) and
/// when the fetch failed (exit 1); otherwise run the plan, streaming output, stopping at the first
/// real failure (exit 1), and print what changed (exit 0).
pub fn run_apply(now: DateTime<Utc>) -> i32 {
    let status = match fetch_status(default_fetcher().as_ref()) {
        Ok(s) => {
            write_cache(&cache_success(&s, now));
            s
        }
        Err(e) => {
            write_cache(&cache_attempt_failed(read_cache().as_ref(), now));
            eprintln!("c3 update: could not check for updates - {}", e.reason);
            return 1;
        }
    };
    if !status.update_available() {
        println!(
            "c3 update: already at v{} - nothing to apply",
            status.installed.bare()
        );
        return 0;
    }
    // (11b F97-2) the install and the sync are bound to the API commit; without it we refuse.
    let rev = match status.latest_sha.clone().filter(|s| !s.is_empty()) {
        Some(r) => r,
        None => {
            eprintln!(
                "c3 update: the release commit sha is unknown - refusing to install without binding to a commit"
            );
            return 1;
        }
    };
    let tag = status.latest_tag_str();
    let checkout = resolve_checkout();
    let steps = plan(
        &tag,
        &rev,
        built_with_index_surreal(),
        &checkout,
        claude_on_path(),
    );
    println!(
        "c3 update: updating from v{} to {tag} ({rev})",
        status.installed.bare()
    );
    let mut ran: Vec<String> = Vec::new();
    for step in &steps {
        match run_step(step) {
            Ok(true) => ran.push(step.command()),
            Ok(false) => {} // skipped, non-fatal, already reported
            Err(e) => {
                eprintln!("c3 update: `{}` failed - {e}", step.command());
                eprintln!("c3 update: stopped; some steps may not have run");
                return 1;
            }
        }
    }
    println!("c3 update: done. Ran:");
    for r in &ran {
        println!("  - {r}");
    }
    println!("c3 update: start a new Claude Code session to pick up the updated plugin.");
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The env-touching tests share the process environment; they take turns.
    static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn scratch(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let p = std::env::temp_dir().join(format!("c3-upd-{tag}-{}-{nanos}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn v(major: u64, minor: u64, patch: u64) -> Version {
        Version {
            major,
            minor,
            patch,
        }
    }

    // ------------------------------------------------------------------- version parse/compare

    #[test]
    fn parse_only_vmajor_minor_patch() {
        assert_eq!(parse_tag("v0.2.3"), Some(v(0, 2, 3)));
        assert_eq!(parse_tag("v12.0.9"), Some(v(12, 0, 9)));
        // non-semver tag names are ignored
        assert_eq!(parse_tag("0.2.3"), None); // no leading v
        assert_eq!(parse_tag("v0.2"), None); // too few parts
        assert_eq!(parse_tag("v0.2.3.4"), None); // too many parts
        assert_eq!(parse_tag("v0.2.3-rc1"), None); // pre-release suffix
        assert_eq!(parse_tag("vnightly"), None);
        assert_eq!(parse_tag("latest"), None);
    }

    #[test]
    fn semver_ordering_and_latest() {
        assert!(v(0, 2, 10) > v(0, 2, 9));
        assert!(v(1, 0, 0) > v(0, 99, 99));
        assert!(v(0, 3, 0) > v(0, 2, 9));
        let tags = vec![
            Tag {
                name: "v0.2.3".into(),
                commit: TagCommit { sha: "aaa".into() },
            },
            Tag {
                name: "nightly".into(), // ignored
                commit: TagCommit::default(),
            },
            Tag {
                name: "v0.2.10".into(),
                commit: TagCommit { sha: "ccc".into() },
            },
            Tag {
                name: "v0.2.9".into(),
                commit: TagCommit { sha: "bbb".into() },
            },
        ];
        let (ver, name, sha) = pick_latest(&tags).unwrap();
        assert_eq!(ver, v(0, 2, 10));
        assert_eq!(name, "v0.2.10");
        assert_eq!(sha, "ccc");
        // no semver tag at all
        assert!(pick_latest(&[Tag {
            name: "nightly".into(),
            commit: TagCommit::default()
        }])
        .is_none());
    }

    #[test]
    fn hook_line_shape() {
        let s = Status {
            installed: v(0, 2, 3),
            latest: Some(v(0, 2, 4)),
            latest_tag: Some("v0.2.4".into()),
            latest_sha: Some("deadbeef".into()),
        };
        assert!(s.update_available());
        assert_eq!(
            format_hook_line(&s),
            "c3: update available v0.2.4 (installed 0.2.3) - review: https://github.com/xelth-com/C3/blob/v0.2.4/CHANGELOG.md ; apply: c3 update --apply"
        );
        let same = Status {
            installed: v(0, 2, 4),
            latest: Some(v(0, 2, 4)),
            latest_tag: Some("v0.2.4".into()),
            latest_sha: None,
        };
        assert!(!same.update_available());
    }

    // ------------------------------------------------------------------- the plan (incl. F97-2)

    #[test]
    fn plan_for_each_install_kind() {
        // no checkout, light bundle, claude present
        assert_eq!(
            plan("v9.9.9", "abc123", false, &Checkout::Unknown, true),
            vec![
                Step::CargoInstall {
                    rev: "abc123".into(),
                    index_surreal: false
                },
                Step::PluginUpdate,
            ]
        );
        // index-surreal bundle, claude absent → manual plugin step and the --features flag
        let steps = plan("v9.9.9", "abc123", true, &Checkout::Unknown, false);
        assert_eq!(steps.last(), Some(&Step::PluginUpdateManual));
        // (F97-2 closing) the cargo step pins the API COMMIT, never the tag name.
        let cargo = steps[0].command();
        assert_eq!(
            cargo,
            "cargo install --git https://github.com/xelth-com/C3.git --rev abc123 --locked --features index-surreal c3-cli"
        );
        assert!(cargo.contains("--rev abc123"), "{cargo}");
        assert!(
            !cargo.contains("--tag"),
            "fails-before: wave 11 used --tag: {cargo}"
        );
        // a clean checkout → a GitSync step between install and plugin
        let ready = Checkout::Ready {
            id: CheckoutId {
                path: PathBuf::from("/src/c3"),
                branch: "main".into(),
                head: "ffff".into(),
            },
        };
        let steps = plan("v9.9.9", "abc123", false, &ready, true);
        assert!(matches!(steps[1], Step::GitSync { .. }));
        // a dirty/detached checkout → a GitSyncSkipped step (planned, not fatal)
        let dirty = Checkout::Skip {
            path: PathBuf::from("/src/c3"),
            why: "its working tree is not clean".into(),
        };
        let steps = plan("v9.9.9", "abc123", false, &dirty, true);
        assert!(matches!(steps[1], Step::GitSyncSkipped { .. }));
    }

    // ------------------------------------------------------------------- git checkout inspection

    fn git_run(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn git_out(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn init_repo(dir: &Path) {
        git_run(dir, &["init", "-q", "-b", "main"]);
        git_run(dir, &["config", "user.email", "t@example.com"]);
        git_run(dir, &["config", "user.name", "t"]);
        git_run(dir, &["config", "commit.gpgsign", "false"]);
        std::fs::write(dir.join("a.txt"), "one\n").unwrap();
        git_run(dir, &["add", "."]);
        git_run(dir, &["commit", "-q", "-m", "one"]);
    }

    fn head_of(dir: &Path) -> String {
        git_out(dir, &["rev-parse", "HEAD"])
    }

    #[test]
    fn inspect_clean_dirty_detached_and_non_repo() {
        // not a repo
        let empty = scratch("norepo");
        assert!(matches!(inspect_checkout(&empty), Checkout::Skip { .. }));

        let repo = scratch("clean");
        init_repo(&repo);
        match inspect_checkout(&repo) {
            Checkout::Ready { id } => {
                assert_eq!(id.branch, "main");
                assert_eq!(id.head, head_of(&repo));
            }
            other => panic!("expected Ready, got {other:?}"),
        }

        // dirty
        std::fs::write(repo.join("a.txt"), "two\n").unwrap();
        match inspect_checkout(&repo) {
            Checkout::Skip { why, .. } => assert!(why.contains("not clean"), "{why}"),
            other => panic!("expected dirty Skip, got {other:?}"),
        }
        // detached HEAD (clean again first)
        git_run(&repo, &["checkout", "-q", "--", "a.txt"]);
        let head = head_of(&repo);
        git_run(&repo, &["checkout", "-q", &head]);
        match inspect_checkout(&repo) {
            Checkout::Skip { why, .. } => assert!(why.contains("detached"), "{why}"),
            other => panic!("expected detached Skip, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&empty);
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn ff_merge_succeeds_and_non_ff_is_detected() {
        // fast-forward: main behind a tag that is a descendant
        let repo = scratch("ff");
        init_repo(&repo);
        std::fs::write(repo.join("a.txt"), "two\n").unwrap();
        git_run(&repo, &["add", "."]);
        git_run(&repo, &["commit", "-q", "-m", "two"]);
        git_run(&repo, &["tag", "v2.0.0"]);
        git_run(&repo, &["reset", "--hard", "-q", "HEAD~1"]);
        assert!(git_ff_merge(&repo, "v2.0.0").is_ok());

        // non-fast-forward: a tag on a divergent commit
        let repo2 = scratch("nonff");
        init_repo(&repo2);
        git_run(&repo2, &["checkout", "-q", "-b", "side"]);
        std::fs::write(repo2.join("b.txt"), "side\n").unwrap();
        git_run(&repo2, &["add", "."]);
        git_run(&repo2, &["commit", "-q", "-m", "side"]);
        git_run(&repo2, &["tag", "v3.0.0"]);
        git_run(&repo2, &["checkout", "-q", "main"]);
        std::fs::write(repo2.join("a.txt"), "main2\n").unwrap();
        git_run(&repo2, &["add", "."]);
        git_run(&repo2, &["commit", "-q", "-m", "main2"]);
        assert!(git_ff_merge(&repo2, "v3.0.0").is_err());

        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(&repo2);
    }

    // (F97-2 closing) sync is bound to the canonical repo's release COMMIT: a matching peeled
    // commit fast-forwards; a mismatched recorded sha is skipped and HEAD does not move (on wave
    // 11's logic — fetch origin + merge the tag NAME — the mismatch case would have merged).
    #[test]
    fn sync_merges_on_matching_commit_and_skips_on_mismatch() {
        // the canonical repository: two commits, v9.0.0 at the tip.
        let canon = scratch("canon");
        init_repo(&canon);
        std::fs::write(canon.join("a.txt"), "two\n").unwrap();
        git_run(&canon, &["add", "."]);
        git_run(&canon, &["commit", "-q", "-m", "two"]);
        git_run(&canon, &["tag", "v9.0.0"]);
        let release_sha = git_out(&canon, &["rev-parse", "v9.0.0^{commit}"]);

        // a clean checkout one commit behind the release (so a fast-forward is possible).
        let work = scratch("work");
        git_run(
            work.parent().unwrap(),
            &[
                "clone",
                "-q",
                canon.to_str().unwrap(),
                work.to_str().unwrap(),
            ],
        );
        git_run(&work, &["config", "user.email", "t@example.com"]);
        git_run(&work, &["config", "user.name", "t"]);
        git_run(&work, &["reset", "--hard", "-q", "HEAD~1"]);
        let behind = head_of(&work);

        let id = match inspect_checkout(&work) {
            Checkout::Ready { id } => id,
            other => panic!("expected Ready, got {other:?}"),
        };

        // mismatch: the recorded sha is NOT the canonical release commit → skip, HEAD unmoved.
        let wrong = "0000000000000000000000000000000000000000";
        let e = sync_checkout(&id, canon.to_str().unwrap(), "v9.0.0", wrong).unwrap_err();
        assert!(e.contains("does not match the GitHub API commit"), "{e}");
        assert_eq!(
            head_of(&work),
            behind,
            "HEAD must not move on a sha mismatch"
        );

        // match: fast-forward to the release commit.
        sync_checkout(&id, canon.to_str().unwrap(), "v9.0.0", &release_sha).unwrap();
        assert_eq!(
            head_of(&work),
            release_sha,
            "HEAD fast-forwarded to the release"
        );

        let _ = std::fs::remove_dir_all(&canon);
        let _ = std::fs::remove_dir_all(&work);
    }

    // (F97-1 closing) the checkout identity is revalidated right before the merge: if the tree is
    // dirtied or the branch switched between planning and the merge, sync skips and HEAD is
    // unchanged, naming the precondition (wave 11 never rechecked, so it would have merged).
    #[test]
    fn sync_revalidates_identity_before_merge() {
        let canon = scratch("canon2");
        init_repo(&canon);
        std::fs::write(canon.join("a.txt"), "two\n").unwrap();
        git_run(&canon, &["add", "."]);
        git_run(&canon, &["commit", "-q", "-m", "two"]);
        git_run(&canon, &["tag", "v9.0.0"]);
        let release_sha = git_out(&canon, &["rev-parse", "v9.0.0^{commit}"]);

        // dirtied between planning and the merge
        let work = scratch("work2");
        git_run(
            work.parent().unwrap(),
            &[
                "clone",
                "-q",
                canon.to_str().unwrap(),
                work.to_str().unwrap(),
            ],
        );
        git_run(&work, &["config", "user.email", "t@example.com"]);
        git_run(&work, &["config", "user.name", "t"]);
        git_run(&work, &["reset", "--hard", "-q", "HEAD~1"]);
        let id = match inspect_checkout(&work) {
            Checkout::Ready { id } => id,
            other => panic!("expected Ready, got {other:?}"),
        };
        let before = head_of(&work);
        // A NONCONFLICTING untracked file: `git merge --ff-only v9.0.0` (the old logic) would still
        // succeed and move HEAD; revalidation must catch the dirty tree and skip instead.
        std::fs::write(work.join("c.txt"), "scratch\n").unwrap();
        let e = sync_checkout(&id, canon.to_str().unwrap(), "v9.0.0", &release_sha).unwrap_err();
        assert!(e.contains("not clean"), "names the precondition: {e}");
        assert_eq!(
            head_of(&work),
            before,
            "HEAD unchanged when the tree became dirty"
        );

        // branch switched between planning and the merge
        let work2 = scratch("work3");
        git_run(
            work2.parent().unwrap(),
            &[
                "clone",
                "-q",
                canon.to_str().unwrap(),
                work2.to_str().unwrap(),
            ],
        );
        git_run(&work2, &["config", "user.email", "t@example.com"]);
        git_run(&work2, &["config", "user.name", "t"]);
        git_run(&work2, &["reset", "--hard", "-q", "HEAD~1"]);
        let id2 = match inspect_checkout(&work2) {
            Checkout::Ready { id } => id,
            other => panic!("expected Ready, got {other:?}"),
        };
        let before2 = head_of(&work2);
        git_run(&work2, &["checkout", "-q", "-b", "other"]); // switch branch after planning
        let e2 = sync_checkout(&id2, canon.to_str().unwrap(), "v9.0.0", &release_sha).unwrap_err();
        assert!(
            e2.contains("branch changed"),
            "names the precondition: {e2}"
        );
        assert_eq!(
            head_of(&work2),
            before2,
            "HEAD unchanged when the branch switched"
        );

        let _ = std::fs::remove_dir_all(&canon);
        let _ = std::fs::remove_dir_all(&work);
        let _ = std::fs::remove_dir_all(&work2);
    }

    // ------------------------------------------------------------------- fetch via a mock

    struct MockFetcher {
        body: Option<Vec<u8>>,
    }
    impl Fetcher for MockFetcher {
        fn get(
            &self,
            _url: &str,
            _etag: Option<&str>,
            _timeout: Duration,
        ) -> Result<Fetched, String> {
            match &self.body {
                Some(b) => Ok(Fetched {
                    status: 200,
                    body: b.clone(),
                    etag: None,
                }),
                None => Err("offline".to_string()),
            }
        }
    }

    #[test]
    fn fetch_status_parses_and_reports_offline() {
        let body =
            br#"[{"name":"v99.0.0","commit":{"sha":"z"}},{"name":"v0.0.1","commit":{"sha":"y"}}]"#;
        let s = fetch_status(&MockFetcher {
            body: Some(body.to_vec()),
        })
        .unwrap();
        assert_eq!(s.latest, Some(v(99, 0, 0)));
        assert_eq!(s.latest_sha.as_deref(), Some("z"));
        assert!(s.update_available());

        let off = fetch_status(&MockFetcher { body: None }).unwrap_err();
        assert_eq!(off.reason, "offline");
    }

    // ------------------------------------------------------------------- cache round-trip + state

    #[test]
    fn cache_atomic_round_trip_and_fields() {
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let home = scratch("home");
        let prev = std::env::var_os("CODEX_HOME");
        std::env::set_var("CODEX_HOME", &home);

        let now = chrono::DateTime::parse_from_rfc3339("2026-10-22T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let status = Status {
            installed: v(0, 2, 3),
            latest: Some(v(0, 2, 4)),
            latest_tag: Some("v0.2.4".into()),
            latest_sha: Some("sha1".into()),
        };
        write_cache(&cache_success(&status, now));
        let p = home.join("c3").join("update-check.json");
        assert!(p.exists(), "cache file written");
        assert!(
            !home.join("c3").join("update-check.json.tmp").exists(),
            "no temp left behind"
        );
        let c = read_cache().unwrap();
        assert_eq!(c.latest, "0.2.4");
        assert_eq!(c.latest_sha, "sha1");
        assert_eq!(c.installed, "0.2.3");
        assert_eq!(c.source, "github-tags");
        assert_eq!(c.checked_at, c.attempted_at, "a success stamps both");

        match prev {
            Some(v) => std::env::set_var("CODEX_HOME", v),
            None => std::env::remove_var("CODEX_HOME"),
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    // (F97-3 closing, unit) attempt/success state and the throttle/offer windows.
    #[test]
    fn attempt_state_throttle_and_offer_windows() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-10-22T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let st = Status {
            installed: v(0, 2, 3),
            latest: Some(v(0, 2, 4)),
            latest_tag: Some("v0.2.4".into()),
            latest_sha: Some("s".into()),
        };

        // no cache → attempt
        assert!(should_attempt(None, now));
        // recent success (1 h ago) → no attempt
        let recent = cache_success(&st, now - chrono::Duration::hours(1));
        assert!(!should_attempt(Some(&recent), now));
        // old success (25 h) but a failed attempt 30 min ago → backoff, no attempt
        let mut backoff = cache_success(&st, now - chrono::Duration::hours(25));
        backoff.attempted_at = rfc3339(now - chrono::Duration::minutes(30));
        assert!(!should_attempt(Some(&backoff), now));
        // old success and old attempt → attempt
        let mut stale = cache_success(&st, now - chrono::Duration::hours(25));
        stale.attempted_at = rfc3339(now - chrono::Duration::hours(2));
        assert!(should_attempt(Some(&stale), now));

        // a failed attempt preserves the prior success fields and only advances attempted_at
        let prev = cache_success(&st, now - chrono::Duration::hours(30));
        let failed = cache_attempt_failed(Some(&prev), now);
        assert_eq!(failed.checked_at, prev.checked_at);
        assert_eq!(failed.latest, prev.latest);
        assert_ne!(failed.attempted_at, prev.attempted_at);

        // offer window: a 6-day-old success offers, an 8-day-old one does not
        assert!(offerable(
            &cache_success(&st, now - chrono::Duration::days(6)),
            now
        ));
        assert!(!offerable(
            &cache_success(&st, now - chrono::Duration::days(8)),
            now
        ));
    }

    // ------------------------------------------------------------------- the off switch

    #[test]
    fn off_switch_env() {
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var_os("C3_UPDATE_CHECK");
        std::env::set_var("C3_UPDATE_CHECK", "off");
        assert!(!check_enabled());
        std::env::set_var("C3_UPDATE_CHECK", "on");
        assert!(check_enabled());
        match prev {
            Some(v) => std::env::set_var("C3_UPDATE_CHECK", v),
            None => std::env::remove_var("C3_UPDATE_CHECK"),
        }
    }
}
