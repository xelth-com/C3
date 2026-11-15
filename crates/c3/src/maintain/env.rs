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
    fn run(&self, program: &str, args: &[&str], env: &[(&str, &str)], cwd: Option<&Path>)
        -> RunResult;

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

    /// A plain HTTPS GET with the given request `headers`, returning `(status, body)` or an error.
    /// Used for the model-list endpoints and the kimi `latest` marker; failures are ignored by the
    /// caller. `follow_redirects` is set only for KEYLESS public GETs (the kimi marker/installer,
    /// which a CDN redirects by region) - NEVER for a request that carries an API key, so a key is
    /// never forwarded to a redirect target.
    fn http_get(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        timeout: Duration,
        follow_redirects: bool,
    ) -> Result<(u16, String), String>;
}

/// The production environment: a real `std::process::Command` (host markers scrubbed from the
/// child, exactly as the engine launchers scrub them) and a real ureq GET (`https` only, no
/// redirects).
pub struct SystemEnv;

impl Env for SystemEnv {
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
        timeout: Duration,
        follow_redirects: bool,
    ) -> Result<(u16, String), String> {
        if !url.starts_with("https://") {
            return Err("only https is allowed".to_string());
        }
        // Redirects are followed by hand, never by ureq: every hop must stay on https and on the
        // site the request started from (`code.kimi.ai` -> `cdn.kimi.ai`), so a redirect can
        // neither downgrade the transport nor hand a download - the kimi installer script is run
        // after it is fetched - to another site.
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(timeout)
            .timeout(timeout)
            .redirects(0)
            .build();
        let ua = format!("c3/{}", env!("CARGO_PKG_VERSION"));
        let mut current = url.to_string();
        let mut hops = 0;
        loop {
            let mut req = agent.get(&current).set("User-Agent", &ua);
            for (k, v) in headers {
                req = req.set(k, v);
            }
            match req.call() {
                Ok(resp) => {
                    let status = resp.status();
                    if follow_redirects && matches!(status, 301 | 302 | 303 | 307 | 308) {
                        hops += 1;
                        if hops > MAX_REDIRECTS {
                            return Err("too many redirects".to_string());
                        }
                        current = redirect_target(&current, resp.header("location").unwrap_or(""))?;
                        continue;
                    }
                    // Read the body as bytes and lossily decode: the kimi `latest` marker answers
                    // with no text content-type, which `into_string()` can choke on. A body that
                    // reaches the cap is an ERROR, never a truncated prefix: the kimi installer is
                    // run after it is fetched, so a cut-off script must never be written or executed.
                    let buf = read_capped(resp.into_reader(), HTTP_BODY_CAP)?;
                    return Ok((status, String::from_utf8_lossy(&buf).into_owned()));
                }
                Err(ureq::Error::Status(code, _)) => return Ok((code, String::new())),
                Err(e) => return Err(c3_core::one_line(&e.to_string())),
            }
        }
    }
}

/// How many redirects a keyless GET follows.
const MAX_REDIRECTS: u32 = 3;

/// The largest HTTP body a maintenance GET accepts. A body that reaches this cap is refused (a
/// truncated installer is never written or run); legitimate installers and model lists are well
/// under it.
const HTTP_BODY_CAP: usize = 8 * 1024 * 1024;

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

/// The URL a redirect from `from` to `location` leads to, or why it is refused: the target must be
/// https and on the same site as `from` (the same last two host labels, `code.kimi.ai` ->
/// `cdn.kimi.ai`). A relative `location` is resolved against `from`.
pub(crate) fn redirect_target(from: &str, location: &str) -> Result<String, String> {
    let location = location.trim();
    if location.is_empty() {
        return Err("a redirect without a location".to_string());
    }
    let base = url::Url::parse(from).map_err(|e| format!("bad url: {e}"))?;
    let next = base
        .join(location)
        .map_err(|e| format!("bad redirect location: {e}"))?;
    if next.scheme() != "https" {
        return Err(format!("refused a redirect to {}", next.scheme()));
    }
    let site = |u: &url::Url| -> String {
        let host = u.host_str().unwrap_or("").to_ascii_lowercase();
        let labels: Vec<&str> = host.rsplit('.').take(2).collect();
        labels.into_iter().rev().collect::<Vec<_>>().join(".")
    };
    if site(&base).is_empty() || site(&base) != site(&next) {
        return Err(format!(
            "refused a redirect to another site ({})",
            next.host_str().unwrap_or("?")
        ));
    }
    Ok(next.to_string())
}

#[cfg(test)]
mod tests {
    use super::{is_secret_env, read_capped, redirect_target};

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
        for n in ["PATH", "HOME", "TEMP", "HTTP_PROXY", "HTTPS_PROXY", "NO_PROXY", "KIMI_VERSION"] {
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

    #[test]
    fn a_redirect_stays_on_https_and_on_the_same_site() {
        assert_eq!(
            redirect_target(
                "https://code.kimi.ai/kimi-code/latest",
                "https://cdn.kimi.ai/kimi-code/latest"
            )
            .unwrap(),
            "https://cdn.kimi.ai/kimi-code/latest"
        );
        // a relative location is resolved against the request
        assert_eq!(
            redirect_target("https://code.kimi.ai/kimi-code/latest", "/v2/latest").unwrap(),
            "https://code.kimi.ai/v2/latest"
        );
        // a transport downgrade, another site and an empty location are refused
        assert!(redirect_target(
            "https://code.kimi.ai/kimi-code/install.ps1",
            "http://cdn.kimi.ai/kimi-code/install.ps1"
        )
        .is_err());
        assert!(redirect_target(
            "https://code.kimi.ai/kimi-code/install.ps1",
            "https://cdn.example.net/kimi-code/install.ps1"
        )
        .is_err());
        assert!(redirect_target("https://code.kimi.ai/kimi-code/latest", "  ").is_err());
    }
}
