//! `c3 update` — C3 checks for its own newer release and offers it (wave 11).
//!
//! Source of truth: the git tags of the C3 repository, read through the GitHub tags API as an
//! unauthenticated JSON array `[{"name":"v0.2.3","commit":{"sha":...}},...]`. The newest release is
//! the highest semantic version among tag names of the form `v<major>.<minor>.<patch>` (anything
//! else is ignored) and is compared with the running binary's `CARGO_PKG_VERSION`.
//!
//! Nothing here ever changes anything on its own. The SessionStart hook only READS the cache,
//! refreshing it at most once a day within a 3 s budget and silently ([`hook_line`]); `c3 update`
//! reports ([`run_check_report`]); `c3 update --apply` is the only path that installs
//! ([`run_apply`]), and only ever: `cargo install` from the tag, an optional fast-forward of a
//! known clean source checkout, and the plugin update. It never follows a redirect, never sends
//! anything but the GET, and never writes outside `<codex home>/c3/`.
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
/// The git remote `--apply` installs from.
pub const GIT_URL: &str = "https://github.com/xelth-com/C3.git";
/// The source descriptor recorded in the cache.
pub const SOURCE: &str = "github-tags";

/// Do not check more than once per 24 h (the hook; `c3 update` forces a check regardless).
const CHECK_EVERY: chrono::Duration = chrono::Duration::hours(24);
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

/// `<codex home>/c3/update-check.json` (§3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCache {
    pub checked_at: String,
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

