//! The single injected seam between `c3 maintain` and the outside world: running a CLI and making
//! an HTTP GET. Production uses [`SystemEnv`]; the unit tests script a fake so no pass ever touches
//! a real CLI, the network or a model. Every external command of a pass goes through [`Env::run`],
//! every model-list HTTP read through [`Env::http_get`].

use std::path::Path;
use std::time::Duration;

/// The result of running a CLI: the exit code (`None` on a signal / spawn failure) and the captured
/// output. `ok()` is "exited 0".
#[derive(Debug, Clone, Default)]
pub struct RunResult {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl RunResult {
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }

    /// stdout and stderr joined (for a version/model probe that may print to either).
    pub fn combined(&self) -> String {
        if self.stderr.trim().is_empty() {
            self.stdout.clone()
        } else if self.stdout.trim().is_empty() {
            self.stderr.clone()
        } else {
            format!("{}\n{}", self.stdout, self.stderr)
        }
    }

    /// A one-line failure reason (the first non-empty stderr line, else stdout, else the code).
    pub fn fail_reason(&self) -> String {
        let pick = |s: &str| {
            s.lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .map(|l| l.to_string())
        };
        pick(&self.stderr)
            .or_else(|| pick(&self.stdout))
            .unwrap_or_else(|| match self.code {
                Some(c) => format!("exit {c}"),
                None => "the command did not complete".to_string(),
            })
    }
}

/// The outside world a maintenance pass talks to. Both methods are best-effort and never panic.
pub trait Env {
    /// Run `program` with `args`, extra `env` pairs set on top of the inherited environment, in
    /// `cwd` (or the current directory). Never inherits the coordinator's host markers.
    fn run(
        &self,
        program: &str,
        args: &[&str],
        env: &[(&str, &str)],
        cwd: Option<&Path>,
    ) -> RunResult;

    /// Like [`Env::run`], but for a child that must NOT see the operator's provider secrets: the
    /// third-party update/install commands (the downloaded kimi installer, `npm install`, `grok
    /// update`) and the version probes. Every variable whose name is a secret ([`is_secret_env`]) is
    /// removed from the child's environment; proxy variables stay. The smoke's `c3 consult` children
    /// keep the environment (they need the keys) and go through [`Env::run`]. The default forwards to
    /// `run` so the unit-test fakes need not distinguish; [`SystemEnv`] overrides it.
    fn run_clean(
        &self,
        program: &str,
        args: &[&str],
        env: &[(&str, &str)],
        cwd: Option<&Path>,
    ) -> RunResult {
        self.run(program, args, env, cwd)
    }

    /// A plain HTTPS GET with the given request `headers` under `policy` (redirects, one absolute
    /// deadline, the body cap), returning `(status, body)` or an error. Used for the model-list
    /// endpoints, the public catalogs and pages, and the kimi `latest` marker and installer;
    /// failures are ignored by the caller. A request that carries an API key uses
    /// [`Redirects::Never`], so a key is never forwarded to a redirect target.
    fn http_get(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        policy: GetPolicy,
    ) -> Result<(u16, String), String>;

    /// Whether this is the production [`SystemEnv`] (the test-mode routing of the public model
    /// sources applies to it only; the unit-test fakes answer for themselves).
    fn is_system(&self) -> bool {
        false
    }
}

/// The production environment: a real `std::process::Command` (host markers scrubbed from the
/// child, exactly as the engine launchers scrub them) and a real ureq GET (`https` only, redirects
/// followed by hand per [`GetPolicy`]).
pub struct SystemEnv;

impl Env for SystemEnv {
    fn is_system(&self) -> bool {
        true
    }

    fn run(
        &self,
        program: &str,
        args: &[&str],
        env: &[(&str, &str)],
        cwd: Option<&Path>,
    ) -> RunResult {
        let mut cmd = std::process::Command::new(program);
        cmd.args(args);
        if let Some(d) = cwd {
            cmd.current_dir(d);
        }
        for (k, v) in env {
            cmd.env(k, v);
        }
        crate::engines::scrub_host_markers(&mut cmd);
        run_command(cmd)
    }