fn cache_from(status: &Status, now: DateTime<Utc>) -> UpdateCache {
    UpdateCache {
        checked_at: now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        latest: status.latest.map(|v| v.bare()).unwrap_or_default(),
        latest_sha: status.latest_sha.clone().unwrap_or_default(),
        installed: status.installed.bare(),
        source: SOURCE.to_string(),
    }
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

/// A cached check younger than 24 h is fresh (the hook does not re-fetch).
fn is_fresh(c: &UpdateCache, now: DateTime<Utc>) -> bool {
    DateTime::parse_from_rfc3339(&c.checked_at)
        .map(|t| now.signed_duration_since(t.with_timezone(&Utc)) < CHECK_EVERY)
        .unwrap_or(false)
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

/// The hook's once-a-day check (item 5). Returns the update line to print, or `None` (disabled,
/// offline with no cache, up to date, or any error). Never panics, never blocks past the 3 s
/// budget, never writes outside `<codex home>/c3/`.
pub fn hook_line(now: DateTime<Utc>) -> Option<String> {
    if !check_enabled() {
        return None;
    }
    let cache = read_cache();
    let status = match cache.as_ref().filter(|c| is_fresh(c, now)) {
        Some(c) => status_from_cache(c),
        None => match fetch_status(default_fetcher().as_ref()) {
            Ok(s) => {
                write_cache(&cache_from(&s, now));
                s
            }
            // A failed fetch keeps the previous cache and is silent in the hook (§3).
            Err(_) => status_from_cache(cache.as_ref()?),
        },
    };
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

/// Whether a known source checkout can be fast-forwarded (item 7b), decided WITHOUT the network
/// (`git status --porcelain` + the symbolic HEAD).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checkout {
    /// A clean working tree on a branch: a fast-forward will be attempted.
    Ready { path: PathBuf, branch: String },
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

/// Inspect a known checkout WITHOUT the network: clean + on a branch → [`Checkout::Ready`]; else
/// [`Checkout::Skip`] with the reason (not a git repo / a dirty tree / a detached HEAD).
pub fn inspect_checkout(path: &Path) -> Checkout {
    match git(path, &["rev-parse", "--is-inside-work-tree"]) {
        Ok(s) if s.trim() == "true" => {}
        _ => {
            return Checkout::Skip {
                path: path.into(),
                why: "it is not a git work tree".into(),
            }
        }
    }
    match git(path, &["status", "--porcelain"]) {
        Ok(s) if s.trim().is_empty() => {}
        Ok(_) => {
            return Checkout::Skip {
                path: path.into(),
                why: "its working tree is not clean".into(),
            }
        }
        Err(_) => {
            return Checkout::Skip {
                path: path.into(),
                why: "it is not a git work tree".into(),
            }
        }
    }
    match git(path, &["symbolic-ref", "--short", "-q", "HEAD"]) {
        Ok(s) if !s.trim().is_empty() => Checkout::Ready {
            path: path.into(),
            branch: s.trim().to_string(),
        },
        _ => Checkout::Skip {
            path: path.into(),
            why: "its HEAD is detached".into(),
        },
    }
}

/// One step `--apply` would run, in order (item 7). A pure value, so the plan is testable without
/// running anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// (a) `cargo install --git <GIT_URL> --tag <tag> --locked [--features index-surreal] c3-cli`.
    CargoInstall { tag: String, index_surreal: bool },
    /// (b) fast-forward a clean source checkout to the tag.
    GitSync { path: PathBuf, tag: String },
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
            Step::CargoInstall { tag, index_surreal } => {
                let feat = if *index_surreal {
                    " --features index-surreal"
                } else {
                    ""
                };
                format!("cargo install --git {GIT_URL} --tag {tag} --locked{feat} c3-cli")
            }
            Step::GitSync { path, tag } => format!(
                "git -C {p} fetch --tags origin && git -C {p} merge --ff-only {tag}",
                p = path.display()
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

/// Build the ordered plan for a latest tag and environment (item 7). Pure: all the environment
/// probing (the feature bundle, the checkout, `claude` on PATH) is passed in.
pub fn plan(
    tag: &str,
    index_surreal: bool,
    checkout: &Checkout,
    claude_on_path: bool,
) -> Vec<Step> {
    let mut steps = vec![Step::CargoInstall {
        tag: tag.to_string(),
        index_surreal,
    }];
    match checkout {
        Checkout::Ready { path, .. } => steps.push(Step::GitSync {
            path: path.clone(),
            tag: tag.to_string(),
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
        Checkout::Ready { path, branch } => format!(
            "cargo install ({bundle}) + fast-forward the source checkout at {} (on {branch})",
            path.display()
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

/// `git merge --ff-only <tag>` in `dir` (no fetch): `Ok` on a fast-forward, `Err(reason)` on a
/// non-fast-forward (the reason printed, the step skipped, never fatal).
fn git_ff_merge(dir: &Path, tag: &str) -> Result<(), String> {
    git(dir, &["merge", "--ff-only", tag]).map(|_| ())
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
/// skipped (non-fatal: a known-dirty/detached checkout, a non-ff, or `claude` not on PATH);
/// `Err` is a real command failure that stops the apply.
fn run_step(step: &Step) -> Result<bool, String> {
    match step {
        Step::CargoInstall { tag, index_surreal } => {
            let mut args = vec![
                "install",
                "--git",
                GIT_URL,
                "--tag",
                tag.as_str(),
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
        Step::GitSync { path, tag } => {
            // (b) never fails (a): a fetch failure or a non-ff is printed and skipped.
            if let Err(e) = stream("git", &["fetch", "--tags", "origin"], Some(path)) {
                println!(
                    "c3 update: skipped the source checkout fast-forward - git fetch failed ({e})"
                );
                return Ok(false);
            }
            if let Err(e) = git_ff_merge(path, tag) {
                println!("c3 update: skipped the source checkout fast-forward - not a fast-forward ({e})");
                return Ok(false);
            }
            println!(
                "c3 update: fast-forwarded the source checkout at {} to {tag}",
                path.display()
            );
            Ok(true)
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

/// `c3 update` / `c3 update --check` (item 6): force a check now (ignoring the 24 h cache age),
/// write the cache, print the report, change nothing. Exit 0 up to date, 3 an update exists, 1 on a
/// fetch failure.
pub fn run_check_report(now: DateTime<Utc>) -> i32 {
    let status = match fetch_status(default_fetcher().as_ref()) {
        Ok(s) => {
            write_cache(&cache_from(&s, now));
            s
        }
        Err(e) => {
            eprintln!("c3 update: could not check for updates - {}", e.reason);
            return 1;
        }
    };
    let available = status.update_available();
    let tag = status.latest_tag_str();
    let index = built_with_index_surreal();
    let checkout = resolve_checkout();

    println!("c3 update");
    println!("  installed: {}", status.installed.bare());
    match status.latest {
        Some(l) => println!("  latest:    {} ({SOURCE})", l.tag()),
        None => println!("  latest:    unknown (no release tag found)"),
    }
    if available {
        println!("  status:    an update is available");
        println!("  changelog: https://github.com/{REPO}/blob/{tag}/CHANGELOG.md");
    } else {
        println!("  status:    up to date");
    }
    println!("  install:   {}", describe_install_kind(&checkout, index));
    if available {
        println!("  --apply would run, stopping at the first failure:");
        for step in plan(&tag, index, &checkout, claude_on_path()) {
            println!("      {}", step.command());
        }
    }
    println!("  nothing was changed.");
    if available {
        3
    } else {
        0
    }
}

/// `c3 update --apply` (item 7): force a check, then install. Refuse when up to date (exit 0) and
/// when the fetch failed (exit 1); otherwise run the plan, streaming output, stopping at the first
/// real failure (exit 1), and print what changed (exit 0).
pub fn run_apply(now: DateTime<Utc>) -> i32 {
    let status = match fetch_status(default_fetcher().as_ref()) {
        Ok(s) => {
            write_cache(&cache_from(&s, now));
            s
        }
        Err(e) => {
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
    let tag = status.latest_tag_str();
    let checkout = resolve_checkout();
    let steps = plan(
        &tag,
        built_with_index_surreal(),
        &checkout,
        claude_on_path(),
    );
    println!(
        "c3 update: updating from v{} to {tag}",
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

    // ------------------------------------------------------------------- the plan

    #[test]
    fn plan_for_each_install_kind() {
        // no checkout, light bundle, claude present
        assert_eq!(
            plan("v9.9.9", false, &Checkout::Unknown, true),
            vec![
                Step::CargoInstall {
                    tag: "v9.9.9".into(),
                    index_surreal: false
                },
                Step::PluginUpdate,
            ]
        );
        // index-surreal bundle, claude absent → manual plugin step and the --features flag
        let steps = plan("v9.9.9", true, &Checkout::Unknown, false);
        assert_eq!(steps[0].command(), "cargo install --git https://github.com/xelth-com/C3.git --tag v9.9.9 --locked --features index-surreal c3-cli");
        assert_eq!(steps.last(), Some(&Step::PluginUpdateManual));
        // a clean checkout → a GitSync step between install and plugin
        let ready = Checkout::Ready {
            path: PathBuf::from("/src/c3"),
            branch: "main".into(),
        };
        let steps = plan("v9.9.9", false, &ready, true);
        assert!(matches!(steps[1], Step::GitSync { .. }));
        // a dirty/detached checkout → a GitSyncSkipped step (planned, not fatal)
        let dirty = Checkout::Skip {
            path: PathBuf::from("/src/c3"),
            why: "its working tree is not clean".into(),
        };
        let steps = plan("v9.9.9", false, &dirty, true);
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

    fn init_repo(dir: &Path) {
        git_run(dir, &["init", "-q", "-b", "main"]);
        git_run(dir, &["config", "user.email", "t@example.com"]);
        git_run(dir, &["config", "user.name", "t"]);
        git_run(dir, &["config", "commit.gpgsign", "false"]);
        std::fs::write(dir.join("a.txt"), "one\n").unwrap();
        git_run(dir, &["add", "."]);
        git_run(dir, &["commit", "-q", "-m", "one"]);
    }

    #[test]
    fn inspect_clean_dirty_detached_and_non_repo() {
        // not a repo
        let empty = scratch("norepo");
        assert!(matches!(inspect_checkout(&empty), Checkout::Skip { .. }));

        let repo = scratch("clean");
        init_repo(&repo);
        match inspect_checkout(&repo) {
            Checkout::Ready { branch, .. } => assert_eq!(branch, "main"),
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
        let head = {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(["rev-parse", "HEAD"])
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
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
        // move main back one commit so v2.0.0 is strictly ahead (a fast-forward)
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
        assert!(s.update_available());

        let off = fetch_status(&MockFetcher { body: None }).unwrap_err();
        assert_eq!(off.reason, "offline");
    }

    // ------------------------------------------------------------------- cache round-trip + staleness

    #[test]
    fn cache_atomic_round_trip_and_staleness() {
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
        write_cache(&cache_from(&status, now));
        let p = home.join("c3").join("update-check.json");
        assert!(p.exists(), "cache file written");
        assert!(
            !home.join("c3").join("update-check.json.tmp").exists(),
            "no temp left behind"
        );
        let c = read_cache().unwrap();
        assert_eq!(c.latest, "0.2.4");
        assert_eq!(c.installed, "0.2.3");
        assert_eq!(c.source, "github-tags");
        assert!(is_fresh(&c, now));
        // 25 h later it is stale
        assert!(!is_fresh(&c, now + chrono::Duration::hours(25)));

        match prev {
            Some(v) => std::env::set_var("CODEX_HOME", v),
            None => std::env::remove_var("CODEX_HOME"),
        }
        let _ = std::fs::remove_dir_all(&home);
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