    fn run_clean(
        &self,
        program: &str,
        args: &[&str],
        env: &[(&str, &str)],
        cwd: Option<&Path>,
    ) -> RunResult {
        let mut cmd = std::process::Command::new(program);
        cmd.args(args);
        if let Some(d) = cwd {
            cmd.current_dir(d);
        }
        // Strip the operator's provider secrets from the inherited environment before the explicit
        // pairs (a pin like KIMI_VERSION) are set, so third-party install/update code never sees a
        // key. Proxy variables are left in place (an update may need the proxy to reach the network).
        for (k, _) in std::env::vars() {
            if is_secret_env(&k) {
                cmd.env_remove(&k);
            }
        }
        for (k, v) in env {
            cmd.env(k, v);
        }
        crate::engines::scrub_host_markers(&mut cmd);
        run_command(cmd)
    }

    fn http_get(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        policy: GetPolicy,
    ) -> Result<(u16, String), String> {
        // Redirects are followed by hand, never by ureq ([`fetch`] re-validates every hop against
        // the policy), and the deadline is absolute: each hop gets only what is left of it, and
        // ureq applies a request's timeout to its body read as well.
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(policy.timeout)
            .redirects(0)
            .build();
        let ua = format!("c3/{}", env!("CARGO_PKG_VERSION"));
        fetch(url, policy, |current, remaining| {
            let mut req = agent.get(current).timeout(remaining).set("User-Agent", &ua);
            for (k, v) in headers {
                req = req.set(k, v);
            }
            match req.call() {
                Ok(resp) => {
                    let status = resp.status();
                    if policy.redirects != Redirects::Never
                        && matches!(status, 301 | 302 | 303 | 307 | 308)
                    {
                        return Ok(Hop::Redirect(
                            resp.header("location").unwrap_or("").to_string(),
                        ));
                    }
                    Ok(Hop::Body(status, resp.into_reader()))
                }
                Err(ureq::Error::Status(code, _)) => Ok(Hop::Status(code)),
                Err(e) => Err(c3_core::one_line(&e.to_string())),
            }
        })
    }
}

/// How a GET follows a redirect. Always by hand ([`fetch`]), never by the HTTP client, and every
/// hop is re-validated: https, the port the request started on, no userinfo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Redirects {
    /// Never: a request that carries a key (an endpoint's model list). A redirect answer is
    /// returned as it is, so a key never reaches another URL.
    Never,
    /// The kimi vendor installer and its `latest` marker: the same host as the previous hop, or the
    /// one mirror listed for it in [`INSTALLER_REDIRECT_HOSTS`].
    Installer,
    /// A public catalog or page: the host the request started on - no exception, not even the
    /// installer's mirror pair.
    SameOrigin,
}

/// The policy of one GET.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetPolicy {
    /// ONE absolute deadline for the whole fetch: connecting, every redirect hop and the body read.
    pub timeout: Duration,
    pub redirects: Redirects,
    /// The largest body accepted. The reader stops after `cap + 1` bytes, and a body that long is
    /// an error, never a truncated prefix.
    pub cap: usize,
}

/// One answer of a hop in [`fetch`]: a redirect to follow (its `location`), a body to read, or an
/// error status (no body).
pub(crate) enum Hop<R> {
    Redirect(String),
    Body(u16, R),
    Status(u16),
}

/// The redirect loop of a GET over an injected transport `hop(url, remaining)` (production: one
/// ureq request whose timeout is `remaining`). The start URL and every redirect target are
/// validated by the policy ([`check_start`], [`redirect_target`]), at most [`MAX_REDIRECTS`]
/// redirects are followed, the deadline is absolute across all hops, and the body is read through
/// [`read_capped`] with the policy's cap.
pub(crate) fn fetch<R: std::io::Read>(
    url: &str,
    policy: GetPolicy,
    mut hop: impl FnMut(&str, Duration) -> Result<Hop<R>, String>,
) -> Result<(u16, String), String> {
    check_start(url)?;
    let deadline = std::time::Instant::now() + policy.timeout;
    let mut current = url.to_string();
    let mut hops = 0;
    loop {
        let remaining = deadline
            .checked_duration_since(std::time::Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| {
                format!(
                    "the request did not finish within {} ms (one deadline over every redirect)",
                    policy.timeout.as_millis()
                )
            })?;
        match hop(&current, remaining)? {
            Hop::Redirect(location) => {
                hops += 1;
                if hops > MAX_REDIRECTS {
                    return Err("too many redirects".to_string());
                }
                current = redirect_target(url, &current, &location, policy.redirects)?;
            }
            Hop::Body(status, reader) => {
                // Read the body as bytes and lossily decode: the kimi `latest` marker answers with
                // no text content-type. A body that reaches the cap is an ERROR, never a truncated
                // prefix: the kimi installer is run after it is fetched, so a cut-off script must
                // never be written or executed.
                let buf = read_capped(reader, policy.cap)?;
                return Ok((status, String::from_utf8_lossy(&buf).into_owned()));
            }
            Hop::Status(code) => return Ok((code, String::new())),
        }
    }
}

/// The start URL of a GET: https, with a host, without userinfo.
fn check_start(url: &str) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("only https is allowed".to_string());
    }
    let u = url::Url::parse(url).map_err(|e| format!("bad url: {e}"))?;
    if u.host_str().unwrap_or("").is_empty() {
        return Err("a url without a host".to_string());
    }
    if !u.username().is_empty() || u.password().is_some() {
        return Err("a url with credentials is never requested".to_string());
    }
    Ok(())
}

/// How many redirects a GET follows.
const MAX_REDIRECTS: u32 = 3;

/// The largest body the kimi installer download, its `latest` marker and the model-list endpoints
/// accept. A body that reaches this cap is refused (a truncated installer is never written or run);
/// legitimate installers and model lists are well under it. The public catalogs and pages have
/// their own, smaller cap (`web::BODY_CAP`).
pub const HTTP_BODY_CAP: usize = 8 * 1024 * 1024;

/// Run a prepared command and capture its output (a spawn failure is `code: None`).
fn run_command(mut cmd: std::process::Command) -> RunResult {
    match cmd.output() {
        Ok(out) => RunResult {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        },
        Err(e) => RunResult {
            code: None,
            stdout: String::new(),
            stderr: c3_core::one_line(&e.to_string()),
        },
    }
}

/// Whether an environment variable name holds a provider secret that an update/install child must
/// not see: a name ending in `_API_KEY`, `_TOKEN`, `_SECRET`, `_PASSWORD` or `_KEY` (case-
/// insensitive), or an `ANTHROPIC_*` auth variable. Proxy variables (`HTTP_PROXY`, …) never match.
pub fn is_secret_env(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    n.ends_with("_API_KEY")
        || n.ends_with("_TOKEN")
        || n.ends_with("_SECRET")
        || n.ends_with("_PASSWORD")
        || n.ends_with("_KEY")
        || n.starts_with("ANTHROPIC_")
}

/// Read a response body, refusing a body that reaches `cap` (a truncated download is never used).
fn read_capped<R: std::io::Read>(r: R, cap: usize) -> Result<Vec<u8>, String> {
    use std::io::Read as _;
    let mut buf: Vec<u8> = Vec::new();
    r.take(cap as u64 + 1)
        .read_to_end(&mut buf)
        .map_err(|e| c3_core::one_line(&e.to_string()))?;
    if buf.len() > cap {
        return Err(format!(
            "the response body exceeded the {cap}-byte cap (a truncated download is never used)"
        ));
    }
    Ok(buf)
}

/// The hosts the kimi installer download may be redirected to from another host: `(from, to)`,
/// both exact, for [`Redirects::Installer`] only. Seen live 2026-11-15: `code.kimi.ai` answers the
/// version marker and the installer with a 302 to `cdn.kimi.ai`. Any other cross-host redirect is
/// refused - "some subdomain of the same site" is not good enough for a script that is run after
/// it is fetched - and a public catalog or page never gets even this pair.
const INSTALLER_REDIRECT_HOSTS: &[(&str, &str)] = &[("code.kimi.ai", "cdn.kimi.ai")];

/// The URL a redirect from `from` to `location` leads to, or why it is refused. `start` is the URL
/// the request started on. Every policy: the target is https, on the start's port (the scheme's
/// default when none is written) and carries no userinfo. [`Redirects::SameOrigin`]: on the start's
/// host. [`Redirects::Installer`]: on the host of `from` or on the one listed for it in
/// [`INSTALLER_REDIRECT_HOSTS`]. [`Redirects::Never`] follows nothing. A relative `location` is
/// resolved against `from`.
pub(crate) fn redirect_target(
    start: &str,
    from: &str,
    location: &str,
    policy: Redirects,
) -> Result<String, String> {
    if policy == Redirects::Never {
        return Err("this request follows no redirect".to_string());
    }
    let location = location.trim();
    if location.is_empty() {
        return Err("a redirect without a location".to_string());
    }
    let start_u = url::Url::parse(start).map_err(|e| format!("bad url: {e}"))?;
    let base = url::Url::parse(from).map_err(|e| format!("bad url: {e}"))?;
    let next = base
        .join(location)
        .map_err(|e| format!("bad redirect location: {e}"))?;
    if next.scheme() != "https" {
        return Err(format!("refused a redirect to {}", next.scheme()));
    }
    if !next.username().is_empty() || next.password().is_some() {
        return Err("refused a redirect to a url with credentials".to_string());
    }
    if next.port_or_known_default() != start_u.port_or_known_default() {
        return Err("refused a redirect to another port".to_string());
    }
    let start_host = start_u.host_str().unwrap_or("").to_ascii_lowercase();
    let from_host = base.host_str().unwrap_or("").to_ascii_lowercase();
    let to_host = next.host_str().unwrap_or("").to_ascii_lowercase();
    let allowed = match policy {
        Redirects::SameOrigin => !start_host.is_empty() && to_host == start_host,
        _ => {
            !from_host.is_empty()
                && (from_host == to_host
                    || INSTALLER_REDIRECT_HOSTS
                        .iter()
                        .any(|(f, t)| *f == from_host && *t == to_host))
        }
    };
    if !allowed {
        return Err(format!(
            "refused a redirect to another host ({})",
            if to_host.is_empty() { "?" } else { &to_host }
        ));
    }
    Ok(next.to_string())
}

#[cfg(test)]
mod tests {
    use super::{fetch, is_secret_env, read_capped, redirect_target, GetPolicy, Hop, Redirects};

    #[test]
    fn the_secret_env_filter() {
        for n in [
            "ZAI_API_KEY",
            "KIMI_API_KEY",
            "GITHUB_TOKEN",
            "FOO_SECRET",
            "BAR_PASSWORD",
            "BAZ_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_BASE_URL",
            "xai_api_key",
        ] {
            assert!(is_secret_env(n), "{n} should be scrubbed");
        }
        for n in [
            "PATH",
            "HOME",
            "TEMP",
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "NO_PROXY",
            "KIMI_VERSION",
        ] {
            assert!(!is_secret_env(n), "{n} should be kept");
        }
    }

    #[test]
    fn a_body_that_reaches_the_cap_is_an_error() {
        let under = vec![b'x'; 90];
        assert_eq!(read_capped(under.as_slice(), 100).unwrap().len(), 90);
        let over = vec![b'x'; 120];
        assert!(read_capped(over.as_slice(), 100).is_err());
        // exactly at the cap is accepted; one past it is not
        assert!(read_capped(vec![b'x'; 100].as_slice(), 100).is_ok());
        assert!(read_capped(vec![b'x'; 101].as_slice(), 100).is_err());
    }

    /// The installer policy from a single hop (start == from).
    fn installer(from: &str, location: &str) -> Result<String, String> {
        redirect_target(from, from, location, Redirects::Installer)
    }

    #[test]
    fn a_redirect_stays_on_https_and_on_the_same_site() {
        assert_eq!(
            installer(
                "https://code.kimi.ai/kimi-code/latest",
                "https://cdn.kimi.ai/kimi-code/latest"
            )
            .unwrap(),
            "https://cdn.kimi.ai/kimi-code/latest"
        );
        // a relative location is resolved against the request
        assert_eq!(
            installer("https://code.kimi.ai/kimi-code/latest", "/v2/latest").unwrap(),
            "https://code.kimi.ai/v2/latest"
        );
        // a transport downgrade, another site and an empty location are refused
        assert!(installer(
            "https://code.kimi.ai/kimi-code/install.ps1",
            "http://cdn.kimi.ai/kimi-code/install.ps1"
        )
        .is_err());
        assert!(installer(
            "https://code.kimi.ai/kimi-code/install.ps1",
            "https://cdn.example.net/kimi-code/install.ps1"
        )
        .is_err());
        assert!(installer("https://code.kimi.ai/kimi-code/latest", "  ").is_err());
        // another subdomain of the same site is not enough: only the listed mirror is accepted,
        // and only from the host it is listed for
        assert!(installer(
            "https://code.kimi.ai/kimi-code/install.ps1",
            "https://files.kimi.ai/kimi-code/install.ps1"
        )
        .is_err());
        assert!(installer(
            "https://cdn.kimi.ai/kimi-code/install.ps1",
            "https://code.kimi.ai/kimi-code/install.ps1"
        )
        .is_err());
        // (F02-12) the installer pair, too, keeps the port and refuses userinfo
        assert!(installer(
            "https://code.kimi.ai/kimi-code/latest",
            "https://cdn.kimi.ai:8443/kimi-code/latest"
        )
        .is_err());
        assert!(installer(
            "https://code.kimi.ai/kimi-code/latest",
            "https://u:p@cdn.kimi.ai/kimi-code/latest"
        )
        .is_err());
        // a key-carrying request follows nothing
        assert!(redirect_target(
            "https://api.z.ai/v1/models",
            "https://api.z.ai/v1/models",
            "/v2/models",
            Redirects::Never
        )
        .is_err());
    }

    /// (F02-12, F04-8) A public catalog or page has its own redirect policy: the start's host AND
    /// port, no userinfo, re-validated at every hop - the kimi installer's mirror pair does not
    /// apply to it.
    #[test]
    fn a_public_source_redirect_stays_on_its_origin() {
        let same = |start: &str, from: &str, loc: &str| {
            redirect_target(start, from, loc, Redirects::SameOrigin)
        };
        let s = "https://catalog.example/api/v1/models";
        assert_eq!(
            same(s, s, "/api/v2/models").unwrap(),
            "https://catalog.example/api/v2/models"
        );
        assert_eq!(
            same(s, s, "https://CATALOG.example:443/x").unwrap(),
            "https://catalog.example/x",
            "the explicit default port and the host's case are the same origin"
        );
        // the installer's pair is NOT honoured for a public source
        assert!(same(
            "https://code.kimi.ai/models.json",
            "https://code.kimi.ai/models.json",
            "https://cdn.kimi.ai/models.json"
        )
        .is_err());
        // another port, userinfo, another host, a downgrade
        assert!(same(s, s, "https://catalog.example:8443/x").is_err());
        assert!(same(s, s, "https://user@catalog.example/x").is_err());
        assert!(same(s, s, "https://user:pw@catalog.example/x").is_err());
        assert!(same(s, s, "https://evil.example/x").is_err());
        assert!(same(s, s, "http://catalog.example/x").is_err());
        // a later hop is still judged against the START, not the previous hop
        assert!(same(s, "https://evil.example/x", "https://evil.example/y").is_err());
    }

    fn policy(ms: u64, redirects: Redirects, cap: usize) -> GetPolicy {
        GetPolicy {
            timeout: std::time::Duration::from_millis(ms),
            redirects,
            cap,
        }
    }

    /// (F02-12, F02-13) The redirect loop: the start URL is validated, every hop is re-validated
    /// (a userinfo or cross-host hop stops the fetch before it is requested), at most three
    /// redirects are followed, and the body is read through the policy's cap.
    #[test]
    fn the_fetch_loop_validates_every_hop_and_caps_the_body() {
        // a start with credentials or over http is never requested
        let mut asked: Vec<String> = Vec::new();
        let r = fetch::<&[u8]>(
            "https://u:p@catalog.example/m",
            policy(5_000, Redirects::SameOrigin, 100),
            |u, _| {
                asked.push(u.to_string());
                Ok(Hop::Status(500))
            },
        );
        assert!(r.is_err() && asked.is_empty(), "{r:?} {asked:?}");
        assert!(fetch::<&[u8]>(
            "http://catalog.example/m",
            policy(5_000, Redirects::SameOrigin, 100),
            |_, _| Ok(Hop::Status(500))
        )
        .is_err());

        // a same-origin hop is followed, a userinfo hop is refused before it is requested
        let mut asked: Vec<String> = Vec::new();
        let r = fetch::<&[u8]>(
            "https://catalog.example/m",
            policy(5_000, Redirects::SameOrigin, 100),
            |u, _| {
                asked.push(u.to_string());
                Ok(match asked.len() {
                    1 => Hop::Redirect("/m2".into()),
                    _ => Hop::Redirect("https://x:y@catalog.example/m3".into()),
                })
            },
        );
        assert!(r.unwrap_err().contains("credentials"));
        assert_eq!(
            asked,
            vec!["https://catalog.example/m", "https://catalog.example/m2"]
        );

        // at most three redirects
        let mut n = 0;
        let r = fetch::<&[u8]>(
            "https://catalog.example/m",
            policy(5_000, Redirects::SameOrigin, 100),
            |_, _| {
                n += 1;
                Ok(Hop::Redirect(format!("/m{n}")))
            },
        );
        assert_eq!(r.unwrap_err(), "too many redirects");
        assert_eq!(n, 4, "the start and three redirects were requested");

        // the body: at the cap is read, one byte past it is an error (the reader stops at cap+1)
        let body = vec![b'x'; 100];
        let r = fetch(
            "https://catalog.example/m",
            policy(5_000, Redirects::SameOrigin, 100),
            |_, _| Ok(Hop::Body(200, body.as_slice())),
        );
        assert_eq!(r.unwrap().1.len(), 100);
        let body = vec![b'x'; 101];
        assert!(fetch(
            "https://catalog.example/m",
            policy(5_000, Redirects::SameOrigin, 100),
            |_, _| Ok(Hop::Body(200, body.as_slice())),
        )
        .is_err());
    }

    /// (F02-13) ONE absolute deadline over every hop: each hop is given only what is left, and a
    /// chain of slow redirects ends at the deadline instead of restarting the clock per hop.
    #[test]
    fn the_fetch_deadline_is_absolute_across_hops() {
        let mut given: Vec<std::time::Duration> = Vec::new();
        let start = std::time::Instant::now();
        let r = fetch::<&[u8]>(
            "https://catalog.example/m",
            policy(300, Redirects::SameOrigin, 100),
            |_, remaining| {
                given.push(remaining);
                std::thread::sleep(std::time::Duration::from_millis(120));
                Ok(Hop::Redirect("/again".into()))
            },
        );
        let elapsed = start.elapsed();
        let err = r.unwrap_err();
        assert!(err.contains("did not finish within 300 ms"), "{err}");
        assert!(given.len() >= 2 && given.len() <= 3, "{given:?}");
        assert!(
            given.windows(2).all(|w| w[1] < w[0]),
            "each hop gets less time: {given:?}"
        );
        assert!(given[0] <= std::time::Duration::from_millis(300));
        assert!(
            elapsed < std::time::Duration::from_millis(1_000),
            "the chain ended at the deadline: {elapsed:?}"
        );
    }
}
